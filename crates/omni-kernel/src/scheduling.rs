//! Process and thread scheduling.
//!
//! ## Status
//!
//! P6.4 scaffold. Trait surface for the [`Scheduler`] is defined; the
//! v1 reference implementation (a multi-level feedback queue with
//! thermal awareness and an AI-workload class) lands once IPC is
//! operational.
//!
//! ## Design rationale
//!
//! - **One scheduler trait, multiple policies.** The kernel ships a
//!   default scheduler; alternate policies are an OIP-ratified extension
//!   point.
//! - **AI workloads are a distinct class.** AI inference produces bursty,
//!   memory-bandwidth-bound load that interacts poorly with classic
//!   round-robin. The scheduler is aware of an `AI_PRIORITY_CLASS` and
//!   factors NPU/GPU availability into dispatch.
//! - **Thermal awareness.** On hardware that exposes temperature
//!   counters, the scheduler can throttle a class without throttling
//!   the whole CPU.

// The scheduler interfaces directly with `omni_context_switch` (assembly)
// and reads/writes `TaskControlBlock::context.rsp` via raw pointers; the
// kernel-task spawn path also relies on raw pointer arithmetic over the
// bootloader direct-map. Each `unsafe` block carries a `// SAFETY:` comment.
#![allow(unsafe_code)]

use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;

use crate::{KernelError, KernelResult};

// -----------------------------------------------------------------------------
// MB8 preemption signaling — atomic flags used by the LAPIC timer path
// -----------------------------------------------------------------------------

/// Set to `true` by the LAPIC timer tick when the current task's quantum has
/// expired. Consumed by `kernel_check_need_resched` (bare_metal::lapic) at the
/// safe tail of the timer interrupt, just before `iretq`.
///
/// Decoupling the tick (sets the flag, returns immediately) from the actual
/// `yield_current` call (runs at the very end of the IRQ stub, with the trap
/// frame still in place) keeps the interrupt path lock-free and isolates the
/// scheduler from re-entrancy by future interrupt sources (keyboard, disk,
/// NIC) that may set the same flag without knowing how a context switch
/// works.
pub static NEED_RESCHED: AtomicBool = AtomicBool::new(false);

/// Set to `true` for the duration of any call into `RoundRobinScheduler::yield_current`
/// (or `preempt`). The timer interrupt's `check_need_resched` consults this to
/// avoid recursing into the scheduler if the previous yield (e.g. from a
/// cooperative `TaskYield` syscall) is still on the stack.
pub static IN_SCHEDULER: AtomicBool = AtomicBool::new(false);

// -----------------------------------------------------------------------------
// Task identifier
// -----------------------------------------------------------------------------

/// Kernel-side task identifier. Opaque to userspace.
///
/// Distinct from `omni_types::AgentId` (an agent is a higher-level
/// concept that may map to multiple tasks). The kernel does not know
/// about agents; userspace bridges the two via the runtime service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub u64);

/// Priority class for a task.
///
/// The enum is `#[repr(u8)]` so it can be stored compactly in the task
/// control block.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PriorityClass {
    /// System (kernel-internal services).
    System = 0,
    /// Real-time (e.g., audio, video).
    RealTime = 1,
    /// Interactive (foreground user processes).
    Interactive = 2,
    /// AI inference (bursty, memory-bandwidth-bound).
    AiInference = 3,
    /// Background (batch work, indexing).
    Background = 4,
    /// Idle.
    Idle = 5,
}

// -----------------------------------------------------------------------------
// Scheduler state
// -----------------------------------------------------------------------------

/// State of a task as the scheduler sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskState {
    /// The task is on the run queue.
    Runnable,
    /// The task is executing on a CPU.
    Running,
    /// The task is blocked on IPC.
    BlockedOnIpc,
    /// The task is blocked on a syscall.
    BlockedOnSyscall,
    /// The task is sleeping until a deadline.
    Sleeping,
    /// The task has exited.
    Terminated,
}

// -----------------------------------------------------------------------------
// Scheduler trait
// -----------------------------------------------------------------------------

/// The scheduler trait.
///
/// The kernel holds exactly one `dyn Scheduler` per CPU. Cross-CPU work
/// stealing is implemented inside the chosen scheduler, not at the trait
/// boundary; this keeps the trait small and lets specific schedulers
/// pick the right policy.
pub trait Scheduler {
    /// Adds `task` to the scheduler's queues with the given priority.
    fn enqueue(&mut self, task: TaskId, priority: PriorityClass) -> KernelResult<()>;

    /// Removes `task` from the scheduler. Called when the task exits.
    fn dequeue(&mut self, task: TaskId) -> KernelResult<()>;

    /// Selects the next task to run on this CPU. Returns `None` if no
    /// task is runnable (in which case the caller can idle the CPU).
    fn pick_next(&mut self) -> Option<TaskId>;

    /// Records that the currently-running task is yielding the CPU
    /// voluntarily (e.g., it blocked on IPC).
    fn yield_current(&mut self, current: TaskId, new_state: TaskState) -> KernelResult<()>;

    /// Records that the currently-running task has exhausted its time
    /// slice; the scheduler may rotate it to the back of its priority
    /// queue.
    fn preempt(&mut self, current: TaskId) -> KernelResult<()>;
}

// -----------------------------------------------------------------------------
// CpuContext & TaskControlBlock — MB6
// -----------------------------------------------------------------------------

/// Saved CPU context for a kernel task.
///
/// Only RSP is stored here; the full callee-saved register set is pushed onto
/// the task's kernel stack by `context_switch` before saving RSP.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuContext {
    /// Saved kernel stack pointer. Points to the top of the callee-saved
    /// frame that `context_switch` left on the stack.
    pub rsp: u64,
}

/// Per-task kernel state (Task Control Block).
#[derive(Debug)]
pub struct TaskControlBlock {
    /// Kernel-side identifier.
    pub id: TaskId,
    /// Current scheduling state.
    pub state: TaskState,
    /// Priority class assigned at creation.
    pub priority: PriorityClass,
    /// Saved CPU context (kernel-stack RSP + callee-saved regs on that stack).
    pub context: CpuContext,
    /// Physical base address of this task's kernel stack (4 KiB frame).
    /// Retained for deallocation in `TaskExit` (MB7+).
    pub kernel_stack_phys: u64,
    /// Virtual base address of the task's kernel stack (MB10): the *bottom*
    /// of the writable 4 KiB page (its *top* is `kernel_stack_va +
    /// KERNEL_STACK_SIZE`). `0` is the sentinel used by
    /// `spawn_bootstrap_task` — the bootstrap kmain task re-uses the boot
    /// stack and has no entry in the isolated VA range.
    pub kernel_stack_va: u64,
}

// -----------------------------------------------------------------------------
// MB10 — Kernel stack isolation constants
// -----------------------------------------------------------------------------

/// Base of the kernel-only VA range that holds kernel-task stacks.
/// `0xFFFF_C000_0000_0000` is half-canonical kernel-half on x86_64 long mode;
/// disjoint from the bootloader's direct-map (`0xFFFF_8800_…` on
/// `bootloader 0.11`) and from the future user-space range planned for MB11
/// (`0x0000_0040_…`). See `docs/adr/0002-mb10-kernel-stack-isolation.md`.
pub const KERNEL_STACK_VA_BASE: u64 = 0xFFFF_C000_0000_0000;

/// Exclusive upper bound of the kernel-stack VA range — 8 TiB of address
/// space (`~1 G slots`), ample for Phase 1.
pub const KERNEL_STACK_VA_END: u64 = 0xFFFF_C800_0000_0000;

/// Writable kernel-stack size per task, in bytes (4 KiB single frame).
pub const KERNEL_STACK_SIZE: u64 = 0x1000;

/// Address-space stride per slot — 4 KiB guard page (not mapped) + 4 KiB
/// stack page (mapped). Walking the range by `KERNEL_STACK_STRIDE` gives
/// the *guard* VA of each slot; adding `KERNEL_STACK_SIZE` (= guard size)
/// gives the writable stack base.
pub const KERNEL_STACK_STRIDE: u64 = 0x2000;

// -----------------------------------------------------------------------------
// RoundRobinScheduler — MB6 concrete implementation
// -----------------------------------------------------------------------------

const NUM_PRIORITY_CLASSES: usize = 6;

/// Cooperative, single-CPU, round-robin scheduler.
///
/// One run queue per [`PriorityClass`]. `pick_next` scans from `System` (0)
/// to `Idle` (5) and takes the head of the first non-empty queue. No
/// preemption in MB6 — the LAPIC timer and `preempt` path land in MB7.
pub struct RoundRobinScheduler {
    /// All task control blocks (searched by `TaskId`).
    tasks: Vec<TaskControlBlock>,
    /// Per-priority run queues storing `TaskId.0` values.
    run_queues: [Vec<u64>; NUM_PRIORITY_CLASSES],
    /// `TaskId.0` of the task currently on-CPU (`None` until first switch).
    current: Option<u64>,
    /// Monotonically increasing counter for fresh `TaskId` allocation.
    /// Read only in cfg-gated code (bare-metal + test); suppress the lint.
    #[allow(dead_code)]
    next_id: u64,
    /// MB10 — index of the next kernel-stack slot to hand out from the
    /// isolated VA range `[KERNEL_STACK_VA_BASE, KERNEL_STACK_VA_END)`.
    /// Pure bump allocator: slots are never reused (task-exit dealloc
    /// arrives with the process model in MB11+).
    ///
    /// Read only by `allocate_stack_slot`, which itself is cfg-gated for
    /// the bare-metal x86_64 spawn path and the host-side unit tests.
    /// Suppress dead-code lint on non-x86_64 host clippy runs.
    #[allow(dead_code)]
    next_kernel_stack_slot: usize,
}

impl RoundRobinScheduler {
    /// Create an empty scheduler.
    ///
    /// `const fn` so it can initialise a `static mut` without a lazy wrapper.
    pub const fn new() -> Self {
        Self {
            tasks: Vec::new(),
            run_queues: [
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
            current: None,
            next_id: 1,
            next_kernel_stack_slot: 0,
        }
    }

    /// MB10 — returns the next kernel-stack VA slot, advancing the bump
    /// allocator. Returns `None` if the isolated range is exhausted
    /// (would require ~1 G task spawns).
    ///
    /// Called only by `spawn_kernel_task` (bare-metal x86_64) and by the
    /// host-side unit tests; suppress dead-code lint on non-x86_64
    /// `cargo clippy --workspace` runs.
    #[allow(dead_code)]
    fn allocate_stack_slot(&mut self) -> Option<u64> {
        let slot = self.next_kernel_stack_slot as u64;
        let slot_base = KERNEL_STACK_VA_BASE.checked_add(slot.checked_mul(KERNEL_STACK_STRIDE)?)?;
        // Guard page sits at slot_base; stack page sits at slot_base +
        // KERNEL_STACK_SIZE; stack top (the value used for the initial RSP)
        // is slot_base + KERNEL_STACK_STRIDE — must remain inside the range.
        let stack_top = slot_base.checked_add(KERNEL_STACK_STRIDE)?;
        if stack_top > KERNEL_STACK_VA_END {
            return None;
        }
        self.next_kernel_stack_slot = self.next_kernel_stack_slot.checked_add(1)?;
        // Return the writable stack base (i.e. the guard page is right *below* it).
        Some(slot_base + KERNEL_STACK_SIZE)
    }

    /// Return the `TaskId` of the task currently running on this CPU.
    pub fn current_task_id(&self) -> Option<TaskId> {
        self.current.map(TaskId)
    }

    /// Spawn a kernel-mode task from a function pointer and a pre-allocated
    /// physical kernel stack frame, mapping the frame into the isolated
    /// kernel-stack VA range introduced in MB10 (ADR-0002).
    ///
    /// Layout per slot (one slot = `KERNEL_STACK_STRIDE` bytes of VA):
    ///
    /// ```text
    ///   slot_base                          ┐ 4 KiB guard page — NOT mapped
    ///   slot_base + KERNEL_STACK_SIZE      ┤ stack bottom — PRESENT|WRITABLE|NX
    ///   slot_base + KERNEL_STACK_STRIDE    ┘ stack top (initial RSP target)
    /// ```
    ///
    /// Stack overflow past `KERNEL_STACK_SIZE` bytes triggers `#PF` on the
    /// guard page with `CR2` = guard VA — caught by the IDT handler from
    /// MB3.
    ///
    /// `kernel_stack_phys` is the physical base of a 4 KiB frame already
    /// allocated from `alloc` by the caller (so `spawn_kernel_task` itself
    /// can stay infallible w.r.t. physical exhaustion: the caller already
    /// surfaced that case). `mapper` and `alloc` are required by the inner
    /// `PageMapper::map_4k` call.
    ///
    /// # Safety
    ///
    /// `kernel_stack_phys` must be the base of a valid, exclusively owned
    /// 4 KiB frame. `mapper` and `alloc` must point at the kernel's active
    /// `PageMapper` / `BitmapFrameAllocator` — calling this from any
    /// non-kernel context corrupts the bootloader page tables.
    #[cfg(all(feature = "bare-metal", target_arch = "x86_64"))]
    pub unsafe fn spawn_kernel_task<const N: usize>(
        &mut self,
        entry: fn() -> !,
        kernel_stack_phys: u64,
        mapper: &mut crate::bare_metal::paging::PageMapper,
        alloc: &mut crate::memory::BitmapFrameAllocator<N>,
        priority: PriorityClass,
    ) -> KernelResult<TaskId> {
        use crate::bare_metal::context_switch::setup_task_frame;
        use crate::bare_metal::paging::{PTE_NO_EXEC, PTE_PRESENT, PTE_WRITABLE};
        use crate::memory::{PhysAddr, VirtAddr};

        let kernel_stack_va = self
            .allocate_stack_slot()
            .ok_or(KernelError::ResourceExhausted)?;

        // Map the writable stack page; deliberately leave the guard page
        // (kernel_stack_va - KERNEL_STACK_SIZE) un-mapped.
        if !mapper.map_4k(
            VirtAddr(kernel_stack_va),
            PhysAddr(kernel_stack_phys),
            PTE_PRESENT | PTE_WRITABLE | PTE_NO_EXEC,
            alloc,
        ) {
            return Err(KernelError::ResourceExhausted);
        }

        // Stack grows downward; initial RSP = top of the writable page.
        let stack_virt_top = kernel_stack_va + KERNEL_STACK_SIZE;
        // SAFETY: stack_virt_top is the top of a 4 KiB writable kernel
        // stack page that we just mapped exclusively for this task.
        let initial_rsp = unsafe { setup_task_frame(stack_virt_top, entry as u64) };

        let id = TaskId(self.next_id);
        self.next_id += 1;

        self.tasks.push(TaskControlBlock {
            id,
            state: TaskState::Runnable,
            priority,
            context: CpuContext { rsp: initial_rsp },
            kernel_stack_phys,
            kernel_stack_va,
        });
        self.run_queues[priority as usize].push(id.0);
        Ok(id)
    }

    /// Register the currently-executing kernel flow (i.e. `kmain` itself)
    /// as a scheduler-visible task without allocating a fresh kernel stack.
    ///
    /// Required by MB8 preemption: the LAPIC timer's `yield_current` needs a
    /// `current` task to save state into. Before `sti` we don't yet have one
    /// because `kmain` is running on the boot stack, outside the scheduler.
    /// This method creates a placeholder TCB whose `context.rsp` is `0`
    /// (sentinel — meaningful only until the first preemption overwrites it)
    /// and whose `kernel_stack_phys` is `0` (no owned frame: re-uses the boot
    /// stack), then installs it as `current`.
    ///
    /// The very first timer tick after `sti` will trigger `omni_context_switch`,
    /// which pushes kmain's callee-saved registers onto the boot stack and
    /// stores the real RSP in this TCB. From that moment kmain is a regular
    /// preemptible task.
    pub fn spawn_bootstrap_task(&mut self, priority: PriorityClass) -> KernelResult<TaskId> {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        self.tasks.push(TaskControlBlock {
            id,
            state: TaskState::Running,
            priority,
            // Sentinel: overwritten by the first `context_switch` save.
            context: CpuContext { rsp: 0 },
            // No owned stack frame; the boot stack is used in-place.
            kernel_stack_phys: 0,
            // Sentinel: bootstrap kmain task lives on the boot stack, not in
            // the MB10 isolated VA range.
            kernel_stack_va: 0,
        });
        // CRUCIAL: timer's `kernel_check_need_resched` would early-return
        // without this; the placeholder must be the current task already.
        self.current = Some(id.0);
        Ok(id)
    }

    /// Test helper: create a minimal TCB with a zeroed context and enqueue it.
    #[cfg(test)]
    pub fn mock_enqueue(&mut self, priority: PriorityClass) -> TaskId {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        self.tasks.push(TaskControlBlock {
            id,
            state: TaskState::Runnable,
            priority,
            context: CpuContext::default(),
            kernel_stack_phys: 0,
            kernel_stack_va: 0,
        });
        self.run_queues[priority as usize].push(id.0);
        id
    }
}

impl Scheduler for RoundRobinScheduler {
    fn enqueue(&mut self, task: TaskId, priority: PriorityClass) -> KernelResult<()> {
        let tcb = self
            .tasks
            .iter_mut()
            .find(|t| t.id.0 == task.0)
            .ok_or(KernelError::InvalidArgument)?;
        tcb.state = TaskState::Runnable;
        tcb.priority = priority;
        self.run_queues[priority as usize].push(task.0);
        Ok(())
    }

    fn dequeue(&mut self, task: TaskId) -> KernelResult<()> {
        for queue in &mut self.run_queues {
            queue.retain(|&id| id != task.0);
        }
        if let Some(tcb) = self.tasks.iter_mut().find(|t| t.id.0 == task.0) {
            tcb.state = TaskState::Terminated;
        }
        if self.current == Some(task.0) {
            self.current = None;
        }
        Ok(())
    }

    fn pick_next(&mut self) -> Option<TaskId> {
        for queue in &mut self.run_queues {
            if !queue.is_empty() {
                return Some(TaskId(queue.remove(0)));
            }
        }
        None
    }

    fn yield_current(&mut self, current: TaskId, new_state: TaskState) -> KernelResult<()> {
        let cur_idx = self
            .tasks
            .iter()
            .position(|t| t.id.0 == current.0)
            .ok_or(KernelError::InvalidArgument)?;

        // Update state and re-queue if still runnable.
        let prio = self.tasks[cur_idx].priority as usize;
        self.tasks[cur_idx].state = new_state;
        if new_state == TaskState::Runnable {
            self.run_queues[prio].push(current.0);
        }

        let Some(next) = self.pick_next() else {
            return Ok(());
        };

        // Same task picked (only one runnable) — resume without switching.
        if next.0 == current.0 {
            self.tasks[cur_idx].state = TaskState::Running;
            self.current = Some(next.0);
            return Ok(());
        }

        let next_idx = self
            .tasks
            .iter()
            .position(|t| t.id.0 == next.0)
            .ok_or(KernelError::Internal)?;
        self.tasks[next_idx].state = TaskState::Running;
        self.current = Some(next.0);

        // SAFETY: single-CPU, non-preemptive; both stack frames are valid
        // kernel memory established by spawn_kernel_task or a prior switch.
        #[cfg(all(feature = "bare-metal", target_arch = "x86_64"))]
        unsafe {
            let to_rsp_val = self.tasks[next_idx].context.rsp;
            let from_rsp_ptr: *mut u64 = &mut self.tasks[cur_idx].context.rsp as *mut u64;
            crate::bare_metal::context_switch::context_switch(from_rsp_ptr, to_rsp_val);
        }
        let _ = next_idx; // suppress unused-variable on non-bare-metal builds

        Ok(())
    }

    fn preempt(&mut self, current: TaskId) -> KernelResult<()> {
        // Rotate current task to the back of its priority queue and switch to
        // the next runnable task. Called by the LAPIC timer handler (MB8+).
        self.yield_current(current, TaskState::Runnable)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_class_fits_in_one_byte() {
        assert_eq!(core::mem::size_of::<PriorityClass>(), 1);
    }

    #[test]
    fn task_id_round_trips() {
        let t = TaskId(0xDEAD_BEEFu64);
        assert_eq!(t.0, 0xDEAD_BEEFu64);
    }

    #[test]
    fn round_robin_yields_in_priority_order() {
        let mut sched = RoundRobinScheduler::new();
        let idle = sched.mock_enqueue(PriorityClass::Idle);
        let sys = sched.mock_enqueue(PriorityClass::System);
        // System (0) must be picked before Idle (5).
        assert_eq!(sched.pick_next(), Some(sys));
        assert_eq!(sched.pick_next(), Some(idle));
        assert_eq!(sched.pick_next(), None);
    }

    #[test]
    fn round_robin_within_same_priority() {
        let mut sched = RoundRobinScheduler::new();
        let t1 = sched.mock_enqueue(PriorityClass::Interactive);
        let t2 = sched.mock_enqueue(PriorityClass::Interactive);
        assert_eq!(sched.pick_next(), Some(t1));
        assert_eq!(sched.pick_next(), Some(t2));
        assert_eq!(sched.pick_next(), None);
    }

    #[test]
    fn yield_current_re_enqueues_runnable() {
        let mut sched = RoundRobinScheduler::new();
        let t1 = sched.mock_enqueue(PriorityClass::Interactive);
        let t2 = sched.mock_enqueue(PriorityClass::Interactive);
        sched.current = Some(t1.0);
        // Drain t1 from the queue (simulate it being picked and running).
        let _ = sched.pick_next(); // picks t1
        // Now t2 is still in queue. Yield t1 as Runnable — should switch to t2.
        sched.yield_current(t1, TaskState::Runnable).unwrap();
        // After yield, t2 should be current.
        assert_eq!(sched.current_task_id(), Some(t2));
    }

    #[test]
    fn dequeue_removes_from_run_queue() {
        let mut sched = RoundRobinScheduler::new();
        let t = sched.mock_enqueue(PriorityClass::Background);
        sched.dequeue(t).unwrap();
        assert_eq!(sched.pick_next(), None);
    }

    #[test]
    fn preempt_is_noop() {
        let mut sched = RoundRobinScheduler::new();
        let t = sched.mock_enqueue(PriorityClass::RealTime);
        assert!(sched.preempt(t).is_ok());
    }

    // -------------------------------------------------------------------------
    // MB10 — kernel-stack VA range invariants
    // -------------------------------------------------------------------------

    #[test]
    fn mb10_first_slot_starts_after_guard_page() {
        let mut sched = RoundRobinScheduler::new();
        let va = sched.allocate_stack_slot().expect("first slot");
        // Slot 0: guard at BASE, stack at BASE + KERNEL_STACK_SIZE.
        assert_eq!(va, KERNEL_STACK_VA_BASE + KERNEL_STACK_SIZE);
        // The guard page sits one stack-size BELOW the writable stack base.
        assert_eq!(va - KERNEL_STACK_SIZE, KERNEL_STACK_VA_BASE);
    }

    #[test]
    fn mb10_consecutive_slots_advance_by_stride() {
        let mut sched = RoundRobinScheduler::new();
        let va0 = sched.allocate_stack_slot().expect("slot 0");
        let va1 = sched.allocate_stack_slot().expect("slot 1");
        let va2 = sched.allocate_stack_slot().expect("slot 2");
        assert_eq!(va1 - va0, KERNEL_STACK_STRIDE);
        assert_eq!(va2 - va1, KERNEL_STACK_STRIDE);
        // All stay inside the dedicated range.
        for va in [va0, va1, va2] {
            assert!(va >= KERNEL_STACK_VA_BASE + KERNEL_STACK_SIZE);
            assert!(va + KERNEL_STACK_SIZE <= KERNEL_STACK_VA_END);
        }
    }

    #[test]
    fn mb10_guard_page_layout_is_below_each_stack() {
        let mut sched = RoundRobinScheduler::new();
        for expected_slot in 0u64..4 {
            let va = sched.allocate_stack_slot().expect("slot");
            let slot_base = KERNEL_STACK_VA_BASE + expected_slot * KERNEL_STACK_STRIDE;
            // Guard page is the 4 KiB ABOVE slot_base; stack page is the next 4 KiB.
            assert_eq!(va, slot_base + KERNEL_STACK_SIZE);
        }
    }

    #[test]
    fn mb10_constants_fit_their_arithmetic_invariants() {
        // 8 TiB total range (BASE = 0xFFFF_C000_…, END = 0xFFFF_C800_…).
        assert_eq!(KERNEL_STACK_VA_END - KERNEL_STACK_VA_BASE, 8u64 << 40);
        // Stride = 2 × stack page (guard + stack).
        assert_eq!(KERNEL_STACK_STRIDE, KERNEL_STACK_SIZE * 2);
        // Stack page = 4 KiB.
        assert_eq!(KERNEL_STACK_SIZE, 0x1000);
        // Range capacity ≈ 1 G slots — much more than Phase 1 needs.
        let slots = (KERNEL_STACK_VA_END - KERNEL_STACK_VA_BASE) / KERNEL_STACK_STRIDE;
        assert!(slots >= 1_000_000_000);
    }

    #[test]
    fn bootstrap_task_is_installed_as_current_with_sentinel_rsp() {
        let mut sched = RoundRobinScheduler::new();
        let id = sched.spawn_bootstrap_task(PriorityClass::System).unwrap();
        assert_eq!(sched.current_task_id(), Some(id));
        let tcb = sched
            .tasks
            .iter()
            .find(|t| t.id.0 == id.0)
            .expect("bootstrap TCB must be present");
        // Sentinel values — both filled by the first preemption.
        assert_eq!(tcb.context.rsp, 0);
        assert_eq!(tcb.kernel_stack_phys, 0);
        // Bootstrap task is *not* enqueued on a run queue: it is already
        // executing. The first `yield_current` will re-queue it as Runnable.
        assert!(sched.run_queues.iter().all(|q| q.is_empty()));
        assert_eq!(tcb.state, TaskState::Running);
    }
}
