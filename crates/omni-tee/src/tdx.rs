//! Intel TDX backend — **scaffold only**.
//!
//! Feature-gated behind `tdx`. The trait surface is in place and the
//! implementation compiles, but every method returns
//! [`TeeErrorKind::Unsupported`] until P5.2 lands the real integration
//! with TDX firmware (quote generation via DCAP / QGS, quote verification
//! via PCK chain validation).
//!
//! ## TDX integration roadmap (P5.2)
//!
//! 1. Vendor library selection:
//!     - Option A: `tdx-attest-rs` (Intel-maintained Rust wrapper). License:
//!       BSD-3-Clause. Stable since 2024.
//!     - Option B: hand-rolled FFI to `libtdx_attest.so` (Intel C library).
//!       Smaller dep but more attack surface in unsafe code.
//!
//!    Decision via Standards-Track OIP at P5.2 kickoff.
//! 2. Quote generation flow:
//!    - Obtain a `tdx_report_t` from `ioctl(TDX_CMD_GET_REPORT0, ...)`.
//!    - Submit the report to the in-host Quoting Service (QGS) or to a
//!      remote PCCS for signing.
//!    - Wrap the resulting quote bytes in `Quote { body: ... }`.
//! 3. Quote verification flow:
//!    - Parse the quote header, body, and signature data.
//!    - Walk the PCK certificate chain to the Intel SGX Root CA.
//!    - Verify the PCK signature.
//!    - Cross-check the embedded MRTD, RTMRs, and TCB level against the
//!      caller-supplied allowlist.
//! 4. Sealing flow:
//!    - TDX does NOT provide native sealing in v1 hardware; the OMNI
//!      sealing layer derives a sealing key from the attested measurement
//!      via HKDF and uses ChaCha20-Poly1305 from `omni-crypto`.
//! 5. `derive_key_for` flow:
//!    - HKDF(IKM = `local_attest_secret` || `peer_quote.measurement`,
//!           info = "OMNI-PROTO-v0.1/tdx-derive").
//!    - The `local_attest_secret` is sealed at first boot and bound to
//!      the local TDX measurement.
//!
//! The `cfg(feature = "tdx")` gating lives on `pub mod tdx;` in
//! [`crate`]; we do not repeat it here.

use alloc::vec::Vec;

use crate::{
    attestation::{Measurement, Nonce, Quote},
    sealed_keys::{SealPolicy, SealedBlob, TeeSharedKey},
    traits::{TeeBackend, TeeError, TeeErrorKind, TeeFamily},
};

/// Intel TDX backend.
///
/// Construction succeeds even if TDX is not available; method calls
/// return [`TeeErrorKind::Unsupported`] in that case. This lets the
/// consumer construct the backend optimistically and detect TDX
/// availability lazily at the first call.
#[derive(Debug, Default)]
pub struct TdxBackend {
    /// Reserved for future configuration (PCCS URL, allowed PCK CA list,
    /// TCB level overrides). Empty in v0.1; the type exists so adding
    /// fields later is not a breaking change.
    _config: (),
}

impl TdxBackend {
    /// Constructs a default TDX backend.
    #[must_use]
    pub const fn new() -> Self {
        Self { _config: () }
    }

    /// Convenience helper used by every method to return the same
    /// "not implemented yet" error.
    fn not_yet_implemented(context: &'static str) -> TeeError {
        TeeError::new(TeeErrorKind::Unsupported, context)
    }
}

impl TeeBackend for TdxBackend {
    fn family(&self) -> TeeFamily {
        TeeFamily::IntelTdx
    }

    fn attest(&self, _nonce: &Nonce, _report_data: Option<&[u8]>) -> Result<Quote, TeeError> {
        // TODO(P5.2): produce TDX quote via DCAP / QGS.
        Err(Self::not_yet_implemented("tdx: attest not yet implemented"))
    }

    fn verify_quote(
        &self,
        _quote: &Quote,
        _expected_nonce: &Nonce,
        _expected_measurement: &Measurement,
    ) -> Result<(), TeeError> {
        // TODO(P5.2): parse quote, verify PCK chain, check MRTD/RTMRs/TCB.
        Err(Self::not_yet_implemented(
            "tdx: verify_quote not yet implemented",
        ))
    }

    fn seal(&self, _plaintext: &[u8], _policy: &SealPolicy) -> Result<SealedBlob, TeeError> {
        // TODO(P5.2): HKDF(local_attest_secret) → AEAD seal.
        Err(Self::not_yet_implemented("tdx: seal not yet implemented"))
    }

    fn unseal(&self, _blob: &SealedBlob) -> Result<Vec<u8>, TeeError> {
        // TODO(P5.2): HKDF(local_attest_secret) → AEAD open.
        Err(Self::not_yet_implemented("tdx: unseal not yet implemented"))
    }

    fn derive_key_for(&self, _peer_attestation: &Quote) -> Result<TeeSharedKey, TeeError> {
        // TODO(P5.2): HKDF over local_attest_secret + peer measurement.
        Err(Self::not_yet_implemented(
            "tdx: derive_key_for not yet implemented",
        ))
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn family_is_intel_tdx() {
        let b = TdxBackend::new();
        assert_eq!(b.family(), TeeFamily::IntelTdx);
    }

    #[test]
    fn every_method_returns_unsupported() {
        let b = TdxBackend::new();
        let nonce = Nonce::zero();
        let m = Measurement::zero();
        let policy = SealPolicy::new(TeeFamily::IntelTdx, m);
        let body = alloc::vec![0u8; 1];
        let quote = Quote {
            version: crate::attestation::QuoteVersion::V0_1,
            family: TeeFamily::IntelTdx,
            measurement: m,
            nonce,
            report_data: None,
            body,
        };

        assert_eq!(
            b.attest(&nonce, None).unwrap_err().kind,
            TeeErrorKind::Unsupported
        );
        assert_eq!(
            b.verify_quote(&quote, &nonce, &m).unwrap_err().kind,
            TeeErrorKind::Unsupported
        );
        assert_eq!(
            b.seal(&[1u8, 2, 3], &policy).unwrap_err().kind,
            TeeErrorKind::Unsupported
        );
        let blob = SealedBlob {
            envelope_version: SealedBlob::CURRENT_ENVELOPE_VERSION,
            policy,
            ciphertext: alloc::vec![0u8; 4],
        };
        assert_eq!(b.unseal(&blob).unwrap_err().kind, TeeErrorKind::Unsupported);
        assert_eq!(
            b.derive_key_for(&quote).unwrap_err().kind,
            TeeErrorKind::Unsupported
        );
    }
}
