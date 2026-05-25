//! `x86_64` 4-level page-table walker (Track B, MB2).
//!
//! Provides read access to the bootloader-installed page tables and a
//! `map_4k` / `unmap_4k` interface for adding kernel mappings in the
//! direct-mapped physical window. Does **not** write to CR3 — the
//! bootloader page tables remain the active tables; this module adds
//! new mappings within the identity map that the bootloader already
//! installed.
//!
//! ## Direct-map convention
//!
//! `bootloader 0.11` with `Mapping::Dynamic` maps all physical memory
//! at a virtual offset `physical_memory_offset` (from `BootInfo`).
//! Every physical address `p` is accessible at virtual address
//! `p + phys_offset`. `PageMapper` uses this window to traverse and
//! mutate page-table structures without a recursive mapping.
//!
//! ## Huge-page handling
//!
//! `translate` follows huge-page entries (PS=1) at both PDPT (1 GiB)
//! and PD (2 MiB) levels and returns the correct physical address using
//! the appropriate page offset (30 bits for 1 GiB, 21 bits for 2 MiB).
//! This is required because `bootloader 0.11` installs the linear
//! direct-map of physical memory using 1 GiB / 2 MiB pages; without
//! huge-page traversal, `translate` would report most of physical RAM
//! as unmapped even though the CPU resolves it correctly.
//!
//! `map_4k` operates only on 4 KiB entries: it does not currently split
//! a huge-page mapping that already covers the target address. Callers
//! that need to override a huge-page-backed region must split it
//! externally (out of scope for MB9).

#![allow(
    unsafe_code,
    reason = "page-table walker reads/writes raw frame pointers; SAFETY per fn"
)]

use crate::memory::{BitmapFrameAllocator, PhysAddr, VirtAddr};

// -----------------------------------------------------------------------
// PTE flag constants
// -----------------------------------------------------------------------

/// Page is present in memory.
pub const PTE_PRESENT: u64 = 1 << 0;
/// Page is writable.
pub const PTE_WRITABLE: u64 = 1 << 1;
/// Page is accessible from user mode (ring 3).
pub const PTE_USER: u64 = 1 << 2;
/// Page size: at PD level marks a 2 MiB page, at PDPT level marks a 1 GiB page.
///
/// Reserved (must be 0) in PML4E and PTE; ignored by hardware when set there.
pub const PTE_HUGE: u64 = 1 << 7;
/// Execute-disable bit (requires `IA32_EFER.NXE` to be set).
pub const PTE_NO_EXEC: u64 = 1 << 63;

// -----------------------------------------------------------------------
// Huge-page frame masks
//
// PS=1 entries encode the frame at coarser-than-4K granularity:
//   PDPTE 1 GiB → bits [51:30] are the 1 GiB-aligned physical frame.
//   PDE    2 MiB → bits [51:21] are the 2 MiB-aligned physical frame.
// These constants isolate those frame bits; the corresponding offset is
// the low (30 or 21) bits of the virtual address.
// -----------------------------------------------------------------------

/// Frame mask for a PDPTE with PS=1 (1 GiB page): bits \[51:30\].
const HUGE_1G_FRAME_MASK: u64 = 0x000F_FFFF_C000_0000;
/// Offset mask within a 1 GiB page: bits \[29:0\].
const HUGE_1G_OFFSET_MASK: u64 = 0x0000_0000_3FFF_FFFF;
/// Frame mask for a PDE with PS=1 (2 MiB page): bits \[51:21\].
const HUGE_2M_FRAME_MASK: u64 = 0x000F_FFFF_FFE0_0000;
/// Offset mask within a 2 MiB page: bits \[20:0\].
const HUGE_2M_OFFSET_MASK: u64 = 0x0000_0000_001F_FFFF;

// -----------------------------------------------------------------------
// PageTableEntry
// -----------------------------------------------------------------------

/// A single `x86_64` page-table entry (8 bytes).
///
/// Bits \[11:0\] are flags; bits \[51:12\] are the physical frame address
/// (4 KiB-aligned); bit 63 is the NX flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    /// Returns `true` if the PRESENT bit (bit 0) is set.
    #[inline]
    #[must_use]
    pub const fn is_present(self) -> bool {
        self.0 & PTE_PRESENT != 0
    }

    /// Extracts the physical frame address (bits \[51:12\]).
    ///
    /// The low 12 flag bits and high reserved bits are masked out.
    #[inline]
    #[must_use]
    pub const fn phys_addr(self) -> PhysAddr {
        PhysAddr(self.0 & 0x000F_FFFF_FFFF_F000)
    }

    /// Sets this entry to `frame | flags`. `frame` must be 4 KiB-aligned;
    /// excess low bits are masked away before writing.
    #[inline]
    pub fn set_frame(&mut self, frame: PhysAddr, flags: u64) {
        self.0 = (frame.0 & 0x000F_FFFF_FFFF_F000) | flags;
    }

    /// Clears the entry (marks as not-present, zeroes all bits).
    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

// -----------------------------------------------------------------------
// RawPageTable
// -----------------------------------------------------------------------

/// A 512-entry `x86_64` page table (PML4, PDPT, PD, or PT level).
///
/// Aligned to 4 KiB so a single instance fits exactly in one physical
/// frame. `repr(C)` ensures a predictable field layout with no padding.
#[repr(C, align(4096))]
pub struct RawPageTable {
    entries: [PageTableEntry; 512],
}

impl RawPageTable {
    /// Returns a zeroed (all-not-present) page table.
    pub const fn empty() -> Self {
        Self {
            entries: [PageTableEntry(0); 512],
        }
    }

    /// Returns the entry at `idx` by value.
    #[inline]
    #[must_use]
    #[allow(
        clippy::indexing_slicing,
        reason = "idx is always 0..512, produced by virt_index()"
    )]
    pub fn entry(&self, idx: usize) -> PageTableEntry {
        self.entries[idx]
    }

    /// Returns a mutable reference to the entry at `idx`.
    #[inline]
    #[allow(
        clippy::indexing_slicing,
        reason = "idx is always 0..512, produced by virt_index()"
    )]
    pub fn entry_mut(&mut self, idx: usize) -> &mut PageTableEntry {
        &mut self.entries[idx]
    }
}

// -----------------------------------------------------------------------
// PageLevel
// -----------------------------------------------------------------------

/// The four `x86_64` paging levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLevel {
    /// Level 4: Page Map Level 4.
    Pml4,
    /// Level 3: Page Directory Pointer Table.
    Pdpt,
    /// Level 2: Page Directory.
    Pd,
    /// Level 1: Page Table (leaf level for 4 KiB pages).
    Pt,
}

/// Extracts the 9-bit index into the page table at `level` from `virt`.
///
/// | Level | Bits    | Shift |
/// |-------|---------|-------|
/// | PML4  | \[47:39\] | 39    |
/// | PDPT  | \[38:30\] | 30    |
/// | PD    | \[29:21\] | 21    |
/// | PT    | \[20:12\] | 12    |
#[inline]
#[must_use]
pub const fn virt_index(virt: VirtAddr, level: PageLevel) -> usize {
    let shift = match level {
        PageLevel::Pml4 => 39u64,
        PageLevel::Pdpt => 30u64,
        PageLevel::Pd => 21u64,
        PageLevel::Pt => 12u64,
    };
    ((virt.0 >> shift) & 0x1FF) as usize
}

// -----------------------------------------------------------------------
// PageMapper
// -----------------------------------------------------------------------

/// Page-table mapper backed by the bootloader's direct physical-memory map.
///
/// `phys_offset` is `BootInfo.physical_memory_offset` — the virtual address
/// at which the entire physical address space is mapped linearly. Physical
/// address `p` is accessible at `phys_offset + p`. `root_phys` is the
/// physical address of the PML4 table read from CR3.
///
/// ## Safety invariant
///
/// The caller must ensure `phys_offset` correctly describes the direct map
/// and that `root_phys` is the physical address of the active PML4 table.
/// Using incorrect values will dereference arbitrary virtual memory.
pub struct PageMapper {
    phys_offset: u64,
    /// Physical address of the PML4 root table (from CR3).
    pub root_phys: PhysAddr,
}

impl PageMapper {
    /// Constructs a mapper from the bootloader's direct-map offset and
    /// the physical address of the PML4 (from CR3, low 12 bits masked).
    #[must_use]
    pub fn new(phys_offset: u64, root_phys: PhysAddr) -> Self {
        Self {
            phys_offset,
            root_phys,
        }
    }

    /// Direct-map offset (`BootInfo.physical_memory_offset`). Used by
    /// `super::address_space::AddressSpace` to read/write raw page-table
    /// frames through the bootloader's direct map.
    #[must_use]
    pub const fn phys_offset(&self) -> u64 {
        self.phys_offset
    }

    /// Translates a virtual address to a physical address by walking the
    /// active 4-level page tables.
    ///
    /// Follows PS=1 entries at PDPT (1 GiB) and PD (2 MiB) levels and
    /// returns the physical address with the appropriate page offset.
    /// Returns `None` only if any traversed entry is not-present.
    #[must_use]
    #[allow(
        clippy::similar_names,
        reason = "page-table level variable names are intentionally terse"
    )]
    /// Translate a VA using an explicit page-table root (e.g. a per-process PML4).
    pub fn translate_in(&self, root: PhysAddr, virt: VirtAddr) -> Option<PhysAddr> {
        let pml4 = self.table_ptr(root);
        let pml4e = unsafe { (*pml4).entry(virt_index(virt, PageLevel::Pml4)) };
        if !pml4e.is_present() { return None; }
        let pdpt = self.table_ptr(pml4e.phys_addr());
        let pdpte = unsafe { (*pdpt).entry(virt_index(virt, PageLevel::Pdpt)) };
        if !pdpte.is_present() { return None; }
        if pdpte.0 & PTE_HUGE != 0 {
            return Some(PhysAddr((pdpte.0 & HUGE_1G_FRAME_MASK) + (virt.0 & HUGE_1G_OFFSET_MASK)));
        }
        let pd = self.table_ptr(pdpte.phys_addr());
        let pde = unsafe { (*pd).entry(virt_index(virt, PageLevel::Pd)) };
        if !pde.is_present() { return None; }
        if pde.0 & PTE_HUGE != 0 {
            return Some(PhysAddr((pde.0 & HUGE_2M_FRAME_MASK) + (virt.0 & HUGE_2M_OFFSET_MASK)));
        }
        let pt = self.table_ptr(pde.phys_addr());
        let pte = unsafe { (*pt).entry(virt_index(virt, PageLevel::Pt)) };
        if !pte.is_present() { return None; }
        Some(PhysAddr(pte.phys_addr().0 + (virt.0 & 0xFFF)))
    }

    /// Translate a VA through the mapper's own root page table.
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let pml4 = self.table_ptr(self.root_phys);
        let pml4e = unsafe { (*pml4).entry(virt_index(virt, PageLevel::Pml4)) };
        if !pml4e.is_present() {
            return None;
        }

        let pdpt = self.table_ptr(pml4e.phys_addr());
        let pdpte = unsafe { (*pdpt).entry(virt_index(virt, PageLevel::Pdpt)) };
        if !pdpte.is_present() {
            return None;
        }
        if pdpte.0 & PTE_HUGE != 0 {
            // 1 GiB page: frame is bits [51:30], offset is the low 30 bits of virt.
            let frame = pdpte.0 & HUGE_1G_FRAME_MASK;
            let offset = virt.0 & HUGE_1G_OFFSET_MASK;
            return Some(PhysAddr(frame + offset));
        }

        let pd = self.table_ptr(pdpte.phys_addr());
        let pde = unsafe { (*pd).entry(virt_index(virt, PageLevel::Pd)) };
        if !pde.is_present() {
            return None;
        }
        if pde.0 & PTE_HUGE != 0 {
            // 2 MiB page: frame is bits [51:21], offset is the low 21 bits of virt.
            let frame = pde.0 & HUGE_2M_FRAME_MASK;
            let offset = virt.0 & HUGE_2M_OFFSET_MASK;
            return Some(PhysAddr(frame + offset));
        }

        let pt = self.table_ptr(pde.phys_addr());
        let pte = unsafe { (*pt).entry(virt_index(virt, PageLevel::Pt)) };
        if !pte.is_present() {
            return None;
        }

        let page_offset = virt.0 & 0xFFF;
        Some(PhysAddr(pte.phys_addr().0 + page_offset))
    }

    /// Maps the 4 KiB page at `virt` to physical frame `phys` with `flags`,
    /// using `self.root_phys` as the root PML4. Thin wrapper around
    /// [`Self::map_4k_into`] for the common case of mapping into the
    /// active address space.
    pub fn map_4k<const N: usize>(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: u64,
        alloc: &mut BitmapFrameAllocator<N>,
    ) -> bool {
        let root = self.root_phys;
        self.map_4k_into(root, virt, phys, flags, alloc)
    }

    /// Maps the 4 KiB page at `virt` to physical frame `phys` with `flags`
    /// in the page-table tree rooted at `root_phys`.
    ///
    /// MB11: required by [`super::address_space::AddressSpace`] to map
    /// pages into per-process PML4s without mutating `self.root_phys`.
    ///
    /// Intermediate page-table frames (PDPT, PD, PT) are allocated from
    /// `alloc` as needed; they are always mapped with PRESENT | WRITABLE.
    ///
    /// Returns `false` if:
    /// - the allocator could not provide a frame for a missing table,
    /// - the target page was already mapped.
    #[allow(
        clippy::similar_names,
        reason = "page-table level variable names are intentionally terse"
    )]
    pub fn map_4k_into<const N: usize>(
        &mut self,
        root_phys: PhysAddr,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: u64,
        alloc: &mut BitmapFrameAllocator<N>,
    ) -> bool {
        let pml4_idx = virt_index(virt, PageLevel::Pml4);
        let pdpt_idx = virt_index(virt, PageLevel::Pdpt);
        let pd_idx = virt_index(virt, PageLevel::Pd);
        let pt_idx = virt_index(virt, PageLevel::Pt);

        // Intermediate page-table entries (PML4E, PDPTE, PDE) must carry
        // PTE_USER when the leaf PTE is user-accessible, otherwise the CPU
        // raises #PF on any Ring 3 access regardless of the leaf flags.
        let intermediate_flags = if flags & PTE_USER != 0 {
            PTE_PRESENT | PTE_WRITABLE | PTE_USER
        } else {
            PTE_PRESENT | PTE_WRITABLE
        };

        // PML4 → PDPT
        let pml4 = self.table_ptr_mut(root_phys);
        let pdpt_phys = {
            let e = unsafe { (*pml4).entry(pml4_idx) };
            if e.is_present() {
                // If the entry exists but lacks USER and we need it, upgrade.
                if flags & PTE_USER != 0 && e.0 & PTE_USER == 0 {
                    unsafe {
                        (*pml4)
                            .entry_mut(pml4_idx)
                            .set_frame(e.phys_addr(), intermediate_flags);
                    }
                }
                e.phys_addr()
            } else {
                let Some(f) = alloc.alloc_frame() else {
                    return false;
                };
                unsafe {
                    self.zero_table(f);
                }
                unsafe {
                    (*pml4)
                        .entry_mut(pml4_idx)
                        .set_frame(f, intermediate_flags);
                }
                f
            }
        };

        // PDPT → PD
        let pdpt = self.table_ptr_mut(pdpt_phys);
        let pd_phys = {
            let e = unsafe { (*pdpt).entry(pdpt_idx) };
            if e.is_present() {
                if flags & PTE_USER != 0 && e.0 & PTE_USER == 0 {
                    unsafe {
                        (*pdpt)
                            .entry_mut(pdpt_idx)
                            .set_frame(e.phys_addr(), intermediate_flags);
                    }
                }
                e.phys_addr()
            } else {
                let Some(f) = alloc.alloc_frame() else {
                    return false;
                };
                unsafe {
                    self.zero_table(f);
                }
                unsafe {
                    (*pdpt)
                        .entry_mut(pdpt_idx)
                        .set_frame(f, intermediate_flags);
                }
                f
            }
        };

        // PD → PT
        let pd = self.table_ptr_mut(pd_phys);
        let pt_phys = {
            let e = unsafe { (*pd).entry(pd_idx) };
            if e.is_present() {
                if flags & PTE_USER != 0 && e.0 & PTE_USER == 0 {
                    unsafe {
                        (*pd)
                            .entry_mut(pd_idx)
                            .set_frame(e.phys_addr(), intermediate_flags);
                    }
                }
                e.phys_addr()
            } else {
                let Some(f) = alloc.alloc_frame() else {
                    return false;
                };
                unsafe {
                    self.zero_table(f);
                }
                unsafe {
                    (*pd)
                        .entry_mut(pd_idx)
                        .set_frame(f, intermediate_flags);
                }
                f
            }
        };

        // Leaf PT entry
        let pt = self.table_ptr_mut(pt_phys);
        let existing = unsafe { (*pt).entry(pt_idx) };
        if existing.is_present() {
            return false;
        } // already mapped
        unsafe { (*pt).entry_mut(pt_idx).set_frame(phys, flags | PTE_PRESENT) };
        true
    }

    /// Unmaps the 4 KiB page at `virt` and invalidates the TLB entry.
    ///
    /// Returns `false` if the page was not present (no-op).
    #[allow(
        clippy::similar_names,
        reason = "page-table level variable names are intentionally terse"
    )]
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "mutation occurs through raw pointers in the direct-map window"
    )]
    pub fn unmap_4k(&mut self, virt: VirtAddr) -> bool {
        let pml4 = self.table_ptr(self.root_phys);
        let pml4e = unsafe { (*pml4).entry(virt_index(virt, PageLevel::Pml4)) };
        if !pml4e.is_present() {
            return false;
        }

        let pdpt = self.table_ptr(pml4e.phys_addr());
        let pdpte = unsafe { (*pdpt).entry(virt_index(virt, PageLevel::Pdpt)) };
        if !pdpte.is_present() {
            return false;
        }

        let pd = self.table_ptr(pdpte.phys_addr());
        let pde = unsafe { (*pd).entry(virt_index(virt, PageLevel::Pd)) };
        if !pde.is_present() {
            return false;
        }

        let pt = self.table_ptr_mut(pde.phys_addr());
        let pte = unsafe { (*pt).entry(virt_index(virt, PageLevel::Pt)) };
        if !pte.is_present() {
            return false;
        }

        unsafe {
            (*pt).entry_mut(virt_index(virt, PageLevel::Pt)).clear();
        }
        // Invalidate TLB — only meaningful on bare-metal; no-op on non-x86 hosts.
        #[cfg(all(target_arch = "x86_64", not(test)))]
        unsafe {
            super::arch::invlpg(virt.0);
        }
        true
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Converts a physical address to a `*const RawPageTable` via the
    /// direct-map window: `phys_offset + p`.
    #[inline]
    fn table_ptr(&self, phys: PhysAddr) -> *const RawPageTable {
        self.phys_offset.wrapping_add(phys.0) as *const RawPageTable
    }

    /// Mutable variant of [`table_ptr`](Self::table_ptr).
    #[inline]
    fn table_ptr_mut(&self, phys: PhysAddr) -> *mut RawPageTable {
        self.phys_offset.wrapping_add(phys.0) as *mut RawPageTable
    }

    /// Zeroes all 4 096 bytes of the page-table frame at `phys`.
    ///
    /// # Safety
    ///
    /// Caller must ensure `phys` is a freshly-allocated, non-aliased frame
    /// within the direct-map window.
    unsafe fn zero_table(&self, phys: PhysAddr) {
        let ptr = self.table_ptr_mut(phys).cast::<u8>();
        unsafe { core::ptr::write_bytes(ptr, 0, 4096) };
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::BitmapFrameAllocator;

    // 64 frames of fake physical memory (256 KiB).
    // Using heap allocation to guarantee 4096-byte alignment.
    const ARENA_FRAMES: u64 = 64;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "64 fits usize on every supported target"
    )]
    const ARENA_SIZE: usize = ARENA_FRAMES as usize * 4096;
    const PHYS_BASE: u64 = 0x0100_0000; // 16 MiB — arbitrary "physical" base

    struct TestArena {
        ptr: *mut u8,
        layout: std::alloc::Layout,
    }

    impl TestArena {
        fn new() -> Self {
            let layout = std::alloc::Layout::from_size_align(ARENA_SIZE, 4096).unwrap();
            let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
            assert!(!ptr.is_null(), "test arena allocation failed");
            Self { ptr, layout }
        }

        fn phys_offset(&self) -> u64 {
            (self.ptr as u64).wrapping_sub(PHYS_BASE)
        }
    }

    impl Drop for TestArena {
        fn drop(&mut self) {
            unsafe { std::alloc::dealloc(self.ptr, self.layout) };
        }
    }

    fn make_mapper() -> (TestArena, PageMapper, BitmapFrameAllocator<1>) {
        let arena = TestArena::new();
        let phys_offset = arena.phys_offset();

        // Frame 0 at PHYS_BASE is the PML4.  Mark the rest free.
        let mut alloc = BitmapFrameAllocator::<1>::new(PhysAddr(PHYS_BASE));
        alloc.mark_range_free(PhysAddr(PHYS_BASE), ARENA_FRAMES * 4096);
        alloc.mark_range_used(PhysAddr(PHYS_BASE), 4096); // PML4 is pre-allocated

        let mapper = PageMapper::new(phys_offset, PhysAddr(PHYS_BASE));
        (arena, mapper, alloc)
    }

    // -------------------------------------------------------------------
    // virt_index tests
    // -------------------------------------------------------------------

    #[test]
    fn virt_index_zero_address() {
        let v = VirtAddr(0x0000_0000_0000_0000);
        assert_eq!(virt_index(v, PageLevel::Pml4), 0);
        assert_eq!(virt_index(v, PageLevel::Pdpt), 0);
        assert_eq!(virt_index(v, PageLevel::Pd), 0);
        assert_eq!(virt_index(v, PageLevel::Pt), 0);
    }

    #[test]
    fn virt_index_known_values() {
        // Pack indices 1,2,3,4 into the 4 levels.
        let virt = VirtAddr((1u64 << 39) | (2u64 << 30) | (3u64 << 21) | (4u64 << 12));
        assert_eq!(virt_index(virt, PageLevel::Pml4), 1);
        assert_eq!(virt_index(virt, PageLevel::Pdpt), 2);
        assert_eq!(virt_index(virt, PageLevel::Pd), 3);
        assert_eq!(virt_index(virt, PageLevel::Pt), 4);
    }

    #[test]
    fn virt_index_max_indices() {
        // All indices = 511 (0x1FF).
        let virt =
            VirtAddr((0x1FFu64 << 39) | (0x1FFu64 << 30) | (0x1FFu64 << 21) | (0x1FFu64 << 12));
        assert_eq!(virt_index(virt, PageLevel::Pml4), 511);
        assert_eq!(virt_index(virt, PageLevel::Pdpt), 511);
        assert_eq!(virt_index(virt, PageLevel::Pd), 511);
        assert_eq!(virt_index(virt, PageLevel::Pt), 511);
    }

    // -------------------------------------------------------------------
    // PageTableEntry tests
    // -------------------------------------------------------------------

    #[test]
    fn entry_default_not_present() {
        let e = PageTableEntry::default();
        assert!(!e.is_present());
        assert_eq!(e.phys_addr(), PhysAddr(0));
    }

    #[test]
    fn entry_set_and_read_frame() {
        let mut e = PageTableEntry(0);
        e.set_frame(PhysAddr(0x0000_DEAD_0000_0000), PTE_PRESENT | PTE_WRITABLE);
        assert!(e.is_present());
        assert_eq!(e.phys_addr(), PhysAddr(0x0000_DEAD_0000_0000));
    }

    #[test]
    fn entry_low_flag_bits_not_in_phys_addr() {
        let e = PageTableEntry(0x0000_DEAD_0000_0FFF);
        assert_eq!(e.phys_addr(), PhysAddr(0x0000_DEAD_0000_0000));
    }

    #[test]
    fn entry_clear_zeroes_all() {
        let mut e = PageTableEntry(0xFFFF_FFFF_FFFF_FFFF);
        e.clear();
        assert_eq!(e, PageTableEntry(0));
        assert!(!e.is_present());
    }

    // -------------------------------------------------------------------
    // PageMapper tests
    // -------------------------------------------------------------------

    #[test]
    fn translate_unmapped_is_none() {
        let (_arena, mapper, _alloc) = make_mapper();
        assert!(mapper.translate(VirtAddr(0x0000_0000_1234_5000)).is_none());
    }

    #[test]
    fn map_and_translate_round_trip() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let virt = VirtAddr(0x0000_0001_0000_0000);
        let phys = PhysAddr(0x0000_0002_0000_0000);
        assert!(mapper.map_4k(virt, phys, PTE_WRITABLE, &mut alloc));
        assert_eq!(mapper.translate(virt), Some(phys));
    }

    #[test]
    fn translate_preserves_page_offset() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let phys_base = PhysAddr(0x0000_0003_0000_0000);
        let virt_page = VirtAddr(0x0000_0001_8000_0000);
        assert!(mapper.map_4k(virt_page, phys_base, 0, &mut alloc));
        // Address within the page — offset should be added to phys_base.
        assert_eq!(
            mapper.translate(VirtAddr(virt_page.0 + 0xA00)),
            Some(PhysAddr(phys_base.0 + 0xA00)),
        );
    }

    #[test]
    fn unmap_not_present_returns_false() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        assert!(!mapper.unmap_4k(VirtAddr(0x0000_DEAD_BEEF_0000)));
    }

    #[test]
    fn map_then_unmap_then_translate_is_none() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let virt = VirtAddr(0x0000_0000_8000_0000);
        let phys = PhysAddr(0x0000_0001_0000_0000);
        assert!(mapper.map_4k(virt, phys, 0, &mut alloc));
        assert_eq!(mapper.translate(virt), Some(phys));
        assert!(mapper.unmap_4k(virt));
        assert!(mapper.translate(virt).is_none());
    }

    #[test]
    fn double_map_returns_false() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let virt = VirtAddr(0x0000_0000_4000_0000);
        let phys = PhysAddr(0x0000_0004_0000_0000);
        assert!(mapper.map_4k(virt, phys, 0, &mut alloc));
        assert!(!mapper.map_4k(virt, phys, 0, &mut alloc));
    }

    #[test]
    fn double_unmap_returns_false() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let virt = VirtAddr(0x0000_0000_2000_0000);
        let phys = PhysAddr(0x0000_0005_0000_0000);
        assert!(mapper.map_4k(virt, phys, 0, &mut alloc));
        assert!(mapper.unmap_4k(virt));
        assert!(!mapper.unmap_4k(virt));
    }

    #[test]
    fn adjacent_pages_share_intermediate_tables() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        // Two pages in the same PT (differ only in PT index).
        let virt1 = VirtAddr(0x0000_0000_1000_0000); // PT idx 0
        let virt2 = VirtAddr(0x0000_0000_1000_1000); // PT idx 1
        let phys1 = PhysAddr(0x0000_0010_0000_0000);
        let phys2 = PhysAddr(0x0000_0011_0000_0000);

        let frames_before = alloc.free_frames();
        assert!(mapper.map_4k(virt1, phys1, 0, &mut alloc));
        let frames_after_first = alloc.free_frames();
        // First map allocates 3 intermediate frames (PDPT, PD, PT).
        assert_eq!(frames_before - frames_after_first, 3);

        assert!(mapper.map_4k(virt2, phys2, 0, &mut alloc));
        let frames_after_second = alloc.free_frames();
        // Second map reuses all 3 existing tables — 0 new frames.
        assert_eq!(frames_after_first - frames_after_second, 0);

        assert_eq!(mapper.translate(virt1), Some(phys1));
        assert_eq!(mapper.translate(virt2), Some(phys2));
    }

    #[test]
    fn out_of_frames_returns_false() {
        let (_arena, mut mapper, mut alloc) = make_mapper();
        // Exhaust all free frames.
        while alloc.alloc_frame().is_some() {}
        // With no frames left, map_4k must fail.
        let result = mapper.map_4k(
            VirtAddr(0x0000_0000_F000_0000),
            PhysAddr(0x0000_0006_0000_0000),
            0,
            &mut alloc,
        );
        assert!(!result);
    }

    // -------------------------------------------------------------------
    // Huge-page translate tests (MB9)
    // -------------------------------------------------------------------

    /// Writes a 1 GiB huge-page PDPTE at `pdpt_idx` pointing to `huge_frame`,
    /// installing a PML4E that references a fresh PDPT in the arena at
    /// `pdpt_phys`. Returns nothing — the mapper observes the change via the
    /// direct-map window.
    fn install_1g_huge(
        mapper: &mut PageMapper,
        pdpt_phys: PhysAddr,
        pml4_idx: usize,
        pdpt_idx: usize,
        huge_frame: u64,
    ) {
        let pml4 = mapper.table_ptr_mut(mapper.root_phys);
        unsafe {
            (*pml4)
                .entry_mut(pml4_idx)
                .set_frame(pdpt_phys, PTE_PRESENT | PTE_WRITABLE);
            let pdpt = mapper.table_ptr_mut(pdpt_phys);
            (*pdpt).entry_mut(pdpt_idx).0 = huge_frame | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE;
        }
    }

    /// Same as `install_1g_huge` but at PD level — installs PML4→PDPT→PD chain
    /// and writes a 2 MiB huge-page PDE at `pd_idx` pointing to `huge_frame`.
    fn install_2m_huge(
        mapper: &mut PageMapper,
        pdpt_phys: PhysAddr,
        pd_phys: PhysAddr,
        pml4_idx: usize,
        pdpt_idx: usize,
        pd_idx: usize,
        huge_frame: u64,
    ) {
        let pml4 = mapper.table_ptr_mut(mapper.root_phys);
        unsafe {
            (*pml4)
                .entry_mut(pml4_idx)
                .set_frame(pdpt_phys, PTE_PRESENT | PTE_WRITABLE);
            let pdpt = mapper.table_ptr_mut(pdpt_phys);
            (*pdpt)
                .entry_mut(pdpt_idx)
                .set_frame(pd_phys, PTE_PRESENT | PTE_WRITABLE);
            let pd = mapper.table_ptr_mut(pd_phys);
            (*pd).entry_mut(pd_idx).0 = huge_frame | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE;
        }
    }

    #[test]
    fn translate_follows_1gib_huge_page_at_start() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let huge_frame = 0x0000_0001_8000_0000u64; // 6 GiB, 1 GiB-aligned
        install_1g_huge(
            &mut mapper,
            pdpt_phys,
            /*pml4=*/ 0,
            /*pdpt=*/ 1,
            huge_frame,
        );
        // virt = PML4=0, PDPT=1 → (1 << 30) = 0x4000_0000
        assert_eq!(
            mapper.translate(VirtAddr(1u64 << 30)),
            Some(PhysAddr(huge_frame)),
        );
    }

    #[test]
    fn translate_follows_1gib_huge_page_middle_offset() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let huge_frame = 0x0000_0001_8000_0000u64;
        install_1g_huge(&mut mapper, pdpt_phys, 0, 1, huge_frame);
        let virt = (1u64 << 30) + 0x1234_5678;
        assert_eq!(
            mapper.translate(VirtAddr(virt)),
            Some(PhysAddr(huge_frame + 0x1234_5678)),
        );
    }

    #[test]
    fn translate_follows_1gib_huge_page_last_byte() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let huge_frame = 0x0000_0001_8000_0000u64;
        install_1g_huge(&mut mapper, pdpt_phys, 0, 1, huge_frame);
        let virt = (1u64 << 30) + 0x3FFF_FFFF; // last byte in the 1 GiB region
        assert_eq!(
            mapper.translate(VirtAddr(virt)),
            Some(PhysAddr(huge_frame + 0x3FFF_FFFF)),
        );
    }

    #[test]
    fn translate_follows_2mib_huge_page_at_start() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let pd_phys = PhysAddr(PHYS_BASE + 8192);
        let huge_frame = 0x0000_0002_0040_0000u64; // 2 MiB-aligned (low 21 bits = 0)
        install_2m_huge(&mut mapper, pdpt_phys, pd_phys, 0, 0, 2, huge_frame);
        // virt = PD=2 → (2 << 21) = 0x40_0000
        assert_eq!(
            mapper.translate(VirtAddr(2u64 << 21)),
            Some(PhysAddr(huge_frame)),
        );
    }

    #[test]
    fn translate_follows_2mib_huge_page_middle_offset() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let pd_phys = PhysAddr(PHYS_BASE + 8192);
        let huge_frame = 0x0000_0002_0040_0000u64;
        install_2m_huge(&mut mapper, pdpt_phys, pd_phys, 0, 0, 2, huge_frame);
        let virt = (2u64 << 21) + 0x10_0000; // 1 MiB into the 2 MiB page
        assert_eq!(
            mapper.translate(VirtAddr(virt)),
            Some(PhysAddr(huge_frame + 0x10_0000)),
        );
    }

    #[test]
    fn translate_follows_2mib_huge_page_last_byte() {
        let (_arena, mut mapper, _alloc) = make_mapper();
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        let pd_phys = PhysAddr(PHYS_BASE + 8192);
        let huge_frame = 0x0000_0002_0040_0000u64;
        install_2m_huge(&mut mapper, pdpt_phys, pd_phys, 0, 0, 2, huge_frame);
        let virt = (2u64 << 21) + 0x1F_FFFF; // last byte in the 2 MiB region
        assert_eq!(
            mapper.translate(VirtAddr(virt)),
            Some(PhysAddr(huge_frame + 0x1F_FFFF)),
        );
    }

    #[test]
    fn translate_1gib_huge_does_not_dereference_pd() {
        // Regression: when PDPTE has PS=1, the walker must not load the next
        // level's table pointer (the PDPTE frame field is the leaf, not a
        // pointer to a PD). Catches a future bug where the walker would
        // dereference a bogus table pointer.
        let (_arena, mut mapper, _alloc) = make_mapper();
        // Point the PDPTE to an unmapped "phys" address (would fault if walked).
        let pdpt_phys = PhysAddr(PHYS_BASE + 4096);
        // The huge_frame field of the PDPTE is set to something that, if
        // treated as a PD pointer, would index outside the arena.
        let huge_frame = 0x0000_00FF_C000_0000u64;
        install_1g_huge(&mut mapper, pdpt_phys, 0, 1, huge_frame);
        // Translation must succeed without panicking / segfaulting on the
        // test harness.
        assert_eq!(
            mapper.translate(VirtAddr(1u64 << 30)),
            Some(PhysAddr(huge_frame)),
        );
    }

    #[test]
    fn translate_4kib_path_still_works_after_huge_page_support() {
        // Regression for the 4 KiB code path after the huge-page changes.
        let (_arena, mut mapper, mut alloc) = make_mapper();
        let virt = VirtAddr(0x0000_0007_0000_0000);
        let phys = PhysAddr(0x0000_0008_0000_0000);
        assert!(mapper.map_4k(virt, phys, PTE_WRITABLE, &mut alloc));
        assert_eq!(mapper.translate(virt), Some(phys));
        assert_eq!(
            mapper.translate(VirtAddr(virt.0 + 0xABC)),
            Some(PhysAddr(phys.0 + 0xABC)),
        );
    }
}
