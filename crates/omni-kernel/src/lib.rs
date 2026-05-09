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
//! Draft v0.1 — scaffold only. The kernel is a placeholder library that
//! compiles cleanly in the workspace. Phase 1 (months 6–18) replaces this
//! with a `no_std` bare-metal kernel with bootloader integration. See
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md) § "Phase 1".
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
//! - [`capabilities`] — capability validation and minting.
//! - [`syscall`] — system call dispatch.

#![doc(html_root_url = "https://docs.omni-os.org/omni-kernel")]
#![warn(missing_docs)]

// TODO(phase-1): convert to `no_std` and add `#![no_main]` for bare-metal.

/// Virtual memory, page tables, and allocators.
pub mod memory {
    // TODO(phase-1): page table management, virtual memory subsystem.
}

/// Process and thread scheduling.
pub mod scheduling {
    // TODO(phase-1): scheduler with thermal-aware and AI-workload-aware
    // policies.
}

/// Inter-process communication primitives.
pub mod ipc {
    // TODO(phase-1): typed message passing IPC.
}

/// Capability validation and minting.
pub mod capabilities {
    // TODO(phase-1): kernel-side capability handling.
}

/// System call dispatch.
pub mod syscall {
    // TODO(phase-1): syscall table + dispatch.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
