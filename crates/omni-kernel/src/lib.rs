//! # `omni-kernel`
//!
//! The OMNI OS microkernel.
//!
//! Responsibilities (and only these):
//!
//! - Memory management (virtual memory, page tables, allocators)
//! - Process and thread scheduling
//! - Inter-process communication (typed message passing)
//! - Capability-based security primitives
//! - Hardware abstraction interfaces (HAL contracts)
//!
//! Everything else — filesystems, drivers, networking stacks, AI runtime —
//! runs as user-space services communicating via IPC. This minimizes the
//! Trusted Computing Base.
//!
//! ## Status
//!
//! Draft v0.2 — module surface and trait skeletons are landed for memory,
//! scheduling, IPC, capabilities, and syscall dispatch. The crate still
//! compiles in `std` mode by default; the `no_std + no_main` bare-metal
//! transition is gated behind the `bare-metal` feature, which switches
//! `lib.rs` (and every module) to `#![no_std]` and disables anything that
//! pulls in libstd. The transition to a real bare-metal binary lands in
//! P6.1–P6.2 per [`/oips/oip-kernel-003.md`](../../../oips/oip-kernel-003.md).
//!
//! ## Design rationale
//!
//! 1. **Microkernel**: smaller TCB → smaller attack surface. Faults in a
//!    service crash that service, not the kernel.
//! 2. **Rust + memory safety**: eliminates entire classes of vulnerabilities
//!    that plague C kernels (use-after-free, buffer overflows, data races).
//! 3. **Capability-based security**: the only way to act on a resource is
//!    to present a valid capability. No ambient authority, no superuser.
//! 4. **Message passing IPC**: typed, async-friendly, encryption-aware.
//! 5. **Verifiability over time**: a small kernel is amenable to formal
//!    methods (in line with seL4 prior art). Long-term goal: formal proofs
//!    for the IPC and capability subsystems.
//!
//! ## Modules
//!
//! - [`memory`] — virtual memory, page tables, allocators.
//! - [`scheduling`] — process and thread scheduling.
//! - [`ipc`] — inter-process communication primitives.
//! - [`capabilities`] — kernel-side capability validation and minting.
//! - [`syscall`] — system call dispatch.

#![doc(html_root_url = "https://docs.omni-os.org/omni-kernel")]
// `no_std` / `no_main` are only meaningful in non-test builds. Tests
// always require `std` (for the test harness) and a `main` (for the
// runner), so we suppress both attributes under `cfg(test)`. Under
// `cargo build --features bare-metal`, the kernel still compiles as
// `no_std + no_main` exactly as P6.1 requires.
#![cfg_attr(all(feature = "bare-metal", not(test)), no_std)]
#![cfg_attr(all(feature = "bare-metal", not(test)), no_main)]
#![warn(missing_docs)]
// Trait scaffolds in `memory`, `scheduling`, `ipc`, `capabilities`,
// and `syscall` currently expose `Result`-returning methods whose
// concrete error contracts are settled per-subsystem in P6.3+. Until
// the corresponding impls land, the per-method `# Errors` sections
// would all read "returns `NotYetImplemented`", which is noise. The
// allow is removed in the OIP that activates the corresponding
// subsystem.
#![allow(clippy::missing_errors_doc)]

// `alloc` is available even in `no_std` mode (the bare-metal kernel
// provides its own allocator). In `std` builds, `alloc` is re-exported
// transparently.
extern crate alloc;

pub mod capabilities;
pub mod ipc;
pub mod memory;
pub mod scheduling;
pub mod syscall;

// Bare-metal runtime: panic handler, global allocator, early console,
// arch intrinsics. Lives only when the `bare-metal` feature is on; the
// inner `#[panic_handler]` and `#[global_allocator]` items are further
// gated `not(test)` to keep `cargo test --all-features` compilable.
//
// Specified by OIP-Kernel-012 (was OIP-Kernel-004 — renumbered at
// Draft → Review on 2026-05-14 per OIP-Process-001 §8.3 to free the
// "004" integer for the canonical OIP-Serde-004).
#[cfg(feature = "bare-metal")]
pub mod bare_metal;

// -----------------------------------------------------------------------------
// Kernel-wide error type
// -----------------------------------------------------------------------------

/// Kernel-side error discriminant.
///
/// Kept deliberately small and PII-safe. Userspace receives errors in
/// `omni_types::OmniError` form via the syscall ABI; this enum is the
/// kernel's internal representation, mapped at the syscall boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelError {
    /// The operation is not yet implemented in this kernel build.
    /// Returned by every scaffold method until its corresponding P6 task
    /// lands.
    NotYetImplemented,
    /// A capability check failed. The caller did not present a valid
    /// capability for the requested operation.
    CapabilityDenied,
    /// A resource is exhausted (out of memory, no free thread slots, IPC
    /// queue full, etc.).
    ResourceExhausted,
    /// Invalid argument from userspace. The syscall layer is supposed to
    /// catch most of these; this variant is for the edge cases the
    /// syscall layer cannot validate without context.
    InvalidArgument,
    /// Internal invariant violation. Indicates a kernel bug.
    Internal,
}

// -----------------------------------------------------------------------------
// Kernel-wide result alias
// -----------------------------------------------------------------------------

/// Standard `Result` type for kernel operations.
pub type KernelResult<T> = Result<T, KernelError>;

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod sanity {
    use super::KernelError;

    #[test]
    fn kernel_error_is_small() {
        // The error enum should fit in 1 or 2 bytes so it can be returned
        // efficiently from syscall fast-paths.
        assert!(core::mem::size_of::<KernelError>() <= 2);
    }
}
