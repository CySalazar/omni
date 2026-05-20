//! System call dispatch.
//!
//! ## Status
//!
//! P6.5 scaffold. The syscall *number* enumeration is locked in for
//! the v0.1 protocol surface; the actual dispatcher (which lives in
//! arch-specific entry code, e.g. `int 0x80` / `syscall` / `sysenter`
//! handlers on `x86_64`) is owned by the bootloader integration in P6.2.
//!
//! ## Design rationale
//!
//! - **Stable numeric ABI.** Syscall numbers are immutable after v1.0;
//!   adding a syscall is an OIP. This is the closest the kernel comes
//!   to a userspace ABI guarantee.
//! - **Capability-checked at the entry point.** Every syscall validates
//!   the caller's capability for the requested action before dispatching
//!   to the subsystem.
//! - **Small surface.** The v1 kernel exposes a deliberately small set
//!   of syscalls. Higher-level functionality (e.g. AI invocation) is
//!   provided by userspace services reached via IPC, not by direct
//!   syscall.

#![allow(
    clippy::missing_errors_doc,
    reason = "trait scaffold dispatch returns NotYetImplemented until MB11/MB12 wire handlers"
)]

use crate::KernelResult;

// -----------------------------------------------------------------------------
// Syscall numbers
// -----------------------------------------------------------------------------

/// Stable numeric identifiers for kernel syscalls.
///
/// **The numeric value is part of the userspace ABI.** Do NOT renumber
/// existing variants; only append new variants at the end. Removing a
/// variant requires an OIP and a multi-year deprecation window.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallNumber {
    // ----- Memory -----
    /// `mmap` equivalent: map an anonymous page region.
    MemMap = 1,
    /// Unmap a previously-mapped region.
    MemUnmap = 2,

    // ----- Scheduling / process -----
    /// Create a new task (process or thread).
    TaskCreate = 10,
    /// Terminate the calling task.
    TaskExit = 11,
    /// Yield the CPU voluntarily.
    TaskYield = 12,
    /// Sleep until a deadline.
    TaskSleep = 13,

    // ----- IPC -----
    /// Create a new channel.
    IpcCreateChannel = 20,
    /// Destroy a channel.
    IpcDestroyChannel = 21,
    /// Send a message.
    IpcSend = 22,
    /// Receive a message.
    IpcReceive = 23,

    // ----- Capabilities -----
    /// Validate a capability.
    CapValidate = 30,
    /// Revoke a capability.
    CapRevoke = 31,
    /// Derive an attenuated capability (Macaroons-style).
    CapAttenuate = 32,

    // ----- TEE / Attestation -----
    /// Request a TEE attestation quote.
    TeeAttest = 40,
    /// Verify a peer's quote.
    TeeVerifyQuote = 41,
    /// Seal a blob under the current TEE measurement.
    TeeSeal = 42,
    /// Unseal a blob.
    TeeUnseal = 43,

    // ----- Time -----
    /// Get monotonic time (nanoseconds since boot).
    TimeMonotonicNanos = 50,

    // ----- I/O (MB11) -----
    /// Write a user-supplied byte slice to the kernel console. ABI:
    /// `(ptr: u64, len: u64) -> u64`. Returns `len` on success or
    /// `u64::MAX` on a validation failure.
    WriteConsole = 60,

    // ----- Driver framework (OIP-013, P6.7.3 skeleton) -----
    // Numeric decade `7x` reserved for the user-space driver framework.
    // See `OIP-Driver-Framework-013` Appendix A for the reconciliation
    // rationale (the original Draft proposed `22..=25` but those slots
    // are MB12-IPC-locked). Handlers are scaffolded to
    // `KernelError::NotYetImplemented` (ENOSYS-equivalent) until the
    // P6.7.8 first-party driver implementations land.
    //
    /// Map a PCI BAR MMIO region into the caller's address space.
    /// ABI: `(phys_base, len, flags, cap_ptr, cap_len) -> va_base`.
    /// See `OIP-Driver-Framework-013` § S2.
    MmioMap = 70,
    /// Install an IOMMU DMA window.
    /// ABI: `(iova_base, len, direction, cap_ptr, cap_len) -> 0`.
    /// See `OIP-Driver-Framework-013` § S3.
    DmaMap = 71,
    /// Attach an IRQ line to a per-driver IPC channel.
    /// ABI: `(irq_line, ipc_channel_id, cap_ptr, cap_len, 0) -> 0`.
    /// See `OIP-Driver-Framework-013` § S4.
    IrqAttach = 72,
    /// Load a signed driver image.
    /// ABI: `(manifest_ptr, manifest_len, image_ptr, image_len, 0) -> driver_pid`.
    /// See `OIP-Driver-Framework-013` § S5.
    DriverLoad = 73,
    /// Issue a kernel-mediated TDCALL on Intel TDX (Ring 0 only).
    /// ABI: `(leaf, r10, r11, r12, r13) -> rax_packed`.
    /// See `OIP-Driver-TEE-016` § S5.3 (editorially reconciled to 74).
    TeeTdcall = 74,
    /// Issue a kernel-mediated SEV-SNP MSR write (Ring 0 only).
    /// ABI: `(msr_index, value_lo, value_hi, payload_ptr, payload_len) -> 0`.
    /// See `OIP-Driver-TEE-016` § S6.3 (editorially reconciled to 75).
    TeeMsr = 75,
}

// -----------------------------------------------------------------------------
// Syscall dispatcher trait
// -----------------------------------------------------------------------------

/// Trait for the kernel syscall dispatcher.
///
/// The arch-specific entry code (`int 0x80` etc) translates the
/// arch-level register state into a call to `dispatch`; this trait
/// keeps the dispatch logic arch-neutral.
pub trait SyscallDispatcher {
    /// Dispatches a syscall by number with up to 6 generic register
    /// arguments (the `x86_64` ABI fits in 6 GPRs). Returns the syscall
    /// result code or [`crate::KernelError`].
    fn dispatch(&mut self, number: SyscallNumber, args: [u64; 6]) -> KernelResult<u64>;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_numbers_are_stable() {
        // These constants form the userspace ABI. Any test failure
        // here is a deliberate ABI change and MUST go through OIP.
        assert_eq!(SyscallNumber::MemMap as u32, 1);
        assert_eq!(SyscallNumber::TaskCreate as u32, 10);
        assert_eq!(SyscallNumber::IpcSend as u32, 22);
        assert_eq!(SyscallNumber::CapValidate as u32, 30);
        assert_eq!(SyscallNumber::TeeAttest as u32, 40);
        assert_eq!(SyscallNumber::TimeMonotonicNanos as u32, 50);
        // OIP-013 + OIP-016 driver-framework decade (P6.7.3 skeleton).
        // Pinning these here prevents an accidental renumber that would
        // silently break a driver manifest signed against the old number.
        assert_eq!(SyscallNumber::MmioMap as u32, 70);
        assert_eq!(SyscallNumber::DmaMap as u32, 71);
        assert_eq!(SyscallNumber::IrqAttach as u32, 72);
        assert_eq!(SyscallNumber::DriverLoad as u32, 73);
        assert_eq!(SyscallNumber::TeeTdcall as u32, 74);
        assert_eq!(SyscallNumber::TeeMsr as u32, 75);
    }

    #[test]
    fn syscall_number_fits_in_u32() {
        assert_eq!(core::mem::size_of::<SyscallNumber>(), 4);
    }
}
