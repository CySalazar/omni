//! Container engine trait — the central abstraction for the hypervisor
//! backend selected at build time (KVM / TDX / SEV-SNP).
//!
//! See `OIP-Container-006` § 1 ("Container model") and § 10 ("Reference
//! Implementation") for the rationale and the feature-gated backend
//! selection.
//!
//! ## Why a trait
//!
//! Three hypervisor backends are in scope for v1.x:
//!
//! - **KVM** (default): plain KVM-based micro-VM on VT-x / AMD-V. No
//!   confidentiality vs. the host kernel; suitable for non-TEE hardware
//!   and for development.
//! - **Intel TDX**: confidential VM (TD) on TDX-capable Xeon. Per-VM
//!   measurement; host kernel is outside the trust boundary.
//! - **AMD SEV-SNP**: confidential VM on Milan+ EPYC. Equivalent guarantees
//!   to TDX from a trust-boundary perspective.
//!
//! All three implement the same `ContainerEngine` trait. Consumers
//! (`omni-shell`, the management REST API, mesh peers offloading work)
//! depend only on the trait, not on the concrete backend.
//!
//! ## v0.1 status
//!
//! Every method on the trait returns
//! [`ContainerError::NotYetImplemented`] with a static call-site slug.
//! Real KVM ioctl wiring, TDX attestation
//! glue, and SEV-SNP firmware calls land in follow-up OIPs (one per
//! subsystem). The trait shape itself is the public commitment from
//! `OIP-Container-006`.

use crate::attestation::ContainerQuote;
use crate::image::OciImageRef;
use crate::lifecycle::ContainerLifecycleState;
use crate::profile::CapabilityProfile;
use crate::{ContainerError, ContainerResult};

/// Opaque, engine-assigned identifier for a single container instance.
///
/// The internal representation is a `u64` for engine-local
/// addressability; cross-host references use the mesh-level `NodeId` /
/// attested-channel identifiers, never the local `ContainerId`. Two
/// containers spawned on the same host MUST receive distinct
/// `ContainerId` values; `Default` returns the canonical
/// "uninitialized" sentinel (`0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ContainerId(pub u64);

impl ContainerId {
    /// Construct from a raw `u64`. Engine-internal use only.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Borrow the raw `u64`. Used by tracing / audit log code only.
    #[must_use]
    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

/// Launch-time specification for a new container.
///
/// Every field is required by `OIP-Container-006` § 4 (CLI surface) at
/// some level — either passed explicitly by the user or derived from
/// the selected capability profile. The struct is intentionally
/// non-exhaustive to allow follow-up OIPs to add fields (e.g.,
/// `--cpus`, `--memory`, `--snapshot-id`) without breaking the trait
/// surface.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ContainerSpec {
    /// OCI image to run (or the Wine guest image alias when invoked
    /// via `omni-container run-windows`).
    pub image: OciImageRef,
    /// Capability profile that determines the granted virtio scopes.
    pub profile: CapabilityProfile,
    /// Whether to require a confidential VM (TDX / SEV-SNP). If true
    /// and the host has no TEE, the engine returns
    /// [`ContainerError::Backend`] without booting the guest.
    pub tee_required: bool,
}

/// Vendor-neutral container engine surface.
///
/// Implementations:
/// - `KvmEngine` (default, gated by `kvm` feature) — placeholder in
///   v0.1; real implementation in a follow-up OIP.
/// - `TdxEngine` (gated by `tdx` feature) — Intel TDX confidential VM.
/// - `SevSnpEngine` (gated by `sev-snp` feature) — AMD SEV-SNP.
///
/// All methods are synchronous for v0.1 to keep the trait surface
/// trivially mockable. A follow-up OIP introduces an `async` variant
/// for the long-running `run` and `snapshot` paths once the runtime
/// (tokio vs. smol) is chosen.
pub trait ContainerEngine: Send + Sync {
    /// Provision a container from its spec. After this call returns
    /// `Ok`, the container is in
    /// [`ContainerLifecycleState::Provisioning`].
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Image`] if the OCI image cannot be
    /// fetched / verified, [`ContainerError::Capability`] if the
    /// requested profile cannot be satisfied by the host's grant set,
    /// or [`ContainerError::Backend`] if hypervisor setup fails.
    fn provision(&self, spec: ContainerSpec) -> ContainerResult<ContainerId>;

    /// Transition a provisioned container to
    /// [`ContainerLifecycleState::Running`].
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Lifecycle`] if the container is not
    /// in `Provisioning`, or [`ContainerError::Backend`] if the guest
    /// kernel fails to boot.
    fn run(&self, id: ContainerId) -> ContainerResult<()>;

    /// Pause a running container (`VMPAUSE` on TDX, equivalent on
    /// SEV-SNP / KVM). Memory remains in place; no CPU is consumed.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Lifecycle`] if the container is not
    /// in `Running`.
    fn suspend(&self, id: ContainerId) -> ContainerResult<()>;

    /// Resume a suspended container.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Lifecycle`] if the container is not
    /// in `Suspended`.
    fn resume(&self, id: ContainerId) -> ContainerResult<()>;

    /// Capture a container snapshot (memory + disk + vCPU state),
    /// sealed under the host TEE's current measurement. The container
    /// transitions to [`ContainerLifecycleState::Snapshotted`].
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Lifecycle`] if the container is not
    /// in `Suspended` (per `OIP-Container-006` § 5, snapshot is
    /// reached via Suspended → Snapshotted), or
    /// [`ContainerError::Attestation`] if the sealing operation
    /// fails.
    fn snapshot(&self, id: ContainerId) -> ContainerResult<()>;

    /// Terminate a container. Releases all hypervisor and virtio
    /// resources. The container transitions to
    /// [`ContainerLifecycleState::Terminated`] via `Terminating`.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Backend`] if resource release fails
    /// (best-effort cleanup is performed regardless).
    fn terminate(&self, id: ContainerId) -> ContainerResult<()>;

    /// Read-only query for the current lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Backend`] if `id` does not refer to
    /// a container managed by this engine.
    fn state(&self, id: ContainerId) -> ContainerResult<ContainerLifecycleState>;

    /// Generate a per-container attestation quote.
    ///
    /// The quote covers the host TEE measurement, the guest kernel
    /// hash, the OCI image digest, the granted capability set, and a
    /// verifier-supplied nonce, per `OIP-Container-006` § 6.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Attestation`] if the host has no TEE
    /// available or the underlying [`omni_tee::TeeBackend::attest`]
    /// call fails.
    fn attest(&self, id: ContainerId, nonce: &[u8]) -> ContainerResult<ContainerQuote>;
}

// -----------------------------------------------------------------------------
// v0.1 backend stubs — feature-gated
// -----------------------------------------------------------------------------

/// Placeholder KVM backend. Every method returns
/// [`ContainerError::NotYetImplemented`] in v0.1; real implementation
/// in a follow-up OIP wiring `kvm-ioctls`.
#[cfg(feature = "kvm")]
#[derive(Debug, Default)]
pub struct KvmEngine {
    _private: (),
}

#[cfg(feature = "kvm")]
impl KvmEngine {
    /// Construct a new placeholder KVM engine.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(feature = "kvm")]
impl ContainerEngine for KvmEngine {
    fn provision(&self, _spec: ContainerSpec) -> ContainerResult<ContainerId> {
        Err(ContainerError::NotYetImplemented("engine::kvm::provision"))
    }
    fn run(&self, _id: ContainerId) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("engine::kvm::run"))
    }
    fn suspend(&self, _id: ContainerId) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("engine::kvm::suspend"))
    }
    fn resume(&self, _id: ContainerId) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("engine::kvm::resume"))
    }
    fn snapshot(&self, _id: ContainerId) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("engine::kvm::snapshot"))
    }
    fn terminate(&self, _id: ContainerId) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("engine::kvm::terminate"))
    }
    fn state(&self, _id: ContainerId) -> ContainerResult<ContainerLifecycleState> {
        Err(ContainerError::NotYetImplemented("engine::kvm::state"))
    }
    fn attest(&self, _id: ContainerId, _nonce: &[u8]) -> ContainerResult<ContainerQuote> {
        Err(ContainerError::NotYetImplemented("engine::kvm::attest"))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;
    use crate::profile::CapabilityProfile;

    fn sample_spec() -> ContainerSpec {
        ContainerSpec {
            image: OciImageRef::parse("alpine:latest").expect("parses"),
            profile: CapabilityProfile::CliTool,
            tee_required: false,
        }
    }

    #[test]
    fn container_id_round_trip() {
        let raw = 0x1234_5678_9abc_def0_u64;
        let id = ContainerId::from_raw(raw);
        assert_eq!(id.as_raw(), raw);
    }

    #[test]
    fn container_id_default_is_zero() {
        assert_eq!(ContainerId::default().as_raw(), 0);
    }

    #[cfg(feature = "kvm")]
    #[test]
    fn kvm_engine_provision_returns_not_yet_implemented() {
        let engine = KvmEngine::new();
        let err = engine.provision(sample_spec()).expect_err("stub");
        match err {
            ContainerError::NotYetImplemented(slug) => {
                assert_eq!(slug, "engine::kvm::provision");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[cfg(feature = "kvm")]
    #[test]
    fn kvm_engine_lifecycle_methods_all_stubbed() {
        let engine = KvmEngine::new();
        let id = ContainerId::from_raw(1);
        for slug in [
            "engine::kvm::run",
            "engine::kvm::suspend",
            "engine::kvm::resume",
            "engine::kvm::snapshot",
            "engine::kvm::terminate",
            "engine::kvm::state",
        ] {
            let err = match slug {
                "engine::kvm::run" => engine.run(id).expect_err("stub"),
                "engine::kvm::suspend" => engine.suspend(id).expect_err("stub"),
                "engine::kvm::resume" => engine.resume(id).expect_err("stub"),
                "engine::kvm::snapshot" => engine.snapshot(id).expect_err("stub"),
                "engine::kvm::terminate" => engine.terminate(id).expect_err("stub"),
                "engine::kvm::state" => engine.state(id).map(|_| ()).expect_err("stub"),
                _ => unreachable!(),
            };
            match err {
                ContainerError::NotYetImplemented(s) => assert_eq!(s, slug),
                other => panic!("expected NotYetImplemented({slug}), got {other:?}"),
            }
        }
    }

    #[cfg(feature = "kvm")]
    #[test]
    fn kvm_engine_attest_returns_not_yet_implemented() {
        let engine = KvmEngine::new();
        let err = engine
            .attest(ContainerId::from_raw(1), b"nonce")
            .expect_err("stub");
        assert!(matches!(err, ContainerError::NotYetImplemented(_)));
    }
}
