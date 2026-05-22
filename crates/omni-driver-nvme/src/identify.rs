//! NVMe Identify response parsers.
//!
//! Pure-function decoders for the 4 KiB response payloads the
//! controller writes into the IOVA buffer the driver supplied with
//! `Identify Controller` (NVMe 1.4 § 5.15.2 Figure 247) and
//! `Identify Namespace` (§ 5.15.2 Figure 245). The driver consumes
//! a small subset of the fields for Phase-1 bring-up per
//! OIP-Driver-NVMe-014 § S6 steps 8–10:
//!
//! - `Identify Controller` → `NN` (Number of Namespaces, the count
//!   of namespaces the controller exposes).
//! - `Identify Namespace` → `NSZE` (Namespace Size in 4 KiB
//!   sectors), `NCAP` (Namespace Capacity), `LBADS` (LBA Data
//!   Size — Phase-1 requires `LBADS = 12`, i.e. 4 KiB sectors).
//!
//! All multi-byte fields are little-endian per NVMe 1.4 § 3.0.
//!
//! ## What this module does NOT do
//!
//! - It does NOT validate the response page's checksum (NVMe has
//!   none — integrity is enforced by the IOMMU + completion
//!   status word).
//! - It does NOT pull every field — only the subset Phase-1 needs.
//!   Future fields land as additional pure-function accessors on
//!   the [`IdentifyController`] / [`IdentifyNamespace`] views.

use core::convert::TryInto;

/// Required size of an Identify response page per NVMe 1.4
/// § 5.15.2.
pub const IDENTIFY_RESPONSE_BYTES: usize = 4096;

/// `LBADS` value Phase-1 NVMe accepts (4 KiB sectors per OMNI OS
/// kernel page size). NVMe 1.4 § 5.15.2 encodes LBADS as the
/// `log2(LBA size)` so `LBADS = 12` corresponds to 4096-byte
/// sectors.
pub const PHASE_1_REQUIRED_LBADS: u8 = 12;

/// Errors a parser can surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IdentifyError {
    /// The supplied page is smaller than
    /// [`IDENTIFY_RESPONSE_BYTES`]. Indicates a buffer-shape
    /// regression upstream (the IOMMU buffer is expected to be
    /// exactly 4 KiB).
    PageTooSmall,
    /// `LBADS` field is not equal to [`PHASE_1_REQUIRED_LBADS`].
    /// Per OIP-014 § S6 step 10 the driver rejects any namespace
    /// whose sector size is not 4 KiB.
    UnsupportedLbads {
        /// The actual `LBADS` value the controller reported.
        observed: u8,
    },
}

// =============================================================================
// IdentifyController
// =============================================================================

/// Zero-copy view of the 4 KiB Identify Controller response page.
///
/// The view stores the byte slice borrow; field accessors decode
/// fields on demand so the parser never allocates.
#[derive(Debug, Clone, Copy)]
pub struct IdentifyController<'a> {
    page: &'a [u8],
}

impl<'a> IdentifyController<'a> {
    /// `NN` — Number of Namespaces field offset (Figure 247).
    pub const NN_OFFSET: usize = 516;

    /// Construct the view over a 4 KiB response page.
    ///
    /// # Errors
    ///
    /// - [`IdentifyError::PageTooSmall`] if `page.len() <
    ///   IDENTIFY_RESPONSE_BYTES`.
    pub fn new(page: &'a [u8]) -> Result<Self, IdentifyError> {
        if page.len() < IDENTIFY_RESPONSE_BYTES {
            return Err(IdentifyError::PageTooSmall);
        }
        Ok(Self { page })
    }

    /// Number of Namespaces the controller exposes.
    ///
    /// NVMe 1.4 § 5.15.2 Figure 247 encodes NN as a 32-bit
    /// little-endian value at offset 516. Phase-1 driver uses
    /// `NN` only to log the controller's capacity (the single
    /// supported namespace is identified via
    /// `Identify(ActiveNsList)` at the next bring-up step).
    #[must_use]
    pub fn nn(&self) -> u32 {
        read_le_u32(self.page, Self::NN_OFFSET)
    }
}

// =============================================================================
// IdentifyNamespace
// =============================================================================

/// Zero-copy view of the 4 KiB Identify Namespace response page.
#[derive(Debug, Clone, Copy)]
pub struct IdentifyNamespace<'a> {
    page: &'a [u8],
}

impl<'a> IdentifyNamespace<'a> {
    /// `NSZE` — Namespace Size field offset (Figure 245).
    pub const NSZE_OFFSET: usize = 0;

    /// `NCAP` — Namespace Capacity field offset.
    pub const NCAP_OFFSET: usize = 8;

    /// `NLBAF` — Number of LBA Formats field offset (1-based
    /// count − 1; e.g. `NLBAF = 0` means one format).
    pub const NLBAF_OFFSET: usize = 25;

    /// `FLBAS` — Formatted LBA Size field offset. Bits 3..=0
    /// select which of the up-to-16 `LBAF` entries below
    /// describes the namespace's active format.
    pub const FLBAS_OFFSET: usize = 26;

    /// `LBAF0` — first LBA Format descriptor offset. Each
    /// descriptor is 4 bytes; the LBADS byte we want lives at
    /// offset `LBAF_BASE + 2 + 4 * format_index`.
    pub const LBAF_BASE_OFFSET: usize = 128;

    /// Bytes per `LBAF` descriptor (NVMe 1.4 § 5.15.2 Figure 245).
    pub const LBAF_BYTES: usize = 4;

    /// Construct the view over a 4 KiB response page.
    ///
    /// # Errors
    ///
    /// - [`IdentifyError::PageTooSmall`] if `page.len() <
    ///   IDENTIFY_RESPONSE_BYTES`.
    pub fn new(page: &'a [u8]) -> Result<Self, IdentifyError> {
        if page.len() < IDENTIFY_RESPONSE_BYTES {
            return Err(IdentifyError::PageTooSmall);
        }
        Ok(Self { page })
    }

    /// Namespace Size in 4 KiB sectors per NVMe 1.4 § 5.15.2
    /// Figure 245. The value is the total LBA count of the
    /// namespace; multiplied by the active LBA size it gives the
    /// total byte capacity.
    #[must_use]
    pub fn nsze(&self) -> u64 {
        read_le_u64(self.page, Self::NSZE_OFFSET)
    }

    /// Namespace Capacity in sectors (the largest value
    /// `NSZE` may grow to without re-formatting).
    #[must_use]
    pub fn ncap(&self) -> u64 {
        read_le_u64(self.page, Self::NCAP_OFFSET)
    }

    /// `FLBAS` byte (Formatted LBA Size). Bits 3..=0 select the
    /// active LBA format; bits 4..=6 are extended metadata
    /// flags Phase-1 ignores.
    #[must_use]
    pub fn flbas(&self) -> u8 {
        self.page.get(Self::FLBAS_OFFSET).copied().unwrap_or(0)
    }

    /// Active LBA format index — `FLBAS & 0xF`.
    #[must_use]
    pub fn active_format_index(&self) -> u8 {
        self.flbas() & 0x0F
    }

    /// LBA Data Size of the active LBA format, as the
    /// `log2(sector_size_in_bytes)` per NVMe 1.4 § 5.15.2.
    ///
    /// `LBADS = 12` ↔ 4 KiB sectors (Phase-1 supported value).
    #[must_use]
    pub fn lbads(&self) -> u8 {
        let format_index = self.active_format_index() as usize;
        let lbaf_offset = Self::LBAF_BASE_OFFSET + format_index * Self::LBAF_BYTES;
        // The LBA Data Size byte lives at offset `+2` inside the
        // LBAF descriptor (Figure 245); bits 4..=0 hold the value.
        self.page
            .get(lbaf_offset + 2)
            .copied()
            .map_or(0, |b| b & 0x1F)
    }

    /// Returns `Ok(byte_size)` when the active LBA format matches
    /// the Phase-1 4 KiB requirement, where `byte_size = NSZE *
    /// 4096`. Returns [`IdentifyError::UnsupportedLbads`] when
    /// `LBADS != PHASE_1_REQUIRED_LBADS`.
    ///
    /// # Errors
    ///
    /// - [`IdentifyError::UnsupportedLbads`] per OIP-014 § S6
    ///   step 10.
    pub fn validated_byte_size(&self) -> Result<u64, IdentifyError> {
        let lbads = self.lbads();
        if lbads != PHASE_1_REQUIRED_LBADS {
            return Err(IdentifyError::UnsupportedLbads { observed: lbads });
        }
        // Sector size = 1 << LBADS = 4096; NSZE * 4096 = NSZE << 12.
        Ok(self.nsze().wrapping_shl(u32::from(lbads)))
    }
}

// =============================================================================
// Internal byte helpers
// =============================================================================

fn read_le_u32(page: &[u8], off: usize) -> u32 {
    let slice = page
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .unwrap_or([0u8; 4]);
    u32::from_le_bytes(slice)
}

fn read_le_u64(page: &[u8], off: usize) -> u64 {
    let slice = page
        .get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .unwrap_or([0u8; 8]);
    u64::from_le_bytes(slice)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "host tests pre-allocate a fixed 4 KiB page and write canonical NVMe field offsets in-place; the indexing pattern mirrors NVMe 1.4 Figure 245/247 verbatim"
)]
mod tests {
    use super::*;
    use alloc::vec;

    fn zero_page() -> alloc::vec::Vec<u8> {
        vec![0u8; IDENTIFY_RESPONSE_BYTES]
    }

    // -------------------------------------------------------------------
    // Constants tripwires
    // -------------------------------------------------------------------

    #[test]
    fn identify_response_bytes_matches_spec() {
        assert_eq!(IDENTIFY_RESPONSE_BYTES, 4096);
    }

    #[test]
    fn phase_1_required_lbads_matches_oip_014() {
        // OIP-014 § S6 step 10 — `LBADS = 12` for 4 KiB sectors.
        assert_eq!(PHASE_1_REQUIRED_LBADS, 12);
    }

    // -------------------------------------------------------------------
    // IdentifyController
    // -------------------------------------------------------------------

    #[test]
    fn identify_controller_rejects_undersized_page() {
        let small = vec![0u8; IDENTIFY_RESPONSE_BYTES - 1];
        let res = IdentifyController::new(&small);
        assert!(matches!(res, Err(IdentifyError::PageTooSmall)));
    }

    #[test]
    fn identify_controller_nn_at_offset_516_little_endian() {
        let mut page = zero_page();
        // NN = 0x1234_5678 little-endian.
        page[IdentifyController::NN_OFFSET] = 0x78;
        page[IdentifyController::NN_OFFSET + 1] = 0x56;
        page[IdentifyController::NN_OFFSET + 2] = 0x34;
        page[IdentifyController::NN_OFFSET + 3] = 0x12;
        let view = IdentifyController::new(&page).unwrap();
        assert_eq!(view.nn(), 0x1234_5678);
    }

    #[test]
    fn identify_controller_nn_zero_for_empty_page() {
        let page = zero_page();
        let view = IdentifyController::new(&page).unwrap();
        assert_eq!(view.nn(), 0);
    }

    // -------------------------------------------------------------------
    // IdentifyNamespace
    // -------------------------------------------------------------------

    #[test]
    fn identify_namespace_rejects_undersized_page() {
        let small = vec![0u8; IDENTIFY_RESPONSE_BYTES - 1];
        let res = IdentifyNamespace::new(&small);
        assert!(matches!(res, Err(IdentifyError::PageTooSmall)));
    }

    #[test]
    fn identify_namespace_nsze_at_offset_zero_little_endian() {
        let mut page = zero_page();
        // NSZE = 0x0000_0000_1000_0000 (= 256 Mi sectors at 4 KiB =
        // 1 TiB namespace).
        let val: u64 = 0x0000_0000_1000_0000;
        page[..8].copy_from_slice(&val.to_le_bytes());
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(view.nsze(), val);
    }

    #[test]
    fn identify_namespace_ncap_at_offset_eight() {
        let mut page = zero_page();
        let val: u64 = 0xCAFE_BABE_DEAD_BEEF;
        page[8..16].copy_from_slice(&val.to_le_bytes());
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(view.ncap(), val);
    }

    #[test]
    fn identify_namespace_active_format_index_masks_low_4_bits() {
        let mut page = zero_page();
        // FLBAS = 0b1010_0011 → active format index = 0b0011 = 3.
        page[IdentifyNamespace::FLBAS_OFFSET] = 0b1010_0011;
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(view.flbas(), 0b1010_0011);
        assert_eq!(view.active_format_index(), 3);
    }

    #[test]
    fn identify_namespace_lbads_reads_active_format_descriptor() {
        let mut page = zero_page();
        // Active format = 1 (FLBAS low nibble = 1).
        page[IdentifyNamespace::FLBAS_OFFSET] = 0x01;
        // LBAF1 lives at LBAF_BASE_OFFSET + 1 * 4 = 132. The
        // LBADS byte is at offset +2 inside the descriptor =
        // 132 + 2 = 134. LBADS = 12 (4 KiB sectors).
        let lbaf1_lbads_offset = IdentifyNamespace::LBAF_BASE_OFFSET + 4 + 2;
        page[lbaf1_lbads_offset] = 12;
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(view.lbads(), 12);
    }

    #[test]
    fn identify_namespace_lbads_masks_high_3_bits() {
        let mut page = zero_page();
        page[IdentifyNamespace::FLBAS_OFFSET] = 0x00; // active = 0
        // LBAF0 LBADS byte at LBAF_BASE_OFFSET + 0 * 4 + 2 = 130.
        // Set high bits + LBADS = 12.
        page[130] = 0b1110_1100; // 0xEC = high bits 111 | LBADS 01100 = 12
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(view.lbads(), 12, "high bits must be masked away");
    }

    #[test]
    fn identify_namespace_validated_byte_size_returns_size_when_lbads_12() {
        let mut page = zero_page();
        // NSZE = 1024 sectors.
        let nsze: u64 = 1024;
        page[..8].copy_from_slice(&nsze.to_le_bytes());
        // FLBAS = 0 → active format = 0.
        page[IdentifyNamespace::FLBAS_OFFSET] = 0;
        // LBAF0 LBADS = 12.
        page[IdentifyNamespace::LBAF_BASE_OFFSET + 2] = 12;
        let view = IdentifyNamespace::new(&page).unwrap();
        let size = view.validated_byte_size().expect("LBADS=12 accepted");
        // Expected: 1024 sectors * 4096 bytes/sector = 4 MiB =
        // 0x40_0000.
        assert_eq!(size, 1024 * 4096);
    }

    #[test]
    fn identify_namespace_validated_byte_size_rejects_lbads_9() {
        let mut page = zero_page();
        page[..8].copy_from_slice(&1024_u64.to_le_bytes());
        page[IdentifyNamespace::FLBAS_OFFSET] = 0;
        // LBAF0 LBADS = 9 (= 512-byte sectors, the legacy default).
        page[IdentifyNamespace::LBAF_BASE_OFFSET + 2] = 9;
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(
            view.validated_byte_size(),
            Err(IdentifyError::UnsupportedLbads { observed: 9 })
        );
    }

    #[test]
    fn identify_namespace_validated_byte_size_rejects_lbads_zero_default_page() {
        // Empty page → LBADS = 0 → rejected.
        let page = zero_page();
        let view = IdentifyNamespace::new(&page).unwrap();
        assert_eq!(
            view.validated_byte_size(),
            Err(IdentifyError::UnsupportedLbads { observed: 0 })
        );
    }

    // -------------------------------------------------------------------
    // Error taxonomy
    // -------------------------------------------------------------------

    #[test]
    fn identify_error_variants_are_distinguishable() {
        let page_too_small = IdentifyError::PageTooSmall;
        let unsupported_9 = IdentifyError::UnsupportedLbads { observed: 9 };
        let unsupported_10 = IdentifyError::UnsupportedLbads { observed: 10 };
        assert_ne!(page_too_small, unsupported_9);
        assert_ne!(unsupported_9, unsupported_10);
        assert_eq!(unsupported_9, IdentifyError::UnsupportedLbads { observed: 9 });
    }
}
