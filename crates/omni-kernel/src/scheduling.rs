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

use crate::KernelResult;

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
}
