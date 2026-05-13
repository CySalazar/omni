//! `virtio-gpu` host-side backend trait — bridges to the OMNI tensor
//! HAL's GPU dispatch surface.
//!
//! See `OIP-Container-006` § 3.

use crate::{ContainerError, ContainerResult};

/// GPU access mode requested by the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuAccess {
    /// Shared (multiplexed) access to any GPU available on the host.
    Shared,
    /// Exclusive access to a specific GPU. The string is the
    /// host-local GPU identifier (e.g., `0`, `1`); a follow-up OIP
    /// formalizes the identifier shape.
    Exclusive,
}

/// virtio-gpu backend trait.
pub trait VirtioGpuBackend: Send + Sync {
    /// Provision a GPU context for a container.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Capability`] if the container's
    /// capability set does not grant `gpu:shared` or
    /// `gpu:exclusive:<id>`, [`ContainerError::Virtio`] for host-side
    /// device errors, or [`ContainerError::NotYetImplemented`] in
    /// the v0.1 scaffold.
    fn provision_context(&self, access: GpuAccess) -> ContainerResult<u64>;
}

/// v0.1 stub.
#[derive(Debug, Default)]
pub struct StubVirtioGpu;

impl VirtioGpuBackend for StubVirtioGpu {
    fn provision_context(&self, _access: GpuAccess) -> ContainerResult<u64> {
        Err(ContainerError::NotYetImplemented(
            "virtio::gpu::provision_context",
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stub_provision_context_returns_not_yet_implemented() {
        let b = StubVirtioGpu;
        let err = b.provision_context(GpuAccess::Shared).expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::gpu::provision_context")
        ));
    }
}
