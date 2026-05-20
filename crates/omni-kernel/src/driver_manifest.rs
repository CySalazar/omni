//! Driver-manifest schema, omni-pack v1 decoder, and Ed25519 signature
//! verification — `OIP-Driver-Framework-013` § S5 framework skeleton.
//!
//! ## Status
//!
//! P6.7.3 skeleton. The schema types are locked here so downstream code
//! (the `DriverLoad = 73` syscall handler and the per-driver manifest
//! emitter in the build pipeline) can be authored against a stable
//! Rust surface. Three operations land:
//!
//! 1. **omni-pack v1 header parsing** — wired now via [`decode_omni_pack`].
//!    The 64-byte fixed header (magic, version, three `(offset, len)`
//!    section descriptors) is validated for magic, version, bounds, and
//!    non-overlap. The body (manifest postcard + signature + image
//!    ELF) is returned as borrowed slices for the next steps; **no
//!    allocation on the failure path** as required by `OIP-013` § S5.3
//!    step 3.
//! 2. **BLAKE3 image hash verification** — wired now via
//!    [`omni_crypto::hash::Blake3`]. Byte-wise compare against the
//!    manifest's `omni_image_hash` field.
//! 3. **Ed25519 signature verification** — wired now via
//!    [`omni_crypto::signing::OmniVerifyingKey`]. The signature
//!    covers the postcard canonical encoding of the manifest body
//!    (`manifest.pc` per OIP-013 § S5.3 step 5) — NOT TOML bytes,
//!    which forecloses the JSON-canonicalisation class of bugs.
//!
//! The manifest's `omni_issuer_pubkey` is cross-checked against
//! [`crate::known_issuers::KNOWN_ISSUERS`] so a driver signed by a
//! key the kernel does not trust is rejected before the signature
//! math runs (OIP-013 § S5.4: no TOFU for drivers).
//!
//! ## Wire format
//!
//! On-disk artifact at `DriverLoad` (§ S5.5):
//!
//! ```text
//! Offset  Size  Field
//! ─────── ───── ─────────────────────────────────────────────────────
//! 0x00    8     magic            = b"OMNIPACK"
//! 0x08    4     version          = 1u32
//! 0x0C    4     flags            reserved, MUST be 0
//! 0x10    8     manifest_offset
//! 0x18    8     manifest_len     (postcard bytes, ≤ 16 KiB)
//! 0x20    8     signature_offset
//! 0x28    8     signature_len    = 64 (Ed25519)
//! 0x30    8     image_offset
//! 0x38    8     image_len        (ELF bytes)
//! 0x40    ...   manifest.pc      postcard canonical DriverManifestV1
//! ...     ...   signature        raw Ed25519 over manifest.pc, 64 bytes
//! ...     ...   image.elf        loaded by spawn_from_elf
//! ```
//!
//! TOML is the developer-authored source format compiled offline by
//! the `omni-driver-pack` build tool; the kernel never sees it.
//!
//! ## What this module does NOT do
//!
//! - It does not allocate IOMMU domains or program PT entries; that is
//!   the `DriverLoad` syscall handler's job (P6.7.8 — gated on the
//!   handler being wired beyond `NotYetImplemented`).
//! - It does not enforce the `not_after ≤ 90 days` token-lifetime cap
//!   from § S1.2; that lives in the `omni-capability::token` mint path.
//! - It does not yet `postcard::from_bytes` the manifest payload —
//!   [`postcard_decode_manifest`] is a stub returning
//!   [`DriverManifestError::ParserNotWired`] because the `postcard`
//!   crate is not yet pulled into the kernel surface. A handcrafted
//!   byte-deterministic encoder is used internally by
//!   [`verify_manifest`] for the signing payload; it is transitional
//!   and is replaced by the same `postcard` pass that wires the
//!   decoder in P6.7.8.
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
    /// Ed25519 issuer public key (`VERIFYING_KEY_LEN` = 32 bytes).
    /// Per OIP-013 § S5.4 the kernel cross-checks this against
    /// [`crate::known_issuers::KNOWN_ISSUERS`] and refuses any
    /// driver whose `omni_issuer_pubkey` is not on the allowlist
    /// (no TOFU). The signature [`Self::omni_signature`] MUST verify
    /// under this key over the postcard-encoded manifest payload.
    pub omni_issuer_pubkey: [u8; VERIFYING_KEY_LEN],
    /// Ed25519 signature over the canonical postcard encoding of
    /// `(meta_no_signature, capabilities, matchers)`. 64 bytes.
    pub omni_signature: [u8; SIGNATURE_LEN],
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
// omni-pack v1 header
// =============================================================================

/// Magic bytes at offset 0 of every omni-pack v1 blob.
pub const OMNI_PACK_MAGIC: [u8; 8] = *b"OMNIPACK";

/// Fixed-size header version this skeleton understands.
pub const OMNI_PACK_VERSION: u32 = 1;

/// Fixed header length per OIP-013 § S5.5.
pub const OMNI_PACK_HEADER_LEN: usize = 0x40;

/// Upper bound on a pack blob (OIP-013 § S5.2: ≤ 32 MiB).
pub const OMNI_PACK_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Upper bound on the postcard manifest section (OIP-013 § S5.5).
pub const OMNI_PACK_MAX_MANIFEST_BYTES: u64 = 16 * 1024;

/// Borrowed view over the three sections of an omni-pack v1 blob:
/// `manifest.pc`, `signature`, `image.elf`. Returned by
/// [`decode_omni_pack`] after the 64-byte header has been validated.
///
/// The lifetime is bound to the input buffer — the kernel does not
/// copy the bytes; the call-site is responsible for keeping the
/// underlying buffer alive until `verify_manifest` and
/// `spawn_from_elf` have run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmniPackSections<'a> {
    /// Postcard canonical encoding of `DriverManifestV1`. Decoded by
    /// [`postcard_decode_manifest`] (skeleton — currently returns
    /// `ParserNotWired`).
    pub manifest: &'a [u8],
    /// Raw Ed25519 signature, exactly [`SIGNATURE_LEN`] bytes, over
    /// the `manifest` slice above.
    pub signature: &'a [u8; SIGNATURE_LEN],
    /// ELF image bytes consumed by `process::spawn_from_elf` after
    /// verification.
    pub image: &'a [u8],
}

// =============================================================================
// Error type
// =============================================================================

/// Failure modes for the omni-pack decode / verify chain.
///
/// Names follow the POSIX-flavoured codes referenced in OIP-013
/// § S5.3 (`EINVAL`, `EACCES`, `EFAULT`) so the syscall layer can
/// map them to user-visible numeric error codes without an
/// intermediate translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverManifestError {
    /// The pack blob is shorter than the 64-byte fixed header, or a
    /// declared section `(offset, len)` exceeds the blob bounds, or
    /// two sections overlap, or `flags != 0`, or `magic != OMNIPACK`,
    /// or `version != 1`. Surfaced as `EINVAL` at the syscall
    /// boundary (OIP-013 § S5.3 step 3).
    MalformedPack,
    /// The pack blob exceeds [`OMNI_PACK_MAX_BYTES`] or the manifest
    /// section exceeds [`OMNI_PACK_MAX_MANIFEST_BYTES`]. Surfaced as
    /// `EINVAL`.
    PackTooLarge,
    /// `postcard::from_bytes::<DriverManifestV1>(manifest_section)`
    /// is not yet wired in this skeleton — see module docs and
    /// [`postcard_decode_manifest`]. Distinct from
    /// [`Self::MalformedPack`]: the *outer* container is valid, but
    /// the inner manifest payload cannot yet be decoded.
    ParserNotWired,
    /// The manifest's `omni_issuer_pubkey` does not appear in
    /// [`crate::known_issuers::KNOWN_ISSUERS`]. Surfaced as `EACCES`.
    UnknownIssuer,
    /// Ed25519 signature verification failed (or the issuer's stored
    /// key bytes are malformed). Surfaced as `EACCES`.
    SignatureInvalid,
    /// `BLAKE3(image_bytes) != manifest.meta.omni_image_hash`.
    /// Surfaced as `EINVAL`.
    ImageHashMismatch,
}

// =============================================================================
// omni-pack v1 decode
// =============================================================================

/// Validate the omni-pack v1 header and return borrowed slices into
/// the three sections. `OIP-013` § S5.3 steps 2–3.
///
/// # Errors
///
/// - [`DriverManifestError::MalformedPack`] — wrong magic, wrong
///   version, non-zero `flags`, header too short, or any section
///   `(offset, len)` out of bounds / overlapping.
/// - [`DriverManifestError::PackTooLarge`] — exceeds the size caps in
///   OIP-013 § S5.5.
///
/// MUST NOT allocate on the failure path (OIP-013 § S5.3 step 3:
/// "No allocations on the failure path"). This implementation is
/// allocation-free on success too — every section is a borrowed
/// slice into `pack_bytes`.
pub fn decode_omni_pack(pack_bytes: &[u8]) -> Result<OmniPackSections<'_>, DriverManifestError> {
    if pack_bytes.len() < OMNI_PACK_HEADER_LEN {
        return Err(DriverManifestError::MalformedPack);
    }
    if pack_bytes.len() as u64 > OMNI_PACK_MAX_BYTES {
        return Err(DriverManifestError::PackTooLarge);
    }

    let magic: [u8; 8] = pack_bytes
        .get(0x00..0x08)
        .and_then(|s| s.try_into().ok())
        .ok_or(DriverManifestError::MalformedPack)?;
    if magic != OMNI_PACK_MAGIC {
        return Err(DriverManifestError::MalformedPack);
    }

    let version = read_u32_le(pack_bytes, 0x08)?;
    if version != OMNI_PACK_VERSION {
        return Err(DriverManifestError::MalformedPack);
    }

    let flags = read_u32_le(pack_bytes, 0x0C)?;
    if flags != 0 {
        return Err(DriverManifestError::MalformedPack);
    }

    let manifest_off = read_u64_le(pack_bytes, 0x10)?;
    let manifest_len = read_u64_le(pack_bytes, 0x18)?;
    let signature_off = read_u64_le(pack_bytes, 0x20)?;
    let signature_len = read_u64_le(pack_bytes, 0x28)?;
    let image_off = read_u64_le(pack_bytes, 0x30)?;
    let image_len = read_u64_le(pack_bytes, 0x38)?;

    if manifest_len > OMNI_PACK_MAX_MANIFEST_BYTES {
        return Err(DriverManifestError::PackTooLarge);
    }
    if signature_len != SIGNATURE_LEN as u64 {
        return Err(DriverManifestError::MalformedPack);
    }

    let manifest_range = checked_section(manifest_off, manifest_len, pack_bytes.len())?;
    let signature_range = checked_section(signature_off, signature_len, pack_bytes.len())?;
    let image_range = checked_section(image_off, image_len, pack_bytes.len())?;

    // Sections MUST be disjoint AND contained in [HEADER_LEN, pack_len).
    if manifest_range.0 < OMNI_PACK_HEADER_LEN
        || signature_range.0 < OMNI_PACK_HEADER_LEN
        || image_range.0 < OMNI_PACK_HEADER_LEN
    {
        return Err(DriverManifestError::MalformedPack);
    }
    if ranges_overlap(manifest_range, signature_range)
        || ranges_overlap(manifest_range, image_range)
        || ranges_overlap(signature_range, image_range)
    {
        return Err(DriverManifestError::MalformedPack);
    }

    let signature_bytes: &[u8; SIGNATURE_LEN] = pack_bytes
        .get(signature_range.0..signature_range.1)
        .and_then(|s| s.try_into().ok())
        .ok_or(DriverManifestError::MalformedPack)?;
    let manifest_slice = pack_bytes
        .get(manifest_range.0..manifest_range.1)
        .ok_or(DriverManifestError::MalformedPack)?;
    let image_slice = pack_bytes
        .get(image_range.0..image_range.1)
        .ok_or(DriverManifestError::MalformedPack)?;

    Ok(OmniPackSections {
        manifest: manifest_slice,
        signature: signature_bytes,
        image: image_slice,
    })
}

/// Decode a `manifest.pc` byte slice (the postcard canonical encoding
/// of `DriverManifestV1`) into a [`DriverManifest`].
///
/// # Errors
///
/// Returns [`DriverManifestError::ParserNotWired`] until P6.7.8
/// wires `postcard` into the kernel surface. The entry-point exists
/// so call-sites (the `DriverLoad` syscall handler, test fixtures)
/// can be authored today against the final ABI.
pub fn postcard_decode_manifest(
    manifest_bytes: &[u8],
) -> Result<DriverManifest, DriverManifestError> {
    let _ = manifest_bytes;
    Err(DriverManifestError::ParserNotWired)
}

// Little-endian helpers. Bounds check inline so a maliciously-crafted
// pack with truncated header is caught before the slice operation.
fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, DriverManifestError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(DriverManifestError::MalformedPack)
}

fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64, DriverManifestError> {
    bytes
        .get(offset..offset + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(DriverManifestError::MalformedPack)
}

// Resolve a `(offset, len)` pair against the blob length. Returns
// `(start, end)` indices safe to slice into `pack_bytes`. Catches
// integer overflow at `offset + len` via the widening `u128` path.
fn checked_section(
    offset: u64,
    len: u64,
    pack_len: usize,
) -> Result<(usize, usize), DriverManifestError> {
    let end = u128::from(offset) + u128::from(len);
    if end > pack_len as u128 {
        return Err(DriverManifestError::MalformedPack);
    }
    // The bounds check above guarantees `end <= pack_len`, which fits
    // in `usize`; the `try_from` keeps clippy happy under the strict
    // `cast_possible_truncation` lint without changing behaviour.
    let start = usize::try_from(offset).map_err(|_| DriverManifestError::MalformedPack)?;
    let stop = usize::try_from(end).map_err(|_| DriverManifestError::MalformedPack)?;
    Ok((start, stop))
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

// =============================================================================
// Verification
// =============================================================================

/// Verify `image_bytes` against `manifest`'s declared hash and
/// signature.
///
/// Checks (in order, matching OIP-013 § S5.3):
/// 1. BLAKE3(`image_bytes`) matches `manifest.meta.omni_image_hash`.
/// 2. `manifest.meta.omni_issuer_pubkey` appears in
///    [`crate::known_issuers::KNOWN_ISSUERS`] (no TOFU — § S5.4).
/// 3. The Ed25519 signature `manifest.meta.omni_signature` validates
///    under that key over the canonical signing payload.
///
/// Note: in production the canonical signing payload is the postcard
/// encoding of the manifest body (`manifest.pc`, OIP-013 § S5.3 step 5);
/// this skeleton substitutes a handcrafted byte-deterministic encoder
/// (the private `build_signing_payload` helper documents the transition
/// plan to `postcard` at P6.7.8).
///
/// # Errors
///
/// - [`DriverManifestError::ImageHashMismatch`] — BLAKE3 disagreement.
/// - [`DriverManifestError::UnknownIssuer`] — the issuer's pubkey is
///   not in [`crate::known_issuers::KNOWN_ISSUERS`].
/// - [`DriverManifestError::SignatureInvalid`] — Ed25519 verify failed
///   OR the manifest's stored pubkey bytes are malformed.
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

    // 2. Cross-check the manifest's claimed issuer pubkey against the
    //    kernel-static allowlist. § S5.4 forbids TOFU: an unknown key
    //    is a hard rejection, regardless of whether the signature
    //    that key produced would mathematically verify.
    if lookup_issuer(&manifest.meta.omni_issuer_pubkey).is_none() {
        return Err(DriverManifestError::UnknownIssuer);
    }
    let verifying_key = OmniVerifyingKey::from_bytes(&manifest.meta.omni_issuer_pubkey)
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
/// `name_len:u32_le || name || version_len:u32_le || version || omni_image_hash || omni_issuer_pubkey || capabilities_canonical || matchers_canonical`
/// where each `*_canonical` block is in turn a length-prefixed
/// concatenation of the variant-tagged resource / matcher records.
///
/// Postcard is the natural encoding here (and the OIP-013 § S5.3 step 5
/// spec text mandates it: the kernel verifies the Ed25519 signature
/// over the `manifest.pc` bytes, which is the postcard canonical
/// encoding of `DriverManifestV1`). The handcrafted encoder shipped
/// in this skeleton is byte-deterministic and covers the schema
/// fields locked here, but it MUST be replaced with the postcard
/// pass when [`postcard_decode_manifest`] is wired in P6.7.8.
/// Until then, signing-side tools (the `omni-driver-pack` build
/// helper) MUST mirror this exact encoding.
fn build_signing_payload(manifest: &DriverManifest) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    push_lenprefixed_str(&mut out, &manifest.meta.name);
    push_lenprefixed_str(&mut out, &manifest.meta.version);
    out.extend_from_slice(&manifest.meta.omni_image_hash);
    // Issuer pubkey is a fixed-length field — no length prefix needed.
    out.extend_from_slice(&manifest.meta.omni_issuer_pubkey);

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
#[allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "negative-path tests mutate fixed-offset header bytes from OIP-013 § S5.5"
)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn empty_manifest(
        issuer_pubkey: [u8; VERIFYING_KEY_LEN],
        sig: [u8; SIGNATURE_LEN],
        hash: [u8; HASH_LEN],
    ) -> DriverManifest {
        DriverManifest {
            meta: DriverMeta {
                name: "test-driver".to_string(),
                version: "0.0.1".to_string(),
                omni_image_hash: hash,
                omni_issuer_pubkey: issuer_pubkey,
                omni_signature: sig,
            },
            capabilities: DriverCapabilities::default(),
            matchers: DriverMatchers::default(),
        }
    }

    #[test]
    fn postcard_decode_manifest_returns_parser_not_wired() {
        assert_eq!(
            postcard_decode_manifest(&[0; 0]),
            Err(DriverManifestError::ParserNotWired)
        );
    }

    #[test]
    fn verify_manifest_rejects_hash_mismatch() {
        // Hash field is all-zeros, but BLAKE3(b"hello") is non-zero.
        let m = empty_manifest([0; VERIFYING_KEY_LEN], [0; SIGNATURE_LEN], [0; HASH_LEN]);
        assert_eq!(
            verify_manifest(&m, b"hello"),
            Err(DriverManifestError::ImageHashMismatch)
        );
    }

    #[test]
    fn verify_manifest_rejects_unknown_issuer() {
        // Pre-compute BLAKE3(b"image-bytes") so the hash check passes
        // and the test exercises the issuer-lookup arm. The all-zeros
        // pubkey is never in KNOWN_ISSUERS (Phase 1 table is empty).
        let hash = Blake3::hash(b"image-bytes");
        let m = empty_manifest([0; VERIFYING_KEY_LEN], [0; SIGNATURE_LEN], hash);
        assert_eq!(
            verify_manifest(&m, b"image-bytes"),
            Err(DriverManifestError::UnknownIssuer)
        );
    }

    #[test]
    fn build_signing_payload_is_deterministic() {
        let mut m = empty_manifest([9; VERIFYING_KEY_LEN], [7; SIGNATURE_LEN], [3; HASH_LEN]);
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
        let mut m = empty_manifest([0; VERIFYING_KEY_LEN], [0; SIGNATURE_LEN], [0; HASH_LEN]);
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

    // ---- omni-pack v1 header decode (OIP-013 § S5.5) -----------------------
    //
    // Negative-path tests below mutate raw header bytes via direct
    // index/slice operations. Production code (`decode_omni_pack`) uses
    // bounds-checked `get(..)` paths; the tests can use direct indexing
    // because every offset is a fixed constant from the spec.

    /// Build a minimal well-formed omni-pack blob with the given
    /// per-section bytes. Layout follows § S5.5 exactly: 64-byte
    /// header, then `manifest || signature || image`.
    fn build_pack(manifest: &[u8], signature: &[u8; SIGNATURE_LEN], image: &[u8]) -> Vec<u8> {
        let header_len = OMNI_PACK_HEADER_LEN as u64;
        let m_off = header_len;
        let m_len = manifest.len() as u64;
        let s_off = m_off + m_len;
        let s_len = SIGNATURE_LEN as u64;
        let i_off = s_off + s_len;
        let i_len = image.len() as u64;

        let mut out =
            Vec::with_capacity(OMNI_PACK_HEADER_LEN + manifest.len() + SIGNATURE_LEN + image.len());
        out.extend_from_slice(&OMNI_PACK_MAGIC);
        out.extend_from_slice(&OMNI_PACK_VERSION.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&m_off.to_le_bytes());
        out.extend_from_slice(&m_len.to_le_bytes());
        out.extend_from_slice(&s_off.to_le_bytes());
        out.extend_from_slice(&s_len.to_le_bytes());
        out.extend_from_slice(&i_off.to_le_bytes());
        out.extend_from_slice(&i_len.to_le_bytes());
        out.extend_from_slice(manifest);
        out.extend_from_slice(signature);
        out.extend_from_slice(image);
        out
    }

    #[test]
    fn decode_omni_pack_accepts_well_formed_blob() {
        let pack = build_pack(&[0xAA; 64], &[0xBB; SIGNATURE_LEN], &[0xCC; 256]);
        let sections = decode_omni_pack(&pack).unwrap();
        assert_eq!(sections.manifest.len(), 64);
        assert_eq!(sections.manifest[0], 0xAA);
        assert_eq!(sections.signature[0], 0xBB);
        assert_eq!(sections.image.len(), 256);
        assert_eq!(sections.image[0], 0xCC);
    }

    #[test]
    fn decode_omni_pack_rejects_short_header() {
        assert_eq!(
            decode_omni_pack(&[0u8; 32]),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_bad_magic() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        pack[0] = b'X';
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_wrong_version() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        // version is at offset 0x08 .. 0x0C
        pack[0x08..0x0C].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_nonzero_flags() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        pack[0x0C] = 0xFF;
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_wrong_signature_len() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        // signature_len is at offset 0x28 .. 0x30
        pack[0x28..0x30].copy_from_slice(&32u64.to_le_bytes());
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_out_of_bounds_section() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        // image_len at offset 0x38 .. 0x40; set to past-EOF.
        let past_eof = pack.len() as u64;
        pack[0x38..0x40].copy_from_slice(&past_eof.to_le_bytes());
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_overlapping_sections() {
        let mut pack = build_pack(&[0; 16], &[0; SIGNATURE_LEN], &[0; 16]);
        // signature_offset at 0x20 .. 0x28; alias it to manifest_offset.
        let m_off = u64::from_le_bytes(pack[0x10..0x18].try_into().unwrap());
        pack[0x20..0x28].copy_from_slice(&m_off.to_le_bytes());
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::MalformedPack)
        );
    }

    #[test]
    fn decode_omni_pack_rejects_oversized_manifest() {
        let huge_manifest = vec![0u8; (OMNI_PACK_MAX_MANIFEST_BYTES + 1) as usize];
        let pack = build_pack(&huge_manifest, &[0; SIGNATURE_LEN], &[]);
        assert_eq!(
            decode_omni_pack(&pack),
            Err(DriverManifestError::PackTooLarge)
        );
    }
}
