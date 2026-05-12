//! `virtio-rng` host-side backend — bridges guest entropy requests to
//! the kernel CSPRNG.
//!
//! See `OIP-Container-006` § 3. The `virtio-rng` capability is
//! **always granted** to every container; entropy is a non-rivalrous
//! resource and starving a container of randomness is an
//! availability hazard with no defensive value.

use crate::{ContainerError, ContainerResult};

/// virtio-rng backend trait.
pub trait VirtioRngBackend: Send + Sync {
    /// Fill the caller's buffer with cryptographically-strong random
    /// bytes from the host CSPRNG (`getrandom`).
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Virtio`] for host-side errors or
    /// [`ContainerError::NotYetImplemented`] in the v0.1 scaffold.
    fn fill(&self, buf: &mut [u8]) -> ContainerResult<()>;
}

/// v0.1 stub.
#[derive(Debug, Default)]
pub struct StubVirtioRng;

impl VirtioRngBackend for StubVirtioRng {
    fn fill(&self, _buf: &mut [u8]) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("virtio::rng::fill"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stub_fill_returns_not_yet_implemented() {
        let b = StubVirtioRng;
        let mut buf = [0u8; 16];
        let err = b.fill(&mut buf).expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::rng::fill")
        ));
    }
}
