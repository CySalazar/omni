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

// =============================================================================
// MB13.c — Ed25519CapabilityProvider
// =============================================================================
//
// What this delivers vs. what's still pending:
//
// - **Delivered (MB13.c):** real Ed25519 signature verification of
//   `omni_capability::CapabilityToken` blobs presented to the kernel.
//   The provider also wraps the existing per-IPC shape-matching path so
//   it can drop-in replace [`StubCapabilityProvider`] without touching
//   the IPC registry.
// - **Deferred (MB13.d):** the `IpcCreateChannel` syscall ABI extension
//   that actually plumbs signed tokens from user space into the kernel.
//   Until that lands, MB13.c provides the verification *machinery*
//   (callable from kernel-internal tests and future syscall handlers)
//   but is not yet wired into the IPC boot path. `StubCapabilityProvider`
//   therefore remains the boot-wiring default until MB13.d.
//
// Design rationale:
//
// - Per-IPC checks (`KernelCapabilityCheck::verify`) stay O(1) shape
//   matching — re-verifying an Ed25519 signature on every IPC send/recv
//   would add ~50 µs per call (RustCrypto soft backend on bare-metal)
//   for no security benefit, because the signature is invariant once
//   accepted at channel creation.
// - Signature verification is exposed as a dedicated method
//   `verify_signed_token` that the MB13.d `IpcCreateChannel` handler
//   will call ONCE per channel.
// - The provider carries the kernel's own `NodeId` (used as the TEE
//   attestation source against which `payload.subject` is matched).
//   For MB13.c this is a fixed all-zero placeholder until `omni-tee`
//   (P5) supplies a real attested identity.

/// MB13.c — Ed25519-backed capability verifier.
///
/// Verifies the signature, time window, and TEE-binding of an
/// [`omni_capability::CapabilityToken`] by delegating to
/// [`omni_capability::CapabilityToken::verify_full`] with a fixed
/// [`StubAttestation`] sourced from `self.node_id` and an empty
/// [`RevocationList`]. Per-IPC checks fall back to the same
/// shape-matching semantics as [`StubCapabilityProvider`].
#[derive(Debug, Clone, Copy)]
pub struct Ed25519CapabilityProvider {
    /// `NodeId` this kernel claims as its TEE attestation identity. For
    /// MB13.c this is the all-zero placeholder; a real attested value
    /// will come from `omni-tee` (P5) once a TEE backend lands.
    node_id_bytes: [u8; 32],
}

impl Ed25519CapabilityProvider {
    /// Construct a provider bound to the supplied 32-byte TEE
    /// attestation hash.
    #[must_use]
    pub const fn with_node_id(node_id_bytes: [u8; 32]) -> Self {
        Self { node_id_bytes }
    }

    /// Construct a provider bound to the all-zero placeholder node id.
    /// MB13.c uses this until `omni-tee` provides a real attested id.
    #[must_use]
    pub const fn placeholder() -> Self {
        Self::with_node_id([0u8; 32])
    }

    /// Verify the token's signature only (no time / TEE / revocation
    /// checks).
    ///
    /// Used by tests and by paths that have already validated the time
    /// window through a separate clock source.
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "Signature verification reads the token's embedded `issuer` key only; the \
                  provider's stored `node_id_bytes` becomes relevant only in `verify_signed_token`. \
                  Keeping the method on `&self` mirrors the `verify_signed_token` shape so callers \
                  can swap between the two without restructuring."
    )]
    pub fn verify_signature_only(
        &self,
        token: &omni_capability::CapabilityToken,
    ) -> CapabilityVerdict {
        match token.verify_signature() {
            Ok(()) => CapabilityVerdict::Authorised,
            Err(_) => CapabilityVerdict::Denied,
        }
    }

    /// Full verification: signature, time window, TEE binding, and
    /// revocation status — i.e. exactly what
    /// [`omni_capability::CapabilityToken::verify_full`] checks.
    ///
    /// The TEE attestation source is a [`StubAttestation`] bound to
    /// `self.node_id_bytes`; the revocation list is empty (per-channel
    /// revocation lands with MB13.d). Callers MUST supply a monotonic
    /// `now` — at boot the kernel uses RTC seconds via
    /// [`crate::bare_metal::arch::rtc_seconds`].
    #[must_use]
    pub fn verify_signed_token(
        &self,
        token: &omni_capability::CapabilityToken,
        now: u64,
    ) -> CapabilityVerdict {
        let attestation = omni_capability::tee::StubAttestation {
            fixed_node_id: omni_types::identity::NodeId::from_attestation_hash(self.node_id_bytes),
        };
        let revocation = omni_capability::revocation::RevocationList::new();
        match token.verify_full(now, &attestation, &revocation) {
            Ok(()) => CapabilityVerdict::Authorised,
            Err(_) => CapabilityVerdict::Denied,
        }
    }
}

impl Default for Ed25519CapabilityProvider {
    fn default() -> Self {
        Self::placeholder()
    }
}

impl KernelCapabilityCheck for Ed25519CapabilityProvider {
    #[allow(
        clippy::unused_self,
        reason = "Per-IPC path keeps the same shape-match semantics as StubCapabilityProvider; \
                  the provider's `node_id_bytes` is consumed by `verify_signed_token`, not here."
    )]
    fn verify(
        &self,
        token: &KernelCapabilityToken,
        action: KernelAction,
        resource: KernelResource,
    ) -> CapabilityVerdict {
        // Per-IPC path: O(1) action/resource shape match — same
        // semantics as `StubCapabilityProvider`. Ed25519 signature
        // verification is a one-shot operation done at channel
        // creation via `verify_signed_token` (MB13.d).
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

    // -------------------------------------------------------------------
    // MB13.c — Ed25519CapabilityProvider
    // -------------------------------------------------------------------
    //
    // The host test build has the full `omni-capability` userspace
    // features (`mint`, `id-generation`, `rng`) available, so we can
    // mint real Ed25519-signed tokens and exercise the verify path
    // end-to-end. The bare-metal build of `omni-kernel` does not
    // include these tests (the `cfg(test)` gate keeps them off).

    use omni_capability::{
        CapabilityToken,
        scope::{Action, Resource, Scope, TimeWindow},
    };
    use omni_crypto::signing::OmniSigningKey;
    use omni_types::identity::NodeId;

    fn fresh_ipc_token(channel_id: u64) -> (CapabilityToken, NodeId, OmniSigningKey, u64) {
        let issuer_key = OmniSigningKey::generate();
        let node_bytes = [0xAB; 32];
        let node_id = NodeId::from_attestation_hash(node_bytes);
        let scope = Scope {
            action: Action::IpcSend,
            resource: Resource::IpcChannel(channel_id),
            window: TimeWindow::new(100, 200).expect("valid window"),
            caveats: alloc::vec::Vec::new(),
        };
        let token = CapabilityToken::mint(&issuer_key, node_id, scope, None).expect("mint");
        // `now` chosen inside the window.
        (token, node_id, issuer_key, 150)
    }

    #[test]
    fn ed25519_signature_only_accepts_freshly_minted_token() {
        let (token, _node, _key, _now) = fresh_ipc_token(7);
        let provider = Ed25519CapabilityProvider::placeholder();
        assert_eq!(
            provider.verify_signature_only(&token),
            CapabilityVerdict::Authorised
        );
    }

    #[test]
    fn ed25519_signature_only_rejects_tampered_payload() {
        let (mut token, _node, _key, _now) = fresh_ipc_token(7);
        // Mutate the scope window so the signature pre-image changes.
        token.payload.scope.window = TimeWindow::new(0, u64::MAX).expect("widened window");
        let provider = Ed25519CapabilityProvider::placeholder();
        assert_eq!(
            provider.verify_signature_only(&token),
            CapabilityVerdict::Denied
        );
    }

    #[test]
    fn ed25519_verify_full_accepts_inside_window_with_matching_node() {
        let (token, node, _key, now) = fresh_ipc_token(7);
        let provider = Ed25519CapabilityProvider::with_node_id(*node.as_bytes());
        assert_eq!(
            provider.verify_signed_token(&token, now),
            CapabilityVerdict::Authorised
        );
    }

    #[test]
    fn ed25519_verify_full_rejects_outside_window() {
        let (token, node, _key, _now) = fresh_ipc_token(7);
        let provider = Ed25519CapabilityProvider::with_node_id(*node.as_bytes());
        // `now = 50` is before the token's `not_before = 100`.
        assert_eq!(
            provider.verify_signed_token(&token, 50),
            CapabilityVerdict::Denied
        );
        // `now = 250` is past the token's `not_after = 200`.
        assert_eq!(
            provider.verify_signed_token(&token, 250),
            CapabilityVerdict::Denied
        );
    }

    #[test]
    fn ed25519_verify_full_rejects_attestation_mismatch() {
        // Mint a token bound to `node_a`; verify with a provider
        // pretending to be `node_b`. TEE binding must fail.
        let (token, _node_a, _key, now) = fresh_ipc_token(7);
        let provider = Ed25519CapabilityProvider::with_node_id([0xCC; 32]);
        assert_eq!(
            provider.verify_signed_token(&token, now),
            CapabilityVerdict::Denied
        );
    }

    #[test]
    fn ed25519_per_ipc_check_matches_stub_semantics() {
        // The per-IPC `verify` call MUST keep shape-matching semantics
        // so Ed25519CapabilityProvider can drop-in replace
        // StubCapabilityProvider without regressing MB12 behaviour.
        let token = KernelCapabilityToken {
            subject: KernelPrincipal::from_bytes([0x42; 32]),
            action: KernelAction::IpcSend,
            resource: KernelResource::IpcChannel(11),
        };
        let provider = Ed25519CapabilityProvider::placeholder();
        assert_eq!(
            provider.verify(
                &token,
                KernelAction::IpcSend,
                KernelResource::IpcChannel(11)
            ),
            CapabilityVerdict::Authorised
        );
        assert_eq!(
            provider.verify(
                &token,
                KernelAction::IpcRecv,
                KernelResource::IpcChannel(11)
            ),
            CapabilityVerdict::Denied
        );
        assert_eq!(
            provider.verify(
                &token,
                KernelAction::IpcSend,
                KernelResource::IpcChannel(12)
            ),
            CapabilityVerdict::Denied
        );
    }
}
