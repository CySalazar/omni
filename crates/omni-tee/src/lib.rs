//! # `omni-tee`
//!
//! Trusted Execution Environment abstractions for OMNI OS.
//!
//! TEE attestation is the hardware root of trust for OMNI OS. Mesh
//! participation is gated on producing a valid remote attestation report.
//! This crate exposes a vendor-neutral trait, plus concrete implementations
//! for Intel TDX and AMD SEV-SNP. Apple Secure Enclave and `ARMv9` CCA
//! Realms are planned for v1.1+.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold only. Implementation in Phase 1 (Intel TDX, AMD
//! SEV-SNP). Apple Silicon in Phase 5 (v1.1).
//!
//! ## Design rationale
//!
//! - **Vendor neutrality**: the rest of OMNI OS interacts with TEEs only
//!   through traits in [`traits`]. Adding a new TEE family requires
//!   implementing the trait, not changing callers.
//! - **No software fallback**: a node without a working TEE cannot
//!   participate in the mesh. This is a hard hardware requirement, not a
//!   degradable feature.
//! - **Attestation freshness**: attestation reports are short-lived;
//!   re-attestation is a cheap operation done frequently.
//! - **TEE diversity defense**: the mesh as a whole gains robustness from
//!   running on heterogeneous TEEs. A break of one vendor does not break
//!   the whole network.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! § "TEE compromise resistance" and
//! [`/docs/07-hardware-requirements.md`](../../../docs/07-hardware-requirements.md).
//!
//! ## Modules
//!
//! - [`traits`] — vendor-neutral TEE trait definitions.
//! - [`attestation`] — attestation report types and verification.
//! - [`sealed_keys`] — TEE-bound sealed key provisioning.
//! - [`tdx`] — Intel TDX implementation.
//! - [`sev_snp`] — AMD SEV-SNP implementation.

#![doc(html_root_url = "https://docs.omni-os.org/omni-tee")]
#![warn(missing_docs)]

/// Vendor-neutral TEE trait definitions.
pub mod traits {
    // TODO(phase-1): define `TrustedEnv` trait + supporting types.
}

/// Remote attestation report types and verification.
pub mod attestation {
    // TODO(phase-1): vendor-neutral `AttestationReport` + verifier.
}

/// TEE-bound sealed key provisioning.
pub mod sealed_keys {
    // TODO(phase-1): seal/unseal API tied to TEE measurements.
}

/// Intel TDX implementation.
pub mod tdx {
    // TODO(phase-1): TDX quote generation + verification.
}

/// AMD SEV-SNP implementation.
pub mod sev_snp {
    // TODO(phase-1): SEV-SNP attestation + verification.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
