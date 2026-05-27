//! Signed auto-update mechanism.
//!
//! The bridge checks for updates from a Stichting OMNI-operated server
//! on startup and periodically. Updates are signed by the Stichting
//! OMNI release key and verified before application.
//!
//! ## Security properties
//!
//! - Update server is HTTPS with certificate pinning.
//! - Release artifacts are signed with Ed25519 (Sigstore).
//! - One previous version is kept on disk for rollback.
//! - `--skip-update` CLI flag disables auto-update for users who
//!   prefer manual updates or distro-packaged versions.

/// Update check interval in seconds (default: 24 hours).
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 86400;

/// Result of an update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Currently running the latest version.
    UpToDate,
    /// A newer version is available.
    Available {
        /// The new version string.
        version: String,
        /// SHA-256 hash of the new binary.
        sha256: String,
    },
    /// Update check failed (network error, server unavailable).
    CheckFailed {
        /// Human-readable error description.
        reason: String,
    },
}

/// Checks for available updates.
///
/// Contacts the update server over HTTPS and compares the latest
/// published version with the currently running version.
pub async fn check() -> UpdateStatus {
    // TODO(oip-025-phase-5): Implement update check.
    //
    // Steps:
    // 1. GET https://updates.omni-os.org/mesh-bridge/latest.json
    //    (certificate-pinned, timeout 10s).
    // 2. Parse response: { version, sha256, signature, download_url }.
    // 3. Compare version with env!("CARGO_PKG_VERSION").
    // 4. If newer, return UpdateStatus::Available.

    tracing::debug!("update check: not yet implemented");
    UpdateStatus::UpToDate
}

/// Downloads and applies an update.
///
/// # Safety
///
/// This replaces the running binary. The application must be restarted
/// after a successful update.
pub async fn apply(_status: &UpdateStatus) -> crate::Result<()> {
    // TODO(oip-025-phase-5): Implement update download and apply.
    //
    // Steps:
    // 1. Download new binary to a temporary file.
    // 2. Verify Ed25519 signature against the Stichting OMNI release key.
    // 3. Verify SHA-256 matches the manifest.
    // 4. Rename current binary to `omni-mesh-bridge.prev` (rollback).
    // 5. Move new binary to the current binary's path.
    // 6. Request application restart.

    tracing::debug!("update apply: not yet implemented");
    Ok(())
}

/// Rolls back to the previous version.
pub fn rollback() -> crate::Result<()> {
    // TODO(oip-025-phase-5): Rename `.prev` back to current.
    tracing::debug!("rollback: not yet implemented");
    Ok(())
}

/// Verifies the currently running binary against the Sigstore
/// transparency log.
pub fn verify_binary() -> crate::Result<bool> {
    // TODO(oip-025-phase-5): Compute SHA-256 of the running binary
    // and check against Sigstore CT log entries.
    tracing::debug!("binary verification: not yet implemented");
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn check_returns_up_to_date_for_now() {
        let status = check().await;
        assert_eq!(status, UpdateStatus::UpToDate);
    }
}
