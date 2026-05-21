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

pub mod dmar;
pub mod domain;
pub mod ivrs;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuVendor {
    /// Intel VT-d backend (`vtd` module, lands P6.7.9-pre.1).
    Intel,
    /// AMD-Vi backend (`amdvi` module, lands P6.7.9-pre.1).
    Amd,
    /// Phase 1 passthrough mode: no IOMMU programming, `iova == phys`.
    /// Documented in OIP-013 § S3.3 Appendix B amendment 1.
    Passthrough,
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

#[cfg(test)]
mod tests {
    use super::{DomainId, IommuBackend, IommuError, IommuFlags, IommuVendor, PassthroughBackend};

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
}
