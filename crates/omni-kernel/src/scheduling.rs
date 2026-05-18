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
}

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
        }
    }

    /// Return the `TaskId` of the task currently running on this CPU.
    pub fn current_task_id(&self) -> Option<TaskId> {
        self.current.map(TaskId)
    }

    /// Spawn a kernel-mode task from a function pointer and a pre-allocated
    /// kernel stack frame.
    ///
    /// `kernel_stack_phys` is the physical base of a 4 KiB frame from
    /// `BitmapFrameAllocator`. `phys_offset` is the bootloader direct-map
    /// window base used to compute the virtual address.
    ///
    /// # Safety
    ///
    /// `kernel_stack_phys` must be the base of a valid, exclusively owned
    /// 4 KiB kernel stack frame. `phys_offset` must match the bootloader's
    /// direct-map offset.
    #[cfg(all(feature = "bare-metal", target_arch = "x86_64"))]
    pub unsafe fn spawn_kernel_task(
        &mut self,
        entry: fn() -> !,
        kernel_stack_phys: u64,
        phys_offset: u64,
        priority: PriorityClass,
    ) -> KernelResult<TaskId> {
        use crate::bare_metal::context_switch::setup_task_frame;

        // Stack grows downward; virtual top of the 4 KiB frame = base + 4096.
        let stack_virt_top = kernel_stack_phys + phys_offset + 4096;
        // SAFETY: stack_virt_top is the top of a valid, writable kernel stack frame.
        let initial_rsp = unsafe { setup_task_frame(stack_virt_top, entry as u64) };

        let id = TaskId(self.next_id);
        self.next_id += 1;

        self.tasks.push(TaskControlBlock {
            id,
            state: TaskState::Runnable,
            priority,
            context: CpuContext { rsp: initial_rsp },
            kernel_stack_phys,
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
