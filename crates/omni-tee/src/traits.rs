//! `TeeBackend` trait and the supporting [`TeeFamily`] / [`TeeError`] types.
//!
//! The trait surface is intentionally minimal. Everything a higher-layer
//! consumer needs — generating a quote, verifying a peer's quote, sealing
//! key material, and deriving a TEE-bound shared secret — is captured by
//! five methods. Adding a method to this trait is a breaking change and
//! requires an OIP.

use alloc::vec::Vec;

use crate::{
    attestation::{Measurement, Nonce, Quote},
    sealed_keys::{SealPolicy, SealedBlob, TeeSharedKey},
};

// -----------------------------------------------------------------------------
// TeeFamily
// -----------------------------------------------------------------------------

/// Enumerates the TEE families OMNI OS supports.
///
/// One-byte representation is enforced (see `lib.rs::sanity`) so the type
/// can be encoded compactly on the wire and inside attestation reports.
///
/// Adding a variant requires updating:
///   - the measurement-allowlist propagation logic in `omni-mesh`,
///   - the wire-format documentation in `docs/03-mesh-protocol.md`,
///   - and a Standards-Track OIP that ratifies the new family.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TeeFamily {
    /// Intel Trust Domain Extensions. Available from 4th-generation Xeon
    /// Scalable (Sapphire Rapids, 2023) onward.
    IntelTdx = 1,
    /// AMD Secure Encrypted Virtualization — Secure Nested Paging.
    /// Available from EPYC Milan (3rd-gen, 2021) onward.
    AmdSevSnp = 2,
    /// Apple Silicon Secure Enclave + Private Cloud Compute pattern.
    /// Planned for v1.1. Reserved variant; never produced by `attest()`
    /// in v1.0 builds.
    AppleSecureEnclave = 3,
    /// `ARMv9` Confidential Compute Architecture Realms. Planned for v1.2+.
    /// Reserved variant.
    ArmCca = 4,
    /// In-process deterministic mock. Only valid for the `mock` feature
    /// or for explicit test contexts. Production builds MUST reject
    /// quotes whose family is `Mock`.
    Mock = 0xFF,
}

impl TeeFamily {
    /// Returns `true` if the family represents real hardware that can be
    /// trusted in production. The `Mock` family is excluded; reserved
    /// variants (`AppleSecureEnclave`, `ArmCca`) are excluded until their
    /// respective enabling OIPs are ratified.
    #[must_use]
    pub const fn is_production(self) -> bool {
        matches!(self, Self::IntelTdx | Self::AmdSevSnp)
    }
}

// -----------------------------------------------------------------------------
// TeeError
// -----------------------------------------------------------------------------

/// Discriminant for [`TeeError`].
///
/// The discriminant carries no payload and is PII-safe — it is suitable for
/// inclusion in logs, audit records, and OIP disposition tables. The full
/// error (with optional payload) is in [`TeeError`].
///
/// **Why a separate discriminant**: PII safety. Tests, logs, and metrics
/// frequently report the *kind* of error without revealing the offending
/// inputs. Mixing payload into the kind makes that unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeeErrorKind {
    /// The platform does not support TEE attestation, or the requested
    /// family is not installed.
    Unsupported,
    /// Quote generation failed (TEE firmware refusal, transient error).
    QuoteGenerationFailed,
    /// Quote signature did not verify against the platform attestation
    /// key. The quote is fraudulent or corrupted.
    QuoteSignatureInvalid,
    /// Quote was syntactically well-formed but its embedded measurement
    /// is not on the active allowlist.
    QuoteMeasurementRejected,
    /// Quote nonce did not match the value the verifier expected. Replay
    /// or transcript-binding failure.
    QuoteNonceMismatch,
    /// Quote freshness window exceeded (e.g., TCB recovery happened
    /// since the quote was generated and the new minimum is higher).
    QuoteStale,
    /// Sealing failed: either the policy is not satisfiable on this
    /// platform, or the underlying TEE refused the operation.
    SealFailed,
    /// Unsealing failed: the sealed blob's policy does not match the
    /// current TEE measurement, the blob was tampered with, or the TEE
    /// migrated since the seal (e.g., live migration without sealed-key
    /// migration support).
    UnsealFailed,
    /// `derive_key_for` failed: typically the peer attestation provided
    /// is invalid or the platform cannot derive a key bound to it.
    KeyDerivationFailed,
    /// Internal invariant violation. This is a bug in `omni-tee`; report
    /// it via [`SECURITY.md`](../../../../SECURITY.md).
    Internal,
}

/// Error type returned by every fallible [`TeeBackend`] method.
///
/// The error carries a [`TeeErrorKind`] discriminant plus an optional
/// `&'static str` context slug. The slug is **PII-safe by construction**:
/// it is a compile-time string literal selected by the implementer, never
/// user input. Consumers can log it freely.
///
/// Conversion to `omni_types::OmniError` is intentionally *not* exposed
/// here; the consumer crate that needs the conversion implements it in
/// its own boundary module. This keeps `omni-tee` decoupled from the
/// project-wide error taxonomy and avoids a layering inversion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("tee error: kind={kind:?}, context={context}")]
pub struct TeeError {
    /// Discriminant. Safe for logging.
    pub kind: TeeErrorKind,
    /// Static context slug. Safe for logging. Implementers MUST NOT use
    /// `String` here; the `&'static str` constraint forces compile-time
    /// strings, eliminating PII-in-error-message risk.
    pub context: &'static str,
}

impl TeeError {
    /// Convenience constructor.
    #[must_use]
    pub const fn new(kind: TeeErrorKind, context: &'static str) -> Self {
        Self { kind, context }
    }
}

// -----------------------------------------------------------------------------
// TeeBackend trait
// -----------------------------------------------------------------------------

/// Vendor-neutral abstraction over a Trusted Execution Environment.
///
/// Implementations live in:
///   - [`crate::mock::MockTeeBackend`] for tests,
///   - [`crate::tdx::TdxBackend`] for Intel TDX (feature `tdx`),
///   - [`crate::sev_snp::SevSnpBackend`] for AMD SEV-SNP (feature `sev-snp`).
///
/// The trait is `Send + Sync` so consumers can hold a `dyn TeeBackend`
/// across threads in userspace services (`omni-runtime`, `omni-mesh`).
/// All methods take `&self`; an implementation that needs interior
/// mutability uses an appropriate primitive (`spin::Mutex` in `no_std`,
/// `std::sync::Mutex` in std contexts).
///
/// ## Error model
///
/// Every method returns [`TeeError`]. **No method panics on a runtime
/// failure**; panics are reserved for genuine impossibilities (e.g., a
/// constant assertion failed). Consumers that need to convert a
/// [`TeeError`] to a project-wide [`omni_types::OmniError`] do so at
/// their crate boundary.
pub trait TeeBackend: Send + Sync {
    /// Returns which [`TeeFamily`] this backend speaks. Used by routing,
    /// allowlist filtering, and structured logs.
    fn family(&self) -> TeeFamily;

    /// Produces a [`Quote`] attesting that this code is running inside the
    /// claimed TEE measurement.
    ///
    /// The `nonce` argument MUST be supplied by the verifier (typically a
    /// peer in the mesh handshake) and MUST NOT be predictable to the
    /// attestor before this call. The TEE firmware embeds the nonce into
    /// the signed payload of the quote; this is what makes the quote
    /// non-replayable.
    ///
    /// The optional `report_data` carries up to 32 bytes that the
    /// attestor wishes to bind into the quote (e.g., the hash of the
    /// transcript-so-far in the mesh handshake). Some TEE families allow
    /// 64 bytes; if more than 32 is requested, the implementer MUST
    /// return `Unsupported`.
    ///
    /// # Errors
    ///
    /// - [`TeeErrorKind::Unsupported`] if the platform cannot produce
    ///   quotes (typically a misconfigured TEE).
    /// - [`TeeErrorKind::QuoteGenerationFailed`] for transient TEE
    ///   firmware refusal; consumer may retry with backoff.
    fn attest(&self, nonce: &Nonce, report_data: Option<&[u8]>) -> Result<Quote, TeeError>;

    /// Verifies a peer-supplied [`Quote`] against the local allowlist of
    /// acceptable measurements and the expected nonce.
    ///
    /// `expected_nonce` is the nonce the verifier sent to the peer when
    /// asking for attestation. The verifier MUST keep this nonce in
    /// session state and pass it back here.
    ///
    /// `expected_measurement` is one specific measurement the verifier
    /// is willing to accept. Multiple measurements (an allowlist) are
    /// verified by calling this method once per candidate measurement
    /// and accepting the first match.
    ///
    /// # Errors
    ///
    /// - [`TeeErrorKind::QuoteSignatureInvalid`] for tampered or
    ///   incorrectly-signed quotes.
    /// - [`TeeErrorKind::QuoteMeasurementRejected`] when the embedded
    ///   measurement does not match `expected_measurement`.
    /// - [`TeeErrorKind::QuoteNonceMismatch`] when the embedded nonce
    ///   does not match `expected_nonce`.
    /// - [`TeeErrorKind::QuoteStale`] when the quote's TCB level is
    ///   below the current minimum.
    fn verify_quote(
        &self,
        quote: &Quote,
        expected_nonce: &Nonce,
        expected_measurement: &Measurement,
    ) -> Result<(), TeeError>;

    /// Seals `plaintext` under `policy`. The resulting [`SealedBlob`]
    /// can be persisted to untrusted storage; only the same TEE
    /// measurement (per `policy`) can unseal it later.
    ///
    /// # Errors
    ///
    /// - [`TeeErrorKind::SealFailed`] if the policy cannot be satisfied
    ///   on this platform.
    fn seal(&self, plaintext: &[u8], policy: &SealPolicy) -> Result<SealedBlob, TeeError>;

    /// Unseals `blob`, returning the original plaintext.
    ///
    /// The blob's embedded policy MUST be compatible with the current TEE
    /// measurement; otherwise [`TeeErrorKind::UnsealFailed`] is returned.
    ///
    /// # Errors
    ///
    /// - [`TeeErrorKind::UnsealFailed`] for any failure: tampering,
    ///   policy mismatch, post-migration measurement drift.
    fn unseal(&self, blob: &SealedBlob) -> Result<Vec<u8>, TeeError>;

    /// Derives a [`TeeSharedKey`] bound to a peer's attestation report.
    ///
    /// The returned key is suitable as the input keying material to
    /// HKDF for AEAD session keys. The semantic guarantee is that **only**
    /// the peer whose attestation produced `peer_attestation` can derive
    /// the same shared secret; even a compromise of the local OS cannot
    /// access the shared key without breaking the TEE.
    ///
    /// # Errors
    ///
    /// - [`TeeErrorKind::KeyDerivationFailed`] if the peer attestation is
    ///   invalid or the platform cannot bind keys to attestation reports.
    fn derive_key_for(&self, peer_attestation: &Quote) -> Result<TeeSharedKey, TeeError>;
}
