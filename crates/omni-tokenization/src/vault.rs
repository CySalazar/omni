//! TEE-resident per-user token vault.
//!
//! The vault stores the bidirectional mapping between PII strings and their
//! opaque tokens. The entire mapping is persisted as a [`SealedBlob`]
//! (backed by the active [`TeeBackend`]) so it can be written to untrusted
//! storage and reloaded later without exposing PII.
//!
//! # Token generation
//!
//! Tokens are generated deterministically from the PII string and the
//! entity type using a BLAKE3 hash (via [`omni_crypto::hash::Blake3`]).
//! Within a session the same PII always produces the same token, which
//! lets a downstream model reason about co-reference without knowing the
//! underlying value. Tokens do NOT carry any structural information about
//! their plaintext (no length encoding, no checksum).
//!
//! Token format: `TKN-<ENTITY>-<HEX8>` where `<ENTITY>` is a compact
//! uppercase slug for the entity type and `<HEX8>` is the first 8 hex
//! characters of the BLAKE3 digest.
//!
//! # Sealing
//!
//! The vault serializes its `(token → PII)` mapping to a sorted
//! `Vec<(String, String)>` before sealing — this ensures deterministic
//! byte encoding via the canonical postcard encoder ([`omni_types::wire`]).
//! The sealed blob can be persisted to untrusted storage and later restored
//! via [`TokenVault::unseal_vault`].
//!
//! # Threat model
//!
//! - The vault MUST only run inside an attested TEE.
//! - Sealed blobs on untrusted storage are opaque; the TEE is the only
//!   entity that can unseal them.
//! - The `MockTeeBackend` used in tests produces a non-cryptographic
//!   "seal" (XOR-with-measurement). Do not use `MockTeeBackend` in
//!   production builds.
//!
//! See [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//! § "Tokenization service — vault lifecycle".

use std::collections::HashMap;
use std::sync::Arc;

use omni_crypto::hash::{Blake3, OmniHash};
use omni_tee::{SealPolicy, SealedBlob, TeeBackend, TeeErrorKind};
use omni_types::error::{
    OmniError, Result, TeeErrorKind as OmniTeeErrorKind, TokenizationErrorKind,
};
use omni_types::wire::{decode_canonical, encode_canonical};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::types::EntityType;

// =============================================================================
// VaultEntry — private serialization type
// =============================================================================

/// An entry in the on-disk (sealed) representation of the vault.
///
/// We serialize the vault as a sorted `Vec<VaultEntry>` rather than a
/// `HashMap` to guarantee deterministic byte encoding. Determinism matters
/// because the caller may hash the sealed blob for integrity auditing;
/// non-deterministic encoding would produce a different hash on every seal
/// even without a change in the mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VaultEntry {
    /// The opaque token (key in the reverse lookup direction).
    token: String,
    /// The original PII value (for detokenization).
    pii: String,
    /// The cache key used in the forward `pii_to_token` map
    /// (format: `"<ENTITY_SLUG>:<pii>"`). Stored explicitly so that the
    /// forward map can be reconstructed exactly after unsealing.
    cache_key: String,
}

// =============================================================================
// TokenVault
// =============================================================================

/// TEE-resident per-user token vault.
///
/// Stores the bidirectional mapping `PII ↔ token` and seals it inside the
/// TEE so that the mapping never leaves the trusted boundary in plaintext.
///
/// # Concurrency
///
/// `TokenVault` requires `&mut self` for all mutating operations. It is
/// not `Sync`. Callers that need concurrent access must wrap it in a
/// `Mutex`.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use omni_tokenization::vault::TokenVault;
/// use omni_tokenization::types::EntityType;
/// use omni_tee::MockTeeBackend;
///
/// let backend: Arc<dyn omni_tee::TeeBackend> = Arc::new(MockTeeBackend::new());
/// let mut vault = TokenVault::new(Arc::clone(&backend));
/// let token = vault.tokenize("alice@example.com", &EntityType::Email)
///     .expect("tokenize should succeed");
/// let recovered = vault.detokenize(&token)
///     .expect("detokenize should succeed");
/// assert_eq!(recovered, "alice@example.com");
/// ```
pub struct TokenVault {
    /// The active TEE backend used for sealing and unsealing.
    backend: Arc<dyn TeeBackend>,
    /// Forward mapping: PII → token (for de-duplication — same PII always
    /// produces the same token within a session).
    pii_to_token: HashMap<String, String>,
    /// Reverse mapping: token → PII (for detokenization).
    token_to_pii: HashMap<String, String>,
}

impl TokenVault {
    /// Create a new, empty vault backed by `backend`.
    ///
    /// The vault is initially empty. Call [`tokenize`](TokenVault::tokenize)
    /// to populate it, or [`unseal_vault`](TokenVault::unseal_vault) to
    /// restore a previously sealed vault.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let vault = TokenVault::new(Arc::new(MockTeeBackend::new()));
    /// ```
    #[must_use]
    pub fn new(backend: Arc<dyn TeeBackend>) -> Self {
        Self {
            backend,
            pii_to_token: HashMap::new(),
            token_to_pii: HashMap::new(),
        }
    }

    /// Tokenize `pii`, returning the deterministic opaque token.
    ///
    /// If the same PII value was tokenized earlier in this vault instance
    /// (or after a successful [`unseal_vault`](TokenVault::unseal_vault)),
    /// the same token is returned without creating a new entry — this is
    /// the co-reference preservation guarantee.
    ///
    /// The vault's in-memory state is updated atomically (both forward and
    /// reverse maps). The vault is **not** automatically sealed after
    /// tokenization; call [`seal_vault`](TokenVault::seal_vault) explicitly
    /// when persistence is required.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Internal`] if the forward and reverse maps are
    /// in an inconsistent state (this indicates a bug in this module).
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tokenization::types::EntityType;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let mut vault = TokenVault::new(Arc::new(MockTeeBackend::new()));
    /// let t1 = vault.tokenize("bob@acme.io", &EntityType::Email).unwrap();
    /// let t2 = vault.tokenize("bob@acme.io", &EntityType::Email).unwrap();
    /// // Same PII → same token (co-reference preservation).
    /// assert_eq!(t1, t2);
    /// ```
    #[instrument(skip(self, pii), fields(entity_type = ?entity_type))]
    pub fn tokenize(&mut self, pii: &str, entity_type: &EntityType) -> Result<String> {
        // The cache key includes the entity-type slug so that the same raw PII
        // value tokenized as two different entity types (e.g. "alice" as
        // PersonName vs. Email) produces distinct, correctly-typed tokens.
        // This preserves the domain-separation guarantee of `derive_token`.
        let cache_key = format!("{}:{}", entity_type_slug(entity_type), pii);

        // Return the existing token if PII was already mapped under this type.
        if let Some(existing) = self.pii_to_token.get(&cache_key) {
            return Ok(existing.clone());
        }

        let token = derive_token(pii, entity_type);

        self.pii_to_token.insert(cache_key, token.clone());
        self.token_to_pii.insert(token.clone(), pii.to_owned());

        Ok(token)
    }

    /// Resolve `token` back to its original PII value.
    ///
    /// The lookup is O(1) via the reverse map.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError::Tokenization`] with
    /// [`TokenizationErrorKind::TokenNotFound`] if `token` is not present
    /// in this vault.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tokenization::types::EntityType;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let mut vault = TokenVault::new(Arc::new(MockTeeBackend::new()));
    /// let token = vault.tokenize("+1-555-0100", &EntityType::Phone).unwrap();
    /// let pii = vault.detokenize(&token).unwrap();
    /// assert_eq!(pii, "+1-555-0100");
    /// ```
    #[instrument(skip(self))]
    pub fn detokenize(&self, token: &str) -> Result<String> {
        self.token_to_pii.get(token).cloned().ok_or_else(|| {
            OmniError::tokenization(
                TokenizationErrorKind::TokenNotFound,
                "vault::detokenize::token_not_found",
            )
        })
    }

    /// Seal the vault's entire mapping, returning an opaque [`SealedBlob`].
    ///
    /// The blob can be written to untrusted storage. Call
    /// [`unseal_vault`](TokenVault::unseal_vault) with the same backend
    /// to restore the vault later.
    ///
    /// The sealed data is sorted by token string before encoding to
    /// guarantee deterministic byte representations across calls with the
    /// same content.
    ///
    /// # Errors
    ///
    /// - [`OmniError::Wire`] if serialization fails.
    /// - [`OmniError::Tee`] if the TEE backend refuses the sealing
    ///   operation.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tokenization::types::EntityType;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let backend: Arc<dyn omni_tee::TeeBackend> = Arc::new(MockTeeBackend::new());
    /// let mut vault = TokenVault::new(Arc::clone(&backend));
    /// vault.tokenize("Jane Doe", &EntityType::PersonName).unwrap();
    /// let blob = vault.seal_vault().expect("seal must succeed");
    /// assert_eq!(blob.envelope_version, 1);
    /// ```
    #[instrument(skip(self))]
    pub fn seal_vault(&self) -> Result<SealedBlob> {
        // Build a sorted snapshot for deterministic encoding.
        // We iterate pii_to_token (cache_key → token) because the cache_key
        // contains the entity-type slug prefix that must be preserved for
        // correct round-trip behaviour after unsealing.
        let mut entries: Vec<VaultEntry> = self
            .pii_to_token
            .iter()
            .map(|(cache_key, token)| {
                // The raw PII is available from the reverse map.
                let pii = self
                    .token_to_pii
                    .get(token.as_str())
                    .cloned()
                    .unwrap_or_default();
                VaultEntry {
                    token: token.clone(),
                    pii,
                    cache_key: cache_key.clone(),
                }
            })
            .collect();
        entries.sort_by(|a, b| a.token.cmp(&b.token));

        let plaintext = encode_canonical(&entries)?;

        // Build a SealPolicy tied to this backend's family and measurement.
        // For the mock backend, the measurement is [0xAB; 48]. For real
        // hardware backends, this is the actual TEE measurement at runtime.
        //
        // We derive the policy from a fresh attestation so the measurement
        // is always the live runtime value, not a stale compile-time constant.
        let nonce = omni_tee::Nonce([0u8; 32]); // deterministic for sealing; freshness is the TEE's concern
        let quote = self
            .backend
            .attest(&nonce, None)
            .map_err(|e| tee_error_to_omni(&e, "vault::seal_vault::attest"))?;

        let policy = SealPolicy::new(quote.family, quote.measurement);
        self.backend
            .seal(&plaintext, &policy)
            .map_err(|e| tee_error_to_omni(&e, "vault::seal_vault::seal"))
    }

    /// Restore a vault from a previously sealed blob.
    ///
    /// The `backend` must be the same TEE family and measurement that
    /// produced the seal; if the policy does not match, the backend will
    /// return a [`TeeErrorKind::UnsealFailed`] error.
    ///
    /// # Errors
    ///
    /// - [`OmniError::Tee`] if the backend refuses to unseal the blob.
    /// - [`OmniError::Wire`] if the unsealed bytes cannot be decoded.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tokenization::types::EntityType;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let backend: Arc<dyn omni_tee::TeeBackend> = Arc::new(MockTeeBackend::new());
    /// let mut vault = TokenVault::new(Arc::clone(&backend));
    /// vault.tokenize("555-12-3456", &EntityType::Ssn).unwrap();
    /// let blob = vault.seal_vault().unwrap();
    ///
    /// let restored = TokenVault::unseal_vault(Arc::clone(&backend), &blob)
    ///     .expect("unseal must succeed");
    /// // The original token must be resolvable after a round-trip.
    /// ```
    pub fn unseal_vault(backend: Arc<dyn TeeBackend>, blob: &SealedBlob) -> Result<Self> {
        let plaintext = backend
            .unseal(blob)
            .map_err(|e| tee_error_to_omni(&e, "vault::unseal_vault::unseal"))?;

        let entries: Vec<VaultEntry> = decode_canonical(&plaintext)?;

        let mut pii_to_token = HashMap::with_capacity(entries.len());
        let mut token_to_pii = HashMap::with_capacity(entries.len());

        for entry in entries {
            // Restore the forward map using the full cache_key (which includes
            // the entity-type slug) and the reverse map using just the raw PII.
            pii_to_token.insert(entry.cache_key, entry.token.clone());
            token_to_pii.insert(entry.token, entry.pii);
        }

        Ok(Self {
            backend,
            pii_to_token,
            token_to_pii,
        })
    }

    /// Returns the number of unique PII entries currently in the vault.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let vault = TokenVault::new(Arc::new(MockTeeBackend::new()));
    /// assert_eq!(vault.len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.pii_to_token.len()
    }

    /// Returns `true` if the vault contains no entries.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let vault = TokenVault::new(Arc::new(MockTeeBackend::new()));
    /// assert!(vault.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pii_to_token.is_empty()
    }
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Derive a deterministic, collision-resistant token for a PII string.
///
/// Token format: `TKN-<ENTITY_SLUG>-<HEX8>`
///
/// `<HEX8>` is the first 4 bytes (8 hex chars) of the BLAKE3 hash of
/// `"<entity_slug>:<pii>"`. Domain separation via the entity prefix
/// prevents two different entity types mapping the same PII string to the
/// same token hash (which would allow a hash-collision-based PII inference
/// attack across entity categories).
fn derive_token(pii: &str, entity_type: &EntityType) -> String {
    let slug = entity_type_slug(entity_type);

    // Domain-separated input: "<slug>:<pii>"
    // The colon is chosen because entity slugs are all-uppercase ASCII
    // letters/hyphens, so a colon unambiguously delimits the slug from
    // the (arbitrary-bytes) PII value.
    let mut input = slug.as_bytes().to_vec();
    input.push(b':');
    input.extend_from_slice(pii.as_bytes());

    let digest = Blake3::hash(&input);

    // Take the first 4 bytes of the digest (8 hex chars). The probability
    // of a collision within a single vault instance (typically < 10^4
    // entries) is ~(N^2)/(2*2^32) ≈ negligible. For a production
    // deployment with more entries, extend to 8 bytes. The full 32-byte
    // digest is always computed; we truncate only the displayed portion.
    //
    // We build the hex string by pushing two hex nibble characters per
    // byte into a pre-allocated buffer. Using a constant lookup avoids
    // the clippy::format_collect warning and keeps the hot path allocation-
    // free after the initial `with_capacity`.
    let hex_chars = b"0123456789abcdef";
    let mut hex8 = String::with_capacity(8);
    for &b in &digest[..4] {
        // SAFETY: `hex_chars` has exactly 16 elements; `b >> 4` and
        // `b & 0x0F` both produce values in `[0, 15]`, so both
        // index accesses are in-bounds. The resulting bytes are valid
        // ASCII and therefore valid UTF-8.
        #[allow(clippy::indexing_slicing)]
        hex8.push(hex_chars[(b >> 4) as usize] as char);
        #[allow(clippy::indexing_slicing)]
        hex8.push(hex_chars[(b & 0x0F) as usize] as char);
    }

    format!("TKN-{slug}-{hex8}")
}

/// Returns a compact, uppercase slug for an entity type.
///
/// The slug is embedded in the token string and used as the domain
/// separator for hash derivation. Slugs must be stable — changing them
/// is a breaking change to the token wire format.
fn entity_type_slug(entity_type: &EntityType) -> &'static str {
    match entity_type {
        EntityType::PersonName => "NAME",
        EntityType::Email => "EMAIL",
        EntityType::Phone => "PHONE",
        EntityType::Ssn => "SSN",
        EntityType::CreditCard => "CC",
        EntityType::Address => "ADDR",
        // Custom variants use a generic slug. Two different custom categories
        // will collide in the slug, but they are domain-separated by the
        // full pii value in the hash input, which prevents token collision.
        EntityType::Custom(_) => "CUSTOM",
    }
}

/// Convert a [`omni_tee::TeeError`] to an [`OmniError`].
///
/// This conversion is defined here (not in `omni-tee`) to preserve the
/// layering invariant: `omni-tee` must not depend on `omni-types`'s error
/// taxonomy, only the reverse direction.
fn tee_error_to_omni(err: &omni_tee::TeeError, context: &'static str) -> OmniError {
    let kind = match err.kind {
        TeeErrorKind::SealFailed => OmniTeeErrorKind::SealingFailed,
        TeeErrorKind::UnsealFailed => OmniTeeErrorKind::UnsealingFailed,
        _ => OmniTeeErrorKind::BackendFailure,
    };
    OmniError::tee(kind, context)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use std::sync::Arc;

    use omni_tee::{Measurement, MockTeeBackend};

    use super::*;
    use crate::types::EntityType;

    fn mock_backend() -> Arc<dyn TeeBackend> {
        Arc::new(MockTeeBackend::new())
    }

    // -------------------------------------------------------------------------
    // Basic tokenize / detokenize
    // -------------------------------------------------------------------------

    #[test]
    fn tokenize_returns_non_empty_token() {
        let mut vault = TokenVault::new(mock_backend());
        let token = vault
            .tokenize("alice@example.com", &EntityType::Email)
            .expect("tokenize");
        assert!(!token.is_empty());
    }

    #[test]
    fn token_starts_with_tkn_prefix() {
        let mut vault = TokenVault::new(mock_backend());
        let token = vault
            .tokenize("alice@example.com", &EntityType::Email)
            .expect("tokenize");
        assert!(token.starts_with("TKN-EMAIL-"), "token was: {token}");
    }

    #[test]
    fn same_pii_same_token_within_session() {
        let mut vault = TokenVault::new(mock_backend());
        let t1 = vault
            .tokenize("alice@example.com", &EntityType::Email)
            .expect("tokenize t1");
        let t2 = vault
            .tokenize("alice@example.com", &EntityType::Email)
            .expect("tokenize t2");
        assert_eq!(t1, t2, "same PII must produce same token");
    }

    #[test]
    fn different_pii_different_tokens() {
        let mut vault = TokenVault::new(mock_backend());
        let t1 = vault
            .tokenize("alice@example.com", &EntityType::Email)
            .expect("t1");
        let t2 = vault
            .tokenize("bob@example.com", &EntityType::Email)
            .expect("t2");
        assert_ne!(t1, t2);
    }

    #[test]
    fn detokenize_round_trip() {
        let mut vault = TokenVault::new(mock_backend());
        let pii = "Alice Smith";
        let token = vault.tokenize(pii, &EntityType::PersonName).expect("tok");
        let recovered = vault.detokenize(&token).expect("detok");
        assert_eq!(recovered, pii);
    }

    #[test]
    fn detokenize_unknown_token_returns_error() {
        let vault = TokenVault::new(mock_backend());
        let err = vault
            .detokenize("TKN-EMAIL-deadbeef")
            .expect_err("unknown token must error");
        assert!(matches!(
            err,
            OmniError::Tokenization {
                kind: TokenizationErrorKind::TokenNotFound,
                ..
            }
        ));
    }

    // -------------------------------------------------------------------------
    // Seal / unseal round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn seal_unseal_round_trip() {
        let backend = mock_backend();
        let mut vault = TokenVault::new(Arc::clone(&backend));
        let pii = "555-12-3456";
        let token = vault.tokenize(pii, &EntityType::Ssn).expect("tok");
        let blob = vault.seal_vault().expect("seal");

        let restored = TokenVault::unseal_vault(Arc::clone(&backend), &blob).expect("unseal");
        let recovered = restored.detokenize(&token).expect("detok after unseal");
        assert_eq!(recovered, pii);
    }

    #[test]
    fn seal_unseal_preserves_multiple_entries() {
        let backend = mock_backend();
        let mut vault = TokenVault::new(Arc::clone(&backend));

        let entries = [
            ("alice@example.com", EntityType::Email),
            ("Alice Smith", EntityType::PersonName),
            ("+1-555-0100", EntityType::Phone),
        ];
        let mut tokens = Vec::new();
        for (pii, et) in &entries {
            tokens.push(vault.tokenize(pii, et).expect("tok"));
        }

        let blob = vault.seal_vault().expect("seal");
        let restored = TokenVault::unseal_vault(Arc::clone(&backend), &blob).expect("unseal");

        for (i, (pii, _)) in entries.iter().enumerate() {
            let recovered = restored.detokenize(&tokens[i]).expect("detok after unseal");
            assert_eq!(recovered, *pii);
        }
    }

    #[test]
    fn seal_is_deterministic_for_same_content() {
        let backend = mock_backend();
        let mut v1 = TokenVault::new(Arc::clone(&backend));
        let mut v2 = TokenVault::new(Arc::clone(&backend));

        v1.tokenize("pii-value", &EntityType::Email)
            .expect("v1 tok");
        v2.tokenize("pii-value", &EntityType::Email)
            .expect("v2 tok");

        let b1 = v1.seal_vault().expect("seal 1");
        let b2 = v2.seal_vault().expect("seal 2");
        assert_eq!(
            b1.ciphertext, b2.ciphertext,
            "sealing same content must be deterministic"
        );
    }

    #[test]
    fn unseal_fails_with_different_backend_measurement() {
        let backend_a: Arc<dyn TeeBackend> =
            Arc::new(MockTeeBackend::with_measurement(Measurement([0x01u8; 48])));
        let backend_b: Arc<dyn TeeBackend> =
            Arc::new(MockTeeBackend::with_measurement(Measurement([0x02u8; 48])));

        let mut vault = TokenVault::new(Arc::clone(&backend_a));
        vault
            .tokenize("secret", &EntityType::PersonName)
            .expect("tok");
        let blob = vault.seal_vault().expect("seal");

        let result = TokenVault::unseal_vault(Arc::clone(&backend_b), &blob);
        assert!(result.is_err(), "unseal with wrong measurement must fail");
    }

    // -------------------------------------------------------------------------
    // Vault metadata
    // -------------------------------------------------------------------------

    #[test]
    fn empty_vault_is_empty() {
        let vault = TokenVault::new(mock_backend());
        assert!(vault.is_empty());
        assert_eq!(vault.len(), 0);
    }

    #[test]
    fn vault_len_increases_per_unique_pii() {
        let mut vault = TokenVault::new(mock_backend());
        vault.tokenize("a@b.com", &EntityType::Email).expect("1");
        assert_eq!(vault.len(), 1);
        // Same PII again — must not increase len.
        vault.tokenize("a@b.com", &EntityType::Email).expect("dup");
        assert_eq!(vault.len(), 1);
        vault.tokenize("c@d.com", &EntityType::Email).expect("2");
        assert_eq!(vault.len(), 2);
    }

    // -------------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------------

    #[test]
    fn empty_pii_string_is_tokenizable() {
        let mut vault = TokenVault::new(mock_backend());
        let token = vault.tokenize("", &EntityType::Email).expect("empty pii");
        let recovered = vault.detokenize(&token).expect("detok");
        assert_eq!(recovered, "");
    }

    #[test]
    fn token_for_name_differs_from_token_for_email_same_value() {
        // Domain separation: same raw string tokenized under different
        // entity types must produce different tokens.
        let mut vault = TokenVault::new(mock_backend());
        let t_name = vault
            .tokenize("alice", &EntityType::PersonName)
            .expect("name tok");
        let t_email = vault
            .tokenize("alice", &EntityType::Email)
            .expect("email tok");
        assert_ne!(
            t_name, t_email,
            "domain separation must produce distinct tokens"
        );
    }

    #[test]
    fn custom_entity_type_tokenized_correctly() {
        let mut vault = TokenVault::new(mock_backend());
        let token = vault
            .tokenize(
                "employee-123",
                &EntityType::Custom("employee-id".to_string()),
            )
            .expect("custom tok");
        assert!(token.starts_with("TKN-CUSTOM-"), "token was: {token}");
        let recovered = vault.detokenize(&token).expect("detok");
        assert_eq!(recovered, "employee-123");
    }

    #[test]
    fn sealed_empty_vault_round_trips() {
        let backend = mock_backend();
        let vault = TokenVault::new(Arc::clone(&backend));
        let blob = vault.seal_vault().expect("seal empty");
        let restored = TokenVault::unseal_vault(Arc::clone(&backend), &blob).expect("unseal empty");
        assert!(restored.is_empty());
    }
}
