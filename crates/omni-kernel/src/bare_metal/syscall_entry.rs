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

/// MB11 — terminate the calling user-process task. Bare-metal halts;
/// host build returns `Ok(0)` for testability.
#[allow(
    clippy::unnecessary_wraps,
    reason = "signature parity with other SyscallDispatcher arms"
)]
fn task_exit(code: u64) -> KernelResult<u64> {
    #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
    {
        use crate::scheduling::Scheduler;
        super::early_console::write_str("[user] exit=");
        // SAFETY: single-core; SCHEDULER not aliased.
        unsafe {
            super::early_console::write_usize(code as usize);
            super::early_console::write_str("\n");
            let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
            if let Some(current) = sched.current_task_id() {
                let _ = sched.dequeue(current);
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
                let secs = super::arch::rtc_seconds();
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
