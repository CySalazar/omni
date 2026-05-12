//! Memory management: virtual memory, page tables, allocators.
//!
//! ## Status
//!
//! P6.3 scaffold. The trait surface and helper types are defined; the
//! concrete `x86_64` page-table machinery lands when the bootloader
//! hand-off is operational (P6.2).
//!
//! ## Design rationale
//!
//! - **Single allocator interface.** The kernel exposes one
//!   [`Allocator`] trait; concrete impls include a bump allocator (used
//!   during early boot), a slab allocator (mature kernel state), and a
//!   per-task region allocator (per-process address spaces).
//! - **Page-table abstractions are arch-specific.** The trait surface in
//!   [`PageTable`] is arch-neutral, but the impls live in arch-gated
//!   modules. `x86_64` lands first.
//! - **Capabilities gate every allocation.** A caller must present a
//!   capability authorizing the requested allocation class; the kernel
//!   refuses ambient allocation entirely.

use crate::{KernelResult, bitflags_simple};

// -----------------------------------------------------------------------------
// Address types
// -----------------------------------------------------------------------------

/// A physical address. 64-bit; the upper bits are reserved by the
/// hardware in the v1 platform support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

/// A virtual address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtAddr(pub u64);

impl PhysAddr {
    /// Convenience: returns the address as a raw `u64`.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl VirtAddr {
    /// Convenience: returns the address as a raw `u64`.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

// -----------------------------------------------------------------------------
// Page size
// -----------------------------------------------------------------------------

/// Page sizes supported by the kernel.
///
/// The v1 kernel supports 4 KiB and 2 MiB pages on `x86_64`. 1 GiB pages
/// are a roadmap item (P6.3) once the page-table walker is mature; the
/// variant exists in the enum to make adding the impl a non-breaking
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageSize {
    /// 4 KiB — standard page.
    Page4Kib,
    /// 2 MiB — large page.
    Page2Mib,
    /// 1 GiB — huge page (reserved for v1.x).
    Page1Gib,
}

impl PageSize {
    /// Returns the page size in bytes.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        match self {
            Self::Page4Kib => 4 * 1024,
            Self::Page2Mib => 2 * 1024 * 1024,
            Self::Page1Gib => 1024 * 1024 * 1024,
        }
    }
}

// -----------------------------------------------------------------------------
// Page table flags
// -----------------------------------------------------------------------------

bitflags_simple! {
    /// Page-table entry flags. Mapped to the architecture's bit positions
    /// inside the arch-specific page-table walker.
    ///
    /// (We use a hand-rolled bitflags-like macro to avoid the `bitflags`
    /// crate dependency at this layer; the kernel deliberately keeps its
    /// dep graph minimal.)
    pub struct PageFlags: u32 {
        /// The page is present in memory.
        const PRESENT     = 0b0000_0001;
        /// The page is writable.
        const WRITABLE    = 0b0000_0010;
        /// The page is accessible from user mode.
        const USER        = 0b0000_0100;
        /// The page is executable.
        const EXECUTABLE  = 0b0000_1000;
        /// The page is cacheable.
        const CACHEABLE   = 0b0001_0000;
        /// The page is global (TLB persists across address-space switches).
        const GLOBAL      = 0b0010_0000;
    }
}

// -----------------------------------------------------------------------------
// Allocator trait
// -----------------------------------------------------------------------------

/// A kernel allocator.
///
/// All allocations require a capability per the kernel security model.
/// The capability is validated by the syscall layer; allocators only see
/// already-authorized calls.
pub trait Allocator {
    /// Allocates `count` contiguous pages of `size` and returns the
    /// physical address of the first page.
    ///
    /// # Errors
    ///
    /// - [`crate::KernelError::ResourceExhausted`] if no contiguous region
    ///   is available.
    fn allocate(&mut self, size: PageSize, count: usize) -> KernelResult<PhysAddr>;

    /// Releases a previously-allocated region.
    fn free(&mut self, addr: PhysAddr, size: PageSize, count: usize) -> KernelResult<()>;

    /// Returns the total number of bytes currently allocated.
    fn allocated_bytes(&self) -> u64;
}

// -----------------------------------------------------------------------------
// PageTable trait
// -----------------------------------------------------------------------------

/// A page-table walker. Arch-specific implementations live in
/// `omni-kernel-arch-x86_64` (P6.3+) and similar.
pub trait PageTable {
    /// Maps `virt` to `phys` with the given flags.
    fn map(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        size: PageSize,
        flags: PageFlags,
    ) -> KernelResult<()>;

    /// Unmaps `virt`. Returns the previously-mapped physical address.
    fn unmap(&mut self, virt: VirtAddr) -> KernelResult<PhysAddr>;

    /// Returns the physical address `virt` maps to, or `None`.
    fn translate(&self, virt: VirtAddr) -> Option<PhysAddr>;

    /// Flushes the TLB entry for `virt`. Architecture-specific.
    fn flush(&mut self, virt: VirtAddr);
}

// -----------------------------------------------------------------------------
// Internal bitflags-like macro
// -----------------------------------------------------------------------------

/// Tiny replacement for the `bitflags` crate. Sufficient for the small
/// number of bitflag types in the kernel; eliminates an external dep.
#[macro_export]
macro_rules! bitflags_simple {
    (
        $(#[$meta:meta])*
        pub struct $name:ident: $repr:ty {
            $($(#[$bit_meta:meta])* const $bit:ident = $value:expr;)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub $repr);

        impl $name {
            $($(#[$bit_meta])* pub const $bit: Self = Self($value);)*

            /// Returns the underlying bit representation.
            #[must_use]
            pub const fn bits(self) -> $repr { self.0 }

            /// Returns `true` if all bits in `other` are set.
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }
        }

        impl core::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
    };
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_bytes_correct() {
        assert_eq!(PageSize::Page4Kib.bytes(), 4096);
        assert_eq!(PageSize::Page2Mib.bytes(), 2 * 1024 * 1024);
        assert_eq!(PageSize::Page1Gib.bytes(), 1024 * 1024 * 1024);
    }

    #[test]
    fn page_flags_combine() {
        let combined = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER;
        assert!(combined.contains(PageFlags::PRESENT));
        assert!(combined.contains(PageFlags::WRITABLE));
        assert!(combined.contains(PageFlags::USER));
        assert!(!combined.contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn addr_types_round_trip() {
        let p = PhysAddr(0xDEAD_BEEFu64);
        let v = VirtAddr(0xCAFE_F00Du64);
        assert_eq!(p.as_u64(), 0xDEAD_BEEFu64);
        assert_eq!(v.as_u64(), 0xCAFE_F00Du64);
    }
}
