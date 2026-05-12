//! Capability token data structure, signing, and verification.
//!
//! # Wire format
//!
//! A [`CapabilityToken`] is the pair `(TokenPayload, OmniSignature)`.
//! The signature covers the canonical encoding of the payload, where
//! "canonical" means:
//!
//! * `postcard` 1.x with default options: LEB128 varints, length-prefixed
//!   sequences/strings, COBS framing on output. The encoding is canonical
//!   (one byte sequence per value) under `OIP-Serde-004`.
//! * Field order is the textual order in [`TokenPayload`]; do not
//!   reorder fields in this file without a wire-format major bump.
//! * `Vec`s and `String`s carry a varint length prefix; enum variants
//!   carry their `serde` discriminant as a varint tag.
//!
//! These rules ensure two encoders on different platforms produce
//! byte-identical pre-images, which is the security-critical invariant
//! for signature verification.
//!
//! Per `OIP-Serde-004` M2, all encode/decode flow through
//! [`omni_types::wire`] — never call `postcard::*` directly. The
//! workspace clippy `disallowed-methods` lint enforces this.

use alloc::vec::Vec;

use omni_crypto::signing::{OmniSignature, OmniSigningKey, OmniVerifyingKey};
use omni_types::error::{CapabilityErrorKind, OmniError, Result};
use omni_types::identity::{CapabilityId, NodeId};
use omni_types::wire;
use serde::{Deserialize, Serialize};

use crate::scope::Scope;
use crate::tee::AttestationSource;

// =============================================================================
// TokenPayload
// =============================================================================

/// The signed body of a capability token.
///
/// Field order is the wire order. Do not reorder.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TokenPayload {
    /// Identifier of this capability. Used as the lookup key in the
    /// revocation list.
    pub id: CapabilityId,

    /// `NodeId` of the subject this capability is bound to. The
    /// calling node's TEE attestation MUST match this value at use
    /// time, otherwise verification fails.
    pub subject: NodeId,

    /// Public key of the issuer. Verifying the signature requires
    /// this key. Embedding the public key in the payload (rather
    /// than relying on an out-of-band lookup) keeps every token
    /// self-contained — handy for mesh peers that cannot do a
    /// synchronous DHT lookup mid-handshake.
    pub issuer: OmniVerifyingKey,

    /// Identifier of the parent token in the attenuation chain.
    /// `None` for root tokens minted directly by the issuer.
    pub parent: Option<CapabilityId>,

    /// The authority granted by this capability.
    pub scope: Scope,
}

impl TokenPayload {
    /// Encode this payload into the canonical byte representation
    /// used as the signature pre-image.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Capability`] with
    /// [`CapabilityErrorKind::MalformedToken`] if encoding fails
    /// (which only happens on out-of-memory or truly broken `Serde`
    /// impls — practically infallible for our types).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        wire::encode_canonical(self).map_err(|_| {
            OmniError::capability(
                CapabilityErrorKind::MalformedToken,
                "token::canonical_bytes::encode",
            )
        })
    }
}

// =============================================================================
// CapabilityToken
// =============================================================================

/// A signed capability token: payload + Ed25519 signature.
///
/// Construct via [`CapabilityToken::mint`]. Verify via
/// [`CapabilityToken::verify_signature`] (signature only) or
/// [`CapabilityToken::verify_full`] (signature + time + TEE binding).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// The signed body. Holds everything except the signature itself.
    pub payload: TokenPayload,
    /// Ed25519 signature over the canonical encoding of `payload`.
    pub signature: OmniSignature,
}

impl CapabilityToken {
    /// Mint a new capability token.
    ///
    /// The caller provides the issuer's signing key; the public key
    /// is derived from it and embedded in the payload. This keeps
    /// the API simple and prevents accidentally embedding the wrong
    /// public key.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Capability`] with
    /// [`CapabilityErrorKind::MalformedToken`] only if canonical
    /// encoding fails (see [`TokenPayload::canonical_bytes`]).
    pub fn mint(
        issuer_key: &OmniSigningKey,
        subject: NodeId,
        scope: Scope,
        parent: Option<CapabilityId>,
    ) -> Result<Self> {
        let payload = TokenPayload {
            id: CapabilityId::new(),
            subject,
            issuer: issuer_key.verifying_key(),
            parent,
            scope,
        };
        let bytes = payload.canonical_bytes()?;
        let signature = issuer_key.sign(&bytes);
        Ok(Self { payload, signature })
    }

    /// Re-sign an arbitrary payload. Used by attenuation, which builds
    /// the child payload from the parent and then signs.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Capability`] with
    /// [`CapabilityErrorKind::MalformedToken`] on encoding failure.
    pub fn sign_payload(issuer_key: &OmniSigningKey, payload: TokenPayload) -> Result<Self> {
        let bytes = payload.canonical_bytes()?;
        let signature = issuer_key.sign(&bytes);
        Ok(Self { payload, signature })
    }

    /// Verify the token's signature against the issuer public key
    /// embedded in its payload.
    ///
    /// This is the cheap, stateless half of token verification. Use
    /// [`CapabilityToken::verify_full`] when you also need to check
    /// time, TEE binding, and revocation status.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Capability`] with
    /// [`CapabilityErrorKind::InvalidSignature`] on signature failure
    /// or [`CapabilityErrorKind::MalformedToken`] on encoding failure.
    pub fn verify_signature(&self) -> Result<()> {
        let bytes = self.payload.canonical_bytes()?;
        self.payload
            .issuer
            .verify(&bytes, &self.signature)
            .map_err(|_| {
                OmniError::capability(
                    CapabilityErrorKind::InvalidSignature,
                    "token::verify_signature",
                )
            })
    }

    /// Full verification: signature, time window, TEE binding, and
    /// revocation status.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Capability`] with the most specific
    /// failure kind that applies (`InvalidSignature`, `Expired`,
    /// `NotYetValid`, `AttestationMismatch`, or `Revoked`).
    pub fn verify_full(
        &self,
        now: u64,
        attestation: &dyn AttestationSource,
        revocation: &crate::revocation::RevocationList,
    ) -> Result<()> {
        // 1. Signature.
        self.verify_signature()?;

        // 2. Revocation. We check this before the time window so a
        //    revoked but still-in-window token reports `Revoked`,
        //    which is the more actionable error.
        if revocation.contains(&self.payload.id) {
            return Err(OmniError::capability(
                CapabilityErrorKind::Revoked,
                "token::verify_full::revocation",
            ));
        }

        // 3. Time window.
        if now < self.payload.scope.window.not_before {
            return Err(OmniError::capability(
                CapabilityErrorKind::NotYetValid,
                "token::verify_full::not_before",
            ));
        }
        if now >= self.payload.scope.window.not_after {
            return Err(OmniError::capability(
                CapabilityErrorKind::Expired,
                "token::verify_full::not_after",
            ));
        }

        // 4. TEE binding.
        let local_node = attestation.current_node_id().map_err(|_| {
            OmniError::capability(
                CapabilityErrorKind::AttestationMismatch,
                "token::verify_full::attestation_unavailable",
            )
        })?;
        if local_node != self.payload.subject {
            return Err(OmniError::capability(
                CapabilityErrorKind::AttestationMismatch,
                "token::verify_full::subject_mismatch",
            ));
        }

        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revocation::RevocationList;
    use crate::scope::{Action, Resource, TimeWindow};
    use crate::tee::StubAttestation;

    fn fresh_scope() -> Scope {
        Scope {
            action: Action::Read,
            resource: Resource::Any,
            window: TimeWindow::new(100, 200).unwrap(),
            caveats: alloc::vec::Vec::new(),
        }
    }

    fn fresh_node() -> NodeId {
        NodeId::from_attestation_hash([0xAA; 32])
    }

    #[test]
    fn mint_and_verify_signature_round_trip() {
        let sk = OmniSigningKey::generate();
        let token = CapabilityToken::mint(&sk, fresh_node(), fresh_scope(), None).unwrap();
        token.verify_signature().unwrap();
    }

    #[test]
    fn tampered_payload_breaks_signature() {
        let sk = OmniSigningKey::generate();
        let mut token = CapabilityToken::mint(&sk, fresh_node(), fresh_scope(), None).unwrap();
        // Mutate the scope; the signature was computed over the
        // original payload, so verification must now fail.
        token.payload.scope.window = TimeWindow::new(0, u64::MAX).unwrap();
        let err = token.verify_signature().unwrap_err();
        match err {
            OmniError::Capability { kind, .. } => {
                assert_eq!(kind, CapabilityErrorKind::InvalidSignature);
            }
            _ => panic!("expected Capability::InvalidSignature"),
        }
    }

    #[test]
    fn verify_full_succeeds_inside_window() {
        let sk = OmniSigningKey::generate();
        let node = fresh_node();
        let token = CapabilityToken::mint(&sk, node, fresh_scope(), None).unwrap();
        let attest = StubAttestation {
            fixed_node_id: node,
        };
        let rev = RevocationList::new();
        token.verify_full(150, &attest, &rev).unwrap();
    }

    #[test]
    fn verify_full_rejects_before_nbf() {
        let sk = OmniSigningKey::generate();
        let node = fresh_node();
        let token = CapabilityToken::mint(&sk, node, fresh_scope(), None).unwrap();
        let attest = StubAttestation {
            fixed_node_id: node,
        };
        let rev = RevocationList::new();
        let err = token.verify_full(50, &attest, &rev).unwrap_err();
        match err {
            OmniError::Capability { kind, .. } => {
                assert_eq!(kind, CapabilityErrorKind::NotYetValid);
            }
            _ => panic!("expected Capability::NotYetValid"),
        }
    }

    #[test]
    fn verify_full_rejects_after_exp() {
        let sk = OmniSigningKey::generate();
        let node = fresh_node();
        let token = CapabilityToken::mint(&sk, node, fresh_scope(), None).unwrap();
        let attest = StubAttestation {
            fixed_node_id: node,
        };
        let rev = RevocationList::new();
        let err = token.verify_full(200, &attest, &rev).unwrap_err();
        match err {
            OmniError::Capability { kind, .. } => {
                assert_eq!(kind, CapabilityErrorKind::Expired);
            }
            _ => panic!("expected Capability::Expired"),
        }
    }

    #[test]
    fn verify_full_rejects_attestation_mismatch() {
        let sk = OmniSigningKey::generate();
        let node = fresh_node();
        let token = CapabilityToken::mint(&sk, node, fresh_scope(), None).unwrap();
        let other_node = NodeId::from_attestation_hash([0xBB; 32]);
        let attest = StubAttestation {
            fixed_node_id: other_node,
        };
        let rev = RevocationList::new();
        let err = token.verify_full(150, &attest, &rev).unwrap_err();
        match err {
            OmniError::Capability { kind, .. } => {
                assert_eq!(kind, CapabilityErrorKind::AttestationMismatch);
            }
            _ => panic!("expected Capability::AttestationMismatch"),
        }
    }

    #[test]
    fn verify_full_rejects_revoked() {
        let sk = OmniSigningKey::generate();
        let node = fresh_node();
        let token = CapabilityToken::mint(&sk, node, fresh_scope(), None).unwrap();
        let attest = StubAttestation {
            fixed_node_id: node,
        };
        let mut rev = RevocationList::new();
        rev.revoke(token.payload.id);
        let err = token.verify_full(150, &attest, &rev).unwrap_err();
        match err {
            OmniError::Capability { kind, .. } => assert_eq!(kind, CapabilityErrorKind::Revoked),
            _ => panic!("expected Capability::Revoked"),
        }
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        // Same payload -> same bytes. This is the security-critical
        // invariant for signature pre-images.
        let payload = TokenPayload {
            id: CapabilityId::from_bytes([1u8; 16]),
            subject: NodeId::from_attestation_hash([2u8; 32]),
            issuer: OmniSigningKey::from_bytes([3u8; 32]).verifying_key(),
            parent: None,
            scope: fresh_scope(),
        };
        let a = payload.canonical_bytes().unwrap();
        let b = payload.canonical_bytes().unwrap();
        assert_eq!(a, b);
    }
}
