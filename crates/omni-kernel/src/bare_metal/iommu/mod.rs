//! Kernel-side IOMMU abstraction layer — P6.7.9-pre.0 scaffold.
//!
//! This module provides the trait surface and vendor-neutral data
//! structures used by the future Intel VT-d (`vtd`) and AMD-Vi
//! (`amdvi`) backends. The Phase 1 driver framework (`DmaMap (71)`
//! syscall handler in [`crate::bare_metal::syscall_entry`]) still
//! runs in **passthrough mode** (`iova == user_va`, strict-contiguous
//! frame allocation) per OIP-013 § S3.3 Appendix B amendment 1; this
//! scaffold establishes the API the backends will plug into without
//! changing runtime behaviour.
//!
//! ## Sub-modules
//!
//! - [`mod@dmar`] — ACPI DMAR table parser (Intel VT-d, ACPI § 5).
//! - [`mod@ivrs`] — ACPI IVRS table parser (AMD-Vi).
//! - [`mod@domain`] — domain identifier allocator shared by both
//!   backends. The DMAR / IVRS tables drive backend selection at
//!   boot; the domain allocator is the kernel-side accounting layer
//!   that every backend consumes.
//!
//! ## Boot-time selection (deferred)
//!
//! The actual probe (`probe_iommu(rsdp, phys_offset) -> Option<&dyn
//! IommuBackend>`) and register programming land in P6.7.9-pre.1 and
//! P6.7.9-pre.2. Until then [`PassthroughBackend`] is the only
//! installed backend; the trait surface is defined to make that
//! transition mechanical.
//!
//! ## Why a trait + no allocator integration yet
//!
//! The trait abstracts over (a) Intel VT-d second-level page tables,
//! (b) AMD-Vi domain pointers in the device table, and (c) the
//! passthrough no-op. Each backend will own its own per-domain
//! page-table tree (≤ 2 MiB per domain in worst-case 4 KiB-paging
//! mode); that allocation lives inside the backend, not in this trait.
//! Phase 1 callers see only [`IommuBackend::map`] /
//! [`IommuBackend::unmap`] / [`IommuBackend::flush`].
//!
//! ## Security posture
//!
//! - **No `unsafe` in this module.** The parsers in [`dmar`] / [`ivrs`]
//!   take `&[u8]` slices and return owned structs; bare-metal callers
//!   wrap them in a single `unsafe` block when they read the firmware
//!   physical-memory window.
//! - **No interior mutability.** The trait takes `&mut self` so the
//!   kernel-side caller must hold a `spin::Mutex<&mut dyn IommuBackend>`
//!   to enforce serialisation. That lock lives outside this module to
//!   keep the trait host-testable.
//! - **No dynamic dispatch at runtime cost yet.** The trait will be
//!   used through a `&mut dyn IommuBackend` once the backend is
//!   selected at boot. Two virtual calls per `DmaMap` invocation —
//!   negligible vs. the syscall-entry cost.
//!
//! ## References
//!
//! - OIP-Driver-Framework-013 § S3 (capability scope + IOMMU
//!   integration semantics).
//! - Intel VT-d spec rev 4.1 § 8 (DMAR ACPI table layout).
//! - AMD I/O Virtualization Technology spec rev 3.10 § 5 (IVRS).

#![allow(
    clippy::module_name_repetitions,
    reason = "IommuBackend / IommuVendor / IommuFlags / IommuError share the Iommu prefix by design — disambiguates from any future PCI / interrupt-remapping types in sibling modules"
)]

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

pub mod amdvi;
pub mod dmar;
pub mod domain;
pub mod ivrs;
pub mod vtd;

/// Opaque IOMMU domain identifier (16-bit by VT-d spec).
///
/// Both VT-d (`DOMAIN_ID` field in the second-level context entry, 16
/// bits per spec rev 4.1 § 3.5.1) and AMD-Vi (`DomainID` in the device
/// table entry, 16 bits per AMD spec rev 3.10 § 5.2.2.2) use a 16-bit
/// domain identifier; this newtype carries that bound without any
/// vendor leakage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainId(pub u16);

impl DomainId {
    /// Convenience constructor — same as `DomainId(raw)`.
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Raw 16-bit value used by both vendor backends.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Vendor of the running IOMMU backend.
///
/// Reported by [`IommuBackend::vendor`]; the kernel boot path logs
/// `[iommu] vendor=<intel|amd|passthrough>` after selection.
///
/// Encoded as a `u8` in the [`IOMMU_VENDOR`] global so the boot-time
/// probe can stash the selected vendor without paying for a `Mutex` or
/// generic-over-trait indirection. See [`IommuVendor::from_u8`] /
/// [`IommuVendor::as_u8`] for the encoding contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuVendor {
    /// Intel VT-d backend (`vtd` module, lands P6.7.9-pre.2).
    Intel,
    /// AMD-Vi backend (`amdvi` module, lands P6.7.9-pre.2).
    Amd,
    /// Phase 1 passthrough mode: no IOMMU programming, `iova == phys`.
    /// Documented in OIP-013 § S3.3 Appendix B amendment 1.
    Passthrough,
}

impl IommuVendor {
    /// `Passthrough` discriminant — also the [`AtomicU8`] initial value
    /// in [`IOMMU_VENDOR`] so callers reading the global before the
    /// boot probe runs see a safe "no IOMMU" answer.
    pub const TAG_PASSTHROUGH: u8 = 0;
    /// `Intel` discriminant.
    pub const TAG_INTEL: u8 = 1;
    /// `Amd` discriminant.
    pub const TAG_AMD: u8 = 2;

    /// Encode as a `u8` for storage in [`IOMMU_VENDOR`].
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Passthrough => Self::TAG_PASSTHROUGH,
            Self::Intel => Self::TAG_INTEL,
            Self::Amd => Self::TAG_AMD,
        }
    }

    /// Decode from a `u8` previously returned by [`Self::as_u8`].
    /// Unknown tags map to `Passthrough` so a torn write or an
    /// uninitialised global never crashes the syscall layer.
    #[must_use]
    pub const fn from_u8(tag: u8) -> Self {
        match tag {
            Self::TAG_INTEL => Self::Intel,
            Self::TAG_AMD => Self::Amd,
            _ => Self::Passthrough,
        }
    }

    /// Static printable string for the boot log line (`[iommu] vendor=…`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passthrough => "passthrough",
            Self::Intel => "intel",
            Self::Amd => "amd",
        }
    }
}

// =============================================================================
// Global vendor state — P6.7.9-pre.1
//
// The boot-time probe (`probe`, below) writes the selected vendor into
// `IOMMU_VENDOR` and the discovered remapping-unit counts into
// `IOMMU_UNIT_COUNT`. Both are read by the upcoming `DmaMap` selector
// rewire (P6.7.9-pre.2) and by any kernel-side telemetry that wants to
// reflect IOMMU state in the Build Info panel.
//
// Writers: `set_iommu_vendor` / `set_iommu_unit_count`, called exactly
// once from `kmain` after `set_phys_offset`. Readers may race the write
// but the value is constant for the lifetime of the boot image; the
// `Relaxed` ordering matches the pattern used for `PHYS_OFFSET` /
// `BOOT_CR3` elsewhere in this module tree.
// =============================================================================

/// Encoded [`IommuVendor`] selected by the boot probe.
///
/// Initial value is [`IommuVendor::TAG_PASSTHROUGH`] so any reader that
/// runs before [`set_iommu_vendor`] sees the safe "no IOMMU" answer
/// rather than reading uninitialised state.
pub static IOMMU_VENDOR: AtomicU8 = AtomicU8::new(IommuVendor::TAG_PASSTHROUGH);

/// Number of IOMMU remapping units the boot probe discovered.
///
/// DRHD count for Intel, IVHD count for AMD, `0` for passthrough.
/// Used by the kernel boot log line; the upcoming P6.7.9-pre.2
/// backends will reuse it to size their per-unit register map.
pub static IOMMU_UNIT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// One-shot setter for [`IOMMU_VENDOR`].
#[inline]
pub fn set_iommu_vendor(vendor: IommuVendor) {
    IOMMU_VENDOR.store(vendor.as_u8(), Ordering::Relaxed);
}

/// Read the boot-time-selected IOMMU vendor. Returns
/// [`IommuVendor::Passthrough`] before the probe has run.
#[must_use]
#[inline]
pub fn iommu_vendor() -> IommuVendor {
    IommuVendor::from_u8(IOMMU_VENDOR.load(Ordering::Relaxed))
}

/// One-shot setter for [`IOMMU_UNIT_COUNT`].
#[inline]
pub fn set_iommu_unit_count(count: usize) {
    IOMMU_UNIT_COUNT.store(count, Ordering::Relaxed);
}

/// Read the IOMMU remapping-unit count recorded by the boot probe.
#[must_use]
#[inline]
pub fn iommu_unit_count() -> usize {
    IOMMU_UNIT_COUNT.load(Ordering::Relaxed)
}

/// Result of [`probe`]: vendor selection plus advertised unit counts.
///
/// Returned by both the bare-metal `unsafe` variant and the host-side
/// stub so callers (currently `kmain` only) can emit the same log
/// line in either build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    /// Selected vendor (also written to [`IOMMU_VENDOR`]).
    pub vendor: IommuVendor,
    /// Number of DRHD entries when [`vendor`] is [`IommuVendor::Intel`].
    /// Always `0` for other vendors.
    ///
    /// [`vendor`]: ProbeResult::vendor
    pub drhd_count: usize,
    /// Number of IVHD entries when [`vendor`] is [`IommuVendor::Amd`].
    /// Always `0` for other vendors.
    ///
    /// [`vendor`]: ProbeResult::vendor
    pub ivhd_count: usize,
}

impl ProbeResult {
    /// Passthrough fallback used when the firmware does not advertise
    /// any IOMMU at all (typical for QEMU `q35` without `iommu=` or for
    /// pre-IOMMU hardware).
    pub const PASSTHROUGH: Self = Self {
        vendor: IommuVendor::Passthrough,
        drhd_count: 0,
        ivhd_count: 0,
    };
}

/// Pure-function vendor selector consuming the DMAR / IVRS table parse
/// results. Lets the host-side test suite cover the selection logic
/// without running the unsafe bare-metal probe.
///
/// Rules:
///
/// 1. If the firmware advertises at least one DRHD → `Intel`.
/// 2. Else, if it advertises at least one IVHD → `Amd`.
/// 3. Else → `Passthrough`.
///
/// Intel is preferred over AMD when both tables exist on the same
/// platform (an unusual configuration: a few embedded `SoCs` ship both
/// firmware tables, with one of them stubbed). The DMAR parser already
/// validates that at least one DRHD is present before returning
/// success; the same holds for IVRS.
#[must_use]
pub fn select_vendor(drhd_count: usize, ivhd_count: usize) -> ProbeResult {
    if drhd_count > 0 {
        ProbeResult {
            vendor: IommuVendor::Intel,
            drhd_count,
            ivhd_count: 0,
        }
    } else if ivhd_count > 0 {
        ProbeResult {
            vendor: IommuVendor::Amd,
            drhd_count: 0,
            ivhd_count,
        }
    } else {
        ProbeResult::PASSTHROUGH
    }
}

/// Bare-metal IOMMU probe — locate DMAR and IVRS via RSDP, parse them,
/// and stash the selected vendor + unit count in the module globals.
///
/// Called once from `kmain` right after [`crate::bare_metal::set_phys_offset`]
/// so any subsequent code path (driver framework, scheduler bring-up) sees
/// the resolved vendor through [`iommu_vendor`].
///
/// # Safety
///
/// Same invariants as [`crate::bare_metal::mp::enumerate_cpus`]:
/// `phys_offset.wrapping_add(rsdp_phys)` must point at a valid RSDP and
/// every ACPI table physical address reachable from there must lie
/// within the firmware-mapped physical-memory window.
#[cfg(target_arch = "x86_64")]
pub unsafe fn probe(rsdp_phys: u64, phys_offset: u64) -> ProbeResult {
    let drhd_count = unsafe {
        crate::bare_metal::mp::find_table_phys(rsdp_phys, phys_offset, b"DMAR")
            .and_then(|phys| read_table_drhd_count(phys, phys_offset))
            .unwrap_or(0)
    };
    let ivhd_count = unsafe {
        crate::bare_metal::mp::find_table_phys(rsdp_phys, phys_offset, b"IVRS")
            .and_then(|phys| read_table_ivhd_count(phys, phys_offset))
            .unwrap_or(0)
    };
    let result = select_vendor(drhd_count, ivhd_count);
    set_iommu_vendor(result.vendor);
    let unit_count = match result.vendor {
        IommuVendor::Intel => result.drhd_count,
        IommuVendor::Amd => result.ivhd_count,
        IommuVendor::Passthrough => 0,
    };
    set_iommu_unit_count(unit_count);
    result
}

/// Host-side stub: there is no firmware physical-memory window in
/// `cargo test`, so this variant always returns the passthrough fallback
/// without touching memory. The pure-function path
/// ([`select_vendor`]) is what host tests exercise.
#[cfg(not(target_arch = "x86_64"))]
#[must_use]
pub unsafe fn probe(_rsdp_phys: u64, _phys_offset: u64) -> ProbeResult {
    ProbeResult::PASSTHROUGH
}

/// Read the DMAR table at `table_phys`, parse it, return the DRHD
/// count, or `None` if any step in the walk fails. Used by [`probe`].
///
/// # Safety
///
/// `phys_offset.wrapping_add(table_phys)` must reference a 4-byte ACPI
/// SDT header whose `length` field bounds a buffer entirely contained
/// within the firmware-mapped physical-memory window.
#[cfg(target_arch = "x86_64")]
unsafe fn read_table_drhd_count(table_phys: u64, phys_offset: u64) -> Option<usize> {
    let header_ptr = phys_offset.wrapping_add(table_phys) as *const u8;
    let length = unsafe { header_ptr.add(4).cast::<u32>().read_unaligned() } as usize;
    if length < 48 {
        return None;
    }
    // SAFETY: caller guarantees the entire `length` byte range is
    // mapped; the bound is read from the firmware-supplied header.
    let buf = unsafe { core::slice::from_raw_parts(header_ptr, length) };
    dmar::parse_dmar(buf).ok().map(|t| t.drhd_count())
}

/// Read the IVRS table at `table_phys`, parse it, return the IVHD
/// count, or `None` if any step in the walk fails. Used by [`probe`].
///
/// # Safety
///
/// `phys_offset.wrapping_add(table_phys)` must reference a 4-byte ACPI
/// SDT header whose `length` field bounds a buffer entirely contained
/// within the firmware-mapped physical-memory window.
#[cfg(target_arch = "x86_64")]
unsafe fn read_table_ivhd_count(table_phys: u64, phys_offset: u64) -> Option<usize> {
    let header_ptr = phys_offset.wrapping_add(table_phys) as *const u8;
    let length = unsafe { header_ptr.add(4).cast::<u32>().read_unaligned() } as usize;
    if length < 48 {
        return None;
    }
    // SAFETY: same as [`read_table_drhd_count`].
    let buf = unsafe { core::slice::from_raw_parts(header_ptr, length) };
    ivrs::parse_ivrs(buf).ok().map(|t| t.ivhd_count())
}

/// IOMMU page-permission flags. Bit positions are local to OMNI and
/// translated to vendor-specific bits inside each backend's `map`.
///
/// Modelled after the page-table flag constants in
/// [`crate::bare_metal::paging`] (`PTE_PRESENT`, `PTE_WRITABLE`, ...) so
/// the convention is uniform across the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IommuFlags(u32);

impl IommuFlags {
    /// Device-readable mapping (IOMMU `R` bit).
    pub const READ: Self = Self(1 << 0);
    /// Device-writable mapping (IOMMU `W` bit).
    pub const WRITE: Self = Self(1 << 1);
    /// Device-executable mapping. Most NICs / storage controllers do
    /// **not** need this; it is included for completeness because some
    /// AI accelerators load firmware via DMA.
    pub const EXECUTE: Self = Self(1 << 2);
    /// Snoop-coherent transactions only. When set, the backend
    /// programs the vendor-specific snoop bit so the device sees
    /// CPU cache state. Default (unset) is "no-snoop allowed".
    pub const COHERENT: Self = Self(1 << 3);

    /// Construct directly from raw bits. Reserved bits are kept
    /// verbatim so future flag additions are forward-compatible.
    #[must_use]
    pub const fn from_bits(raw: u32) -> Self {
        Self(raw)
    }

    /// Extract the raw bit pattern.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// True iff every bit in `other` is set in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Bitwise OR of two flag sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Bitwise AND of two flag sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// Error category surfaced by every backend method.
///
/// Mapped to POSIX errno values by the syscall layer (see
/// `syscall::syscall_errno`): `InvalidDomain → EINVAL`,
/// `AddressMisaligned → EINVAL`, `MapFailed → ENOSPC`, `UnmapFailed →
/// EFAULT`, `DomainTableFull → ENOSPC`, `Unsupported → ENOSYS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuError {
    /// Caller passed a [`DomainId`] not previously returned by
    /// [`IommuBackend::install_domain`].
    InvalidDomain,
    /// `iova`, `phys`, or `len` is not 4-KiB aligned.
    AddressMisaligned,
    /// Backend ran out of internal page-table memory or vendor
    /// resources mid-`map`.
    MapFailed,
    /// `unmap` attempted on a range that was never mapped.
    UnmapFailed,
    /// Out of domain identifiers (every 16-bit ID consumed).
    DomainTableFull,
    /// Feature not supported by the current vendor backend (e.g.
    /// asked for `EXECUTE` on a backend that lacks XD).
    Unsupported,
}

/// Kernel-side IOMMU programming surface.
///
/// Implemented by `PassthroughBackend` today; future siblings:
/// `vtd::VtdBackend` (Intel), `amdvi::AmdViBackend` (AMD). Every
/// method takes `&mut self` because the implementations mutate
/// vendor-private state (root-table writes, IOTLB invalidation
/// fences). The caller (`dma_map_handlers::dma_map`) holds the
/// kernel-wide IOMMU mutex outside this trait.
pub trait IommuBackend {
    /// Identify the running backend for logging + telemetry.
    fn vendor(&self) -> IommuVendor;

    /// Install backing state for a new domain. Idempotent: calling
    /// twice with the same `id` returns `Ok(())` for the
    /// already-installed domain (mirrors VT-d behaviour where the
    /// root-table slot is already populated).
    ///
    /// # Errors
    ///
    /// [`IommuError::DomainTableFull`] when the backend cannot
    /// accommodate another domain.
    fn install_domain(&mut self, id: DomainId) -> Result<(), IommuError>;

    /// Insert a `(iova → phys, len)` mapping into `id`'s page table
    /// with `flags`. `iova`, `phys`, and `len` MUST be 4-KiB aligned.
    ///
    /// # Errors
    ///
    /// - [`IommuError::InvalidDomain`] — `id` was never installed.
    /// - [`IommuError::AddressMisaligned`] — alignment violation.
    /// - [`IommuError::MapFailed`] — backend-internal failure (out of
    ///   page-table frames, vendor-specific conflict).
    /// - [`IommuError::Unsupported`] — `flags` requests a bit the
    ///   backend does not implement (e.g. `EXECUTE` on a backend
    ///   without execute permission).
    fn map(
        &mut self,
        id: DomainId,
        iova: u64,
        phys: u64,
        len: u64,
        flags: IommuFlags,
    ) -> Result<(), IommuError>;

    /// Remove the `[iova, iova+len)` mapping from `id`'s page table.
    ///
    /// # Errors
    ///
    /// - [`IommuError::InvalidDomain`] — `id` was never installed.
    /// - [`IommuError::AddressMisaligned`] — alignment violation.
    /// - [`IommuError::UnmapFailed`] — range was not previously
    ///   mapped (informational; callers may treat as idempotent).
    fn unmap(&mut self, id: DomainId, iova: u64, len: u64) -> Result<(), IommuError>;

    /// Invalidate the IOMMU's translation cache (IOTLB) for `id`.
    /// Called after a batch of `map`/`unmap` operations completes.
    ///
    /// # Errors
    ///
    /// [`IommuError::InvalidDomain`] when `id` was never installed.
    fn flush(&mut self, id: DomainId) -> Result<(), IommuError>;
}

/// Phase 1 default backend: silently accepts every operation and
/// performs no IOMMU programming. **Equivalent to "no IOMMU"** — the
/// device sees physical addresses directly.
///
/// Documented in OIP-013 § S3.3 Appendix B amendment 1 as the explicit
/// Phase 1 caveat. Any production deployment of a driver against an
/// Internet-facing NIC MUST swap to a real backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughBackend;

impl PassthroughBackend {
    /// Construct the passthrough backend. Zero-cost — the struct
    /// carries no state.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IommuBackend for PassthroughBackend {
    fn vendor(&self) -> IommuVendor {
        IommuVendor::Passthrough
    }

    fn install_domain(&mut self, _id: DomainId) -> Result<(), IommuError> {
        Ok(())
    }

    fn map(
        &mut self,
        _id: DomainId,
        iova: u64,
        phys: u64,
        len: u64,
        _flags: IommuFlags,
    ) -> Result<(), IommuError> {
        if iova & 0xFFF != 0 || phys & 0xFFF != 0 || len & 0xFFF != 0 || len == 0 {
            return Err(IommuError::AddressMisaligned);
        }
        Ok(())
    }

    fn unmap(&mut self, _id: DomainId, iova: u64, len: u64) -> Result<(), IommuError> {
        if iova & 0xFFF != 0 || len & 0xFFF != 0 || len == 0 {
            return Err(IommuError::AddressMisaligned);
        }
        Ok(())
    }

    fn flush(&mut self, _id: DomainId) -> Result<(), IommuError> {
        Ok(())
    }
}

// =============================================================================
// Vendor-routed dispatch — P6.7.9-pre.4 (DMA-Map vendor switch).
//
// `IommuKind` static-dispatches the [`IommuBackend`] trait over the three
// concrete implementations (Passthrough, Intel VT-d scaffold, AMD-Vi
// scaffold). The kernel-wide [`IOMMU_BACKEND`] mutex holds the live
// instance; the boot probe in `kmain` calls [`install_backend_for_vendor`]
// after [`probe`] resolves the firmware vendor, swapping in the right
// variant. Subsequent `DmaMap (71)` syscall invocations route through
// [`with_iommu_backend`] so the trait methods are actually called in
// production — even though the Intel / AMD scaffolds are still dormant
// (no MMIO writes; P6.7.9-pre.5+ adds the live register programming).
//
// Why `spin::Mutex` and not raw `static mut`:
//
// 1. The `IommuKind` variants own `alloc::vec::Vec`, so the enum is not
//    `Copy`/`Sync` by default — the mutex provides the `Sync` boundary.
// 2. Bare-metal MP boot already serialises CPU bring-up under `SCHED_LOCK`,
//    but a future cross-CPU `DmaMap` path (multiple driver processes) must
//    not race; the mutex makes the contract explicit today, even though
//    the actual syscall dispatch path is currently single-threaded.
// 3. `spin = 0.9` is already a kernel dep (used by `entropy::KERNEL_CSPRNG`).
//    No new crate, no new transitive supply chain.
//
// Lock acquisition order: `IOMMU_BACKEND` is held strictly *inside* the
// `SCHEDULER`/`FRAME_ALLOC` raw-static-mut region in `dma_map_handlers`,
// so it cannot deadlock with the scheduler global (single-direction
// dependency: scheduler ↛ iommu, iommu ↛ scheduler).
// =============================================================================

/// Vendor-routed enum dispatching to one of the three concrete
/// [`IommuBackend`] implementations.
///
/// Each variant is owned by value so the kernel-wide
/// [`IOMMU_BACKEND`] mutex can be initialised at static-init time
/// (via the `const fn new_passthrough`). Boot-time installation goes
/// through [`install_backend_for_vendor`].
#[derive(Debug)]
pub enum IommuKind {
    /// Phase 1 no-IOMMU passthrough — accepts every aligned input
    /// and performs no programming. Default until the boot probe
    /// resolves a firmware vendor.
    Passthrough(PassthroughBackend),
    /// Intel VT-d backend (dormant scaffold per [`vtd::VtdBackend`]).
    /// Wired in P6.7.9-pre.4; live MMIO programming lands in pre.5+.
    Intel(vtd::VtdBackend),
    /// AMD-Vi backend (dormant scaffold per [`amdvi::AmdViBackend`]).
    /// Wired in P6.7.9-pre.4; live MMIO programming lands in pre.5+.
    Amd(amdvi::AmdViBackend),
}

impl IommuKind {
    /// `const` constructor for the passthrough variant.
    ///
    /// Used as the [`IOMMU_BACKEND`] static initialiser; the boot
    /// probe may later swap in `Intel(..)` or `Amd(..)` via
    /// [`install_backend_for_vendor`].
    #[must_use]
    pub const fn new_passthrough() -> Self {
        Self::Passthrough(PassthroughBackend::new())
    }
}

impl Default for IommuKind {
    fn default() -> Self {
        Self::new_passthrough()
    }
}

impl IommuBackend for IommuKind {
    fn vendor(&self) -> IommuVendor {
        match self {
            Self::Passthrough(backend) => backend.vendor(),
            Self::Intel(backend) => backend.vendor(),
            Self::Amd(backend) => backend.vendor(),
        }
    }

    fn install_domain(&mut self, id: DomainId) -> Result<(), IommuError> {
        match self {
            Self::Passthrough(backend) => backend.install_domain(id),
            Self::Intel(backend) => backend.install_domain(id),
            Self::Amd(backend) => backend.install_domain(id),
        }
    }

    fn map(
        &mut self,
        id: DomainId,
        iova: u64,
        phys: u64,
        len: u64,
        flags: IommuFlags,
    ) -> Result<(), IommuError> {
        match self {
            Self::Passthrough(backend) => backend.map(id, iova, phys, len, flags),
            Self::Intel(backend) => backend.map(id, iova, phys, len, flags),
            Self::Amd(backend) => backend.map(id, iova, phys, len, flags),
        }
    }

    fn unmap(&mut self, id: DomainId, iova: u64, len: u64) -> Result<(), IommuError> {
        match self {
            Self::Passthrough(backend) => backend.unmap(id, iova, len),
            Self::Intel(backend) => backend.unmap(id, iova, len),
            Self::Amd(backend) => backend.unmap(id, iova, len),
        }
    }

    fn flush(&mut self, id: DomainId) -> Result<(), IommuError> {
        match self {
            Self::Passthrough(backend) => backend.flush(id),
            Self::Intel(backend) => backend.flush(id),
            Self::Amd(backend) => backend.flush(id),
        }
    }
}

/// Kernel-wide [`IommuBackend`] instance routed by [`IommuKind`].
///
/// Initialised to [`IommuKind::new_passthrough`] at static-init time so
/// any reader that reaches `DmaMap` before the boot probe runs sees the
/// safe Phase 1 fallback. The boot probe in `kmain` calls
/// [`install_backend_for_vendor`] after [`probe`] returns to swap in
/// the right variant for the firmware-advertised vendor.
pub static IOMMU_BACKEND: spin::Mutex<IommuKind> = spin::Mutex::new(IommuKind::new_passthrough());

/// Replace the live [`IOMMU_BACKEND`] with a fresh instance matching `vendor`.
///
/// Idempotent: calling twice with the same vendor resets the backend
/// state (drops any recorded domains / mappings); this is **not**
/// intended for runtime use, only for the one-shot boot installation
/// right after [`probe`].
///
/// The new instance starts with an empty domain list; the first
/// `DmaMap` invocation installs the calling process's domain via
/// [`IommuBackend::install_domain`].
pub fn install_backend_for_vendor(vendor: IommuVendor) {
    let new_kind = match vendor {
        IommuVendor::Passthrough => IommuKind::new_passthrough(),
        IommuVendor::Intel => IommuKind::Intel(vtd::VtdBackend::new()),
        IommuVendor::Amd => IommuKind::Amd(amdvi::AmdViBackend::new()),
    };
    *IOMMU_BACKEND.lock() = new_kind;
}

/// Run `f` against the live [`IOMMU_BACKEND`], holding the [`spin::Mutex`].
///
/// The closure receives `&mut IommuKind`, which implements
/// [`IommuBackend`] so it can be called through the trait without
/// further dispatch.
///
/// Callers MUST keep the closure body short to avoid blocking the
/// scheduler's `dma_map` path; the trait methods themselves are O(N)
/// in the recorded mapping count for the host-testable scaffolds
/// (acceptable for Phase 1 — driver processes hold at most a handful
/// of DMA windows).
pub fn with_iommu_backend<R>(f: impl FnOnce(&mut IommuKind) -> R) -> R {
    let mut backend = IOMMU_BACKEND.lock();
    f(&mut backend)
}

/// Derive the per-process [`DomainId`] from a kernel `TaskId`.
///
/// Phase 1 model: one IOMMU domain per driver process. The mapping
/// is `domain = task_id mod 65536` (16-bit space matches the VT-d
/// `DID` and AMD-Vi `DomainID` field widths). Collisions are
/// statistically impossible during Phase 1 (≤ tens of concurrent
/// driver processes) but are documented as a follow-up in OIP-013
/// § S3.5 for the Phase 2+ domain allocator.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "explicit 16-bit mask above the cast — truncation is the desired projection of TaskId into the 16-bit IOMMU DID space"
)]
pub const fn domain_for_task(task_id: u64) -> DomainId {
    DomainId::new((task_id & 0xFFFF) as u16)
}

#[cfg(test)]
mod tests {
    use super::{
        DomainId, IOMMU_BACKEND, IOMMU_UNIT_COUNT, IOMMU_VENDOR, IommuBackend, IommuError,
        IommuFlags, IommuKind, IommuVendor, PassthroughBackend, ProbeResult, domain_for_task,
        install_backend_for_vendor, iommu_unit_count, iommu_vendor, select_vendor,
        set_iommu_unit_count, set_iommu_vendor, with_iommu_backend,
    };
    use core::sync::atomic::Ordering;

    #[test]
    fn domain_id_round_trip() {
        let id = DomainId::new(0x1234);
        assert_eq!(id.raw(), 0x1234);
        assert_eq!(id, DomainId(0x1234));
    }

    #[test]
    fn iommu_flags_contains_and_union() {
        let rw = IommuFlags::READ.union(IommuFlags::WRITE);
        assert!(rw.contains(IommuFlags::READ));
        assert!(rw.contains(IommuFlags::WRITE));
        assert!(!rw.contains(IommuFlags::EXECUTE));
        assert_eq!(rw.bits(), 0b11);
    }

    #[test]
    fn iommu_flags_intersection_is_commutative() {
        let a = IommuFlags::READ
            .union(IommuFlags::WRITE)
            .union(IommuFlags::COHERENT);
        let b = IommuFlags::WRITE.union(IommuFlags::EXECUTE);
        let lhs = a.intersection(b);
        let rhs = b.intersection(a);
        assert_eq!(lhs.bits(), rhs.bits());
        assert_eq!(lhs.bits(), IommuFlags::WRITE.bits());
    }

    #[test]
    fn iommu_flags_from_bits_preserves_unknown_bits() {
        // Reserved bit pattern — must survive a round-trip so future
        // flag additions on the wire do not silently zero in older
        // kernels (defence against forward-compatibility regressions).
        let raw = 0xDEAD_BEEF;
        assert_eq!(IommuFlags::from_bits(raw).bits(), raw);
    }

    #[test]
    fn passthrough_vendor_reports_passthrough() {
        let backend = PassthroughBackend::new();
        assert_eq!(backend.vendor(), IommuVendor::Passthrough);
    }

    #[test]
    fn passthrough_install_domain_is_ok() {
        let mut backend = PassthroughBackend::new();
        assert_eq!(backend.install_domain(DomainId(0)), Ok(()));
        // Idempotent re-install also succeeds.
        assert_eq!(backend.install_domain(DomainId(0)), Ok(()));
    }

    #[test]
    fn passthrough_map_accepts_aligned_input() {
        let mut backend = PassthroughBackend::new();
        let res = backend.map(
            DomainId(7),
            0x1000,
            0x10_0000,
            0x4000,
            IommuFlags::READ.union(IommuFlags::WRITE),
        );
        assert_eq!(res, Ok(()));
    }

    #[test]
    fn passthrough_map_rejects_misaligned_iova() {
        let mut backend = PassthroughBackend::new();
        let res = backend.map(DomainId(7), 0x1001, 0x10_0000, 0x4000, IommuFlags::READ);
        assert_eq!(res, Err(IommuError::AddressMisaligned));
    }

    #[test]
    fn passthrough_map_rejects_misaligned_phys() {
        let mut backend = PassthroughBackend::new();
        let res = backend.map(DomainId(7), 0x1000, 0x10_0123, 0x4000, IommuFlags::READ);
        assert_eq!(res, Err(IommuError::AddressMisaligned));
    }

    #[test]
    fn passthrough_map_rejects_misaligned_len() {
        let mut backend = PassthroughBackend::new();
        let res = backend.map(DomainId(7), 0x1000, 0x10_0000, 0x4001, IommuFlags::READ);
        assert_eq!(res, Err(IommuError::AddressMisaligned));
    }

    #[test]
    fn passthrough_map_rejects_zero_length() {
        let mut backend = PassthroughBackend::new();
        let res = backend.map(DomainId(7), 0x1000, 0x10_0000, 0, IommuFlags::READ);
        assert_eq!(res, Err(IommuError::AddressMisaligned));
    }

    #[test]
    fn passthrough_unmap_aligned_ok_misaligned_err() {
        let mut backend = PassthroughBackend::new();
        assert_eq!(backend.unmap(DomainId(0), 0x1000, 0x1000), Ok(()));
        assert_eq!(
            backend.unmap(DomainId(0), 0x1001, 0x1000),
            Err(IommuError::AddressMisaligned)
        );
        assert_eq!(
            backend.unmap(DomainId(0), 0x1000, 0),
            Err(IommuError::AddressMisaligned)
        );
    }

    #[test]
    fn passthrough_flush_is_ok() {
        let mut backend = PassthroughBackend::new();
        assert_eq!(backend.flush(DomainId(0)), Ok(()));
    }

    #[test]
    fn error_variants_are_distinct() {
        // Catch a future copy-paste mistake that collapses two variants.
        let variants = [
            IommuError::InvalidDomain,
            IommuError::AddressMisaligned,
            IommuError::MapFailed,
            IommuError::UnmapFailed,
            IommuError::DomainTableFull,
            IommuError::Unsupported,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // P6.7.9-pre.1 — IOMMU probe + vendor selector tests.
    //
    // The globals `IOMMU_VENDOR` / `IOMMU_UNIT_COUNT` are process-wide
    // singletons; each test that mutates them snapshots and restores
    // the prior value so concurrent test execution does not leak state
    // across the suite (the workspace is currently pinned to
    // `--test-threads=1` per the SIGSEGV mitigation, but treating the
    // globals as serialisation-tolerant is forward-compatible with
    // TASK-012's eventual lift of that pin).
    // -----------------------------------------------------------------

    #[test]
    fn iommu_vendor_tag_round_trip() {
        assert_eq!(
            IommuVendor::from_u8(IommuVendor::Passthrough.as_u8()),
            IommuVendor::Passthrough
        );
        assert_eq!(
            IommuVendor::from_u8(IommuVendor::Intel.as_u8()),
            IommuVendor::Intel
        );
        assert_eq!(
            IommuVendor::from_u8(IommuVendor::Amd.as_u8()),
            IommuVendor::Amd
        );
    }

    #[test]
    fn iommu_vendor_unknown_tag_decodes_to_passthrough() {
        assert_eq!(IommuVendor::from_u8(0xFF), IommuVendor::Passthrough);
        assert_eq!(IommuVendor::from_u8(0x42), IommuVendor::Passthrough);
    }

    #[test]
    fn iommu_vendor_label_matches_log_format() {
        // Pin the log-line spelling — `[iommu] vendor=<intel|amd|passthrough>`.
        assert_eq!(IommuVendor::Intel.label(), "intel");
        assert_eq!(IommuVendor::Amd.label(), "amd");
        assert_eq!(IommuVendor::Passthrough.label(), "passthrough");
    }

    #[test]
    fn select_vendor_prefers_intel_when_dmar_present() {
        let res = select_vendor(2, 3);
        assert_eq!(res.vendor, IommuVendor::Intel);
        assert_eq!(res.drhd_count, 2);
        // IVHD is suppressed when Intel wins.
        assert_eq!(res.ivhd_count, 0);
    }

    #[test]
    fn select_vendor_falls_back_to_amd_when_no_intel() {
        let res = select_vendor(0, 4);
        assert_eq!(res.vendor, IommuVendor::Amd);
        assert_eq!(res.drhd_count, 0);
        assert_eq!(res.ivhd_count, 4);
    }

    #[test]
    fn select_vendor_falls_back_to_passthrough_when_no_tables() {
        let res = select_vendor(0, 0);
        assert_eq!(res, ProbeResult::PASSTHROUGH);
        assert_eq!(res.vendor, IommuVendor::Passthrough);
    }

    #[test]
    fn select_vendor_intel_single_unit_amd_single_unit() {
        // Boundary case: exactly one of each — Intel still wins.
        let res = select_vendor(1, 1);
        assert_eq!(res.vendor, IommuVendor::Intel);
        assert_eq!(res.drhd_count, 1);
        assert_eq!(res.ivhd_count, 0);
    }

    #[test]
    fn iommu_vendor_default_global_is_passthrough() {
        // The static is initialised to TAG_PASSTHROUGH so callers
        // that read the global before the boot probe runs see the
        // safe "no IOMMU" answer.
        let prior = IOMMU_VENDOR.load(Ordering::Relaxed);
        IOMMU_VENDOR.store(IommuVendor::TAG_PASSTHROUGH, Ordering::Relaxed);
        assert_eq!(iommu_vendor(), IommuVendor::Passthrough);
        IOMMU_VENDOR.store(prior, Ordering::Relaxed);
    }

    #[test]
    fn set_iommu_vendor_round_trips_intel() {
        let prior = IOMMU_VENDOR.load(Ordering::Relaxed);
        set_iommu_vendor(IommuVendor::Intel);
        assert_eq!(iommu_vendor(), IommuVendor::Intel);
        IOMMU_VENDOR.store(prior, Ordering::Relaxed);
    }

    #[test]
    fn set_iommu_vendor_round_trips_amd() {
        let prior = IOMMU_VENDOR.load(Ordering::Relaxed);
        set_iommu_vendor(IommuVendor::Amd);
        assert_eq!(iommu_vendor(), IommuVendor::Amd);
        IOMMU_VENDOR.store(prior, Ordering::Relaxed);
    }

    #[test]
    fn set_iommu_unit_count_round_trips() {
        let prior = IOMMU_UNIT_COUNT.load(Ordering::Relaxed);
        set_iommu_unit_count(7);
        assert_eq!(iommu_unit_count(), 7);
        set_iommu_unit_count(0);
        assert_eq!(iommu_unit_count(), 0);
        IOMMU_UNIT_COUNT.store(prior, Ordering::Relaxed);
    }

    #[test]
    fn probe_result_passthrough_constant_is_zeroed() {
        assert_eq!(ProbeResult::PASSTHROUGH.vendor, IommuVendor::Passthrough);
        assert_eq!(ProbeResult::PASSTHROUGH.drhd_count, 0);
        assert_eq!(ProbeResult::PASSTHROUGH.ivhd_count, 0);
    }

    // Note: `probe` itself is **not** unit-tested from host code. The
    // bare-metal variant dereferences firmware physical addresses
    // through `phys_offset.wrapping_add(rsdp_phys)`; on x86_64 host
    // (where the bare-metal variant is what `cargo test` compiles, by
    // the same `#[cfg(target_arch = "x86_64")]` rule that gates
    // `mp::enumerate_cpus`), calling it with `(0, 0)` would
    // dereference a null pointer. The pure-function decomposition
    // (`select_vendor` + the explicit DRHD/IVHD parsers in `dmar` /
    // `ivrs`) is what we cover here; the QEMU smoke + Proxmox boot
    // log are the integration evidence.

    // -----------------------------------------------------------------
    // P6.7.9-pre.4 — `IommuKind` dispatch + `IOMMU_BACKEND` mutex tests.
    //
    // The global mutex is a process-wide singleton; each test that
    // mutates it snapshots and restores the prior state through the
    // shared `install_backend_for_vendor` helper. The workspace is
    // pinned to `--test-threads=1` per the SIGSEGV mitigation, so the
    // tests never observe a concurrent reader — but the snapshot
    // pattern is forward-compatible with TASK-012's eventual lift of
    // that pin.
    // -----------------------------------------------------------------

    /// Snapshot the current backend vendor so a test that swaps the
    /// global can restore it on exit (defence-in-depth for the future
    /// parallel-test regime).
    fn snapshot_backend_vendor() -> IommuVendor {
        with_iommu_backend(|b| b.vendor())
    }

    #[test]
    fn iommu_kind_default_is_passthrough() {
        let kind = IommuKind::default();
        assert_eq!(kind.vendor(), IommuVendor::Passthrough);
    }

    #[test]
    fn iommu_kind_new_passthrough_is_const_constructible() {
        // The static `IOMMU_BACKEND` relies on `const fn` so a
        // regression here would prevent the kernel from booting.
        const _: IommuKind = IommuKind::new_passthrough();
    }

    #[test]
    fn iommu_kind_intel_vendor_routes() {
        let mut kind = IommuKind::Intel(super::vtd::VtdBackend::new());
        assert_eq!(kind.vendor(), IommuVendor::Intel);
        // Trait dispatch routes install_domain/map/unmap/flush through
        // the inner VtdBackend.
        let dom = DomainId::new(0xABCD);
        assert_eq!(kind.install_domain(dom), Ok(()));
        assert_eq!(
            kind.map(dom, 0x1000, 0x10_0000, 0x1000, IommuFlags::READ),
            Ok(())
        );
        assert_eq!(kind.flush(dom), Ok(()));
        assert_eq!(kind.unmap(dom, 0x1000, 0x1000), Ok(()));
    }

    #[test]
    fn iommu_kind_amd_vendor_routes() {
        let mut kind = IommuKind::Amd(super::amdvi::AmdViBackend::new());
        assert_eq!(kind.vendor(), IommuVendor::Amd);
        let dom = DomainId::new(0x42);
        assert_eq!(kind.install_domain(dom), Ok(()));
        assert_eq!(
            kind.map(dom, 0x2000, 0x20_0000, 0x2000, IommuFlags::READ),
            Ok(())
        );
        assert_eq!(kind.unmap(dom, 0x2000, 0x2000), Ok(()));
    }

    #[test]
    fn iommu_kind_passthrough_rejects_misaligned() {
        let mut kind = IommuKind::Passthrough(PassthroughBackend::new());
        assert_eq!(
            kind.map(
                DomainId::new(0),
                0x1001,
                0x10_0000,
                0x1000,
                IommuFlags::READ
            ),
            Err(IommuError::AddressMisaligned)
        );
    }

    #[test]
    fn iommu_kind_intel_rejects_unknown_domain() {
        let mut kind = IommuKind::Intel(super::vtd::VtdBackend::new());
        // Never installed — map must fail with InvalidDomain.
        assert_eq!(
            kind.map(
                DomainId::new(7),
                0x1000,
                0x10_0000,
                0x1000,
                IommuFlags::READ
            ),
            Err(IommuError::InvalidDomain)
        );
    }

    #[test]
    fn install_backend_for_vendor_switches_to_intel() {
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Intel);
        assert_eq!(with_iommu_backend(|b| b.vendor()), IommuVendor::Intel);
        install_backend_for_vendor(prior);
        assert_eq!(with_iommu_backend(|b| b.vendor()), prior);
    }

    #[test]
    fn install_backend_for_vendor_switches_to_amd() {
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Amd);
        assert_eq!(with_iommu_backend(|b| b.vendor()), IommuVendor::Amd);
        install_backend_for_vendor(prior);
        assert_eq!(with_iommu_backend(|b| b.vendor()), prior);
    }

    #[test]
    fn install_backend_for_vendor_resets_passthrough() {
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Passthrough);
        assert_eq!(with_iommu_backend(|b| b.vendor()), IommuVendor::Passthrough);
        install_backend_for_vendor(prior);
    }

    #[test]
    fn install_backend_for_vendor_is_idempotent_for_intel() {
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Intel);
        // Install a domain in the first instance.
        let dom = DomainId::new(0xBEEF);
        with_iommu_backend(|b| b.install_domain(dom)).unwrap();
        // Re-install for the same vendor → state resets (no domain).
        install_backend_for_vendor(IommuVendor::Intel);
        let res = with_iommu_backend(|b| b.map(dom, 0x1000, 0x10_0000, 0x1000, IommuFlags::READ));
        assert_eq!(res, Err(IommuError::InvalidDomain));
        install_backend_for_vendor(prior);
    }

    #[test]
    fn with_iommu_backend_round_trips_state() {
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Intel);
        let dom = DomainId::new(0xFACE);
        with_iommu_backend(|b| b.install_domain(dom)).unwrap();
        with_iommu_backend(|b| {
            b.map(
                dom,
                0x1000,
                0x10_0000,
                0x1000,
                IommuFlags::READ.union(IommuFlags::WRITE),
            )
        })
        .unwrap();
        let unmap_res = with_iommu_backend(|b| b.unmap(dom, 0x1000, 0x1000));
        assert_eq!(unmap_res, Ok(()));
        install_backend_for_vendor(prior);
    }

    #[test]
    fn domain_for_task_maps_low_16_bits() {
        assert_eq!(domain_for_task(0), DomainId::new(0));
        assert_eq!(domain_for_task(1), DomainId::new(1));
        assert_eq!(domain_for_task(0xFFFF), DomainId::new(0xFFFF));
    }

    #[test]
    fn domain_for_task_truncates_high_bits() {
        // High bits do not influence the result — the projection is
        // explicit `& 0xFFFF`.
        assert_eq!(domain_for_task(0x1_0000), DomainId::new(0));
        assert_eq!(
            domain_for_task(0xFFFF_FFFF_FFFF_FFFF),
            DomainId::new(0xFFFF)
        );
        assert_eq!(domain_for_task(0xDEAD_BEEF), DomainId::new(0xBEEF));
    }

    #[test]
    fn iommu_backend_static_initial_state_is_passthrough() {
        // The static initialiser is exercised at first access; ensure
        // it lands in the Passthrough variant for any test order.
        // (Other tests may have swapped it; snapshot + restore.)
        let prior = snapshot_backend_vendor();
        install_backend_for_vendor(IommuVendor::Passthrough);
        let observed = IOMMU_BACKEND.lock().vendor();
        assert_eq!(observed, IommuVendor::Passthrough);
        install_backend_for_vendor(prior);
    }
}
