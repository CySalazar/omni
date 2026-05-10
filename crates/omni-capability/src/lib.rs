//! # `omni-capability`
//!
//! Capability tokens for OMNI OS.
//!
//! Implements Macaroons-style cryptographic tokens that grant a
//! specific subject the right to perform a specific action on a
//! specific resource, for a bounded time window. Capabilities replace
//! traditional Unix permissions for AI workloads, where agents may
//! compose actions across many resources without re-authenticating
//! per call.
//!
//! ## Wire format
//!
//! Tokens are canonically encoded with `bincode` v2 in
//! fixed-int / no-trailing-data mode, then signed with `Ed25519`
//! ([`omni_crypto::signing`]). The pre-image of the signature is the
//! canonical encoding of the token's payload (every field except the
//! signature itself). Wire format details live in
//! `/docs/03-mesh-protocol.md` § "Capability tokens".
//!
//! ## Trust model
//!
//! A capability is valid iff:
//!
//! 1. The signature verifies under the issuer's public key.
//! 2. The current time is within `[not_before, not_after)`.
//! 3. The token is not in the revocation list ([`revocation`]).
//! 4. The TEE attestation of the calling node matches the
//!    `subject` `NodeId`.
//! 5. Every caveat applied along the attenuation chain holds (each
//!    [`attenuation::CaveatPredicate`] returns `true` for the
//!    requested action and resource).
//!
//! Items 1–3 are enforced in this crate. Item 4 is enforced via the
//! [`tee`] trait (placeholder; concrete `TeeBackend` impls land in
//! `omni-tee` per P5 in `/todo.md`). Item 5 is enforced by
//! [`attenuation::verify_chain_link`] together with the per-caveat
//! evaluator implementations the consumer registers.
//!
//! ## Modules
//!
//! - [`token`]       — token data structure, signing, verification.
//! - [`scope`]       — typed action × resource × time vocabulary.
//! - [`attenuation`] — Macaroons-style derivation of child tokens.
//! - [`revocation`]  — in-memory revocation list with bloom filter.
//! - [`tee`]         — trait surface for TEE attestation binding (P5).

#![doc(html_root_url = "https://docs.omni-os.org/omni-capability")]
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
// See `omni-types/src/lib.rs` for the rationale.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unnecessary_wraps,
        clippy::indexing_slicing,
    )
)]

extern crate alloc;

pub mod attenuation;
pub mod revocation;
pub mod scope;
pub mod tee;
pub mod token;

// Re-export the most-used items at the crate root for ergonomic imports.
pub use crate::scope::{Action, Caveat, Resource, Scope, TimeWindow};
pub use crate::token::{CapabilityToken, TokenPayload};
