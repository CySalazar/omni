//! `virtio-vsock` host-side bridge to the OMNI IPC layer.
//!
//! See `OIP-Container-006` § 3.

use crate::{ContainerError, ContainerResult};

/// virtio-vsock backend trait — bridges the guest vsock to an OMNI
/// IPC channel.
pub trait VirtioVsockBackend: Send + Sync {
    /// Connect the guest end of a vsock to an OMNI IPC channel by id.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Capability`] if the container does
    /// not hold `ipc:channel:<id>`, [`ContainerError::Virtio`] for
    /// transport errors, or [`ContainerError::NotYetImplemented`]
    /// in the v0.1 scaffold.
    fn connect_channel(&self, channel_id: &str) -> ContainerResult<u64>;
}

/// v0.1 stub.
#[derive(Debug, Default)]
pub struct StubVirtioVsock;

impl VirtioVsockBackend for StubVirtioVsock {
    fn connect_channel(&self, _channel_id: &str) -> ContainerResult<u64> {
        Err(ContainerError::NotYetImplemented(
            "virtio::vsock::connect_channel",
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stub_connect_channel_returns_not_yet_implemented() {
        let b = StubVirtioVsock;
        let err = b.connect_channel("inference").expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::vsock::connect_channel")
        ));
    }
}
