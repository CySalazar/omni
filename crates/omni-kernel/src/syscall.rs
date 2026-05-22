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

    // ----- BLK service-channel registry (OIP-Driver-NVMe-014 § S4) -----
    // Numeric range `76..=78` reserved for the kernel-mediated BLK
    // channel registry that backs the `omni.svc.blk.<diskN>` IPC
    // channel namespace. Producer drivers (NVMe today, future
    // SATA / virtio-blk) call `BlkRegister` after they create the
    // channel via `IpcCreateChannel`; the consumer filesystem
    // service calls `BlkLookup` to resolve `disk_slot → ChannelId`
    // without sniffing the IPC layer by string. See
    // `OIP-Driver-NVMe-014` § S4 + § S6 step 12.
    /// Record an `omni.svc.blk.<disk_slot>` channel in the kernel
    /// BLK registry. ABI:
    /// `(disk_slot_ptr, disk_slot_len, channel_id, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// The caller MUST already own the supplied `channel_id`; the
    /// kernel rejects the call with `EACCES` otherwise. Disk-slot
    /// validation matches [`crate::services::blk::BlkChannelRegistry::register`]
    /// (ASCII `[A-Za-z0-9_-]`, ≤ `MAX_DISK_SLOT_LEN` bytes).
    BlkRegister = 76,
    /// Remove an `omni.svc.blk.<disk_slot>` mapping the caller owns.
    /// ABI: `(disk_slot_ptr, disk_slot_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// Returns `EACCES` if the caller is not the recorded owner;
    /// task-exit clean-up is handled separately via
    /// [`crate::services::blk::BlkChannelRegistry::clear_for_owner`].
    BlkUnregister = 77,
    /// Resolve `omni.svc.blk.<disk_slot>` to its live channel id.
    /// ABI: `(disk_slot_ptr, disk_slot_len, 0, 0, 0, 0) -> (rax=channel_id, rdx=0)`
    /// on success; `(rax=0, rdx=ENOENT)` if the slot is not
    /// registered. Read-only — the channel id alone confers no
    /// IPC authority (`IpcSend` / `IpcRecv` still require the
    /// per-channel capability tokens minted at create time).
    BlkLookup = 78,
}

// -----------------------------------------------------------------------------
// Two-register return value (OIP-013 § S2)
// -----------------------------------------------------------------------------

/// Two-register syscall return value.
///
/// The single-register dispatch path returns its value in `RAX`. Some
/// syscalls — initially `MmioMap` per `OIP-Driver-Framework-013` § S2 —
/// also report a POSIX-style error code in `RDX`. The `#[repr(C)]`
/// layout matches the System V AMD64 return convention for a struct
/// of two `INTEGER`-class fields: `rax = first u64`, `rdx = second
/// u64`. The kernel's `extern "C"` syscall dispatcher returns this
/// type by value; the assembly trampoline preserves RDX through to
/// the user-mode `sysretq` / `iretq` so user space observes the pair.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyscallReturn {
    /// Primary return value (`RAX`). Convention: non-zero on success
    /// for handlers that return a handle/VA/length; zero on hard
    /// errors when paired with a non-zero `rdx`.
    pub rax: u64,
    /// Secondary return value (`RDX`). `0` on success; one of the
    /// [`syscall_errno`] codes on error.
    pub rdx: u64,
}

impl SyscallReturn {
    /// Build a successful return with the supplied primary value and
    /// `rdx = 0` (no error).
    #[must_use]
    pub const fn ok(rax: u64) -> Self {
        Self { rax, rdx: 0 }
    }

    /// Build an error return with `rax = 0` and the supplied errno
    /// code in `rdx`.
    #[must_use]
    pub const fn err(errno: u64) -> Self {
        Self { rax: 0, rdx: errno }
    }
}

/// POSIX-aligned syscall errno codes used in the two-register return
/// path. Numbering follows Linux `errno-base.h` for the subset that
/// `OIP-Driver-Framework-013` § S2.3 references.
pub mod syscall_errno {
    /// No such entry — used by the BLK lookup syscall when the
    /// requested disk slot is not registered. POSIX `ENOENT = 2`.
    pub const ENOENT: u64 = 2;
    /// Permission denied — capability verification failed.
    pub const EACCES: u64 = 13;
    /// Bad address — user pointer or length is invalid.
    pub const EFAULT: u64 = 14;
    /// Invalid argument — alignment, range, or reserved bits.
    pub const EINVAL: u64 = 22;
    /// No space left — driver VA range exhausted, or BLK registry
    /// full (`MAX_BLK_CHANNELS`).
    pub const ENOSPC: u64 = 28;
    /// Function not implemented — feature requires runtime support
    /// that has not been initialised (e.g. PAT for WC mappings).
    pub const ENOSYS: u64 = 38;
    /// Object already exists — BLK registry already holds an entry
    /// for the requested disk slot. POSIX `EEXIST = 17`.
    pub const EEXIST: u64 = 17;
    /// Internal kernel invariant violation — surfaces
    /// [`crate::services::blk::BlkRegistryError::Internal`] at
    /// the BLK syscall boundary without aborting the kernel. POSIX
    /// `EIO = 5`.
    pub const EIO: u64 = 5;
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

    /// Dispatches a syscall and returns both `RAX` and `RDX`.
    ///
    /// Default implementation defers to [`Self::dispatch`] and wraps
    /// the result as [`SyscallReturn::ok`] on success or
    /// `SyscallReturn::err(syscall_errno::EINVAL)` on a `KernelError`.
    /// Handlers that need the richer two-register ABI (e.g. `MmioMap`)
    /// override this method to return the specific errno.
    fn dispatch_full(
        &mut self,
        number: SyscallNumber,
        args: [u64; 6],
    ) -> KernelResult<SyscallReturn> {
        self.dispatch(number, args).map(SyscallReturn::ok)
    }
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
        // OIP-Driver-NVMe-014 § S4 + § S6 step 12 BLK registry decade.
        // Pinning these numbers prevents an accidental renumber that
        // would silently break a future NVMe / SATA / virtio-blk
        // driver manifest signed against the old numbers.
        assert_eq!(SyscallNumber::BlkRegister as u32, 76);
        assert_eq!(SyscallNumber::BlkUnregister as u32, 77);
        assert_eq!(SyscallNumber::BlkLookup as u32, 78);
    }

    #[test]
    fn syscall_number_fits_in_u32() {
        assert_eq!(core::mem::size_of::<SyscallNumber>(), 4);
    }

    // ---- Two-register return path (OIP-013 § S2) -------------------------

    #[test]
    fn syscall_return_ok_zero_errno() {
        let r = SyscallReturn::ok(0x4000_0000);
        assert_eq!(r.rax, 0x4000_0000);
        assert_eq!(r.rdx, 0);
    }

    #[test]
    fn syscall_return_err_zero_rax() {
        let r = SyscallReturn::err(syscall_errno::EACCES);
        assert_eq!(r.rax, 0);
        assert_eq!(r.rdx, 13);
    }

    #[test]
    fn syscall_return_is_two_u64_struct() {
        // Repr(C) on x86_64 places two u64 fields in (rax, rdx) at the
        // SysV ABI boundary. Pin the layout so a re-order would surface
        // as a failing test before the ABI breaks. Field-offset checks
        // are sufficient — the SysV "two INTEGER fields ≤ 16 bytes →
        // return in (rax, rdx)" rule is keyed on the in-memory layout.
        assert_eq!(core::mem::size_of::<SyscallReturn>(), 16);
        assert_eq!(core::mem::align_of::<SyscallReturn>(), 8);
        let r = SyscallReturn { rax: 1, rdx: 2 };
        assert_eq!(r.rax, 1);
        assert_eq!(r.rdx, 2);
        assert_eq!(core::mem::offset_of!(SyscallReturn, rax), 0);
        assert_eq!(core::mem::offset_of!(SyscallReturn, rdx), 8);
    }

    #[test]
    fn syscall_errno_codes_are_posix_aligned() {
        assert_eq!(syscall_errno::ENOENT, 2);
        assert_eq!(syscall_errno::EIO, 5);
        assert_eq!(syscall_errno::EACCES, 13);
        assert_eq!(syscall_errno::EFAULT, 14);
        assert_eq!(syscall_errno::EEXIST, 17);
        assert_eq!(syscall_errno::EINVAL, 22);
        assert_eq!(syscall_errno::ENOSPC, 28);
        assert_eq!(syscall_errno::ENOSYS, 38);
    }
}
