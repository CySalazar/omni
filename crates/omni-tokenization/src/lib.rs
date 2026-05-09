//! # `omni-tokenization`
//!
//! PII tokenization service for OMNI OS.
//!
//! Replaces personally identifiable information (PII) with deterministic
//! tokens before any inference workload leaves the user's TEE. The
//! mapping between PII and tokens lives in a per-user vault inside the
//! TEE; the model only ever sees tokens, never raw PII.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Implementation arrives in Phase 2 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md).
//!
//! ## Design rationale
//!
//! - **Local-only by construction**: tokenization runs inside the user's
//!   TEE. The vault never leaves the device; remote nodes see only tokens.
//! - **Deterministic tokens for the user, scrambled across sessions**:
//!   within a session the same PII produces the same token (so the model
//!   can reason about co-reference). Across sessions, tokens are
//!   re-scrambled to prevent linkability.
//! - **NER classifier on-device**: PII spans are detected by a small
//!   local model. False negatives are conservative — when in doubt, the
//!   data is treated as PII.
//! - **De-tokenization happens locally**: model responses containing
//!   tokens are de-tokenized inside the TEE on the user's device.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! § "Tokenization service".
//!
//! ## Modules
//!
//! - [`ner`] — Named Entity Recognition for PII spans.
//! - [`vault`] — per-user token vault inside TEE.
//! - [`policy`] — policy for what counts as PII (configurable per
//!   regulatory regime: GDPR, HIPAA, etc.).
//! - [`types`] — request / response types for the tokenization API.

#![doc(html_root_url = "https://docs.omni-os.org/omni-tokenization")]
#![warn(missing_docs)]

/// Named Entity Recognition for PII spans.
pub mod ner {
    // TODO(phase-2): on-device NER classifier integration.
}

/// Per-user token vault inside TEE.
pub mod vault {
    // TODO(phase-2): TEE-resident token vault with sealed storage.
}

/// Policy for what counts as PII.
pub mod policy {
    // TODO(phase-2): GDPR, HIPAA, PCI policies as configurable presets.
}

/// Request / response types for the tokenization API.
pub mod types {
    // TODO(phase-2): tokenize / detokenize request types.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
