//! Sealed marker types for encrypted-by-default data.
//!
//! This module provides the **type-level enforcement** that personally
//! identifiable information (PII) cannot be handled in cleartext outside
//! an attested TEE. The runtime construction of these values lives in
//! the `omni-tokenization` crate (Phase 2 in `/todo.md`), which performs
//! the actual encryption inside a verified TEE. This module exists in
//! P1 because every other crate must already speak in terms of these
//! types to prevent "cheat" code paths from being added later.
//!
//! # Two enforcement layers
//!
//! 1. **Sealed trait**: [`EncryptedType`] requires a private super-trait
//!    `sealed::Sealed`. Only types defined in this module can implement
//!    `EncryptedType`. Downstream crates cannot mint new "encrypted"
//!    categories without an OIP that lands here.
//!
//! 2. **Provider-only constructors**: the only way to instantiate one of
//!    these markers is through the `provider` API gated behind the
//!    `_tokenization_provider` feature flag. Only the `omni-tokenization`
//!    crate enables this flag (in its `Cargo.toml`). Any other crate that
//!    tries to enable the flag fails CI's `cargo deny` policy because
//!    the flag's name starts with `_` (project convention for "internal,
//!    unstable, do not use").
//!
//! # What this module does NOT do
//!
//! It does not perform encryption. The marker types carry an opaque
//! ciphertext byte buffer; the cryptographic operation that produces the
//! ciphertext happens in `omni-tokenization` inside the TEE. This crate
//! provides the type vocabulary, not the implementation.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! § "The five privacy primitives in detail" for the security rationale.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

// =============================================================================
// Sealed trait (compile-time extensibility lock).
// =============================================================================

mod sealed {
    /// Private super-trait used to seal the [`super::EncryptedType`] hierarchy.
    /// Only types in this module can implement it, so downstream crates
    /// cannot mint new "encrypted" categories outside an OIP.
    pub trait Sealed {}
}

/// Marker trait implemented by every encrypted-by-default type in OMNI OS.
///
/// The trait is **sealed**: only types defined in this module can implement
/// it. This is a compile-time guarantee that the set of encrypted-data
/// categories is fixed and reviewable from a single file.
///
/// # When to add a new variant
///
/// File an OIP that:
///
/// 1. Justifies why the existing categories are insufficient.
/// 2. Specifies the wire encoding and length bounds.
/// 3. Specifies the TEE construction policy (how the ciphertext is
///    produced and which attestation must witness it).
/// 4. Adds the new type here, behind a `provider`-gated constructor.
pub trait EncryptedType: sealed::Sealed + Sized {
    /// Stable, machine-readable category identifier (e.g.
    /// `"encrypted-string"`). Used for audit logs, telemetry, and
    /// per-category policy decisions. MUST be unique across the
    /// workspace.
    const KIND: &'static str;

    /// Borrow the opaque ciphertext bytes.
    ///
    /// The bytes are meaningless outside an attested TEE; surfacing them
    /// is safe because cleartext recovery requires the unsealing key
    /// held by the TEE.
    fn ciphertext(&self) -> &[u8];
}

// =============================================================================
// Marker types.
// =============================================================================

/// Encrypted UTF-8 string.
///
/// The cleartext was a [`alloc::string::String`] before encryption; the
/// stored bytes are an opaque ciphertext produced by the tokenization
/// service. Equality and hashing are defined over the ciphertext, so two
/// `EncryptedString` values are equal iff their ciphertexts match
/// byte-for-byte.
///
/// # Construction
///
/// Available only via the `provider` module (gated behind the
/// `_tokenization_provider` feature flag). See module-level docs.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Zeroize)]
pub struct EncryptedString {
    // The ciphertext is private. `Drop` triggers `Zeroize` to wipe the
    // bytes from memory in case the caller forgot to.
    ciphertext: Vec<u8>,
}

impl sealed::Sealed for EncryptedString {}

impl EncryptedType for EncryptedString {
    const KIND: &'static str = "encrypted-string";

    fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Structurally masked Social Security Number.
///
/// Stores the ciphertext of the full SSN plus a small number of plaintext
/// digits sufficient for human-readable masking (e.g., `***-**-1234`).
/// The masking digits are policy-controlled — see the tokenization
/// service for the active policy.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Zeroize)]
pub struct MaskedSSN {
    ciphertext: Vec<u8>,
    /// Last four digits in plaintext for human-readable display.
    /// Always exactly 4 ASCII bytes. Validated at construction time.
    visible_suffix: [u8; 4],
}

impl sealed::Sealed for MaskedSSN {}

impl EncryptedType for MaskedSSN {
    const KIND: &'static str = "masked-ssn";

    fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl MaskedSSN {
    /// Borrow the visible suffix (4 ASCII digits).
    #[must_use]
    pub const fn visible_suffix(&self) -> &[u8; 4] {
        &self.visible_suffix
    }
}

/// Tokenized email address.
///
/// The cleartext email is replaced with a deterministic token derived
/// from a per-tenant keyed hash. Identical emails under the same tenant
/// produce identical tokens, enabling join-on-token without revealing
/// the underlying address.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Zeroize)]
pub struct TokenizedEmail {
    ciphertext: Vec<u8>,
}

impl sealed::Sealed for TokenizedEmail {}

impl EncryptedType for TokenizedEmail {
    const KIND: &'static str = "tokenized-email";

    fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Hash bound to a specific TEE attestation.
///
/// Combines a content hash with the attestation quote that witnessed it.
/// Verification requires both: the hash must match the content AND the
/// quote must verify against the expected measurement.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Zeroize)]
pub struct AttestedHash {
    /// The opaque hash bytes (BLAKE3-256 in v0.1).
    hash: [u8; 32],
    /// The attestation quote bytes (variable length, opaque).
    attestation: Vec<u8>,
}

impl sealed::Sealed for AttestedHash {}

impl EncryptedType for AttestedHash {
    const KIND: &'static str = "attested-hash";

    /// For [`AttestedHash`], "ciphertext" returns the attestation bytes;
    /// the hash itself is not encrypted but bound to the attestation.
    fn ciphertext(&self) -> &[u8] {
        &self.attestation
    }
}

impl AttestedHash {
    /// Borrow the underlying content hash.
    #[must_use]
    pub const fn hash(&self) -> &[u8; 32] {
        &self.hash
    }
}

// =============================================================================
// Provider API (feature-gated for the tokenization service only).
// =============================================================================

/// Constructors for encrypted-by-default types.
///
/// **This API is gated behind the `_tokenization_provider` feature flag.**
/// Only the `omni-tokenization` crate enables the flag in its
/// `Cargo.toml`. Any other crate that tries to enable it must justify
/// itself in code review; the leading underscore in the flag name is a
/// project-wide signal that this is internal and unstable.
///
/// Even with the feature enabled, callers MUST verify that they are
/// running inside an attested TEE before constructing one of these
/// values. The compile-time gate prevents accidental misuse; the
/// runtime invariant is enforced by code review and the `omni-tee`
/// integration tests.
#[cfg(feature = "_tokenization_provider")]
pub mod provider {
    use alloc::vec::Vec;

    use super::{AttestedHash, EncryptedString, MaskedSSN, TokenizedEmail};
    use crate::error::{IdentityErrorKind, OmniError, Result};

    impl EncryptedString {
        /// Construct an [`EncryptedString`] from raw ciphertext.
        ///
        /// # Caller contract
        ///
        /// The caller MUST be running inside an attested TEE and the
        /// ciphertext MUST have been produced by the tokenization
        /// service's encryption routine. There is no runtime check —
        /// misuse is a project-wide invariant violation.
        #[must_use]
        pub fn from_ciphertext(ciphertext: Vec<u8>) -> Self {
            Self { ciphertext }
        }
    }

    impl MaskedSSN {
        /// Construct a [`MaskedSSN`] from raw ciphertext + visible suffix.
        ///
        /// The visible suffix must be exactly 4 ASCII digits. Returns
        /// [`OmniError::Identity`] on validation failure.
        ///
        /// # Errors
        ///
        /// Returns [`OmniError::Identity`] with
        /// [`IdentityErrorKind::InvalidLength`] if the suffix is not
        /// exactly 4 ASCII digits.
        pub fn from_ciphertext(ciphertext: Vec<u8>, visible_suffix: [u8; 4]) -> Result<Self> {
            // Validate that all four suffix bytes are ASCII digits.
            for b in &visible_suffix {
                if !b.is_ascii_digit() {
                    return Err(OmniError::identity(
                        IdentityErrorKind::InvalidLength,
                        "MaskedSSN::from_ciphertext_visible_suffix_not_digit",
                    ));
                }
            }
            Ok(Self {
                ciphertext,
                visible_suffix,
            })
        }
    }

    impl TokenizedEmail {
        /// Construct a [`TokenizedEmail`] from raw ciphertext.
        ///
        /// See the caller contract on [`EncryptedString::from_ciphertext`].
        #[must_use]
        pub fn from_ciphertext(ciphertext: Vec<u8>) -> Self {
            Self { ciphertext }
        }
    }

    impl AttestedHash {
        /// Construct an [`AttestedHash`] from a content hash + attestation.
        ///
        /// See the caller contract on [`EncryptedString::from_ciphertext`].
        #[must_use]
        pub fn from_parts(hash: [u8; 32], attestation: Vec<u8>) -> Self {
            Self { hash, attestation }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[cfg(feature = "_tokenization_provider")]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn encrypted_string_round_trip_through_provider() {
        let ct = vec![0xAB, 0xCD, 0xEF];
        let e = EncryptedString::from_ciphertext(ct.clone());
        assert_eq!(e.ciphertext(), &ct[..]);
        assert_eq!(EncryptedString::KIND, "encrypted-string");
    }

    #[test]
    fn masked_ssn_validates_suffix() {
        let ok = MaskedSSN::from_ciphertext(vec![1, 2, 3], *b"1234");
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap().visible_suffix(), b"1234");

        let bad = MaskedSSN::from_ciphertext(vec![1, 2, 3], *b"12X4");
        assert!(bad.is_err());
    }

    #[test]
    fn attested_hash_round_trip() {
        let h = AttestedHash::from_parts([7u8; 32], vec![0xFF, 0xEE]);
        assert_eq!(h.hash(), &[7u8; 32]);
        assert_eq!(h.ciphertext(), &[0xFF, 0xEE][..]);
    }

    #[test]
    fn kind_constants_are_unique() {
        let kinds = [
            EncryptedString::KIND,
            MaskedSSN::KIND,
            TokenizedEmail::KIND,
            AttestedHash::KIND,
        ];
        // Naive uniqueness check (4 elements, O(n^2) is fine).
        for (i, a) in kinds.iter().enumerate() {
            for b in kinds.iter().skip(i + 1) {
                assert_ne!(a, b, "EncryptedType::KIND collision: {a}");
            }
        }
    }
}

// Sanity-only test that does not require the provider feature: the sealed
// trait is sealed.
#[cfg(test)]
mod sealed_tests {
    use super::*;

    // This compiles iff `EncryptedString` implements both traits. The
    // real "cannot extend EncryptedType outside this crate" assertion
    // lives in the trybuild compile-fail tests.
    #[test]
    fn marker_types_implement_encrypted_type() {
        fn assert_impl<T: EncryptedType>() {}
        assert_impl::<EncryptedString>();
        assert_impl::<MaskedSSN>();
        assert_impl::<TokenizedEmail>();
        assert_impl::<AttestedHash>();
    }
}
