//! Kernel-side Ed25519 signing key used to mint driver capability
//! tokens deposited at `DriverLoad` (P6.7.8.9, `OIP-013` § S5.3 step 8).
//!
//! ## Two distinct trust roots
//!
//! `OIP-013` deliberately separates two signing roles:
//!
//! 1. **Driver issuers** — the entities that sign `omni-pack v1`
//!    driver manifests. Their public keys live in
//!    [`crate::known_issuers::KNOWN_ISSUERS`] (`OIP-013` § S5.4) and
//!    are consumed by [`crate::driver_manifest::verify_manifest`] at
//!    `DriverLoad` time.
//! 2. **Kernel capability issuer** — *this* module. The kernel itself
//!    signs the `CapabilityToken`s it deposits in a freshly-spawned
//!    driver's address space so that subsequent
//!    `MmioMap`/`DmaMap`/`IrqAttach` syscalls passing those tokens
//!    can be authenticated against
//!    [`crate::capabilities::Ed25519CapabilityProvider`].
//!
//! Keeping the two roots separate means a compromise of a driver
//! issuer key does NOT let the attacker mint capability tokens, and
//! vice versa.
//!
//! ## DEV ONLY seed
//!
//! For Phase 1 the kernel signing seed is a fixed compile-time
//! constant: [`DRIVER_CAP_ISSUER_SEED`]. This mirrors the existing
//! placeholder pattern in [`crate::capabilities::Ed25519CapabilityProvider`]
//! (`node_id_bytes = [0u8; 32]`) and is **not** a security boundary —
//! it is a development scaffold that lets the rest of the deposit /
//! verify path land before TEE-derived sealing keys are available.
//!
//! Replacement plan (post-P5.2):
//! - On Intel TDX, derive the seed from a fixed-context `TDREPORT`
//!   sealing key (HKDF over `TDREPORT.measurement` + a domain
//!   separator).
//! - On AMD SEV-SNP, derive from `SNP_DERIVE_KEY` with the same
//!   domain-separator schema.
//! - The TEE-derived seed is then loaded into this module once per
//!   boot via a new initialiser (`init_from_sealing_key`) and the
//!   compile-time constant is removed.
//!
//! The replacement is tracked as a follow-up to P6.7.8.9 and will
//! require its own OIP (key custody policy + activation gate).

use omni_crypto::signing::OmniSigningKey;

/// 32-byte deterministic seed for the kernel driver-capability
/// issuer's Ed25519 signing key.
///
/// **DEV ONLY.** The value is a fixed pattern that is intentionally
/// not random — it is designed to be obviously a placeholder when
/// inspected (`0xCA, 0xFE, 0xBA, 0xBE` × 8).
///
/// Production substitute is documented in the module docstring.
pub const DRIVER_CAP_ISSUER_SEED: [u8; 32] = [
    0xCA, 0xFE, 0xBA, 0xBE, 0xCA, 0xFE, 0xBA, 0xBE, //
    0xCA, 0xFE, 0xBA, 0xBE, 0xCA, 0xFE, 0xBA, 0xBE, //
    0xCA, 0xFE, 0xBA, 0xBE, 0xCA, 0xFE, 0xBA, 0xBE, //
    0xCA, 0xFE, 0xBA, 0xBE, 0xCA, 0xFE, 0xBA, 0xBE, //
];

/// Construct the kernel's driver-capability issuer signing key.
///
/// The resulting [`OmniSigningKey`] owns its key material with
/// `ZeroizeOnDrop`; callers should hold it on the stack for the
/// minimum time needed to mint + sign the deposit batch.
#[must_use]
pub fn kernel_signing_key() -> OmniSigningKey {
    OmniSigningKey::from_bytes(DRIVER_CAP_ISSUER_SEED)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_round_trips_to_same_verifying_key() {
        // Both `OmniSigningKey::from_bytes` calls deterministically
        // derive the same public key — verified by comparing the
        // raw 32-byte representations.
        let k1 = kernel_signing_key();
        let k2 = kernel_signing_key();
        assert_eq!(k1.verifying_key().as_bytes(), k2.verifying_key().as_bytes());
    }

    #[test]
    fn signing_key_produces_verifiable_signature() {
        let key = kernel_signing_key();
        let vk = key.verifying_key();
        let msg = b"P6.7.8.9 cap deposit trampoline";
        let sig = key.sign(msg);
        // `verify` returns `Ok(())` only when the signature matches
        // the message under the public key.
        assert!(vk.verify(msg, &sig).is_ok());
    }

    #[test]
    fn seed_is_documented_placeholder_pattern() {
        // Assert the seed matches the documented `0xCAFEBABE × 8`
        // pattern. If a future PR changes the placeholder, this test
        // catches the drift and forces the documentation to follow.
        for chunk in DRIVER_CAP_ISSUER_SEED.chunks_exact(4) {
            assert_eq!(chunk, &[0xCA, 0xFE, 0xBA, 0xBE]);
        }
    }
}
