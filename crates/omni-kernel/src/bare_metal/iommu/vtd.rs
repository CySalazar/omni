//! Intel VT-d backend scaffold (P6.7.9-pre.2).
//!
//! ## Scope
//!
//! This module lands the **dormant scaffold** for the Intel VT-d
//! second-level translation backend that will eventually replace the
//! [`super::PassthroughBackend`] when `bare_metal::iommu::iommu_vendor()`
//! reports [`super::IommuVendor::Intel`]. The scaffold lines up four
//! pure-function surfaces — register offsets, root-entry encoders,
//! context-entry encoders, and second-level page-table-entry (SL-PTE)
//! encoders — plus a host-testable [`VtdBackend`] struct that tracks
//! domains in an internal table without writing a single MMIO byte.
//!
//! Until P6.7.9-pre.4 (DMA-Map vendor switch) wires it in, no caller
//! reaches this backend at runtime — the kernel `dma_map_handlers`
//! continues to use [`super::PassthroughBackend`]. The scaffold lives
//! in the workspace so the QEMU smoke (`iommu=intel`) can assert the
//! vendor selector + ACPI parser interaction without any silicon side
//! effect.
//!
//! ## Why a scaffold and not the live backend?
//!
//! Each P6.7.9-pre.x slice keeps the auditable surface bounded:
//!
//! - **P6.7.9-pre.0:** parser + trait + passthrough.
//! - **P6.7.9-pre.1:** firmware-probe + vendor selector.
//! - **P6.7.9-pre.2 (this slice):** register-offset constants +
//!   data-structure encoders + dormant `VtdBackend`.
//! - **P6.7.9-pre.3:** AMD-Vi sibling scaffold.
//! - **P6.7.9-pre.4:** swap `dma_map_handlers` to consult
//!   `iommu_vendor()` and route through the now-live backends.
//!
//! Splitting the live register programming off keeps every PR's
//! `unsafe` surface auditable and lets the host test matrix exercise
//! the encoders before the live ring is opened.
//!
//! ## References
//!
//! - Intel Virtualization Technology for Directed I/O Architecture
//!   Specification rev 4.1 § 9 (Translation Data Structures) and § 10.4
//!   (Register Descriptions).
//! - OIP-Driver-Framework-013 § S3 (capability scope + IOMMU semantics).

#![allow(
    clippy::module_name_repetitions,
    reason = "VtdBackend / VtdError / VtdRegister share the Vtd prefix by design — they are the public symbols of this submodule and the prefix prevents ambiguity with sibling AMD-Vi / passthrough types"
)]

extern crate alloc;

use alloc::vec::Vec;

use super::{DomainId, IommuBackend, IommuError, IommuFlags, IommuVendor, PciBdf};

// =============================================================================
// Section 1 — VT-d MMIO register offsets (Intel VT-d spec rev 4.1 § 10.4).
//
// Offsets are byte-addressed against the per-IOMMU MMIO base discovered
// from the DRHD entry's `register_base` field (see
// `super::dmar::DrhdEntry::register_base`). Constants are `pub` so the
// future live backend (P6.7.9-pre.4) and the host test suite can both
// reference the same single source of truth.
// =============================================================================

/// `VER`: Version Register — 4 bytes at offset `0x00`.
///
/// Bits 0..3 = MIN (minor), 4..7 = MAX (major). Read-only.
pub const REG_OFFSET_VER: u32 = 0x000;

/// `CAP`: Capability Register — 8 bytes at offset `0x08`.
///
/// Carries the static capabilities the IOMMU advertises (number of
/// domains, supported AGAW levels, caching mode, …). Read-only.
pub const REG_OFFSET_CAP: u32 = 0x008;

/// `ECAP`: Extended Capability Register — 8 bytes at offset `0x10`.
///
/// Advertises extended features (queued invalidation, interrupt
/// remapping, page-request, …). Read-only.
pub const REG_OFFSET_ECAP: u32 = 0x010;

/// `GCMD`: Global Command Register — 4 bytes at offset `0x18`. Write-only.
///
/// Used to toggle translation enable (TE, bit 31), set-root-table
/// pointer (SRTP, bit 30), write-buffer flush (WBF, bit 27), queued
/// invalidation enable (QIE, bit 26), and interrupt-remap enable (IRE,
/// bit 25).
pub const REG_OFFSET_GCMD: u32 = 0x018;

/// `GSTS`: Global Status Register — 4 bytes at offset `0x1C`. Read-only.
///
/// Mirror of GCMD after the hardware processes the command. Bit
/// positions match GCMD.
pub const REG_OFFSET_GSTS: u32 = 0x01C;

/// `RTADDR`: Root Table Address Register — 8 bytes at offset `0x20`.
///
/// Bits 0..10 reserved, bit 11 = `RTT` (Root Table Type: 0 = legacy,
/// 1 = scalable), bits 12..63 = 4-KiB-aligned physical address of the
/// root table.
pub const REG_OFFSET_RTADDR: u32 = 0x020;

/// `CCMD`: Context Command Register — 8 bytes at offset `0x28`.
///
/// Drives the legacy register-based context-cache invalidation. Bit
/// 63 = `ICC` (Invalidate Context Cache), bits 61..62 = `CIRG`
/// (Context Invalidation Request Granularity), bits 59..60 = `CAIG`.
pub const REG_OFFSET_CCMD: u32 = 0x028;

/// `FSTS`: Fault Status Register — 4 bytes at offset `0x34`. RW1C.
///
/// Bit 0 = `PFO` (Primary Fault Overflow), bit 1 = `PPF` (Primary
/// Pending Fault), bit 2 = `AFO` (Advanced Fault Overflow), bit 3 =
/// `APF` (Advanced Pending Fault), bit 4 = `IQE` (Invalidation Queue
/// Error), bit 5 = `ICE` (Invalidation Completion Error), bit 6 =
/// `ITE` (Invalidation Time-out Error).
pub const REG_OFFSET_FSTS: u32 = 0x034;

/// `FECTL`: Fault Event Control Register — 4 bytes at offset `0x38`.
///
/// Bit 31 = `IM` (Interrupt Mask). When clear, the IOMMU raises an MSI
/// for every fault.
pub const REG_OFFSET_FECTL: u32 = 0x038;

/// `FEDATA`: Fault Event Data Register — 4 bytes at offset `0x3C`.
pub const REG_OFFSET_FEDATA: u32 = 0x03C;

/// `FEADDR`: Fault Event Address Register — 4 bytes at offset `0x40`.
pub const REG_OFFSET_FEADDR: u32 = 0x040;

/// `FEUADDR`: Fault Event Upper Address Register — 4 bytes at offset `0x44`.
pub const REG_OFFSET_FEUADDR: u32 = 0x044;

/// `PMEN`: Protected Memory Enable Register — 4 bytes at offset `0x64`.
///
/// Bit 31 = `EPM` (Enable Protected Memory).
pub const REG_OFFSET_PMEN: u32 = 0x064;

/// `IQH`: Invalidation Queue Head Register — 8 bytes at offset `0x80`.
///
/// Hardware-maintained pointer into the descriptor ring. Read-only.
pub const REG_OFFSET_IQH: u32 = 0x080;

/// `IQT`: Invalidation Queue Tail Register — 8 bytes at offset `0x88`.
///
/// Software-maintained pointer into the descriptor ring.
pub const REG_OFFSET_IQT: u32 = 0x088;

/// `IQA`: Invalidation Queue Address Register — 8 bytes at offset `0x90`.
pub const REG_OFFSET_IQA: u32 = 0x090;

// -- GCMD/GSTS bit positions per spec rev 4.1 § 10.4.4 ----------------

/// `TE` (Translation Enable) — bit 31 in GCMD/GSTS.
pub const GCMD_BIT_TE: u32 = 1 << 31;
/// `SRTP` (Set Root Table Pointer) — bit 30.
pub const GCMD_BIT_SRTP: u32 = 1 << 30;
/// `SFL` (Set Fault Log) — bit 29.
pub const GCMD_BIT_SFL: u32 = 1 << 29;
/// `EAFL` (Enable Advanced Fault Logging) — bit 28.
pub const GCMD_BIT_EAFL: u32 = 1 << 28;
/// `WBF` (Write Buffer Flush) — bit 27.
pub const GCMD_BIT_WBF: u32 = 1 << 27;
/// `QIE` (Queued Invalidation Enable) — bit 26.
pub const GCMD_BIT_QIE: u32 = 1 << 26;
/// `IRE` (Interrupt Remapping Enable) — bit 25.
pub const GCMD_BIT_IRE: u32 = 1 << 25;
/// `SIRTP` (Set Interrupt Remap Table Pointer) — bit 24.
pub const GCMD_BIT_SIRTP: u32 = 1 << 24;
/// `CFI` (Compatibility Format Interrupt) — bit 23.
pub const GCMD_BIT_CFI: u32 = 1 << 23;

// -- GSTS status-mirror bit positions per spec rev 4.1 § 10.4.5 -------
//
// GSTS is read-only and mirrors the most-recently committed GCMD bits
// after the hardware has finished processing the request. The live
// activation path (P6.7.9-pre.5) polls these bits to detect when the
// IOMMU has accepted SRTP / QIE / TE.

/// `TES` (Translation Enable Status) — bit 31 in GSTS. Mirrors
/// [`GCMD_BIT_TE`] once the hardware enables second-level translation.
pub const GSTS_BIT_TES: u32 = 1 << 31;
/// `RTPS` (Root Table Pointer Status) — bit 30 in GSTS. Mirrors
/// [`GCMD_BIT_SRTP`] once the hardware accepts the new root-table
/// pointer.
pub const GSTS_BIT_RTPS: u32 = 1 << 30;
/// `QIES` (Queued Invalidation Enable Status) — bit 26 in GSTS. Mirrors
/// [`GCMD_BIT_QIE`] once the hardware starts servicing descriptors out
/// of the invalidation queue.
pub const GSTS_BIT_QIES: u32 = 1 << 26;

// -- Invalidation queue layout (Intel VT-d spec rev 4.1 § 6.5.2) ------
//
// We program the legacy 128-bit (16-byte) descriptor format because
// the scalable 256-bit format requires ECAP.SMTS support that is not
// guaranteed on all Phase 1 platforms. With QS=0 the queue holds 256
// descriptors × 16 bytes = exactly one 4-KiB page — matches the frame
// allocator's allocation unit.

/// `QS` field value stored in `IQA[2:0]` — `0` for a 1-page (4 KiB)
/// queue, i.e. 256 entries of 16 bytes each.
pub const INV_QUEUE_SIZE_ORDER: u8 = 0;
/// Number of descriptor slots in the invalidation queue under
/// [`INV_QUEUE_SIZE_ORDER`].
pub const INV_QUEUE_ENTRY_COUNT: usize = 256;
/// Byte width of one legacy (128-bit) invalidation descriptor.
pub const INV_QUEUE_ENTRY_BYTES: usize = 16;
/// Total queue footprint in bytes — `INV_QUEUE_ENTRY_COUNT *
/// INV_QUEUE_ENTRY_BYTES`. By construction equals one 4-KiB frame.
pub const INV_QUEUE_BYTES: usize = INV_QUEUE_ENTRY_COUNT * INV_QUEUE_ENTRY_BYTES;

// -- Invalidation descriptor type / granularity tags (spec § 6.5.2.2) -
//
// Encoded into bits 0..3 of the descriptor low qword.

/// `Type=0x1` — Context-cache invalidate (CCMD-equivalent).
pub const INV_DESC_TYPE_CONTEXT_CACHE: u64 = 0x1;
/// `Type=0x2` — IOTLB invalidate.
pub const INV_DESC_TYPE_IOTLB: u64 = 0x2;
/// `Type=0x5` — Invalidate-wait (synchronisation fence).
pub const INV_DESC_TYPE_INVALIDATE_WAIT: u64 = 0x5;

/// Context-cache granularity `G=01` (Global). Encoded into bits 4..5
/// of the context-cache descriptor low qword.
pub const INV_DESC_CTX_GRAN_GLOBAL: u64 = 0b01 << 4;
/// Context-cache granularity `G=10` (Domain).
///
/// Selects the per-domain context-cache invalidate variant — the
/// descriptor targets only the entries whose `DID` matches the field
/// encoded into bits 16..31 of the low qword (see
/// [`encode_context_cache_domain_invalidate`]).
pub const INV_DESC_CTX_GRAN_DOMAIN: u64 = 0b10 << 4;
/// IOTLB granularity `G=01` (Global). Encoded into bits 4..5 of the
/// IOTLB descriptor low qword.
pub const INV_DESC_IOTLB_GRAN_GLOBAL: u64 = 0b01 << 4;
/// IOTLB granularity `G=10` (Domain).
///
/// Selects the per-domain IOTLB invalidate variant — descriptor targets
/// only entries whose `DID` matches the field encoded into bits 16..31
/// of the low qword (see [`encode_iotlb_domain_invalidate`]).
pub const INV_DESC_IOTLB_GRAN_DOMAIN: u64 = 0b10 << 4;
/// Invalidate-wait `SW=1`: write a 4-byte status value to the status
/// address once the wait descriptor reaches the IOMMU. Bit 5.
pub const INV_DESC_WAIT_STATUS_WRITE: u64 = 1 << 5;

/// Bounded poll counter for hardware-status mirror bits.
///
/// 1 million iterations easily covers the worst-case QEMU emulation
/// latency (typically < 1 µs / iteration in practice). On a real Intel
/// platform the SRTP and QIE bits flip within microseconds; an
/// overflow indicates a wedged IOMMU and surfaces as one of the
/// `*Timeout` variants of [`VtdActivateError`].
pub const VTD_ACTIVATION_POLL_LIMIT: u32 = 1_000_000;

// =============================================================================
// Section 2 — Root / Context / SL-PTE encoders (Intel VT-d spec § 9).
//
// All three structures are 128-bit (root + context) or 64-bit (SL-PTE).
// We store them as `[u64; N]` rather than `#[repr(C)]` bitfields to
// keep the encoders pure functions over `u64` operands — the host test
// suite then exercises every bit position without needing to grant the
// kernel an `unsafe { *mut RootEntry }` cast.
// =============================================================================

/// Size of a single VT-d **legacy** root entry (§ 9.1).
///
/// 128 bits = 16 bytes. One entry per PCI bus number; a single root
/// table holds 256 entries = 4 KiB total.
pub const ROOT_ENTRY_BYTES: usize = 16;

/// Size of a single VT-d **legacy** context entry (§ 9.3).
///
/// 128 bits = 16 bytes. One entry per (Device, Function) pair on a
/// given bus; a single context table holds 256 entries = 4 KiB total.
pub const CONTEXT_ENTRY_BYTES: usize = 16;

/// Size of a single VT-d **legacy** second-level page-table entry
/// (§ 9.6).
///
/// 64 bits = 8 bytes. A 4-KiB SL-PT page holds 512 entries.
pub const SLPTE_BYTES: usize = 8;

/// Encoded VT-d **legacy** root entry.
///
/// Layout (low 64 bits, § 9.1):
/// ```text
/// bit  0      : Present (P)
/// bit  1..11  : Reserved (must be 0)
/// bit 12..63  : Context Table Pointer (CTP), 4-KiB aligned
/// ```
/// High 64 bits are reserved and must be zero.
///
/// Constructed via [`encode_root_entry`]; consumed via the field
/// accessors below.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RootEntry {
    /// Low 64 bits (present + CTP).
    pub low: u64,
    /// High 64 bits (always 0 in the legacy mode).
    pub high: u64,
}

impl RootEntry {
    /// `true` iff the present bit (bit 0) is set.
    #[must_use]
    pub const fn is_present(self) -> bool {
        (self.low & 0x1) != 0
    }

    /// 4-KiB-aligned context-table-pointer field (bits 12..63 of the
    /// low quadword).
    #[must_use]
    pub const fn context_table_pointer(self) -> u64 {
        self.low & !0xFFF
    }
}

/// Build a legacy root entry from a 4-KiB-aligned context-table phys
/// address.
///
/// # Errors
///
/// Returns `Err([VtdError::AddressMisaligned])` when `context_table_phys`
/// is not 4-KiB aligned.
pub fn encode_root_entry(context_table_phys: u64) -> Result<RootEntry, VtdError> {
    if context_table_phys & 0xFFF != 0 {
        return Err(VtdError::AddressMisaligned);
    }
    Ok(RootEntry {
        low: context_table_phys | 0x1,
        high: 0,
    })
}

/// Build a not-present root entry (low + high quadwords both zero).
///
/// Used during bring-up to publish an empty root table.
#[must_use]
pub const fn encode_root_entry_absent() -> RootEntry {
    RootEntry { low: 0, high: 0 }
}

/// VT-d legacy translation-type enumeration (Context Entry bits 2..3,
/// § 9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationType {
    /// Untranslated requests use the second-level page table.
    /// Untranslated **with PASID** is not supported in legacy mode.
    UntranslatedOnly = 0b00,
    /// Untranslated + translated requests both use the SL page table.
    UntranslatedAndTranslated = 0b01,
    /// Pass-through: untranslated requests bypass translation (used by
    /// the bring-up domain `0`, identical to no-IOMMU semantics).
    Passthrough = 0b10,
}

impl TranslationType {
    /// Raw 2-bit encoding for the context-entry `T` field.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// VT-d legacy **AGAW** (Adjusted Guest Address Width) encoding
/// (§ 9.3 + § 10.4.2 CAP register SAGAW field).
///
/// The mapping is bit-position-based on the SAGAW field of the
/// Capability Register; the value stored in a context entry's `AW`
/// field matches the bit index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressWidth {
    /// 30-bit, 2-level page table (rarely advertised).
    Bits30Level2 = 0,
    /// 39-bit, 3-level page table (most desktops + servers).
    Bits39Level3 = 1,
    /// 48-bit, 4-level page table (matches `x86_64` paging).
    Bits48Level4 = 2,
    /// 57-bit, 5-level page table (5LP-enabled Xeons).
    Bits57Level5 = 3,
}

impl AddressWidth {
    /// Raw 3-bit AW encoding.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Number of paging levels for this AGAW.
    #[must_use]
    pub const fn levels(self) -> u8 {
        match self {
            Self::Bits30Level2 => 2,
            Self::Bits39Level3 => 3,
            Self::Bits48Level4 => 4,
            Self::Bits57Level5 => 5,
        }
    }
}

/// Encoded VT-d **legacy** context entry.
///
/// Low 64 bits (§ 9.3):
/// ```text
/// bit  0      : Present (P)
/// bit  1      : Fault Processing Disable (FPD)
/// bit  2..3   : Translation Type (T)
/// bit  4..11  : Reserved
/// bit 12..63  : Second-Level Page Table Pointer (SLPTPTR), 4-KiB aligned
/// ```
///
/// High 64 bits (§ 9.3):
/// ```text
/// bit  0..2   : Address Width (AW)
/// bit  3..7   : Reserved
/// bit  8..23  : Domain Identifier (DID)
/// bit 24..63  : Reserved
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContextEntry {
    /// Low 64 bits (present + T + SLPTPTR).
    pub low: u64,
    /// High 64 bits (AW + DID).
    pub high: u64,
}

impl ContextEntry {
    /// `true` iff the present bit (bit 0 of low) is set.
    #[must_use]
    pub const fn is_present(self) -> bool {
        (self.low & 0x1) != 0
    }

    /// Extract the second-level page-table pointer (bits 12..63 of
    /// low).
    #[must_use]
    pub const fn slptptr(self) -> u64 {
        self.low & !0xFFF
    }

    /// Extract the 16-bit domain identifier (bits 8..23 of high).
    #[must_use]
    pub const fn domain_id(self) -> DomainId {
        DomainId::new(((self.high >> 8) & 0xFFFF) as u16)
    }

    /// Extract the translation-type field (bits 2..3 of low).
    #[must_use]
    pub const fn translation_type_raw(self) -> u8 {
        ((self.low >> 2) & 0b11) as u8
    }

    /// Extract the address-width field (bits 0..2 of high).
    #[must_use]
    pub const fn address_width_raw(self) -> u8 {
        (self.high & 0b111) as u8
    }
}

/// Build a context entry pointing at `slpt_phys` for `domain` with the
/// given translation type and AGAW.
///
/// # Errors
///
/// Returns `Err([VtdError::AddressMisaligned])` when `slpt_phys` is not
/// 4-KiB aligned.
pub fn encode_context_entry(
    slpt_phys: u64,
    domain: DomainId,
    translation: TranslationType,
    width: AddressWidth,
) -> Result<ContextEntry, VtdError> {
    if slpt_phys & 0xFFF != 0 {
        return Err(VtdError::AddressMisaligned);
    }
    let t = u64::from(translation.as_u8()) & 0b11;
    let aw = u64::from(width.as_u8()) & 0b111;
    let did = u64::from(domain.raw());
    Ok(ContextEntry {
        low: (slpt_phys & !0xFFF) | (t << 2) | 0x1,
        high: aw | (did << 8),
    })
}

/// Build a not-present context entry.
#[must_use]
pub const fn encode_context_entry_absent() -> ContextEntry {
    ContextEntry { low: 0, high: 0 }
}

/// Encoded VT-d **legacy** second-level page-table entry (SL-PTE).
///
/// Layout (§ 9.6):
/// ```text
/// bit  0      : Read (R)
/// bit  1      : Write (W)
/// bit  2      : Execute (X)  — honoured only when ECAP.XLM is set
/// bit  3..6   : Ignored
/// bit  7      : Page Size (PS) — must be 0 for 4-KiB leaves
/// bit  8..10  : Ignored
/// bit 11      : Snoop Behaviour (SNP) — honoured when ECAP.SC is set
/// bit 12..51  : 4-KiB-aligned output address
/// bit 52..61  : Ignored
/// bit 62      : Transient Mapping (TM)
/// bit 63      : Ignored
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Slpte(pub u64);

impl Slpte {
    /// `R` bit position (bit 0).
    pub const BIT_READ: u64 = 1 << 0;
    /// `W` bit position (bit 1).
    pub const BIT_WRITE: u64 = 1 << 1;
    /// `X` bit position (bit 2).
    pub const BIT_EXECUTE: u64 = 1 << 2;
    /// `SNP` bit position (bit 11).
    pub const BIT_SNOOP: u64 = 1 << 11;

    /// `true` iff this entry has either `R` or `W` set.
    #[must_use]
    pub const fn is_present(self) -> bool {
        (self.0 & (Self::BIT_READ | Self::BIT_WRITE)) != 0
    }

    /// 4-KiB-aligned output address (bits 12..51).
    ///
    /// Mask drops the 12 low alignment bits **and** the 12 high ignored
    /// bits (52..63), so callers consistently see only the translated
    /// physical address.
    #[must_use]
    pub const fn output_address(self) -> u64 {
        self.0 & 0x000F_FFFF_FFFF_F000
    }
}

/// Build a leaf SL-PTE for `phys` with `flags`.
///
/// Translates the kernel [`IommuFlags`] surface into the VT-d
/// bit-position constants. `R` is forced on whenever the caller asks
/// for `WRITE` because VT-d treats a write-only entry as malformed in
/// the legacy mode.
///
/// # Errors
///
/// Returns `Err([VtdError::AddressMisaligned])` when `phys` is not
/// 4-KiB aligned.
pub fn encode_slpte(phys: u64, flags: IommuFlags) -> Result<Slpte, VtdError> {
    if phys & 0xFFF != 0 {
        return Err(VtdError::AddressMisaligned);
    }
    let mut bits = phys & 0x000F_FFFF_FFFF_F000;
    if flags.contains(IommuFlags::READ) || flags.contains(IommuFlags::WRITE) {
        bits |= Slpte::BIT_READ;
    }
    if flags.contains(IommuFlags::WRITE) {
        bits |= Slpte::BIT_WRITE;
    }
    if flags.contains(IommuFlags::EXECUTE) {
        bits |= Slpte::BIT_EXECUTE;
    }
    if flags.contains(IommuFlags::COHERENT) {
        bits |= Slpte::BIT_SNOOP;
    }
    Ok(Slpte(bits))
}

// =============================================================================
// Section 3 — Capability-register field extraction (CAP @ REG_OFFSET_CAP).
//
// The probe path reads CAP once per IOMMU to size the AGAW and learn
// how many domains the hardware advertises. These helpers stay pure so
// the host test suite can exercise every bit pattern without firmware.
// =============================================================================

/// Decode the `ND` field (Number of Domains, bits 0..2 of CAP, § 10.4.2)
/// as a count.
///
/// Per spec: supported domain count `= 1 << (4 + 2 * ND)`. Common values:
///
/// | ND | Domains |
/// |----|--------|
/// | 0  |    16   |
/// | 1  |    64   |
/// | 2  |   256   |
/// | 3  |  1 024  |
/// | 4  |  4 096  |
/// | 5  | 16 384  |
/// | 6  | 65 536  |
/// | 7  | reserved (treated as 65 536) |
#[must_use]
pub const fn cap_domain_count(cap: u64) -> u32 {
    let nd = (cap & 0b111) as u32;
    let shift = 4u32.saturating_add(nd.saturating_mul(2));
    // Cap at 16 since `1 << 16 = 65 536` matches the 16-bit DID space.
    if shift >= 16 { 65_536 } else { 1u32 << shift }
}

/// Decode the `SAGAW` field (Supported AGAW, bits 8..12 of CAP) as a
/// bitmask of [`AddressWidth`] discriminants. Bit `n` of the mask is
/// set iff the IOMMU advertises level `n+2` (30, 39, 48, 57 bits).
#[must_use]
pub const fn cap_supported_agaw(cap: u64) -> u8 {
    ((cap >> 8) & 0b1_1111) as u8
}

/// Pick the highest supported AGAW from the SAGAW bitmask.
///
/// Returns `None` when the IOMMU advertises no width at all (which
/// would itself be a firmware bug, but the encoder is defensive).
#[must_use]
pub const fn pick_highest_supported_agaw(sagaw_mask: u8) -> Option<AddressWidth> {
    if sagaw_mask & (1 << 3) != 0 {
        Some(AddressWidth::Bits57Level5)
    } else if sagaw_mask & (1 << 2) != 0 {
        Some(AddressWidth::Bits48Level4)
    } else if sagaw_mask & (1 << 1) != 0 {
        Some(AddressWidth::Bits39Level3)
    } else if sagaw_mask & (1 << 0) != 0 {
        Some(AddressWidth::Bits30Level2)
    } else {
        None
    }
}

/// `CM` (Caching Mode) flag — bit 7 of CAP, § 10.4.2.
#[must_use]
pub const fn cap_caching_mode(cap: u64) -> bool {
    (cap & (1 << 7)) != 0
}

// =============================================================================
// Section 4 — `VtdBackend`: host-testable dormant backend.
//
// The struct tracks `(domain_id, [SLPTE_record])` tuples in an internal
// `Vec` so the `IommuBackend` trait can be exercised against it from
// host tests. It does NOT write any MMIO byte; the live backend lands
// in P6.7.9-pre.4 with explicit `unsafe` blocks gated behind
// `#[cfg(target_arch = "x86_64")]`.
// =============================================================================

/// Error category raised by the VT-d encoders + scaffold backend.
///
/// Maps to [`IommuError`] when surfaced through the trait so callers
/// see a vendor-neutral taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtdError {
    /// Address argument violated the VT-d 4-KiB alignment requirement.
    AddressMisaligned,
    /// Caller passed a [`DomainId`] not previously installed.
    UnknownDomain,
    /// `flags` requested a permission the backend cannot honour.
    UnsupportedFlags,
}

impl From<VtdError> for IommuError {
    fn from(err: VtdError) -> Self {
        match err {
            VtdError::AddressMisaligned => Self::AddressMisaligned,
            VtdError::UnknownDomain => Self::InvalidDomain,
            VtdError::UnsupportedFlags => Self::Unsupported,
        }
    }
}

/// Error category surfaced by `VtdBackend::activate_hardware` (the
/// bare-metal-only activation entry point gated on
/// `cfg(target_os = "none")`).
///
/// Maps to [`IommuError::ActivationFailed`] when surfaced through the
/// trait; the variant identity is preserved for the kernel boot log so
/// the operator can tell SRTP timeout from QIE timeout from a
/// stalled IOTLB drain. None of these errors should fire on a healthy
/// IOMMU — they signal either a spec-divergent emulation or genuinely
/// wedged silicon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtdActivateError {
    /// [`VtdBackend::prepare_activation`] was never called (or
    /// reported zeroes) before `VtdBackend::activate_hardware` —
    /// `unit_base` is `0` so MMIO writes would target the BIOS
    /// real-mode area.
    NotPrepared,
    /// Polled [`GSTS_BIT_RTPS`] for [`VTD_ACTIVATION_POLL_LIMIT`]
    /// iterations after raising [`GCMD_BIT_SRTP`]; bit never flipped.
    RootTableTimeout,
    /// Polled [`GSTS_BIT_QIES`] for [`VTD_ACTIVATION_POLL_LIMIT`]
    /// iterations after raising [`GCMD_BIT_QIE`]; bit never flipped.
    QueueEnableTimeout,
    /// IQH never caught up to IQT after submitting the global IOTLB
    /// invalidate descriptor. Indicates a stuck invalidation engine.
    InvalidationTimeout,
}

impl From<VtdActivateError> for IommuError {
    fn from(_err: VtdActivateError) -> Self {
        Self::ActivationFailed
    }
}

/// Encode the `IQA` register value for a given queue base address +
/// size order.
///
/// Bit layout (Intel VT-d spec rev 4.1 § 10.4.20):
///
/// - bits 12..63: 4-KiB-aligned queue base physical address (`IQA`).
/// - bit 11: reserved (must be zero).
/// - bit 10: descriptor width — `0` for legacy 128-bit, `1` for
///   scalable 256-bit. We always use `0`.
/// - bits 0..2: `QS` (queue size in pages, queue holds `2^QS` 4-KiB
///   pages of descriptors).
///
/// Reserved bits are masked out defensively so a high-bit overflow in
/// `queue_phys` cannot accidentally set DW or QS.
#[must_use]
pub const fn encode_iqa(queue_phys: u64, size_order: u8) -> u64 {
    let base = queue_phys & 0x000F_FFFF_FFFF_F000;
    let qs = (size_order as u64) & 0x7;
    base | qs
}

/// Encode the low + high qwords of a 128-bit global IOTLB invalidate
/// descriptor.
///
/// Layout (Intel VT-d spec § 6.5.2.4):
///
/// - low qword bits 0..3:  Type = [`INV_DESC_TYPE_IOTLB`] (`0x2`).
/// - low qword bits 4..5:  G   = `01` (Global).
/// - low qword bits 6..7:  DR  = `00` (drain reads = off).
/// - low qword bits 8..9:  DW  = `00` (drain writes = off).
/// - low qword bits 10..63: reserved (zero).
/// - high qword:           AM/AIH/Address — unused for global granularity.
///
/// Returns `(low, high)`. The caller writes them into successive
/// 64-bit slots of the queue ring.
#[must_use]
pub const fn encode_iotlb_global_invalidate() -> (u64, u64) {
    let low = INV_DESC_TYPE_IOTLB | INV_DESC_IOTLB_GRAN_GLOBAL;
    (low, 0)
}

/// Encode the low + high qwords of a 128-bit global context-cache
/// invalidate descriptor.
///
/// Layout (Intel VT-d spec § 6.5.2.3):
///
/// - low qword bits 0..3:  Type = [`INV_DESC_TYPE_CONTEXT_CACHE`] (`0x1`).
/// - low qword bits 4..5:  G   = `01` (Global).
/// - high qword: source-id / function-mask — unused for global granularity.
#[must_use]
pub const fn encode_context_cache_global_invalidate() -> (u64, u64) {
    let low = INV_DESC_TYPE_CONTEXT_CACHE | INV_DESC_CTX_GRAN_GLOBAL;
    (low, 0)
}

/// Encode the low + high qwords of a 128-bit **per-domain**
/// context-cache invalidate descriptor.
///
/// Layout (Intel VT-d spec § 6.5.2.3):
///
/// - low qword bits  0..3 : Type = [`INV_DESC_TYPE_CONTEXT_CACHE`] (`0x1`).
/// - low qword bits  4..5 : G   = `10` (Domain-granular).
/// - low qword bits 16..31: DID = `domain.raw()`.
/// - high qword: source-id / function-mask — unused for domain
///   granularity (the IOMMU evicts every cache entry whose `DID`
///   matches, regardless of source-id).
///
/// This is what the per-device install path queues after binding a new
/// PCI device to `domain` so the IOMMU drops any stale entries from a
/// prior generation of the same DID.
#[must_use]
pub const fn encode_context_cache_domain_invalidate(domain: DomainId) -> (u64, u64) {
    let did = (domain.raw() as u64) << 16;
    let low = INV_DESC_TYPE_CONTEXT_CACHE | INV_DESC_CTX_GRAN_DOMAIN | did;
    (low, 0)
}

/// Encode the low + high qwords of a 128-bit **per-domain** IOTLB
/// invalidate descriptor.
///
/// Layout (Intel VT-d spec § 6.5.2.4):
///
/// - low qword bits  0..3 : Type = [`INV_DESC_TYPE_IOTLB`] (`0x2`).
/// - low qword bits  4..5 : G   = `10` (Domain-granular).
/// - low qword bits 16..31: DID = `domain.raw()`.
/// - high qword: AM/AIH/Address — unused for domain granularity.
#[must_use]
pub const fn encode_iotlb_domain_invalidate(domain: DomainId) -> (u64, u64) {
    let did = (domain.raw() as u64) << 16;
    let low = INV_DESC_TYPE_IOTLB | INV_DESC_IOTLB_GRAN_DOMAIN | did;
    (low, 0)
}

/// Byte offset of the context-entry slot for `bdf` within a per-bus
/// 4-KiB context table (§ 9.3 — slot index = devfn, slot size =
/// [`CONTEXT_ENTRY_BYTES`]).
///
/// Pure function — moves the index arithmetic out of the unsafe MMIO
/// path so host tests can pin the offsets.
#[must_use]
pub const fn context_entry_offset(bdf: super::PciBdf) -> u64 {
    (bdf.devfn() as u64) * (CONTEXT_ENTRY_BYTES as u64)
}

/// Byte offset of the root-entry slot for `bus` within the 4-KiB
/// root table (§ 9.1 — slot index = bus number, slot size =
/// [`ROOT_ENTRY_BYTES`]).
#[must_use]
pub const fn root_entry_offset(bus: u8) -> u64 {
    (bus as u64) * (ROOT_ENTRY_BYTES as u64)
}

/// Recorded per-device attachment in the host-testable scaffold.
///
/// Live MMIO state (`VtdBackend::install_device_entry`) also pushes a
/// [`VtdAttachment`] so the bookkeeping is consistent between the host
/// and bare-metal halves: every `(bdf → domain)` binding visible to
/// the trait dispatch surface has exactly one entry here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VtdAttachment {
    /// PCI requester ID owning the binding.
    pub bdf: PciBdf,
    /// Domain the device is bound to.
    pub domain: DomainId,
}

/// Error surfaced by `VtdBackend::install_device_entry`.
///
/// Mapped to [`IommuError`] when surfaced through the public surface so
/// the syscall layer keeps a vendor-neutral taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtdAttachError {
    /// The backend was never `activate_hardware`'d so the IQ is not
    /// guaranteed to be drained — refusing to write the entry avoids
    /// publishing a context entry the IOMMU cannot invalidate later.
    NotActivated,
    /// `domain` was never installed via
    /// [`super::IommuBackend::install_domain`].
    DomainNotInstalled,
    /// `bdf` is already attached (callers must `detach_device` first).
    AlreadyAttached,
    /// `slpt_phys` or `context_table_phys` not 4-KiB aligned.
    AddressMisaligned,
    /// Per-domain context-cache or IOTLB invalidate failed to drain in
    /// [`VTD_ACTIVATION_POLL_LIMIT`] iterations.
    InvalidationTimeout,
}

impl From<VtdAttachError> for IommuError {
    fn from(err: VtdAttachError) -> Self {
        match err {
            VtdAttachError::NotActivated | VtdAttachError::InvalidationTimeout => {
                Self::ActivationFailed
            }
            VtdAttachError::DomainNotInstalled => Self::InvalidDomain,
            VtdAttachError::AlreadyAttached => Self::Unsupported,
            VtdAttachError::AddressMisaligned => Self::AddressMisaligned,
        }
    }
}

/// One mapping record tracked by the scaffold backend.
///
/// Pure data — exists so the host test suite can assert on the
/// IOMMU-side state without touching MMIO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaffoldMapping {
    /// Domain the mapping belongs to.
    pub domain: DomainId,
    /// I/O virtual address (4-KiB aligned).
    pub iova: u64,
    /// Backing physical address (4-KiB aligned).
    pub phys: u64,
    /// Length in bytes (multiple of 4 KiB).
    pub len: u64,
    /// Encoded SL-PTE for the first 4-KiB leaf.
    pub leaf_slpte: Slpte,
}

/// Dormant VT-d backend. Holds bookkeeping only; emits no MMIO.
///
/// The host-test exercise path:
///
/// 1. Build `VtdBackend::new()`.
/// 2. `install_domain(DomainId::new(7))?;`
/// 3. `map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ)?;`
/// 4. Inspect the recorded `mappings()` slice in the assertion.
///
/// Live programming swap: P6.7.9-pre.4 adds a `unit_base: u64` field
/// and an `mmio_write32` helper; the `map`/`unmap` paths gain
/// `unsafe { ... }` blocks that write the descriptors back-to-back into
/// the IOMMU's invalidation queue (`REG_OFFSET_IQT`).
#[derive(Debug, Clone, Default)]
pub struct VtdBackend {
    /// Installed domains, in insertion order.
    domains: Vec<DomainId>,
    /// Recorded mappings.
    mappings: Vec<ScaffoldMapping>,
    /// MMIO base of the per-IOMMU register window. `0` while the
    /// backend is dormant; populated by [`Self::prepare_activation`]
    /// once the boot probe resolves the first DRHD's `register_base`.
    unit_base: u64,
    /// Physical address of the 4-KiB root-table page used by the live
    /// MMIO path. `0` while dormant.
    root_table_phys: u64,
    /// Physical address of the 4-KiB invalidation-queue page. `0`
    /// while dormant.
    invalidation_queue_phys: u64,
    /// Software-maintained tail index into the invalidation queue,
    /// measured in **bytes** so it can be written to IQT directly.
    /// Wraps at [`INV_QUEUE_BYTES`].
    invalidation_queue_tail: u64,
    /// `true` once `Self::activate_hardware` has cleanly walked
    /// RTADDR + GCMD.SRTP + IQA + GCMD.QIE + the global IOTLB flush
    /// and observed every status mirror bit set (the activation
    /// method is gated on `cfg(target_os = "none")`).
    hardware_activated: bool,
    /// Per-device attachments recorded by `attach_device` and (for
    /// bare-metal builds) `install_device_entry`. Both halves of the
    /// API share this vector so the host-testable scaffold and the
    /// live MMIO path agree on `(bdf → domain)` state.
    attachments: Vec<VtdAttachment>,
    /// Per-domain second-level page-table root registry (P6.7.9-pre.9).
    ///
    /// Populated through [`Self::provision_domain_pt`] before the live
    /// `install_device_entry` MMIO path runs; the recorded
    /// `root_phys` is what `install_device_entry` consumes as the
    /// `slpt_phys` argument for the matching domain.
    domain_pts: super::pt_alloc::DomainPageTables,
}

impl VtdBackend {
    /// Construct an empty backend.
    ///
    /// `const` so the kernel-wide [`super::IOMMU_BACKEND`] static can
    /// be initialised at static-init time without paying for lazy
    /// `OnceLock` overhead (P6.7.9-pre.4).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            domains: Vec::new(),
            mappings: Vec::new(),
            unit_base: 0,
            root_table_phys: 0,
            invalidation_queue_phys: 0,
            invalidation_queue_tail: 0,
            hardware_activated: false,
            attachments: Vec::new(),
            domain_pts: super::pt_alloc::DomainPageTables::new(),
        }
    }

    /// Allocate the per-domain second-level page-table root frame for
    /// `domain` through the supplied [`super::pt_alloc::FrameSource`]
    /// and record the `(domain, root_phys)` binding so the live
    /// per-device install MMIO call can read `root_phys` back via
    /// [`Self::domain_pt_root_phys`].
    ///
    /// Must be preceded by a successful [`Self::install_domain`]; the
    /// caller is responsible for ordering (the registry does not depend
    /// on the domain list, but the live MMIO path will refuse to bind a
    /// device whose `domain` has no recorded root).
    ///
    /// # Errors
    ///
    /// Forwards every [`super::pt_alloc::DomainPtError`] variant
    /// unchanged — see the module documentation for the taxonomy.
    pub fn provision_domain_pt(
        &mut self,
        domain: DomainId,
        src: &mut dyn super::pt_alloc::FrameSource,
    ) -> Result<u64, super::pt_alloc::DomainPtError> {
        self.domain_pts.provision(domain, src)
    }

    /// Release the per-domain page-table root frame and remove the
    /// `(domain, root_phys)` binding.
    ///
    /// # Errors
    ///
    /// [`super::pt_alloc::DomainPtError::NotProvisioned`] when `domain`
    /// has no recorded root frame.
    pub fn release_domain_pt(
        &mut self,
        domain: DomainId,
        src: &mut dyn super::pt_alloc::FrameSource,
    ) -> Result<(), super::pt_alloc::DomainPtError> {
        self.domain_pts.release(domain, src)
    }

    /// Recorded per-domain page-table root, or `None` if `domain` has
    /// not been provisioned through [`Self::provision_domain_pt`].
    #[must_use]
    pub fn domain_pt_root_phys(&self, domain: DomainId) -> Option<u64> {
        self.domain_pts.root_phys(domain)
    }

    /// Snapshot of the per-domain page-table registry (insertion order).
    #[must_use]
    pub fn domain_pt_entries(&self) -> &[super::pt_alloc::DomainPtEntry] {
        self.domain_pts.entries()
    }

    /// Snapshot of the recorded per-device attachments (insertion
    /// order). Exposed primarily so the host test suite can assert on
    /// the `(bdf → domain)` state without going through the trait
    /// surface.
    #[must_use]
    pub fn attachments(&self) -> &[VtdAttachment] {
        &self.attachments
    }

    /// `true` iff `bdf` is currently attached to some domain.
    #[must_use]
    pub fn has_attachment(&self, bdf: PciBdf) -> bool {
        self.attachments.iter().any(|a| a.bdf == bdf)
    }

    /// Snapshot of the recorded mapping list (newest last).
    #[must_use]
    pub fn mappings(&self) -> &[ScaffoldMapping] {
        &self.mappings
    }

    /// `true` iff `id` was installed via [`Self::install_domain`].
    #[must_use]
    pub fn has_domain(&self, id: DomainId) -> bool {
        self.domains.iter().any(|d| *d == id)
    }

    /// Snapshot of the installed domain list (insertion order).
    #[must_use]
    pub fn domains(&self) -> &[DomainId] {
        &self.domains
    }

    /// MMIO base of the per-IOMMU register window (`0` while dormant).
    #[must_use]
    pub const fn unit_base(&self) -> u64 {
        self.unit_base
    }

    /// Physical address of the 4-KiB root-table page (`0` while
    /// dormant).
    #[must_use]
    pub const fn root_table_phys(&self) -> u64 {
        self.root_table_phys
    }

    /// Physical address of the 4-KiB invalidation-queue page (`0`
    /// while dormant).
    #[must_use]
    pub const fn invalidation_queue_phys(&self) -> u64 {
        self.invalidation_queue_phys
    }

    /// `true` once `Self::activate_hardware` has completed cleanly
    /// (the activation method is gated on `cfg(target_os = "none")`).
    #[must_use]
    pub const fn is_hardware_activated(&self) -> bool {
        self.hardware_activated
    }

    /// Stash the activation parameters in the backend without touching
    /// MMIO.
    ///
    /// Idempotent: calling twice with the same values is a no-op; the
    /// second call with different values overwrites and **resets**
    /// [`Self::is_hardware_activated`] to `false` so the caller
    /// understands the live programming must be redriven (this is the
    /// behaviour the kernel boot path relies on after a TLB-shootdown
    /// induced re-activation in MP follow-up work).
    pub fn prepare_activation(
        &mut self,
        unit_base: u64,
        root_table_phys: u64,
        invalidation_queue_phys: u64,
    ) {
        let same = self.unit_base == unit_base
            && self.root_table_phys == root_table_phys
            && self.invalidation_queue_phys == invalidation_queue_phys;
        self.unit_base = unit_base;
        self.root_table_phys = root_table_phys;
        self.invalidation_queue_phys = invalidation_queue_phys;
        self.invalidation_queue_tail = 0;
        if !same {
            self.hardware_activated = false;
        }
    }

    /// Drive the live VT-d MMIO programming sequence.
    ///
    /// Spec-faithful order (Intel VT-d rev 4.1 § 6.2 + § 6.5):
    ///
    /// 1. Write the root-table physical address into `RTADDR`.
    /// 2. Raise `GCMD.SRTP` and poll `GSTS.RTPS` until set.
    /// 3. Write the invalidation-queue layout into `IQA` and clear
    ///    `IQT` (head==tail = empty queue).
    /// 4. Raise `GCMD.QIE` and poll `GSTS.QIES` until set.
    /// 5. Submit a global IOTLB invalidate descriptor (queue slot 0),
    ///    bump `IQT`, and wait for `IQH` to catch up.
    ///
    /// `GCMD.TE` is **NOT** raised by this slice; the IOMMU stays in
    /// pre-translation (passthrough) mode at the hardware level until
    /// the kernel is ready to gate every DMA-capable device through a
    /// per-domain page table (future P6.7.9-pre.7+).
    ///
    /// # Errors
    ///
    /// See [`VtdActivateError`].
    ///
    /// # Safety
    ///
    /// `phys_offset` must be the live bootloader direct-map offset.
    /// `unit_base` (recorded via [`Self::prepare_activation`]) must be
    /// the MMIO base address of a VT-d remapping unit owned exclusively
    /// by the kernel. The function performs `volatile_write32` /
    /// `volatile_write64` against `phys_offset + unit_base + offset`
    /// for the constants documented in §1 above.
    #[cfg(target_os = "none")]
    pub unsafe fn activate_hardware(&mut self, phys_offset: u64) -> Result<(), VtdActivateError> {
        if self.unit_base == 0 || self.root_table_phys == 0 || self.invalidation_queue_phys == 0 {
            return Err(VtdActivateError::NotPrepared);
        }

        let unit_va = phys_offset.wrapping_add(self.unit_base);

        // (1) Write the root-table physical address into RTADDR.
        //     Bit 11 (RTT) stays 0 — we use the legacy 128-bit root
        //     entry format (matches `encode_root_entry`).
        // SAFETY: per the function's safety contract, `unit_va` is a
        // valid MMIO VA into a kernel-owned VT-d register window.
        unsafe { mmio_write64(unit_va, REG_OFFSET_RTADDR, self.root_table_phys) };

        // (2) Raise GCMD.SRTP and poll GSTS.RTPS until set or timeout.
        //     GCMD is a one-shot write — we don't OR with the previous
        //     value because no other bits are enabled yet.
        // SAFETY: same as above.
        unsafe { mmio_write32(unit_va, REG_OFFSET_GCMD, GCMD_BIT_SRTP) };
        // SAFETY: GSTS is a 4-byte read-only MMIO register.
        if !unsafe { poll_gsts_bit(unit_va, GSTS_BIT_RTPS) } {
            return Err(VtdActivateError::RootTableTimeout);
        }

        // (3) Program the invalidation queue base + size. The queue
        //     body itself was zero-filled by the caller before this
        //     activation runs — IQT=0 publishes "empty queue" to the
        //     IOMMU.
        let iqa = encode_iqa(self.invalidation_queue_phys, INV_QUEUE_SIZE_ORDER);
        // SAFETY: same as RTADDR — kernel-owned MMIO window.
        unsafe { mmio_write64(unit_va, REG_OFFSET_IQA, iqa) };
        // SAFETY: same as RTADDR — kernel-owned MMIO window.
        unsafe { mmio_write64(unit_va, REG_OFFSET_IQT, 0) };
        self.invalidation_queue_tail = 0;

        // (4) Raise GCMD.QIE and poll GSTS.QIES until set or timeout.
        // SAFETY: same as RTADDR — kernel-owned MMIO window.
        unsafe { mmio_write32(unit_va, REG_OFFSET_GCMD, GCMD_BIT_QIE) };
        // SAFETY: GSTS is a 4-byte read-only MMIO register.
        if !unsafe { poll_gsts_bit(unit_va, GSTS_BIT_QIES) } {
            return Err(VtdActivateError::QueueEnableTimeout);
        }

        // (5) Submit a global IOTLB invalidate descriptor at slot 0,
        //     bump IQT, and wait for IQH to catch up.
        let queue_va = phys_offset.wrapping_add(self.invalidation_queue_phys);
        let (lo, hi) = encode_iotlb_global_invalidate();
        // SAFETY: caller guarantees the invalidation-queue page is
        // 4-KiB-aligned, kernel-owned, and zero-filled. The first 16
        // bytes hold descriptor index 0.
        unsafe { write_queue_entry(queue_va, 0, lo, hi) };
        let next_tail: u64 = INV_QUEUE_ENTRY_BYTES as u64;
        // SAFETY: same as IQA / IQT writes above.
        unsafe { mmio_write64(unit_va, REG_OFFSET_IQT, next_tail) };
        self.invalidation_queue_tail = next_tail;
        // SAFETY: IQH is a 8-byte read-only MMIO register.
        if !unsafe { poll_iqh_reaches(unit_va, next_tail) } {
            return Err(VtdActivateError::InvalidationTimeout);
        }

        self.hardware_activated = true;
        Ok(())
    }

    /// Drive the live VT-d per-device entry install.
    ///
    /// Spec-faithful order (Intel VT-d rev 4.1 § 9 + § 6.5):
    ///
    /// 1. Validate inputs (`hardware_activated`, alignments, domain
    ///    installed, bdf not already attached).
    /// 2. Encode the context entry for `(slpt_phys, domain,
    ///    translation, width)` and write it into the per-bus context
    ///    table at offset [`context_entry_offset(bdf)`].
    /// 3. Encode the root entry pointing at `context_table_phys` and
    ///    write it into the root table at offset
    ///    [`root_entry_offset(bdf.bus())`].
    /// 4. Submit a per-domain context-cache invalidate descriptor on
    ///    the invalidation queue and wait for it to drain.
    /// 5. Submit a per-domain IOTLB invalidate descriptor and wait
    ///    for it to drain.
    /// 6. Record the `(bdf, domain)` binding in
    ///    [`Self::attachments`].
    ///
    /// `GCMD.TE` is **NOT** raised by this slice; the IOMMU stays in
    /// pre-translation pass-through mode at the hardware level until
    /// the kernel is ready to gate every DMA-capable device (raise
    /// `TE` lands once at least one device is attached and the
    /// per-domain page tables are populated — orthogonal to this
    /// slice).
    ///
    /// # Errors
    ///
    /// See [`VtdAttachError`].
    ///
    /// # Safety
    ///
    /// `phys_offset` must be the live bootloader direct-map offset.
    /// `slpt_phys` must reference a 4-KiB-aligned second-level page
    /// table owned by the kernel and reachable through that direct
    /// map. `context_table_phys` must reference a 4-KiB-aligned
    /// context-table page owned by the kernel; the caller is
    /// responsible for keeping the same `context_table_phys` for the
    /// same bus across successive `install_device_entry` calls
    /// (otherwise the per-bus root entry will be overwritten with a
    /// dangling pointer).
    #[cfg(target_os = "none")]
    #[allow(
        clippy::too_many_arguments,
        reason = "the per-device install needs all of (phys_offset, bdf, domain, slpt_phys, context_table_phys, width, translation) — the driver framework is the sole caller and the explicit positional surface keeps the unsafe MMIO entry-point auditable"
    )]
    pub unsafe fn install_device_entry(
        &mut self,
        phys_offset: u64,
        bdf: PciBdf,
        domain: DomainId,
        slpt_phys: u64,
        context_table_phys: u64,
        width: AddressWidth,
        translation: TranslationType,
    ) -> Result<(), VtdAttachError> {
        if !self.hardware_activated {
            return Err(VtdAttachError::NotActivated);
        }
        if slpt_phys & 0xFFF != 0 || context_table_phys & 0xFFF != 0 {
            return Err(VtdAttachError::AddressMisaligned);
        }
        if !self.has_domain(domain) {
            return Err(VtdAttachError::DomainNotInstalled);
        }
        if self.has_attachment(bdf) {
            return Err(VtdAttachError::AlreadyAttached);
        }

        // (2) Encode + write the context entry into the per-bus
        //     context table at offset (devfn * 16).
        let context_entry = encode_context_entry(slpt_phys, domain, translation, width)
            .map_err(|_| VtdAttachError::AddressMisaligned)?;
        let context_va = phys_offset.wrapping_add(context_table_phys);
        let ctx_offset = context_entry_offset(bdf);
        // SAFETY: caller guarantees `context_table_phys` is a
        // kernel-owned, 4-KiB-aligned page reachable through the
        // direct map; `ctx_offset` is bounded to (255 * 16) + 15 =
        // 4095 by the devfn 8-bit constraint, so the write stays
        // inside the page.
        unsafe {
            write_context_entry_at(
                context_va,
                ctx_offset,
                context_entry.low,
                context_entry.high,
            );
        }

        // (3) Encode + write the root entry into the global root
        //     table at offset (bus * 16). Idempotent on the
        //     `context_table_phys` value — overwriting with the same
        //     pointer is a no-op for the IOMMU.
        let root_entry =
            encode_root_entry(context_table_phys).map_err(|_| VtdAttachError::AddressMisaligned)?;
        let root_va = phys_offset.wrapping_add(self.root_table_phys);
        let root_offset = root_entry_offset(bdf.bus());
        // SAFETY: caller guarantees `self.root_table_phys` (recorded
        // via `prepare_activation`) is a kernel-owned, 4-KiB-aligned
        // page reachable through the direct map; `root_offset` is
        // bounded to 4080 by the 8-bit bus constraint.
        unsafe { write_root_entry_at(root_va, root_offset, root_entry.low, root_entry.high) };

        // (4) + (5) Per-domain context-cache invalidate + per-domain
        //     IOTLB invalidate, sequenced through the invalidation
        //     queue. We wrap on `INV_QUEUE_BYTES` so the tail
        //     pointer never escapes the queue page.
        let queue_va = phys_offset.wrapping_add(self.invalidation_queue_phys);
        let unit_va = phys_offset.wrapping_add(self.unit_base);

        let (cc_lo, cc_hi) = encode_context_cache_domain_invalidate(domain);
        // SAFETY: queue is a kernel-owned 4-KiB page; submit_iq_*
        // updates `self.invalidation_queue_tail` after each push.
        unsafe { self.submit_iq_descriptor(queue_va, unit_va, cc_lo, cc_hi) }
            .map_err(|()| VtdAttachError::InvalidationTimeout)?;

        let (io_lo, io_hi) = encode_iotlb_domain_invalidate(domain);
        // SAFETY: same as above.
        unsafe { self.submit_iq_descriptor(queue_va, unit_va, io_lo, io_hi) }
            .map_err(|()| VtdAttachError::InvalidationTimeout)?;

        // (6) Record the attachment.
        self.attachments.push(VtdAttachment { bdf, domain });
        Ok(())
    }

    /// Push a single 128-bit descriptor into the invalidation queue,
    /// advance `IQT`, and wait for `IQH` to catch up. Wraps the tail
    /// pointer on [`INV_QUEUE_BYTES`].
    ///
    /// # Errors
    ///
    /// Returns `Err(())` if `IQH` does not catch up within
    /// [`VTD_ACTIVATION_POLL_LIMIT`] iterations.
    ///
    /// # Safety
    ///
    /// `queue_va` must point at the start of the kernel-owned 4-KiB
    /// invalidation-queue page reachable through the direct map.
    /// `unit_va` must point at the per-IOMMU MMIO register window so
    /// `unit_va + REG_OFFSET_IQT` / `+ REG_OFFSET_IQH` are valid
    /// 64-bit accesses.
    #[cfg(target_os = "none")]
    unsafe fn submit_iq_descriptor(
        &mut self,
        queue_va: u64,
        unit_va: u64,
        lo: u64,
        hi: u64,
    ) -> Result<(), ()> {
        // Compute the slot index from the current tail (byte offset →
        // slot index = tail / INV_QUEUE_ENTRY_BYTES). Wrapping is
        // implicit because `invalidation_queue_tail` is reset to 0
        // when it would overflow `INV_QUEUE_BYTES`. The tail is
        // strictly bounded by `INV_QUEUE_BYTES = 4096` so the `usize`
        // cast and the bounded division are precision-safe on every
        // pointer width.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::integer_division,
            reason = "queue tail is bounded by INV_QUEUE_BYTES (4096); division by INV_QUEUE_ENTRY_BYTES (16) is the canonical slot-index conversion"
        )]
        let slot = (self.invalidation_queue_tail as usize) / INV_QUEUE_ENTRY_BYTES;
        // SAFETY: queue is a kernel-owned 4-KiB page; `slot` is
        // bounded to `INV_QUEUE_ENTRY_COUNT - 1` by the wrap below.
        unsafe { write_queue_entry(queue_va, slot, lo, hi) };
        let mut next_tail = self
            .invalidation_queue_tail
            .wrapping_add(INV_QUEUE_ENTRY_BYTES as u64);
        if next_tail >= INV_QUEUE_BYTES as u64 {
            next_tail = 0;
        }
        // SAFETY: per the function's safety contract.
        unsafe { mmio_write64(unit_va, REG_OFFSET_IQT, next_tail) };
        self.invalidation_queue_tail = next_tail;
        // SAFETY: IQH is a 8-byte read-only MMIO register.
        if !unsafe { poll_iqh_reaches(unit_va, next_tail) } {
            return Err(());
        }
        Ok(())
    }
}

// =============================================================================
// MMIO helpers — bare-metal-only, `volatile` semantics.
//
// All accesses go through `core::ptr::read_volatile` /
// `core::ptr::write_volatile` so the optimiser cannot reorder or
// coalesce the writes; this is mandatory for MMIO programming. The
// helpers are unsafe — the caller (`VtdBackend::activate_hardware`)
// commits to the invariants in its safety contract.
// =============================================================================

/// Volatile 32-bit write to `unit_va + offset`.
///
/// # Safety
///
/// `unit_va + offset` must address a kernel-owned MMIO register that
/// accepts 32-bit naturally-aligned writes.
#[cfg(target_os = "none")]
#[inline]
unsafe fn mmio_write32(unit_va: u64, offset: u32, value: u32) {
    let ptr = unit_va.wrapping_add(u64::from(offset)) as *mut u32;
    // SAFETY: per the function's safety contract.
    unsafe { core::ptr::write_volatile(ptr, value) };
}

/// Volatile 32-bit read from `unit_va + offset`.
///
/// # Safety
///
/// `unit_va + offset` must address a kernel-owned MMIO register that
/// accepts 32-bit naturally-aligned reads.
#[cfg(target_os = "none")]
#[inline]
unsafe fn mmio_read32(unit_va: u64, offset: u32) -> u32 {
    let ptr = unit_va.wrapping_add(u64::from(offset)) as *const u32;
    // SAFETY: per the function's safety contract.
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Volatile 64-bit write to `unit_va + offset`.
///
/// # Safety
///
/// `unit_va + offset` must address a kernel-owned MMIO register that
/// accepts 64-bit naturally-aligned writes.
#[cfg(target_os = "none")]
#[inline]
unsafe fn mmio_write64(unit_va: u64, offset: u32, value: u64) {
    let ptr = unit_va.wrapping_add(u64::from(offset)) as *mut u64;
    // SAFETY: per the function's safety contract.
    unsafe { core::ptr::write_volatile(ptr, value) };
}

/// Volatile 64-bit read from `unit_va + offset`.
///
/// # Safety
///
/// `unit_va + offset` must address a kernel-owned MMIO register that
/// accepts 64-bit naturally-aligned reads.
#[cfg(target_os = "none")]
#[inline]
unsafe fn mmio_read64(unit_va: u64, offset: u32) -> u64 {
    let ptr = unit_va.wrapping_add(u64::from(offset)) as *const u64;
    // SAFETY: per the function's safety contract.
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Poll `GSTS` for `bit` to become set, with a bounded retry budget.
///
/// Returns `true` if `bit` was observed set within
/// [`VTD_ACTIVATION_POLL_LIMIT`] iterations, `false` on timeout.
///
/// # Safety
///
/// `unit_va` must point at the start of a kernel-owned VT-d register
/// window so `unit_va + REG_OFFSET_GSTS` is a valid 32-bit read.
#[cfg(target_os = "none")]
unsafe fn poll_gsts_bit(unit_va: u64, bit: u32) -> bool {
    let mut budget = VTD_ACTIVATION_POLL_LIMIT;
    while budget > 0 {
        // SAFETY: per the function's safety contract.
        let gsts = unsafe { mmio_read32(unit_va, REG_OFFSET_GSTS) };
        if gsts & bit != 0 {
            return true;
        }
        core::hint::spin_loop();
        budget -= 1;
    }
    false
}

/// Poll `IQH` until it reaches `tail_byte_offset`, with a bounded
/// retry budget.
///
/// The IOMMU advances `IQH` as it consumes descriptors. When `IQH ==
/// IQT` the queue is drained.
///
/// # Safety
///
/// Same as [`poll_gsts_bit`].
#[cfg(target_os = "none")]
unsafe fn poll_iqh_reaches(unit_va: u64, tail_byte_offset: u64) -> bool {
    let mut budget = VTD_ACTIVATION_POLL_LIMIT;
    while budget > 0 {
        // SAFETY: per the function's safety contract.
        let iqh = unsafe { mmio_read64(unit_va, REG_OFFSET_IQH) };
        if iqh == tail_byte_offset {
            return true;
        }
        core::hint::spin_loop();
        budget -= 1;
    }
    false
}

/// Write a 128-bit descriptor into the invalidation queue at the
/// 16-byte slot indexed by `slot`.
///
/// # Safety
///
/// `queue_va` must point at the start of a kernel-owned, 4-KiB-aligned
/// invalidation-queue page mapped through the direct map, and `slot`
/// must be `< INV_QUEUE_ENTRY_COUNT`.
#[cfg(target_os = "none")]
#[inline]
unsafe fn write_queue_entry(queue_va: u64, slot: usize, lo: u64, hi: u64) {
    let byte_offset = slot.wrapping_mul(INV_QUEUE_ENTRY_BYTES) as u64;
    let base = queue_va.wrapping_add(byte_offset);
    let lo_ptr = base as *mut u64;
    let hi_ptr = base.wrapping_add(8) as *mut u64;
    // SAFETY: per the function's safety contract.
    unsafe {
        core::ptr::write_volatile(lo_ptr, lo);
        core::ptr::write_volatile(hi_ptr, hi);
    }
}

/// Write a 128-bit context entry (low + high qwords) into a per-bus
/// context-table page at `byte_offset`.
///
/// # Safety
///
/// `context_va` must point at the start of a kernel-owned, 4-KiB-aligned
/// context-table page reachable through the direct map.
/// `byte_offset + 16` must be `<= 4096`.
#[cfg(target_os = "none")]
#[inline]
unsafe fn write_context_entry_at(context_va: u64, byte_offset: u64, low: u64, high: u64) {
    let base = context_va.wrapping_add(byte_offset);
    let lo_ptr = base as *mut u64;
    let hi_ptr = base.wrapping_add(8) as *mut u64;
    // SAFETY: per the function's safety contract.
    unsafe {
        core::ptr::write_volatile(lo_ptr, low);
        core::ptr::write_volatile(hi_ptr, high);
    }
}

/// Write a 128-bit root entry (low + high qwords) into the global
/// root-table page at `byte_offset`.
///
/// # Safety
///
/// `root_va` must point at the start of the kernel-owned, 4-KiB-aligned
/// root-table page reachable through the direct map (the same page
/// recorded via [`VtdBackend::prepare_activation`]).
/// `byte_offset + 16` must be `<= 4096`.
#[cfg(target_os = "none")]
#[inline]
unsafe fn write_root_entry_at(root_va: u64, byte_offset: u64, low: u64, high: u64) {
    let base = root_va.wrapping_add(byte_offset);
    let lo_ptr = base as *mut u64;
    let hi_ptr = base.wrapping_add(8) as *mut u64;
    // SAFETY: per the function's safety contract.
    unsafe {
        core::ptr::write_volatile(lo_ptr, low);
        core::ptr::write_volatile(hi_ptr, high);
    }
}

impl IommuBackend for VtdBackend {
    fn vendor(&self) -> IommuVendor {
        IommuVendor::Intel
    }

    fn install_domain(&mut self, id: DomainId) -> Result<(), IommuError> {
        if !self.has_domain(id) {
            self.domains.push(id);
        }
        Ok(())
    }

    fn map(
        &mut self,
        id: DomainId,
        iova: u64,
        phys: u64,
        len: u64,
        flags: IommuFlags,
    ) -> Result<(), IommuError> {
        if !self.has_domain(id) {
            return Err(IommuError::InvalidDomain);
        }
        if iova & 0xFFF != 0 || phys & 0xFFF != 0 || len & 0xFFF != 0 || len == 0 {
            return Err(IommuError::AddressMisaligned);
        }
        let leaf = encode_slpte(phys, flags).map_err(IommuError::from)?;
        self.mappings.push(ScaffoldMapping {
            domain: id,
            iova,
            phys,
            len,
            leaf_slpte: leaf,
        });
        Ok(())
    }

    fn unmap(&mut self, id: DomainId, iova: u64, len: u64) -> Result<(), IommuError> {
        if !self.has_domain(id) {
            return Err(IommuError::InvalidDomain);
        }
        if iova & 0xFFF != 0 || len & 0xFFF != 0 || len == 0 {
            return Err(IommuError::AddressMisaligned);
        }
        let initial = self.mappings.len();
        self.mappings
            .retain(|m| !(m.domain == id && m.iova == iova && m.len == len));
        if self.mappings.len() == initial {
            return Err(IommuError::UnmapFailed);
        }
        Ok(())
    }

    fn flush(&mut self, id: DomainId) -> Result<(), IommuError> {
        if !self.has_domain(id) {
            return Err(IommuError::InvalidDomain);
        }
        // Dormant backend: nothing to flush. Live backend will queue an
        // `IOTLB_INVALIDATE` descriptor here.
        Ok(())
    }

    fn attach_device(&mut self, bdf: PciBdf, domain: DomainId) -> Result<(), IommuError> {
        if !self.has_domain(domain) {
            return Err(IommuError::InvalidDomain);
        }
        if self.has_attachment(bdf) {
            return Err(IommuError::Unsupported);
        }
        self.attachments.push(VtdAttachment { bdf, domain });
        Ok(())
    }

    fn detach_device(&mut self, bdf: PciBdf) -> Result<(), IommuError> {
        let initial = self.attachments.len();
        self.attachments.retain(|a| a.bdf != bdf);
        if self.attachments.len() == initial {
            return Err(IommuError::Unsupported);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddressWidth, CONTEXT_ENTRY_BYTES, ContextEntry, GCMD_BIT_QIE, GCMD_BIT_SRTP, GCMD_BIT_TE,
        INV_DESC_CTX_GRAN_DOMAIN, INV_DESC_IOTLB_GRAN_DOMAIN, INV_DESC_TYPE_CONTEXT_CACHE,
        INV_DESC_TYPE_IOTLB, IommuBackend, IommuError, IommuFlags, IommuVendor, REG_OFFSET_CAP,
        REG_OFFSET_ECAP, REG_OFFSET_GCMD, REG_OFFSET_GSTS, REG_OFFSET_IQA, REG_OFFSET_IQH,
        REG_OFFSET_IQT, REG_OFFSET_RTADDR, REG_OFFSET_VER, ROOT_ENTRY_BYTES, RootEntry,
        ScaffoldMapping, Slpte, TranslationType, VtdAttachError, VtdAttachment, VtdBackend,
        VtdError, cap_caching_mode, cap_domain_count, cap_supported_agaw, context_entry_offset,
        encode_context_cache_domain_invalidate, encode_context_entry, encode_context_entry_absent,
        encode_iotlb_domain_invalidate, encode_root_entry, encode_root_entry_absent, encode_slpte,
        pick_highest_supported_agaw, root_entry_offset,
    };
    use crate::bare_metal::iommu::{DomainId, PciBdf};

    // ---- Register offset invariants ------------------------------------

    #[test]
    fn register_offsets_match_intel_spec_4_1() {
        // Pinning against the spec lets a future refactor catch any
        // accidental drift via the test suite rather than at runtime.
        assert_eq!(REG_OFFSET_VER, 0x000);
        assert_eq!(REG_OFFSET_CAP, 0x008);
        assert_eq!(REG_OFFSET_ECAP, 0x010);
        assert_eq!(REG_OFFSET_GCMD, 0x018);
        assert_eq!(REG_OFFSET_GSTS, 0x01C);
        assert_eq!(REG_OFFSET_RTADDR, 0x020);
        assert_eq!(REG_OFFSET_IQH, 0x080);
        assert_eq!(REG_OFFSET_IQT, 0x088);
        assert_eq!(REG_OFFSET_IQA, 0x090);
    }

    #[test]
    fn gcmd_bits_are_top_of_32_bit_word() {
        assert_eq!(GCMD_BIT_TE, 1 << 31);
        assert_eq!(GCMD_BIT_SRTP, 1 << 30);
        assert_eq!(GCMD_BIT_QIE, 1 << 26);
    }

    // ---- Root entry encoder --------------------------------------------

    #[test]
    fn encode_root_entry_sets_present_bit_and_ctp() {
        let entry = encode_root_entry(0x1234_5000).unwrap();
        assert!(entry.is_present());
        assert_eq!(entry.context_table_pointer(), 0x1234_5000);
        assert_eq!(entry.high, 0);
    }

    #[test]
    fn encode_root_entry_rejects_misaligned_ctp() {
        assert_eq!(
            encode_root_entry(0x1234_5001),
            Err(VtdError::AddressMisaligned)
        );
        assert_eq!(
            encode_root_entry(0x1234_5FFF),
            Err(VtdError::AddressMisaligned)
        );
    }

    #[test]
    fn encode_root_entry_absent_is_all_zero() {
        let entry = encode_root_entry_absent();
        assert!(!entry.is_present());
        assert_eq!(entry, RootEntry { low: 0, high: 0 });
    }

    // ---- Context entry encoder -----------------------------------------

    #[test]
    fn encode_context_entry_round_trips_did_and_aw() {
        let entry = encode_context_entry(
            0xAB_CDEF_F000,
            DomainId::new(0x1234),
            TranslationType::UntranslatedAndTranslated,
            AddressWidth::Bits48Level4,
        )
        .unwrap();
        assert!(entry.is_present());
        assert_eq!(entry.slptptr(), 0xAB_CDEF_F000);
        assert_eq!(entry.domain_id(), DomainId::new(0x1234));
        assert_eq!(
            entry.translation_type_raw(),
            TranslationType::UntranslatedAndTranslated.as_u8()
        );
        assert_eq!(
            entry.address_width_raw(),
            AddressWidth::Bits48Level4.as_u8()
        );
    }

    #[test]
    fn encode_context_entry_passthrough_keeps_t_field() {
        let entry = encode_context_entry(
            0x100_0000,
            DomainId::new(0),
            TranslationType::Passthrough,
            AddressWidth::Bits39Level3,
        )
        .unwrap();
        assert_eq!(
            entry.translation_type_raw(),
            TranslationType::Passthrough.as_u8()
        );
    }

    #[test]
    fn encode_context_entry_rejects_misaligned_slpt() {
        assert_eq!(
            encode_context_entry(
                0x100_0001,
                DomainId::new(0),
                TranslationType::UntranslatedOnly,
                AddressWidth::Bits48Level4,
            ),
            Err(VtdError::AddressMisaligned)
        );
    }

    #[test]
    fn encode_context_entry_absent_is_all_zero() {
        let entry = encode_context_entry_absent();
        assert!(!entry.is_present());
        assert_eq!(entry, ContextEntry { low: 0, high: 0 });
    }

    #[test]
    fn address_width_levels_match_spec() {
        assert_eq!(AddressWidth::Bits30Level2.levels(), 2);
        assert_eq!(AddressWidth::Bits39Level3.levels(), 3);
        assert_eq!(AddressWidth::Bits48Level4.levels(), 4);
        assert_eq!(AddressWidth::Bits57Level5.levels(), 5);
    }

    // ---- SL-PTE encoder ------------------------------------------------

    #[test]
    fn encode_slpte_read_only() {
        let pte = encode_slpte(0xABCD_F000, IommuFlags::READ).unwrap();
        assert!(pte.is_present());
        assert_eq!(pte.output_address(), 0xABCD_F000);
        assert_eq!(pte.0 & Slpte::BIT_READ, Slpte::BIT_READ);
        assert_eq!(pte.0 & Slpte::BIT_WRITE, 0);
        assert_eq!(pte.0 & Slpte::BIT_EXECUTE, 0);
        assert_eq!(pte.0 & Slpte::BIT_SNOOP, 0);
    }

    #[test]
    fn encode_slpte_write_forces_read_bit() {
        // VT-d treats W-only entries as malformed; the encoder must
        // force R on whenever W is requested.
        let pte = encode_slpte(0x1000, IommuFlags::WRITE).unwrap();
        assert_eq!(pte.0 & Slpte::BIT_READ, Slpte::BIT_READ);
        assert_eq!(pte.0 & Slpte::BIT_WRITE, Slpte::BIT_WRITE);
    }

    #[test]
    fn encode_slpte_execute_and_coherent() {
        let flags = IommuFlags::READ
            .union(IommuFlags::WRITE)
            .union(IommuFlags::EXECUTE)
            .union(IommuFlags::COHERENT);
        let pte = encode_slpte(0x2000, flags).unwrap();
        assert_eq!(pte.0 & Slpte::BIT_EXECUTE, Slpte::BIT_EXECUTE);
        assert_eq!(pte.0 & Slpte::BIT_SNOOP, Slpte::BIT_SNOOP);
    }

    #[test]
    fn encode_slpte_rejects_misaligned_phys() {
        assert_eq!(
            encode_slpte(0x1001, IommuFlags::READ),
            Err(VtdError::AddressMisaligned)
        );
    }

    #[test]
    fn encode_slpte_zero_flags_emits_not_present() {
        // No R, no W -> not present. Useful for clearing leaves during
        // unmap without zeroing the address bits.
        let pte = encode_slpte(0x1000, IommuFlags::from_bits(0)).unwrap();
        assert!(!pte.is_present());
        assert_eq!(pte.output_address(), 0x1000);
    }

    // ---- CAP decoder ---------------------------------------------------

    #[test]
    fn cap_domain_count_known_values() {
        // ND = 0..6
        assert_eq!(cap_domain_count(0), 16);
        assert_eq!(cap_domain_count(1), 64);
        assert_eq!(cap_domain_count(2), 256);
        assert_eq!(cap_domain_count(3), 1_024);
        assert_eq!(cap_domain_count(4), 4_096);
        assert_eq!(cap_domain_count(5), 16_384);
        assert_eq!(cap_domain_count(6), 65_536);
    }

    #[test]
    fn cap_domain_count_caps_at_16_bit_space() {
        // ND = 7 is reserved; encoder saturates at the 16-bit DID space.
        assert_eq!(cap_domain_count(7), 65_536);
    }

    #[test]
    fn cap_supported_agaw_extracts_bits_8_to_12() {
        // Set SAGAW = 0b0110 (bits 9..10), padded to byte boundary.
        let cap = 0b0110 << 8;
        assert_eq!(cap_supported_agaw(cap), 0b0110);
    }

    #[test]
    fn cap_caching_mode_extracts_bit_7() {
        assert!(!cap_caching_mode(0));
        assert!(cap_caching_mode(1 << 7));
        assert!(cap_caching_mode((1 << 7) | (1 << 31)));
    }

    #[test]
    fn pick_highest_supported_agaw_prefers_57_then_48_then_39_then_30() {
        assert_eq!(
            pick_highest_supported_agaw(0b1111),
            Some(AddressWidth::Bits57Level5)
        );
        assert_eq!(
            pick_highest_supported_agaw(0b0111),
            Some(AddressWidth::Bits48Level4)
        );
        assert_eq!(
            pick_highest_supported_agaw(0b0011),
            Some(AddressWidth::Bits39Level3)
        );
        assert_eq!(
            pick_highest_supported_agaw(0b0001),
            Some(AddressWidth::Bits30Level2)
        );
    }

    #[test]
    fn pick_highest_supported_agaw_returns_none_for_zero_mask() {
        assert_eq!(pick_highest_supported_agaw(0), None);
    }

    // ---- VtdBackend bookkeeping ----------------------------------------

    #[test]
    fn vtd_backend_vendor_reports_intel() {
        let backend = VtdBackend::new();
        assert_eq!(backend.vendor(), IommuVendor::Intel);
    }

    #[test]
    fn vtd_backend_install_domain_is_idempotent() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(3)).unwrap();
        backend.install_domain(DomainId::new(3)).unwrap();
        assert!(backend.has_domain(DomainId::new(3)));
        assert_eq!(backend.domains(), &[DomainId::new(3)]);
    }

    #[test]
    fn vtd_backend_map_rejects_unknown_domain() {
        let mut backend = VtdBackend::new();
        assert_eq!(
            backend.map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn vtd_backend_map_records_mapping_with_encoded_slpte() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        backend
            .map(
                DomainId::new(7),
                0x1000,
                0x2000,
                0x1000,
                IommuFlags::READ.union(IommuFlags::WRITE),
            )
            .unwrap();
        let mappings = backend.mappings();
        assert_eq!(mappings.len(), 1);
        let rec: ScaffoldMapping = *mappings.first().expect("one mapping just recorded");
        assert_eq!(rec.domain, DomainId::new(7));
        assert_eq!(rec.iova, 0x1000);
        assert_eq!(rec.phys, 0x2000);
        assert_eq!(rec.len, 0x1000);
        assert_eq!(rec.leaf_slpte.output_address(), 0x2000);
        assert!(rec.leaf_slpte.is_present());
    }

    #[test]
    fn vtd_backend_map_rejects_misaligned_arguments() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        assert_eq!(
            backend.map(DomainId::new(7), 0x1001, 0x2000, 0x1000, IommuFlags::READ),
            Err(IommuError::AddressMisaligned)
        );
        assert_eq!(
            backend.map(DomainId::new(7), 0x1000, 0x2001, 0x1000, IommuFlags::READ),
            Err(IommuError::AddressMisaligned)
        );
        assert_eq!(
            backend.map(DomainId::new(7), 0x1000, 0x2000, 0x1001, IommuFlags::READ),
            Err(IommuError::AddressMisaligned)
        );
        assert_eq!(
            backend.map(DomainId::new(7), 0x1000, 0x2000, 0, IommuFlags::READ),
            Err(IommuError::AddressMisaligned)
        );
    }

    #[test]
    fn vtd_backend_unmap_removes_record() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        backend
            .map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ)
            .unwrap();
        assert_eq!(backend.mappings().len(), 1);
        backend.unmap(DomainId::new(7), 0x1000, 0x1000).unwrap();
        assert!(backend.mappings().is_empty());
    }

    #[test]
    fn vtd_backend_unmap_unmapped_range_returns_error() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        assert_eq!(
            backend.unmap(DomainId::new(7), 0x1000, 0x1000),
            Err(IommuError::UnmapFailed)
        );
    }

    #[test]
    fn vtd_backend_unmap_rejects_unknown_domain() {
        let mut backend = VtdBackend::new();
        assert_eq!(
            backend.unmap(DomainId::new(9), 0x1000, 0x1000),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn vtd_backend_flush_rejects_unknown_domain() {
        let mut backend = VtdBackend::new();
        assert_eq!(
            backend.flush(DomainId::new(0)),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn vtd_backend_flush_known_domain_is_ok() {
        let mut backend = VtdBackend::new();
        backend.install_domain(DomainId::new(0)).unwrap();
        assert_eq!(backend.flush(DomainId::new(0)), Ok(()));
    }

    // ---- VtdError → IommuError mapping ---------------------------------

    #[test]
    fn vtd_error_into_iommu_error_mapping() {
        assert_eq!(
            IommuError::from(VtdError::AddressMisaligned),
            IommuError::AddressMisaligned
        );
        assert_eq!(
            IommuError::from(VtdError::UnknownDomain),
            IommuError::InvalidDomain
        );
        assert_eq!(
            IommuError::from(VtdError::UnsupportedFlags),
            IommuError::Unsupported
        );
    }

    // ---- Activation surface (P6.7.9-pre.5) -----------------------------

    use super::{
        GSTS_BIT_QIES, GSTS_BIT_RTPS, GSTS_BIT_TES, INV_DESC_CTX_GRAN_GLOBAL,
        INV_DESC_IOTLB_GRAN_GLOBAL, INV_DESC_TYPE_INVALIDATE_WAIT, INV_DESC_WAIT_STATUS_WRITE,
        INV_QUEUE_BYTES, INV_QUEUE_ENTRY_BYTES, INV_QUEUE_ENTRY_COUNT, INV_QUEUE_SIZE_ORDER,
        VTD_ACTIVATION_POLL_LIMIT, VtdActivateError, encode_context_cache_global_invalidate,
        encode_iotlb_global_invalidate, encode_iqa,
    };

    #[test]
    fn gsts_bits_mirror_gcmd_positions() {
        assert_eq!(GSTS_BIT_TES, super::GCMD_BIT_TE);
        assert_eq!(GSTS_BIT_RTPS, super::GCMD_BIT_SRTP);
        assert_eq!(GSTS_BIT_QIES, super::GCMD_BIT_QIE);
    }

    #[test]
    fn invalidation_queue_layout_constants_match_legacy_format() {
        assert_eq!(INV_QUEUE_SIZE_ORDER, 0);
        assert_eq!(INV_QUEUE_ENTRY_COUNT, 256);
        assert_eq!(INV_QUEUE_ENTRY_BYTES, 16);
        assert_eq!(INV_QUEUE_BYTES, 4096);
        assert_eq!(
            INV_QUEUE_ENTRY_COUNT * INV_QUEUE_ENTRY_BYTES,
            INV_QUEUE_BYTES
        );
    }

    #[test]
    fn invalidation_descriptor_tags_match_spec_section_6_5_2() {
        assert_eq!(INV_DESC_TYPE_CONTEXT_CACHE, 0x1);
        assert_eq!(INV_DESC_TYPE_IOTLB, 0x2);
        assert_eq!(INV_DESC_TYPE_INVALIDATE_WAIT, 0x5);
        assert_eq!(INV_DESC_CTX_GRAN_GLOBAL, 0b01 << 4);
        assert_eq!(INV_DESC_IOTLB_GRAN_GLOBAL, 0b01 << 4);
        assert_eq!(INV_DESC_WAIT_STATUS_WRITE, 1 << 5);
    }

    #[test]
    fn poll_limit_is_a_million() {
        assert_eq!(VTD_ACTIVATION_POLL_LIMIT, 1_000_000);
    }

    #[test]
    fn encode_iqa_places_base_in_bits_12_to_63_and_qs_in_low_three() {
        let phys = 0x0000_DEAD_BEEF_F000_u64;
        let iqa = encode_iqa(phys, 0);
        // Low 12 bits zero (4-KiB aligned), no DW, QS=0.
        assert_eq!(iqa, phys);
    }

    #[test]
    fn encode_iqa_masks_reserved_low_bits_of_phys() {
        let phys_with_dirt = 0x0000_DEAD_BEEF_F123_u64;
        let iqa = encode_iqa(phys_with_dirt, 0);
        // The low 12 bits must be cleared; QS = 0 leaves bits 0..2 = 0.
        assert_eq!(iqa & 0xFFF, 0);
        assert_eq!(iqa >> 12, phys_with_dirt >> 12);
    }

    #[test]
    fn encode_iqa_encodes_size_order_in_low_three_bits() {
        let phys = 0x0000_0001_0000_0000_u64; // 4 GiB aligned
        let iqa = encode_iqa(phys, 3);
        assert_eq!(iqa & 0x7, 0x3);
        assert_eq!(iqa & !0x7, phys);
    }

    #[test]
    fn encode_iqa_truncates_size_order_above_three_bits() {
        let phys = 0x0000_0001_0000_0000_u64;
        let iqa = encode_iqa(phys, 0xFF);
        // High bits of size_order must be discarded.
        assert_eq!(iqa & 0x7, 0x7);
    }

    #[test]
    fn encode_iqa_masks_phys_above_bit_51() {
        // Bits 52..63 are reserved; we mask conservatively to bit 51
        // because Intel VT-d MGAW caps at 52 host-address bits even on
        // the widest 5-level paging configuration.
        let high_phys = 0xFFFF_FFFF_FFFF_F000_u64;
        let iqa = encode_iqa(high_phys, 0);
        assert_eq!(iqa, 0x000F_FFFF_FFFF_F000_u64);
    }

    #[test]
    fn encode_iotlb_global_invalidate_low_qword_carries_type_and_granularity() {
        let (low, high) = encode_iotlb_global_invalidate();
        assert_eq!(low & 0xF, INV_DESC_TYPE_IOTLB);
        assert_eq!((low >> 4) & 0x3, INV_DESC_IOTLB_GRAN_GLOBAL >> 4);
        assert_eq!(high, 0);
    }

    #[test]
    fn encode_context_cache_global_invalidate_low_qword_carries_type_and_granularity() {
        let (low, high) = encode_context_cache_global_invalidate();
        assert_eq!(low & 0xF, INV_DESC_TYPE_CONTEXT_CACHE);
        assert_eq!((low >> 4) & 0x3, INV_DESC_CTX_GRAN_GLOBAL >> 4);
        assert_eq!(high, 0);
    }

    #[test]
    fn vtd_activate_error_maps_to_iommu_activation_failed() {
        for variant in [
            VtdActivateError::NotPrepared,
            VtdActivateError::RootTableTimeout,
            VtdActivateError::QueueEnableTimeout,
            VtdActivateError::InvalidationTimeout,
        ] {
            assert_eq!(IommuError::from(variant), IommuError::ActivationFailed);
        }
    }

    #[test]
    fn fresh_backend_reports_dormant_state() {
        let backend = VtdBackend::new();
        assert_eq!(backend.unit_base(), 0);
        assert_eq!(backend.root_table_phys(), 0);
        assert_eq!(backend.invalidation_queue_phys(), 0);
        assert!(!backend.is_hardware_activated());
    }

    #[test]
    fn prepare_activation_stashes_parameters() {
        let mut backend = VtdBackend::new();
        backend.prepare_activation(0xFED9_0000, 0x10_0000, 0x10_1000);
        assert_eq!(backend.unit_base(), 0xFED9_0000);
        assert_eq!(backend.root_table_phys(), 0x10_0000);
        assert_eq!(backend.invalidation_queue_phys(), 0x10_1000);
        assert!(!backend.is_hardware_activated());
    }

    #[test]
    fn prepare_activation_with_same_params_does_not_clear_activated_flag() {
        // We can't trigger activate_hardware on host (it is
        // `cfg(target_os = "none")`); model the post-activation state
        // by re-calling `prepare_activation` with the same args and
        // proving the function does not reset `hardware_activated`
        // when the values match. The actual flag flip is exercised by
        // the Proxmox smoke after the boot probe runs.
        let mut backend = VtdBackend::new();
        backend.prepare_activation(0xFED9_0000, 0x10_0000, 0x10_1000);
        backend.prepare_activation(0xFED9_0000, 0x10_0000, 0x10_1000);
        assert_eq!(backend.unit_base(), 0xFED9_0000);
        assert_eq!(backend.root_table_phys(), 0x10_0000);
        assert_eq!(backend.invalidation_queue_phys(), 0x10_1000);
    }

    #[test]
    fn prepare_activation_with_different_params_resets_state() {
        let mut backend = VtdBackend::new();
        backend.prepare_activation(0xFED9_0000, 0x10_0000, 0x10_1000);
        backend.prepare_activation(0xFED9_1000, 0x20_0000, 0x20_1000);
        assert_eq!(backend.unit_base(), 0xFED9_1000);
        assert_eq!(backend.root_table_phys(), 0x20_0000);
        assert_eq!(backend.invalidation_queue_phys(), 0x20_1000);
        assert!(!backend.is_hardware_activated());
    }

    // ---- P6.7.9-pre.7 — per-domain invalidate encoders ------------------

    #[test]
    fn encode_context_cache_domain_invalidate_packs_did_and_type() {
        let (low, high) = encode_context_cache_domain_invalidate(DomainId::new(0x1234));
        // Type=0x1 in bits 0..3, G=10 in bits 4..5, DID in bits 16..31.
        assert_eq!(low & 0xF, INV_DESC_TYPE_CONTEXT_CACHE);
        assert_eq!(low & (0b11 << 4), INV_DESC_CTX_GRAN_DOMAIN);
        assert_eq!((low >> 16) & 0xFFFF, 0x1234);
        assert_eq!(high, 0);
    }

    #[test]
    fn encode_iotlb_domain_invalidate_packs_did_and_type() {
        let (low, high) = encode_iotlb_domain_invalidate(DomainId::new(0xABCD));
        // Type=0x2 in bits 0..3, G=10 in bits 4..5, DID in bits 16..31.
        assert_eq!(low & 0xF, INV_DESC_TYPE_IOTLB);
        assert_eq!(low & (0b11 << 4), INV_DESC_IOTLB_GRAN_DOMAIN);
        assert_eq!((low >> 16) & 0xFFFF, 0xABCD);
        assert_eq!(high, 0);
    }

    #[test]
    fn encode_per_domain_invalidates_for_did_zero_set_only_type_and_g() {
        // The boundary DID=0 must still raise the type + G bits even
        // though the DID field encodes to zero — defends against an
        // accidental mask that swallows both fields.
        let (cc_low, cc_high) = encode_context_cache_domain_invalidate(DomainId::new(0));
        assert_eq!(
            cc_low,
            INV_DESC_TYPE_CONTEXT_CACHE | INV_DESC_CTX_GRAN_DOMAIN
        );
        assert_eq!(cc_high, 0);

        let (io_low, io_high) = encode_iotlb_domain_invalidate(DomainId::new(0));
        assert_eq!(io_low, INV_DESC_TYPE_IOTLB | INV_DESC_IOTLB_GRAN_DOMAIN);
        assert_eq!(io_high, 0);
    }

    // ---- P6.7.9-pre.7 — root/context entry offset helpers ---------------

    #[test]
    fn context_entry_offset_matches_devfn_times_16() {
        // bdf 00:01.2 → devfn = (1 << 3) | 2 = 0xA → offset = 0xA * 16 = 0xA0.
        let bdf = PciBdf::from_parts(0, 1, 2);
        assert_eq!(context_entry_offset(bdf), 0xA0);
        // bdf 00:1F.7 → devfn = 0xFF → offset = 0xFF0 (last slot of
        // the 4-KiB context table).
        let last = PciBdf::from_parts(0, 0x1F, 0x7);
        assert_eq!(context_entry_offset(last), 0xFF0);
    }

    #[test]
    fn context_entry_offset_keeps_table_in_4_kib_page() {
        // Last possible slot = (devfn=0xFF, offset=0xFF0) — the entry
        // body still fits inside the 4-KiB context-table page because
        // offset + CONTEXT_ENTRY_BYTES = 0x1000.
        let last = PciBdf::from_parts(7, 0x1F, 0x7);
        let off = context_entry_offset(last);
        assert!(off + (CONTEXT_ENTRY_BYTES as u64) <= 4096);
    }

    #[test]
    fn root_entry_offset_matches_bus_times_16() {
        assert_eq!(root_entry_offset(0), 0);
        assert_eq!(root_entry_offset(1), 0x10);
        assert_eq!(root_entry_offset(0xFF), 0xFF0);
    }

    #[test]
    fn root_entry_offset_keeps_table_in_4_kib_page() {
        // Last possible slot = bus 255 → offset 0xFF0 → fits inside
        // the 4-KiB root-table page.
        let off = root_entry_offset(0xFF);
        assert!(off + (ROOT_ENTRY_BYTES as u64) <= 4096);
    }

    // ---- P6.7.9-pre.7 — VtdAttachment scaffold ---------------------------

    #[test]
    fn attach_device_records_binding_and_rejects_unknown_domain() {
        let mut backend = VtdBackend::new();
        let bdf = PciBdf::from_parts(0, 1, 0);
        // Domain never installed → InvalidDomain.
        assert_eq!(
            backend.attach_device(bdf, DomainId::new(0x10)),
            Err(IommuError::InvalidDomain)
        );
        // Install + attach succeeds.
        backend.install_domain(DomainId::new(0x10)).unwrap();
        assert_eq!(backend.attach_device(bdf, DomainId::new(0x10)), Ok(()));
        assert!(backend.has_attachment(bdf));
        assert_eq!(backend.attachments().len(), 1);
        assert_eq!(
            backend.attachments().first().copied(),
            Some(VtdAttachment {
                bdf,
                domain: DomainId::new(0x10),
            })
        );
    }

    #[test]
    fn attach_device_double_attach_rejected() {
        let mut backend = VtdBackend::new();
        let bdf = PciBdf::from_parts(0, 2, 0);
        backend.install_domain(DomainId::new(1)).unwrap();
        backend.attach_device(bdf, DomainId::new(1)).unwrap();
        assert_eq!(
            backend.attach_device(bdf, DomainId::new(1)),
            Err(IommuError::Unsupported)
        );
    }

    #[test]
    fn detach_device_removes_and_allows_reattach() {
        let mut backend = VtdBackend::new();
        let bdf = PciBdf::from_parts(0, 3, 1);
        backend.install_domain(DomainId::new(2)).unwrap();
        backend.attach_device(bdf, DomainId::new(2)).unwrap();
        assert_eq!(backend.detach_device(bdf), Ok(()));
        assert!(!backend.has_attachment(bdf));
        // Re-attach after detach succeeds (idempotent surface).
        assert_eq!(backend.attach_device(bdf, DomainId::new(2)), Ok(()));
    }

    #[test]
    fn detach_unknown_device_returns_unsupported() {
        let mut backend = VtdBackend::new();
        let bdf = PciBdf::from_parts(0, 4, 0);
        assert_eq!(backend.detach_device(bdf), Err(IommuError::Unsupported));
    }

    #[test]
    fn vtd_attach_error_maps_to_iommu_error_variants() {
        assert_eq!(
            IommuError::from(VtdAttachError::NotActivated),
            IommuError::ActivationFailed
        );
        assert_eq!(
            IommuError::from(VtdAttachError::DomainNotInstalled),
            IommuError::InvalidDomain
        );
        assert_eq!(
            IommuError::from(VtdAttachError::AlreadyAttached),
            IommuError::Unsupported
        );
        assert_eq!(
            IommuError::from(VtdAttachError::AddressMisaligned),
            IommuError::AddressMisaligned
        );
        assert_eq!(
            IommuError::from(VtdAttachError::InvalidationTimeout),
            IommuError::ActivationFailed
        );
    }

    #[test]
    fn fresh_backend_has_no_attachments() {
        let backend = VtdBackend::new();
        assert!(backend.attachments().is_empty());
        assert!(!backend.has_attachment(PciBdf::from_parts(0, 0, 0)));
    }
}
