//! `x86_64` cooperative context switch — MB6 deliverable + MB8 trampoline.
//!
//! Provides three items used by [`crate::scheduling::RoundRobinScheduler`]:
//!
//! - `context_switch`: assembly routine that saves the current task's
//!   callee-saved registers + RSP to `*from_rsp`, then resumes the task
//!   whose kernel-stack pointer is `to_rsp`.
//! - `omni_task_entry_trampoline`: tiny stub (`sti; ret`) that wraps a
//!   freshly-spawned task's entry point so the task starts with `IF = 1`
//!   even when the first `context_switch` into it runs inside the LAPIC
//!   timer's Interrupt Gate (which masks `IF`). MB8.
//! - `setup_task_frame`: Rust helper that writes the initial stack frame
//!   for a freshly-spawned kernel task so that the first `context_switch`
//!   into it lands at the trampoline, which immediately enables interrupts
//!   and then jumps to the real entry function.
//!
//! ## Stack layout after `context_switch` saves a task
//!
//! ```text
//! higher address (stack top)
//!   ┌────────────────────────┐
//!   │  RIP (return address)  │  ← pushed implicitly by `call context_switch`
//!   │  RBP                   │  ← pushed by the stub
//!   │  RBX                   │
//!   │  R12                   │
//!   │  R13                   │
//!   │  R14                   │
//!   │  R15                   │  ← RSP saved here in TCB
//!   └────────────────────────┘
//! lower address (stack grows ↓)
//! ```
//!
//! ## Stack layout produced by `setup_task_frame` (MB8)
//!
//! ```text
//! higher address (stack top)
//!   ┌────────────────────────────────┐
//!   │  entry (real fn() -> !)        │  ← popped by trampoline's `ret`
//!   │  omni_task_entry_trampoline    │  ← popped by context_switch's `ret`
//!   │  RBP = 0                       │
//!   │  RBX = 0                       │
//!   │  R12 = 0                       │
//!   │  R13 = 0                       │
//!   │  R14 = 0                       │
//!   │  R15 = 0                       │  ← initial RSP saved in TCB
//!   └────────────────────────────────┘
//! ```
//!
//! The trampoline is the indirection that solves the "first switch from
//! inside an Interrupt Gate leaves IF=0" problem: an Interrupt Gate clears
//! IF on entry; if the very first scheduler entry into a brand-new task is
//! triggered by the timer IRQ, the task would otherwise run with interrupts
//! permanently disabled (no further preemption possible).

#![allow(unsafe_code)]

// Only meaningful on x86_64 — all items are gated accordingly.
#[cfg(target_arch = "x86_64")]
use core::arch::global_asm;

// ---------------------------------------------------------------------------
// Assembly stub
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    /// Save the current task's callee-saved registers to `*from_rsp`, then
    /// resume the task whose kernel stack pointer is `to_rsp`.
    ///
    /// Calling convention (System V AMD64):
    /// - `from_rsp` (RDI): address of the `rsp` field in the current TCB.
    /// - `to_rsp`   (RSI): value to load into RSP for the next task.
    ///
    /// On entry the CPU has already pushed the return address via `call`.
    /// The stub pushes RBP, RBX, R12–R15 (6 × 8 = 48 bytes), stores RSP,
    /// then loads `to_rsp`, pops the registers in reverse order, and `ret`s
    /// into the next task at its saved RIP.
    ///
    /// # Safety
    ///
    /// - `from_rsp` must point to a valid `u64` in the current task's TCB.
    /// - `to_rsp` must be an RSP value saved by a previous `context_switch`
    ///   call OR the initial RSP set up by [`setup_task_frame`].
    /// - Must be called with interrupts disabled (IF = 0).
    /// - Must be called on a single CPU (no SMP in MB6 scope).
    #[link_name = "omni_context_switch"]
    pub fn context_switch(from_rsp: *mut u64, to_rsp: u64);
}

#[cfg(target_arch = "x86_64")]
global_asm!(
    ".global omni_context_switch",
    "omni_context_switch:",
    // Save callee-saved registers (System V AMD64 §3.2.1).
    // RIP is already on the stack (pushed by the `call` instruction).
    "    push rbp",
    "    push rbx",
    "    push r12",
    "    push r13",
    "    push r14",
    "    push r15",
    // Store current RSP in *from_rsp (RDI = first argument).
    "    mov [rdi], rsp",
    // Load next task's RSP (RSI = second argument).
    "    mov rsp, rsi",
    // Restore next task's callee-saved registers.
    "    pop r15",
    "    pop r14",
    "    pop r13",
    "    pop r12",
    "    pop rbx",
    "    pop rbp",
    // Return to the next task's saved RIP.
    "    ret",
);

// ---------------------------------------------------------------------------
// Task entry trampoline (MB8)
// ---------------------------------------------------------------------------
//
// A freshly-spawned task whose first `context_switch` lands inside an
// Interrupt Gate handler (e.g. the LAPIC timer) would otherwise start with
// `IF = 0` and never be preempted again. The trampoline reopens the IF
// gate before transferring control to the real entry function.
//
// Calling convention: invoked via `ret` from `omni_context_switch`. The
// callee-saved registers and the real entry RIP are already on the stack
// in the layout produced by `setup_task_frame`. `sti` enables interrupts;
// `ret` then pops the real `entry` and jumps to it.
#[cfg(target_arch = "x86_64")]
global_asm!(
    ".global omni_task_entry_trampoline",
    "omni_task_entry_trampoline:",
    "    sti",
    "    ret",
);

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    /// Address-only handle to the asm trampoline; we never `call` it from
    /// Rust — `setup_task_frame` only takes its address and pushes it onto
    /// the new task's stack so `context_switch`'s `ret` lands inside it.
    #[link_name = "omni_task_entry_trampoline"]
    fn omni_task_entry_trampoline();
}

// ---------------------------------------------------------------------------
// Initial stack frame helper
// ---------------------------------------------------------------------------

/// Write an initial stack frame so that the first [`context_switch`] into a
/// newly-spawned task lands in the `omni_task_entry_trampoline` (which
/// enables interrupts) and then in the real `entry` function.
///
/// The frame writes 8 words downward from `stack_top`:
///
/// 1. `entry` — popped by the trampoline's `ret`, becomes the task's first RIP.
/// 2. `omni_task_entry_trampoline` address — popped by `context_switch`'s `ret`.
/// 3. Six zero-initialised callee-saved registers (RBP, RBX, R12, R13, R14, R15)
///    in the order `context_switch` pops them (words 3 through 8 of the frame).
///
/// Returns the initial RSP value to store in the task's [`CpuContext`].
///
/// # Safety
///
/// `stack_top` must be the virtual address of the top (highest address) of a
/// valid, writable, exclusively-owned kernel stack of at least 64 bytes
/// (8 × 8 = the frame written here). In practice the caller allocates a full
/// 4 KiB frame so this is always satisfied.
#[cfg(target_arch = "x86_64")]
pub unsafe fn setup_task_frame(stack_top: u64, entry: u64) -> u64 {
    let trampoline_addr = omni_task_entry_trampoline as usize as u64;
    let mut sp = stack_top;
    unsafe {
        sp -= 8;
        // Popped by the trampoline's `ret`: becomes the task's real RIP.
        (sp as *mut u64).write(entry);
        sp -= 8;
        // Popped by `context_switch`'s `ret`: enters the trampoline.
        (sp as *mut u64).write(trampoline_addr);
        sp -= 8;
        (sp as *mut u64).write(0); // RBP = 0
        sp -= 8;
        (sp as *mut u64).write(0); // RBX = 0
        sp -= 8;
        (sp as *mut u64).write(0); // R12 = 0
        sp -= 8;
        (sp as *mut u64).write(0); // R13 = 0
        sp -= 8;
        (sp as *mut u64).write(0); // R14 = 0
        sp -= 8;
        (sp as *mut u64).write(0); // R15 = 0
    }
    sp
}
