//! Kernel-side capability validation and minting.
//!
//! ## Status
//!
//! P6.5 scaffold. The kernel-side capability table holds the master
//! signing keys (in TPM / Secure Enclave) and a fast index by
//! capability ID. The userspace-facing capability *token* lives in
//! `omni_capability`; this module owns the kernel's authoritative
//! validation step.
//!
//! ## Design rationale
//!
//! - **Master signing keys never leave the kernel.** The kernel holds
//!   the signing keys inside the TPM or Secure Enclave; minting a
//!   capability requires the kernel to sign on behalf of an authorized
//!   caller. This makes the kernel's TCB the root of all capability
//!   trust.
//! - **Revocation lists.** A short-TTL revocation list per (subject,
//!   action) pair invalidates leaked capabilities quickly without
//!   storing them all.
//! - **TEE binding.** A capability is valid only when invoked from
//!   inside the TEE whose attestation the capability was minted
//!   against. The kernel cross-checks the calling TEE measurement on
//!   every validation.

#![allow(
    clippy::missing_errors_doc,
    reason = "trait scaffold methods return NotYetImplemented until OIP activates P6.5+"
)]

use crate::KernelResult;

// -----------------------------------------------------------------------------
// Capability identifier (kernel-side)
// -----------------------------------------------------------------------------

/// Kernel-internal capability identifier. Maps 1:1 to the userspace
/// `omni_capability::token::CapabilityId` but lives in a separate type
/// to prevent accidental confusion across the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KernelCapabilityId(pub u128);

// -----------------------------------------------------------------------------
// Capability table trait
// -----------------------------------------------------------------------------

/// Kernel capability table. Holds minted capabilities indexed by ID,
/// plus the revocation list.
pub trait CapabilityTable {
    /// Validates that `id` is currently a valid capability. Returns
    /// `Ok(())` if valid, `Err(CapabilityDenied)` otherwise. The
    /// implementation MUST also check that the calling TEE measurement
    /// matches the measurement the capability was minted against.
    fn validate(&self, id: KernelCapabilityId) -> KernelResult<()>;

    /// Records a capability as revoked. Subsequent `validate` calls
    /// for this id return `CapabilityDenied`.
    fn revoke(&mut self, id: KernelCapabilityId) -> KernelResult<()>;

    /// Returns the current size of the revocation list. Useful for
    /// alerting on suspicious activity (e.g., mass revocation).
    fn revocation_list_size(&self) -> usize;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_capability_id_round_trips() {
        let id = KernelCapabilityId(0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210u128);
        assert_eq!(id.0, 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210u128);
    }
}
