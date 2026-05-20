//! Data-transfer model — PRP (Physical Region Page lists) only.
//!
//! [`OIP-Driver-NVMe-014`] § M4 + § R1 lock the v0.3 deliverable to PRP
//! lists. SGL is explicitly out of scope (the manifest schema accepts
//! only `transfer_model = "prp"`; any other value MUST be rejected with
//! `EINVAL` per OIP-014 § S1.1).
//!
//! ## Why a const-fn module
//!
//! The bring-up FSM and the eventual command-issuing path both need
//! cheap, host-testable predicates for the two PRP invariants:
//!
//! 1. **Alignment.** Every buffer pointer (PRP1 and every entry of the
//!    PRP list) MUST be aligned to the controller's memory page size
//!    (MPS). For v0.3 the kernel page size is 4 KiB
//!    (`CC.MPS = 0`), so PRP entries are 4 KiB-aligned.
//! 2. **Span coverage.** PRP1 covers the first page; if the transfer
//!    spans more than one page, PRP2 either points at a second page
//!    (two-page transfer) or at a flat array of additional 4 KiB
//!    pointers (`> 2` pages). The driver picks the layout based on the
//!    transfer's byte length.
//!
//! Both predicates are pure functions so they can be unit-tested
//! host-side without standing up a controller.
//!
//! [`OIP-Driver-NVMe-014`]: ../../../oips/oip-driver-nvme-014.md

/// Page size used by the PRP transfer model in v0.3.
///
/// Anchored to the kernel's 4 KiB page size (`CC.MPS = 0` per
/// [`crate::controller_regs::CC_MPS_SHIFT`]). PRP entries MUST be
/// aligned to this boundary; the driver rejects misaligned buffers
/// with `BlkResponse::InvalidArgument` per OIP-014 § TC5.
pub const PRP_PAGE_SIZE: usize = 4096;

/// Maximum block count per PRP-list transfer for the v0.3 deliverable.
///
/// OIP-014 § S2 caps `block_count` at 2048 4 KiB blocks (= 8 MiB),
/// which fits comfortably within a one-page PRP list (the list itself
/// is 4 KiB / 8 bytes = 512 entries; each entry covers one 4 KiB page,
/// so the list addresses up to 2 MiB by itself — combined with PRP1's
/// first page we get up to 513 × 4 KiB ≈ 2 MiB. For larger transfers
/// the driver chains, but v0.3 caps below the chain threshold).
pub const MAX_BLOCK_COUNT_PER_COMMAND: u32 = 2048;

/// PRP list entry size in bytes (NVMe 1.4 § 4.1.4).
///
/// Each PRP entry is a 64-bit physical (IOVA after `DmaMap`) pointer.
pub const PRP_ENTRY_BYTES: usize = 8;

/// Number of PRP entries that fit in one 4 KiB list page.
///
/// `4096 / 8 = 512`. The driver allocates one PRP-list page per
/// transfer that spans more than two NVMe pages; that page covers up
/// to `512 × 4 KiB = 2 MiB` of additional payload (on top of PRP1's
/// first page).
#[allow(
    clippy::integer_division,
    reason = "compile-time division of two `const usize` powers-of-two — \
              `4096 / 8 = 512` is exact and has no runtime cost"
)]
pub const PRP_ENTRIES_PER_LIST_PAGE: usize = PRP_PAGE_SIZE / PRP_ENTRY_BYTES;

/// The data-transfer model accepted by the v0.3 driver.
///
/// `#[non_exhaustive]` so a future OIP (`OIP-Driver-NVMe-SGL-XXX`) can
/// add the SGL variant without breaking the wire-format contract. Until
/// then, the only valid value is [`TransferModel::Prp`]; any other
/// manifest value is rejected with `EINVAL`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferModel {
    /// Physical Region Page lists — the only model accepted in v0.3
    /// (OIP-014 § M4 / § R1).
    Prp,
}

impl TransferModel {
    /// Parse a manifest `transfer_model = "..."` string.
    ///
    /// Returns `None` for any unrecognized value; the caller maps that
    /// to `EINVAL` per OIP-014 § S1.1.
    #[must_use]
    pub fn from_manifest_str(s: &str) -> Option<Self> {
        match s {
            "prp" => Some(Self::Prp),
            _ => None,
        }
    }

    /// The canonical manifest string for this transfer model.
    #[must_use]
    pub const fn as_manifest_str(self) -> &'static str {
        match self {
            Self::Prp => "prp",
        }
    }
}

/// Returns `true` if `iova` satisfies the PRP alignment invariant
/// ([`PRP_PAGE_SIZE`]-aligned). The driver rejects any buffer that
/// fails this check with `BlkResponse::InvalidArgument` per OIP-014
/// § TC5.
#[must_use]
pub const fn is_prp_aligned(iova: u64) -> bool {
    // PRP_PAGE_SIZE is a power of 2; the `iova & (size - 1)` trick is
    // exact and overflow-free even at `u64::MAX`.
    (iova & (PRP_PAGE_SIZE as u64 - 1)) == 0
}

/// Returns the byte length implied by `block_count` 4 KiB blocks.
///
/// Returns `None` on overflow (cannot happen at
/// `block_count ≤ MAX_BLOCK_COUNT_PER_COMMAND`, but kept as
/// defence-in-depth for the eventual command-validation path).
#[must_use]
pub const fn block_payload_bytes(block_count: u32) -> Option<usize> {
    (block_count as usize).checked_mul(PRP_PAGE_SIZE)
}

/// Layout of the PRP fields for a transfer of the given byte length.
///
/// - `len ≤ PRP_PAGE_SIZE`: one page, PRP2 = 0.
/// - `len ≤ 2 × PRP_PAGE_SIZE`: two pages, PRP2 = pointer to the
///   second page directly.
/// - `len  > 2 × PRP_PAGE_SIZE`: PRP2 = pointer to a flat array of
///   4 KiB-page pointers (one PRP-list page).
///
/// NVMe 1.4 § 4.1.4 defines the chained case (`PRP2 = pointer to next
/// PRP list page`), but v0.3 caps transfer size below the chain
/// threshold per [`MAX_BLOCK_COUNT_PER_COMMAND`], so the driver never
/// emits a chained PRP list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrpLayout {
    /// Single-page transfer; PRP1 covers everything, PRP2 unused
    /// (the driver writes 0).
    SinglePage,
    /// Two-page transfer; PRP1 covers the first page, PRP2 covers the
    /// second.
    TwoPages,
    /// Multi-page transfer; PRP1 covers the first page, PRP2 points at
    /// a PRP-list page containing `n_entries` 4 KiB pointers.
    PrpList {
        /// Number of 4 KiB pointers in the PRP list (excluding PRP1).
        n_entries: usize,
    },
}

/// Pick the PRP layout for a transfer of `len` bytes. `len = 0`
/// degenerates to `SinglePage` (a flush-style command); the caller is
/// responsible for rejecting actual zero-length data transfers earlier.
#[must_use]
pub const fn prp_layout(len: usize) -> PrpLayout {
    if len <= PRP_PAGE_SIZE {
        PrpLayout::SinglePage
    } else if len <= 2 * PRP_PAGE_SIZE {
        PrpLayout::TwoPages
    } else {
        // Number of additional pages beyond PRP1: ceil((len - 4096) /
        // 4096). All values fit in `usize` because `len < 2 MiB` for
        // v0.3 — but we use `checked_*` arithmetic anyway to keep the
        // function defence-in-depth against future cap relaxations.
        let extra_bytes = len - PRP_PAGE_SIZE;
        let n = extra_bytes.div_ceil(PRP_PAGE_SIZE);
        PrpLayout::PrpList { n_entries: n }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_matches_kernel_invariant() {
        // The kernel's page size is 4 KiB everywhere (boot, MMU,
        // virtio, NVMe). Drift here would silently break PRP layout.
        assert_eq!(PRP_PAGE_SIZE, 4096);
    }

    #[test]
    fn entries_per_list_page_matches_spec() {
        // 4096 / 8 = 512 — the driver allocates this exact number of
        // slots when it picks `PrpList`.
        assert_eq!(PRP_ENTRIES_PER_LIST_PAGE, 512);
    }

    #[test]
    fn prp_only_manifest_value() {
        // OIP-014 § S1.1: only "prp" is accepted; anything else is
        // EINVAL.
        assert_eq!(
            TransferModel::from_manifest_str("prp"),
            Some(TransferModel::Prp)
        );
        assert_eq!(TransferModel::from_manifest_str("sgl"), None);
        assert_eq!(TransferModel::from_manifest_str("PRP"), None);
        assert_eq!(TransferModel::from_manifest_str(""), None);
    }

    #[test]
    fn manifest_str_round_trips() {
        assert_eq!(TransferModel::Prp.as_manifest_str(), "prp");
        assert_eq!(
            TransferModel::from_manifest_str(TransferModel::Prp.as_manifest_str()),
            Some(TransferModel::Prp)
        );
    }

    #[test]
    fn alignment_check_accepts_page_boundaries() {
        assert!(is_prp_aligned(0));
        assert!(is_prp_aligned(0x1000));
        assert!(is_prp_aligned(0x10_0000));
        assert!(is_prp_aligned(0xDEAD_0000));
    }

    #[test]
    fn alignment_check_rejects_misaligned_addresses() {
        // OIP-014 § TC5: misaligned buffer -> BlkResponse::InvalidArgument.
        assert!(!is_prp_aligned(0x1));
        assert!(!is_prp_aligned(0xFFF));
        assert!(!is_prp_aligned(0x1001));
        assert!(!is_prp_aligned(0x1234));
    }

    #[test]
    fn single_page_layout_for_one_block() {
        let len = block_payload_bytes(1).unwrap();
        assert_eq!(len, PRP_PAGE_SIZE);
        assert_eq!(prp_layout(len), PrpLayout::SinglePage);
    }

    #[test]
    fn two_page_layout_at_eight_kib() {
        let len = block_payload_bytes(2).unwrap();
        assert_eq!(len, 2 * PRP_PAGE_SIZE);
        assert_eq!(prp_layout(len), PrpLayout::TwoPages);
    }

    #[test]
    fn prp_list_layout_at_three_pages() {
        // 3 × 4 KiB > 2 × 4 KiB → PrpList with 2 extra entries.
        let len = block_payload_bytes(3).unwrap();
        assert_eq!(prp_layout(len), PrpLayout::PrpList { n_entries: 2 });
    }

    #[test]
    fn prp_list_layout_for_max_block_count() {
        // OIP-014 § S2 cap: 2048 × 4 KiB = 8 MiB. PRP1 covers the
        // first page; the list covers the remaining 2047 pages.
        let len = block_payload_bytes(MAX_BLOCK_COUNT_PER_COMMAND).unwrap();
        assert_eq!(prp_layout(len), PrpLayout::PrpList { n_entries: 2047 });
    }

    #[test]
    fn block_payload_bytes_is_exact_multiples() {
        assert_eq!(block_payload_bytes(0), Some(0));
        assert_eq!(block_payload_bytes(1), Some(4096));
        assert_eq!(block_payload_bytes(8), Some(8 * 4096));
    }

    #[test]
    fn prp_layout_zero_length_degenerates_to_single_page() {
        // Flush/Discard carry no data buffer; the layout helper still
        // returns a well-defined value (the call site never inspects
        // PRP fields in that case).
        assert_eq!(prp_layout(0), PrpLayout::SinglePage);
    }

    #[test]
    fn prp_layout_just_above_one_page_boundary() {
        assert_eq!(prp_layout(PRP_PAGE_SIZE + 1), PrpLayout::TwoPages);
    }

    #[test]
    fn prp_layout_just_above_two_page_boundary() {
        // 8193 bytes: PRP1 covers bytes 0..4096 (one page); the
        // remaining 4097 bytes span two more pages (4096..8192 +
        // 8192..8193), so the PRP list needs 2 entries.
        assert_eq!(
            prp_layout(2 * PRP_PAGE_SIZE + 1),
            PrpLayout::PrpList { n_entries: 2 }
        );
    }
}
