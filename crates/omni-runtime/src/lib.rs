//! # `omni-runtime`
//!
//! AI Runtime Service for OMNI OS.
//!
//! The privileged user-space service that exposes AI as a system primitive.
//! Applications call into the runtime through capability-checked syscalls;
//! the runtime owns model lifecycle, inference scheduling, and decisions
//! about which execution tier handles each workload.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Implementation arrives in Phase 2 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md).
//!
//! ## Design rationale
//!
//! - **Capability-checked entry points**: every public function accepts a
//!   capability token; invalid tokens are rejected at the API boundary.
//! - **Tier routing**: the runtime decides whether a given workload is
//!   served by Tier 0 (local), Tier 1 (personal cluster), Tier 2 (mesh),
//!   or Tier 3 (commercial cloud), based on workload sensitivity, user
//!   policy, and available resources. See
//!   [`/docs/02-architecture.md`](../../../docs/02-architecture.md)
//!   § "Execution tiers".
//! - **Model attestation enforced**: a model whose signature does not
//!   verify is rejected at load time. No exceptions.
//! - **Audit log**: every invocation produces a structured record. See
//!   [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//!   § "Audit log".
//!
//! ## Modules
//!
//! - [`model`] — model lifecycle (load, unload, attest, version).
//! - [`inference`] — inference orchestration on the local node.
//! - [`scheduler`] — workload scheduling across accelerators.
//! - [`router`] — execution tier routing decisions.
//! - [`attestation`] — model signature verification.

#![doc(html_root_url = "https://docs.omni-os.org/omni-runtime")]
#![warn(missing_docs)]

/// Model lifecycle: load, unload, attest, version.
pub mod model {
    // TODO(phase-2): model registry + lifecycle.
}

/// Inference orchestration on the local node.
pub mod inference {
    // TODO(phase-2): inference pipeline glue.
}

/// Workload scheduling across accelerators.
pub mod scheduler {
    // TODO(phase-2): scheduler with cost model + thermal awareness.
}

/// Execution tier routing decisions.
pub mod router {
    // TODO(phase-2): tier router with policy engine.
}

/// Model signature verification (Sigstore-style).
pub mod attestation {
    // TODO(phase-2): model manifest verification.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
