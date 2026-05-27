//! Confidential MicroVM backend (Tier 0).
//!
//! Launches a minimal OMNI OS image inside a hardware-encrypted VM
//! (SEV-SNP or TDX). Communication with the CVM is over virtio-vsock.
//!
//! ## VMM selection
//!
//! | Platform | VMM                     |
//! |----------|-------------------------|
//! | Linux    | Cloud Hypervisor (Rust) |
//! | Windows  | Hyper-V via WHP API     |
//!
//! ## Image
//!
//! The CVM runs `omni-mesh-bridge-cvm.img`, a minimal OMNI OS image
//! containing only the microkernel, `omni-tee` driver (TDX or SEV-SNP),
//! `omni-mesh` client, and a virtio-vsock driver. Target size: ≤ 64 MiB.

#[cfg(feature = "cvm")]
use super::DynBackend;
#[cfg(feature = "cvm")]
use crate::platform::DetectedPlatform;
#[cfg(feature = "cvm")]
use crate::BridgeError;

/// Path to the CVM image, relative to the application data directory.
pub const CVM_IMAGE_FILENAME: &str = "omni-mesh-bridge-cvm.img";

/// Default memory allocation for the CVM in MiB.
pub const CVM_DEFAULT_MEMORY_MIB: u32 = 512;

/// Default number of vCPUs for the CVM.
pub const CVM_DEFAULT_VCPUS: u32 = 2;

/// vsock port used for host ↔ CVM control channel.
pub const VSOCK_CONTROL_PORT: u32 = 5100;

/// vsock port used for host ↔ CVM mesh traffic relay.
pub const VSOCK_MESH_PORT: u32 = 5101;

/// Initializes the CVM backend.
///
/// Steps:
/// 1. Locate the CVM image on disk.
/// 2. Verify its signature against the Stichting OMNI release key.
/// 3. Launch the VMM with SEV-SNP or TDX enabled.
/// 4. Wait for the CVM to boot and report attestation via vsock.
/// 5. Return a `TeeBackend` proxy that delegates to the CVM.
#[cfg(feature = "cvm")]
pub fn init(_platform: &DetectedPlatform) -> crate::Result<DynBackend> {
    // TODO(oip-025-phase-4): Full CVM launch implementation.
    //
    // Implementation outline:
    // - Linux: spawn Cloud Hypervisor with --api-socket, --kernel,
    //   --memory size=512M, --cpus boot=2, --vsock cid=3
    //   --platform sev_snp=on (or tdx=on)
    // - Windows: use WHP API to create a VM partition with
    //   SEV-SNP/TDX isolation.
    // - Verify CVM boot via vsock handshake within 10s timeout.
    // - Return CvmProxyBackend that forwards TeeBackend calls
    //   over vsock to the OMNI mesh node inside the CVM.

    Err(BridgeError::BackendInit(
        "CVM backend not yet implemented — see OIP-025 Phase 4".into(),
    ))
}
