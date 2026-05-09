//! # `omni-types`
//!
//! Shared core types for OMNI OS.
//!
//! This crate defines the foundational types used across the rest of the
//! OMNI OS workspace. It sits at the bottom of the dependency tree: every
//! other crate may depend on `omni-types`, but `omni-types` depends on
//! nothing internal.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold only. Implementation arrives in Phase 1 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md). Modules currently
//! contain only documentation describing their intended contents.
//!
//! ## Design rationale
//!
//! Encrypted-by-default types follow a "make-illegal-states-unrepresentable"
//! philosophy. A function cannot accidentally accept plaintext PII when its
//! signature requires an `EncryptedString`, because the only way to construct
//! one is through the tokenization service running inside an attested TEE.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! for the security rationale and
//! [`/docs/02-architecture.md`](../../../docs/02-architecture.md) for the
//! architectural context.
//!
//! ## Modules
//!
//! - [`encrypted`] — Encrypted-by-default data types.
//! - [`identity`] — Node, agent, model, and capability identifiers.
//! - [`error`] — Common error types used across the workspace.
//! - [`version`] — Semantic version + protocol version helpers.

#![doc(html_root_url = "https://docs.omni-os.org/omni-types")]
#![warn(missing_docs)]

// TODO(phase-1): re-enable `no_std` once kernel scaffolding stabilizes.
// #![no_std]
// extern crate alloc;

pub mod encrypted;
pub mod error;
pub mod identity;
pub mod version;

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles in the workspace.
    /// Real tests are added per module as implementations land.
    #[test]
    fn placeholder() {}
}
