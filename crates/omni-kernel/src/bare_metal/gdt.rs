//! Kernel Global Descriptor Table (`GDT`) for `x86_64` long mode.
//!
//! Installs a 7-entry GDT under kernel control, replacing the
//! bootloader's temporary GDT. Layout per ADR-0004 § 1:
//!
//! | Slot | Selector | Contenuto             | Access | Flags | DPL |
//! |------|----------|-----------------------|--------|-------|-----|
//! | 0    | 0x00     | null                  | —      | —     | —   |
//! | 1    | 0x08     | kcode64               | 0x9B   | 0xA   | 0   |
//! | 2    | 0x10     | kdata64               | 0x93   | 0xC   | 0   |
//! | 3    | 0x18     | user-data placeholder | 0xF2   | 0xC   | 3   |
//! | 4    | 0x20     | ucode64               | 0xFA   | 0xA   | 3   |
//! | 5    | 0x28     | TSS low word          | (sys)  | 0x0   | 0   |
//! | 6    | —        | TSS high word         | —      | —     | —   |
//!
//! Selectors used by other subsystems:
//!
//! - `MSR_STAR[63:48] = 0x10` produces SYSRET q `CS = 0x10+16|3 = 0x23`
//!   (slot 4 = ucode64) and `SS = 0x10+8|3 = 0x1B` (slot 3 = user-data).
//! - `iretq` to Ring 3 pushes `CS = 0x23` and `SS = 0x1B`.
//! - `ltr 0x28` installs the static TSS in [`super::tss`].
//!
//! Call [`gdt_init`] once from `kmain` BEFORE [`super::tss::ltr_load`].

#![allow(
    unsafe_code,
    reason = "lgdt + segment-register loads via inline asm; SAFETY per call site"
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "GDT byte-size limit fits u16 by construction"
)]

// -----------------------------------------------------------------------------
// GDT entry descriptors (x86_64 long mode)
//
// Each entry is a u64 encoding:
//   [0:15]  limit low
//   [16:39] base low
//   [40:47] access byte (P, DPL, S, type)
//   [48:51] limit high
//   [52:55] flags (G, D/B, L, AVL)
//   [56:63] base high
//
// In 64-bit long mode, base and limit are ignored for code/data segments
// (flat model). Only the access byte and the L/D bits in flags matter.
// -----------------------------------------------------------------------------

/// 64-bit kernel code segment (DPL=0, L=1, D=0, G=1).
///
/// Access 0x9B = P=1, DPL=00, S=1, type=0xB (execute/read, A=1 pre-set).
/// Flags  0xA  = G=1, L=1, D/B=0, AVL=0.
#[cfg(target_arch = "x86_64")]
const KCODE64: u64 = 0x00AF_9B00_0000_FFFF;

/// Kernel data segment (DPL=0, G=1, D/B=1).
///
/// Access 0x93 = P=1, DPL=00, S=1, type=0x3 (read/write, A=1 pre-set).
/// Flags  0xC  = G=1, D/B=1, L=0, AVL=0.
#[cfg(target_arch = "x86_64")]
const KDATA64: u64 = 0x00CF_9300_0000_FFFF;

/// User data segment (DPL=3, G=1, D/B=1).
///
/// Access 0xF2 = P=1, DPL=11, S=1, type=0x2 (read/write, A=0).
/// Flags  0xC  = G=1, D/B=1, L=0, AVL=0.
///
/// Used as the SS selector for Ring 3 (selector 0x1B = slot 3 | RPL=3).
#[cfg(any(target_arch = "x86_64", test))]
const UDATA64: u64 = 0x00CF_F200_0000_FFFF;

/// 64-bit user code segment (DPL=3, L=1, D=0, G=1).
///
/// Access 0xFA = P=1, DPL=11, S=1, type=0xA (execute/read, A=0).
/// Flags  0xA  = G=1, L=1, D/B=0, AVL=0.
///
/// Used as the CS selector for Ring 3 (selector 0x23 = slot 4 | RPL=3).
#[cfg(any(target_arch = "x86_64", test))]
const UCODE64: u64 = 0x00AF_FA00_0000_FFFF;

/// Number of u64 slots in the GDT. Slots 5+6 host the 16-byte TSS
/// descriptor (high & low words). Computed at init time.
const GDT_LEN: usize = 7;

/// GDT slots. Slots 5+6 are filled in by `gdt_init` from the static TSS.
#[unsafe(no_mangle)]
static mut GDT: [u64; GDT_LEN] = [0; GDT_LEN];

/// User code segment selector (slot 4, RPL=3). Loaded into `CS` by the
/// `iretq` Ring 3 trampoline.
pub const USER_CS: u16 = 0x23;

/// User data segment selector (slot 3, RPL=3). Loaded into `SS` by the
/// `iretq` Ring 3 trampoline.
pub const USER_SS: u16 = 0x1B;

/// Kernel code segment selector (slot 1, RPL=0).
pub const KERNEL_CS: u16 = 0x08;

/// Kernel data segment selector (slot 2, RPL=0).
pub const KERNEL_SS: u16 = 0x10;

/// `MSR_STAR[63:48]` value that yields `SYSRET q` `CS = USER_CS = 0x23`
/// and `SS = USER_SS = 0x1B`. Derived from the SDM arithmetic
/// `CS = base + 16 | 3`, `SS = base + 8 | 3`.
pub const STAR_USER_BASE: u16 = 0x10;

/// `MSR_STAR[47:32]` value that yields `SYSCALL` `CS = KERNEL_CS = 0x08`
/// and `SS = KERNEL_SS = 0x10`. Derived from the SDM arithmetic
/// `CS = base`, `SS = base + 8`.
pub const STAR_KERNEL_BASE: u16 = 0x08;

/// GDTR pseudo-descriptor: 2-byte limit + 8-byte base (10 bytes, packed).
#[cfg(target_arch = "x86_64")]
#[repr(C, packed)]
struct Gdtr {
    limit: u16,
    base: u64,
}

/// Installs the kernel GDT and reloads all segment registers.
///
/// Must be called once from `kmain` before interrupts or any other
/// subsystem that depends on segment registers. Safe to call on the
/// existing stack; does not touch the heap.
///
/// # Order
///
/// `gdt_init` must precede [`super::tss::ltr_load`] (the TSS descriptor
/// at slots 5+6 must be in place before `ltr 0x28` executes).
///
/// # Segment selectors after return
///
/// | Segment | Selector  |
/// |---------|-----------|
/// | CS      | `0x08`    |
/// | DS/ES/SS| `0x10`    |
/// | FS/GS   | `0x00`    |
#[cfg(target_arch = "x86_64")]
pub fn gdt_init() {
    use core::arch::asm;

    // Populate the GDT entries. Slot 5+6 = TSS descriptor (low+high).
    let (tss_low, tss_high) = super::tss::tss_descriptor_for_static();
    // SAFETY: single-core init; GDT is not aliased.
    unsafe {
        let p = core::ptr::addr_of_mut!(GDT);
        (*p)[0] = 0;
        (*p)[1] = KCODE64;
        (*p)[2] = KDATA64;
        (*p)[3] = UDATA64;
        (*p)[4] = UCODE64;
        (*p)[5] = tss_low;
        (*p)[6] = tss_high;
    }

    let gdtr = Gdtr {
        // SAFETY: GDT has 7 u64 entries → limit = 7*8-1 = 55. Use raw
        // pointer to avoid taking a reference to a mutable static.
        limit: (GDT_LEN * core::mem::size_of::<u64>() - 1) as u16,
        // SAFETY: &raw GDT is a valid non-null pointer for the lifetime
        // of the kernel.
        base: core::ptr::addr_of!(GDT) as u64,
    };

    // SAFETY: Ring-0 bare-metal. `lgdt` and segment-register moves are
    // privileged but legal at CPL=0. The GDT we install is valid for
    // 64-bit long mode (L=1 code segments, flat data). We skip the
    // far-return CS reload because the bootloader uses the identical CS
    // selector (0x08) with the same attributes; the processor's cached
    // CS descriptor remains correct without an explicit flush.
    unsafe {
        asm!(
            // 1. Load our GDT.
            "lgdt [{gdtr}]",
            // 2. Reload data segments with kernel data selector (0x10).
            "mov {ds:e}, 0x10",
            "mov ds, {ds:x}",
            "mov es, {ds:x}",
            "mov ss, {ds:x}",
            // 3. Null out FS and GS (no thread-local storage yet).
            "xor {ds:e}, {ds:e}",
            "mov fs, {ds:x}",
            "mov gs, {ds:x}",
            gdtr = in(reg) core::ptr::addr_of!(gdtr) as u64,
            ds   = out(reg) _,
            options(nostack, preserves_flags),
        );
    }
}

/// No-op stub for non-x86_64 hosts (host unit-test builds on ARM, etc.).
#[cfg(not(target_arch = "x86_64"))]
pub fn gdt_init() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_code_selector_is_0x23() {
        assert_eq!(USER_CS, 0x23);
    }

    #[test]
    fn user_ss_selector_is_0x1b() {
        assert_eq!(USER_SS, 0x1B);
    }

    #[test]
    fn kernel_selectors_unchanged() {
        assert_eq!(KERNEL_CS, 0x08);
        assert_eq!(KERNEL_SS, 0x10);
    }

    #[test]
    fn sysret_arithmetic_matches_intel_sdm() {
        // STAR[63:48] = 0x10
        //   SYSRET q CS = base + 16 | 3
        //   SYSRET q SS = base + 8  | 3
        let base = u32::from(STAR_USER_BASE);
        let cs = (base + 16) | 3;
        let ss = (base + 8) | 3;
        assert_eq!(cs as u16, USER_CS);
        assert_eq!(ss as u16, USER_SS);
    }

    #[test]
    fn syscall_arithmetic_matches_intel_sdm() {
        // STAR[47:32] = 0x08
        //   SYSCALL CS = base
        //   SYSCALL SS = base + 8
        let base = u32::from(STAR_KERNEL_BASE);
        let cs = base;
        let ss = base + 8;
        assert_eq!(cs as u16, KERNEL_CS);
        assert_eq!(ss as u16, KERNEL_SS);
    }

    #[test]
    fn user_data_descriptor_access_is_0xf2() {
        let access = (UDATA64 >> 40) & 0xFF;
        assert_eq!(access, 0xF2);
    }

    #[test]
    fn user_code_descriptor_access_is_0xfa() {
        let access = (UCODE64 >> 40) & 0xFF;
        assert_eq!(access, 0xFA);
    }

    #[test]
    fn user_code_descriptor_is_long_mode() {
        // Flags nibble (bits 52..56) must include L=1 (bit 53).
        let flags = (UCODE64 >> 52) & 0xF;
        assert_eq!(flags & 0x2, 0x2);
    }
}
