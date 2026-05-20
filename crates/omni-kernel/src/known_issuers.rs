//! Static allowlist of Ed25519 public keys that the kernel trusts to sign
//! driver manifests.
//!
//! Specified by `OIP-Driver-Framework-013` § S5: the kernel MUST verify a
//! driver image's signature against an entry in this table at
//! `DriverLoad`. There is **no TOFU / runtime trust acquisition** path —
//! every issuer is baked at compile time so the trust base is explicit,
//! small, auditable, and reviewable in a single location.
//!
//! ## Provisioning workflow
//!
//! Issuers are added by editing `docs/protocol/driver-issuers.toml` and
//! re-running the kernel build. Each entry consists of an ASCII identifier
//! (logged on `DriverLoad` for auditability) and a 32-byte Ed25519
//! verifying key. The file format is intentionally trivial so a human
//! reviewer can verify it without tooling.
//!
//! ## Phase 1 state
//!
//! No first-party driver image has been signed yet — the table is
//! empty. The first issuer will be provisioned alongside the
//! `omni-driver-virtio-net` image (P6.7.8 M1). Until then, every
//! `DriverLoad` call returns `KernelError::CapabilityDenied` because
//! [`lookup_issuer`] cannot resolve any key.

use omni_crypto::signing::VERIFYING_KEY_LEN;

/// An entry in the static driver-issuer allowlist.
///
/// The `id` is a short ASCII label suitable for logging (e.g.
/// `"omni-os-stichting"`, `"omni-driver-team"`). The verifying key is
/// the bare 32-byte Ed25519 public key — wrapping in
/// [`omni_crypto::signing::OmniVerifyingKey`] is deferred to
/// [`lookup_issuer`] so the static table stays `const`-constructible.
#[derive(Debug, Clone, Copy)]
pub struct KnownIssuer {
    /// Stable issuer identifier. ASCII-only.
    pub id: &'static str,
    /// Ed25519 verifying key bytes (`VERIFYING_KEY_LEN = 32`).
    pub verifying_key: [u8; VERIFYING_KEY_LEN],
}

/// Static allowlist consulted by `DriverLoad`.
///
/// Currently empty: P6.7.8 will populate this when the first signed
/// driver image is provisioned. Keep the array as `&'static [...]`
/// (rather than a `const N: usize`) so adding entries is a one-line
/// edit that does not cascade into call-site array-length generics.
pub static KNOWN_ISSUERS: &[KnownIssuer] = &[
    // (intentionally empty — see module docs)
];

/// Look an issuer up by id. Returns `None` if the id is unknown.
///
/// Used by `DriverLoad` to resolve the manifest's declared issuer (a
/// short ASCII tag the kernel logs for auditability) to the Ed25519
/// public key it must verify the signature with. The constant-time
/// guarantee of the `subtle` crate is **not** required here because
/// the id is non-secret (the manifest is unencrypted on disk).
#[must_use]
pub fn lookup_issuer(id: &str) -> Option<&'static KnownIssuer> {
    KNOWN_ISSUERS.iter().find(|i| i.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_table_is_empty() {
        // This test pins the documented Phase 1 state. When the first
        // issuer is provisioned in P6.7.8, this test will fail and the
        // tester will update it to assert the actual entry — that's the
        // intended forcing function so the change is reviewed deliberately.
        assert!(
            KNOWN_ISSUERS.is_empty(),
            "KNOWN_ISSUERS no longer empty — update phase1_table_is_empty test"
        );
    }

    #[test]
    fn lookup_unknown_issuer_returns_none() {
        assert!(lookup_issuer("nobody").is_none());
        assert!(lookup_issuer("").is_none());
    }
}
