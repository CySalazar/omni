//! Kernel-side capability validation and minting.
//!
//! ## Status
//!
//! Two layers coexist here:
//!
//! 1. **Long-term capability table (P6.5+ scaffold).** Master signing
//!    keys held in TPM/Secure Enclave, indexed by `KernelCapabilityId`.
//!    Trait surface only; implementation lands with `omni-capability`
//!    integration (MB13+).
//! 2. **MB12 lightweight IPC capability check.** The
//!    [`KernelCapabilityCheck`] trait + [`StubCapabilityProvider`]
//!    implementation enable cross-process IPC gating without dragging
//!    `omni-capability` (and its `omni-crypto` SIMD dependency chain)
//!    into the kernel's bare-metal build today. ADR-0005 captures the
//!    security argument and the MB13 migration plan.
//!
//! ## Design rationale (long-term, layer 1)
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

// =============================================================================
// MB12 — Lightweight IPC capability check
// =============================================================================
//
// What's here, and what's not:
//
// - Subject identifier ([`KernelPrincipal`]) — a 32-byte opaque hash that
//   names a userspace authority. The kernel does not interpret the bits;
//   it only compares two principals for equality.
// - A two-variant [`KernelAction`] enum (`IpcSend` / `IpcRecv`) and a
//   one-variant [`KernelResource`] enum (`IpcChannel(u64)`). Mirror of
//   `omni-capability`'s vocabulary, restricted to what MB12 actually
//   uses.
// - A [`KernelCapabilityCheck`] trait + a [`StubCapabilityProvider`]
//   implementation. The stub does action/resource shape-matching but no
//   signature verification — Ed25519 verify becomes available with MB13
//   once `omni-crypto-verify` (or `omni-crypto` with SIMD intrinsics
//   gated) builds on `x86_64-unknown-none`.
//
// Why this is sound enough for Phase 1:
//
// - The `MessageEnvelope::sender` field is stamped by the kernel from
//   the active task id at SYSCALL entry, so it cannot be forged from
//   userspace.
// - Per-process CR3 (MB11) means a process cannot read or write another
//   process's address space without going through these syscalls — the
//   capability check is the only gate.
// - The token is held entirely inside the kernel after
//   `IpcCreateChannel`. Userspace cannot replay or mutate it.

/// 32-byte opaque principal identifier.
///
/// In MB13, this becomes a thin wrapper around
/// `omni_types::identity::NodeId`. For MB12 the kernel only needs byte
/// equality, so the bare-bones newtype here keeps `omni-types` (with
/// `id-generation` → `getrandom`) out of the kernel's bare-metal build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelPrincipal([u8; 32]);

impl KernelPrincipal {
    /// Construct a principal from a raw 32-byte hash.
    ///
    /// `from_bytes([0; 32])` is the conventional "unauthenticated kernel
    /// task" principal used for `kmain`, the idle task, and any
    /// kernel-spawned worker that has no userspace counterpart.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The all-zero principal — kernel-internal tasks (idle, bootstrap)
    /// adopt this so they can hold a PCB-shaped principal field without
    /// any authentication step.
    pub const ZERO: Self = Self([0u8; 32]);
}

/// What an IPC capability authorises.
///
/// MB12 only needs the send/recv pair; expanding the enum is an
/// `#[non_exhaustive]`-friendly additive change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KernelAction {
    /// May enqueue a message on the channel.
    IpcSend,
    /// May dequeue messages from the channel.
    IpcRecv,
}

/// The resource a [`KernelAction`] applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KernelResource {
    /// A specific IPC channel, identified by its kernel-allocated id.
    IpcChannel(u64),
}

/// Kernel-side capability token presented at `IpcCreateChannel`.
///
/// The kernel stores `subject` inside the channel entry (one slot per
/// direction) and compares each subsequent send/recv against the
/// running task's PCB principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelCapabilityToken {
    /// The authorised principal.
    pub subject: KernelPrincipal,
    /// The authorised action.
    pub action: KernelAction,
    /// The resource the action applies to.
    pub resource: KernelResource,
}

/// Verification outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityVerdict {
    /// The token is valid and authorises the requested action.
    Authorised,
    /// The token is rejected (signature failed in MB13; action/resource
    /// shape mismatch in MB12).
    Denied,
}

/// MB12 capability verifier. Exactly one implementation today
/// ([`StubCapabilityProvider`]); MB13 swaps in a real Ed25519 verifier
/// that consults a revocation list + TEE attestation table.
pub trait KernelCapabilityCheck {
    /// Verify that `token` authorises `action` on `resource`.
    fn verify(
        &self,
        token: &KernelCapabilityToken,
        action: KernelAction,
        resource: KernelResource,
    ) -> CapabilityVerdict;
}

/// MB12 stub: trusts the token verbatim, only verifying that its
/// `action`/`resource` fields match the requested operation. The
/// signature step is the MB13 follow-up.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubCapabilityProvider;

impl KernelCapabilityCheck for StubCapabilityProvider {
    fn verify(
        &self,
        token: &KernelCapabilityToken,
        action: KernelAction,
        resource: KernelResource,
    ) -> CapabilityVerdict {
        if token.action == action && token.resource == resource {
            CapabilityVerdict::Authorised
        } else {
            CapabilityVerdict::Denied
        }
    }
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

    #[test]
    fn kernel_principal_zero_is_all_zeroes() {
        assert_eq!(KernelPrincipal::ZERO.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn stub_authorises_matching_action_and_resource() {
        let token = KernelCapabilityToken {
            subject: KernelPrincipal::from_bytes([0x42; 32]),
            action: KernelAction::IpcSend,
            resource: KernelResource::IpcChannel(7),
        };
        let v = StubCapabilityProvider.verify(
            &token,
            KernelAction::IpcSend,
            KernelResource::IpcChannel(7),
        );
        assert_eq!(v, CapabilityVerdict::Authorised);
    }

    #[test]
    fn stub_denies_action_mismatch() {
        let token = KernelCapabilityToken {
            subject: KernelPrincipal::from_bytes([0x42; 32]),
            action: KernelAction::IpcSend,
            resource: KernelResource::IpcChannel(7),
        };
        let v = StubCapabilityProvider.verify(
            &token,
            KernelAction::IpcRecv,
            KernelResource::IpcChannel(7),
        );
        assert_eq!(v, CapabilityVerdict::Denied);
    }

    #[test]
    fn stub_denies_resource_mismatch() {
        let token = KernelCapabilityToken {
            subject: KernelPrincipal::from_bytes([0x42; 32]),
            action: KernelAction::IpcSend,
            resource: KernelResource::IpcChannel(7),
        };
        let v = StubCapabilityProvider.verify(
            &token,
            KernelAction::IpcSend,
            KernelResource::IpcChannel(8),
        );
        assert_eq!(v, CapabilityVerdict::Denied);
    }
}
