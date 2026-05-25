//! # `omni-tokenization`
//!
//! PII tokenization service for OMNI OS.
//!
//! Replaces personally identifiable information (PII) with deterministic
//! tokens before any inference workload leaves the user's TEE. The
//! mapping between PII and tokens lives in a per-user vault inside the
//! TEE; the model only ever sees tokens, never raw PII.
//!
//! ## Architecture overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │             TokenizationService                  │
//! │  ┌──────────────┐  ┌──────────────┐             │
//! │  │ NerClassifier │  │ PolicyEngine │             │
//! │  └──────┬───────┘  └──────┬───────┘             │
//! │         │ NerSpans         │ should_tokenize?     │
//! │         └────────┬─────────┘                     │
//! │                  ▼                               │
//! │           ┌─────────────┐                        │
//! │           │  TokenVault │ (TEE-sealed)            │
//! │           └─────────────┘                        │
//! └─────────────────────────────────────────────────┘
//! ```
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
//! ## Connection to `omni-types::encrypted`
//!
//! The marker types in [`omni_types::encrypted`] are the *stored form* of
//! values that this crate has processed:
//!
//! - [`omni_types::encrypted::EncryptedString`] — a string encrypted by
//!   this crate inside the TEE.
//! - [`omni_types::encrypted::TokenizedEmail`] — an email tokenized by this
//!   crate.
//! - [`omni_types::encrypted::MaskedSSN`] — an SSN masked and encrypted by
//!   this crate.
//! - [`omni_types::encrypted::AttestedHash`] — a hash bound to the TEE
//!   attestation that witnessed the tokenization.
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
// Allow unwrap/expect/panic in test code. Mirrors the workspace-level
// cfg-test allowances in `omni-types` and `omni-tee`.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
    )
)]

pub mod encrypted_pipeline;
pub mod ner;
pub mod policy;
pub mod privacy;
pub mod types;
pub mod vault;

use std::sync::Arc;

use omni_tee::TeeBackend;
use omni_types::error::{OmniError, Result};
use tracing::instrument;

use crate::ner::NerClassifier;
use crate::policy::PolicyEngine;
use crate::types::{
    DetokenizeRequest, DetokenizeResponse, Replacement, TokenizeRequest, TokenizeResponse,
};
use crate::vault::TokenVault;

// =============================================================================
// TokenizationService
// =============================================================================

/// Top-level PII tokenization service.
///
/// `TokenizationService` composes the NER classifier, the policy engine,
/// and the token vault into the end-to-end request/response pipeline. It is
/// the single entry point for callers that want to tokenize or de-tokenize
/// text.
///
/// # Thread safety
///
/// `TokenizationService` is not `Sync` because [`TokenVault`] requires
/// `&mut self` for tokenization. Callers that need to share a service
/// instance across threads must wrap it in a `Mutex` or similar.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use omni_tokenization::TokenizationService;
/// use omni_tokenization::policy::PolicyPreset;
/// use omni_tokenization::types::TokenizeRequest;
/// use omni_tee::MockTeeBackend;
/// use omni_types::identity::SessionId;
///
/// let mut service = TokenizationService::new(Arc::new(MockTeeBackend::new()));
/// let req = TokenizeRequest {
///     session_id: SessionId::new(),
///     text: "Contact alice@example.com for details.".to_string(),
///     policy: PolicyPreset::Gdpr,
/// };
/// let resp = service.tokenize(req).expect("tokenize must succeed");
/// // The email should have been replaced.
/// assert!(!resp.tokenized_text.contains("alice@example.com"));
/// assert!(!resp.replacements.is_empty());
/// ```
pub struct TokenizationService {
    ner: NerClassifier,
    vault: TokenVault,
}

impl TokenizationService {
    /// Create a new `TokenizationService` backed by `backend`.
    ///
    /// The service creates a fresh, empty vault. Use
    /// [`TokenizationService::from_vault`] to start from a previously
    /// sealed vault.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::TokenizationService;
    /// use omni_tee::MockTeeBackend;
    ///
    /// let svc = TokenizationService::new(Arc::new(MockTeeBackend::new()));
    /// ```
    #[must_use]
    pub fn new(backend: Arc<dyn TeeBackend>) -> Self {
        Self {
            ner: NerClassifier::new(),
            vault: TokenVault::new(backend),
        }
    }

    /// Create a `TokenizationService` from an existing [`TokenVault`].
    ///
    /// Use this constructor when restoring a session from a previously
    /// sealed blob:
    ///
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use omni_tokenization::TokenizationService;
    /// use omni_tokenization::vault::TokenVault;
    /// use omni_tee::{MockTeeBackend, SealedBlob};
    ///
    /// fn restore(backend: Arc<MockTeeBackend>, blob: &SealedBlob)
    ///     -> omni_types::error::Result<TokenizationService>
    /// {
    ///     let vault = TokenVault::unseal_vault(backend, blob)?;
    ///     Ok(TokenizationService::from_vault(vault))
    /// }
    /// ```
    #[must_use]
    pub fn from_vault(vault: TokenVault) -> Self {
        Self {
            ner: NerClassifier::new(),
            vault,
        }
    }

    /// Crate-private helper: tokenize a single PII value via the vault.
    ///
    /// Exposed to `encrypted_pipeline` so that module can drive vault
    /// tokenization for individual spans without requiring a full
    /// [`TokenizeRequest`]. The vault's co-reference semantics are preserved:
    /// the same PII value under the same entity type returns the same token
    /// within a session.
    pub(crate) fn vault_tokenize(
        &mut self,
        pii: &str,
        entity_type: &crate::types::EntityType,
    ) -> Result<String> {
        self.vault.tokenize(pii, entity_type)
    }

    /// Tokenize the text in `req`, returning the tokenized text and
    /// the substitution manifest.
    ///
    /// The method:
    /// 1. Runs the NER classifier to detect PII spans.
    /// 2. Consults the policy engine (built from `req.policy`) to filter
    ///    to only the spans that must be tokenized under the active policy.
    /// 3. Processes spans right-to-left (highest byte offset first) so
    ///    earlier spans' offsets remain valid.
    /// 4. Returns the tokenized text and the substitution manifest.
    ///
    /// # Errors
    ///
    /// Returns [`OmniError`] if the vault's tokenize operation fails.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::TokenizationService;
    /// use omni_tokenization::policy::PolicyPreset;
    /// use omni_tokenization::types::TokenizeRequest;
    /// use omni_tee::MockTeeBackend;
    /// use omni_types::identity::SessionId;
    ///
    /// let mut svc = TokenizationService::new(Arc::new(MockTeeBackend::new()));
    /// let req = TokenizeRequest {
    ///     session_id: SessionId::new(),
    ///     text: "Email alice@example.com".to_string(),
    ///     policy: PolicyPreset::Gdpr,
    /// };
    /// let resp = svc.tokenize(req).unwrap();
    /// assert!(!resp.tokenized_text.contains("alice@example.com"));
    /// ```
    #[instrument(skip(self, req), fields(policy = ?req.policy))]
    pub fn tokenize(&mut self, req: TokenizeRequest) -> Result<TokenizeResponse> {
        // Destructure the request so we can move individual fields without
        // triggering needless_pass_by_value (the value IS consumed here).
        let TokenizeRequest {
            session_id,
            text: original_text,
            policy: policy_preset,
        } = req;

        let policy = PolicyEngine::new(policy_preset);
        let spans = self.ner.classify(&original_text);

        // Filter spans to those the policy mandates tokenizing.
        let mut actionable: Vec<_> = spans
            .into_iter()
            .filter(|span| policy.should_tokenize(&span.entity_type))
            .collect();

        // Sort descending by start so right-to-left replacement preserves
        // earlier byte offsets.
        actionable.sort_by(|a, b| b.start.cmp(&a.start));

        let mut text = original_text;
        let mut replacements: Vec<Replacement> = Vec::with_capacity(actionable.len());

        for span in &actionable {
            // Bounds check: the classifier should only emit valid spans, but
            // we validate defensively to avoid panicking on unexpected input.
            if span.start > span.end || span.end > text.len() {
                return Err(OmniError::internal(
                    "tokenization::tokenize::span_out_of_bounds",
                ));
            }

            let pii = text[span.start..span.end].to_owned();
            let token = self.vault.tokenize(&pii, &span.entity_type)?;

            // Replace the span in the text.
            text.replace_range(span.start..span.end, &token);

            replacements.push(Replacement {
                // The span's start is the original offset; the caller asked for
                // original_span coordinates, not post-substitution coordinates.
                original_span: (span.start, span.end),
                token,
                entity_type: span.entity_type.clone(),
            });
        }

        // Sort replacements ascending by span start for the caller's
        // convenience (we processed them descending but the manifest should
        // read left-to-right).
        replacements.sort_by_key(|r| r.original_span.0);

        Ok(TokenizeResponse {
            session_id,
            tokenized_text: text,
            replacements,
        })
    }

    /// De-tokenize the text in `req`, resolving tokens back to their
    /// original PII values.
    ///
    /// The method scans `req.tokenized_text` for substrings that match
    /// known tokens in the vault and replaces each one with the
    /// corresponding PII value. Tokens not present in the vault are left
    /// in place.
    ///
    /// # Errors
    ///
    /// Currently infallible (returns `Ok` always). Token lookup failures
    /// are silent (unknown tokens are left verbatim). Future versions may
    /// optionally return errors on unknown tokens.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use omni_tokenization::TokenizationService;
    /// use omni_tokenization::policy::PolicyPreset;
    /// use omni_tokenization::types::{DetokenizeRequest, TokenizeRequest};
    /// use omni_tee::MockTeeBackend;
    /// use omni_types::identity::SessionId;
    ///
    /// let mut svc = TokenizationService::new(Arc::new(MockTeeBackend::new()));
    /// let session = SessionId::new();
    ///
    /// let tok_resp = svc.tokenize(TokenizeRequest {
    ///     session_id: session,
    ///     text: "alice@example.com".to_string(),
    ///     policy: PolicyPreset::Gdpr,
    /// }).unwrap();
    ///
    /// let detok_resp = svc.detokenize(DetokenizeRequest {
    ///     session_id: session,
    ///     tokenized_text: tok_resp.tokenized_text,
    /// }).unwrap();
    ///
    /// assert_eq!(detok_resp.text, "alice@example.com");
    /// ```
    #[instrument(skip(self, req))]
    pub fn detokenize(&self, req: DetokenizeRequest) -> Result<DetokenizeResponse> {
        // Destructure the request so we can move fields, consuming the value
        // and satisfying the clippy::needless_pass_by_value invariant.
        let DetokenizeRequest {
            session_id,
            tokenized_text,
        } = req;

        // Simple approach: try to look up every whitespace-delimited word in
        // the vault. If it resolves, replace it. If not, leave it.
        //
        // This is O(N*M) in the worst case where N is the number of words
        // and M is the vault size. For typical use cases (a few hundred
        // entries in the vault, a few paragraphs of text) this is
        // perfectly adequate. A trie-based approach would be needed at
        // scale.
        //
        // We process the text word-by-word from right to left (descending
        // byte offset) so that replacements do not shift the offsets of
        // words to the left.
        let mut sorted_spans = collect_token_spans(&tokenized_text);
        sorted_spans.sort_by(|a, b| b.0.cmp(&a.0));

        let mut result = tokenized_text.clone();
        for (start, end) in sorted_spans {
            let word = &tokenized_text[start..end];
            if let Ok(pii) = self.vault.detokenize(word) {
                result.replace_range(start..end, &pii);
            }
        }

        Ok(DetokenizeResponse {
            session_id,
            text: result,
        })
    }
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Collect byte-offset spans for every whitespace-delimited word in `text`.
///
/// Returns `(start, end)` pairs where `text[start..end]` is the word.
fn collect_token_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut word_start: Option<usize> = None;

    for (i, &b) in bytes.iter().enumerate() {
        let is_ws = b == b' ' || b == b'\t' || b == b'\n' || b == b'\r';
        match (is_ws, word_start) {
            (false, None) => {
                word_start = Some(i);
            }
            (true, Some(start)) => {
                spans.push((start, i));
                word_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = word_start {
        spans.push((start, bytes.len()));
    }
    spans
}

// =============================================================================
// Integration tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omni_tee::MockTeeBackend;
    use omni_types::identity::SessionId;

    use super::*;
    use crate::policy::PolicyPreset;
    use crate::types::{DetokenizeRequest, TokenizeRequest};

    fn new_service() -> TokenizationService {
        TokenizationService::new(Arc::new(MockTeeBackend::new()))
    }

    #[test]
    fn placeholder_test_from_original_scaffold_still_passes() {}

    // -------------------------------------------------------------------------
    // Tokenize round-trip integration
    // -------------------------------------------------------------------------

    #[test]
    fn tokenize_email_under_gdpr_replaces_span() {
        let mut svc = new_service();
        let resp = svc
            .tokenize(TokenizeRequest {
                session_id: SessionId::new(),
                text: "Contact alice@example.com for details.".to_string(),
                policy: PolicyPreset::Gdpr,
            })
            .expect("tokenize");
        assert!(
            !resp.tokenized_text.contains("alice@example.com"),
            "email must be replaced"
        );
        assert_eq!(resp.replacements.len(), 1);
    }

    #[test]
    fn tokenize_email_under_pci_does_not_replace_span() {
        let mut svc = new_service();
        let resp = svc
            .tokenize(TokenizeRequest {
                session_id: SessionId::new(),
                text: "Contact alice@example.com".to_string(),
                policy: PolicyPreset::Pci,
            })
            .expect("tokenize");
        // PCI does not cover Email.
        assert_eq!(
            resp.tokenized_text, "Contact alice@example.com",
            "PCI must not tokenize email"
        );
        assert!(resp.replacements.is_empty());
    }

    #[test]
    fn tokenize_then_detokenize_recovers_original() {
        let mut svc = new_service();
        let original = "Reach alice@example.com or call 555-123-4567 anytime.";
        let session = SessionId::new();

        let tok = svc
            .tokenize(TokenizeRequest {
                session_id: session,
                text: original.to_string(),
                policy: PolicyPreset::Strict,
            })
            .expect("tokenize");

        let detok = svc
            .detokenize(DetokenizeRequest {
                session_id: session,
                tokenized_text: tok.tokenized_text,
            })
            .expect("detokenize");

        assert_eq!(detok.text, original, "round-trip must recover original");
    }

    #[test]
    fn tokenize_empty_text_returns_empty_response() {
        let mut svc = new_service();
        let resp = svc
            .tokenize(TokenizeRequest {
                session_id: SessionId::new(),
                text: String::new(),
                policy: PolicyPreset::Gdpr,
            })
            .expect("empty tokenize");
        assert!(resp.tokenized_text.is_empty());
        assert!(resp.replacements.is_empty());
    }

    #[test]
    fn detokenize_unknown_token_leaves_it_verbatim() {
        let svc = new_service();
        let resp = svc
            .detokenize(DetokenizeRequest {
                session_id: SessionId::new(),
                tokenized_text: "Hello TKN-EMAIL-unknown world".to_string(),
            })
            .expect("detokenize");
        assert_eq!(resp.text, "Hello TKN-EMAIL-unknown world");
    }
}
