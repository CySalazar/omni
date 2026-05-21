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

use super::{DomainId, IommuBackend, IommuError, IommuFlags, IommuVendor};

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
        }
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
}

#[cfg(test)]
mod tests {
    use super::{
        AddressWidth, ContextEntry, GCMD_BIT_QIE, GCMD_BIT_SRTP, GCMD_BIT_TE, IommuBackend,
        IommuError, IommuFlags, IommuVendor, REG_OFFSET_CAP, REG_OFFSET_ECAP, REG_OFFSET_GCMD,
        REG_OFFSET_GSTS, REG_OFFSET_IQA, REG_OFFSET_IQH, REG_OFFSET_IQT, REG_OFFSET_RTADDR,
        REG_OFFSET_VER, RootEntry, ScaffoldMapping, Slpte, TranslationType, VtdBackend, VtdError,
        cap_caching_mode, cap_domain_count, cap_supported_agaw, encode_context_entry,
        encode_context_entry_absent, encode_root_entry, encode_root_entry_absent, encode_slpte,
        pick_highest_supported_agaw,
    };
    use crate::bare_metal::iommu::DomainId;

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
}
