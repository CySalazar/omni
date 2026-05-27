//! # `omni-spark`
//!
//! **OMNI Spark** — cross-platform desktop application that enables
//! Linux, Windows, and macOS users to participate in the OMNI OS mesh
//! at the highest trust tier their hardware supports.
//!
//! ## Architecture (OIP-025)
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                       OMNI Spark                             │
//! │                                                              │
//! │  ┌────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
//! │  │  Platform   │  │  Tier        │  │  Mesh Protocol       │ │
//! │  │  Detector   │  │  Provisioner │  │  Client              │ │
//! │  └──────┬──────┘  └──────┬───────┘  └──────────┬───────────┘ │
//! │         │                │                      │            │
//! │  ┌──────▼────────────────▼──────────────────────▼─────────┐ │
//! │  │                Security Substrate                       │ │
//! │  │  ┌────────┐  ┌──────────┐  ┌───────┐  ┌────────────┐  │ │
//! │  │  │  CVM   │  │ Platform │  │ TPM   │  │ Software   │  │ │
//! │  │  │ Tier 0 │  │ Enclave  │  │ 2.0   │  │ MPC        │  │ │
//! │  │  │        │  │ Tier 1   │  │Tier 2 │  │ Tier 3     │  │ │
//! │  │  └────────┘  └──────────┘  └───────┘  └────────────┘  │ │
//! │  └────────────────────────────────────────────────────────┘ │
//! │                                                              │
//! │  ┌────────────────────────────────────────────────────────┐  │
//! │  │               System Tray / Status UI                   │  │
//! │  └────────────────────────────────────────────────────────┘  │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Modules
//!
//! - [`platform`] — host hardware detection and tier assignment.
//! - [`backend`] — security substrate backends (CVM, SE, TPM, MPC).
//! - [`mesh_client`] — mesh protocol client for `std` targets.
//! - [`hardening`] — application-level security hardening.
//! - [`ui`] — system tray and status interface.
//! - [`config`] — user configuration and persistence.
//! - [`update`] — signed auto-update mechanism.

#![doc(html_root_url = "https://docs.omni-os.org/omni-spark")]
#![warn(missing_docs)]

pub mod platform;
pub mod backend;
pub mod mesh_client;
pub mod hardening;
pub mod ui;
pub mod config;
pub mod update;

/// Application-wide result type.
pub type Result<T> = std::result::Result<T, BridgeError>;

/// Top-level error type for OMNI Spark.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// Platform detection failed.
    #[error("platform detection failed: {0}")]
    PlatformDetection(String),

    /// The selected backend could not be initialized.
    #[error("backend initialization failed: {0}")]
    BackendInit(String),

    /// Mesh protocol error.
    #[error("mesh protocol error: {0}")]
    MeshProtocol(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// The requested operation requires a higher trust tier.
    #[error("tier insufficient: have {have}, need {need}")]
    TierInsufficient {
        /// The node's current tier.
        have: u8,
        /// The minimum tier required for the operation.
        need: u8,
    },

    /// TEE backend error (delegated from `omni-tee`).
    #[error("tee backend: {0}")]
    Tee(#[from] omni_tee::TeeError),
}

/// Initializes OMNI Spark.
///
/// This is the main entry point called from `main.rs`. It:
/// 1. Detects the host platform and available security primitives.
/// 2. Provisions the highest-tier backend.
/// 3. Connects to the mesh.
/// 4. Launches the system tray UI.
///
/// Returns only on shutdown (user quit or fatal error).
pub async fn run() -> Result<()> {
    // Phase 1: Detect platform capabilities
    let detected = platform::detect()?;
    tracing::info!(
        tier = detected.max_tier as u8,
        backend = %detected.backend_name(),
        cvm_available = detected.cvm_available,
        "platform detection complete"
    );

    // Phase 2: Provision the security backend
    let _backend = backend::provision(&detected)?;
    tracing::info!("security backend provisioned");

    // Phase 3: Apply application hardening
    hardening::apply()?;
    tracing::info!("application hardening applied");

    // Phase 4: Connect to the mesh
    // TODO(oip-025-phase-1): mesh_client::connect(&backend).await?;

    // Phase 5: Launch UI
    // TODO(oip-025-phase-5): ui::run_tray(&detected).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
