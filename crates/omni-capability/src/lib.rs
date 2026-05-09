//! # `omni-capability`
//!
//! Capability tokens for OMNI OS.
//!
//! Implements Macaroons-style cryptographic tokens that grant a specific
//! actor the right to perform a specific action on a specific resource,
//! for a bounded period of time. Capabilities replace traditional Unix
//! permissions for AI workloads, where agents may compose actions across
//! many resources.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold only. Implementation arrives in Phase 1 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md).
//!
//! ## Design rationale
//!
//! - **Attenuable delegation**: a parent capability can produce a child
//!   capability with strictly more restrictive scope (Macaroons-style).
//!   This enables agent composition without privilege escalation.
//! - **Short TTL** (minutes by default): combined with a revocation list,
//!   allows fast revocation without long-lived state.
//! - **TEE-bound**: a capability is valid only when invoked from inside
//!   the TEE whose attestation the capability was minted against.
//! - **Master keys in TPM/Secure Enclave**: signing keys never leave the
//!   hardware root of trust.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! § "Capability-based access control".
//!
//! ## Modules
//!
//! - [`token`] — the capability token data structure and signing.
//! - [`scope`] — scope predicates (action × resource × time).
//! - [`attenuation`] — Macaroons-style derivation of attenuated tokens.
//! - [`revocation`] — revocation list management.

#![doc(html_root_url = "https://docs.omni-os.org/omni-capability")]
#![warn(missing_docs)]

/// The capability token data structure and signing routines.
pub mod token {
    // TODO(phase-1): define `CapabilityToken`, signing, verification.
}

/// Scope predicates: action × resource × time bounds.
pub mod scope {
    // TODO(phase-1): scope grammar and matching.
}

/// Macaroons-style derivation of attenuated tokens.
pub mod attenuation {
    // TODO(phase-1): derivation rules with strict monotonic restriction.
}

/// Revocation list management.
pub mod revocation {
    // TODO(phase-1): revocation list with short-TTL design.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
