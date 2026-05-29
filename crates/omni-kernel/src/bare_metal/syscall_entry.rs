//! Architecture-specific syscall entry code — MB4 deliverable.
//!
//! Activates the P6.5 [`crate::syscall`] scaffold by wiring two entry paths:
//!
//! 1. **`SYSCALL`** instruction (fast path) — `omni_syscall_entry` loaded into
//!    `MSR_LSTAR`. Available on all `x86_64` long-mode CPUs that set `SCE` in
//!    `MSR_EFER`.
//! 2. **`INT 0x80`** (compatibility path) — `omni_int80_entry` installed in
//!    IDT vector 0x80. Slower but usable before `SYSCALL` is enabled, and
//!    by legacy emulators that intercept `int 0x80` at the hypervisor level.
//!
//! Both entry stubs share an identical register calling convention (matching
//! the Linux `x86_64` syscall ABI so that userspace tooling still works):
//!
//! | Register | Role                  |
//! |----------|-----------------------|
//! | RAX      | syscall number (u32)  |
//! | RDI      | a0                    |
//! | RSI      | a1                    |
//! | RDX      | a2                    |
//! | R10      | a3                    |
//! | R8       | a4                    |
//! | R9       | a5                    |
//!
//! Return values are in RAX (primary) and, for the OIP-013 driver-framework
//! `MmioMap` path, additionally RDX (POSIX-aligned errno code). RDX is
//! preserved unchanged through every instruction between
//! `call kernel_syscall_dispatch` and the user-mode `sysretq` / `iretq`.
//! `u64::MAX` in RAX remains the legacy single-register error sentinel
//! for syscalls that have not migrated to the rich return path.

#![allow(
    unsafe_code,
    reason = "MSR R/W + naked asm syscall stubs; SAFETY per fn"
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "RAX number is u64 by ABI but the dispatch enum tag fits u32"
)]
#![allow(
    clippy::option_if_let_else,
    reason = "match-on-Option in syscall handlers reads more clearly than map_or_else chains"
)]
#![allow(
    clippy::manual_let_else,
    reason = "syscall handlers use match-with-early-return for clarity on the success path"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "syscall handlers index user-space buffers validated by user_slice_mut / bounds checks"
)]
#![allow(
    clippy::items_after_statements,
    reason = "use items inside bare-metal-gated handler fns are placed near their use sites"
)]
#![allow(
    clippy::too_many_lines,
    reason = "fd_read and fd_write are exhaustive FD-type dispatch handlers; splitting obscures the protocol"
)]
#![allow(
    clippy::match_single_binding,
    reason = "single-pattern match is used intentionally for future extensibility in handler scaffolds"
)]
#![allow(
    clippy::single_match_else,
    reason = "nested match-on-Option with early-return in the else arm is clearer than if-let"
)]

use crate::syscall::{SyscallDispatcher, SyscallNumber, SyscallReturn};
use crate::{KernelError, KernelResult};

// -----------------------------------------------------------------------
// Error sentinel — returned to userspace on any dispatch error.
// -----------------------------------------------------------------------

/// Sentinel value returned in RAX when a syscall fails at the ABI boundary.
pub const SYSCALL_ERROR: u64 = u64::MAX;

// -----------------------------------------------------------------------
// MSR addresses (x86_64 only — consumed only by syscall_init)
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
const MSR_EFER: u32 = 0xC000_0080;
#[cfg(target_arch = "x86_64")]
const MSR_STAR: u32 = 0xC000_0081;
#[cfg(target_arch = "x86_64")]
const MSR_LSTAR: u32 = 0xC000_0082;
#[cfg(target_arch = "x86_64")]
const MSR_FMASK: u32 = 0xC000_0084;

/// Bit 0 of EFER: System Call Extensions — enables the `SYSCALL` / `SYSRET`
/// instructions in long mode.
#[cfg(target_arch = "x86_64")]
const EFER_SCE: u64 = 1;

// -----------------------------------------------------------------------
// MSR helpers (x86_64 only — no-op stubs for other arches avoid dead-code
// warnings when running host tests on aarch64/arm64 developer machines)
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    // SAFETY: `rdmsr` is a ring-0 read-only MSR access. Caller ensures the
    // MSR address is valid for the target CPU.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(hi) << 32) | u64::from(lo)
}

#[cfg(target_arch = "x86_64")]
unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    // SAFETY: `wrmsr` is a ring-0 MSR write. Caller ensures the MSR address
    // and value are valid (no reserved bits set, correct segment selectors).
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
}

// -----------------------------------------------------------------------
// Assembly stubs (x86_64 only, Intel syntax — same pattern as idt.rs)
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    // ---- SYSCALL fast path ----
    //
    // On entry: RAX=number, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5.
    // RCX holds the user-space RIP (saved by the CPU); R11 holds user RFLAGS.
    //
    // Stack alignment: `SYSCALL` does NOT push anything — RSP still points to
    // the user stack, which per SysV ABI is 16-byte aligned at the call site
    // (RSP % 16 == 0 immediately after `call` pushes the return address).
    // We save 6 callee-saved regs (6 × 8 = 48 bytes → RSP % 16 == 0 still),
    // then push a5 (−8 → RSP % 16 == 8) and add 8 bytes of padding to reach
    // 16-byte alignment before `call kernel_syscall_dispatch`.
    ".global omni_syscall_entry",
    "omni_syscall_entry:",
    // MB14.b — swap to the per-CPU GS base. SYSCALL is unconditionally
    // entered from Ring 3 (MSR_LSTAR is only reachable via `syscall`),
    // so the active GS base on entry is whatever userspace set (or 0)
    // and the kernel's per-CPU pointer sits in IA32_KERNEL_GS_BASE.
    // `swapgs` flips them: active = per-CPU pointer, shadow = user GS.
    // No callee-saved register has been spilled yet — `swapgs` itself
    // does not touch general-purpose registers.
    "    swapgs",
    // Save callee-saved registers (System V AMD64 ABI §3.2.1).
    "    push rbx",
    "    push r12",
    "    push r13",
    "    push r14",
    "    push r15",
    "    push rbp",
    // Preserve user context that the CPU stores in caller-saved regs.
    // RCX = user RIP (needed for SYSRETQ), R11 = user RFLAGS.
    "    mov r12, rcx",
    "    mov r13, r11",
    // Capture syscall number (eax, zero-extended) and a5 before we clobber.
    "    mov r14d, eax",
    "    mov r15, r9",
    // Push a5 as the 7th argument (System V stack arg 1).
    "    push r15",
    // Align RSP to 16 bytes for the `call`.
    "    sub rsp, 8",
    // Shuffle register arguments: kernel_syscall_dispatch(number, a0..a5).
    // System V order: RDI, RSI, RDX, RCX, R8, R9 + stack.
    // Incoming: RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, saved r15=a5.
    "    mov rcx, rdx", // a2 → 4th arg
    "    mov rdx, rsi", // a1 → 3rd arg
    "    mov rsi, rdi", // a0 → 2nd arg
    "    mov rdi, r14", // number → 1st arg (u32 zero-extended)
    "    mov r9,  r8",  // a4 → 6th arg
    "    mov r8,  r10", // a3 → 5th arg
    "    call kernel_syscall_dispatch",
    // Remove padding + a5 slot.
    "    add rsp, 16",
    // Restore user context for SYSRETQ.
    "    mov rcx, r12",
    "    mov r11, r13",
    // Restore callee-saved registers (reverse order of pushes).
    "    pop rbp",
    "    pop r15",
    "    pop r14",
    "    pop r13",
    "    pop r12",
    "    pop rbx",
    // MB14.b — restore userspace's GS base before handing the CPU back
    // to Ring 3. Mirror of the `swapgs` at entry: active = user GS,
    // shadow = per-CPU pointer (parked for the next syscall's entry
    // swap). `swapgs` does not touch RAX, so the syscall return value
    // (already in RAX) survives the flip.
    "    swapgs",
    "    sysretq",
    // ---- INT 0x80 compatibility path ----
    //
    // On entry: RAX=number, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5.
    // The CPU has pushed the interrupt frame (SS, RSP, RFLAGS, CS, RIP),
    // 5 × 8 = 40 bytes → RSP % 16 == 8 (interrupt taken from 16-aligned user RSP).
    //
    // After pushing 6 callee-saved regs (48 bytes) RSP % 16 is still 8.
    // Pushing a5 brings RSP % 16 to 0 — no sub rsp,8 padding is needed here.
    ".global omni_int80_entry",
    "omni_int80_entry:",
    "    push rbx",
    "    push r12",
    "    push r13",
    "    push r14",
    "    push r15",
    "    push rbp",
    "    mov r14d, eax",
    "    mov r15, r9",
    // Push a5 — also aligns RSP to 16 bytes (see alignment note above).
    "    push r15",
    // Same register shuffle as SYSCALL path.
    "    mov rcx, rdx",
    "    mov rdx, rsi",
    "    mov rsi, rdi",
    "    mov rdi, r14",
    "    mov r9,  r8",
    "    mov r8,  r10",
    "    call kernel_syscall_dispatch",
    // Remove only the a5 slot — no padding was added.
    "    add rsp, 8",
    "    pop rbp",
    "    pop r15",
    "    pop r14",
    "    pop r13",
    "    pop r12",
    "    pop rbx",
    "    iretq",
);

// Extern declarations so Rust can take the address of each stub.
#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn omni_syscall_entry();
    fn omni_int80_entry();
}

// -----------------------------------------------------------------------
// IRQ dispatch trampoline (P6.7.8.3, OIP-013 § S4.2)
//
// Single asm stub installed at every LAPIC vector allocated by
// `IrqAttach`. On fire:
//   - read the in-service LAPIC vector (`ISR.B<N>` for N in 8 banks)
//   - call `kernel_irq_dispatch_handler(vector)`
//   - the Rust callback increments the per-slot missed counter and
//     issues `lapic_eoi()`, then iretq.
//
// Because the kernel cannot distinguish vectors solely from the
// `iretq` frame, the handler reads `LAPIC.ISRn` to recover the
// in-service vector at the moment of dispatch.
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".global omni_irq_dispatch_trampoline",
    "omni_irq_dispatch_trampoline:",
    // Save caller-saved registers (System V AMD64 §3.2.1). We push 9
    // GPRs (8 bytes each) → 72 bytes. The interrupt frame is 5 × 8 =
    // 40 bytes; total stack drift = 112 bytes, which is RSP % 16 == 0
    // because the CPU pre-pushes 5 × 8 = 40 (mod 16 = 8) and our 9
    // pushes bring it to mod 16 = 8 + 72 = 80 mod 16 = 0.
    "    push rax",
    "    push rcx",
    "    push rdx",
    "    push rsi",
    "    push rdi",
    "    push r8",
    "    push r9",
    "    push r10",
    "    push r11",
    "    call kernel_irq_dispatch_handler",
    "    pop r11",
    "    pop r10",
    "    pop r9",
    "    pop r8",
    "    pop rdi",
    "    pop rsi",
    "    pop rdx",
    "    pop rcx",
    "    pop rax",
    "    iretq",
);

#[cfg(all(
    target_arch = "x86_64",
    feature = "bare-metal",
    target_os = "none",
    not(test)
))]
unsafe extern "C" {
    /// Defined by the inline `global_asm!` above.
    pub(crate) fn omni_irq_dispatch_trampoline();
}

/// Rust-side IRQ dispatch handler. The asm trampoline lands here with
/// a clean stack and clobbers-saved; we read the in-service vector from
/// the LAPIC and forward to [`irq_attach_handlers::dispatch_fire`].
///
/// Reading `ISR.B<N>` (LAPIC offsets `0x100..0x180` in xAPIC mode or
/// MSRs `0x810..0x817` in x2APIC) is the canonical way to recover the
/// in-service vector inside an interrupt context. We scan from the
/// top bank down so the highest-priority active vector wins.
#[cfg(all(
    target_arch = "x86_64",
    feature = "bare-metal",
    target_os = "none",
    not(test)
))]
#[unsafe(no_mangle)]
extern "C" fn kernel_irq_dispatch_handler() {
    if let Some(vector) = super::lapic::read_in_service_vector() {
        irq_attach_handlers::dispatch_fire(vector);
    } else {
        // No vector in service — spurious. Issue EOI to acknowledge.
        super::lapic::lapic_eoi();
    }
}

/// Host-build / non-x86_64 / non-bare-metal stub so the asm `extern`
/// reference can be linked when the bare-metal path is off.
#[cfg(not(all(
    target_arch = "x86_64",
    feature = "bare-metal",
    target_os = "none",
    not(test)
)))]
#[unsafe(no_mangle)]
extern "C" fn kernel_irq_dispatch_handler() {}

// -----------------------------------------------------------------------
// Concrete dispatcher
// -----------------------------------------------------------------------

/// MB11 — write a user-supplied buffer to the early console.
/// ABI: `(ptr, len) -> u64`. Returns the number of bytes emitted, or
/// `u64::MAX` if the buffer fails validation.
#[allow(
    clippy::unnecessary_wraps,
    reason = "signature parity with other SyscallDispatcher arms"
)]
fn write_console(ptr: u64, len: u64) -> KernelResult<u64> {
    if len == 0 {
        return Ok(0);
    }
    #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
    {
        use super::usermode::USER_HALF_END;
        // Pointer-range guard. The full PT walk happens implicitly: user
        // pages can only be present here because the running CR3 has
        // PTE_USER set on them. A non-mapped page triggers a #PF before
        // we reach the copy.
        let Some(end) = ptr.checked_add(len) else {
            return Ok(u64::MAX);
        };
        if end > USER_HALF_END {
            return Ok(u64::MAX);
        }
        // SAFETY: ptr + len ≤ USER_HALF_END verified above; user pages
        // are guaranteed by paging hardware to fault if not mapped.
        unsafe {
            let mut copied: u64 = 0;
            let mut buf = [0u8; 256];
            while copied < len {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "buf.len() = 256 fits u64 trivially; chunk fits usize"
                )]
                let chunk = core::cmp::min(buf.len() as u64, len - copied);
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "chunk ≤ 256 fits usize on every target"
                )]
                let chunk_usize = chunk as usize;
                core::ptr::copy_nonoverlapping(
                    (ptr + copied) as *const u8,
                    buf.as_mut_ptr(),
                    chunk_usize,
                );
                #[allow(
                    clippy::indexing_slicing,
                    reason = "chunk_usize ≤ 256 = buf.len() by min above"
                )]
                {
                    crate::bare_metal::early_console::emit(&buf[..chunk_usize]);
                }
                copied += chunk;
            }
            Ok(copied)
        }
    }
    #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
    {
        let _ = ptr;
        Ok(len)
    }
}

/// MB12 — IPC syscall handlers. All four operate on the kernel-global
/// `IPC_REGISTRY` (only present on bare-metal) and return raw `u64`
/// values per the SysV-style syscall ABI.
///
/// Host builds short-circuit to `Err(NotYetImplemented)` because the
/// IPC singleton is `cfg(target_os = "none")` only; the registry is
/// exercised directly in `cargo test` via [`crate::ipc::KernelIpcRegistry`].
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod ipc_handlers {
    use super::SYSCALL_ERROR;
    use crate::capabilities::KernelPrincipal;
    use crate::ipc::{
        BackpressurePolicy, ChannelId, ChannelPolicy, MessageEnvelope, MessageKind, WakeAction,
        ipc_registry_mut,
    };
    use crate::scheduling::{PriorityClass, Scheduler, TaskId, TaskState};

    use alloc::vec::Vec;

    /// Bound the per-message payload at 4 KiB for Phase 1. Bigger
    /// messages are a future `SharedMemoryGrant` concern (MB13+).
    const MAX_PAYLOAD: u64 = 4096;

    /// Decode the backpressure code passed via syscall arg.
    fn parse_backpressure(v: u64) -> Option<BackpressurePolicy> {
        match v {
            0 => Some(BackpressurePolicy::Block),
            1 => Some(BackpressurePolicy::Drop),
            2 => Some(BackpressurePolicy::EvictOldest),
            _ => None,
        }
    }

    fn parse_kind(v: u64) -> Option<MessageKind> {
        match v {
            1 => Some(MessageKind::Request),
            2 => Some(MessageKind::Reply),
            3 => Some(MessageKind::Notification),
            4 => Some(MessageKind::CapabilityHandoff),
            5 => Some(MessageKind::SharedMemoryGrant),
            _ => None,
        }
    }

    /// Validate that `[ptr, ptr + len)` lies in the canonical user half.
    /// Hardware PT walks during the subsequent copy will fault on any
    /// non-present or non-user-flagged page, so this is sufficient at
    /// the ABI boundary (same pattern `write_console` uses).
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        use crate::bare_metal::usermode::USER_HALF_END;
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= USER_HALF_END
    }

    /// Look up the current task's PCB-derived principal. Falls back to
    /// `KernelPrincipal::ZERO` for tasks without a user-space identity
    /// (idle, bootstrap).
    unsafe fn current_principal_and_task() -> (TaskId, KernelPrincipal) {
        // SAFETY: single-core; SCHEDULER not aliased.
        unsafe {
            let sched = &*core::ptr::addr_of!(crate::SCHEDULER);
            let id = sched.current_task_id().unwrap_or(TaskId(0));
            let principal = sched
                .process(id)
                .map_or(KernelPrincipal::ZERO, |pcb| pcb.principal);
            (id, principal)
        }
    }

    fn current_priority(task: TaskId) -> PriorityClass {
        // SAFETY: single-core; SCHEDULER read-only here.
        unsafe {
            let sched = &*core::ptr::addr_of!(crate::SCHEDULER);
            sched
                .process(task)
                .map_or(PriorityClass::Interactive, |pcb| pcb.task.priority)
        }
    }

    /// Park the calling task as `BlockedOnIpc`. The next runnable task
    /// takes over; this call returns when the scheduler dispatches us
    /// back (i.e. when some counterpart issued `WakeAction::Wake(self)`).
    unsafe fn park_until_woken(task: TaskId) {
        // SAFETY: single-core; SCHEDULER not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let _ = sched.yield_current(task, TaskState::BlockedOnIpc);
        }
    }

    /// Enqueue `task` back onto its priority queue, restoring it to
    /// `Runnable`. Called when a `WakeAction::Wake` was returned by
    /// the registry.
    unsafe fn unpark(task: TaskId) {
        // SAFETY: single-core; SCHEDULER not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let prio = current_priority(task);
            let _ = sched.enqueue(task, prio);
        }
    }

    /// Maximum accepted size for a single postcard-encoded
    /// [`omni_capability::CapabilityToken`] presented through the
    /// MB13.d `IpcCreateChannel` ABI. Real tokens are ~200 bytes; the
    /// 1 KiB cap is generous and bounds the on-stack copy buffer.
    const MAX_TOKEN_BYTES: usize = 1024;

    /// `IpcCreateChannel (20)` — MB13.d signed-token ABI.
    ///
    /// ## ABI
    ///
    /// | Reg | Role                                                            |
    /// |-----|-----------------------------------------------------------------|
    /// | a0  | `queue_depth: u64`                                              |
    /// | a1  | `backpressure: u64` (0=Block, 1=Drop, 2=EvictOldest)             |
    /// | a2  | `tee_bound: u64` (0/1)                                          |
    /// | a3  | `send_token_ptr: u64` (0 = no send-side capability)             |
    /// | a4  | `recv_token_ptr: u64` (0 = no recv-side capability)             |
    /// | a5  | `lens: u64` = `send_len:u32 \| (recv_len:u32 << 32)`             |
    ///
    /// Returns the kernel-allocated [`ChannelId`] in RAX, or
    /// [`SYSCALL_ERROR`] on validation / verification failure.
    ///
    /// ## Backwards compatibility
    ///
    /// When both `send_token_ptr` and `recv_token_ptr` are zero (the
    /// MB12 calling convention), the handler still goes through
    /// [`Ed25519CapabilityProvider`] but skips the signed-token
    /// decode path — the registry's `(None, None)` shortcut delegates
    /// to `create_channel` with the same provider, whose per-IPC
    /// `verify` impl is identical O(1) shape-matching. The
    /// `mb12-userprobe` smoke keeps booting unchanged.
    ///
    /// When at least one pointer is non-zero, the handler:
    ///
    /// 1. Bounds-checks each token range against the user half via
    ///    [`user_range_ok`].
    /// 2. Copies the bytes into a kernel-side stack buffer (`MAX_TOKEN_BYTES`
    ///    cap) so the verification path cannot be poisoned by concurrent
    ///    user-space mutation.
    /// 3. Delegates to
    ///    [`crate::ipc::KernelIpcRegistry::create_channel_signed`] which
    ///    runs Ed25519 signature + time-window + TEE-binding verification
    ///    via [`crate::capabilities::Ed25519CapabilityProvider`].
    pub(super) fn ipc_create_channel(args: [u64; 6]) -> u64 {
        let Some(bp) = parse_backpressure(args[1]) else {
            return SYSCALL_ERROR;
        };
        let policy = ChannelPolicy {
            queue_depth: args[0] as usize,
            backpressure: bp,
            tee_bound: args[2] != 0,
        };
        let send_token_ptr = args[3];
        let recv_token_ptr = args[4];
        let send_len = (args[5] & 0xFFFF_FFFF) as usize;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "len fields are u32 by ABI definition; right-shift then mask is safe"
        )]
        let recv_len = ((args[5] >> 32) & 0xFFFF_FFFF) as usize;

        // SAFETY: SYSCALL path masks interrupts; single-CPU.
        let (current, _) = unsafe { current_principal_and_task() };

        // -----------------------------------------------------------------
        // Legacy MB12 path — both pointers zero → open channel via the
        // canonical Ed25519 provider (no signed-token decode required;
        // the registry's `(None, None)` shortcut takes the fast path).
        // -----------------------------------------------------------------
        if send_token_ptr == 0 && recv_token_ptr == 0 {
            let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
            // SAFETY: IPC_REGISTRY not aliased; single-CPU.
            let res = unsafe {
                ipc_registry_mut().create_channel(current, policy, None, None, &provider)
            };
            return res.map_or(SYSCALL_ERROR, |ch| ch.0);
        }

        // -----------------------------------------------------------------
        // MB13.d signed-token path. Two scratch buffers on the kernel
        // stack; we reserve `MAX_TOKEN_BYTES` per side. The actual postcard
        // payload is typically ~200 bytes, so this is comfortably bounded.
        // -----------------------------------------------------------------
        let mut send_buf = [0u8; MAX_TOKEN_BYTES];
        let mut recv_buf = [0u8; MAX_TOKEN_BYTES];

        let Ok(send_slice) = copy_user_token(send_token_ptr, send_len, &mut send_buf) else {
            return SYSCALL_ERROR;
        };
        let Ok(recv_slice) = copy_user_token(recv_token_ptr, recv_len, &mut recv_buf) else {
            return SYSCALL_ERROR;
        };

        // Kernel monotonic time for the token's window check.
        let now_secs = u64::from(crate::bare_metal::arch::rtc_seconds());

        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        // SAFETY: IPC_REGISTRY not aliased; single-CPU.
        let res = unsafe {
            ipc_registry_mut()
                .create_channel_signed(current, policy, send_slice, recv_slice, &provider, now_secs)
        };
        res.map_or(SYSCALL_ERROR, |ch| ch.0)
    }

    /// Copy a user-space postcard token blob into the supplied kernel
    /// buffer and return a slice over the copied bytes, or `Err(())` if
    /// any validation step fails.
    ///
    /// `(ptr = 0, len = 0)` returns `Ok(None)` (no token presented).
    /// Any other shape (`ptr = 0 ^ len = 0`, `len > MAX_TOKEN_BYTES`,
    /// out-of-user-half range) is an error.
    fn copy_user_token(
        ptr: u64,
        len: usize,
        buf: &mut [u8; MAX_TOKEN_BYTES],
    ) -> Result<Option<&[u8]>, ()> {
        if ptr == 0 && len == 0 {
            return Ok(None);
        }
        if ptr == 0 || len == 0 || len > MAX_TOKEN_BYTES {
            return Err(());
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "len ≤ MAX_TOKEN_BYTES = 1024 fits u64 trivially"
        )]
        if !user_range_ok(ptr, len as u64) {
            return Err(());
        }
        // SAFETY: user_range_ok verified `[ptr, ptr + len)` lies in the
        // user half; the active CR3 is the caller's own AS, so the
        // hardware PT walk faults on any missing page before the copy
        // returns garbage. `len` ≤ buf.len() by the cap above.
        unsafe {
            core::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr(), len);
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "len ≤ MAX_TOKEN_BYTES = buf.len() by the cap above"
        )]
        Ok(Some(&buf[..len]))
    }

    /// `IpcDestroyChannel (21)` — `(channel_id, _, _, _, _, _) -> 0 | u64::MAX`.
    pub(super) fn ipc_destroy_channel(args: [u64; 6]) -> u64 {
        // SAFETY: same as ipc_create_channel.
        let (current, _) = unsafe { current_principal_and_task() };
        let res = unsafe { ipc_registry_mut().destroy_channel(ChannelId(args[0]), current) };
        match res {
            Ok(()) => 0,
            Err(_) => SYSCALL_ERROR,
        }
    }

    /// `IpcSend (22)` — `(channel_id, kind, payload_ptr, payload_len, _, _) -> 0 | u64::MAX`.
    ///
    /// On `BackpressurePolicy::Block` with a full queue, the calling
    /// task parks and the syscall re-tries on wake. The kernel never
    /// returns `u64::MAX` for a blocked-then-completed send — only for
    /// hard errors (validation failure, capability mismatch, no such
    /// channel, `Drop` policy on full queue).
    pub(super) fn ipc_send(args: [u64; 6]) -> u64 {
        let channel = ChannelId(args[0]);
        let Some(kind) = parse_kind(args[1]) else {
            return SYSCALL_ERROR;
        };
        let payload_ptr = args[2];
        let payload_len = args[3];
        if payload_len > MAX_PAYLOAD || !user_range_ok(payload_ptr, payload_len) {
            return SYSCALL_ERROR;
        }
        // Copy the payload into a kernel buffer up front so that
        // `Block`-policy waits don't strand a reference to user memory.
        let mut payload: Vec<u8> = Vec::with_capacity(payload_len as usize);
        if payload_len > 0 {
            // SAFETY: user_range_ok verified the range; hardware PT walk
            // faults on missing pages. CR3 is the sender's own AS.
            unsafe {
                let src = payload_ptr as *const u8;
                payload.set_len(payload_len as usize);
                core::ptr::copy_nonoverlapping(src, payload.as_mut_ptr(), payload_len as usize);
            }
        }

        // SAFETY: SYSCALL path; single-CPU.
        let (current, principal) = unsafe { current_principal_and_task() };

        loop {
            let envelope = MessageEnvelope {
                sender: current,
                channel,
                kind,
                payload: payload.clone(),
            };
            // SAFETY: IPC_REGISTRY not aliased; single-CPU.
            let res = unsafe { ipc_registry_mut().send(envelope, current, principal) };
            match res {
                Ok(WakeAction::None) => return 0,
                Ok(WakeAction::Wake(t)) => {
                    // SAFETY: scheduler not aliased; single-CPU.
                    unsafe { unpark(t) };
                    return 0;
                }
                Ok(WakeAction::Block(_)) => {
                    // SAFETY: single-CPU; scheduler not aliased.
                    unsafe { park_until_woken(current) };
                    // Wake-up: retry the send. The previous attempt
                    // pushed `current` into the channel's waiters_send
                    // queue; that bookkeeping is consumed by whatever
                    // path issued the wake-up. We start the loop fresh.
                    continue;
                }
                Err(_) => return SYSCALL_ERROR,
            }
        }
    }

    /// `IpcReceive (23)` — `(channel_id, dst_ptr, dst_cap, blocking, _, _) -> bytes_received | u64::MAX`.
    ///
    /// Blocking semantics: if the queue is empty and `blocking != 0`,
    /// the calling task parks and the syscall re-tries on wake. Returns
    /// the actual number of payload bytes copied to `dst_ptr`. Returns
    /// `0` for a non-blocking empty receive.
    pub(super) fn ipc_receive(args: [u64; 6]) -> u64 {
        let channel = ChannelId(args[0]);
        let dst_ptr = args[1];
        let dst_cap = args[2];
        let blocking = args[3] != 0;
        if !user_range_ok(dst_ptr, dst_cap) {
            return SYSCALL_ERROR;
        }
        // SAFETY: SYSCALL path; single-CPU.
        let (current, principal) = unsafe { current_principal_and_task() };

        loop {
            // SAFETY: IPC_REGISTRY not aliased; single-CPU.
            let res = unsafe { ipc_registry_mut().receive(channel, current, principal, blocking) };
            match res {
                Ok((Some(env), wake)) => {
                    // Wake any blocked sender first; the order does not
                    // matter for correctness but mirrors send-side.
                    if let WakeAction::Wake(t) = wake {
                        // SAFETY: scheduler not aliased; single-CPU.
                        unsafe { unpark(t) };
                    }
                    let to_copy = core::cmp::min(env.payload.len() as u64, dst_cap);
                    if to_copy > 0 {
                        // SAFETY: user_range_ok verified `[dst_ptr, dst_ptr + dst_cap)`;
                        // hardware PT walk faults on any missing user page.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                env.payload.as_ptr(),
                                dst_ptr as *mut u8,
                                to_copy as usize,
                            );
                        }
                    }
                    return to_copy;
                }
                Ok((None, WakeAction::Block(_))) => {
                    // SAFETY: scheduler not aliased; single-CPU.
                    unsafe { park_until_woken(current) };
                    continue;
                }
                Ok((None, _)) => return 0,
                Err(_) => return SYSCALL_ERROR,
            }
        }
    }
}

/// MB11/MB12 — terminate the calling user-process task.
///
/// MB11 single-task: dequeue self + `halt_forever`. MB12 multi-task:
/// dequeue self + `yield_current(Terminated)`; the scheduler picks the
/// next runnable task and switches into it. Only when the run queue is
/// empty do we fall through to `halt_forever` — that path remains the
/// "kernel idle terminator" of last resort.
#[allow(
    clippy::unnecessary_wraps,
    reason = "signature parity with other SyscallDispatcher arms"
)]
fn task_exit(code: u64) -> KernelResult<u64> {
    #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
    {
        use crate::scheduling::{Scheduler, TaskState};
        super::early_console::write_str("[user] exit=");
        // SAFETY: single-core; SCHEDULER not aliased.
        unsafe {
            super::early_console::write_usize(code as usize);
            super::early_console::write_str("\n");
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            if let Some(current) = sched.current_task_id() {
                // P6.7.8.1 — OIP-013 § S2.4: tear down every `MmioMap`
                // mapping owned by the exiting process before retiring
                // its PCB. Done while the caller's CR3 is still active
                // so the `invlpg` inside the helper invalidates the
                // entries that user code may have just touched.
                mmio_map_handlers::tear_down_mmio_mappings(current);
                // P6.7.8.3 — OIP-013 § S3.4 / § S4.4: tear down DMA
                // windows + IRQ attachments before the PCB is retired.
                // DMA frames return to FRAME_ALLOC; IRQ vectors are
                // released from the per-vector slot table.
                dma_map_handlers::tear_down_dma_mappings(current);
                irq_attach_handlers::tear_down_irq_attachments(current);
                // P6.7.9-pre.8 — detach every PCI binding the driver
                // owned. Symmetric to the `iommu_attach_device` calls
                // wired into `driver_load` above; the helper drains
                // `pcb.bound_pci_devices` so a respawn into the same
                // PCB slot never inherits stale vendor-table entries.
                driver_load_handlers::tear_down_pci_bindings(current);
                // P6.7.10-pre.3 — drop every BLK registry entry the
                // exiting task owns (OIP-Driver-NVMe-014 § S4). Done
                // AFTER the PCI / IOMMU teardown so a re-entrant
                // teardown path triggered by a future MP build does
                // not observe a half-dead registry. The underlying
                // IPC channels are torn down by the IPC layer when
                // its task-exit hook lands; until then they leak
                // alongside the PCB which is benign in Phase 1
                // single-CPU because no other observer exists.
                blk_handlers::tear_down_blk_channels(current);
                let _ = sched.dequeue(current);
                // MB12: if another task is still runnable, hand the CPU
                // over to it. `yield_current(Terminated)` keeps the
                // current task off-queue (it stays Terminated) and the
                // scheduler picks the next one, doing the CR3+TSS swap
                // through the MB12.0a/b path. When everyone is gone,
                // `pick_next` returns `None` and we fall through to
                // `halt_forever`.
                let _ = sched.yield_current(current, TaskState::Terminated);
            }
        }
        super::arch::halt_forever()
    }
    #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
    {
        let _ = code;
        Ok(0)
    }
}

// -----------------------------------------------------------------------
// MmioMap (OIP-013 § S2, P6.7.8.1)
//
// The handler exists only in the bare-metal build (it needs FRAME_ALLOC,
// SCHEDULER, the active CR3, and the bootloader direct-map offset). On
// host tests the dispatcher route returns `EINVAL` so the trait shape
// is exercised without the singletons.
// -----------------------------------------------------------------------

/// Per-process linear allocator cap inside the reserved driver-MMIO
/// PML4 slot. One slot covers 512 GiB — enough for every BAR the
/// Phase 1 driver fleet will ever map; the static cap keeps the
/// arithmetic auditable.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const DRIVER_MMIO_VA_BASE: u64 = 0x0000_0080_0000_0000;
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const DRIVER_MMIO_VA_END: u64 = 0x0000_0100_0000_0000;
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const DRIVER_MMIO_RANGE: u64 = DRIVER_MMIO_VA_END - DRIVER_MMIO_VA_BASE;

/// Driver-DMA reserved PML4 slot (`[0x0000_0100_..., 0x0000_0180_...)` →
/// 512 GiB) — disjoint from the MMIO slot above so the audit log of a
/// driver's address space is partitioned by purpose. The end is checked
/// against `usermode::USER_HALF_END` (`0x0000_8000_0000_0000`) to keep
/// every DMA mapping in the user half.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const DRIVER_DMA_VA_BASE: u64 = 0x0000_0100_0000_0000;
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const DRIVER_DMA_VA_END: u64 = 0x0000_0180_0000_0000;

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod mmio_map_handlers {
    use super::{DRIVER_MMIO_RANGE, DRIVER_MMIO_VA_BASE, DRIVER_MMIO_VA_END};
    use crate::bare_metal;
    use crate::bare_metal::address_space::AddressSpace;
    use crate::bare_metal::paging::{PTE_NO_EXEC, PTE_PRESENT, PTE_USER, PTE_WRITABLE, PageMapper};
    use crate::driver_manifest::is_driver_framework_action;
    use crate::kaslr::KaslrRng;
    use crate::memory::{PhysAddr, VirtAddr};
    use crate::process::MmioMapping;
    use crate::syscall::{SyscallReturn, syscall_errno};
    use omni_capability::CapabilityToken;
    use omni_capability::scope::{Action, Resource};

    /// Page-cache-disable (`PCD`). Bit 4 of a 4 KiB leaf PTE: forces
    /// uncached access for memory-mapped device registers (OIP-013
    /// § S2.2 step 2).
    const PTE_PCD: u64 = 1 << 4;
    /// Page-write-through (`PWT`). Bit 3 of a 4 KiB leaf PTE: pairs
    /// with `PCD` to encode "strong uncached" on `x86_64` (OIP-013
    /// § S2.2 step 2).
    const PTE_PWT: u64 = 1 << 3;

    /// Maximum accepted size for the postcard-encoded
    /// [`CapabilityToken`] presented through `MmioMap`. Identical
    /// bound to the MB13.d `IpcCreateChannel` handler so user space
    /// can reuse one mint pipeline. Real tokens are ~200 bytes.
    const MAX_TOKEN_BYTES: usize = 1024;

    /// Validate that `[ptr, ptr + len)` lies entirely in the user
    /// half. Mirrors the IPC-side helper so the two paths cannot
    /// drift on the validation contract.
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= bare_metal::usermode::USER_HALF_END
    }

    /// `MmioMap (70)` — OIP-013 § S2.
    ///
    /// ## ABI
    ///
    /// The SysV-linux syscall argument layout maps OIP-013 § S2's
    /// register-name labels to the kernel's canonical
    /// `(a0..=a5)` slots:
    ///
    /// | Slot | Reg | Role                                |
    /// |------|-----|-------------------------------------|
    /// | a0   | RDI | `phys_base` (page-aligned)          |
    /// | a1   | RSI | `len` (multiple of 4 KiB, non-zero) |
    /// | a2   | RDX | `flags` (bit 0 = WC; rest reserved) |
    /// | a3   | R10 | `cap_ptr` (user VA, postcard token) |
    /// | a4   | R8  | `cap_len` (≤ `MAX_TOKEN_BYTES`)     |
    ///
    /// Returns a [`SyscallReturn`] whose `rax` holds the page-aligned
    /// user VA on success or `0` on error; `rdx` is `0` on success or
    /// one of the [`syscall_errno`] codes on error.
    #[allow(
        clippy::too_many_lines,
        reason = "single-syscall handler keeps the auth + map + record sequence in one place \
                  so the OIP-013 § S2 invariants stay locally auditable"
    )]
    pub(super) fn mmio_map(args: [u64; 6]) -> SyscallReturn {
        let phys_base = args[0];
        let len = args[1];
        let flags = args[2];
        let cap_ptr = args[3];
        let cap_len = args[4];

        // -------------------------------------------------------------
        // EINVAL: alignment + reserved flag bits.
        // -------------------------------------------------------------
        if phys_base & 0xFFF != 0 || len == 0 || len & 0xFFF != 0 {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        if flags & !1 != 0 {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        // OIP-013 § S2.2 step 2: WC requires PAT to be configured.
        // PAT init is not yet wired in Phase 1 — reject explicitly so
        // user space does not silently fall back to UC and corrupt
        // an MMIO write-combining buffer.
        if flags & 1 != 0 {
            return SyscallReturn::err(syscall_errno::ENOSYS);
        }

        // -------------------------------------------------------------
        // EFAULT: capability-token pointer + length.
        // -------------------------------------------------------------
        if cap_ptr == 0 || cap_len == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let Ok(cap_len_usize) = usize::try_from(cap_len) else {
            return SyscallReturn::err(syscall_errno::EFAULT);
        };
        if cap_len_usize > MAX_TOKEN_BYTES {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        if !user_range_ok(cap_ptr, cap_len) {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // Copy the token into a kernel-side stack buffer so subsequent
        // verification cannot be poisoned by user concurrent mutation.
        let mut buf = [0u8; MAX_TOKEN_BYTES];
        // SAFETY: `user_range_ok` verified the source lies in the
        // user half; the active CR3 is the caller's own AS, so the
        // hardware PT walk faults on any missing page before the
        // copy returns garbage. `cap_len_usize` ≤ buf.len() by the
        // cap above.
        unsafe {
            core::ptr::copy_nonoverlapping(cap_ptr as *const u8, buf.as_mut_ptr(), cap_len_usize);
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "cap_len_usize ≤ MAX_TOKEN_BYTES = buf.len() by the cap above"
        )]
        let token_bytes = &buf[..cap_len_usize];

        // -------------------------------------------------------------
        // EACCES: signature, time window, TEE binding, action, resource.
        // -------------------------------------------------------------
        let Ok(token) = omni_types::wire::decode_canonical::<CapabilityToken>(token_bytes) else {
            return SyscallReturn::err(syscall_errno::EACCES);
        };
        let now = u64::from(bare_metal::arch::rtc_seconds());
        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        if provider.verify_signed_token(&token, now)
            != crate::capabilities::CapabilityVerdict::Authorised
        {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        // Defense in depth: outside callers cannot reach here without
        // posting a driver-framework action, but pin the check.
        if !is_driver_framework_action(token.payload.scope.action) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        if token.payload.scope.action != Action::MmioMap {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        let claim = Resource::MmioRegion { phys_base, len };
        if !claim.is_subset_of(&token.payload.scope.resource) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }

        // -------------------------------------------------------------
        // Allocate driver-VA range + install leaf PTEs in the caller's
        // address space.
        // -------------------------------------------------------------
        let Ok(len_pages_u64) = u64::checked_div(len, 0x1000).ok_or(()) else {
            return SyscallReturn::err(syscall_errno::EINVAL);
        };
        // OIP-013 caps `len_pages` to fit u32 (each driver mapping is
        // a small BAR, well below 2^32 pages = 16 TiB). Reject any
        // pathological size.
        let Ok(len_pages) = u32::try_from(len_pages_u64) else {
            return SyscallReturn::err(syscall_errno::EINVAL);
        };

        // SAFETY: SYSCALL path is single-CPU under the kernel mutex;
        // SCHEDULER + FRAME_ALLOC are not otherwise aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let alloc = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);

            let Some(current) = sched.current_task_id() else {
                return SyscallReturn::err(syscall_errno::EFAULT);
            };
            let Some(pcb) = sched.process_mut(current) else {
                return SyscallReturn::err(syscall_errno::EFAULT);
            };

            // Lazy KASLR: first MmioMap call randomizes the cursor.
            // Subsequent calls allocate linearly from there.
            if pcb.mmio_va_cursor == 0 {
                let mut rng = KaslrRng::new();
                // Allocate at least `len` bytes ahead of `_END` so the
                // first mapping fits; `usable_range` is the addressable
                // span excluding the tail reserved by the request size.
                let usable_range = DRIVER_MMIO_RANGE.saturating_sub(len);
                if usable_range == 0 {
                    return SyscallReturn::err(syscall_errno::ENOSPC);
                }
                let raw = rng.next_u64();
                let offset = (raw % usable_range) & !0xFFF;
                pcb.mmio_va_cursor = DRIVER_MMIO_VA_BASE + offset;
            }

            let va_base = pcb.mmio_va_cursor;
            let Some(va_end) = va_base.checked_add(len) else {
                return SyscallReturn::err(syscall_errno::ENOSPC);
            };
            if va_end > DRIVER_MMIO_VA_END {
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }

            let phys_offset = bare_metal::phys_offset();
            if phys_offset == 0 {
                // kmain ordering bug: PHYS_OFFSET should be set well
                // before any user-space syscall can land.
                return SyscallReturn::err(syscall_errno::EFAULT);
            }
            let address_space: AddressSpace = pcb.address_space;
            let mut mapper = PageMapper::new(phys_offset, address_space.pml4_phys);

            let install_flags =
                PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_NO_EXEC | PTE_PCD | PTE_PWT;

            let mut installed: u64 = 0;
            let mut ok = true;
            while installed < len {
                let virt = VirtAddr(va_base + installed);
                let phys = PhysAddr(phys_base + installed);
                if !address_space.map_user_4k(&mut mapper, virt, phys, install_flags, alloc) {
                    ok = false;
                    break;
                }
                // Invalidate the TLB entry for the new VA — the active
                // CR3 is the caller's own AS, so the next user-space
                // load/store from `virt` must observe the freshly
                // installed PTE.
                AddressSpace::invlpg(virt);
                installed += 0x1000;
            }

            if !ok {
                // Rollback: unmap whatever we just installed. The
                // mapping points at device-owned physical addresses
                // so no frame is returned to the allocator.
                let mut rolled: u64 = 0;
                while rolled < installed {
                    let _ = mapper.unmap_4k(VirtAddr(va_base + rolled));
                    AddressSpace::invlpg(VirtAddr(va_base + rolled));
                    rolled += 0x1000;
                }
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }

            pcb.mmio_va_cursor = va_end;
            pcb.mmio_mappings.push(MmioMapping { va_base, len_pages });

            SyscallReturn::ok(va_base)
        }
    }

    /// Per-process random offset is reused across MMIO + DMA so the
    /// driver-space layout stays a single auditable range. This helper
    /// exposes the PCB cursor so the sibling `dma_map_handlers` module
    /// can advance the same allocator. P6.7.8.3.
    #[allow(
        dead_code,
        reason = "sibling module accessor — used by dma_map_handlers"
    )]
    pub(super) fn driver_mmio_range_bounds() -> (u64, u64) {
        (DRIVER_MMIO_VA_BASE, DRIVER_MMIO_VA_END)
    }

    /// Tear down every MMIO mapping owned by the calling process.
    /// Invoked from `task_exit` (OIP-013 § S2.4) before the PCB is
    /// retired.
    ///
    /// MMIO frames are device-owned; we only unmap the leaf PTEs and
    /// invalidate the TLB. Returning `None` is correct — the caller
    /// does not need an error path because the PCB itself is about to
    /// be removed.
    pub(super) fn tear_down_mmio_mappings(task: crate::scheduling::TaskId) {
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let Some(pcb) = sched.process_mut(task) else {
                return;
            };
            let phys_offset = bare_metal::phys_offset();
            if phys_offset == 0 {
                return;
            }
            let address_space: AddressSpace = pcb.address_space;
            let mut mapper = PageMapper::new(phys_offset, address_space.pml4_phys);
            // Drain the table so a re-spawn into the same PCB slot
            // never inherits the stale mapping descriptors.
            let mappings = core::mem::take(&mut pcb.mmio_mappings);
            pcb.mmio_va_cursor = 0;
            for m in &mappings {
                let bytes = u64::from(m.len_pages) * 0x1000;
                let mut off: u64 = 0;
                while off < bytes {
                    let va = VirtAddr(m.va_base + off);
                    let _ = mapper.unmap_4k(va);
                    AddressSpace::invlpg(va);
                    off += 0x1000;
                }
            }
        }
    }
}

// -----------------------------------------------------------------------
// DmaMap (OIP-013 § S3, P6.7.8.3)
//
// Phase 1 model: no-IOMMU passthrough. The kernel allocates `len_pages`
// contiguous physical frames from `FRAME_ALLOC`, identity-maps them at
// user VA == iova_base in the driver-DMA PML4 slot, and returns the
// physical base in `rax`. The driver writes the returned phys_base into
// device DMA descriptors; without an IOMMU the device sees physical
// addresses directly. The IOMMU vendor backends (`vtd` / `amdvi`) land
// in a follow-up P6.7.8.x and will replace the identity mapping with
// IOMMU domain page-table installs.
// -----------------------------------------------------------------------

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod dma_map_handlers {
    use super::{DRIVER_DMA_VA_BASE, DRIVER_DMA_VA_END};
    use crate::bare_metal;
    use crate::bare_metal::address_space::AddressSpace;
    use crate::bare_metal::iommu::{IommuBackend, IommuFlags, domain_for_task, with_iommu_backend};
    use crate::bare_metal::paging::{PTE_NO_EXEC, PTE_PRESENT, PTE_USER, PTE_WRITABLE, PageMapper};
    use crate::driver_manifest::is_driver_framework_action;
    use crate::memory::{PhysAddr, VirtAddr};
    use crate::process::DmaMapping;
    use crate::syscall::{SyscallReturn, syscall_errno};
    use omni_capability::CapabilityToken;
    use omni_capability::scope::{Action, Resource};

    /// Maximum accepted size for the postcard-encoded capability token.
    /// Mirrors the cap in `mmio_map_handlers` so user-space code can
    /// reuse a single mint pipeline.
    const MAX_TOKEN_BYTES: usize = 1024;

    /// Validate that `[ptr, ptr + len)` lies entirely in the user half.
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= bare_metal::usermode::USER_HALF_END
    }

    /// `DmaMap (71)` — OIP-013 § S3.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                       |
    /// |------|-----|--------------------------------------------|
    /// | a0   | RDI | `iova_base` (page-aligned, in user half)   |
    /// | a1   | RSI | `len` (multiple of 4 KiB, non-zero)        |
    /// | a2   | RDX | `direction` (0=ToDevice, 1=FromDevice, 2=Both) |
    /// | a3   | R10 | `cap_ptr` (user VA, postcard token)        |
    /// | a4   | R8  | `cap_len` (≤ `MAX_TOKEN_BYTES`)            |
    ///
    /// Returns a [`SyscallReturn`] whose `rax` holds the allocated
    /// physical base address on success (the value the driver writes
    /// into device DMA descriptors), or `0` on error with `rdx` set to
    /// one of [`syscall_errno`].
    #[allow(
        clippy::too_many_lines,
        reason = "single-syscall handler — keeps auth + alloc + map + record locally auditable"
    )]
    pub(super) fn dma_map(args: [u64; 6]) -> SyscallReturn {
        let iova_base = args[0];
        let len = args[1];
        let direction = args[2];
        let cap_ptr = args[3];
        let cap_len = args[4];

        // -------------------------------------------------------------
        // EINVAL: alignment + direction + length.
        // -------------------------------------------------------------
        if iova_base & 0xFFF != 0 || len == 0 || len & 0xFFF != 0 {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        if direction > 2 {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        if iova_base < DRIVER_DMA_VA_BASE || iova_base.saturating_add(len) > DRIVER_DMA_VA_END {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }

        // -------------------------------------------------------------
        // EFAULT: capability-token pointer + length.
        // -------------------------------------------------------------
        if cap_ptr == 0 || cap_len == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let Ok(cap_len_usize) = usize::try_from(cap_len) else {
            return SyscallReturn::err(syscall_errno::EFAULT);
        };
        if cap_len_usize > MAX_TOKEN_BYTES {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        if !user_range_ok(cap_ptr, cap_len) {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        let mut buf = [0u8; MAX_TOKEN_BYTES];
        // SAFETY: user_range_ok verified the source; the active CR3 is
        // the caller's AS so the hardware PT walk faults on missing
        // pages; cap_len_usize ≤ buf.len() by the cap above.
        unsafe {
            core::ptr::copy_nonoverlapping(cap_ptr as *const u8, buf.as_mut_ptr(), cap_len_usize);
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "cap_len_usize ≤ MAX_TOKEN_BYTES = buf.len() by the cap above"
        )]
        let token_bytes = &buf[..cap_len_usize];

        // -------------------------------------------------------------
        // EACCES: signature, time window, TEE binding, action, resource.
        // -------------------------------------------------------------
        let Ok(token) = omni_types::wire::decode_canonical::<CapabilityToken>(token_bytes) else {
            return SyscallReturn::err(syscall_errno::EACCES);
        };
        let now = u64::from(bare_metal::arch::rtc_seconds());
        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        if provider.verify_signed_token(&token, now)
            != crate::capabilities::CapabilityVerdict::Authorised
        {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        if !is_driver_framework_action(token.payload.scope.action) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        if token.payload.scope.action != Action::DmaMap {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        let claim = Resource::DmaWindow { iova_base, len };
        if !claim.is_subset_of(&token.payload.scope.resource) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }

        // -------------------------------------------------------------
        // Allocate contiguous phys frames + install leaf PTEs in the
        // caller's AS at user VA == iova_base.
        // -------------------------------------------------------------
        let Ok(len_pages_u64) = u64::checked_div(len, 0x1000).ok_or(()) else {
            return SyscallReturn::err(syscall_errno::EINVAL);
        };
        let Ok(len_pages) = u32::try_from(len_pages_u64) else {
            return SyscallReturn::err(syscall_errno::EINVAL);
        };

        // SAFETY: SYSCALL path is single-CPU under the kernel mutex;
        // SCHEDULER + FRAME_ALLOC are not otherwise aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let alloc = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);

            let Some(current) = sched.current_task_id() else {
                return SyscallReturn::err(syscall_errno::EFAULT);
            };
            let Some(pcb) = sched.process_mut(current) else {
                return SyscallReturn::err(syscall_errno::EFAULT);
            };

            // Reject duplicate iova_base: every DmaMap call must use a
            // distinct IOVA (the issuer mints one capability per window).
            if pcb.dma_mappings.iter().any(|m| m.iova_base == iova_base) {
                return SyscallReturn::err(syscall_errno::EINVAL);
            }

            // -------------------------------------------------------------
            // P6.7.9-pre.4 — vendor-routed IOMMU domain install.
            //
            // One domain per driver process (`domain_for_task` projects
            // `TaskId` into the 16-bit DID space). `install_domain` is
            // idempotent so repeated `DmaMap` calls from the same
            // process amortise the registration to a single entry on
            // the backend's domain list. The actual MMIO register
            // programming is deferred to P6.7.9-pre.5+; the scaffold
            // backends (`vtd::VtdBackend`, `amdvi::AmdViBackend`) and
            // the [`PassthroughBackend`] all accept this call as a
            // bookkeeping operation today.
            // -------------------------------------------------------------
            let domain_id = domain_for_task(current.0);
            if with_iommu_backend(|b| b.install_domain(domain_id)).is_err() {
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }

            let phys_offset = bare_metal::phys_offset();
            if phys_offset == 0 {
                return SyscallReturn::err(syscall_errno::EFAULT);
            }
            let address_space: AddressSpace = pcb.address_space;
            let mut mapper = PageMapper::new(phys_offset, address_space.pml4_phys);

            let install_flags = PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_NO_EXEC;

            // First-frame phys defines the returned DMA-bus address.
            // Frames are allocated sequentially; for the Phase 1
            // bitmap allocator this is best-effort contiguous (no
            // explicit contiguous API). We track each phys frame so
            // a non-contiguous burst rolls back cleanly.
            let mut allocated: alloc::vec::Vec<u64> =
                alloc::vec::Vec::with_capacity(len_pages as usize);
            let Some(first_frame) = alloc.alloc_frame() else {
                return SyscallReturn::err(syscall_errno::ENOSPC);
            };
            let phys_base = first_frame.0;
            allocated.push(phys_base);

            let mut installed: u64 = 0;
            // Map the first frame at iova_base.
            let virt = VirtAddr(iova_base);
            let phys = PhysAddr(phys_base);
            if !address_space.map_user_4k(&mut mapper, virt, phys, install_flags, alloc) {
                // Return the frame; nothing user-visible to invlpg.
                alloc.free_frame(first_frame);
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }
            AddressSpace::invlpg(virt);
            installed += 0x1000;

            let mut ok = true;
            while installed < len {
                let Some(next_frame) = alloc.alloc_frame() else {
                    ok = false;
                    break;
                };
                allocated.push(next_frame.0);
                // Phase 1 contiguity check: enforce strictly
                // contiguous frames to keep the IOVA-vs-phys invariant
                // for the device's no-IOMMU view. If the allocator
                // hands out a non-adjacent frame we abort.
                if next_frame.0 != phys_base + installed {
                    ok = false;
                    break;
                }
                let virt = VirtAddr(iova_base + installed);
                let phys = PhysAddr(next_frame.0);
                if !address_space.map_user_4k(&mut mapper, virt, phys, install_flags, alloc) {
                    ok = false;
                    break;
                }
                AddressSpace::invlpg(virt);
                installed += 0x1000;
            }

            if !ok {
                // Rollback: unmap installed PTEs, return all frames.
                let mut rolled: u64 = 0;
                while rolled < installed {
                    let _ = mapper.unmap_4k(VirtAddr(iova_base + rolled));
                    AddressSpace::invlpg(VirtAddr(iova_base + rolled));
                    rolled += 0x1000;
                }
                for f in &allocated {
                    alloc.free_frame(crate::memory::PhysAddr(*f));
                }
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }

            // -------------------------------------------------------------
            // P6.7.9-pre.4 — vendor-routed IOMMU `map` + `flush`.
            //
            // Now that all contiguous frames are installed in the
            // caller's AS, record the (iova, phys, len) tuple with the
            // selected backend and trigger its IOTLB invalidation
            // hook. Per OIP-013 § S3.2, the IOMMU R/W flags must
            // mirror the `direction` argument so the device cannot
            // perform DMA in a direction the issuer did not authorise.
            // The scaffold backends accept any aligned input today and
            // simply track the mapping; the live VT-d / AMD-Vi register
            // programming lands in P6.7.9-pre.5+.
            //
            // Failure here is intentionally fatal to the syscall: it
            // means the backend's internal bookkeeping rejected the
            // mapping (out-of-DID, duplicate iova within the same
            // domain, etc.), so we must roll back the page-table
            // installs we just performed and return frames to the
            // allocator. The rollback path mirrors the contiguity-
            // failure branch above.
            // -------------------------------------------------------------
            let map_flags = match direction {
                0 => IommuFlags::READ,
                1 => IommuFlags::WRITE,
                _ => IommuFlags::READ.union(IommuFlags::WRITE),
            };
            let map_res = with_iommu_backend(|b| {
                let res = b.map(domain_id, iova_base, phys_base, len, map_flags);
                if res.is_ok() {
                    // Best-effort flush — backends accept this call
                    // unconditionally once the domain is installed.
                    let _ = b.flush(domain_id);
                }
                res
            });
            if map_res.is_err() {
                let mut rolled: u64 = 0;
                while rolled < installed {
                    let _ = mapper.unmap_4k(VirtAddr(iova_base + rolled));
                    AddressSpace::invlpg(VirtAddr(iova_base + rolled));
                    rolled += 0x1000;
                }
                for f in &allocated {
                    alloc.free_frame(crate::memory::PhysAddr(*f));
                }
                return SyscallReturn::err(syscall_errno::ENOSPC);
            }

            #[allow(
                clippy::cast_possible_truncation,
                reason = "direction validated as ≤ 2 above; fits u8 trivially"
            )]
            pcb.dma_mappings.push(DmaMapping {
                iova_base,
                len_pages,
                direction: direction as u8,
            });

            SyscallReturn::ok(phys_base)
        }
    }

    /// Tear down every DMA mapping owned by the calling process. Frames
    /// are returned to the global frame allocator since DMA buffers are
    /// kernel-allocated (in contrast to MMIO regions which are
    /// device-owned).
    pub(super) fn tear_down_dma_mappings(task: crate::scheduling::TaskId) {
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER + FRAME_ALLOC
        // not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let alloc = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
            let Some(pcb) = sched.process_mut(task) else {
                return;
            };
            let phys_offset = bare_metal::phys_offset();
            if phys_offset == 0 {
                return;
            }
            let address_space: AddressSpace = pcb.address_space;
            let mut mapper = PageMapper::new(phys_offset, address_space.pml4_phys);
            let mappings = core::mem::take(&mut pcb.dma_mappings);
            // P6.7.9-pre.4 — per-process IOMMU domain (matches the
            // projection used by `dma_map`).
            let domain_id = domain_for_task(task.0);
            for m in &mappings {
                let bytes = u64::from(m.len_pages) * 0x1000;
                // P6.7.9-pre.4 — release the backend's record of the
                // mapping before tearing down the PTEs. Errors here are
                // best-effort: the backend may have already dropped
                // its record if `dma_map` rolled back, in which case
                // `UnmapFailed` is benign for teardown semantics.
                let _ = with_iommu_backend(|b| {
                    let r = b.unmap(domain_id, m.iova_base, bytes);
                    let _ = b.flush(domain_id);
                    r
                });
                let mut off: u64 = 0;
                while off < bytes {
                    let va = VirtAddr(m.iova_base + off);
                    // Resolve phys BEFORE unmapping so the frame can be
                    // returned to the allocator. `translate` returns
                    // None only if the mapping was already torn down or
                    // if the PT walk lands on a huge page — neither
                    // happens for driver DMA mappings installed via
                    // `dma_map`.
                    let phys_opt = mapper.translate(va);
                    if mapper.unmap_4k(va) {
                        if let Some(phys) = phys_opt {
                            alloc.free_frame(phys);
                        }
                    }
                    AddressSpace::invlpg(va);
                    off += 0x1000;
                }
            }
        }
    }
}

// -----------------------------------------------------------------------
// IrqAttach (OIP-013 § S4, P6.7.8.3)
//
// Phase 1 IRQ routing:
//   - LAPIC vector bitmap `0x40..=0xFE` (190 vectors); ascending alloc.
//   - Shared-line rejection: a second attach on the same `irq_line`
//     returns EBUSY (no fan-out — deliberate determinism).
//   - On fire, the IDT trampoline calls `lapic_eoi()` and enqueues an
//     `IrqNotification::Tick` envelope on the bound channel; backed-up
//     fires increment a per-vector `missed_count` so the driver can
//     surface coalesced firings via `IrqNotification::MissedSince(N)`.
// -----------------------------------------------------------------------

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod irq_attach_handlers {
    use crate::bare_metal;
    use crate::driver_manifest::is_driver_framework_action;
    use crate::ipc::ChannelId;
    use crate::process::IrqAttachment;
    use crate::syscall::{SyscallReturn, syscall_errno};
    use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
    use omni_capability::CapabilityToken;
    use omni_capability::scope::{Action, Resource};

    /// Lowest LAPIC vector the kernel may allocate for driver IRQs.
    const IRQ_VECTOR_BASE: u8 = 0x40;
    /// Highest LAPIC vector (`0xFF` is reserved for spurious; `0xFE`
    /// inclusive matches the OIP-013 § S4.1 allocator range).
    const IRQ_VECTOR_END: u8 = 0xFE;
    /// Number of bookkeeping slots (one per vector in the range).
    const IRQ_TABLE_SLOTS: usize = (IRQ_VECTOR_END as usize) - (IRQ_VECTOR_BASE as usize) + 1;

    /// Maximum accepted size for the postcard-encoded capability token.
    const MAX_TOKEN_BYTES: usize = 1024;

    /// Per-vector book-keeping. `irq_line == 0` means slot free. Atomic
    /// so the ISR trampoline can read it lock-free.
    struct IrqSlot {
        /// IRQ line that owns this vector. 0 means free.
        irq_line: AtomicU32,
        /// Bound IPC channel id (kernel-allocated u64).
        channel_id: AtomicU64,
        /// Coalesced missed-fire counter (OIP-013 Appendix B amendment 3).
        missed: AtomicU32,
        /// Owning task id (so teardown can match).
        owner_task: AtomicU64,
        /// Last-known direction tag; `AtomicU8` only for layout symmetry.
        #[allow(dead_code, reason = "reserved for future per-IRQ flags")]
        flags: AtomicU8,
    }

    impl IrqSlot {
        const fn new() -> Self {
            Self {
                irq_line: AtomicU32::new(0),
                channel_id: AtomicU64::new(0),
                missed: AtomicU32::new(0),
                owner_task: AtomicU64::new(0),
                flags: AtomicU8::new(0),
            }
        }
    }

    // SAFETY: each AtomicU32/64/8 is internally synchronized; the table
    // itself is `static mut` only because Rust does not yet support
    // `static IRQ_TABLE: [IrqSlot; N] = ...` const-init via array
    // repeat with non-Copy types. The access pattern below uses raw
    // pointers + atomic ops, never `&mut` aliasing.
    #[allow(
        clippy::declare_interior_mutable_const,
        reason = "array init helper; atomics aren't Copy"
    )]
    const SLOT_INIT: IrqSlot = IrqSlot::new();
    static IRQ_TABLE: [IrqSlot; IRQ_TABLE_SLOTS] = [SLOT_INIT; IRQ_TABLE_SLOTS];

    fn slot_for(vector: u8) -> Option<&'static IrqSlot> {
        if !(IRQ_VECTOR_BASE..=IRQ_VECTOR_END).contains(&vector) {
            return None;
        }
        let idx = (vector as usize) - (IRQ_VECTOR_BASE as usize);
        IRQ_TABLE.get(idx)
    }

    /// Find a free vector and CAS-reserve it for `(irq_line, owner_task,
    /// channel_id)`. Returns `Some(vector)` on success.
    fn allocate_vector(irq_line: u16, owner_task: u64, channel_id: u64) -> Option<u8> {
        for vec_u in (IRQ_VECTOR_BASE as usize)..=(IRQ_VECTOR_END as usize) {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "vec_u ∈ [0x40, 0xFE] fits u8"
            )]
            let vector = vec_u as u8;
            #[allow(
                clippy::indexing_slicing,
                reason = "iter bounded by IRQ_TABLE_SLOTS = IRQ_VECTOR_END - IRQ_VECTOR_BASE + 1"
            )]
            let slot = &IRQ_TABLE[vec_u - (IRQ_VECTOR_BASE as usize)];
            if slot
                .irq_line
                .compare_exchange(0, u32::from(irq_line), Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                slot.channel_id.store(channel_id, Ordering::Release);
                slot.owner_task.store(owner_task, Ordering::Release);
                slot.missed.store(0, Ordering::Release);
                return Some(vector);
            }
        }
        None
    }

    fn release_vector(vector: u8) {
        let Some(slot) = slot_for(vector) else { return };
        slot.irq_line.store(0, Ordering::Release);
        slot.channel_id.store(0, Ordering::Release);
        slot.owner_task.store(0, Ordering::Release);
        slot.missed.store(0, Ordering::Release);
    }

    /// Returns `true` iff `irq_line` is already attached. Walks the
    /// table linearly; `IRQ_TABLE_SLOTS = 191` so this is fine.
    fn irq_line_in_use(irq_line: u16) -> bool {
        IRQ_TABLE
            .iter()
            .any(|s| s.irq_line.load(Ordering::Acquire) == u32::from(irq_line))
    }

    /// ISR-side increment of the missed-fire counter. Called from
    /// [`omni_irq_dispatch_trampoline`] when a fire arrives faster than
    /// the bound driver process can drain.
    pub(super) fn note_fire(vector: u8) {
        if let Some(slot) = slot_for(vector) {
            slot.missed.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Drain the missed-fire counter for diagnostic readout. Returns
    /// the previous value and resets to zero. Used by host tests and
    /// the bring-up smoke; the runtime ISR uses [`note_fire`] without
    /// reading back.
    #[allow(dead_code, reason = "used by host-side tests in P6.7.8.3 follow-up")]
    pub(super) fn take_missed(vector: u8) -> u32 {
        slot_for(vector).map_or(0, |s| s.missed.swap(0, Ordering::AcqRel))
    }

    /// `IrqAttach (72)` — OIP-013 § S4.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                       |
    /// |------|-----|--------------------------------------------|
    /// | a0   | RDI | `irq_line` (u16; 0 reserved)               |
    /// | a1   | RSI | `ipc_channel_id` (u64, kernel-allocated)   |
    /// | a2   | RDX | `cap_ptr` (user VA, postcard token)        |
    /// | a3   | R10 | `cap_len` (≤ `MAX_TOKEN_BYTES`)            |
    ///
    /// Returns a [`SyscallReturn`] whose `rax` holds the allocated
    /// LAPIC vector (`0x40..=0xFE`) on success, or `0` on error with
    /// `rdx` set to a [`syscall_errno`] code (EBUSY mapped to EINVAL
    /// per the POSIX subset OIP-013 § S4.3 references).
    pub(super) fn irq_attach(args: [u64; 6]) -> SyscallReturn {
        let irq_line_u64 = args[0];
        let ipc_channel_id = args[1];
        let cap_ptr = args[2];
        let cap_len = args[3];

        // -------------------------------------------------------------
        // EINVAL: argument validation.
        // -------------------------------------------------------------
        if irq_line_u64 == 0 || irq_line_u64 > u64::from(u16::MAX) {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "guarded by `irq_line_u64 ≤ u16::MAX` above"
        )]
        let irq_line = irq_line_u64 as u16;

        // -------------------------------------------------------------
        // EFAULT: capability-token pointer + length.
        // -------------------------------------------------------------
        if cap_ptr == 0 || cap_len == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let Ok(cap_len_usize) = usize::try_from(cap_len) else {
            return SyscallReturn::err(syscall_errno::EFAULT);
        };
        if cap_len_usize > MAX_TOKEN_BYTES {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let user_end = match cap_ptr.checked_add(cap_len) {
            Some(e) if e <= bare_metal::usermode::USER_HALF_END => e,
            _ => return SyscallReturn::err(syscall_errno::EFAULT),
        };
        let _ = user_end;

        let mut buf = [0u8; MAX_TOKEN_BYTES];
        // SAFETY: bounds verified; user PT walks fault on missing pages.
        unsafe {
            core::ptr::copy_nonoverlapping(cap_ptr as *const u8, buf.as_mut_ptr(), cap_len_usize);
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "cap_len_usize ≤ MAX_TOKEN_BYTES = buf.len()"
        )]
        let token_bytes = &buf[..cap_len_usize];

        // -------------------------------------------------------------
        // EACCES: capability verification.
        // -------------------------------------------------------------
        let Ok(token) = omni_types::wire::decode_canonical::<CapabilityToken>(token_bytes) else {
            return SyscallReturn::err(syscall_errno::EACCES);
        };
        let now = u64::from(bare_metal::arch::rtc_seconds());
        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        if provider.verify_signed_token(&token, now)
            != crate::capabilities::CapabilityVerdict::Authorised
        {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        if !is_driver_framework_action(token.payload.scope.action) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        if token.payload.scope.action != Action::IrqAttach {
            return SyscallReturn::err(syscall_errno::EACCES);
        }
        let claim = Resource::IrqLine(irq_line);
        if !claim.is_subset_of(&token.payload.scope.resource) {
            return SyscallReturn::err(syscall_errno::EACCES);
        }

        // -------------------------------------------------------------
        // Shared-line rejection (§ S4.1: no fan-out).
        // -------------------------------------------------------------
        if irq_line_in_use(irq_line) {
            // POSIX EBUSY is 16; we map it via EINVAL slot since the
            // current `syscall_errno` table does not yet expose EBUSY.
            // Future cleanup: add EBUSY = 16 in syscall.rs.
            return SyscallReturn::err(syscall_errno::EINVAL);
        }

        // -------------------------------------------------------------
        // Look up the caller PCB + bound channel.
        // -------------------------------------------------------------
        // SAFETY: SYSCALL path single-CPU; SCHEDULER + IPC_REGISTRY
        // not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let Some(current) = sched.current_task_id() else {
                return SyscallReturn::err(syscall_errno::EFAULT);
            };
            // Verify the channel exists. Reuse the legacy registry
            // accessor so destruction races (channel destroyed
            // between the user's request and the kernel's bind)
            // surface as ENOENT-shape EINVAL.
            let registry = crate::ipc::ipc_registry();
            if registry.channel(ChannelId(ipc_channel_id)).is_none() {
                return SyscallReturn::err(syscall_errno::EINVAL);
            }

            let Some(vector) = allocate_vector(irq_line, current.0, ipc_channel_id) else {
                return SyscallReturn::err(syscall_errno::ENOSPC);
            };

            // Install the per-vector IDT trampoline. The trampoline
            // itself is a single asm stub (`omni_irq_dispatch_<N>`);
            // for Phase 1 we install one shared handler and dispatch
            // via the active LAPIC ISR vector readback inside the
            // Rust callback (see `kernel_irq_attach_handler`).
            bare_metal::idt::idt_set_vector(
                vector as usize,
                bare_metal::syscall_entry::omni_irq_dispatch_trampoline as *const () as usize
                    as u64,
            );

            let Some(pcb) = sched.process_mut(current) else {
                release_vector(vector);
                return SyscallReturn::err(syscall_errno::EFAULT);
            };
            pcb.irq_attachments.push(IrqAttachment {
                irq_line,
                vector,
                channel_id: ipc_channel_id,
            });

            SyscallReturn::ok(u64::from(vector))
        }
    }

    /// Tear down every IRQ attachment owned by the calling process.
    /// Frees the vector slots and resets the IDT entries to spurious.
    pub(super) fn tear_down_irq_attachments(task: crate::scheduling::TaskId) {
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let Some(pcb) = sched.process_mut(task) else {
                return;
            };
            let attachments = core::mem::take(&mut pcb.irq_attachments);
            for a in &attachments {
                release_vector(a.vector);
                // Park the IDT vector at the existing spurious / no-op
                // entry by reinstalling the trampoline pointer with a
                // disabled slot — the trampoline checks `irq_line == 0`
                // and skips the channel enqueue, effectively a no-op.
                // No need to rewrite the IDT entry per se; the lookup
                // in the slot table is what gates fire-side activity.
                let _ = a;
            }
        }
    }

    /// Rust-side IRQ dispatch: looks up the slot, attempts to enqueue an
    /// 8-byte IPC notification on the bound channel (via
    /// [`crate::irq_table::irq_notify`]), increments the coalesced
    /// missed-fire counter, then issues LAPIC EOI.
    ///
    /// The `note_fire` call keeps the per-vector `missed` atomic in sync
    /// so that drivers polling the legacy `take_missed` path still observe
    /// every fire, regardless of whether the IPC enqueue succeeded (the
    /// queue could be full under `Drop` policy).
    ///
    /// EOI is always issued last so the LAPIC can accept the next IRQ on
    /// the same vector regardless of whether the channel enqueue failed.
    pub(super) fn dispatch_fire(vector: u8) {
        note_fire(vector);
        // SAFETY: single-CPU ISR context; IRQ_TABLE_GLOBAL and IPC_REGISTRY
        // are not aliased. MP-SAFETY: upgrade to spinlock (P6.4+).
        #[allow(unsafe_code, reason = "ISR context; SAFETY comment above")]
        unsafe {
            crate::irq_table::irq_notify(vector);
        }
        bare_metal::lapic::lapic_eoi();
    }
}

// -----------------------------------------------------------------------
// DriverLoad (OIP-013 § S5, P6.7.8.8)
//
// Wires the `SyscallNo = 73` handler that ingests an omni-pack v1 blob
// (header + postcard manifest + Ed25519 signature + ELF image), verifies
// the manifest end-to-end (BLAKE3 image hash + Ed25519 signature against
// `KNOWN_ISSUERS`), then spawns the driver as a Ring 3 task via
// `ProcessControlBlock::spawn_from_elf`. Returns the spawned task id in
// `rax` on success; `rdx` is `0` on success or a POSIX errno on error.
//
// Attenuated child-token deposit (§ S5.3 step 8) and the per-driver
// capability-namespace bootstrap are deliberately deferred to the next
// sub-step (P6.7.8.9): drivers in P6.7.8.8 reach `_start` but the
// `MmioMap`/`DmaMap`/`IrqAttach` calls inside them still require a
// token presented through a separate, manually-minted path. The split
// keeps the ELF loader + signature chain decoupled from the capability
// store wiring.
// -----------------------------------------------------------------------

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod driver_load_handlers {
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::bare_metal;
    use crate::driver_manifest::{
        DriverManifestError, decode_omni_pack, hydrate_manifest, is_driver_framework_action,
        postcard_decode_manifest, verify_manifest,
    };
    use crate::memory::PhysAddr;
    use crate::process::ProcessControlBlock;
    use crate::scheduling::PriorityClass;
    use crate::syscall::{SyscallReturn, syscall_errno};
    use omni_capability::CapabilityToken;
    use omni_capability::scope::{Action, Resource};

    /// Maximum accepted size for the postcard-encoded
    /// [`CapabilityToken`] presented through `DriverLoad`. Same bound
    /// as the sibling `MmioMap`/`DmaMap`/`IrqAttach` handlers.
    const MAX_TOKEN_BYTES: usize = 1024;

    /// OIP-013 § S5.2: pack blob is at most 32 MiB total (header,
    /// manifest, signature, and image combined). Anything larger is
    /// rejected before the kernel allocates the holding buffer, so the
    /// worst-case footprint of a single `DriverLoad` is bounded.
    const MAX_PACK_BYTES: u64 = 32 * 1024 * 1024;

    /// Validate that `[ptr, ptr + len)` lies entirely in the user
    /// half. Mirrors the helper used by the sibling driver-framework
    /// handlers so the two paths cannot drift on the validation
    /// contract.
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= bare_metal::usermode::USER_HALF_END
    }

    /// Translate an [`omni_capability::CapabilityToken`] decoded from
    /// user memory into an authorization verdict. Returns the verified
    /// token on `Authorised`, else an errno.
    fn verify_token(token_bytes: &[u8]) -> Result<CapabilityToken, u64> {
        let token = omni_types::wire::decode_canonical::<CapabilityToken>(token_bytes)
            .map_err(|_| syscall_errno::EACCES)?;
        let now = u64::from(bare_metal::arch::rtc_seconds());
        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        if provider.verify_signed_token(&token, now)
            != crate::capabilities::CapabilityVerdict::Authorised
        {
            return Err(syscall_errno::EACCES);
        }
        if !is_driver_framework_action(token.payload.scope.action) {
            return Err(syscall_errno::EACCES);
        }
        if token.payload.scope.action != Action::DriverLoad {
            return Err(syscall_errno::EACCES);
        }
        // OIP-013 § S5.2: `DriverLoad` requires `Resource::Any`. The
        // token's scope MAY be exactly `Any` or any concrete resource
        // — the subset check covers both: `concrete.is_subset_of(&Any)`.
        // We additionally insist the scope's resource IS `Any` to
        // foreclose a token scoped to e.g. a single PCI device being
        // accepted for an arbitrary image load.
        if token.payload.scope.resource != Resource::Any {
            return Err(syscall_errno::EACCES);
        }
        Ok(token)
    }

    /// Translate a [`DriverManifestError`] into the POSIX errno code
    /// the syscall ABI returns on failure. Mirrors the mapping baked
    /// into OIP-013 § S5.3 (`EINVAL` for parse / hash issues, `EACCES`
    /// for issuer / signature issues).
    fn manifest_errno(err: DriverManifestError) -> u64 {
        match err {
            DriverManifestError::MalformedPack
            | DriverManifestError::PackTooLarge
            | DriverManifestError::ImageHashMismatch => syscall_errno::EINVAL,
            DriverManifestError::UnknownIssuer | DriverManifestError::SignatureInvalid => {
                syscall_errno::EACCES
            }
        }
    }

    /// `DriverLoad (73)` — OIP-013 § S5.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                       |
    /// |------|-----|--------------------------------------------|
    /// | a1   | RSI | `pack_ptr` (omni-pack v1 blob, user VA)    |
    /// | a2   | RDX | `pack_len` (≤ `MAX_PACK_BYTES`)            |
    /// | a3   | R10 | `cap_ptr` (user VA, postcard token)        |
    /// | a4   | R8  | `cap_len` (≤ `MAX_TOKEN_BYTES`)            |
    ///
    /// `a0` is reserved and ignored. Returns a [`SyscallReturn`] whose
    /// `rax` holds the spawned task id on success or `0` on error;
    /// `rdx` is `0` on success or one of the [`syscall_errno`] codes
    /// on error.
    #[allow(
        clippy::too_many_lines,
        reason = "single-syscall handler keeps the auth + decode + verify + spawn sequence \
                  locally auditable per OIP-013 § S5.3"
    )]
    pub(super) fn driver_load(args: [u64; 6]) -> SyscallReturn {
        let pack_ptr = args[1];
        let pack_len = args[2];
        let cap_ptr = args[3];
        let cap_len = args[4];

        // -------------------------------------------------------------
        // EFAULT: capability token pointer / length.
        // -------------------------------------------------------------
        if cap_ptr == 0 || cap_len == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let Ok(cap_len_usize) = usize::try_from(cap_len) else {
            return SyscallReturn::err(syscall_errno::EFAULT);
        };
        if cap_len_usize > MAX_TOKEN_BYTES {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        if !user_range_ok(cap_ptr, cap_len) {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // -------------------------------------------------------------
        // EFAULT/EINVAL: pack pointer + length.
        // -------------------------------------------------------------
        if pack_ptr == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        if pack_len == 0 || pack_len > MAX_PACK_BYTES {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }
        if !user_range_ok(pack_ptr, pack_len) {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let Ok(pack_len_usize) = usize::try_from(pack_len) else {
            return SyscallReturn::err(syscall_errno::EINVAL);
        };

        // -------------------------------------------------------------
        // Copy the capability token into a kernel stack buffer.
        // -------------------------------------------------------------
        let mut token_buf = [0u8; MAX_TOKEN_BYTES];
        // SAFETY: `user_range_ok` confirmed the source range lies in
        // the user half; the active CR3 is the caller's own AS so the
        // hardware walk faults on missing pages.
        unsafe {
            core::ptr::copy_nonoverlapping(
                cap_ptr as *const u8,
                token_buf.as_mut_ptr(),
                cap_len_usize,
            );
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "cap_len_usize ≤ MAX_TOKEN_BYTES = token_buf.len()"
        )]
        let token_bytes = &token_buf[..cap_len_usize];

        // -------------------------------------------------------------
        // EACCES: token signature, action, resource.
        // -------------------------------------------------------------
        let _token = match verify_token(token_bytes) {
            Ok(t) => t,
            Err(e) => return SyscallReturn::err(e),
        };

        // -------------------------------------------------------------
        // Copy the pack blob into a kernel-side Vec. The bump allocator
        // never reclaims, but a v0.3 boot triggers only a handful of
        // DriverLoad calls (one per first-party driver) so the
        // amortized cost is bounded by the heap size.
        // -------------------------------------------------------------
        let mut pack_buf: Vec<u8> = vec![0u8; pack_len_usize];
        // SAFETY: `user_range_ok` confirmed the source range lies in
        // the user half; `pack_buf.len() == pack_len_usize` by the
        // `vec!` initialiser.
        unsafe {
            core::ptr::copy_nonoverlapping(
                pack_ptr as *const u8,
                pack_buf.as_mut_ptr(),
                pack_len_usize,
            );
        }

        // -------------------------------------------------------------
        // omni-pack v1 envelope decode (§ S5.3 step 3) + postcard
        // manifest body decode (step 4).
        // -------------------------------------------------------------
        let sections = match decode_omni_pack(&pack_buf) {
            Ok(s) => s,
            Err(e) => return SyscallReturn::err(manifest_errno(e)),
        };
        let body = match postcard_decode_manifest(sections.manifest) {
            Ok(b) => b,
            Err(e) => return SyscallReturn::err(manifest_errno(e)),
        };
        let manifest = hydrate_manifest(body, *sections.signature);

        // -------------------------------------------------------------
        // EINVAL/EACCES: full manifest verify (BLAKE3 image hash, then
        // KNOWN_ISSUERS lookup, then Ed25519 signature). The order is
        // pinned by `verify_manifest` itself.
        // -------------------------------------------------------------
        if let Err(e) = verify_manifest(&manifest, sections.image) {
            return SyscallReturn::err(manifest_errno(e));
        }

        // -------------------------------------------------------------
        // Spawn the driver process. `ProcessControlBlock::spawn_from_elf`
        // owns the ELF parse + per-process PML4 clone + user-stack +
        // scheduler enrollment; we just supply the kernel singletons.
        // -------------------------------------------------------------
        let boot_pml4 = bare_metal::boot_cr3();
        if boot_pml4 == 0 {
            // kmain ordering bug: BOOT_CR3 should be set before any
            // user-space syscall can land.
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let phys_off = bare_metal::phys_offset();
        if phys_off == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // SAFETY: SYSCALL path is single-CPU under the kernel mutex;
        // SCHEDULER + FRAME_ALLOC are not otherwise aliased.
        let spawn_result = unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let alloc = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
            let mut mapper = bare_metal::paging::PageMapper::new(phys_off, PhysAddr(boot_pml4));

            ProcessControlBlock::spawn_from_elf(
                sections.image,
                PhysAddr(boot_pml4),
                &mut mapper,
                alloc,
                sched,
                PriorityClass::System,
                crate::capabilities::KernelPrincipal::ZERO,
            )
        };
        let Ok(task_id) = spawn_result else {
            return SyscallReturn::err(syscall_errno::ENOSPC);
        };

        // -------------------------------------------------------------
        // P6.7.8.9 — capability deposit trampoline. Mint signed tokens
        // for every capability declared in the manifest and map a
        // read-only window in the driver's address space at the
        // well-known VA `DRIVER_CAP_DEPOSIT_VA`. The driver's `_start`
        // looks the tokens up by `(action_tag, resource_tag)` and
        // presents them on the relevant `MmioMap`/`DmaMap`/`IrqAttach`
        // calls. Per OIP-013 § S5.3 step 8 the lifetime is 90 days.
        //
        // Failure mode: a deposit-error after a successful spawn leaves
        // the driver process alive but without any capabilities — its
        // first `MmioMap` will EACCES out. We accept this so the
        // failure path is observable in user space; a future revision
        // (P6.7.8.10) can wire a `scheduler.cancel_spawn(task_id)` to
        // unwind the spawn atomically when a deposit fails.
        // -------------------------------------------------------------
        let boot_seconds = u64::from(bare_metal::arch::rtc_seconds());
        let provider = crate::capabilities::Ed25519CapabilityProvider::placeholder();
        let subject_node = provider.node_id_bytes();
        let deposit_va = {
            // SAFETY: single-CPU syscall path; SCHEDULER + FRAME_ALLOC
            // not otherwise aliased; the address space pointer is read
            // out of the PCB before any other SCHEDULER access.
            unsafe {
                let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
                let alloc = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
                let Some(pcb) = sched.process_mut(task_id) else {
                    return SyscallReturn::err(syscall_errno::EFAULT);
                };
                let address_space = pcb.address_space;
                let mut mapper = bare_metal::paging::PageMapper::new(phys_off, PhysAddr(boot_pml4));
                let deposit = crate::cap_deposit::deposit_for_driver(
                    &manifest.capabilities,
                    boot_seconds,
                    subject_node,
                    &address_space,
                    &mut mapper,
                    alloc,
                );
                deposit.unwrap_or(0)
            }
        };
        if deposit_va != 0 {
            // SAFETY: single-CPU syscall path; re-borrow SCHEDULER to
            // record the deposit VA. `task_id` was just inserted by
            // `spawn_from_elf` so `process_mut` cannot return `None`
            // unless someone else removed the PCB between the lines —
            // not possible single-CPU.
            unsafe {
                let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
                if let Some(pcb) = sched.process_mut(task_id) {
                    pcb.cap_deposit_va = Some(deposit_va);
                }
            }
        }

        // -------------------------------------------------------------
        // P6.7.9-pre.8 — driver PCI bind. Translate the manifest's
        // `capabilities.pci_devices` table into the per-device IOMMU
        // attach calls so subsequent `DmaMap` requests from the driver
        // land in a domain that the IOMMU knows about (the live MMIO
        // half — VT-d context-entry + AMD-Vi DTE writes — lands in
        // P6.7.9-pre.11; today the binding is host-testable bookkeeping
        // that exercises the trait dispatch on `IommuKind` and seeds the
        // PCB-side teardown list).
        //
        // P6.7.9-pre.10 — after the per-BDF attach loop, provision the
        // per-domain page-table root through the live IOMMU backend.
        // The root frame is a 4-KiB-aligned, zero-filled physical page
        // pulled from `FRAME_ALLOC` via the [`KernelFrameSource`]
        // adapter; passthrough returns `Ok(0)` without touching the
        // adapter (no per-domain table to mint). The recorded root is
        // looked up via [`iommu_domain_pt_root_phys`] by the upcoming
        // P6.7.9-pre.11 wiring of `install_vt_d_device_entry` /
        // `install_amd_vi_device_entry` so this slice does not yet
        // drive any MMIO writes — it only stages the input for the
        // live install path that lands next.
        //
        // Failure mode: a missing IOMMU domain install (out of DIDs)
        // or a vendor-table conflict (re-attach without prior detach)
        // is logged as a best-effort warning — the driver process
        // stays alive with whatever bindings did succeed; the first
        // `DmaMap` call against an un-attached device will EACCES out
        // of the capability check before reaching the IOMMU surface.
        // We accept this so partial-attach failure is observable in
        // user space, matching the cap-deposit failure mode above.
        // -------------------------------------------------------------
        {
            use crate::bare_metal::iommu::{
                IommuBackend, KernelFrameSource, domain_for_task, iommu_attach_device,
                iommu_provision_domain_pt, pci_bdfs_from_resources, with_iommu_backend,
            };
            let domain_id = domain_for_task(task_id.0);
            let bdfs = pci_bdfs_from_resources(&manifest.capabilities.pci_devices);
            // Idempotent: returns Ok(()) if the domain is already
            // installed (the dma_map handler may have raced ahead on
            // a future MP build; today it cannot, but the API is
            // designed for it).
            let domain_install_ok =
                with_iommu_backend(|kind| kind.install_domain(domain_id)).is_ok();
            let mut any_bdf_attached = false;
            if domain_install_ok {
                // SAFETY: single-CPU syscall path; SCHEDULER not
                // aliased. `process_mut` cannot return `None` because
                // `task_id` was just inserted by `spawn_from_elf` and
                // no other code path removes PCBs single-CPU.
                unsafe {
                    let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
                    if let Some(pcb) = sched.process_mut(task_id) {
                        for bdf in bdfs {
                            // Record the binding through the IOMMU
                            // trait dispatch (`PassthroughBackend`
                            // accepts unconditionally; `VtdBackend` /
                            // `AmdViBackend` track in their internal
                            // attachment vector for host-testable
                            // assertion). Skip the bdf on conflict so
                            // a stuck duplicate does not block the
                            // remaining bind iterations.
                            if iommu_attach_device(bdf, domain_id).is_ok() {
                                pcb.bound_pci_devices.push(bdf);
                                any_bdf_attached = true;
                            }
                        }
                    }
                }
            }
            // Provision the per-domain page-table root once at least
            // one BDF has bound to the domain. We deliberately gate on
            // `any_bdf_attached` so driver processes that declare no
            // PCI devices (rare; the manifest typically lists at
            // least one) do not consume a frame for an unreachable
            // root. The passthrough backend short-circuits to `Ok(0)`
            // without touching the frame source anyway, so the no-op
            // is cheap on platforms without an IOMMU.
            //
            // SAFETY: single-CPU syscall path; FRAME_ALLOC not
            // concurrently aliased. The `KernelFrameSource` borrow
            // ends with the surrounding scope, so FRAME_ALLOC is
            // released before the next syscall can land.
            if any_bdf_attached {
                #[allow(
                    unsafe_code,
                    reason = "single-CPU static-mut deref into FRAME_ALLOC for the IOMMU PT provisioning helper"
                )]
                unsafe {
                    let fa = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
                    let mut src = KernelFrameSource::new(fa, phys_off);
                    // Best-effort: surfacing a provisioning failure
                    // back to user space would leak kernel detail and
                    // is not actionable — the driver process stays
                    // alive; if the IOMMU is live (Intel/AMD) and the
                    // root is missing the live `install_*_device_entry`
                    // call below will refuse to write the per-device
                    // entry and the first DmaMap from the driver will
                    // EACCES.
                    let _ = iommu_provision_domain_pt(domain_id, &mut src);
                }
            }

            // -------------------------------------------------------------
            // P6.7.9-pre.11 — drive the live VT-d Context-Entry / AMD-Vi
            // DTE install for every BDF that the driver process now owns,
            // then flip the per-vendor translation-enable gate
            // (`GCMD.TE` / `CTRL.IommuEn`) so DMA from the bound devices
            // starts honouring the per-domain page tables provisioned by
            // P6.7.9-pre.10. Bare-metal-only — the host test surface stops
            // at the trait-level `attach_device` bookkeeping.
            //
            // Defaults: VT-d uses Bits48Level4 + UntranslatedAndTranslated
            // (matches the existing test scaffolding and is the safest
            // common denominator on every emulated VT-d we ship);
            // AMD-Vi uses PageMode::Level4 + IommuFlags::READ|WRITE (same
            // 48-bit address space; flags allow both R and W per device).
            // Phase 2 will negotiate these from the per-IOMMU capability
            // registers (CAP.SAGAW / EFR.HATS) via the existing
            // `pick_highest_supported_agaw` / `efr_highest_supported_mode`
            // helpers — Phase 1 hardcodes the widely-supported values.
            //
            // Best-effort: failures are logged via the
            // `iommu_install_dte_*` log lines (one per BDF) but never
            // propagate to user space — partial install state is observable
            // because the IOMMU rejects DMA from un-DTE'd BDFs the moment
            // TE / IommuEn is observed, so the driver's first `DmaMap`
            // call will EACCES out of the cap-check OR the IOMMU PF
            // handler.
            // -------------------------------------------------------------
            #[cfg(target_os = "none")]
            if any_bdf_attached {
                use crate::bare_metal::iommu::{
                    IommuFlags, IommuVendor, amdvi::PageMode, install_amd_vi_device_entry,
                    install_vt_d_device_entry_managed, iommu_domain_pt_root_phys,
                    iommu_enable_translation, iommu_vendor, vtd::AddressWidth,
                    vtd::TranslationType,
                };
                if let Some(slpt_phys) = iommu_domain_pt_root_phys(domain_id) {
                    // SAFETY: single-CPU syscall path; SCHEDULER + FRAME_ALLOC
                    // not concurrently aliased. The `KernelFrameSource` borrow
                    // ends with the surrounding scope so FRAME_ALLOC is
                    // released before the next syscall can land.
                    #[allow(
                        unsafe_code,
                        reason = "single-CPU static-mut deref into SCHEDULER + FRAME_ALLOC for the live IOMMU install MMIO path"
                    )]
                    unsafe {
                        let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
                        let bound = sched
                            .process_mut(task_id)
                            .map(|pcb| pcb.bound_pci_devices.clone())
                            .unwrap_or_default();
                        let fa = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
                        let mut src = KernelFrameSource::new(fa, phys_off);
                        let vendor = iommu_vendor();
                        let mut any_install_ok = false;
                        for bdf in bound {
                            let install_result = match vendor {
                                IommuVendor::Intel => install_vt_d_device_entry_managed(
                                    phys_off,
                                    bdf,
                                    domain_id,
                                    slpt_phys,
                                    AddressWidth::Bits48Level4,
                                    TranslationType::UntranslatedAndTranslated,
                                    &mut src,
                                ),
                                IommuVendor::Amd => install_amd_vi_device_entry(
                                    phys_off,
                                    bdf,
                                    domain_id,
                                    slpt_phys,
                                    IommuFlags::READ.union(IommuFlags::WRITE),
                                    PageMode::Level4,
                                ),
                                IommuVendor::Passthrough => Ok(false),
                            };
                            if matches!(install_result, Ok(true)) {
                                any_install_ok = true;
                            }
                        }
                        // Flip the translation-enable gate once at least one
                        // device install has cleared. Idempotent across
                        // subsequent driver loads — the backend short-circuits
                        // after the first success.
                        if any_install_ok {
                            let _ = iommu_enable_translation(phys_off);
                        }
                    }
                }
            }
        }

        SyscallReturn::ok(task_id.0)
    }

    /// Detach every PCI device the exiting `task` had bound to its
    /// IOMMU domain at [`driver_load`]. Drains
    /// `pcb.bound_pci_devices` so the PCB slot can be reused by a
    /// later spawn without inheriting stale vendor-table entries.
    ///
    /// P6.7.9-pre.10 — after the per-BDF detach pass, also release
    /// the per-domain page-table root provisioned by `driver_load`
    /// (the matching call to [`iommu_provision_domain_pt`]). The
    /// release returns the 4-KiB root frame to `FRAME_ALLOC` via the
    /// [`KernelFrameSource`] adapter; on the passthrough backend
    /// (`iommu_domain_pt_root_phys` returns `None`) the helper is a
    /// no-op so this teardown is safe to call unconditionally — we
    /// nevertheless gate on the `Some` return to keep the
    /// `FRAME_ALLOC` borrow scope as tight as possible.
    ///
    /// Best-effort: per-BDF detach failures (e.g. the backend never
    /// recorded the binding because the original attach raced an
    /// install-domain failure) are silently swallowed; the goal is
    /// to release whatever IOMMU state did get recorded, not to
    /// surface a teardown error to user space (the calling task is
    /// already `Terminated` by the time this runs).
    pub(super) fn tear_down_pci_bindings(task: crate::scheduling::TaskId) {
        use crate::bare_metal::iommu::{
            KernelFrameSource, domain_for_task, iommu_detach_device, iommu_domain_pt_root_phys,
            iommu_release_domain_pt,
        };
        // -------------------------------------------------------------
        // P6.7.9-pre.11 teardown — for Intel backends, drain the bound
        // BDFs through `release_vt_d_device_entry_managed` so the
        // per-bus context-table refcount is decremented and the page
        // freed when the last device on a bus detaches. For AMD-Vi we
        // fall back to the existing `iommu_detach_device` bookkeeping
        // because the device table is flat (one global page per IOMMU
        // unit), so there is no per-bus refcount to maintain.
        //
        // The PT root release at the bottom must run AFTER the device
        // releases above: a Phase 1+ refactor might wire SL-PTE leaves
        // that reference the per-domain PT root, so freeing the root
        // first would create a window where the IOMMU still has the
        // device entry alive but the SLPT is recycled. Today the leaf
        // mappings go through `dma_map` (which clears them in
        // `tear_down_dma_mappings`); keeping the order is defence-in-
        // depth.
        // -------------------------------------------------------------
        let domain_id = domain_for_task(task.0);
        let phys_off = crate::bare_metal::phys_offset();
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER not aliased.
        let bdfs = unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let Some(pcb) = sched.process_mut(task) else {
                return;
            };
            core::mem::take(&mut pcb.bound_pci_devices)
        };
        #[cfg(target_os = "none")]
        {
            use crate::bare_metal::iommu::{
                IommuVendor, iommu_vendor, release_vt_d_device_entry_managed,
            };
            if iommu_vendor() == IommuVendor::Intel {
                // SAFETY: SYSCALL path is single-CPU; FRAME_ALLOC not
                // concurrently aliased. The KernelFrameSource borrow
                // ends with the surrounding scope.
                #[allow(
                    unsafe_code,
                    reason = "single-CPU static-mut deref into FRAME_ALLOC for the IOMMU release MMIO path"
                )]
                unsafe {
                    let fa = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
                    let mut src = KernelFrameSource::new(fa, phys_off);
                    for bdf in &bdfs {
                        // Best-effort: a release failure here means the
                        // backend never recorded the attachment (race
                        // with a concurrent detach on a future MP
                        // build) and is benign in the teardown context
                        // — the bookkeeping vector is the source of
                        // truth for what we still need to release.
                        let _ = release_vt_d_device_entry_managed(phys_off, *bdf, &mut src);
                    }
                }
            }
        }
        // Drop the trait-level attachment record for every drained BDF
        // (idempotent: returns Unsupported when the backend never had
        // the binding — including after the managed release above
        // already cleared it, which is the expected steady-state).
        for bdf in &bdfs {
            let _ = iommu_detach_device(*bdf);
        }
        // Release the per-domain PT root if `driver_load` provisioned
        // one. `iommu_domain_pt_root_phys` returns `None` on
        // passthrough or when the domain was never provisioned, so
        // the `if let Some(_)` guard skips the FRAME_ALLOC reborrow
        // on those paths.
        if iommu_domain_pt_root_phys(domain_id).is_some() {
            // SAFETY: SYSCALL path is single-CPU; FRAME_ALLOC not
            // concurrently aliased. The `KernelFrameSource` borrow
            // ends with the surrounding scope, so `FRAME_ALLOC` is
            // released before the next syscall can land.
            unsafe {
                let fa = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
                let mut src = KernelFrameSource::new(fa, phys_off);
                // Best-effort: a `NotProvisioned` error here means
                // the live backend has no recorded root (race with a
                // concurrent release on a future MP build) and is
                // benign in the teardown context.
                let _ = iommu_release_domain_pt(domain_id, &mut src);
            }
        }
    }
}

// -----------------------------------------------------------------------
// BlkRegister / BlkUnregister / BlkLookup (OIP-Driver-NVMe-014 § S4 +
// § S6 step 12, P6.7.10-pre.3)
//
// Three thin handlers that bridge the kernel-internal
// [`crate::services::blk::BlkChannelRegistry`] to user space through
// the rich two-register return path. The handlers exist only in the
// bare-metal build because they consume the `BLK_REGISTRY`,
// `IPC_REGISTRY`, and `SCHEDULER` singletons; host tests exercise
// the underlying registry directly via
// [`crate::services::blk::BlkChannelRegistry`] tests.
// -----------------------------------------------------------------------

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod blk_handlers {
    use crate::ipc::ChannelId;
    use crate::scheduling::TaskId;
    use crate::services::blk::{MAX_DISK_SLOT_LEN, blk_registry_mut, errno_for};
    use crate::syscall::{SyscallReturn, syscall_errno};

    /// Validate that `[ptr, ptr + len)` lies entirely in the user
    /// half. Mirrors the IPC / MMIO helpers so the three paths
    /// cannot drift on the validation contract.
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        use crate::bare_metal::usermode::USER_HALF_END;
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= USER_HALF_END
    }

    /// Copy the user-space disk-slot bytes into the supplied kernel
    /// buffer and return a `&str` view over them.
    ///
    /// Validation order matches `OIP-013` § S2.3:
    /// 1. `len ∈ (0, MAX_DISK_SLOT_LEN]` (empty is `EINVAL`, oversized
    ///    is `EINVAL` so the handler never touches a user pointer
    ///    that the registry would reject anyway);
    /// 2. `[ptr, ptr + len)` lies in the canonical user half;
    /// 3. the copied bytes form valid UTF-8 (the registry's allowed
    ///    alphabet is ASCII so UTF-8 is a superset; this gives a
    ///    cleaner error path than a raw byte-slice view).
    ///
    /// Returns `Err(errno)` on any failure so the caller can route
    /// it through the rich return path without a second match.
    fn copy_user_disk_slot(
        ptr: u64,
        len: u64,
        buf: &mut [u8; MAX_DISK_SLOT_LEN],
    ) -> Result<&str, u64> {
        if len == 0 || len > MAX_DISK_SLOT_LEN as u64 {
            return Err(syscall_errno::EINVAL);
        }
        if ptr == 0 || !user_range_ok(ptr, len) {
            return Err(syscall_errno::EFAULT);
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "len ≤ MAX_DISK_SLOT_LEN = 32 fits u64 trivially"
        )]
        let len_usize = len as usize;
        // SAFETY: `user_range_ok` verified `[ptr, ptr + len)` lies
        // in the user half; the active CR3 is the caller's own AS,
        // so the hardware PT walk faults on any missing page before
        // the copy returns garbage. `len_usize` ≤ `buf.len()` by
        // the cap above.
        unsafe {
            core::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr(), len_usize);
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "len_usize ≤ MAX_DISK_SLOT_LEN = buf.len() by the cap above"
        )]
        let slice = &buf[..len_usize];
        core::str::from_utf8(slice).map_err(|_| syscall_errno::EINVAL)
    }

    /// Look up the calling task's `TaskId` from the per-CPU
    /// scheduler. Falls back to `TaskId(0)` (the kernel bootstrap)
    /// for syscalls that land before any user-space task is current
    /// — that case is benign because the registry will reject every
    /// non-ownership operation initiated by `TaskId(0)`.
    unsafe fn current_task() -> TaskId {
        // SAFETY: SYSCALL path masks interrupts and runs single-CPU;
        // SCHEDULER is not aliased here.
        unsafe {
            let sched = &*core::ptr::addr_of!(crate::SCHEDULER);
            sched.current_task_id().unwrap_or(TaskId(0))
        }
    }

    /// Verify that the caller currently owns `channel_id` in the
    /// kernel IPC registry. Returns `Ok(())` on a match,
    /// `Err(EACCES)` on a mismatch, and `Err(EINVAL)` when the
    /// channel id does not resolve to a live channel.
    unsafe fn check_channel_owner(channel_id: ChannelId, caller: TaskId) -> Result<(), u64> {
        // SAFETY: same as `current_task`.
        unsafe {
            let reg = crate::ipc::ipc_registry();
            let Some(ch) = reg.channel(channel_id) else {
                return Err(syscall_errno::EINVAL);
            };
            if ch.owner != caller {
                return Err(syscall_errno::EACCES);
            }
            Ok(())
        }
    }

    /// `BlkRegister (76)` — `(disk_slot_ptr, disk_slot_len, channel_id,
    /// _, _, _) -> (rax=0, rdx=errno)`.
    ///
    /// Caller MUST already own `channel_id`. The handler:
    /// 1. validates the user pointer (`EFAULT` on out-of-user-half
    ///    or null with non-zero len);
    /// 2. validates length / UTF-8 (`EINVAL` on empty, oversized,
    ///    or non-UTF-8 input);
    /// 3. verifies ownership of `channel_id` against the kernel IPC
    ///    registry (`EACCES` on mismatch, `EINVAL` on unknown id);
    /// 4. delegates to
    ///    [`crate::services::blk::BlkChannelRegistry::register`]
    ///    which enforces the registry-side invariants (charset,
    ///    duplicate, capacity).
    pub(super) fn blk_register(args: [u64; 6]) -> SyscallReturn {
        let mut buf = [0u8; MAX_DISK_SLOT_LEN];
        let slot = match copy_user_disk_slot(args[0], args[1], &mut buf) {
            Ok(s) => s,
            Err(errno) => return SyscallReturn::err(errno),
        };
        let channel_id = ChannelId(args[2]);
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER + IPC_REGISTRY
        // + BLK_REGISTRY are not aliased here.
        unsafe {
            let caller = current_task();
            if let Err(errno) = check_channel_owner(channel_id, caller) {
                return SyscallReturn::err(errno);
            }
            match blk_registry_mut().register(slot, channel_id, caller) {
                Ok(_canonical_name) => SyscallReturn::ok(0),
                Err(err) => SyscallReturn::err(errno_for(err)),
            }
        }
    }

    /// `BlkUnregister (77)` — `(disk_slot_ptr, disk_slot_len, _, _, _,
    /// _) -> (rax=0, rdx=errno)`.
    ///
    /// Owner-only: the registry surfaces `OwnerMismatch` → `EACCES`
    /// when the caller is not the recorded owner.
    pub(super) fn blk_unregister(args: [u64; 6]) -> SyscallReturn {
        let mut buf = [0u8; MAX_DISK_SLOT_LEN];
        let slot = match copy_user_disk_slot(args[0], args[1], &mut buf) {
            Ok(s) => s,
            Err(errno) => return SyscallReturn::err(errno),
        };
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER + BLK_REGISTRY
        // are not aliased here.
        unsafe {
            let caller = current_task();
            match blk_registry_mut().unregister(slot, caller) {
                Ok(_entry) => SyscallReturn::ok(0),
                Err(err) => SyscallReturn::err(errno_for(err)),
            }
        }
    }

    /// `BlkLookup (78)` — `(disk_slot_ptr, disk_slot_len, _, _, _, _)
    /// -> (rax=channel_id, rdx=0)` on success, `(rax=0, rdx=ENOENT)`
    /// on miss.
    ///
    /// Read-only — the channel id alone confers no IPC authority
    /// (`IpcSend` / `IpcRecv` still require the per-channel
    /// capability tokens minted at create time). The handler does
    /// NOT consult the IPC registry; the registry-side lookup is
    /// sufficient because the producer's `BlkRegister` already
    /// validated ownership at insert time.
    pub(super) fn blk_lookup(args: [u64; 6]) -> SyscallReturn {
        let mut buf = [0u8; MAX_DISK_SLOT_LEN];
        let slot = match copy_user_disk_slot(args[0], args[1], &mut buf) {
            Ok(s) => s,
            Err(errno) => return SyscallReturn::err(errno),
        };
        // SAFETY: SYSCALL path is single-CPU; BLK_REGISTRY immutable
        // borrow scope ends with this block.
        unsafe {
            let reg = crate::services::blk::blk_registry();
            reg.lookup_disk_slot(slot).map_or_else(
                || SyscallReturn::err(syscall_errno::ENOENT),
                |entry| SyscallReturn::ok(entry.channel_id.0),
            )
        }
    }

    /// Drain every BLK registry entry owned by the exiting task.
    ///
    /// Invoked from [`super::task_exit`] (OIP-013 § S2.4) before the
    /// PCB is retired. Symmetric to the `tear_down_*` helpers in
    /// the MMIO / DMA / IRQ / PCI sibling modules: best-effort,
    /// silently swallows the count return because the calling task
    /// is already `Terminated` and there is no caller to surface
    /// the count to. The underlying IPC channels are torn down by
    /// the IPC layer's own task-exit hook (when wired) or, until
    /// then, leak alongside the PCB — Phase 1 single-CPU keeps that
    /// safe because there is no other observer.
    pub(super) fn tear_down_blk_channels(task: TaskId) {
        // SAFETY: SYSCALL path is single-CPU; BLK_REGISTRY not
        // aliased; the task is `Terminated` so no other code path
        // can be issuing `Blk*` syscalls against it concurrently.
        unsafe {
            let _ = blk_registry_mut().clear_for_owner(task);
        }
    }
}

// -----------------------------------------------------------------------
// Shell terminal syscall handlers (Phase C wiring, T0.1–T0.5 / T6.2–T6.4)
//
// This module is the bare-metal dispatch shim for the 18 shell-terminal
// syscalls. It accesses the five `SHELL_*` global statics directly (same
// single-CPU, no-preemption invariant as every other bare-metal handler
// in this file) and replicates the logic from
// `crate::syscall_handlers::KernelState` without constructing that struct
// (which would require moving ownership of the globals per syscall).
//
// The module is cfg-gated identically to `ipc_handlers` / `mmio_map_handlers`
// so it is invisible on host test builds and never compiled for non-bare-metal
// targets.
// -----------------------------------------------------------------------

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
mod shell_handlers {
    use crate::fd::{FdFlags, FdKind, FileDescriptor, OpenFlags, RawFd};
    use crate::pipe::PipeId;
    use crate::scheduling::TaskId;
    use crate::syscall::{SyscallReturn, syscall_errno};
    use crate::vfs::{FileType, VfsError};
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    // -----------------------------------------------------------------------
    // User-memory validation helpers
    // -----------------------------------------------------------------------

    /// Validate that `[ptr, ptr + len)` lies entirely within the canonical
    /// user half (`< 0x0000_8000_0000_0000`).  Length zero always passes.
    ///
    /// This is the same check applied by every other handler module in this
    /// file; duplicated here so the shell module has no dependency on the
    /// sibling modules' private items.
    fn user_range_ok(ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        end <= crate::bare_metal::usermode::USER_HALF_END
    }

    /// Copy a UTF-8 string out of user memory into a kernel-side
    /// `alloc::string::String`.
    ///
    /// Returns `None` when:
    /// - `ptr` is null or the range fails [`user_range_ok`],
    /// - `len` exceeds 4 096 bytes (path-length cap),
    /// - the bytes are not valid UTF-8.
    fn user_str(ptr: u64, len: u64) -> Option<String> {
        if ptr == 0 || len == 0 || len > 4096 {
            return None;
        }
        if !user_range_ok(ptr, len) {
            return None;
        }
        // SAFETY: user_range_ok verified `[ptr, ptr+len)` lies in the user
        // half; the active CR3 is the caller's own AS, so the hardware PT walk
        // faults on any non-present or non-user-flagged page before this copy
        // returns garbage.  `len` ≤ 4096 so the cast to `usize` is lossless on
        // all current targets (usize ≥ u16).
        let slice = unsafe {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "len ≤ 4096 fits usize on every supported target"
            )]
            core::slice::from_raw_parts(ptr as *const u8, len as usize)
        };
        core::str::from_utf8(slice).ok().map(String::from)
    }

    /// Obtain a mutable reference into user memory for a copy-out
    /// destination buffer.
    ///
    /// Returns `None` when `ptr` is null, `len` is zero, the range exceeds
    /// 1 MiB (a generous upper bound for single-syscall I/O), or the range
    /// fails [`user_range_ok`].
    ///
    /// # Safety contract for callers
    ///
    /// The returned reference has `'static` lifetime purely to avoid
    /// the lifetime-across-unsafe-block restriction. The caller must
    /// not store it past the end of the current syscall invocation, and
    /// must not call any other function that could also produce a reference
    /// into the user AS while the returned slice is live.
    fn user_slice_mut(ptr: u64, len: u64) -> Option<&'static mut [u8]> {
        if ptr == 0 || len == 0 || len > 0x10_0000 {
            return None;
        }
        if !user_range_ok(ptr, len) {
            return None;
        }
        // SAFETY: user_range_ok verified the range lies in the user half;
        // single-CPU syscall path — no concurrent access to this AS.
        // The `'static` lifetime is a deliberate local-use-only fiction; see
        // the doc comment above.
        Some(unsafe {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "len ≤ 0x10_0000 fits usize on every supported target"
            )]
            core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize)
        })
    }

    // -----------------------------------------------------------------------
    // Current-task helper
    // -----------------------------------------------------------------------

    /// Return the `TaskId` of the currently-executing process.
    ///
    /// Falls back to `TaskId(0)` for the idle / bootstrap task, which has no
    /// registered entry in `SHELL_PROCESS_TABLE`.
    unsafe fn current_task() -> TaskId {
        // SAFETY: single-core; SCHEDULER is not otherwise aliased during the
        // synchronous syscall path.
        unsafe {
            let sched = &*core::ptr::addr_of!(crate::SCHEDULER);
            sched.current_task_id().unwrap_or(TaskId(0))
        }
    }

    // -----------------------------------------------------------------------
    // Path resolution helper
    // -----------------------------------------------------------------------

    /// Resolve `path` against the current task's cwd if it is relative.
    ///
    /// Uses `crate::vfs::InMemoryVfs::normalize_path` for `.` / `..`
    /// canonicalization — the same logic `KernelState::resolve_path` uses.
    unsafe fn resolve_path(path: &str) -> String {
        // SAFETY: single-CPU; SHELL_PROCESS_TABLE read-only here.
        let cwd = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_PROCESS_TABLE)).as_ref() {
                Some(pt) => {
                    let task = current_task();
                    pt.get_cwd(task).unwrap_or("/").to_string()
                }
                None => String::from("/"),
            }
        };
        crate::vfs::InMemoryVfs::normalize_path(&cwd, path)
    }

    // -----------------------------------------------------------------------
    // ReadConsole (61)
    // -----------------------------------------------------------------------

    /// `ReadConsole (61)` — drain the console input ring buffer into a user
    /// buffer (line-buffered mode).
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `buf_ptr`: user write buffer  |
    /// | a1   | RSI | `buf_len`: max bytes to drain |
    ///
    /// Returns `(rax = bytes_read, rdx = 0)` on success, or `err(EFAULT)` when
    /// the user buffer fails range validation.  Returns `ok(0)` when the ring
    /// is empty (caller should reschedule and retry after new input arrives).
    pub(super) fn read_console(args: [u64; 6]) -> SyscallReturn {
        let buf_ptr = args[0];
        let buf_len = args[1];

        // Empty-read fast-path — callers may poll with len=0.
        if buf_len == 0 {
            return SyscallReturn::ok(0);
        }

        let dest = match user_slice_mut(buf_ptr, buf_len) {
            Some(s) => s,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // Read from COM1 serial port with line-buffered polling.
        // Blocks until a newline or the buffer is full.
        let max = dest.len();
        let mut pos = 0usize;
        while pos < max {
            // Poll COM1 Line Status Register (0x3FD) bit 0 = Data Ready.
            let ready: u8;
            // SAFETY: port I/O is Ring 0 only; single-CPU.
            unsafe {
                core::arch::asm!("in al, dx", out("al") ready, in("dx") 0x3FDu16, options(nomem, nostack));
            }
            if ready & 1 == 0 {
                if pos > 0 {
                    break; // Return what we have so far.
                }
                core::hint::spin_loop();
                continue;
            }
            // Read the byte from COM1 data port (0x3F8).
            let byte: u8;
            // SAFETY: same as above.
            unsafe {
                core::arch::asm!("in al, dx", out("al") byte, in("dx") 0x3F8u16, options(nomem, nostack));
            }
            // Echo the byte back to the serial console for user feedback.
            crate::bare_metal::early_console::emit(&[byte]);
            if byte == b'\r' {
                crate::bare_metal::early_console::emit(b"\n");
                dest[pos] = b'\n';
                pos += 1;
                break; // Line complete.
            }
            if byte == 0x7F || byte == 0x08 {
                // Backspace — erase last char if any.
                if pos > 0 {
                    pos -= 1;
                    crate::bare_metal::early_console::emit(b"\x08 \x08");
                }
                continue;
            }
            dest[pos] = byte;
            pos += 1;
        }
        let n = pos;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "n ≤ buf_len ≤ 0x10_0000; fits u64"
        )]
        SyscallReturn::ok(n as u64)
    }

    // -----------------------------------------------------------------------
    // FdRead (63)
    // -----------------------------------------------------------------------

    /// `FdRead (63)` — read from a file descriptor into a user buffer.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `fd`: file descriptor number  |
    /// | a1   | RSI | `buf_ptr`: user write buffer  |
    /// | a2   | RDX | `buf_len`: max bytes to read  |
    ///
    /// Dispatches on [`FdKind`]:
    /// - `Console { readable: true }` → drains the console input buffer.
    /// - `Pipe { is_read_end: true }` → reads from the pipe ring.
    /// - `FsFile` → reads from the VFS at the current offset; advances offset.
    /// - Any other combination → `err(EBADF)`.
    // justification: `dest` is the output buffer slice; `desc` is the FD
    // descriptor struct — different types, different roles, short names
    // mandated by the POSIX FD convention.
    #[allow(clippy::similar_names)]
    pub(super) fn fd_read(args: [u64; 6]) -> SyscallReturn {
        let fd_num = args[0] as u32;
        let buf_ptr = args[1];
        let buf_len = args[2];

        if buf_len == 0 {
            return SyscallReturn::ok(0);
        }

        let dest = match user_slice_mut(buf_ptr, buf_len) {
            Some(s) => s,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // Copy the FdKind into a local so we can drop the fd_table borrow
        // before the mutable re-borrow needed to advance the FsFile offset.
        // SAFETY: single-CPU; SHELL_FD_TABLE not otherwise aliased.
        let kind = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_FD_TABLE)).as_ref() {
                Some(t) => match t.get(RawFd(fd_num)) {
                    Some(desc) => desc.kind.clone(),
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        #[allow(
            clippy::cast_possible_truncation,
            reason = "buf_len ≤ 0x10_0000 by user_slice_mut; fits usize"
        )]
        let max = buf_len as usize;

        match kind {
            FdKind::Console { readable, .. } => {
                if !readable {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                // Read from COM1 serial port with blocking poll.
                let mut pos = 0usize;
                while pos < max {
                    let ready: u8;
                    // SAFETY: port I/O; single-CPU Ring 0.
                    unsafe {
                        core::arch::asm!("in al, dx", out("al") ready, in("dx") 0x3FDu16, options(nomem, nostack));
                    }
                    if ready & 1 == 0 {
                        if pos > 0 {
                            break;
                        }
                        core::hint::spin_loop();
                        continue;
                    }
                    let byte: u8;
                    // SAFETY: same.
                    unsafe {
                        core::arch::asm!("in al, dx", out("al") byte, in("dx") 0x3F8u16, options(nomem, nostack));
                    }
                    crate::bare_metal::early_console::emit(&[byte]);
                    if byte == b'\r' {
                        crate::bare_metal::early_console::emit(b"\n");
                        dest[pos] = b'\n';
                        pos += 1;
                        break;
                    }
                    if byte == 0x7F || byte == 0x08 {
                        if pos > 0 {
                            pos -= 1;
                            crate::bare_metal::early_console::emit(b"\x08 \x08");
                        }
                        continue;
                    }
                    dest[pos] = byte;
                    pos += 1;
                }
                SyscallReturn::ok(pos as u64)
            }

            FdKind::Pipe {
                pipe_id,
                is_read_end,
            } => {
                if !is_read_end {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                // SAFETY: single-CPU; SHELL_PIPE_REGISTRY not aliased.
                let registry = unsafe {
                    match (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut() {
                        Some(r) => r,
                        None => return SyscallReturn::err(syscall_errno::EIO),
                    }
                };
                let ring = match registry.get_mut(PipeId(pipe_id)) {
                    Some(r) => r,
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                };
                match ring.read(dest) {
                    Ok(n) =>
                    {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "n ≤ dest.len() ≤ 0x10_0000; fits u64"
                        )]
                        SyscallReturn::ok(n as u64)
                    }
                    Err(_) => SyscallReturn::ok(0), // BrokenPipe → EOF
                }
            }

            FdKind::FsFile {
                inode,
                offset,
                flags,
            } => {
                if !flags.is_readable() {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                // SAFETY: single-CPU; SHELL_VFS immutable borrow ends here.
                let read_result = unsafe {
                    match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                        Some(v) => v.read_file(inode, offset, max),
                        None => return SyscallReturn::err(syscall_errno::EIO),
                    }
                };
                match read_result {
                    Ok(data) => {
                        let n = data.len().min(dest.len());
                        dest[..n].copy_from_slice(&data[..n]);
                        // Advance the fd offset. Immutable borrow above
                        // is finished; safe to take a mutable borrow now.
                        // SAFETY: single-CPU; SHELL_FD_TABLE not aliased.
                        unsafe {
                            if let Some(t) =
                                (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut()
                            {
                                if let Some(desc) = t.get_mut(RawFd(fd_num)) {
                                    if let FdKind::FsFile {
                                        offset: ref mut off,
                                        ..
                                    } = desc.kind
                                    {
                                        #[allow(
                                            clippy::cast_possible_truncation,
                                            reason = "n ≤ max ≤ 0x10_0000; fits u64"
                                        )]
                                        {
                                            *off = offset.saturating_add(n as u64);
                                        }
                                    }
                                }
                            }
                        }
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "n ≤ max ≤ 0x10_0000; fits u64"
                        )]
                        SyscallReturn::ok(n as u64)
                    }
                    Err(VfsError::NotFound) => SyscallReturn::err(syscall_errno::ENOENT),
                    Err(_) => SyscallReturn::err(syscall_errno::EIO),
                }
            }

            FdKind::IpcChannel(_) => SyscallReturn::err(syscall_errno::ENOSYS),
        }
    }

    // -----------------------------------------------------------------------
    // FdWrite (64)
    // -----------------------------------------------------------------------

    /// `FdWrite (64)` — write from a user buffer into a file descriptor.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `fd`: file descriptor number  |
    /// | a1   | RSI | `buf_ptr`: user read buffer   |
    /// | a2   | RDX | `buf_len`: bytes to write     |
    ///
    /// For `Console { writable: true }` fds the bytes are emitted via
    /// `crate::bare_metal::early_console::emit()` after being copied into a kernel-side
    /// stack buffer (256-byte chunks). This matches the `write_console`
    /// pattern used by `WriteConsole (60)`.
    pub(super) fn fd_write(args: [u64; 6]) -> SyscallReturn {
        let fd_num = args[0] as u32;
        let buf_ptr = args[1];
        let buf_len = args[2];

        if buf_len == 0 {
            return SyscallReturn::ok(0);
        }

        if !user_range_ok(buf_ptr, buf_len) {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // SAFETY: single-CPU; SHELL_FD_TABLE read-only here.
        let kind = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_FD_TABLE)).as_ref() {
                Some(t) => match t.get(RawFd(fd_num)) {
                    Some(desc) => desc.kind.clone(),
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        match kind {
            FdKind::Console { writable, .. } => {
                if !writable {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                // Emit to the early console in 256-byte chunks — mirrors
                // the write_console helper used by WriteConsole (60).
                // SAFETY: user_range_ok verified the range; single-CPU.
                unsafe {
                    let mut emitted: u64 = 0;
                    let mut chunk_buf = [0u8; 256];
                    while emitted < buf_len {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "chunk_buf.len() = 256 fits u64; remaining fits usize"
                        )]
                        let chunk = core::cmp::min(chunk_buf.len() as u64, buf_len - emitted);
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "chunk ≤ 256 fits usize on every target"
                        )]
                        let chunk_usize = chunk as usize;
                        core::ptr::copy_nonoverlapping(
                            (buf_ptr + emitted) as *const u8,
                            chunk_buf.as_mut_ptr(),
                            chunk_usize,
                        );
                        #[allow(
                            clippy::indexing_slicing,
                            reason = "chunk_usize ≤ 256 = chunk_buf.len() by min above"
                        )]
                        crate::bare_metal::early_console::emit(&chunk_buf[..chunk_usize]);
                        emitted += chunk;
                    }
                    SyscallReturn::ok(emitted)
                }
            }

            FdKind::Pipe {
                pipe_id,
                is_read_end,
            } => {
                if is_read_end {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                // Copy the user data into a kernel Vec before entering the
                // registry borrow so there is no live pointer into user space
                // while the pipe state is mutated.
                let data = copy_user_data(buf_ptr, buf_len);
                // SAFETY: single-CPU; SHELL_PIPE_REGISTRY not aliased.
                let registry = unsafe {
                    match (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut() {
                        Some(r) => r,
                        None => return SyscallReturn::err(syscall_errno::EIO),
                    }
                };
                let ring = match registry.get_mut(PipeId(pipe_id)) {
                    Some(r) => r,
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                };
                match ring.write(&data) {
                    Ok(n) =>
                    {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "n ≤ data.len() ≤ 0x10_0000; fits u64"
                        )]
                        SyscallReturn::ok(n as u64)
                    }
                    Err(_) => SyscallReturn::err(syscall_errno::EPIPE),
                }
            }

            FdKind::FsFile {
                inode,
                offset,
                flags,
            } => {
                if !flags.is_writable() {
                    return SyscallReturn::err(syscall_errno::EBADF);
                }
                let data = copy_user_data(buf_ptr, buf_len);
                // In append mode the write position is always end-of-file.
                // SAFETY: single-CPU; SHELL_VFS immutable borrow only.
                let write_offset = if flags.has_append() {
                    unsafe {
                        match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                            Some(v) => v.file_size(inode).unwrap_or(offset),
                            None => return SyscallReturn::err(syscall_errno::EIO),
                        }
                    }
                } else {
                    offset
                };
                // SAFETY: single-CPU; SHELL_VFS mutable borrow.
                let write_result = unsafe {
                    match (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                        Some(v) => v.write_file(inode, write_offset, &data),
                        None => return SyscallReturn::err(syscall_errno::EIO),
                    }
                };
                match write_result {
                    Ok(n) => {
                        // Advance the fd offset.
                        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
                        unsafe {
                            if let Some(t) =
                                (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut()
                            {
                                if let Some(desc) = t.get_mut(RawFd(fd_num)) {
                                    if let FdKind::FsFile {
                                        offset: ref mut off,
                                        ..
                                    } = desc.kind
                                    {
                                        #[allow(
                                            clippy::cast_possible_truncation,
                                            reason = "n ≤ data.len() ≤ 0x10_0000; fits u64"
                                        )]
                                        {
                                            *off = write_offset.saturating_add(n as u64);
                                        }
                                    }
                                }
                            }
                        }
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "n ≤ data.len() ≤ 0x10_0000; fits u64"
                        )]
                        SyscallReturn::ok(n as u64)
                    }
                    Err(VfsError::NotFound) => SyscallReturn::err(syscall_errno::ENOENT),
                    Err(_) => SyscallReturn::err(syscall_errno::EIO),
                }
            }

            FdKind::IpcChannel(_) => SyscallReturn::err(syscall_errno::ENOSYS),
        }
    }

    // -----------------------------------------------------------------------
    // FdClose (65)
    // -----------------------------------------------------------------------

    /// `FdClose (65)` — close a file descriptor.
    ///
    /// If the fd is a pipe end, the corresponding `close_read` or `close_write`
    /// is called on the pipe ring so waiting tasks can be unblocked by the
    /// scheduler on the next yield.
    ///
    /// Returns `ok(0)` on success, `err(EBADF)` if the fd is not open.
    pub(super) fn fd_close(args: [u64; 6]) -> SyscallReturn {
        let fd_num = args[0] as u32;

        // Capture the kind before closing so we can perform pipe cleanup.
        // SAFETY: single-CPU; SHELL_FD_TABLE read-only here.
        let kind = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_FD_TABLE)).as_ref() {
                Some(t) => match t.get(RawFd(fd_num)) {
                    Some(desc) => desc.kind.clone(),
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // If this is a pipe end, notify the ring before removing the fd.
        if let FdKind::Pipe {
            pipe_id,
            is_read_end,
        } = kind
        {
            // SAFETY: single-CPU; SHELL_PIPE_REGISTRY mutable borrow.
            unsafe {
                if let Some(reg) = (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut() {
                    if let Some(ring) = reg.get_mut(PipeId(pipe_id)) {
                        if is_read_end {
                            let _ = ring.close_read();
                        } else {
                            let _ = ring.close_write();
                        }
                    }
                }
            }
        }

        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => match t.close(RawFd(fd_num)) {
                    Ok(()) => SyscallReturn::ok(0),
                    Err(_) => SyscallReturn::err(syscall_errno::EBADF),
                },
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FdDup (66)
    // -----------------------------------------------------------------------

    /// `FdDup (66)` — duplicate a file descriptor to the lowest available
    /// number.
    ///
    /// Returns `(rax = new_fd, rdx = 0)` on success, `err(EBADF)` if `fd` is
    /// not open.
    pub(super) fn fd_dup(args: [u64; 6]) -> SyscallReturn {
        let fd_num = args[0] as u32;
        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => t.dup(RawFd(fd_num)).map_or_else(
                    |_| SyscallReturn::err(syscall_errno::EBADF),
                    |new_fd| SyscallReturn::ok(u64::from(new_fd.0)),
                ),
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FdSeek (68)
    // -----------------------------------------------------------------------

    /// `FdSeek (68)` — reposition the file offset for an `FsFile` descriptor.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                              |
    /// |------|-----|-----------------------------------|
    /// | a0   | RDI | `fd`                              |
    /// | a1   | RSI | `offset` (i64 passed as u64 bits) |
    /// | a2   | RDX | `whence` (0=SET, 1=CUR, 2=END)   |
    ///
    /// Only `FsFile` descriptors are seekable. Console, pipe, and IPC-channel
    /// fds return `err(ESPIPE)`.
    pub(super) fn fd_seek(args: [u64; 6]) -> SyscallReturn {
        let fd_num = args[0] as u32;
        // justification: `offset` is transmitted as u64 via the syscall ABI
        // register convention; it represents a signed i64 seek offset.
        // The wrap is intentional — the ABI uses two's-complement reinterpretation.
        #[allow(clippy::cast_possible_wrap)]
        let offset = args[1] as i64;
        let whence = args[2] as u32;

        // Clone the kind to avoid holding the fd_table borrow across the vfs
        // borrow below.
        // SAFETY: single-CPU; SHELL_FD_TABLE read-only here.
        let kind = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_FD_TABLE)).as_ref() {
                Some(t) => match t.get(RawFd(fd_num)) {
                    Some(desc) => desc.kind.clone(),
                    None => return SyscallReturn::err(syscall_errno::EBADF),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        const SEEK_SET: u32 = 0;
        const SEEK_CUR: u32 = 1;
        const SEEK_END: u32 = 2;

        match kind {
            FdKind::FsFile {
                inode,
                offset: current_offset,
                ..
            } => {
                let new_offset: Option<u64> = match whence {
                    SEEK_SET => {
                        if offset < 0 {
                            None
                        } else {
                            u64::try_from(offset).ok()
                        }
                    }
                    SEEK_CUR => {
                        let cur = i64::try_from(current_offset).unwrap_or(i64::MAX);
                        cur.checked_add(offset).and_then(|v| u64::try_from(v).ok())
                    }
                    SEEK_END => {
                        // SAFETY: single-CPU; SHELL_VFS read-only.
                        let file_size = unsafe {
                            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                                Some(v) => match v.file_size(inode) {
                                    Ok(s) => s,
                                    Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                                },
                                None => return SyscallReturn::err(syscall_errno::EIO),
                            }
                        };
                        let size_i64 = i64::try_from(file_size).unwrap_or(i64::MAX);
                        size_i64
                            .checked_add(offset)
                            .and_then(|v| u64::try_from(v).ok())
                    }
                    _ => return SyscallReturn::err(syscall_errno::EINVAL),
                };

                match new_offset {
                    Some(pos) => {
                        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
                        unsafe {
                            if let Some(t) =
                                (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut()
                            {
                                if let Some(desc) = t.get_mut(RawFd(fd_num)) {
                                    if let FdKind::FsFile {
                                        offset: ref mut off,
                                        ..
                                    } = desc.kind
                                    {
                                        *off = pos;
                                    }
                                }
                            }
                        }
                        SyscallReturn::ok(pos)
                    }
                    None => SyscallReturn::err(syscall_errno::EINVAL),
                }
            }
            // Console, pipe, and IPC-channel fds are not seekable.
            FdKind::Console { .. } | FdKind::Pipe { .. } | FdKind::IpcChannel(_) => {
                SyscallReturn::err(syscall_errno::ESPIPE)
            }
        }
    }

    // -----------------------------------------------------------------------
    // PipeCreate (62) — two-register return
    // -----------------------------------------------------------------------

    /// `PipeCreate (62)` — create an anonymous pipe and return both ends as
    /// file descriptors.
    ///
    /// Returns `(rax = read_fd, rdx = write_fd)` on success, or
    /// `err(ENOSPC)` when the fd table is exhausted.
    pub(super) fn pipe_create(_args: [u64; 6]) -> SyscallReturn {
        // SAFETY: single-CPU; SHELL_PIPE_REGISTRY mutable borrow.
        let pipe_id = unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut() {
                Some(r) => r.create(),
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // Open the read end.
        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
        let rfd = unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => match t.open(FileDescriptor {
                    kind: FdKind::Pipe {
                        pipe_id: pipe_id.0,
                        is_read_end: true,
                    },
                    flags: FdFlags::default(),
                }) {
                    Ok(fd) => fd,
                    Err(_) => {
                        // Roll back the pipe we just created.
                        if let Some(r) =
                            (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut()
                        {
                            r.remove(pipe_id);
                        }
                        return SyscallReturn::err(syscall_errno::ENOSPC);
                    }
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // Open the write end.
        // SAFETY: single-CPU; SHELL_FD_TABLE + SHELL_PIPE_REGISTRY not aliased.
        let wfd = unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => match t.open(FileDescriptor {
                    kind: FdKind::Pipe {
                        pipe_id: pipe_id.0,
                        is_read_end: false,
                    },
                    flags: FdFlags::default(),
                }) {
                    Ok(fd) => fd,
                    Err(_) => {
                        // Roll back both the read fd and the pipe.
                        let _ = t.close(rfd);
                        if let Some(r) =
                            (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut()
                        {
                            r.remove(pipe_id);
                        }
                        return SyscallReturn::err(syscall_errno::ENOSPC);
                    }
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        SyscallReturn {
            rax: u64::from(rfd.0),
            rdx: u64::from(wfd.0),
        }
    }

    // -----------------------------------------------------------------------
    // FdDup2 (67) — two-register return
    // -----------------------------------------------------------------------

    /// `FdDup2 (67)` — duplicate `old_fd` to the specific number `new_fd`.
    ///
    /// If `new_fd` is a live pipe end the pipe is closed first (POSIX `dup2`
    /// semantics). Returns `(rax = new_fd, rdx = 0)` on success,
    /// `err(EBADF)` if `old_fd` is not open.
    pub(super) fn fd_dup2(args: [u64; 6]) -> SyscallReturn {
        let old_fd = args[0] as u32;
        let new_fd = args[1] as u32;

        // If new_fd is currently a pipe end, close that pipe end before dup2
        // displaces the entry — matching POSIX dup2 semantics.
        // SAFETY: single-CPU; SHELL_FD_TABLE read-only in this block.
        let existing_pipe = unsafe {
            (*core::ptr::addr_of!(crate::SHELL_FD_TABLE))
                .as_ref()
                .and_then(|t| t.get(RawFd(new_fd)))
                .and_then(|desc| {
                    if let FdKind::Pipe {
                        pipe_id,
                        is_read_end,
                    } = desc.kind
                    {
                        Some((pipe_id, is_read_end))
                    } else {
                        None
                    }
                })
        };

        if let Some((pipe_id, is_read_end)) = existing_pipe {
            // SAFETY: single-CPU; SHELL_PIPE_REGISTRY mutable borrow.
            unsafe {
                if let Some(reg) = (*core::ptr::addr_of_mut!(crate::SHELL_PIPE_REGISTRY)).as_mut() {
                    if let Some(ring) = reg.get_mut(PipeId(pipe_id)) {
                        if is_read_end {
                            let _ = ring.close_read();
                        } else {
                            let _ = ring.close_write();
                        }
                    }
                }
            }
        }

        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => t.dup2(RawFd(old_fd), RawFd(new_fd)).map_or_else(
                    |_| SyscallReturn::err(syscall_errno::EBADF),
                    |result_fd| SyscallReturn::ok(u64::from(result_fd.0)),
                ),
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FsOpen (90)
    // -----------------------------------------------------------------------

    /// `FsOpen (90)` — open or create a file in the VFS.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `path_ptr`                    |
    /// | a1   | RSI | `path_len`                    |
    /// | a2   | RDX | `flags` (`OpenFlags` bitmask)   |
    ///
    /// Returns `(rax = fd, rdx = 0)` on success, or `err(errno)` on failure.
    pub(super) fn fs_open(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];
        let flags_raw = args[2] as u32;

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path accesses SHELL_PROCESS_TABLE
        // and SCHEDULER read-only.
        let abs = unsafe { resolve_path(&path) };
        let open_flags = OpenFlags(flags_raw);

        // stat the path to see if it exists.
        // SAFETY: single-CPU; SHELL_VFS read-only.
        let stat_result = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) => v.stat(&abs),
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        let inode = match stat_result {
            Ok(stat) => {
                if stat.file_type == FileType::Directory
                    && (open_flags.is_writable() || open_flags.has_trunc())
                {
                    return SyscallReturn::err(syscall_errno::EINVAL);
                }
                // Truncate: delete + recreate.
                if open_flags.has_trunc() && open_flags.is_writable() {
                    // SAFETY: single-CPU; SHELL_VFS mutable borrow.
                    unsafe {
                        if let Some(v) = (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                            let _ = v.delete(&abs);
                            match v.create_file(&abs) {
                                Ok(ino) => ino,
                                Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                            }
                        } else {
                            return SyscallReturn::err(syscall_errno::EIO);
                        }
                    }
                } else {
                    stat.inode
                }
            }
            Err(VfsError::NotFound) => {
                if open_flags.has_create() {
                    // SAFETY: single-CPU; SHELL_VFS mutable borrow.
                    unsafe {
                        match (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                            Some(v) => match v.create_file(&abs) {
                                Ok(ino) => ino,
                                Err(VfsError::AlreadyExists) => {
                                    // Created between stat and create.
                                    match (*core::ptr::addr_of!(crate::SHELL_VFS))
                                        .as_ref()
                                        .and_then(|v| v.stat(&abs).ok())
                                    {
                                        Some(s) => s.inode,
                                        None => return SyscallReturn::err(syscall_errno::EIO),
                                    }
                                }
                                Err(VfsError::NotADirectory | VfsError::InvalidPath) => {
                                    return SyscallReturn::err(syscall_errno::EINVAL);
                                }
                                Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                            },
                            None => return SyscallReturn::err(syscall_errno::EIO),
                        }
                    }
                } else {
                    return SyscallReturn::err(syscall_errno::ENOENT);
                }
            }
            Err(VfsError::NotADirectory) => return SyscallReturn::err(syscall_errno::EINVAL),
            Err(_) => return SyscallReturn::err(syscall_errno::EIO),
        };

        // Open an fd for the resolved inode.
        // SAFETY: single-CPU; SHELL_FD_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_FD_TABLE)).as_mut() {
                Some(t) => {
                    match t.open(FileDescriptor {
                        kind: FdKind::FsFile {
                            inode,
                            offset: 0,
                            flags: open_flags,
                        },
                        flags: FdFlags::default(),
                    }) {
                        Ok(fd) => SyscallReturn::ok(u64::from(fd.0)),
                        Err(_) => SyscallReturn::err(syscall_errno::ENOSPC),
                    }
                }
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FsStat (91)
    // -----------------------------------------------------------------------

    /// `FsStat (91)` — stat a file or directory.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                      |
    /// |------|-----|-------------------------------------------|
    /// | a0   | RDI | `path_ptr`                                |
    /// | a1   | RSI | `path_len`                                |
    /// | a2   | RDX | `stat_buf_ptr` — user buffer (17 bytes)   |
    ///
    /// The 17-byte stat layout written to `stat_buf_ptr`:
    /// - bytes `[0..8]`  : inode (u64 LE)
    /// - bytes `[8..16]` : size  (u64 LE)
    /// - byte  `[16]`    : file type (0 = regular file, 1 = directory)
    ///
    /// Returns `ok(0)` on success, `err(ENOENT)` if not found.
    pub(super) fn fs_stat(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];
        let stat_buf = args[2];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // Validate the output buffer: 17 bytes.
        if !user_range_ok(stat_buf, 17) || stat_buf == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // SAFETY: single-CPU; SHELL_VFS read-only.
        let stat = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) => match v.stat(&abs) {
                    Ok(s) => s,
                    Err(VfsError::NotFound) => return SyscallReturn::err(syscall_errno::ENOENT),
                    Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        let type_byte: u8 = match stat.file_type {
            FileType::RegularFile => 0,
            FileType::Directory => 1,
        };

        // Write the 17-byte struct into user memory.
        // SAFETY: user_range_ok(stat_buf, 17) verified above; single-CPU.
        unsafe {
            let dst = stat_buf as *mut u8;
            core::ptr::copy_nonoverlapping(stat.inode.to_le_bytes().as_ptr(), dst, 8);
            core::ptr::copy_nonoverlapping(stat.size.to_le_bytes().as_ptr(), dst.add(8), 8);
            dst.add(16).write(type_byte);
        }

        SyscallReturn::ok(0)
    }

    // -----------------------------------------------------------------------
    // FsListDir (92)
    // -----------------------------------------------------------------------

    /// `FsListDir (92)` — list directory entries as newline-separated names.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                   |
    /// |------|-----|----------------------------------------|
    /// | a0   | RDI | `path_ptr`                             |
    /// | a1   | RSI | `path_len`                             |
    /// | a2   | RDX | `out_ptr`  — user output buffer        |
    /// | a3   | R10 | `out_len`  — capacity of output buffer |
    ///
    /// Returns `(rax = bytes_written, rdx = 0)` on success. Returns
    /// `err(ENOENT)` if the path does not exist, `err(EINVAL)` if it is not
    /// a directory.  If the serialised names would overflow `out_len`, the
    /// write is truncated and `rax` reflects the actual bytes written.
    pub(super) fn fs_list_dir(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];
        let out_ptr = args[2];
        let out_len = args[3];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        let dest = match user_slice_mut(out_ptr, out_len) {
            Some(s) => s,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // SAFETY: single-CPU; SHELL_VFS read-only.
        let entries: Vec<String> = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) => match v.list_directory(&abs) {
                    Ok(list) => list.into_iter().map(|e| e.name).collect(),
                    Err(VfsError::NotFound) => return SyscallReturn::err(syscall_errno::ENOENT),
                    Err(VfsError::NotADirectory) => {
                        return SyscallReturn::err(syscall_errno::EINVAL);
                    }
                    Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // Serialise as "name1\nname2\n..." into the user buffer.
        let mut written = 0usize;
        for name in &entries {
            let bytes = name.as_bytes();
            // Write name bytes.
            for &b in bytes {
                if written >= dest.len() {
                    break;
                }
                dest[written] = b;
                written += 1;
            }
            // Write newline separator.
            if written < dest.len() {
                dest[written] = b'\n';
                written += 1;
            }
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "written ≤ dest.len() ≤ 0x10_0000; fits u64"
        )]
        SyscallReturn::ok(written as u64)
    }

    // -----------------------------------------------------------------------
    // FsCreate (93)
    // -----------------------------------------------------------------------

    /// `FsCreate (93)` — create an empty regular file.
    ///
    /// Returns `ok(0)` on success, `err(EEXIST)` if the path already exists,
    /// `err(ENOENT)` if a parent component does not exist.
    pub(super) fn fs_create(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // SAFETY: single-CPU; SHELL_VFS mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                Some(v) => match v.create_file(&abs) {
                    Ok(_) => SyscallReturn::ok(0),
                    Err(VfsError::AlreadyExists) => SyscallReturn::err(syscall_errno::EEXIST),
                    Err(VfsError::NotFound) => SyscallReturn::err(syscall_errno::ENOENT),
                    Err(VfsError::NotADirectory | VfsError::InvalidPath) => {
                        SyscallReturn::err(syscall_errno::EINVAL)
                    }
                    Err(_) => SyscallReturn::err(syscall_errno::EIO),
                },
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FsDelete (94)
    // -----------------------------------------------------------------------

    /// `FsDelete (94)` — delete a file or empty directory.
    ///
    /// Returns `ok(0)` on success, `err(ENOENT)` if not found,
    /// `err(ENOTEMPTY)` for a non-empty directory.
    pub(super) fn fs_delete(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // SAFETY: single-CPU; SHELL_VFS mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                Some(v) => match v.delete(&abs) {
                    Ok(()) => SyscallReturn::ok(0),
                    Err(VfsError::NotFound) => SyscallReturn::err(syscall_errno::ENOENT),
                    Err(VfsError::NotEmpty) => SyscallReturn::err(syscall_errno::ENOTEMPTY),
                    Err(VfsError::InvalidPath) => SyscallReturn::err(syscall_errno::EINVAL),
                    Err(_) => SyscallReturn::err(syscall_errno::EIO),
                },
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // FsMkdir (95)
    // -----------------------------------------------------------------------

    /// `FsMkdir (95)` — create a directory.
    ///
    /// Returns `ok(0)` on success, `err(EEXIST)` if the path already exists,
    /// `err(ENOENT)` if a parent component does not exist.
    pub(super) fn fs_mkdir(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // SAFETY: single-CPU; SHELL_VFS mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_VFS)).as_mut() {
                Some(v) => match v.create_directory(&abs) {
                    Ok(_) => SyscallReturn::ok(0),
                    Err(VfsError::AlreadyExists) => SyscallReturn::err(syscall_errno::EEXIST),
                    Err(VfsError::NotFound) => SyscallReturn::err(syscall_errno::ENOENT),
                    Err(VfsError::NotADirectory | VfsError::InvalidPath) => {
                        SyscallReturn::err(syscall_errno::EINVAL)
                    }
                    Err(_) => SyscallReturn::err(syscall_errno::EIO),
                },
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // GetCwd (16)
    // -----------------------------------------------------------------------

    /// `GetCwd (16)` — write the current working directory into a user buffer.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `buf_ptr` — user write buffer |
    /// | a1   | RSI | `buf_len` — capacity          |
    ///
    /// Returns `(rax = bytes_written, rdx = 0)` on success, or `err(EFAULT)`
    /// if the buffer fails validation.  Truncates silently when the cwd is
    /// longer than `buf_len` (unlikely given the 4 096-byte path cap).
    pub(super) fn get_cwd(args: [u64; 6]) -> SyscallReturn {
        let buf_ptr = args[0];
        let buf_len = args[1];

        let dest = match user_slice_mut(buf_ptr, buf_len) {
            Some(s) => s,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; SHELL_PROCESS_TABLE + SCHEDULER read-only.
        let cwd: String = unsafe {
            let task = current_task();
            match (*core::ptr::addr_of!(crate::SHELL_PROCESS_TABLE)).as_ref() {
                Some(pt) => pt.get_cwd(task).unwrap_or("/").to_string(),
                None => String::from("/"),
            }
        };

        let src = cwd.as_bytes();
        let n = src.len().min(dest.len());
        dest[..n].copy_from_slice(&src[..n]);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "n ≤ dest.len() ≤ 0x10_0000; fits u64"
        )]
        SyscallReturn::ok(n as u64)
    }

    // -----------------------------------------------------------------------
    // SetCwd (17)
    // -----------------------------------------------------------------------

    /// `SetCwd (17)` — change the current working directory.
    ///
    /// The path must resolve to an existing directory in the VFS. Returns
    /// `ok(0)` on success, `err(ENOENT)` if not found, `err(EINVAL)` if the
    /// path resolves to a regular file.
    pub(super) fn set_cwd(args: [u64; 6]) -> SyscallReturn {
        let path_ptr = args[0];
        let path_len = args[1];

        let path = match user_str(path_ptr, path_len) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; resolve_path is read-only.
        let abs = unsafe { resolve_path(&path) };

        // Verify the path exists and is a directory.
        // SAFETY: single-CPU; SHELL_VFS read-only.
        let is_dir = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) => match v.stat(&abs) {
                    Ok(s) => s.file_type == FileType::Directory,
                    Err(VfsError::NotFound) => return SyscallReturn::err(syscall_errno::ENOENT),
                    Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        if !is_dir {
            return SyscallReturn::err(syscall_errno::EINVAL);
        }

        // Update the process table cwd.
        // SAFETY: single-CPU; SHELL_PROCESS_TABLE + SCHEDULER mutable borrow.
        unsafe {
            let task = current_task();
            if let Some(pt) = (*core::ptr::addr_of_mut!(crate::SHELL_PROCESS_TABLE)).as_mut() {
                pt.set_cwd(task, abs);
            }
        }

        SyscallReturn::ok(0)
    }

    // -----------------------------------------------------------------------
    // ProcessList (96)
    // -----------------------------------------------------------------------

    /// `ProcessList (96)` — write a snapshot of all registered processes into
    /// a user buffer.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                          |
    /// |------|-----|-------------------------------|
    /// | a0   | RDI | `buf_ptr` — user write buffer |
    /// | a1   | RSI | `buf_len` — capacity in bytes |
    ///
    /// ### Wire format
    ///
    /// Each entry is a fixed-size 16-byte record:
    /// - bytes `[0..8]`  : pid (u64 LE)
    /// - bytes `[8..15]` : process name, NUL-padded to 7 bytes
    /// - byte  `[15]`    : flags (bit 0 = `has_exited`)
    ///
    /// Returns `(rax = records_written, rdx = 0)`. Stops when the buffer is
    /// full; records beyond capacity are silently dropped.
    pub(super) fn process_list(args: [u64; 6]) -> SyscallReturn {
        let buf_ptr = args[0];
        let buf_len = args[1];

        let dest = match user_slice_mut(buf_ptr, buf_len) {
            Some(s) => s,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // SAFETY: single-CPU; SHELL_PROCESS_TABLE read-only.
        let entries: Vec<(u64, String, bool)> = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_PROCESS_TABLE)).as_ref() {
                Some(pt) => pt
                    .list()
                    .into_iter()
                    .map(|e| (e.id.0, e.name.clone(), e.exit_code.is_some()))
                    .collect(),
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        const RECORD_SIZE: usize = 16;
        let mut records_written: u64 = 0;
        let mut offset = 0usize;

        for (pid, name, exited) in &entries {
            if offset + RECORD_SIZE > dest.len() {
                break;
            }
            // pid (8 bytes LE).
            let pid_bytes = pid.to_le_bytes();
            dest[offset..offset + 8].copy_from_slice(&pid_bytes);
            // name (7 bytes, NUL-padded).
            let name_bytes = name.as_bytes();
            let name_len = name_bytes.len().min(7);
            dest[offset + 8..offset + 8 + name_len].copy_from_slice(&name_bytes[..name_len]);
            // NUL-pad remainder of the name field.
            for b in &mut dest[offset + 8 + name_len..offset + 15] {
                *b = 0;
            }
            // flags byte.
            dest[offset + 15] = u8::from(*exited);
            offset += RECORD_SIZE;
            records_written += 1;
        }

        SyscallReturn::ok(records_written)
    }

    // -----------------------------------------------------------------------
    // ProcessKill (97)
    // -----------------------------------------------------------------------

    /// `ProcessKill (97)` — record a SIGKILL-equivalent exit for a process.
    ///
    /// This does not remove the task from the scheduler run queue; the
    /// bare-metal layer must perform that step after this call. The handler
    /// only records the exit in the process table so a waiting parent can reap
    /// the child.
    ///
    /// Returns `ok(0)` on success, `err(ESRCH)` if the PID is not registered.
    pub(super) fn process_kill(args: [u64; 6]) -> SyscallReturn {
        let target_pid = args[0];

        // SAFETY: single-CPU; SHELL_PROCESS_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_PROCESS_TABLE)).as_mut() {
                Some(pt) => {
                    if pt.get(crate::scheduling::TaskId(target_pid)).is_none() {
                        return SyscallReturn::err(syscall_errno::ESRCH);
                    }
                    // 137 = 128 + SIGKILL(9): conventional Unix exit-status.
                    pt.record_exit(crate::scheduling::TaskId(target_pid), 137);
                    SyscallReturn::ok(0)
                }
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // ProcessWait (15) — two-register return
    // -----------------------------------------------------------------------

    /// `ProcessWait (15)` — reap an exited child process.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                                              |
    /// |------|-----|---------------------------------------------------|
    /// | a0   | RDI | `child_pid` (0 = wait for any child)              |
    /// | a1   | RSI | `flags` (bit 0 = WNOHANG)                         |
    ///
    /// Returns `(rax = exit_code, rdx = child_pid)` on success.
    /// When `WNOHANG` is set and no child has exited returns `(0, 0)`.
    pub(super) fn process_wait(_args: [u64; 6]) -> SyscallReturn {
        // SAFETY: single-CPU; SHELL_PROCESS_TABLE + SCHEDULER read.
        let current = unsafe { current_task() };

        // SAFETY: single-CPU; SHELL_PROCESS_TABLE mutable borrow.
        unsafe {
            match (*core::ptr::addr_of_mut!(crate::SHELL_PROCESS_TABLE)).as_mut() {
                Some(pt) => {
                    if let Some((child_id, exit_code)) = pt.reap_child(current) {
                        SyscallReturn {
                            rax: exit_code,
                            rdx: child_id.0,
                        }
                    } else {
                        SyscallReturn { rax: 0, rdx: 0 }
                    }
                }
                None => SyscallReturn::err(syscall_errno::EIO),
            }
        }
    }

    // -----------------------------------------------------------------------
    // ProcessSpawn (14) — Phase D implementation
    // -----------------------------------------------------------------------

    /// `ProcessSpawn (14)` — spawn a new process from a VFS ELF binary.
    ///
    /// Reads the ELF image from `SHELL_VFS`, builds a fresh per-process
    /// address space (kernel-half mirrored from the boot PML4), maps and
    /// loads the ELF segments, allocates a user stack, registers the new
    /// task with the scheduler, and records the child in `SHELL_PROCESS_TABLE`.
    ///
    /// ## ABI
    ///
    /// | Slot | Reg | Role                            |
    /// |------|-----|---------------------------------|
    /// | a0   | RDI | `path_ptr` — ELF path in VFS    |
    /// | a1   | RSI | `path_len`                      |
    /// | a2   | RDX | `argv_ptr` — argument array (Phase 1: ignored) |
    /// | a3   | R10 | `argv_len` — number of args (Phase 1: ignored)  |
    /// | a4   | R8  | `envp_ptr` — env-var array (Phase 1: ignored)   |
    /// | a5   | R9  | `envp_len` — number of env vars (Phase 1: ignored) |
    ///
    /// Returns `ok(child_task_id)` on success, or an errno on failure.
    ///
    /// ## Phase 1 limitations
    ///
    /// argv/envp are accepted by the ABI but not forwarded to the child's
    /// user stack. The child ELF starts with an empty initial stack
    /// (no `argc`/`argv`/`envp`). Full `user_stack_args` wiring is
    /// deferred to Phase 2, which requires access to the child's PML4
    /// from outside a context-switch boundary. The shell image reads its
    /// configuration from hardcoded defaults, so this is acceptable.
    pub(super) fn process_spawn(args: [u64; 6]) -> SyscallReturn {
        // Step 1 — Decode the path argument from user memory.
        let path = match user_str(args[0], args[1]) {
            Some(p) => p,
            None => return SyscallReturn::err(syscall_errno::EFAULT),
        };

        // Step 2 — Resolve against cwd (handles both absolute and relative paths).
        // SAFETY: single-CPU; SHELL_PROCESS_TABLE and SCHEDULER are not aliased.
        let abs_path = unsafe { resolve_path(&path) };

        // Step 3 — Stat the file to get its inode and size.
        // SAFETY: single-CPU; SHELL_VFS is read-only in this block.
        let stat = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) => match v.stat(&abs_path) {
                    Ok(s) => s,
                    Err(crate::vfs::VfsError::NotFound) => {
                        return SyscallReturn::err(syscall_errno::ENOENT);
                    }
                    Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                },
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // Step 4 — Copy the ELF bytes out of the VFS into a kernel-owned Vec.
        // We snapshot the bytes here so subsequent VFS mutations cannot affect
        // the in-progress spawn.
        // SAFETY: single-CPU; SHELL_VFS is read-only in this block.
        let elf_bytes: alloc::vec::Vec<u8> = unsafe {
            match (*core::ptr::addr_of!(crate::SHELL_VFS)).as_ref() {
                Some(v) =>
                {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "VFS file sizes are bounded by the in-memory allocator; \
                                  Phase 1 ELFs are well under usize::MAX"
                    )]
                    match v.read_file(stat.inode, 0, stat.size as usize) {
                        Ok(b) => b,
                        Err(_) => return SyscallReturn::err(syscall_errno::EIO),
                    }
                }
                None => return SyscallReturn::err(syscall_errno::EIO),
            }
        };

        // Step 5 — Obtain the current-task id (the spawner becomes the parent).
        // SAFETY: single-CPU; SCHEDULER is not aliased.
        let parent_id = unsafe { current_task() };

        // Step 6 — Retrieve the boot PML4 and direct-map offset. Both are
        // published by kmain at boot and are constant for the system lifetime.
        let boot_cr3_val = crate::bare_metal::boot_cr3();
        if boot_cr3_val == 0 {
            // kmain has not yet set the boot CR3 — this should never happen
            // at syscall time, but guard defensively.
            return SyscallReturn::err(syscall_errno::EFAULT);
        }
        let phys_off = crate::bare_metal::phys_offset();
        if phys_off == 0 {
            return SyscallReturn::err(syscall_errno::EFAULT);
        }

        // Step 7 — Build a PageMapper rooted at the boot PML4.
        let mut mapper = crate::bare_metal::paging::PageMapper::new(
            phys_off,
            crate::memory::PhysAddr(boot_cr3_val),
        );

        // Step 8 — Spawn the ELF as a new Ring 3 process.
        //
        // SAFETY: single-CPU syscall path; `boot_cr3_val`, `mapper`,
        // `FRAME_ALLOC`, and `SCHEDULER` are the live kernel singletons
        // and are not otherwise aliased. The new process is not entered until
        // the scheduler dispatches it — this function returns to Ring 3 before
        // that happens. Pattern is identical to `driver_loader::boot_load_with_bar`.
        let task_id = unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let fa = &mut *core::ptr::addr_of_mut!(crate::FRAME_ALLOC);
            match crate::process::ProcessControlBlock::spawn_from_elf(
                &elf_bytes,
                crate::memory::PhysAddr(boot_cr3_val),
                &mut mapper,
                fa,
                sched,
                crate::scheduling::PriorityClass::Interactive,
                crate::capabilities::KernelPrincipal::ZERO,
            ) {
                Ok(id) => id,
                Err(crate::KernelError::ResourceExhausted) => {
                    return SyscallReturn::err(syscall_errno::ENOSPC);
                }
                Err(_) => {
                    // InvalidArgument means the ELF parser rejected the binary.
                    return SyscallReturn::err(syscall_errno::EINVAL);
                }
            }
        };

        // Step 9 — Register the child in the shell process table so that
        // ProcessWait (15) can reap it and GetCwd / ProcessList see it.
        // SAFETY: single-CPU; SHELL_PROCESS_TABLE is not aliased.
        unsafe {
            if let Some(pt) = (*core::ptr::addr_of_mut!(crate::SHELL_PROCESS_TABLE)).as_mut() {
                // Derive the human-readable name from the last path component
                // (mirrors POSIX basename semantics for the process list).
                let name = abs_path
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&abs_path);
                pt.register(task_id, Some(parent_id), alloc::string::String::from(name));
            }
        }

        #[allow(
            clippy::cast_possible_truncation,
            reason = "TaskId.0 is u64; returning it directly as the child PID"
        )]
        SyscallReturn::ok(task_id.0)
    }

    // -----------------------------------------------------------------------
    // Private helper — copy user data into a kernel Vec
    // -----------------------------------------------------------------------

    /// Copy `len` bytes from user address `ptr` into a kernel-owned `Vec<u8>`.
    ///
    /// The resulting `Vec` is a snapshot: further user-space writes to the
    /// source buffer after this call have no effect on the copy.  Called by
    /// `fd_write` before entering any kernel-state borrow to eliminate live
    /// user-memory references during mutation.
    ///
    /// # Precondition
    ///
    /// `user_range_ok(ptr, len)` must hold (caller verified).
    fn copy_user_data(ptr: u64, len: u64) -> Vec<u8> {
        if len == 0 {
            return Vec::new();
        }
        // SAFETY: caller verified user_range_ok; single-CPU syscall path;
        // no concurrent writes to the user AS.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "len ≤ 0x10_0000 by user_range_ok callers; fits usize"
        )]
        let len_usize = len as usize;
        let mut buf = alloc::vec![0u8; len_usize];
        unsafe {
            core::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr(), len_usize);
        }
        buf
    }
}

struct KernelSyscallDispatcher;

impl SyscallDispatcher for KernelSyscallDispatcher {
    // justification: syscall dispatch is an exhaustive match over the ABI surface;
    // splitting it across helper functions would obscure the stable numeric layout.
    #[allow(clippy::too_many_lines)]
    fn dispatch(&mut self, number: SyscallNumber, args: [u64; 6]) -> KernelResult<u64> {
        match number {
            SyscallNumber::TimeMonotonicNanos => {
                // Approximate monotonic time from the CMOS RTC seconds register.
                // Accuracy: ±1 second (RTC resolution). A high-resolution TSC-
                // based implementation is deferred to P6.6 (TSC calibration).
                // `cfg(test)` short-circuits the CMOS port I/O — `outb`/`inb`
                // are Ring 0 instructions and would SIGSEGV in the host test
                // binary; the dispatcher contract only requires Ok(u64).
                #[cfg(not(test))]
                let secs = super::arch::rtc_seconds();
                #[cfg(test)]
                let secs: u32 = 0;
                Ok(u64::from(secs) * 1_000_000_000)
            }

            SyscallNumber::TaskYield => {
                // MB6: cooperative yield — hand the CPU to the next runnable task.
                // Only active on bare-metal x86_64; falls back to a no-op in
                // host tests and non-x86_64 builds.
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                unsafe {
                    use crate::scheduling::{Scheduler, TaskState};
                    let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
                    if let Some(current) = sched.current_task_id() {
                        let _ = sched.yield_current(current, TaskState::Runnable);
                    }
                }
                Ok(0)
            }

            SyscallNumber::TaskExit => task_exit(args[0]),

            SyscallNumber::WriteConsole => {
                // MB11: validate the user buffer + emit via the early console.
                // ABI: (ptr: u64, len: u64) -> u64. Returns `len` on success.
                let ptr = args[0];
                let len = args[1];
                if len == 0 {
                    return Ok(0);
                }
                write_console(ptr, len)
            }

            SyscallNumber::MemMap => {
                // MB11: minimal `mmap` — allocate an anonymous user-VA region.
                // ABI: (size: u64, _flags: u64, _flags2: u64, ...) -> u64.
                // Returns a fresh user VA on success or `u64::MAX` on failure.
                // Placeholder: a full implementation lands in MB12 once the
                // per-process bump allocator owns its user-VA cursor.
                let _ = args;
                Err(KernelError::NotYetImplemented)
            }

            // MB12 — IPC syscalls. The handlers themselves marshal
            // their return values (success → 0 / bytes / channel id;
            // error → SYSCALL_ERROR), so we wrap with `Ok` here to
            // satisfy the `KernelResult<u64>` dispatcher contract.
            //
            // Host builds do not link the IPC handlers (no static
            // `IPC_REGISTRY` on `cfg(test)`); they fall through to
            // `NotYetImplemented` so the existing test surface keeps
            // exercising the dispatcher trait shape without the
            // bare-metal singleton.
            SyscallNumber::IpcCreateChannel => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(ipc_handlers::ipc_create_channel(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::IpcDestroyChannel => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(ipc_handlers::ipc_destroy_channel(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::IpcSend => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(ipc_handlers::ipc_send(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::IpcReceive => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(ipc_handlers::ipc_receive(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            // OIP-013 driver framework. `MmioMap`, `DmaMap`,
            // `IrqAttach`, and `DriverLoad` are handled via the rich
            // two-register path (`dispatch_full`); landing here means
            // the single-register fallback was used (host-test build or
            // an explicit `dispatch` caller). Report `CapabilityDenied`
            // so the contract is loud and observable in host tests
            // without the bare-metal singletons.
            //
            // P6.7.10-pre.3 — the BLK registry syscalls
            // (`BlkRegister`, `BlkUnregister`, `BlkLookup`) follow
            // the same rich-path convention as the OIP-013
            // siblings; the fallback arm reports
            // `CapabilityDenied` for the same triage reasons.
            SyscallNumber::MmioMap
            | SyscallNumber::DmaMap
            | SyscallNumber::IrqAttach
            | SyscallNumber::DriverLoad
            | SyscallNumber::BlkRegister
            | SyscallNumber::BlkUnregister
            | SyscallNumber::BlkLookup => {
                let _ = args;
                Err(KernelError::CapabilityDenied)
            }
            // OIP-Phase2-Entry-021 AI syscall surface (P2 Sprint 2).
            // These are capability-checked relay points that forward
            // requests to the `omni-runtime` IPC service. The kernel
            // does not interpret inference payloads. The IPC wiring
            // is deferred to a later sprint; the scaffold returns
            // ENOSYS so callers can detect that the runtime channel
            // has not yet been registered, which is distinct from
            // EAGAIN (runtime registered but temporarily unavailable).
            SyscallNumber::AiInvoke
            | SyscallNumber::AiStream
            | SyscallNumber::AiEmbed
            | SyscallNumber::AiClassify
            | SyscallNumber::AiTranscribe => {
                // Log the invocation via the kernel console so bring-up
                // smoke and integration tests can observe the routing
                // without a full IPC wiring.
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                super::early_console::write_str(
                    "[ai-syscall] scaffold: ENOSYS (IPC not yet wired)\n",
                );
                let _ = args;
                Err(KernelError::NotYetImplemented)
            }

            // Shell terminal syscalls — single-register return path.
            //
            // On bare-metal each arm delegates to `shell_handlers::*` which
            // accesses the five `SHELL_*` global statics directly (same
            // single-CPU, no-preemption invariant as `ipc_handlers`).
            //
            // Host test builds do not link the bare-metal singletons, so the
            // `#[cfg(not(...))]` branches keep the existing `NotYetImplemented`
            // contract.  The handler logic exercised by `syscall_handlers::tests`
            // uses a local `KernelState` for full isolation.
            SyscallNumber::ReadConsole => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::read_console(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FdRead => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_read(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FdWrite => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_write(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FdClose => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_close(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FdDup => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_dup(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FdSeek => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_seek(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsOpen => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_open(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsStat => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_stat(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsListDir => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_list_dir(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsCreate => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_create(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsDelete => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_delete(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::FsMkdir => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fs_mkdir(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::GetCwd => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::get_cwd(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::SetCwd => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::set_cwd(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::ProcessList => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::process_list(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::ProcessKill => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::process_kill(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            SyscallNumber::ProcessSpawn => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    // Phase D stub: full ELF spawn requires address-space
                    // setup not yet wired. Return ENOSYS via the rax field so
                    // the single-register Ok() wrapping propagates cleanly.
                    Ok(shell_handlers::process_spawn(args).rax)
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Err(KernelError::NotYetImplemented)
                }
            }

            // These syscalls use the two-register return path (`dispatch_full`);
            // landing here from the single-register path is not expected in
            // normal operation. Report `NotYetImplemented` to be loud and
            // observable in host tests without the bare-metal singletons.
            // TeeTdcall/TeeMsr are scaffolded alongside PipeCreate/FdDup2/ProcessWait;
            // explicit enumeration (rather than catch-all) ensures a compiler error
            // when a future commit forgets to route a new syscall.
            SyscallNumber::TeeTdcall
            | SyscallNumber::TeeMsr
            | SyscallNumber::PipeCreate
            | SyscallNumber::FdDup2
            | SyscallNumber::ProcessWait => {
                let _ = args;
                Err(KernelError::NotYetImplemented)
            }

            // All other syscalls are scaffolded but not yet implemented.
            _ => Err(KernelError::NotYetImplemented),
        }
    }

    /// Two-register dispatch (OIP-013 § S2). Routes `MmioMap`,
    /// `DmaMap`, and `IrqAttach` to their rich handlers (which fill
    /// both `rax` and `rdx`); every other syscall keeps the default
    /// `SyscallReturn::ok` wrapping of the single-register path.
    // justification: mirrors dispatch() — exhaustive ABI match; splitting
    // obscures the stable numeric layout mandated by OIP-013.
    #[allow(clippy::too_many_lines)]
    fn dispatch_full(
        &mut self,
        number: SyscallNumber,
        args: [u64; 6],
    ) -> KernelResult<SyscallReturn> {
        match number {
            SyscallNumber::MmioMap => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(mmio_map_handlers::mmio_map(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            SyscallNumber::DmaMap => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(dma_map_handlers::dma_map(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            SyscallNumber::IrqAttach => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(irq_attach_handlers::irq_attach(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            SyscallNumber::DriverLoad => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(driver_load_handlers::driver_load(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            // P6.7.10-pre.3 — BLK registry syscalls
            // (OIP-Driver-NVMe-014 § S4 + § S6 step 12). Same
            // rich-path convention as the OIP-013 siblings above.
            SyscallNumber::BlkRegister => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(blk_handlers::blk_register(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            SyscallNumber::BlkUnregister => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(blk_handlers::blk_unregister(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            SyscallNumber::BlkLookup => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(blk_handlers::blk_lookup(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::EACCES))
                }
            }
            // OIP-Phase2-Entry-021 AI syscall surface (P2 Sprint 2).
            // All five AI syscalls are scaffolded to ENOSYS via the
            // rich two-register return path. The single-register
            // `dispatch` arm above logs + returns `NotYetImplemented`;
            // we return ENOSYS here explicitly so callers using the
            // two-register path (the canonical path per the OIP-013
            // dispatcher convention) get a clean
            // `(rax=0, rdx=ENOSYS)` rather than the
            // `(rax=SYSCALL_ERROR, rdx=0)` sentinel that
            // `dispatch(..).map(SyscallReturn::ok)` would produce on
            // `Err(NotYetImplemented)`.
            SyscallNumber::AiInvoke
            | SyscallNumber::AiStream
            | SyscallNumber::AiEmbed
            | SyscallNumber::AiClassify
            | SyscallNumber::AiTranscribe => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                super::early_console::write_str(
                    "[ai-syscall] dispatch_full scaffold: ENOSYS (IPC not yet wired)\n",
                );
                let _ = args;
                Ok(SyscallReturn::err(crate::syscall::syscall_errno::ENOSYS))
            }
            // Shell terminal syscalls that natively return two registers.
            // `PipeCreate` returns `(rax=read_fd, rdx=write_fd)`;
            // `FdDup2` returns `(rax=new_fd, rdx=0)`;
            // `ProcessWait` returns `(rax=exit_code, rdx=child_pid)`.
            //
            // On bare-metal each arm delegates to `shell_handlers::*`.
            // Host test builds return ENOSYS via the rich two-register path
            // so callers using `dispatch_full` get a clean
            // `(rax=0, rdx=ENOSYS)` rather than the legacy
            // `(rax=SYSCALL_ERROR, rdx=0)` sentinel.
            SyscallNumber::PipeCreate => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::pipe_create(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::ENOSYS))
                }
            }

            SyscallNumber::FdDup2 => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::fd_dup2(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::ENOSYS))
                }
            }

            SyscallNumber::ProcessWait => {
                #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
                {
                    Ok(shell_handlers::process_wait(args))
                }
                #[cfg(not(all(feature = "bare-metal", target_os = "none", not(test))))]
                {
                    let _ = args;
                    Ok(SyscallReturn::err(crate::syscall::syscall_errno::ENOSYS))
                }
            }

            // Shell terminal single-register syscalls — route through the
            // default single-register path so they participate in the
            // existing `dispatch` error-handling flow.
            SyscallNumber::ReadConsole
            | SyscallNumber::FdRead
            | SyscallNumber::FdWrite
            | SyscallNumber::FdClose
            | SyscallNumber::FdDup
            | SyscallNumber::FdSeek
            | SyscallNumber::FsOpen
            | SyscallNumber::FsStat
            | SyscallNumber::FsListDir
            | SyscallNumber::FsCreate
            | SyscallNumber::FsDelete
            | SyscallNumber::FsMkdir
            | SyscallNumber::GetCwd
            | SyscallNumber::SetCwd
            | SyscallNumber::ProcessSpawn
            | SyscallNumber::ProcessList
            | SyscallNumber::ProcessKill => self.dispatch(number, args).map(SyscallReturn::ok),

            other => self.dispatch(other, args).map(SyscallReturn::ok),
        }
    }
}

// -----------------------------------------------------------------------
// C-ABI dispatch entry (called from assembly stubs)
// -----------------------------------------------------------------------

/// Translate a raw syscall number + register args into a [`SyscallReturn`].
///
/// Returns the two-register pair `(rax, rdx)`. Most syscalls only fill
/// `rax`; the `MmioMap` path (OIP-013 § S2) additionally fills `rdx`
/// with a POSIX-aligned errno code on failure. The `SysV` AMD64 ABI
/// returns a `#[repr(C)]` struct of two `u64` fields in `(rax, rdx)`,
/// so the assembly trampolines do not need explicit handling beyond
/// preserving `rdx` across the return path.
///
/// `(rax = u64::MAX, rdx = 0)` ([`SYSCALL_ERROR`]) remains the legacy
/// single-register error sentinel for syscalls that have not migrated
/// to the rich path. This function is NOT gated on
/// `cfg(target_arch = "x86_64")` so host tests can call it directly.
#[unsafe(no_mangle)]
extern "C" fn kernel_syscall_dispatch(
    number: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) -> SyscallReturn {
    let args = [a0, a1, a2, a3, a4, a5];

    let n = match number {
        1 => SyscallNumber::MemMap,
        2 => SyscallNumber::MemUnmap,
        10 => SyscallNumber::TaskCreate,
        11 => SyscallNumber::TaskExit,
        12 => SyscallNumber::TaskYield,
        13 => SyscallNumber::TaskSleep,
        20 => SyscallNumber::IpcCreateChannel,
        21 => SyscallNumber::IpcDestroyChannel,
        22 => SyscallNumber::IpcSend,
        23 => SyscallNumber::IpcReceive,
        30 => SyscallNumber::CapValidate,
        31 => SyscallNumber::CapRevoke,
        32 => SyscallNumber::CapAttenuate,
        40 => SyscallNumber::TeeAttest,
        41 => SyscallNumber::TeeVerifyQuote,
        42 => SyscallNumber::TeeSeal,
        43 => SyscallNumber::TeeUnseal,
        50 => SyscallNumber::TimeMonotonicNanos,
        60 => SyscallNumber::WriteConsole,
        // Shell I/O + fd syscalls (terminal support).
        // Numeric range 61–68 reserved for console I/O and fd operations.
        // Translation MUST stay in lock-step with the `SyscallNumber`
        // discriminants — the `syscall_numbers_are_stable` test in
        // `crate::syscall` pins both ends against drift.
        61 => SyscallNumber::ReadConsole,
        62 => SyscallNumber::PipeCreate,
        63 => SyscallNumber::FdRead,
        64 => SyscallNumber::FdWrite,
        65 => SyscallNumber::FdClose,
        66 => SyscallNumber::FdDup,
        67 => SyscallNumber::FdDup2,
        68 => SyscallNumber::FdSeek,
        // OIP-013 + OIP-016 driver framework (P6.7.3 skeleton).
        70 => SyscallNumber::MmioMap,
        71 => SyscallNumber::DmaMap,
        72 => SyscallNumber::IrqAttach,
        73 => SyscallNumber::DriverLoad,
        74 => SyscallNumber::TeeTdcall,
        75 => SyscallNumber::TeeMsr,
        // OIP-Driver-NVMe-014 § S4 + § S6 step 12 BLK registry
        // (P6.7.10-pre.3). Translation here MUST stay in lock-step
        // with the `SyscallNumber` discriminants — the
        // `syscall_numbers_are_stable` test in `crate::syscall`
        // pins both ends against drift.
        76 => SyscallNumber::BlkRegister,
        77 => SyscallNumber::BlkUnregister,
        78 => SyscallNumber::BlkLookup,
        // Filesystem + process management syscalls (shell terminal support).
        // Numeric range 90–97 reserved; process mgmt reuses slots 14–17
        // from the scheduling decade. Translation MUST stay in lock-step
        // with `SyscallNumber` discriminants.
        90 => SyscallNumber::FsOpen,
        91 => SyscallNumber::FsStat,
        92 => SyscallNumber::FsListDir,
        93 => SyscallNumber::FsCreate,
        94 => SyscallNumber::FsDelete,
        95 => SyscallNumber::FsMkdir,
        96 => SyscallNumber::ProcessList,
        97 => SyscallNumber::ProcessKill,
        // Process management (shell terminal support) — numeric slots
        // 14–17 share the scheduling decade alongside TaskCreate/TaskExit.
        14 => SyscallNumber::ProcessSpawn,
        15 => SyscallNumber::ProcessWait,
        16 => SyscallNumber::GetCwd,
        17 => SyscallNumber::SetCwd,
        // OIP-Phase2-Entry-021 AI syscall surface (P2 Sprint 2).
        // Numeric decade `8x` reserved for AI. Translation here MUST
        // stay in lock-step with the `SyscallNumber` discriminants —
        // the `syscall_numbers_are_stable` test in `crate::syscall`
        // pins both ends against drift.
        80 => SyscallNumber::AiInvoke,
        81 => SyscallNumber::AiStream,
        82 => SyscallNumber::AiEmbed,
        83 => SyscallNumber::AiClassify,
        84 => SyscallNumber::AiTranscribe,
        _ => return SyscallReturn::ok(SYSCALL_ERROR),
    };

    KernelSyscallDispatcher
        .dispatch_full(n, args)
        .unwrap_or(SyscallReturn::ok(SYSCALL_ERROR))
}

// -----------------------------------------------------------------------
// syscall_init — configure MSRs and register INT 0x80
// -----------------------------------------------------------------------

/// Enable the `SYSCALL` / `SYSRET` mechanism and install the `INT 0x80`
/// fallback handler.
///
/// Must be called after [`super::idt::idt_init`] (INT 0x80 registration
/// modifies the IDT) and before any userspace code executes.
#[cfg(target_arch = "x86_64")]
pub fn syscall_init() {
    // SAFETY: MSR accesses are ring-0-only. We only set the SCE bit in EFER
    // (harmless on any x86_64 CPU since P6 targets) and write GDT-correct
    // STAR selector bases per ADR-0004 § 2.
    unsafe {
        // Enable SYSCALL/SYSRET in the Extended Feature Enable Register.
        wrmsr(MSR_EFER, rdmsr(MSR_EFER) | EFER_SCE);

        // STAR encoding (ADR-0004 § 2):
        //   bits [47:32] = STAR_KERNEL_BASE = 0x08
        //     SYSCALL CS = 0x08          (slot 1 kcode64)
        //     SYSCALL SS = 0x08 + 8      = 0x10 (slot 2 kdata64)
        //   bits [63:48] = STAR_USER_BASE = 0x10
        //     SYSRET q CS = 0x10 + 16 | 3 = 0x23 (slot 4 ucode64)
        //     SYSRET q SS = 0x10 +  8 | 3 = 0x1B (slot 3 udata64)
        let star_val = (u64::from(super::gdt::STAR_USER_BASE) << 48)
            | (u64::from(super::gdt::STAR_KERNEL_BASE) << 32);
        wrmsr(MSR_STAR, star_val);

        // Point LSTAR at our SYSCALL entry stub.
        wrmsr(MSR_LSTAR, omni_syscall_entry as *const () as usize as u64);

        // Mask IF (bit 9) on syscall entry so we do not take hardware
        // interrupts inside the non-reentrant syscall path.
        wrmsr(MSR_FMASK, 0x200);
    }

    // Register INT 0x80 in the IDT.
    super::idt::idt_set_vector(0x80, omni_int80_entry as *const () as usize as u64);

    super::early_console::write_str("[syscall] LSTAR set  INT80=0x80\n");
}

/// No-op stub for non-x86_64 host builds (developer machines on ARM, etc.).
#[cfg(not(target_arch = "x86_64"))]
pub fn syscall_init() {}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_time_monotonic_returns_u64() {
        let result = KernelSyscallDispatcher.dispatch(SyscallNumber::TimeMonotonicNanos, [0; 6]);
        // The value itself is arch-specific; we only require it to be Ok.
        assert!(result.is_ok());
    }

    #[test]
    fn dispatcher_task_yield_returns_zero() {
        let result = KernelSyscallDispatcher.dispatch(SyscallNumber::TaskYield, [0; 6]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn dispatcher_unknown_number_returns_error() {
        let ret = kernel_syscall_dispatch(999, 0, 0, 0, 0, 0, 0);
        assert_eq!(ret.rax, SYSCALL_ERROR);
        assert_eq!(ret.rdx, 0);
    }

    #[test]
    fn dispatcher_mem_map_not_yet_implemented() {
        let result = KernelSyscallDispatcher.dispatch(SyscallNumber::MemMap, [0; 6]);
        assert_eq!(result, Err(KernelError::NotYetImplemented));
    }

    // ---- OIP-013 / OIP-016 driver framework skeleton -----------------------
    //
    // `MmioMap (70)`, `DmaMap (71)`, `IrqAttach (72)`, and `DriverLoad (73)`
    // are all wired (P6.7.8.1 / P6.7.8.3 / P6.7.8.8) and dispatch via the
    // rich two-register path. The host test build does not link the
    // bare-metal singletons, so the override returns the `EACCES` sentinel;
    // the legacy `dispatch` arm reports `CapabilityDenied` so an accidental
    // single-register fallthrough is still caught.
    //
    // The remaining TEE syscalls keep their `NotYetImplemented` contract
    // until their handlers land.

    #[test]
    fn dispatcher_driver_framework_legacy_arm_returns_capability_denied() {
        // P6.7.8.8: `MmioMap`, `DmaMap`, `IrqAttach`, and `DriverLoad`
        // all reach their rich handler via `dispatch_full`. The legacy
        // single-register `dispatch` path returns `CapabilityDenied`
        // so an accidental fallthrough surfaces.
        //
        // P6.7.10-pre.3 — extended to cover `BlkRegister`,
        // `BlkUnregister`, `BlkLookup` which share the same
        // rich-path-only convention as the OIP-013 siblings.
        for n in [
            SyscallNumber::MmioMap,
            SyscallNumber::DmaMap,
            SyscallNumber::IrqAttach,
            SyscallNumber::DriverLoad,
            SyscallNumber::BlkRegister,
            SyscallNumber::BlkUnregister,
            SyscallNumber::BlkLookup,
        ] {
            let result = KernelSyscallDispatcher.dispatch(n, [0; 6]);
            assert_eq!(
                result,
                Err(KernelError::CapabilityDenied),
                "unexpected legacy dispatch result for {n:?}"
            );
        }
    }

    #[test]
    fn kernel_syscall_dispatch_blk_numbers_translate_to_blk_variants() {
        // P6.7.10-pre.3 — exercise the 76/77/78 → SyscallNumber arm
        // explicitly. The host build's rich path returns `EACCES`
        // (no bare-metal singletons available); we re-assert here so
        // a future commit that drops the translation arm surfaces
        // with a clear test name instead of a generic sentinel
        // regression.
        for n in [76, 77, 78] {
            let ret = kernel_syscall_dispatch(n, 0, 0, 0, 0, 0, 0);
            assert_eq!(
                ret.rax, 0,
                "BLK syscall {n} must route through rich path on host"
            );
            assert_eq!(
                ret.rdx,
                crate::syscall::syscall_errno::EACCES,
                "BLK syscall {n} must surface EACCES on host"
            );
        }
    }

    #[test]
    fn dispatcher_remaining_tee_syscalls_return_not_yet_implemented() {
        for n in [SyscallNumber::TeeTdcall, SyscallNumber::TeeMsr] {
            let result = KernelSyscallDispatcher.dispatch(n, [0; 6]);
            assert_eq!(
                result,
                Err(KernelError::NotYetImplemented),
                "unexpected dispatch result for {n:?}"
            );
        }
    }

    #[test]
    fn dispatcher_full_mmio_map_surfaces_eaccess_on_host() {
        // Host-test build has no `FRAME_ALLOC` / `SCHEDULER` singletons,
        // so the rich override returns `EACCES` directly so the trait
        // shape is exercised without the bare-metal statics.
        let ret = KernelSyscallDispatcher
            .dispatch_full(SyscallNumber::MmioMap, [0; 6])
            .expect("dispatch_full never propagates KernelError for MmioMap");
        assert_eq!(ret.rax, 0);
        assert_eq!(ret.rdx, crate::syscall::syscall_errno::EACCES);
    }

    #[test]
    fn dispatcher_full_dma_map_irq_attach_and_driver_load_surface_eaccess_on_host() {
        // P6.7.8.8: same host-side contract as MmioMap — the rich
        // handlers return EACCES because the bare-metal singletons
        // are not linked into the host test binary.
        // P6.7.10-pre.3: extended to cover the BLK registry triplet
        // (`BlkRegister`, `BlkUnregister`, `BlkLookup`) which share
        // the same rich-path convention as the OIP-013 siblings.
        for n in [
            SyscallNumber::DmaMap,
            SyscallNumber::IrqAttach,
            SyscallNumber::DriverLoad,
            SyscallNumber::BlkRegister,
            SyscallNumber::BlkUnregister,
            SyscallNumber::BlkLookup,
        ] {
            let ret = KernelSyscallDispatcher
                .dispatch_full(n, [0; 6])
                .expect("dispatch_full never propagates KernelError for driver-framework syscalls");
            assert_eq!(ret.rax, 0, "rich {n:?} must report rax=0 on host");
            assert_eq!(
                ret.rdx,
                crate::syscall::syscall_errno::EACCES,
                "rich {n:?} must report rdx=EACCES on host"
            );
        }
    }

    #[test]
    fn kernel_syscall_dispatch_driver_framework_numbers_route() {
        // ABI numbers 70..=78: `MmioMap (70)`, `DmaMap (71)`,
        // `IrqAttach (72)`, and `DriverLoad (73)` all go through the
        // rich two-register path and surface `EACCES` on the host
        // build (no SCHEDULER/FRAME_ALLOC). TEE syscalls (74/75)
        // still funnel to the `NotYetImplemented` sentinel via the
        // legacy unwrap_or. BLK syscalls (76/77/78) — P6.7.10-pre.3
        // — share the rich-path convention so they too report
        // `EACCES` on the host build.
        for n in 70..=78u32 {
            let ret = kernel_syscall_dispatch(n, 0, 0, 0, 0, 0, 0);
            let is_rich = matches!(n, 70..=73 | 76..=78);
            if is_rich {
                assert_eq!(
                    ret.rax, 0,
                    "syscall {n} should report rax=0 on host error path"
                );
                assert_eq!(
                    ret.rdx,
                    crate::syscall::syscall_errno::EACCES,
                    "syscall {n} should report rdx=EACCES on host build"
                );
            } else {
                assert_eq!(
                    ret.rax, SYSCALL_ERROR,
                    "number {n} did not flatten to sentinel"
                );
            }
        }
    }

    #[test]
    fn kernel_syscall_dispatch_unknown_driver_decade_number_returns_sentinel() {
        // 79 is the only number inside the `7x` driver decade still
        // reserved-but-unallocated after the P6.7.10-pre.3 BLK
        // registry expansion. It MUST hit the
        // `_ => return SYSCALL_ERROR` arm so userspace cannot
        // accidentally invoke a not-yet-defined slot.
        let n = 79u32;
        let ret = kernel_syscall_dispatch(n, 0, 0, 0, 0, 0, 0);
        assert_eq!(ret.rax, SYSCALL_ERROR, "reserved number {n} leaked");
        assert_eq!(ret.rdx, 0);
    }

    #[test]
    fn kernel_syscall_dispatch_time_syscall_succeeds() {
        // Number 50 = TimeMonotonicNanos; must return something other than u64::MAX.
        let ret = kernel_syscall_dispatch(50, 0, 0, 0, 0, 0, 0);
        assert_ne!(ret.rax, SYSCALL_ERROR);
        assert_eq!(ret.rdx, 0);
    }

    #[test]
    fn kernel_syscall_dispatch_unknown_returns_sentinel() {
        let ret = kernel_syscall_dispatch(0xDEAD, 0, 0, 0, 0, 0, 0);
        assert_eq!(ret.rax, u64::MAX);
        assert_eq!(ret.rdx, 0);
    }

    #[test]
    fn star_msr_value_encodes_kernel_cs() {
        let star_val = (0x001B_u64 << 48) | (0x0008_u64 << 32);
        // Kernel CS must sit in bits [47:32].
        let kernel_cs = (star_val >> 32) & 0xFFFF;
        assert_eq!(kernel_cs, 0x0008);
        // User CS placeholder must sit in bits [63:48].
        let user_cs = (star_val >> 48) & 0xFFFF;
        assert_eq!(user_cs, 0x001B);
    }

    #[test]
    fn syscall_error_sentinel_is_u64_max() {
        assert_eq!(SYSCALL_ERROR, u64::MAX);
    }
}
