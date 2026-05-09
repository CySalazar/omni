//! Encrypted-by-default data types.
//!
//! These types enforce — at the type system level — that personally
//! identifiable information (PII) cannot be handled in cleartext outside
//! an attested TEE.
//!
//! ## Planned types (Phase 2)
//!
//! - `EncryptedString` — opaque encrypted string; only readable inside a TEE.
//! - `MaskedSSN` — social security number with structural masking.
//! - `TokenizedEmail` — email address replaced with a deterministic token.
//! - `AttestedHash` — hash bound to a specific TEE attestation.
//!
//! ## Construction invariants
//!
//! Construction of any of these types must go through the tokenization
//! service in [`omni-tokenization`](../../../omni-tokenization). Direct
//! construction from cleartext is not exposed in the public API.
//!
//! See [`/docs/04-security-model.md`](../../../../docs/04-security-model.md)
//! § "The five privacy primitives in detail".

// TODO(phase-2): implement encrypted-by-default types per the privacy model.
