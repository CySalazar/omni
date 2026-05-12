//! AMD SEV-SNP backend — **scaffold only**.
//!
//! Feature-gated behind `sev-snp`. Same shape as [`crate::tdx::TdxBackend`]:
//! trait surface implemented, every method returns
//! [`TeeErrorKind::Unsupported`] until P5.3 lands the real integration.
//!
//! ## SEV-SNP integration roadmap (P5.3)
//!
//! 1. Vendor library selection:
//!    - Option A: `sev` crate (Red Hat, maintained, MIT/Apache-2.0).
//!    - Option B: `snpguest` crate (AMD reference, GPL-2; **rejected** —
//!      incompatible with our AGPL-3.0+commercial dual-licensing).
//!    - Option C: hand-rolled FFI to `psp-ioctl` (`/dev/sev-guest`).
//! 2. Attestation report request:
//!    - `ioctl(SNP_GET_REPORT, ...)` returning the attestation report
//!      with embedded `REPORT_DATA` field (set to the OMNI nonce +
//!      transcript hash).
//!    - Wrap in `Quote { body: serialized_report }`.
//! 3. Report verification:
//!    - Parse the AMD attestation report (v2 layout, ABI 1.55).
//!    - Walk the VCEK certificate chain to AMD root.
//!    - Verify ECDSA-P384 signature over the report.
//!    - Cross-check `MEASUREMENT`, `REPORTED_TCB`, `PLATFORM_INFO` against
//!      the allowlist.
//! 4. Sealing flow: same approach as TDX (HKDF over an attested local
//!    secret + AEAD).
//! 5. `derive_key_for`: same HKDF pattern as TDX.
//!
//! The `cfg(feature = "sev-snp")` gating lives on `pub mod sev_snp;` in
//! [`crate`]; we do not repeat it here.

use alloc::vec::Vec;

use crate::{
    attestation::{Measurement, Nonce, Quote},
    sealed_keys::{SealPolicy, SealedBlob, TeeSharedKey},
    traits::{TeeBackend, TeeError, TeeErrorKind, TeeFamily},
};

/// AMD SEV-SNP backend.
#[derive(Debug, Default)]
pub struct SevSnpBackend {
    /// Reserved for future configuration. Empty in v0.1.
    _config: (),
}

impl SevSnpBackend {
    /// Constructs a default SEV-SNP backend.
    #[must_use]
    pub const fn new() -> Self {
        Self { _config: () }
    }

    fn not_yet_implemented(context: &'static str) -> TeeError {
        TeeError::new(TeeErrorKind::Unsupported, context)
    }
}

impl TeeBackend for SevSnpBackend {
    fn family(&self) -> TeeFamily {
        TeeFamily::AmdSevSnp
    }

    fn attest(&self, _nonce: &Nonce, _report_data: Option<&[u8]>) -> Result<Quote, TeeError> {
        Err(Self::not_yet_implemented(
            "sev-snp: attest not yet implemented",
        ))
    }

    fn verify_quote(
        &self,
        _quote: &Quote,
        _expected_nonce: &Nonce,
        _expected_measurement: &Measurement,
    ) -> Result<(), TeeError> {
        Err(Self::not_yet_implemented(
            "sev-snp: verify_quote not yet implemented",
        ))
    }

    fn seal(&self, _plaintext: &[u8], _policy: &SealPolicy) -> Result<SealedBlob, TeeError> {
        Err(Self::not_yet_implemented(
            "sev-snp: seal not yet implemented",
        ))
    }

    fn unseal(&self, _blob: &SealedBlob) -> Result<Vec<u8>, TeeError> {
        Err(Self::not_yet_implemented(
            "sev-snp: unseal not yet implemented",
        ))
    }

    fn derive_key_for(&self, _peer_attestation: &Quote) -> Result<TeeSharedKey, TeeError> {
        Err(Self::not_yet_implemented(
            "sev-snp: derive_key_for not yet implemented",
        ))
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_is_amd_sev_snp() {
        let b = SevSnpBackend::new();
        assert_eq!(b.family(), TeeFamily::AmdSevSnp);
    }
}
