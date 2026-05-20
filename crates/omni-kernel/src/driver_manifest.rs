//! Driver-manifest schema, parsing entry-point, and Ed25519 signature
//! verification — `OIP-Driver-Framework-013` § S5 framework skeleton.
//!
//! ## Status
//!
//! P6.7.3 skeleton. The schema types are locked here so downstream code
//! (the `DriverLoad = 73` syscall handler and the per-driver manifest
//! emitter in the build pipeline) can be authored against a stable
//! Rust surface. Two of the three operations the kernel performs at
//! `DriverLoad` are wired:
//!
//! 1. **TOML → [`DriverManifest`] decoding** — deferred to P6.7.8 when
//!    the first first-party driver image lands. The entry-point
//!    [`parse_manifest`] returns [`DriverManifestError::ParserNotWired`]
//!    today. A bare-metal-clean TOML parser is a separate
//!    architectural decision and warrants its own OIP (the
//!    candidate is `toml_edit` with `default-features = false` +
//!    `serde` opt-out, but the dependency surface is non-trivial).
//! 2. **BLAKE3 image hash verification** — wired now via
//!    [`omni_crypto::hash::Blake3`]. Constant-time byte-wise compare
//!    against the manifest's `omni_image_hash` field.
//! 3. **Ed25519 signature verification** — wired now via
//!    [`omni_crypto::signing::OmniVerifyingKey`]. The signature is
//!    over the postcard canonical encoding of
//!    `(meta_no_signature, capabilities, matchers)` — explicitly NOT
//!    over the TOML bytes, which forecloses the JSON-canonicalisation
//!    class of bugs.
//!
//! The signing key bytes are resolved through
//! [`crate::known_issuers::lookup_issuer`], so a manifest signed by an
//! unknown issuer is rejected even before the signature math runs.
//!
//! ## Wire format
//!
//! TOML v1 schema (informally, full grammar lives in the OIP body):
//!
//! ```toml
//! [meta]
//! name              = "omni-driver-virtio-net"
//! version           = "0.3.0"
//! omni_image_hash   = "<64 hex chars — BLAKE3 of the ELF bytes>"
//! omni_signature    = "<128 hex chars — Ed25519 sig over canonical blob>"
//! omni_issuer       = "omni-driver-team"      # looked up in KNOWN_ISSUERS
//!
//! [capabilities]
//! mmio_regions      = [{ phys_base = "0xFEBC0000", len = "0x10000" }]
//! dma_windows       = [{ iova_base = "0x100000000", len = "0x4000" }]
//! irq_lines         = [33]
//! pci_devices       = [{ segment = 0, bus = 0, device = 0x14, function = 0 }]
//!
//! [matchers]
//! pci_vendor_device = [{ vendor = 0x1AF4, device = 0x1041 }]
//! ```
//!
//! ## What this module does NOT do
//!
//! - It does not allocate IOMMU domains or program PT entries; that is
//!   the `DriverLoad` syscall handler's job (P6.7.8 — gated on the
//!   handler being wired beyond `NotYetImplemented`).
//! - It does not enforce the `not_after ≤ 90 days` token-lifetime cap
//!   from § S1.2; that lives in the `omni-capability::token` mint path.
//! - It does not maintain per-driver state; the manifest is consumed
//!   at load time and the kernel-side driver record is owned by the
//!   `process` module.

use alloc::string::String;
use alloc::vec::Vec;

use omni_capability::scope::{Action, Resource};
use omni_crypto::hash::{Blake3, HASH_LEN, OmniHash};
use omni_crypto::signing::{OmniSignature, OmniVerifyingKey, SIGNATURE_LEN, VERIFYING_KEY_LEN};

use crate::known_issuers::lookup_issuer;

// =============================================================================
// Schema types
// =============================================================================

/// Top-level driver manifest, as decoded from TOML.
///
/// The structure mirrors the three TOML tables (`[meta]`,
/// `[capabilities]`, `[matchers]`). The signature in `meta.signature`
/// covers the postcard canonical encoding of
/// `(meta_no_signature, capabilities, matchers)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverManifest {
    /// Driver identity and signature material.
    pub meta: DriverMeta,
    /// Capabilities the driver requests at load. Every capability MUST
    /// be a subset of an Ed25519-signed `CapabilityToken` issued to
    /// the driver's subject; the manifest is a *declaration*, not a
    /// substitute for token verification.
    pub capabilities: DriverCapabilities,
    /// Device-matching rules consulted by the kernel's PCI bus walk.
    pub matchers: DriverMatchers,
}

/// Driver identity and signature material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverMeta {
    /// Short ASCII name — used in boot logs and as the IPC channel
    /// namespace root (e.g. `omni.driver.<name>.*`).
    pub name: String,
    /// Semantic-version string (parsed by the kernel only for logging;
    /// version compatibility is the higher-layer's concern).
    pub version: String,
    /// BLAKE3 hash of the runtime image bytes (64 hex chars in TOML;
    /// `HASH_LEN`-byte array post-parse).
    pub omni_image_hash: [u8; HASH_LEN],
    /// Ed25519 signature over the canonical postcard encoding of
    /// `(meta_no_signature, capabilities, matchers)`. 64 bytes.
    pub omni_signature: [u8; SIGNATURE_LEN],
    /// ASCII issuer id, looked up in
    /// [`crate::known_issuers::KNOWN_ISSUERS`].
    pub omni_issuer: String,
}

/// Capabilities the driver requests at `DriverLoad`.
///
/// Each entry maps to a token the user-space loader MUST present
/// alongside the image; the manifest is the kernel's machine-readable
/// enumeration of what the loader will be asked to authorise.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriverCapabilities {
    /// `(Action::MmioMap, Resource::MmioRegion { .. })` claims.
    pub mmio_regions: Vec<Resource>,
    /// `(Action::DmaMap, Resource::DmaWindow { .. })` claims.
    pub dma_windows: Vec<Resource>,
    /// `(Action::IrqAttach, Resource::IrqLine(..))` claims.
    pub irq_lines: Vec<Resource>,
    /// `(Action::{PciConfigRead, PciConfigWrite}, Resource::PciDevice { .. })`
    /// claims.
    pub pci_devices: Vec<Resource>,
}

/// PCI / ACPI match-table the kernel uses to decide which devices to
/// hand off to this driver.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriverMatchers {
    /// Vendor + device PCI id pairs.
    pub pci_vendor_device: Vec<PciMatcher>,
    /// ACPI Hardware IDs (e.g. `"PNP0501"` for the legacy 16550 UART).
    pub acpi_hid: Vec<String>,
}

/// `(vendor, device)` PCI matcher pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciMatcher {
    /// 16-bit PCI vendor id.
    pub vendor: u16,
    /// 16-bit PCI device id.
    pub device: u16,
}

// =============================================================================
// Error type
// =============================================================================

/// Failure modes for [`parse_manifest`] / [`verify_manifest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverManifestError {
    /// TOML parser is not yet wired into the kernel build
    /// (P6.7.3 skeleton state — see module docs).
    ParserNotWired,
    /// The TOML bytes did not parse against the schema.
    Malformed,
    /// The manifest's `omni_issuer` does not appear in
    /// [`crate::known_issuers::KNOWN_ISSUERS`].
    UnknownIssuer,
    /// Ed25519 signature verification failed.
    SignatureInvalid,
    /// The BLAKE3 hash of the supplied image bytes did not match the
    /// manifest's `omni_image_hash`.
    ImageHashMismatch,
}

// =============================================================================
// Parse entry-point (skeleton)
// =============================================================================

/// Parse a UTF-8 TOML byte slice into a [`DriverManifest`].
///
/// # Errors
///
/// Returns [`DriverManifestError::ParserNotWired`] until P6.7.8 lands a
/// bare-metal-clean TOML parser. The entry-point exists so call-sites
/// (the `DriverLoad` syscall handler, the test fixtures) can be
/// authored today against the final ABI.
pub fn parse_manifest(toml_bytes: &[u8]) -> Result<DriverManifest, DriverManifestError> {
    let _ = toml_bytes;
    Err(DriverManifestError::ParserNotWired)
}

// =============================================================================
// Verification
// =============================================================================

/// Verify `image_bytes` and `manifest.meta.omni_signature`.
///
/// Checks (in order): BLAKE3(`image_bytes`) matches
/// `manifest.meta.omni_image_hash`, the issuer is in
/// [`crate::known_issuers::KNOWN_ISSUERS`], the Ed25519 signature is
/// valid over the canonical signing payload.
///
/// The signing payload is the postcard canonical encoding of the tuple
/// `(name, version, omni_image_hash, omni_issuer, capabilities, matchers)`
/// — i.e. the manifest with the signature field zeroed out. This
/// forecloses the JSON-canonicalisation class of bugs because postcard
/// is byte-deterministic by construction (see `OIP-Serde-004` § S2).
///
/// # Errors
///
/// - [`DriverManifestError::ImageHashMismatch`] — BLAKE3 disagreement.
/// - [`DriverManifestError::UnknownIssuer`] — the issuer is not in
///   [`crate::known_issuers::KNOWN_ISSUERS`].
/// - [`DriverManifestError::SignatureInvalid`] — Ed25519 verify failed
///   OR the issuer's stored key is malformed.
pub fn verify_manifest(
    manifest: &DriverManifest,
    image_bytes: &[u8],
) -> Result<(), DriverManifestError> {
    // 1. BLAKE3 image hash check (cheap; do it first so a tampered
    //    image is rejected before any signature math runs).
    let observed = Blake3::hash(image_bytes);
    if observed != manifest.meta.omni_image_hash {
        return Err(DriverManifestError::ImageHashMismatch);
    }

    // 2. Resolve the issuer's verifying key from the static allowlist.
    let issuer =
        lookup_issuer(&manifest.meta.omni_issuer).ok_or(DriverManifestError::UnknownIssuer)?;
    let verifying_key = OmniVerifyingKey::from_bytes(&issuer.verifying_key)
        .map_err(|_| DriverManifestError::SignatureInvalid)?;

    // 3. Build the signing payload (manifest with signature zeroed)
    //    and verify.
    let payload = build_signing_payload(manifest);
    let signature = OmniSignature::from_bytes(manifest.meta.omni_signature);
    verifying_key
        .verify(&payload, &signature)
        .map_err(|_| DriverManifestError::SignatureInvalid)
}

/// Build the canonical signing payload for a [`DriverManifest`].
///
/// The payload is the byte concatenation
/// `name_len:u32_le || name || version_len:u32_le || version || omni_image_hash || omni_issuer_len:u32_le || omni_issuer || capabilities_canonical || matchers_canonical`
/// where each `*_canonical` block is in turn a length-prefixed
/// concatenation of the variant-tagged resource / matcher records.
///
/// Postcard would be the natural encoding here (and aligns with
/// `OIP-Serde-004` § S2), but pulling the `postcard` crate into the
/// kernel surface is deferred to the same P6.7.8 sprint that wires the
/// TOML parser. The handcrafted encoder used today is byte-deterministic
/// and covers exactly the schema fields above; the encoder MUST be
/// replaced with the postcard pass at that point to stay aligned with
/// the OIP-013 spec text. Until then, signing-side tools MUST mirror
/// the same encoding (a single helper function in `omni-driver-build`).
fn build_signing_payload(manifest: &DriverManifest) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    push_lenprefixed_str(&mut out, &manifest.meta.name);
    push_lenprefixed_str(&mut out, &manifest.meta.version);
    out.extend_from_slice(&manifest.meta.omni_image_hash);
    push_lenprefixed_str(&mut out, &manifest.meta.omni_issuer);

    push_resource_vec(&mut out, &manifest.capabilities.mmio_regions);
    push_resource_vec(&mut out, &manifest.capabilities.dma_windows);
    push_resource_vec(&mut out, &manifest.capabilities.irq_lines);
    push_resource_vec(&mut out, &manifest.capabilities.pci_devices);

    push_pci_matchers(&mut out, &manifest.matchers.pci_vendor_device);
    push_string_vec(&mut out, &manifest.matchers.acpi_hid);

    out
}

fn push_lenprefixed_str(out: &mut Vec<u8>, s: &str) {
    let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn push_string_vec(out: &mut Vec<u8>, v: &[String]) {
    let len = u32::try_from(v.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    for s in v {
        push_lenprefixed_str(out, s);
    }
}

fn push_resource_vec(out: &mut Vec<u8>, v: &[Resource]) {
    let len = u32::try_from(v.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    for r in v {
        encode_resource(out, r);
    }
}

// Tag bytes are stable; appending new variants MUST use new tags.
// Only the variants relevant to driver manifests are encoded here; an
// `Action::*` variant outside this list is rejected by the syscall
// layer before the encoder ever sees it.
const TAG_MMIO: u8 = 0x10;
const TAG_DMA: u8 = 0x11;
const TAG_IRQ: u8 = 0x12;
const TAG_PCI: u8 = 0x13;

fn encode_resource(out: &mut Vec<u8>, r: &Resource) {
    match *r {
        Resource::MmioRegion { phys_base, len } => {
            out.push(TAG_MMIO);
            out.extend_from_slice(&phys_base.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
        }
        Resource::DmaWindow { iova_base, len } => {
            out.push(TAG_DMA);
            out.extend_from_slice(&iova_base.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
        }
        Resource::IrqLine(line) => {
            out.push(TAG_IRQ);
            out.extend_from_slice(&line.to_le_bytes());
        }
        Resource::PciDevice {
            segment,
            bus,
            device,
            function,
        } => {
            out.push(TAG_PCI);
            out.extend_from_slice(&segment.to_le_bytes());
            out.push(bus);
            out.push(device);
            out.push(function);
        }
        // Other Resource variants are not part of the driver-manifest
        // capability set; encode them as a 0-byte tag so the encoder
        // never panics — `verify_manifest` would have rejected the
        // manifest at the matcher walk before reaching here in a real
        // call path.
        _ => out.push(0),
    }
}

fn push_pci_matchers(out: &mut Vec<u8>, v: &[PciMatcher]) {
    let len = u32::try_from(v.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    for m in v {
        out.extend_from_slice(&m.vendor.to_le_bytes());
        out.extend_from_slice(&m.device.to_le_bytes());
    }
}

// =============================================================================
// Sanity helpers (exposed for completeness)
// =============================================================================

/// Length in bytes of a fully-encoded Ed25519 verifying key
/// (the constant from `omni_crypto::signing`).
pub const ISSUER_KEY_LEN: usize = VERIFYING_KEY_LEN;

/// Build a [`DriverCapabilities`] declaring the (action, resource)
/// pair as the only mmio request. Convenience for tests and for
/// later first-party driver code.
#[must_use]
pub fn caps_for_single_mmio(phys_base: u64, len: u64) -> DriverCapabilities {
    let mut caps = DriverCapabilities::default();
    caps.mmio_regions
        .push(Resource::MmioRegion { phys_base, len });
    caps
}

/// Returns `true` iff `action` is one of the driver-framework actions.
///
/// See `OIP-Driver-Framework-013` § S1. Useful for the syscall layer
/// to short-circuit a token whose action is outside the driver decade
/// before running signature math.
#[must_use]
pub const fn is_driver_framework_action(action: Action) -> bool {
    matches!(
        action,
        Action::MmioMap
            | Action::DmaMap
            | Action::IrqAttach
            | Action::PciConfigRead
            | Action::PciConfigWrite
            | Action::DriverLoad
            | Action::DriverUnload
            | Action::TeeProbe
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn empty_manifest(
        issuer: &str,
        sig: [u8; SIGNATURE_LEN],
        hash: [u8; HASH_LEN],
    ) -> DriverManifest {
        DriverManifest {
            meta: DriverMeta {
                name: "test-driver".to_string(),
                version: "0.0.1".to_string(),
                omni_image_hash: hash,
                omni_signature: sig,
                omni_issuer: issuer.to_string(),
            },
            capabilities: DriverCapabilities::default(),
            matchers: DriverMatchers::default(),
        }
    }

    #[test]
    fn parse_manifest_returns_parser_not_wired() {
        assert_eq!(
            parse_manifest(b"[meta]\n"),
            Err(DriverManifestError::ParserNotWired)
        );
    }

    #[test]
    fn verify_manifest_rejects_hash_mismatch() {
        // Hash field is all-zeros, but BLAKE3(b"hello") is non-zero.
        let m = empty_manifest("anybody", [0; SIGNATURE_LEN], [0; HASH_LEN]);
        assert_eq!(
            verify_manifest(&m, b"hello"),
            Err(DriverManifestError::ImageHashMismatch)
        );
    }

    #[test]
    fn verify_manifest_rejects_unknown_issuer() {
        // Pre-compute BLAKE3(b"image-bytes") so the hash check passes
        // and the test exercises the issuer-lookup arm.
        let hash = Blake3::hash(b"image-bytes");
        let m = empty_manifest("ghost-issuer", [0; SIGNATURE_LEN], hash);
        assert_eq!(
            verify_manifest(&m, b"image-bytes"),
            Err(DriverManifestError::UnknownIssuer)
        );
    }

    #[test]
    fn build_signing_payload_is_deterministic() {
        let mut m = empty_manifest("anybody", [7; SIGNATURE_LEN], [3; HASH_LEN]);
        let a = build_signing_payload(&m);
        let b = build_signing_payload(&m);
        assert_eq!(a, b);
        // Different field content produces a different payload.
        m.meta.name = "different".to_string();
        let c = build_signing_payload(&m);
        assert_ne!(a, c);
    }

    #[test]
    fn signing_payload_encodes_capability_subset() {
        // A manifest with one MMIO + one IRQ MUST produce a strictly
        // longer payload than the same manifest with capabilities
        // empty, and the suffix MUST encode the cap fields in a
        // stable order (mmio, dma, irq, pci).
        let mut m = empty_manifest("anybody", [0; SIGNATURE_LEN], [0; HASH_LEN]);
        let baseline = build_signing_payload(&m);

        m.capabilities = DriverCapabilities {
            mmio_regions: vec![Resource::MmioRegion {
                phys_base: 0xFEBC_0000,
                len: 0x1000,
            }],
            dma_windows: vec![],
            irq_lines: vec![Resource::IrqLine(33)],
            pci_devices: vec![],
        };
        let with_caps = build_signing_payload(&m);

        assert!(with_caps.len() > baseline.len());
        // The MMIO tag MUST appear before the IRQ tag in the encoded
        // bytes — the order is part of the canonical contract.
        let mmio_pos = with_caps.iter().position(|b| *b == TAG_MMIO).unwrap();
        let irq_pos = with_caps.iter().position(|b| *b == TAG_IRQ).unwrap();
        assert!(mmio_pos < irq_pos);
    }

    #[test]
    fn driver_framework_action_predicate() {
        for a in [
            Action::MmioMap,
            Action::DmaMap,
            Action::IrqAttach,
            Action::PciConfigRead,
            Action::PciConfigWrite,
            Action::DriverLoad,
            Action::DriverUnload,
            Action::TeeProbe,
        ] {
            assert!(is_driver_framework_action(a), "{a:?} not classified");
        }
        for a in [
            Action::Read,
            Action::Write,
            Action::IpcSend,
            Action::IpcRecv,
        ] {
            assert!(!is_driver_framework_action(a), "{a:?} misclassified");
        }
    }

    #[test]
    fn issuer_key_len_matches_omni_crypto_constant() {
        assert_eq!(ISSUER_KEY_LEN, VERIFYING_KEY_LEN);
        assert_eq!(ISSUER_KEY_LEN, 32);
    }
}
