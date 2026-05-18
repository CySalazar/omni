//! Kernel Global Descriptor Table (`GDT`) for `x86_64` long mode.
//!
//!
//! Installs a minimal 3-entry GDT under kernel control, replacing the
//! bootloader's temporary GDT. Segments: null (0x00), kernel code (0x08),
//! kernel data (0x10). All kernel-mode (DPL=0); user-mode segments and
//! TSS are deferred to P6.3 (ring-3 enablement).
//!
//! Call [`gdt_init`] once from `kmain`, before any other subsystem.

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
/// Access  0x9B = P=1, DPL=00, S=1, type=0xB (execute/read, A=1 pre-set)
/// Flags   0xA  = G=1, L=1, D/B=0, AVL=0
///
/// The A (Accessed) bit is pre-set so that the CPU does not attempt to write
/// it on first segment-register load. Without it the CPU would write to this
/// page and trigger a #PF because the GDT lives in a read-only ELF segment.
const KCODE64: u64 = 0x00AF_9B00_0000_FFFF;

/// Kernel data segment (DPL=0, G=1, D/B=1).
///
/// Access  0x93 = P=1, DPL=00, S=1, type=0x3 (read/write, A=1 pre-set)
/// Flags   0xC  = G=1, D/B=1, L=0, AVL=0
///
/// A bit pre-set for the same reason as KCODE64.
const KDATA64: u64 = 0x00CF_9300_0000_FFFF;

/// Flat GDT: [null, kernel code, kernel data].
#[unsafe(no_mangle)]
static GDT: [u64; 3] = [0, KCODE64, KDATA64];

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
/// # Segment selectors after return
///
/// | Segment | Selector |
/// |---------|----------|
/// | CS      | `0x08` (inherited from bootloader; explicit reload deferred to P6.3) |
/// | DS/ES/SS| `0x10`   |
/// | FS/GS   | `0x00` (null — no thread-local / syscall use yet) |
#[cfg(target_arch = "x86_64")]
pub fn gdt_init() {
    use core::arch::asm;

    let gdtr = Gdtr {
        // SAFETY: GDT has 3 entries → limit = 3*8-1 = 23.
        limit: (core::mem::size_of_val(&GDT) - 1) as u16,
        // SAFETY: &GDT is a valid non-null pointer for the lifetime of the kernel.
        base: core::ptr::addr_of!(GDT) as u64,
    };

    // SAFETY: Ring-0 bare-metal. `lgdt` and segment-register moves are
    // privileged but legal at CPL=0. The GDT we install is valid for
    // 64-bit long mode (L=1 code segment, flat data). We skip the far-return
    // CS reload because the bootloader uses the identical CS selector (0x08)
    // with the same attributes; the processor's cached CS descriptor remains
    // correct without an explicit flush. A far return will be added at P6.3
    // when the user-mode (ring-3) or TSS descriptors require a new CS layout.
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
