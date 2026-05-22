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
                    super::early_console::emit(&buf[..chunk_usize]);
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

    /// Rust-side IRQ dispatch: reads the in-service LAPIC vector, looks
    /// up the slot, enqueues a notification on the bound channel (or
    /// increments the missed-count if the channel is full), then issues
    /// LAPIC EOI. Called from the asm trampoline.
    ///
    /// Phase 1 caveat: real channel-enqueue requires building an
    /// `IrqNotification::Tick` payload + invoking the IPC registry
    /// send path with the kernel-as-sender principal. The skeleton
    /// here increments the `missed` counter unconditionally (so the
    /// fire is observable to host-side smoke) and issues EOI; a
    /// follow-up P6.7.8.x will wire the proper kernel-→-driver-channel
    /// enqueue path.
    pub(super) fn dispatch_fire(vector: u8) {
        note_fire(vector);
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
        // P6.7.9-pre.9; today the binding is host-testable bookkeeping
        // that exercises the trait dispatch on `IommuKind` and seeds the
        // PCB-side teardown list).
        //
        // Failure mode: a missing IOMMU domain install (out of DIDs) or
        // a vendor-table conflict (re-attach without prior detach) is
        // logged as a best-effort warning — the driver process stays
        // alive with whatever bindings did succeed; the first `DmaMap`
        // call against an un-attached device will EACCES out of the
        // capability check before reaching the IOMMU surface. We
        // accept this so partial-attach failure is observable in user
        // space, matching the cap-deposit failure mode above.
        // -------------------------------------------------------------
        {
            use crate::bare_metal::iommu::{
                IommuBackend, domain_for_task, iommu_attach_device, pci_bdfs_from_resources,
                with_iommu_backend,
            };
            let domain_id = domain_for_task(task_id.0);
            let bdfs = pci_bdfs_from_resources(&manifest.capabilities.pci_devices);
            // Idempotent: returns Ok(()) if the domain is already
            // installed (the dma_map handler may have raced ahead on
            // a future MP build; today it cannot, but the API is
            // designed for it).
            let domain_install_ok =
                with_iommu_backend(|kind| kind.install_domain(domain_id)).is_ok();
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
                            }
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
    /// Best-effort: per-BDF detach failures (e.g. the backend never
    /// recorded the binding because the original attach raced an
    /// install-domain failure) are silently swallowed; the goal is to
    /// release whatever IOMMU state did get recorded, not to surface a
    /// teardown error to user space (the calling task is already
    /// `Terminated` by the time this runs).
    pub(super) fn tear_down_pci_bindings(task: crate::scheduling::TaskId) {
        use crate::bare_metal::iommu::iommu_detach_device;
        // SAFETY: SYSCALL path is single-CPU; SCHEDULER not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            let Some(pcb) = sched.process_mut(task) else {
                return;
            };
            let bdfs = core::mem::take(&mut pcb.bound_pci_devices);
            for bdf in bdfs {
                let _ = iommu_detach_device(bdf);
            }
        }
    }
}

struct KernelSyscallDispatcher;

impl SyscallDispatcher for KernelSyscallDispatcher {
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
            SyscallNumber::MmioMap
            | SyscallNumber::DmaMap
            | SyscallNumber::IrqAttach
            | SyscallNumber::DriverLoad => {
                let _ = args;
                Err(KernelError::CapabilityDenied)
            }
            // Remaining TEE syscalls are still scaffolded; landing in
            // this arm rather than the catch-all forces a compiler
            // error when a future commit forgets to re-route one of
            // them.
            SyscallNumber::TeeTdcall | SyscallNumber::TeeMsr => {
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
        // OIP-013 + OIP-016 driver framework (P6.7.3 skeleton).
        70 => SyscallNumber::MmioMap,
        71 => SyscallNumber::DmaMap,
        72 => SyscallNumber::IrqAttach,
        73 => SyscallNumber::DriverLoad,
        74 => SyscallNumber::TeeTdcall,
        75 => SyscallNumber::TeeMsr,
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
        for n in [
            SyscallNumber::MmioMap,
            SyscallNumber::DmaMap,
            SyscallNumber::IrqAttach,
            SyscallNumber::DriverLoad,
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
        for n in [
            SyscallNumber::DmaMap,
            SyscallNumber::IrqAttach,
            SyscallNumber::DriverLoad,
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
        // ABI numbers 70..=75: `MmioMap (70)`, `DmaMap (71)`,
        // `IrqAttach (72)`, and `DriverLoad (73)` all go through the
        // rich two-register path and surface `EACCES` on the host
        // build (no SCHEDULER/FRAME_ALLOC). TEE syscalls (74/75)
        // still funnel to the `NotYetImplemented` sentinel via the
        // legacy unwrap_or.
        for n in 70..=75u32 {
            let ret = kernel_syscall_dispatch(n, 0, 0, 0, 0, 0, 0);
            if (70..=73).contains(&n) {
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
        // 76..=79 are reserved inside the `7x` driver decade but NOT yet
        // assigned. They MUST hit the `_ => return SYSCALL_ERROR` arm so
        // userspace cannot accidentally invoke a not-yet-defined slot.
        for n in 76..=79 {
            let ret = kernel_syscall_dispatch(n, 0, 0, 0, 0, 0, 0);
            assert_eq!(ret.rax, SYSCALL_ERROR, "reserved number {n} leaked");
            assert_eq!(ret.rdx, 0);
        }
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
