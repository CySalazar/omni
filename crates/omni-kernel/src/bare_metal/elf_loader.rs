//! ELF64 loader — minimal parser + segment mapper (Track B, MB5).
//!
//! Parses a statically-linked ELF64 binary (`ET_EXEC` or `ET_DYN`,
//! `EM_X86_64`, little-endian) and maps its `PT_LOAD` segments into the
//! active page tables via [`super::paging::PageMapper`].
//!
//! ## Scope
//!
//! - [`Elf64::parse`] validates the ELF header and program-header table
//!   without copying any data.
//! - [`Elf64::load_segments`] yields a [`LoadSegment`] for every `PT_LOAD`
//!   entry; the caller decides whether to map or inspect the segment.
//! - `Elf64::map_and_load` allocates physical frames, maps each segment
//!   into the page tables, and copies the segment's file image; BSS
//!   (memsz > filesz) is zeroed.
//!
//! ## Portability
//!
//! The parser (`parse`, `load_segments`, `entry_point`) compiles on every
//! target so that host-side unit tests run on the developer machine.
//! `map_and_load` is gated `#[cfg(target_arch = "x86_64")]` because it
//! calls into the x86_64-only `PageMapper` and `BitmapFrameAllocator`.

#![allow(
    unsafe_code,
    reason = "ELF segment loader copies file bytes via raw ptr::copy_nonoverlapping"
)]
#![allow(
    clippy::integer_division,
    reason = "ELF page math uses 4 KiB byte-aligned truncation by design"
)]
#![allow(
    clippy::indexing_slicing,
    clippy::doc_markdown,
    reason = "byte-offset slicing has explicit bounds check; ELF acronyms in prose"
)]

// ---------------------------------------------------------------------------
// ELF constants
// ---------------------------------------------------------------------------

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;

/// ELF segment flag: executable.
pub const PF_X: u32 = 1;
/// ELF segment flag: writable.
pub const PF_W: u32 = 2;
/// ELF segment flag: readable.
pub const PF_R: u32 = 4;

// ---------------------------------------------------------------------------
// Private read helpers
// ---------------------------------------------------------------------------

#[inline]
fn r_u16(data: &[u8], off: usize) -> Option<u16> {
    u16::from_le_bytes(data.get(off..off + 2)?.try_into().ok()?).into()
}

#[inline]
fn r_u32(data: &[u8], off: usize) -> Option<u32> {
    u32::from_le_bytes(data.get(off..off + 4)?.try_into().ok()?).into()
}

#[inline]
fn r_u64(data: &[u8], off: usize) -> Option<u64> {
    u64::from_le_bytes(data.get(off..off + 8)?.try_into().ok()?).into()
}

// ---------------------------------------------------------------------------
// ElfError
// ---------------------------------------------------------------------------

/// Errors returned by the ELF64 loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Binary is too short to contain a valid ELF64 header.
    TooShort,
    /// Magic bytes `0x7fELF` not found.
    BadMagic,
    /// `EI_CLASS` is not `ELFCLASS64` (2).
    NotElf64,
    /// `EI_DATA` is not `ELFDATA2LSB` (1).
    NotLittleEndian,
    /// `e_machine` is not `EM_X86_64` (62).
    UnsupportedMachine,
    /// `e_type` is neither `ET_EXEC` (2) nor `ET_DYN` (3).
    UnsupportedType,
    /// Program-header table is absent, too small, or out of bounds.
    BadPhdrs,
    /// Frame allocator could not provide a physical frame.
    OutOfFrames,
    /// `PageMapper::map_4k` refused the mapping (already mapped or OOM).
    MappingFailed,
}

// ---------------------------------------------------------------------------
// LoadSegment
// ---------------------------------------------------------------------------

/// A single `PT_LOAD` segment ready to be mapped into the address space.
#[derive(Debug, Clone, Copy)]
pub struct LoadSegment<'a> {
    /// Virtual address of the first byte of this segment.
    pub virt_addr: u64,
    /// Slice of the ELF binary that contains the file image for this segment.
    pub file_data: &'a [u8],
    /// Size of the segment in memory (may be larger than `file_data.len()`).
    pub mem_size: usize,
    /// ELF segment flags (`PF_R`, `PF_W`, `PF_X`).
    pub flags: u32,
}

// ---------------------------------------------------------------------------
// SegIter — private iterator over PT_LOAD entries
// ---------------------------------------------------------------------------

struct SegIter<'a> {
    data: &'a [u8],
    phoff: usize,
    phentsize: usize,
    phnum: usize,
    idx: usize,
}

impl<'a> Iterator for SegIter<'a> {
    type Item = Result<LoadSegment<'a>, ElfError>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "ELF offsets/sizes fit usize on supported platforms"
    )]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.idx >= self.phnum {
                return None;
            }
            let i = self.idx;
            self.idx += 1;

            let base = self.phoff + i * self.phentsize;
            let data = self.data;

            let Some(p_type) = r_u32(data, base) else {
                return Some(Err(ElfError::BadPhdrs));
            };
            if p_type != PT_LOAD {
                continue;
            }

            let Some(p_flags) = r_u32(data, base + 4) else {
                return Some(Err(ElfError::BadPhdrs));
            };
            let Some(p_offset_raw) = r_u64(data, base + 8) else {
                return Some(Err(ElfError::BadPhdrs));
            };
            let Some(p_vaddr) = r_u64(data, base + 16) else {
                return Some(Err(ElfError::BadPhdrs));
            };
            let Some(p_filesz_raw) = r_u64(data, base + 32) else {
                return Some(Err(ElfError::BadPhdrs));
            };
            let Some(p_memsz_raw) = r_u64(data, base + 40) else {
                return Some(Err(ElfError::BadPhdrs));
            };

            let p_offset = p_offset_raw as usize;
            let p_filesz = p_filesz_raw as usize;
            let p_memsz = p_memsz_raw as usize;

            let Some(file_data) = data.get(p_offset..p_offset + p_filesz) else {
                return Some(Err(ElfError::BadPhdrs));
            };

            return Some(Ok(LoadSegment {
                virt_addr: p_vaddr,
                file_data,
                mem_size: p_memsz,
                flags: p_flags,
            }));
        }
    }
}

// ---------------------------------------------------------------------------
// Elf64
// ---------------------------------------------------------------------------

/// A parsed ELF64 binary.
///
/// Holds a reference to the raw bytes; no allocation occurs during parsing.
#[derive(Debug, PartialEq, Eq)]
pub struct Elf64<'a> {
    data: &'a [u8],
    entry: u64,
    e_type: u16,
    phoff: usize,
    phentsize: usize,
    phnum: usize,
}

impl<'a> Elf64<'a> {
    /// Parse an ELF64 binary and validate its header.
    ///
    /// # Errors
    ///
    /// Returns [`ElfError`] if the binary is malformed, too short, or
    /// targets an unsupported architecture or type.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "phoff/phentsize/phnum fit usize on supported platforms"
    )]
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < 64 {
            return Err(ElfError::TooShort);
        }
        // SAFETY: len >= 64 was checked above; these accesses are all within bounds.
        if data.get(0..4) != Some(&ELF_MAGIC[..]) {
            return Err(ElfError::BadMagic);
        }
        if data.get(4).copied() != Some(ELFCLASS64) {
            return Err(ElfError::NotElf64);
        }
        if data.get(5).copied() != Some(ELFDATA2LSB) {
            return Err(ElfError::NotLittleEndian);
        }

        let e_type = r_u16(data, 16).ok_or(ElfError::TooShort)?;
        if e_type != ET_EXEC && e_type != ET_DYN {
            return Err(ElfError::UnsupportedType);
        }

        let e_machine = r_u16(data, 18).ok_or(ElfError::TooShort)?;
        if e_machine != EM_X86_64 {
            return Err(ElfError::UnsupportedMachine);
        }

        let entry = r_u64(data, 24).ok_or(ElfError::TooShort)?;
        let phoff = r_u64(data, 32).ok_or(ElfError::TooShort)? as usize;
        let phentsize = r_u16(data, 54).ok_or(ElfError::TooShort)? as usize;
        let phnum = r_u16(data, 56).ok_or(ElfError::TooShort)? as usize;

        if phentsize < 56 || phoff == 0 || phnum == 0 {
            return Err(ElfError::BadPhdrs);
        }
        if data.len() < phoff + phnum * phentsize {
            return Err(ElfError::TooShort);
        }

        Ok(Self {
            data,
            entry,
            e_type,
            phoff,
            phentsize,
            phnum,
        })
    }

    /// Returns the virtual entry-point address from the ELF header,
    /// adjusted for the load bias when the ELF is a PIE (`ET_DYN`).
    #[inline]
    #[must_use]
    pub fn entry_point(&self) -> u64 {
        self.entry + self.load_bias()
    }

    /// Load bias applied to PIE (`ET_DYN`) executables.
    ///
    /// `ET_EXEC` binaries have absolute addresses and no bias.
    /// `ET_DYN` binaries have relative addresses starting near zero;
    /// the kernel maps them at this fixed base address.
    #[inline]
    #[must_use]
    pub fn load_bias(&self) -> u64 {
        if self.e_type == ET_DYN { 0x40_0000 } else { 0 }
    }

    /// Returns an iterator over the `PT_LOAD` program-header entries.
    pub fn load_segments(&self) -> impl Iterator<Item = Result<LoadSegment<'a>, ElfError>> + 'a {
        SegIter {
            data: self.data,
            phoff: self.phoff,
            phentsize: self.phentsize,
            phnum: self.phnum,
            idx: 0,
        }
    }

    /// Allocate physical frames, map each `PT_LOAD` segment, and copy the
    /// file image. BSS bytes (`memsz > filesz`) are zeroed. Maps into the
    /// active address space (`mapper.root_phys`).
    ///
    /// Returns the entry-point virtual address on success.
    ///
    /// # Errors
    ///
    /// Returns [`ElfError::OutOfFrames`] if the frame allocator is exhausted,
    /// or [`ElfError::MappingFailed`] if a page is already mapped.
    ///
    /// `phys_offset` must equal `BootInfo.physical_memory_offset` — the
    /// virtual base of the bootloader's direct physical-memory window.
    #[cfg(target_arch = "x86_64")]
    pub fn map_and_load<const N: usize>(
        &self,
        mapper: &mut super::paging::PageMapper,
        alloc: &mut crate::memory::BitmapFrameAllocator<N>,
        phys_offset: u64,
    ) -> Result<u64, ElfError> {
        let root = mapper.root_phys;
        self.map_and_load_into(root, mapper, alloc, phys_offset)
    }

    /// Variant of [`Self::map_and_load`] that maps into an explicit
    /// page-table root (e.g. a per-process PML4 owned by an
    /// [`super::address_space::AddressSpace`]).
    ///
    /// MB11: required to load a user ELF into a per-process CR3 without
    /// mutating the live `mapper.root_phys`.
    ///
    /// # Errors
    ///
    /// Same as [`Self::map_and_load`].
    #[cfg(target_arch = "x86_64")]
    pub fn map_and_load_into<const N: usize>(
        &self,
        root_phys: crate::memory::PhysAddr,
        mapper: &mut super::paging::PageMapper,
        alloc: &mut crate::memory::BitmapFrameAllocator<N>,
        phys_offset: u64,
    ) -> Result<u64, ElfError> {
        use core::ptr;

        let bias = self.load_bias();

        for seg_result in self.load_segments() {
            let seg = seg_result?;

            let page_base = (seg.virt_addr + bias) & !0xFFF;
            let page_intra = (seg.virt_addr & 0xFFF) as usize;
            let total_mem = page_intra + seg.mem_size;
            let num_pages = (total_mem + 4095) / 4096;

            for page_i in 0..num_pages {
                let virt = crate::memory::VirtAddr(page_base + page_i as u64 * 4096);
                let frame = alloc.alloc_frame().ok_or(ElfError::OutOfFrames)?;

                if !mapper.map_4k_into(root_phys, virt, frame, pte_flags(seg.flags), alloc) {
                    return Err(ElfError::MappingFailed);
                }

                // SAFETY: frame.0 + phys_offset is within the bootloader's
                // direct-mapped physical window; the frame was just allocated
                // and is not aliased elsewhere.
                let dst = (frame.0 + phys_offset) as *mut u8;

                let page_file_start = (page_i * 4096).saturating_sub(page_intra);
                let page_file_end = page_file_start + 4096;

                if page_file_start < seg.file_data.len() {
                    let copy_len = seg.file_data.len().min(page_file_end) - page_file_start;
                    unsafe {
                        ptr::copy_nonoverlapping(
                            seg.file_data[page_file_start..].as_ptr(),
                            dst,
                            copy_len,
                        );
                        ptr::write_bytes(dst.add(copy_len), 0, 4096 - copy_len);
                    }
                } else {
                    unsafe { ptr::write_bytes(dst, 0, 4096) };
                }
            }
        }

        // Process R_X86_64_RELATIVE relocations for PIE (ET_DYN) binaries.
        // Each entry in the RELA table stores an offset where the load bias
        // must be added to the stored value. Without this step, GOT entries
        // and other absolute references resolve to addresses near zero,
        // causing immediate page faults.
        if bias != 0 {
            self.apply_relative_relocs(root_phys, mapper, bias, phys_offset);
        }

        Ok(self.entry + bias)
    }

    /// Scan the ELF for PT_DYNAMIC, find the RELA table, and process all
    /// `R_X86_64_RELATIVE` entries by adding `bias` to the stored addend.
    #[cfg(target_arch = "x86_64")]
    fn apply_relative_relocs(
        &self,
        root_phys: crate::memory::PhysAddr,
        mapper: &super::paging::PageMapper,
        bias: u64,
        phys_offset: u64,
    ) {
        const PT_DYNAMIC: u32 = 2;
        const DT_RELA: u64 = 7;
        const DT_RELASZ: u64 = 8;
        const R_X86_64_RELATIVE: u32 = 8;

        // Find PT_DYNAMIC segment.
        let mut dyn_offset = 0usize;
        let mut dyn_size = 0usize;
        for i in 0..self.phnum {
            let base = self.phoff + i * self.phentsize;
            if let Some(p_type) = r_u32(self.data, base) {
                if p_type == PT_DYNAMIC {
                    dyn_offset = r_u64(self.data, base + 8).unwrap_or(0) as usize;
                    dyn_size = r_u64(self.data, base + 32).unwrap_or(0) as usize;
                    break;
                }
            }
        }
        if dyn_offset == 0 || dyn_size == 0 {
            return;
        }

        // Parse DYNAMIC entries to find RELA offset and size.
        let mut rela_off = 0u64;
        let mut rela_sz = 0u64;
        let mut pos = dyn_offset;
        while pos + 16 <= dyn_offset + dyn_size {
            let tag = r_u64(self.data, pos).unwrap_or(0);
            let val = r_u64(self.data, pos + 8).unwrap_or(0);
            if tag == 0 {
                break;
            } // DT_NULL
            if tag == DT_RELA {
                rela_off = val;
            }
            if tag == DT_RELASZ {
                rela_sz = val;
            }
            pos += 16;
        }
        if rela_off == 0 || rela_sz == 0 {
            return;
        }

        // The RELA entries are at file offset = rela_off (for PIE, this
        // is the same as the virt_addr since the base is 0).
        let rela_file_off = rela_off as usize;
        let num_entries = rela_sz as usize / 24;

        for i in 0..num_entries {
            let ent_off = rela_file_off + i * 24;
            let r_offset = match r_u64(self.data, ent_off) {
                Some(v) => v,
                None => continue,
            };
            let r_info = match r_u64(self.data, ent_off + 8) {
                Some(v) => v,
                None => continue,
            };
            let r_addend = match r_u64(self.data, ent_off + 16) {
                Some(v) => v,
                None => continue,
            };

            let r_type = (r_info & 0xFFFF_FFFF) as u32;
            if r_type != R_X86_64_RELATIVE {
                continue;
            }

            // Write (bias + addend) at virtual address (bias + r_offset).
            let target_va = bias + r_offset;
            let value = bias + r_addend;

            // Translate VA → physical via the page table we just built.
            if let Some(phys) = mapper.translate_in(root_phys, crate::memory::VirtAddr(target_va)) {
                let dst = (phys.0 + phys_offset) as *mut u64;
                // SAFETY: the page was just mapped and allocated by us;
                // writing the relocation value is required for the binary
                // to function correctly at the biased address.
                unsafe {
                    core::ptr::write(dst, value);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// pte_flags — private
// ---------------------------------------------------------------------------

/// Converts ELF segment flags to x86_64 PTE flags.
#[cfg(target_arch = "x86_64")]
fn pte_flags(elf_flags: u32) -> u64 {
    use super::paging::{PTE_PRESENT, PTE_USER, PTE_WRITABLE};
    let mut f = PTE_PRESENT | PTE_USER;
    if elf_flags & PF_W != 0 {
        f |= PTE_WRITABLE;
    }
    f
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A 120-byte hand-crafted ELF64 binary: ET_EXEC, EM_X86_64,
    /// one PT_LOAD segment at 0x4000_0000, entry=0x4000_0000,
    /// filesz=120, memsz=4096.
    const TEST_ELF: [u8; 120] = [
        // e_ident[16]: magic + class64 + LSB + version + sysv + padding
        0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // e_type=ET_EXEC, e_machine=EM_X86_64
        0x02, 0x00, 0x3E, 0x00, // e_version=1
        0x01, 0x00, 0x00, 0x00, // e_entry=0x4000_0000
        0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, // e_phoff=64
        0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // e_shoff=0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // e_flags=0
        0x00, 0x00, 0x00, 0x00, // e_ehsize=64, e_phentsize=56, e_phnum=1
        0x40, 0x00, 0x38, 0x00, 0x01, 0x00, // e_shentsize=64, e_shnum=0, e_shstrndx=0
        0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
        // Program header at offset 64:
        // p_type=PT_LOAD
        0x01, 0x00, 0x00, 0x00, // p_flags=PF_R|PF_X=5
        0x05, 0x00, 0x00, 0x00, // p_offset=0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // p_vaddr=0x4000_0000
        0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
        // p_paddr=0x4000_0000 (not used by loader)
        0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, // p_filesz=120
        0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // p_memsz=4096
        0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // p_align=4096
        0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn parse_valid_elf_succeeds() {
        assert!(Elf64::parse(&TEST_ELF).is_ok());
    }

    #[test]
    fn entry_point_is_correct() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        assert_eq!(elf.entry_point(), 0x4000_0000);
    }

    #[test]
    #[allow(clippy::indexing_slicing, reason = "segs.len() == 1 asserted above")]
    fn one_load_segment_found() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        let segs: Vec<_> = elf.load_segments().collect();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].is_ok());
    }

    #[test]
    fn segment_virt_addr_correct() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        let seg = elf.load_segments().next().unwrap().unwrap();
        assert_eq!(seg.virt_addr, 0x4000_0000);
    }

    #[test]
    fn segment_file_data_has_correct_length() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        let seg = elf.load_segments().next().unwrap().unwrap();
        assert_eq!(seg.file_data.len(), 120);
    }

    #[test]
    fn segment_mem_size_is_one_page() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        let seg = elf.load_segments().next().unwrap().unwrap();
        assert_eq!(seg.mem_size, 4096);
    }

    #[test]
    fn segment_flags_rx() {
        let elf = Elf64::parse(&TEST_ELF).unwrap();
        let seg = elf.load_segments().next().unwrap().unwrap();
        assert_eq!(seg.flags, PF_R | PF_X);
    }

    #[test]
    fn reject_bad_magic() {
        let mut buf = TEST_ELF;
        buf[0] = 0x00;
        assert_eq!(Elf64::parse(&buf), Err(ElfError::BadMagic));
    }

    #[test]
    fn reject_not_64bit() {
        let mut buf = TEST_ELF;
        buf[4] = 1; // ELFCLASS32
        assert_eq!(Elf64::parse(&buf), Err(ElfError::NotElf64));
    }

    #[test]
    fn reject_big_endian() {
        let mut buf = TEST_ELF;
        buf[5] = 2; // ELFDATA2MSB
        assert_eq!(Elf64::parse(&buf), Err(ElfError::NotLittleEndian));
    }

    #[test]
    fn reject_not_x86_64() {
        let mut buf = TEST_ELF;
        // e_machine at offset 18: set to 3 (EM_386)
        buf[18] = 3;
        buf[19] = 0;
        assert_eq!(Elf64::parse(&buf), Err(ElfError::UnsupportedMachine));
    }

    #[test]
    fn reject_too_short() {
        assert_eq!(Elf64::parse(&TEST_ELF[..10]), Err(ElfError::TooShort));
    }
}
