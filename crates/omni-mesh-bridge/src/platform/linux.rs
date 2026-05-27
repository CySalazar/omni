//! Linux-specific platform probes.

#[cfg(any(feature = "cvm", feature = "tpm2"))]
use super::{BackendKind, DetectedPlatform, TrustTier};
#[cfg(any(feature = "cvm", feature = "tpm2"))]
use std::path::Path;

/// Probes for confidential VM support on Linux.
///
/// Checks for `/dev/sev-guest` (AMD SEV-SNP) or `/dev/tdx-guest`
/// (Intel TDX), plus KVM availability for launching the CVM.
#[cfg(feature = "cvm")]
pub(super) fn probe_cvm() -> Option<DetectedPlatform> {
    let kvm_available = Path::new("/dev/kvm").exists();
    if !kvm_available {
        tracing::debug!("/dev/kvm not found — CVM mode unavailable");
        return None;
    }

    if Path::new("/dev/sev-guest").exists() || Path::new("/dev/sev").exists() {
        tracing::info!("AMD SEV-SNP device detected with KVM support");
        return Some(DetectedPlatform {
            max_tier: TrustTier::FullTee,
            backend: BackendKind::CvmSevSnp,
            cvm_available: true,
            platform_description: "Linux x86_64 — AMD SEV-SNP + KVM".into(),
        });
    }

    if Path::new("/dev/tdx-guest").exists() || Path::new("/dev/tdx_guest").exists() {
        tracing::info!("Intel TDX device detected with KVM support");
        return Some(DetectedPlatform {
            max_tier: TrustTier::FullTee,
            backend: BackendKind::CvmTdx,
            cvm_available: true,
            platform_description: "Linux x86_64 — Intel TDX + KVM".into(),
        });
    }

    tracing::debug!("no TEE guest device found — CVM mode unavailable");
    None
}

/// Probes for TPM 2.0 on Linux.
///
/// Checks for `/dev/tpmrm0` (kernel resource manager, preferred) or
/// `/dev/tpm0` (direct access).
#[cfg(feature = "tpm2")]
pub(super) fn probe_tpm2() -> Option<DetectedPlatform> {
    let tpm_path = if Path::new("/dev/tpmrm0").exists() {
        "/dev/tpmrm0"
    } else if Path::new("/dev/tpm0").exists() {
        "/dev/tpm0"
    } else {
        tracing::debug!("no TPM device found at /dev/tpmrm0 or /dev/tpm0");
        return None;
    };

    tracing::info!(device = tpm_path, "TPM 2.0 device detected");

    // Check if KVM is also available (to note CVM possibility even
    // when CVM feature is not enabled).
    let cvm_available = Path::new("/dev/kvm").exists()
        && (Path::new("/dev/sev-guest").exists() || Path::new("/dev/tdx-guest").exists());

    Some(DetectedPlatform {
        max_tier: TrustTier::MeasuredBoot,
        backend: BackendKind::Tpm2,
        cvm_available,
        platform_description: format!("Linux {} — TPM 2.0 at {tpm_path}", std::env::consts::ARCH),
    })
}
