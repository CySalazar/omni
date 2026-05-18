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
    }

    #[test]
    fn syscall_number_fits_in_u32() {
        assert_eq!(core::mem::size_of::<SyscallNumber>(), 4);
    }
}
