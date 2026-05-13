//! `virtio-net` host-side backend trait.
//!
//! See `OIP-Container-006` § 3. The host-side service runs per-channel
//! firewall rules based on the container's
//! `net:outbound:<host>:<port>` / `net:inbound:<port>` capabilities.

use crate::{ContainerError, ContainerResult};

/// Direction tag for a network flow opened by the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowDirection {
    /// Container-initiated connection to a remote host.
    Outbound,
    /// Listener accepting connections from the host network.
    Inbound,
}

/// virtio-net backend trait.
pub trait VirtioNetBackend: Send + Sync {
    /// Open a TCP / UDP flow against the host network stack.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Capability`] if the firewall rule
    /// for `direction:host:port` is not granted by the container's
    /// capability set, [`ContainerError::Virtio`] for network errors,
    /// or [`ContainerError::NotYetImplemented`] in the v0.1 scaffold.
    fn open_flow(&self, direction: FlowDirection, host: &str, port: u16) -> ContainerResult<u64>;
}

/// v0.1 stub backend.
#[derive(Debug, Default)]
pub struct StubVirtioNet;

impl VirtioNetBackend for StubVirtioNet {
    fn open_flow(
        &self,
        _direction: FlowDirection,
        _host: &str,
        _port: u16,
    ) -> ContainerResult<u64> {
        Err(ContainerError::NotYetImplemented("virtio::net::open_flow"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stub_open_flow_returns_not_yet_implemented() {
        let b = StubVirtioNet;
        let err = b
            .open_flow(FlowDirection::Outbound, "huggingface.co", 443)
            .expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::net::open_flow")
        ));
    }
}
