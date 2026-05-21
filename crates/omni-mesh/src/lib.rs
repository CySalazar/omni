//! # `omni-mesh`
//!
//! Federated mesh protocol implementation for OMNI OS.
//!
//! Implements the peer-to-peer mesh that provides Tier 2 collective
//! compute. Specification lives in
//! [`/docs/03-mesh-protocol.md`](../../../docs/03-mesh-protocol.md);
//! this crate is the Rust implementation that conforms to it.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Implementation arrives in Phase 4 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md). v1 release ships
//! with this crate's first stable interfaces.
//!
//! ## Design rationale
//!
//! - **Privacy by construction**: every payload carries a compliance proof
//!   and a TEE-only decryption envelope. Honest nodes (which is every
//!   node running this crate) reject malformed payloads. A non-compliant
//!   fork cannot pollute the mesh.
//! - **No central authority at runtime**: discovery is via Kademlia DHT;
//!   routing is locally decided per node; reputation is computed locally.
//! - **TEE attestation as identity**: a node's identity is its TEE
//!   attestation. Datacenter-cloning attacks are blocked at the
//!   attestation chain level.
//! - **MoE-friendly routing**: per-token expert dispatch with minimal
//!   cross-node traffic.
//!
//! ## Modules
//!
//! - [`discovery`] — Kademlia DHT peer discovery.
//! - [`transport`] — QUIC + Noise transport layer.
//! - [`attestation`] — peer attestation handshake.
//! - [`routing`] — workload routing across peers.
//! - [`credits`] — compute credit ledger (gossip-replicated).
//! - [`reputation`] — local reputation scoring.
//! - [`compliance_proof`] — compliance proof envelope handling.

#![doc(html_root_url = "https://docs.omni-os.org/omni-mesh")]
#![warn(missing_docs)]

/// Kademlia DHT peer discovery.
pub mod discovery {
    // TODO(phase-4): DHT implementation, bootstrap protocol.
}

/// QUIC + Noise transport layer.
pub mod transport {
    // TODO(phase-4): QUIC streams with Noise_XX handshake.
    // TODO(TASK-022): align wire format to postcard 1.0
    //   per OIP-Serde-004; current bincode 2.0 usage is a
    //   documented gap, see docs/03-mesh-protocol.md:197.
}

/// Peer attestation handshake.
pub mod attestation {
    // TODO(phase-4): mutual TEE attestation as part of handshake.
}

/// Workload routing across peers.
pub mod routing {
    // TODO(phase-4): per-token MoE expert routing.
}

/// Compute credit ledger (gossip-replicated, signed).
pub mod credits {
    // TODO(phase-4): tit-for-tat credit ledger with anti-Sybil bootstrap.
}

/// Local reputation scoring.
pub mod reputation {
    // TODO(phase-4): deterministic reputation algorithm + gossip.
}

/// Compliance proof envelope handling.
pub mod compliance_proof {
    // TODO(phase-4): generation + verification of compliance proofs.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
