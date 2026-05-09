//! # `omni-crypto`
//!
//! Cryptographic primitives for OMNI OS.
//!
//! This crate wraps battle-tested cryptographic libraries (`ring`,
//! `RustCrypto`, `dalek`, `arkworks`) behind OMNI-specific traits and
//! provides higher-level constructions used elsewhere in the workspace
//! (compliance proofs, encrypted-by-default types, capability signing).
//!
//! ## Status
//!
//! Draft v0.1 — scaffold only. Implementation arrives in Phase 1–4 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md). zk-SNARK predicate
//! support is targeted for Phase 4 (mesh release).
//!
//! ## Design rationale
//!
//! 1. **Use libraries; do not write crypto.** All primitives delegate to
//!    well-reviewed implementations. This crate provides the composition
//!    layer, not new algorithms.
//! 2. **Strict error types.** Cryptographic failures must never silently
//!    succeed. Errors are explicit and untyped failures are not allowed.
//! 3. **Constant-time where applicable.** Side-channel-aware coding for
//!    private-key operations. See
//!    [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//!    § "Side channels".
//! 4. **Algorithm agility.** Cipher suites are negotiated, not hardcoded.
//!    Sunset dates for deprecated algorithms are tracked at the protocol
//!    level. Post-quantum migration roadmap targets 2030.
//!
//! ## Modules
//!
//! - [`aead`] — Authenticated Encryption with Associated Data (ChaCha20-Poly1305).
//! - [`signing`] — Digital signatures (Ed25519).
//! - [`kex`] — Key exchange (X25519, hybrid PQ in Phase 4+).
//! - [`hash`] — Cryptographic hashes (SHA-2, SHA-3, BLAKE3).
//! - [`kdf`] — Key derivation functions (HKDF, Argon2).
//! - [`fpe`] — Format-preserving encryption (FF1, FF3-1).
//! - [`snark`] — Zero-knowledge predicates for compliance proofs.

#![doc(html_root_url = "https://docs.omni-os.org/omni-crypto")]
#![warn(missing_docs)]

/// Authenticated Encryption with Associated Data (ChaCha20-Poly1305).
pub mod aead {
    // TODO(phase-1): wrap `chacha20poly1305` with OMNI-specific key types.
}

/// Digital signatures (Ed25519).
pub mod signing {
    // TODO(phase-1): wrap `ed25519-dalek` with OMNI signing API.
}

/// Key exchange (X25519, hybrid PQ in Phase 4+).
pub mod kex {
    // TODO(phase-1): wrap `x25519-dalek`. Phase 4: hybrid Kyber.
}

/// Cryptographic hashes (SHA-2, SHA-3, BLAKE3).
pub mod hash {
    // TODO(phase-1): unified hash trait + impls.
}

/// Key derivation functions (HKDF, Argon2).
pub mod kdf {
    // TODO(phase-1): HKDF for protocol session keys, Argon2 for user secrets.
}

/// Format-preserving encryption (FF1, FF3-1).
pub mod fpe {
    // TODO(phase-4): FF1/FF3-1 implementations for routing metadata.
}

/// Zero-knowledge predicates for compliance proofs.
pub mod snark {
    // TODO(phase-4): zk-SNARK / STARK construction for compliance proofs.
    // Trusted-setup avoidance is a hard requirement; favor STARK or
    // transparent constructions where feasible.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
