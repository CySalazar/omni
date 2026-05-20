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
//! Return value is in RAX. `u64::MAX` is the error sentinel.

#![allow(
    unsafe_code,
    reason = "MSR R/W + naked asm syscall stubs; SAFETY per fn"
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "RAX number is u64 by ABI but the dispatch enum tag fits u32"
)]

use crate::syscall::{SyscallDispatcher, SyscallNumber};
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

            // All other syscalls are scaffolded but not yet implemented.
            _ => Err(KernelError::NotYetImplemented),
        }
    }
}

// -----------------------------------------------------------------------
// C-ABI dispatch entry (called from assembly stubs)
// -----------------------------------------------------------------------

/// Translate a raw syscall number + register args into a `KernelResult`, then
/// flatten to a `u64` for the ABI boundary.
///
/// `u64::MAX` ([`SYSCALL_ERROR`]) is the error sentinel. This function is NOT
/// gated on `cfg(target_arch = "x86_64")` so host tests (on aarch64 dev
/// machines) can call it directly.
#[unsafe(no_mangle)]
extern "C" fn kernel_syscall_dispatch(
    number: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) -> u64 {
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
        _ => return SYSCALL_ERROR,
    };

    KernelSyscallDispatcher
        .dispatch(n, args)
        .unwrap_or(SYSCALL_ERROR)
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
        wrmsr(MSR_LSTAR, omni_syscall_entry as usize as u64);

        // Mask IF (bit 9) on syscall entry so we do not take hardware
        // interrupts inside the non-reentrant syscall path.
        wrmsr(MSR_FMASK, 0x200);
    }

    // Register INT 0x80 in the IDT.
    super::idt::idt_set_vector(0x80, omni_int80_entry as usize as u64);

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
        assert_eq!(ret, SYSCALL_ERROR);
    }

    #[test]
    fn dispatcher_mem_map_not_yet_implemented() {
        let result = KernelSyscallDispatcher.dispatch(SyscallNumber::MemMap, [0; 6]);
        assert_eq!(result, Err(KernelError::NotYetImplemented));
    }

    #[test]
    fn kernel_syscall_dispatch_time_syscall_succeeds() {
        // Number 50 = TimeMonotonicNanos; must return something other than u64::MAX.
        let ret = kernel_syscall_dispatch(50, 0, 0, 0, 0, 0, 0);
        assert_ne!(ret, SYSCALL_ERROR);
    }

    #[test]
    fn kernel_syscall_dispatch_unknown_returns_sentinel() {
        let ret = kernel_syscall_dispatch(0xDEAD, 0, 0, 0, 0, 0, 0);
        assert_eq!(ret, u64::MAX);
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
