//! AMD-Vi backend scaffold (P6.7.9-pre.3).
//!
//! ## Scope
//!
//! Sibling of [`super::vtd`] for AMD platforms: this module lands the
//! **dormant scaffold** for the AMD I/O Virtualization Technology
//! backend that will eventually replace the [`super::PassthroughBackend`]
//! when `bare_metal::iommu::iommu_vendor()` reports
//! [`super::IommuVendor::Amd`]. The scaffold pins four pure-function
//! surfaces — register offsets, Device Table Entry (DTE) encoder, I/O
//! Page Table Entry (PTE) encoder, and Extended Feature Register (EFR)
//! field decoders — plus a host-testable [`AmdViBackend`] struct that
//! tracks domains in an internal table without writing a single MMIO
//! byte.
//!
//! Until P6.7.9-pre.4 (DMA-Map vendor switch) wires it in, no caller
//! reaches this backend at runtime — the kernel `dma_map_handlers`
//! continues to use [`super::PassthroughBackend`]. The scaffold lives
//! in the workspace so the QEMU smoke (`-machine q35,iommu=amd`) can
//! assert the vendor selector + ACPI parser interaction without any
//! silicon side effect.
//!
//! ## Why a scaffold and not the live backend?
//!
//! Each P6.7.9-pre.x slice keeps the auditable surface bounded:
//!
//! - **P6.7.9-pre.0:** parser + trait + passthrough.
//! - **P6.7.9-pre.1:** firmware-probe + vendor selector.
//! - **P6.7.9-pre.2:** Intel VT-d register-offset constants +
//!   data-structure encoders + dormant `VtdBackend`.
//! - **P6.7.9-pre.3 (this slice):** AMD-Vi register-offset constants +
//!   DTE / I/O-PTE encoders + dormant `AmdViBackend`.
//! - **P6.7.9-pre.4:** swap `dma_map_handlers` to consult
//!   `iommu_vendor()` and route through the now-live backends.
//!
//! Splitting the live register programming off keeps every PR's
//! `unsafe` surface auditable and lets the host test matrix exercise
//! the encoders before the live ring is opened.
//!
//! ## References
//!
//! - AMD I/O Virtualization Technology (IOMMU) Specification rev 3.10
//!   § 5 (IOMMU architecture), in particular § 5.2.2 (Device Table
//!   Entry), § 5.3.1 (I/O Page Table), § 5.5 (MMIO register layout),
//!   § 5.7 (Extended Feature Register).
//! - OIP-Driver-Framework-013 § S3 (capability scope + IOMMU
//!   semantics).

#![allow(
    clippy::module_name_repetitions,
    reason = "AmdViBackend / AmdViError / AmdViRegister share the AmdVi prefix by design — they are the public symbols of this submodule and the prefix prevents ambiguity with sibling VT-d / passthrough types"
)]

extern crate alloc;

use alloc::vec::Vec;

use super::{DomainId, IommuBackend, IommuError, IommuFlags, IommuVendor};

// =============================================================================
// Section 1 — AMD-Vi MMIO register offsets (AMD IOMMU spec rev 3.10 § 5.5).
//
// Offsets are byte-addressed against the per-IOMMU MMIO base
// discovered from the IVRS table's IVHD entry `base_address` field
// (see `super::ivrs::IvhdEntry::base_address`). The control + table
// pointer registers live in the first 4 KiB of the window; the queue
// head/tail registers live at offset 0x2000 in the second 4 KiB so
// they can be remapped uncached without disturbing the control plane.
// Constants are `pub` so the future live backend (P6.7.9-pre.4) and
// the host test suite can both reference the same single source of
// truth.
// =============================================================================

/// Device Table Base Address Register — 8 bytes at offset `0x00`.
///
/// Bits 0..51 carry the 4-KiB-aligned physical address of the device
/// table; bits 52..63 carry the table size encoding (`Size = 2^(n+12)`
/// bytes, where `n` is bits 0..8 of the register).
pub const REG_OFFSET_DEVICE_TABLE_BASE: u32 = 0x000;

/// Command Buffer Base Address Register — 8 bytes at offset `0x08`.
///
/// Bits 0..51 carry the 4-KiB-aligned physical base of the ring;
/// bits 56..59 encode log2 of the ring length.
pub const REG_OFFSET_COMMAND_BUFFER_BASE: u32 = 0x008;

/// Event Log Base Address Register — 8 bytes at offset `0x10`.
///
/// Same layout as the command buffer base.
pub const REG_OFFSET_EVENT_LOG_BASE: u32 = 0x010;

/// IOMMU Control Register — 8 bytes at offset `0x18`.
///
/// Each enable bit is paged by [`CTRL_BIT_*`](CTRL_BIT_IOMMU_EN).
pub const REG_OFFSET_CONTROL: u32 = 0x018;

/// Exclusion Range Base Register — 8 bytes at offset `0x20`.
pub const REG_OFFSET_EXCLUSION_BASE: u32 = 0x020;

/// Exclusion Range Limit Register — 8 bytes at offset `0x28`.
pub const REG_OFFSET_EXCLUSION_LIMIT: u32 = 0x028;

/// Extended Feature Register — 8 bytes at offset `0x30`. Read-only.
///
/// Advertises the optional capabilities the silicon implements:
/// prefetch, PPR, NX, GT, hardware-accessed PASID width, etc. See
/// the [`efr_*`](efr_supports_prefetch) decoders below.
pub const REG_OFFSET_EXT_FEATURE: u32 = 0x030;

/// PPR Log Base Address Register — 8 bytes at offset `0x38`.
pub const REG_OFFSET_PPR_LOG_BASE: u32 = 0x038;

/// Hardware Event Upper Register — 8 bytes at offset `0x40`. RW1C.
pub const REG_OFFSET_HW_EVENT_UPPER: u32 = 0x040;

/// Hardware Event Lower Register — 8 bytes at offset `0x48`. RW1C.
pub const REG_OFFSET_HW_EVENT_LOWER: u32 = 0x048;

/// Hardware Event Status Register — 8 bytes at offset `0x50`. RW1C.
pub const REG_OFFSET_HW_EVENT_STATUS: u32 = 0x050;

/// Command Buffer Head Pointer Register — 8 bytes at offset `0x2000`.
///
/// Hardware-maintained pointer into the ring (read-only). Software
/// observes this register to wait for completion of in-flight
/// commands.
pub const REG_OFFSET_COMMAND_BUFFER_HEAD: u32 = 0x2000;

/// Command Buffer Tail Pointer Register — 8 bytes at offset `0x2008`.
///
/// Software-maintained pointer into the ring; advancing the tail
/// notifies the IOMMU of new commands.
pub const REG_OFFSET_COMMAND_BUFFER_TAIL: u32 = 0x2008;

/// Event Log Head Pointer Register — 8 bytes at offset `0x2010`.
pub const REG_OFFSET_EVENT_LOG_HEAD: u32 = 0x2010;

/// Event Log Tail Pointer Register — 8 bytes at offset `0x2018`.
pub const REG_OFFSET_EVENT_LOG_TAIL: u32 = 0x2018;

/// IOMMU Status Register — 8 bytes at offset `0x2020`. RW1C.
///
/// Bit 0 = `EventOverflow`, bit 1 = `EventLogInt`, bit 2 = `ComWaitInt`,
/// bit 3 = `EventLogRun`, bit 4 = `CmdBufRun`. See AMD spec § 5.5.4.
pub const REG_OFFSET_STATUS: u32 = 0x2020;

// -- Control Register bit positions per spec rev 3.10 § 5.5.5 ----------

/// `IommuEn` (Enable IOMMU) — bit 0 of CONTROL.
pub const CTRL_BIT_IOMMU_EN: u64 = 1 << 0;
/// `HtTunEn` (`HyperTransport` Tunnel Enable) — bit 1.
pub const CTRL_BIT_HT_TUN_EN: u64 = 1 << 1;
/// `EventLogEn` (Event Log Enable) — bit 2.
pub const CTRL_BIT_EVENT_LOG_EN: u64 = 1 << 2;
/// `EventIntEn` (Event-log Interrupt Enable) — bit 3.
pub const CTRL_BIT_EVENT_INT_EN: u64 = 1 << 3;
/// `ComWaitIntEn` (Completion-wait Interrupt Enable) — bit 4.
pub const CTRL_BIT_COM_WAIT_INT_EN: u64 = 1 << 4;
/// `Coherent` — bit 10. When set, IOMMU page-walks are snoop coherent.
pub const CTRL_BIT_COHERENT: u64 = 1 << 10;
/// `Isoc` (Isochronous) — bit 11.
pub const CTRL_BIT_ISOC: u64 = 1 << 11;
/// `CmdBufEn` (Command-Buffer Enable) — bit 12.
pub const CTRL_BIT_CMD_BUF_EN: u64 = 1 << 12;
/// `PprLogEn` (PPR Log Enable) — bit 13.
pub const CTRL_BIT_PPR_LOG_EN: u64 = 1 << 13;
/// `PprIntEn` (PPR Interrupt Enable) — bit 14.
pub const CTRL_BIT_PPR_INT_EN: u64 = 1 << 14;
/// `PprEn` (Peripheral Page Request Enable) — bit 15.
pub const CTRL_BIT_PPR_EN: u64 = 1 << 15;
/// `GTEn` (Guest Translation Enable) — bit 16.
pub const CTRL_BIT_GT_EN: u64 = 1 << 16;

// =============================================================================
// Section 2 — Device Table Entry (DTE) + I/O PTE encoders (AMD spec §§ 5.2.2 /
// 5.3.1).
//
// A DTE is 256 bits (4 × u64) per spec § 5.2.2.2; one entry per
// requester ID (BDF). An I/O PTE is 64 bits per spec § 5.3.1. We
// store both as `u64` quadwords (rather than `#[repr(C)]` bitfields)
// so the encoders stay pure functions that the host test suite can
// exercise without an `unsafe { *mut Dte }` cast.
// =============================================================================

/// Size of a single AMD-Vi Device Table Entry (§ 5.2.2.2).
///
/// 256 bits = 32 bytes. One entry per requester ID = 64 KiB total for
/// a flat 16-bit BDF space (`MAX_DEV_TABLE_BYTES = 2 MiB` per
/// 256-entry segment, capped at 64 KiB for the typical single-segment
/// case).
pub const DEVICE_TABLE_ENTRY_BYTES: usize = 32;

/// Size of a single AMD-Vi I/O Page Table Entry (§ 5.3.1).
///
/// 64 bits = 8 bytes. A 4-KiB I/O page table holds 512 entries.
pub const IOPTE_BYTES: usize = 8;

/// Encoded AMD-Vi Device Table Entry.
///
/// Layout (§ 5.2.2.2; only the fields the scaffold encodes/decodes are
/// listed):
/// ```text
/// qword[0]:
///   bit  0    : V (Valid)
///   bit  1    : TV (Translation Information Valid)
///   bit  9..11: Mode (page-table level depth, 1..6; 0 = no translation)
///   bit 12..51: Page-Table Root (4-KiB aligned physical address)
///   bit 61    : IR (I/O Read permission)
///   bit 62    : IW (I/O Write permission)
/// qword[1]:
///   bit  0..15: DomainID (16 bits)
/// qword[2]:
///   reserved / interrupt-remapping (not encoded by the scaffold)
/// qword[3]:
///   reserved (not encoded by the scaffold)
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeviceTableEntry {
    /// Four quadwords (256 bits total).
    pub qwords: [u64; 4],
}

impl DeviceTableEntry {
    /// `V` bit position (`qwords[0]` bit 0).
    pub const BIT_V: u64 = 1 << 0;
    /// `TV` bit position (`qwords[0]` bit 1).
    pub const BIT_TV: u64 = 1 << 1;
    /// `IR` bit position (`qwords[0]` bit 61).
    pub const BIT_IR: u64 = 1 << 61;
    /// `IW` bit position (`qwords[0]` bit 62).
    pub const BIT_IW: u64 = 1 << 62;

    /// `true` iff the Valid bit (`qwords[0]` bit 0) is set.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        (self.qwords[0] & Self::BIT_V) != 0
    }

    /// `true` iff the Translation Valid bit (`qwords[0]` bit 1) is set.
    #[must_use]
    pub const fn translation_valid(self) -> bool {
        (self.qwords[0] & Self::BIT_TV) != 0
    }

    /// Extract the 4-KiB-aligned page-table root pointer (`qwords[0]`
    /// bits 12..51).
    #[must_use]
    pub const fn page_table_root(self) -> u64 {
        self.qwords[0] & 0x000F_FFFF_FFFF_F000
    }

    /// Extract the Mode field (`qwords[0]` bits 9..11).
    #[must_use]
    pub const fn mode_raw(self) -> u8 {
        ((self.qwords[0] >> 9) & 0b111) as u8
    }

    /// Extract the 16-bit Domain identifier (`qwords[1]` bits 0..15).
    #[must_use]
    pub const fn domain_id(self) -> DomainId {
        DomainId::new((self.qwords[1] & 0xFFFF) as u16)
    }

    /// `true` iff the IR (I/O Read) permission bit is set.
    #[must_use]
    pub const fn allows_read(self) -> bool {
        (self.qwords[0] & Self::BIT_IR) != 0
    }

    /// `true` iff the IW (I/O Write) permission bit is set.
    #[must_use]
    pub const fn allows_write(self) -> bool {
        (self.qwords[0] & Self::BIT_IW) != 0
    }
}

/// AMD-Vi page-table-walk depth (DTE Mode field, § 5.2.2.2).
///
/// Selects how many levels of the I/O page table the IOMMU walks
/// before reaching the leaf I/O PTE. `NoTranslation` is the "all
/// requests pass through untranslated" mode used by the bring-up
/// domain `0`; `Level1..6` are the standard 4-KiB / 2-MiB / 1-GiB /
/// 512-GiB / 256-TiB / 128-PiB paging modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMode {
    /// `Mode = 0` — no translation (host-physical = guest-physical).
    NoTranslation = 0,
    /// `Mode = 1` — 1-level table, 4-KiB leaves.
    Level1 = 1,
    /// `Mode = 2` — 2-level table.
    Level2 = 2,
    /// `Mode = 3` — 3-level table.
    Level3 = 3,
    /// `Mode = 4` — 4-level table (matches `x86_64` paging).
    Level4 = 4,
    /// `Mode = 5` — 5-level table.
    Level5 = 5,
    /// `Mode = 6` — 6-level table (256-TiB address space).
    Level6 = 6,
}

impl PageMode {
    /// Raw 3-bit `Mode` encoding for the DTE / I/O PTE.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a 3-bit `Mode` value back into a [`PageMode`]. Values
    /// outside `0..=6` are clamped to [`PageMode::NoTranslation`] so a
    /// torn read never crashes the syscall layer.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Level1,
            2 => Self::Level2,
            3 => Self::Level3,
            4 => Self::Level4,
            5 => Self::Level5,
            6 => Self::Level6,
            _ => Self::NoTranslation,
        }
    }
}

/// Build a Device Table Entry pointing at `page_table_root` for
/// `domain` with the given page-walk depth and I/O permission flags.
///
/// `IommuFlags::READ` / `IommuFlags::WRITE` map to the DTE `IR` / `IW`
/// bits respectively. `IommuFlags::EXECUTE` and `IommuFlags::COHERENT`
/// do not surface at the DTE level on AMD-Vi: the per-page snoop bit
/// lives in the I/O PTE, and DMA does not have an execute permission
/// in AMD-Vi (transactions never fetch instructions). The encoder
/// silently ignores them at the DTE layer; they ARE honoured by
/// [`encode_iopte`] below.
///
/// # Errors
///
/// Returns `Err([AmdViError::AddressMisaligned])` when
/// `page_table_root` is not 4-KiB aligned.
pub fn encode_device_table_entry(
    page_table_root: u64,
    domain: DomainId,
    mode: PageMode,
    flags: IommuFlags,
) -> Result<DeviceTableEntry, AmdViError> {
    if page_table_root & 0xFFF != 0 {
        return Err(AmdViError::AddressMisaligned);
    }
    let mut qword0 = (page_table_root & 0x000F_FFFF_FFFF_F000)
        | ((u64::from(mode.as_u8()) & 0b111) << 9)
        | DeviceTableEntry::BIT_V
        | DeviceTableEntry::BIT_TV;
    if flags.contains(IommuFlags::READ) {
        qword0 |= DeviceTableEntry::BIT_IR;
    }
    if flags.contains(IommuFlags::WRITE) {
        qword0 |= DeviceTableEntry::BIT_IW;
    }
    let qword1 = u64::from(domain.raw());
    Ok(DeviceTableEntry {
        qwords: [qword0, qword1, 0, 0],
    })
}

/// Build a Valid-but-untranslated Device Table Entry.
///
/// Sets `V = 1`, `TV = 0`, all permission bits = 0; AMD-Vi treats
/// such an entry as "device exists but every DMA request is blocked"
/// — the safe default for an enumerated-but-unconfigured PCI device.
#[must_use]
pub const fn encode_device_table_entry_blocked(domain: DomainId) -> DeviceTableEntry {
    DeviceTableEntry {
        qwords: [DeviceTableEntry::BIT_V, domain.raw() as u64, 0, 0],
    }
}

/// Build a not-present Device Table Entry (all quadwords zero).
///
/// Used during bring-up to publish a blank device table. Any DMA
/// request that hits a `V = 0` entry is target-aborted by the IOMMU.
#[must_use]
pub const fn encode_device_table_entry_absent() -> DeviceTableEntry {
    DeviceTableEntry { qwords: [0; 4] }
}

/// Encoded AMD-Vi I/O Page Table Entry (PTE).
///
/// Layout (§ 5.3.1; flags follow the same convention as the
/// CPU-side page tables but the bit positions are AMD-Vi-specific):
/// ```text
/// bit  0      : PR (Present)
/// bit  1..8   : Reserved / software available
/// bit  9..11  : NextLvl (next page-table level; 0 = 4-KiB leaf)
/// bit 12..51  : Page Address (4-KiB-aligned host-physical address)
/// bit 52..60  : Software available / reserved
/// bit 61      : FC (Force Coherent / Snoop)
/// bit 62      : IR (I/O Read permission)
/// bit 63      : IW (I/O Write permission)
/// ```
///
/// Constructed via [`encode_iopte`] and consumed via the field
/// accessors below.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IoPageTableEntry(pub u64);

impl IoPageTableEntry {
    /// `PR` bit position (bit 0).
    pub const BIT_PR: u64 = 1 << 0;
    /// `FC` bit position (bit 61).
    pub const BIT_FC: u64 = 1 << 61;
    /// `IR` bit position (bit 62).
    pub const BIT_IR: u64 = 1 << 62;
    /// `IW` bit position (bit 63).
    pub const BIT_IW: u64 = 1 << 63;

    /// `true` iff the Present bit (bit 0) is set.
    #[must_use]
    pub const fn is_present(self) -> bool {
        (self.0 & Self::BIT_PR) != 0
    }

    /// 4-KiB-aligned output address (bits 12..51).
    #[must_use]
    pub const fn output_address(self) -> u64 {
        self.0 & 0x000F_FFFF_FFFF_F000
    }

    /// Extract the `NextLvl` field (bits 9..11).
    #[must_use]
    pub const fn next_level(self) -> u8 {
        ((self.0 >> 9) & 0b111) as u8
    }

    /// `true` iff the IR (I/O Read) permission bit is set.
    #[must_use]
    pub const fn allows_read(self) -> bool {
        (self.0 & Self::BIT_IR) != 0
    }

    /// `true` iff the IW (I/O Write) permission bit is set.
    #[must_use]
    pub const fn allows_write(self) -> bool {
        (self.0 & Self::BIT_IW) != 0
    }

    /// `true` iff the FC (Force Coherent / Snoop) bit is set.
    #[must_use]
    pub const fn force_coherent(self) -> bool {
        (self.0 & Self::BIT_FC) != 0
    }
}

/// Build a leaf I/O PTE for `phys` with `flags`.
///
/// Translates the kernel [`IommuFlags`] surface into the AMD-Vi
/// bit-position constants:
///
/// - `READ`  → `IR`
/// - `WRITE` → `IW` (plus `IR`, mirroring [`super::vtd::encode_slpte`]
///   semantics — AMD-Vi does not require `IR` for a `IW`-only entry,
///   but treating write-only as malformed keeps the cross-vendor
///   semantics uniform).
/// - `EXECUTE` → ignored (AMD-Vi has no execute bit for DMA).
/// - `COHERENT` → `FC` (Force Coherent / Snoop).
///
/// The encoder always emits a leaf entry (`NextLvl = 0`). Intermediate
/// page-directory entries are constructed by [`encode_pde`].
///
/// # Errors
///
/// Returns `Err([AmdViError::AddressMisaligned])` when `phys` is not
/// 4-KiB aligned.
pub fn encode_iopte(phys: u64, flags: IommuFlags) -> Result<IoPageTableEntry, AmdViError> {
    if phys & 0xFFF != 0 {
        return Err(AmdViError::AddressMisaligned);
    }
    let mut bits = phys & 0x000F_FFFF_FFFF_F000;
    if flags.contains(IommuFlags::READ) || flags.contains(IommuFlags::WRITE) {
        bits |= IoPageTableEntry::BIT_PR;
        bits |= IoPageTableEntry::BIT_IR;
    }
    if flags.contains(IommuFlags::WRITE) {
        bits |= IoPageTableEntry::BIT_IW;
    }
    if flags.contains(IommuFlags::COHERENT) {
        bits |= IoPageTableEntry::BIT_FC;
    }
    Ok(IoPageTableEntry(bits))
}

/// Build a Page Directory Entry (PDE) — an intermediate entry that
/// points at another page table at level `next_level`.
///
/// PDEs are present + carry the `IR/IW` permissions of the entire
/// subtree (AMD-Vi propagates permissions top-down; if `IR` is clear
/// at a PDE, every entry below it is non-readable regardless).
///
/// # Errors
///
/// - `Err([AmdViError::AddressMisaligned])` when `next_table_phys` is
///   not 4-KiB aligned.
/// - `Err([AmdViError::UnsupportedFlags])` when `next_level` is `0`
///   (which would make this a leaf entry; callers must use
///   [`encode_iopte`] for leaves).
pub fn encode_pde(
    next_table_phys: u64,
    next_level: PageMode,
    flags: IommuFlags,
) -> Result<IoPageTableEntry, AmdViError> {
    if next_table_phys & 0xFFF != 0 {
        return Err(AmdViError::AddressMisaligned);
    }
    if matches!(next_level, PageMode::NoTranslation) {
        return Err(AmdViError::UnsupportedFlags);
    }
    let mut bits = next_table_phys & 0x000F_FFFF_FFFF_F000;
    bits |= IoPageTableEntry::BIT_PR;
    bits |= (u64::from(next_level.as_u8()) & 0b111) << 9;
    if flags.contains(IommuFlags::READ) || flags.contains(IommuFlags::WRITE) {
        bits |= IoPageTableEntry::BIT_IR;
    }
    if flags.contains(IommuFlags::WRITE) {
        bits |= IoPageTableEntry::BIT_IW;
    }
    if flags.contains(IommuFlags::COHERENT) {
        bits |= IoPageTableEntry::BIT_FC;
    }
    Ok(IoPageTableEntry(bits))
}

// =============================================================================
// Section 3 — Extended Feature Register (EFR) field decoders
// (AMD IOMMU spec rev 3.10 § 5.7).
//
// The probe path reads EFR once per IOMMU to learn the optional
// capabilities the silicon advertises (Prefetch, PPR, NX, GT, PASID
// width, ...). These helpers stay pure so the host test suite can
// exercise every bit pattern without firmware.
// =============================================================================

/// `PreFSup` — bit 0 of EFR. Set when the IOMMU supports the
/// `PREFETCH_PAGES` invalidation command.
#[must_use]
pub const fn efr_supports_prefetch(efr: u64) -> bool {
    (efr & (1 << 0)) != 0
}

/// `PPRSup` — bit 1 of EFR. Set when Peripheral Page Request is
/// supported.
#[must_use]
pub const fn efr_supports_ppr(efr: u64) -> bool {
    (efr & (1 << 1)) != 0
}

/// `XTSup` — bit 2 of EFR. Set when 6-level page-tables and extended
/// PCI device-ID format are supported.
#[must_use]
pub const fn efr_supports_xt(efr: u64) -> bool {
    (efr & (1 << 2)) != 0
}

/// `NXSup` — bit 3 of EFR. Set when the No-Execute bit is honoured on
/// guest page-table walks (`GTEn` mode).
#[must_use]
pub const fn efr_supports_nx(efr: u64) -> bool {
    (efr & (1 << 3)) != 0
}

/// `GTSup` — bit 4 of EFR. Set when Guest Translation is supported
/// (the IOMMU honours nested page tables for guest DMA requests).
#[must_use]
pub const fn efr_supports_gt(efr: u64) -> bool {
    (efr & (1 << 4)) != 0
}

/// `IASup` — bit 6 of EFR. Set when invalidate-all (`INVALIDATE_ALL`)
/// command is supported.
#[must_use]
pub const fn efr_supports_invalidate_all(efr: u64) -> bool {
    (efr & (1 << 6)) != 0
}

/// `GASup` — bit 7 of EFR. Set when Guest APIC virtualization is
/// supported.
#[must_use]
pub const fn efr_supports_ga(efr: u64) -> bool {
    (efr & (1 << 7)) != 0
}

/// `HESup` — bit 8 of EFR. Set when hardware-error reporting is
/// supported.
#[must_use]
pub const fn efr_supports_hardware_error(efr: u64) -> bool {
    (efr & (1 << 8)) != 0
}

/// `PASmax` — bits 11..15 of EFR (5 bits).
///
/// Maximum PASID width supported: the implementation can address up
/// to `2^(PASmax + 1) - 1` distinct PASIDs.
#[must_use]
pub const fn efr_pas_max(efr: u64) -> u8 {
    ((efr >> 11) & 0b1_1111) as u8
}

/// `HATS` (Host Address Translation Size) — bits 22..23 of EFR
/// (2 bits). Encodes the maximum host address-translation depth the
/// IOMMU advertises:
///
/// | HATS | Levels |
/// |------|--------|
/// | 0    | 4-level (48-bit physical address) |
/// | 1    | 5-level (57-bit) |
/// | 2    | 6-level (64-bit) |
/// | 3    | reserved (treated as `NoTranslation`) |
#[must_use]
pub const fn efr_hats(efr: u64) -> u8 {
    ((efr >> 22) & 0b11) as u8
}

/// Pick the highest supported [`PageMode`] from the EFR `HATS` field.
///
/// Returns [`PageMode::NoTranslation`] for the reserved value `0b11`
/// (defensive — firmware should never advertise this).
#[must_use]
pub const fn efr_highest_supported_mode(efr: u64) -> PageMode {
    match efr_hats(efr) {
        0 => PageMode::Level4,
        1 => PageMode::Level5,
        2 => PageMode::Level6,
        _ => PageMode::NoTranslation,
    }
}

// =============================================================================
// Section 4 — `AmdViBackend`: host-testable dormant backend.
//
// The struct tracks `(domain_id, [mapping_record])` tuples in an
// internal `Vec` so the `IommuBackend` trait can be exercised against
// it from host tests. It does NOT write any MMIO byte; the live
// backend lands in P6.7.9-pre.4 with explicit `unsafe` blocks gated
// behind `#[cfg(target_arch = "x86_64")]`.
// =============================================================================

/// Error category raised by the AMD-Vi encoders + scaffold backend.
///
/// Maps to [`IommuError`] when surfaced through the trait so callers
/// see a vendor-neutral taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmdViError {
    /// Address argument violated the AMD-Vi 4-KiB alignment requirement.
    AddressMisaligned,
    /// Caller passed a [`DomainId`] not previously installed.
    UnknownDomain,
    /// `flags` requested a permission the backend cannot honour, or a
    /// PDE encoder was invoked with [`PageMode::NoTranslation`].
    UnsupportedFlags,
}

impl From<AmdViError> for IommuError {
    fn from(err: AmdViError) -> Self {
        match err {
            AmdViError::AddressMisaligned => Self::AddressMisaligned,
            AmdViError::UnknownDomain => Self::InvalidDomain,
            AmdViError::UnsupportedFlags => Self::Unsupported,
        }
    }
}

/// One mapping record tracked by the scaffold backend.
///
/// Pure data — exists so the host test suite can assert on the
/// IOMMU-side state without touching MMIO. Symmetric to
/// [`super::vtd::ScaffoldMapping`] but carries an AMD-Vi `leaf_iopte`
/// rather than a VT-d `leaf_slpte`.
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
    /// Encoded I/O PTE for the first 4-KiB leaf.
    pub leaf_iopte: IoPageTableEntry,
}

/// Dormant AMD-Vi backend. Holds bookkeeping only; emits no MMIO.
///
/// The host-test exercise path:
///
/// 1. Build `AmdViBackend::new()`.
/// 2. `install_domain(DomainId::new(7))?;`
/// 3. `map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ)?;`
/// 4. Inspect the recorded `mappings()` slice in the assertion.
///
/// Live programming swap: P6.7.9-pre.4 adds a `unit_base: u64` field
/// and an `mmio_write64` helper; the `map`/`unmap` paths gain
/// `unsafe { ... }` blocks that write the descriptors back-to-back
/// into the IOMMU's command buffer
/// (`REG_OFFSET_COMMAND_BUFFER_TAIL`).
#[derive(Debug, Clone, Default)]
pub struct AmdViBackend {
    /// Installed domains, in insertion order.
    domains: Vec<DomainId>,
    /// Recorded mappings.
    mappings: Vec<ScaffoldMapping>,
}

impl AmdViBackend {
    /// Construct an empty backend.
    #[must_use]
    pub fn new() -> Self {
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

impl IommuBackend for AmdViBackend {
    fn vendor(&self) -> IommuVendor {
        IommuVendor::Amd
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
        let leaf = encode_iopte(phys, flags).map_err(IommuError::from)?;
        self.mappings.push(ScaffoldMapping {
            domain: id,
            iova,
            phys,
            len,
            leaf_iopte: leaf,
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
        // Dormant backend: nothing to flush. Live backend will queue
        // an `INVALIDATE_IOMMU_PAGES` command descriptor here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AmdViBackend, AmdViError, CTRL_BIT_CMD_BUF_EN, CTRL_BIT_COHERENT, CTRL_BIT_GT_EN,
        CTRL_BIT_IOMMU_EN, DeviceTableEntry, IoPageTableEntry, IommuBackend, IommuError,
        IommuFlags, IommuVendor, PageMode, REG_OFFSET_COMMAND_BUFFER_BASE,
        REG_OFFSET_COMMAND_BUFFER_HEAD, REG_OFFSET_COMMAND_BUFFER_TAIL, REG_OFFSET_CONTROL,
        REG_OFFSET_DEVICE_TABLE_BASE, REG_OFFSET_EVENT_LOG_BASE, REG_OFFSET_EVENT_LOG_HEAD,
        REG_OFFSET_EVENT_LOG_TAIL, REG_OFFSET_EXT_FEATURE, REG_OFFSET_STATUS, ScaffoldMapping,
        efr_hats, efr_highest_supported_mode, efr_pas_max, efr_supports_ga, efr_supports_gt,
        efr_supports_hardware_error, efr_supports_invalidate_all, efr_supports_nx,
        efr_supports_ppr, efr_supports_prefetch, efr_supports_xt, encode_device_table_entry,
        encode_device_table_entry_absent, encode_device_table_entry_blocked, encode_iopte,
        encode_pde,
    };
    use crate::bare_metal::iommu::DomainId;

    // ---- Register offset invariants ------------------------------------

    #[test]
    fn register_offsets_match_amd_spec_3_10() {
        // Pinning against the spec lets a future refactor catch any
        // accidental drift via the test suite rather than at runtime.
        assert_eq!(REG_OFFSET_DEVICE_TABLE_BASE, 0x000);
        assert_eq!(REG_OFFSET_COMMAND_BUFFER_BASE, 0x008);
        assert_eq!(REG_OFFSET_EVENT_LOG_BASE, 0x010);
        assert_eq!(REG_OFFSET_CONTROL, 0x018);
        assert_eq!(REG_OFFSET_EXT_FEATURE, 0x030);
        assert_eq!(REG_OFFSET_COMMAND_BUFFER_HEAD, 0x2000);
        assert_eq!(REG_OFFSET_COMMAND_BUFFER_TAIL, 0x2008);
        assert_eq!(REG_OFFSET_EVENT_LOG_HEAD, 0x2010);
        assert_eq!(REG_OFFSET_EVENT_LOG_TAIL, 0x2018);
        assert_eq!(REG_OFFSET_STATUS, 0x2020);
    }

    #[test]
    fn control_register_bits_match_spec() {
        assert_eq!(CTRL_BIT_IOMMU_EN, 1 << 0);
        assert_eq!(CTRL_BIT_COHERENT, 1 << 10);
        assert_eq!(CTRL_BIT_CMD_BUF_EN, 1 << 12);
        assert_eq!(CTRL_BIT_GT_EN, 1 << 16);
    }

    // ---- Device Table Entry encoder ------------------------------------

    #[test]
    fn encode_dte_sets_valid_and_translation_bits() {
        let entry = encode_device_table_entry(
            0xABCD_F000,
            DomainId::new(0x42),
            PageMode::Level4,
            IommuFlags::READ.union(IommuFlags::WRITE),
        )
        .unwrap();
        assert!(entry.is_valid());
        assert!(entry.translation_valid());
        assert_eq!(entry.page_table_root(), 0xABCD_F000);
        assert_eq!(entry.mode_raw(), PageMode::Level4.as_u8());
        assert_eq!(entry.domain_id(), DomainId::new(0x42));
        assert!(entry.allows_read());
        assert!(entry.allows_write());
    }

    #[test]
    fn encode_dte_read_only_unsets_iw() {
        let entry =
            encode_device_table_entry(0x1000, DomainId::new(1), PageMode::Level3, IommuFlags::READ)
                .unwrap();
        assert!(entry.allows_read());
        assert!(!entry.allows_write());
    }

    #[test]
    fn encode_dte_rejects_misaligned_root() {
        assert_eq!(
            encode_device_table_entry(0x1001, DomainId::new(0), PageMode::Level4, IommuFlags::READ),
            Err(AmdViError::AddressMisaligned)
        );
        assert_eq!(
            encode_device_table_entry(0x1FFF, DomainId::new(0), PageMode::Level4, IommuFlags::READ),
            Err(AmdViError::AddressMisaligned)
        );
    }

    #[test]
    fn encode_dte_round_trips_domain_id_and_mode() {
        let entry = encode_device_table_entry(
            0x10_0000,
            DomainId::new(0xFFFF),
            PageMode::Level6,
            IommuFlags::READ,
        )
        .unwrap();
        assert_eq!(entry.domain_id(), DomainId::new(0xFFFF));
        assert_eq!(entry.mode_raw(), 6);
    }

    #[test]
    fn encode_dte_no_translation_keeps_mode_zero() {
        let entry = encode_device_table_entry(
            0x10_0000,
            DomainId::new(0),
            PageMode::NoTranslation,
            IommuFlags::READ,
        )
        .unwrap();
        assert!(entry.is_valid());
        assert_eq!(entry.mode_raw(), 0);
    }

    #[test]
    fn encode_dte_absent_is_all_zero() {
        let entry = encode_device_table_entry_absent();
        assert!(!entry.is_valid());
        assert!(!entry.translation_valid());
        assert_eq!(entry, DeviceTableEntry { qwords: [0; 4] });
    }

    #[test]
    fn encode_dte_blocked_has_valid_bit_only() {
        let entry = encode_device_table_entry_blocked(DomainId::new(3));
        assert!(entry.is_valid());
        assert!(!entry.translation_valid());
        assert!(!entry.allows_read());
        assert!(!entry.allows_write());
        assert_eq!(entry.domain_id(), DomainId::new(3));
    }

    #[test]
    fn page_mode_round_trip() {
        for raw in 0u8..=6 {
            assert_eq!(PageMode::from_raw(raw).as_u8(), raw);
        }
        // Reserved value clamps to NoTranslation.
        assert_eq!(PageMode::from_raw(7), PageMode::NoTranslation);
        assert_eq!(PageMode::from_raw(0xFF), PageMode::NoTranslation);
    }

    // ---- I/O PTE encoder -----------------------------------------------

    #[test]
    fn encode_iopte_read_only() {
        let pte = encode_iopte(0xABCD_F000, IommuFlags::READ).unwrap();
        assert!(pte.is_present());
        assert_eq!(pte.output_address(), 0xABCD_F000);
        assert!(pte.allows_read());
        assert!(!pte.allows_write());
        assert!(!pte.force_coherent());
        assert_eq!(pte.next_level(), 0);
    }

    #[test]
    fn encode_iopte_write_forces_read_bit() {
        // AMD-Vi tolerates `IW`-only entries but we mirror the VT-d
        // semantics (force `IR` on whenever `IW` is requested) so the
        // cross-vendor invariant `WRITE ⇒ READ` holds at the SDK
        // surface.
        let pte = encode_iopte(0x1000, IommuFlags::WRITE).unwrap();
        assert!(pte.is_present());
        assert!(pte.allows_read());
        assert!(pte.allows_write());
    }

    #[test]
    fn encode_iopte_coherent_sets_fc_bit() {
        let flags = IommuFlags::READ.union(IommuFlags::COHERENT);
        let pte = encode_iopte(0x2000, flags).unwrap();
        assert!(pte.force_coherent());
        assert!(pte.allows_read());
    }

    #[test]
    fn encode_iopte_execute_flag_is_silently_ignored() {
        // AMD-Vi has no DMA execute bit; the encoder must not panic
        // and must not set any spurious bit when EXECUTE is passed.
        let flags = IommuFlags::READ.union(IommuFlags::EXECUTE);
        let pte = encode_iopte(0x3000, flags).unwrap();
        assert!(pte.is_present());
        assert!(pte.allows_read());
        assert!(!pte.allows_write());
        // No phantom bit set above the address field other than IR.
        assert_eq!(
            pte.0 & !(IoPageTableEntry::BIT_PR | IoPageTableEntry::BIT_IR | 0x000F_FFFF_FFFF_F000),
            0
        );
    }

    #[test]
    fn encode_iopte_rejects_misaligned_phys() {
        assert_eq!(
            encode_iopte(0x1001, IommuFlags::READ),
            Err(AmdViError::AddressMisaligned)
        );
    }

    #[test]
    fn encode_iopte_zero_flags_emits_not_present() {
        // No R, no W -> not present. Useful for clearing leaves during
        // unmap without zeroing the address bits.
        let pte = encode_iopte(0x1000, IommuFlags::from_bits(0)).unwrap();
        assert!(!pte.is_present());
        assert_eq!(pte.output_address(), 0x1000);
    }

    // ---- PDE encoder ---------------------------------------------------

    #[test]
    fn encode_pde_sets_next_level_and_permissions() {
        let pde = encode_pde(
            0xC0DE_0000,
            PageMode::Level2,
            IommuFlags::READ.union(IommuFlags::WRITE),
        )
        .unwrap();
        assert!(pde.is_present());
        assert_eq!(pde.next_level(), 2);
        assert_eq!(pde.output_address(), 0xC0DE_0000);
        assert!(pde.allows_read());
        assert!(pde.allows_write());
    }

    #[test]
    fn encode_pde_rejects_leaf_level() {
        assert_eq!(
            encode_pde(0x1000, PageMode::NoTranslation, IommuFlags::READ),
            Err(AmdViError::UnsupportedFlags)
        );
    }

    #[test]
    fn encode_pde_rejects_misaligned_phys() {
        assert_eq!(
            encode_pde(0x1001, PageMode::Level3, IommuFlags::READ),
            Err(AmdViError::AddressMisaligned)
        );
    }

    // ---- EFR decoders --------------------------------------------------

    #[test]
    fn efr_feature_bits_round_trip() {
        let efr = (1u64 << 0)
            | (1 << 1)
            | (1 << 2)
            | (1 << 3)
            | (1 << 4)
            | (1 << 6)
            | (1 << 7)
            | (1 << 8);
        assert!(efr_supports_prefetch(efr));
        assert!(efr_supports_ppr(efr));
        assert!(efr_supports_xt(efr));
        assert!(efr_supports_nx(efr));
        assert!(efr_supports_gt(efr));
        assert!(efr_supports_invalidate_all(efr));
        assert!(efr_supports_ga(efr));
        assert!(efr_supports_hardware_error(efr));
    }

    #[test]
    fn efr_feature_bits_zero_efr_reports_nothing() {
        assert!(!efr_supports_prefetch(0));
        assert!(!efr_supports_ppr(0));
        assert!(!efr_supports_nx(0));
        assert!(!efr_supports_gt(0));
        assert!(!efr_supports_invalidate_all(0));
    }

    #[test]
    fn efr_pas_max_extracts_bits_11_to_15() {
        let efr = 0b1_1111u64 << 11;
        assert_eq!(efr_pas_max(efr), 0b1_1111);
        let efr = 0b1_0101u64 << 11;
        assert_eq!(efr_pas_max(efr), 0b1_0101);
    }

    #[test]
    fn efr_hats_decodes_known_levels() {
        // HATS = 0 → 4-level (Level4)
        assert_eq!(efr_hats(0), 0);
        assert_eq!(efr_highest_supported_mode(0), PageMode::Level4);
        // HATS = 1 → 5-level
        assert_eq!(efr_hats(1u64 << 22), 1);
        assert_eq!(efr_highest_supported_mode(1u64 << 22), PageMode::Level5);
        // HATS = 2 → 6-level
        assert_eq!(efr_hats(2u64 << 22), 2);
        assert_eq!(efr_highest_supported_mode(2u64 << 22), PageMode::Level6);
        // HATS = 3 → reserved → NoTranslation (defensive).
        assert_eq!(efr_hats(3u64 << 22), 3);
        assert_eq!(
            efr_highest_supported_mode(3u64 << 22),
            PageMode::NoTranslation
        );
    }

    // ---- AmdViBackend bookkeeping --------------------------------------

    #[test]
    fn amdvi_backend_vendor_reports_amd() {
        let backend = AmdViBackend::new();
        assert_eq!(backend.vendor(), IommuVendor::Amd);
    }

    #[test]
    fn amdvi_backend_install_domain_is_idempotent() {
        let mut backend = AmdViBackend::new();
        backend.install_domain(DomainId::new(3)).unwrap();
        backend.install_domain(DomainId::new(3)).unwrap();
        assert!(backend.has_domain(DomainId::new(3)));
        assert_eq!(backend.domains(), &[DomainId::new(3)]);
    }

    #[test]
    fn amdvi_backend_map_rejects_unknown_domain() {
        let mut backend = AmdViBackend::new();
        assert_eq!(
            backend.map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn amdvi_backend_map_records_mapping_with_encoded_iopte() {
        let mut backend = AmdViBackend::new();
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
        assert_eq!(rec.leaf_iopte.output_address(), 0x2000);
        assert!(rec.leaf_iopte.is_present());
        assert!(rec.leaf_iopte.allows_write());
    }

    #[test]
    fn amdvi_backend_map_rejects_misaligned_arguments() {
        let mut backend = AmdViBackend::new();
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
    fn amdvi_backend_unmap_removes_record() {
        let mut backend = AmdViBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        backend
            .map(DomainId::new(7), 0x1000, 0x2000, 0x1000, IommuFlags::READ)
            .unwrap();
        assert_eq!(backend.mappings().len(), 1);
        backend.unmap(DomainId::new(7), 0x1000, 0x1000).unwrap();
        assert!(backend.mappings().is_empty());
    }

    #[test]
    fn amdvi_backend_unmap_unmapped_range_returns_error() {
        let mut backend = AmdViBackend::new();
        backend.install_domain(DomainId::new(7)).unwrap();
        assert_eq!(
            backend.unmap(DomainId::new(7), 0x1000, 0x1000),
            Err(IommuError::UnmapFailed)
        );
    }

    #[test]
    fn amdvi_backend_unmap_rejects_unknown_domain() {
        let mut backend = AmdViBackend::new();
        assert_eq!(
            backend.unmap(DomainId::new(9), 0x1000, 0x1000),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn amdvi_backend_flush_rejects_unknown_domain() {
        let mut backend = AmdViBackend::new();
        assert_eq!(
            backend.flush(DomainId::new(0)),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn amdvi_backend_flush_known_domain_is_ok() {
        let mut backend = AmdViBackend::new();
        backend.install_domain(DomainId::new(0)).unwrap();
        assert_eq!(backend.flush(DomainId::new(0)), Ok(()));
    }

    // ---- AmdViError → IommuError mapping -------------------------------

    #[test]
    fn amdvi_error_into_iommu_error_mapping() {
        assert_eq!(
            IommuError::from(AmdViError::AddressMisaligned),
            IommuError::AddressMisaligned
        );
        assert_eq!(
            IommuError::from(AmdViError::UnknownDomain),
            IommuError::InvalidDomain
        );
        assert_eq!(
            IommuError::from(AmdViError::UnsupportedFlags),
            IommuError::Unsupported
        );
    }
}
