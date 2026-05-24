//! # `omni-container`
//!
//! `OmniContainer` is the **native micro-VM container engine** for OMNI OS,
//! specified by [`OIP-Container-006`](../../../oips/oip-container-006.md).
//! It runs Linux applications inside a Stichting-signed minimal guest
//! kernel, runs Windows applications via a pre-baked Wine guest image
//! (`omni/linux-wine:N-stable`), and exposes both classes of workload to
//! the OMNI capability model through a strict virtio-only host↔guest I/O
//! boundary.
//!
//! ## Status
//!
//! **Skeleton (v0.1).** This crate provides the public trait surface,
//! type definitions, the lifecycle state machine, and the capability
//! profile parser. Every operational method on [`engine::ContainerEngine`]
//! currently returns [`ContainerError::NotYetImplemented`] with a
//! PII-safe static context slug. Real implementations of the KVM /
//! TDX / SEV-SNP backends and the virtio host-side services land in
//! follow-up OIPs (one per major subsystem — engine, image, virtio,
//! attestation, profile).
//!
//! ## Design pillars (per `OIP-Container-006`)
//!
//! 1. **One container = one micro-VM.** No multi-container-per-VM
//!    (Kata-style shared-kernel pods are not supported in v1.x).
//! 2. **virtio-only I/O.** Every guest↔host data path is mediated by a
//!    capability-bound OMNI userspace backend; no PCI device passthrough
//!    in v1.x. Documented in [`virtio`] module.
//! 3. **Per-container TEE attestation.** On TDX / SEV-SNP capable hosts
//!    the container runs as a confidential VM by default; the host-side
//!    attestation quote covers the guest kernel hash, OCI image digest,
//!    and the granted capability set (see [`attestation`]).
//! 4. **Stichting-signed guest kernel only.** Users cannot ship their
//!    own guest kernel in v1.x. A future
//!    `OIP-Container-BYOLinux-XXX` lifts this for advanced users with
//!    explicit risk acknowledgement.
//! 5. **Capabilities are launch-time-bound and immutable.**
//!    Mid-lifetime capability expansion is denied; create a new
//!    container with the broader profile instead.
//!
//! ## Modules
//!
//! - [`engine`]      — the [`engine::ContainerEngine`] trait and the
//!                     hypervisor-backend abstraction (KVM / TDX /
//!                     SEV-SNP), per `OIP-Container-006` § 1.
//! - [`image`]       — [`image::OciImageRef`] newtype + OCI image
//!                     references and the OMNI extension manifest
//!                     parser, per `OIP-Container-006` § 7.
//! - [`lifecycle`]   — the seven-state container lifecycle state
//!                     machine and the transition-validation helpers,
//!                     per `OIP-Container-006` § 5.
//! - [`attestation`] — per-container quote generation surface bridging
//!                     [`omni_tee::TeeBackend`] to the container's own
//!                     measurement, per `OIP-Container-006` § 6.
//! - [`profile`]     — [`profile::CapabilityProfile`] enum + parser for
//!                     the five built-in capability profiles, per
//!                     `OIP-Container-006` § 4.
//! - [`virtio`]      — virtio device-backend trait skeletons (fs / net /
//!                     vsock / gpu / rng), per `OIP-Container-006` § 3.
//! - [`cli`]         — `omni-container` CLI argument types (`run`,
//!                     `run-windows`, `ps`), per `OIP-Container-006`
//!                     § 4 and § 8.
//!
//! ## Why `std` rather than `no_std + alloc`
//!
//! Unlike the foundational crates (`omni-types`, `omni-crypto`,
//! `omni-capability`, `omni-tee`, `omni-kernel`), `omni-container` runs
//! in userspace on the host OMNI OS: it needs `std::fs` (OCI image
//! cache), `std::process` (virtio-backend lifecycle), `std::sync`
//! primitives (engine-wide state), and `std::net` (REST API for the
//! management plane, plus mesh handshakes for cross-host offload).
//! Forcing `no_std + alloc` here would be premature optimization with
//! no benefit; `OIP-Container-006` § 1 box-diagram annotation explicitly
//! permits `std`.

#![doc(html_root_url = "https://docs.omni-os.org/omni-container")]
#![warn(missing_docs)]
// `clippy::literal_string_with_formatting_args` triggers a known false
// positive on the `=` banner comments at the top of `clippy.toml`
// itself (the lint diagnostic location points at the config file, not
// at any of this crate's source). The lint surface is preserved
// everywhere else in the workspace; we only suppress it here because
// the false-positive activates only on this crate's clippy invocation
// for reasons that look upstream-bug-shaped.
#![allow(clippy::literal_string_with_formatting_args)]

pub mod attestation;
pub mod cli;
pub mod console;
pub mod engine;
pub mod hypervisor;
pub mod image;
pub mod lifecycle;
pub mod profile;
pub mod virtio;

// -----------------------------------------------------------------------------
// Top-level re-exports
// -----------------------------------------------------------------------------
// Surface the most-used types at the crate root so consumers can write
// `use omni_container::{ContainerEngine, ContainerLifecycleState,
// CapabilityProfile, OciImageRef, ContainerError};` without navigating
// the module tree.

pub use console::ConsoleOutput;
pub use engine::ContainerEngine;
#[cfg(feature = "kvm")]
pub use engine::KvmEngine;
pub use hypervisor::{Hypervisor, MockHypervisor, VcpuExit, VcpuHandle, VmHandle};
pub use image::OciImageRef;
pub use lifecycle::{ContainerLifecycleState, TransitionError};
pub use profile::CapabilityProfile;

// -----------------------------------------------------------------------------
// Crate-wide error type
// -----------------------------------------------------------------------------

/// Top-level error type for the container engine.
///
/// Every operational path that has not yet been implemented returns
/// [`ContainerError::NotYetImplemented`] with a **PII-safe static
/// context slug** (e.g. `"engine::provision"`, `"virtio::fs::open"`).
/// The slug is intentionally a `&'static str` so it cannot leak runtime
/// state — only the call-site identifier. This mirrors the
/// `OmniError::context` convention enforced across the workspace
/// (`omni-types::OmniError`).
///
/// Real backends populate the `Backend`, `Capability`, `Image`,
/// `Lifecycle`, `Virtio`, and `Attestation` variants once the
/// corresponding OIPs are filed and merged.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// A code path that the v0.1 scaffold deliberately leaves as a
    /// stub. The static slug names the call site so reviewers can map
    /// failures back to the unimplemented surface without exposing any
    /// runtime detail.
    #[error("not yet implemented: {0}")]
    NotYetImplemented(&'static str),

    /// A hypervisor / hardware-backend failure (KVM ioctl, TDX
    /// attestation hardware path, SEV-SNP firmware path). The slug is
    /// the static call-site identifier.
    #[error("hypervisor backend error: {0}")]
    Backend(&'static str),

    /// A capability check failed at a virtio-backend boundary. The
    /// container attempted an operation outside the granted scope.
    #[error("capability denied: {0}")]
    Capability(&'static str),

    /// An OCI image could not be fetched, verified, or cached.
    #[error("OCI image error: {0}")]
    Image(&'static str),

    /// A container lifecycle transition was rejected (see
    /// [`lifecycle::TransitionError`]).
    #[error("lifecycle transition rejected: {0}")]
    Lifecycle(#[from] lifecycle::TransitionError),

    /// A virtio device backend failed.
    #[error("virtio backend error: {0}")]
    Virtio(&'static str),

    /// Per-container attestation could not be generated or verified.
    #[error("attestation error: {0}")]
    Attestation(&'static str),

    /// A capability profile string could not be parsed (see
    /// [`profile::ProfileParseError`]).
    #[error("profile parse error: {0}")]
    Profile(#[from] profile::ProfileParseError),
}

/// Convenience alias used throughout the crate.
pub type ContainerResult<T> = core::result::Result<T, ContainerError>;
