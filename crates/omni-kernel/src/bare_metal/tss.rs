//! Task State Segment (`TSS`) for `x86_64` long mode (MB11).
//!
//! In long mode the TSS no longer holds full task state for hardware
//! task switching (the feature is unavailable in 64-bit mode). It does
//! still serve two essential roles:
//!
//! 1. **`rsp0`** — the kernel stack pointer the CPU loads when a Ring 3
//!    → Ring 0 privilege transition happens via an interrupt or
//!    exception. Without a valid `rsp0`, a #PF arriving while user code
//!    is executing would land on the user stack and likely fault again.
//! 2. **`ist1..ist7`** — Interrupt Stack Table entries: per-vector
//!    dedicated kernel stacks used when the corresponding IDT entry's
//!    IST field is non-zero. MB11 uses IST1 for `#DF` (already planned
//!    in ADR-0002) and IST2 for `#PF` (so a stack-overflow inside a
//!    user-mode page fault handler does not double-fault).
//!
//! Loaded via `ltr <selector>` after the GDT install. ADR-0004 § 3.

#![allow(unsafe_code, reason = "static mut TSS + ltr asm; SAFETY per init fn")]

#[cfg(target_arch = "x86_64")]
use core::arch::asm;

/// `x86_64` TSS layout per Intel SDM Vol 3A §7.7. 104 bytes packed.
#[repr(C, packed)]
pub struct Tss {
    reserved0: u32,
    /// Ring 0 kernel stack pointer. CPU loads this on Ring 3 → Ring 0
    /// transition via interrupt/exception.
    pub rsp0: u64,
    /// Ring 1 stack pointer (unused in long mode but field is reserved).
    pub rsp1: u64,
    /// Ring 2 stack pointer (unused in long mode but field is reserved).
    pub rsp2: u64,
    reserved1: u64,
    /// Interrupt Stack Table entry 1 — dedicated stack for #DF.
    pub ist1: u64,
    /// Interrupt Stack Table entry 2 — dedicated stack for #PF.
    pub ist2: u64,
    /// Interrupt Stack Table entry 3 (reserved for future use).
    pub ist3: u64,
    /// Interrupt Stack Table entry 4 (reserved for future use).
    pub ist4: u64,
    /// Interrupt Stack Table entry 5 (reserved for future use).
    pub ist5: u64,
    /// Interrupt Stack Table entry 6 (reserved for future use).
    pub ist6: u64,
    /// Interrupt Stack Table entry 7 (reserved for future use).
    pub ist7: u64,
    reserved2: u64,
    reserved3: u16,
    /// I/O permission bitmap base — set to `sizeof(Tss)` (104) to
    /// indicate that there is no I/O bitmap.
    pub iomap_base: u16,
}

impl Tss {
    /// Construct an all-zero TSS with `iomap_base` set to 104 (the
    /// canonical "no I/O bitmap" value).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved2: 0,
            reserved3: 0,
            iomap_base: 104,
        }
    }
}

impl Default for Tss {
    fn default() -> Self {
        Self::new()
    }
}

/// Single, global, ring-0 TSS. MB11 is single-CPU; MP will introduce
/// per-CPU TSS arrays in a future milestone.
#[unsafe(no_mangle)]
static mut TSS: Tss = Tss::new();

/// Set the kernel-stack pointer the CPU loads on Ring 3 → Ring 0
/// privilege transitions (interrupts / exceptions from user mode).
///
/// Must be called by the scheduler on every context switch into a
/// user-process task, with `rsp0` set to the top of that process's
/// kernel stack.
#[cfg(target_arch = "x86_64")]
pub fn set_rsp0(rsp0: u64) {
    // SAFETY: single-core; TSS is not aliased; raw pointer write.
    unsafe {
        let p = core::ptr::addr_of_mut!(TSS);
        (*p).rsp0 = rsp0;
    }
}

/// Stub for non-x86_64 host builds.
#[cfg(not(target_arch = "x86_64"))]
pub fn set_rsp0(_rsp0: u64) {}

/// TSS GDT selector — slot 5 with RPL=0 = `0x28`.
pub const TSS_SELECTOR: u16 = 0x28;

/// Compute the two 8-byte words that encode a 64-bit TSS descriptor at
/// GDT slots `[slot, slot+1]`.
///
/// Per Intel SDM Vol 3A §7.2.3:
/// - Low word: limit\[15:0\], base\[15:0\], base\[23:16\], type=0x9
///   (available 64-bit TSS), S=0, DPL=0, P=1, limit\[19:16\], G=0,
///   base\[31:24\].
/// - High word: base\[63:32\], reserved.
#[must_use]
pub fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let limit_lo = u64::from(limit) & 0xFFFF;
    let limit_hi = (u64::from(limit) >> 16) & 0xF;
    let base_lo = base & 0xFFFF;
    let base_mid = (base >> 16) & 0xFF;
    let base_mid_hi = (base >> 24) & 0xFF;
    let base_hi = base >> 32;
    // Access byte 0x89 = P=1, DPL=00, S=0, type=0x9 (avail. 64-bit TSS).
    // Flags nibble bits 52..56 = 0 (G=0).
    let low = limit_lo
        | (base_lo << 16)
        | (base_mid << 32)
        | (0x89_u64 << 40)
        | (limit_hi << 48)
        | (base_mid_hi << 56);
    let high = base_hi;
    (low, high)
}

/// Build the (low, high) TSS descriptor words for the static `TSS`.
#[must_use]
pub fn tss_descriptor_for_static() -> (u64, u64) {
    let base = core::ptr::addr_of!(TSS) as u64;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "sizeof(Tss) = 104 fits u32 trivially"
    )]
    let limit = (core::mem::size_of::<Tss>() - 1) as u32;
    tss_descriptor(base, limit)
}

/// Issue `ltr <selector>` to install the TSS as the current task
/// register. Called once at boot after `lgdt`.
#[cfg(target_arch = "x86_64")]
pub fn ltr_load() {
    // SAFETY: ltr is privileged but legal at CPL=0; selector points to
    // a present TSS descriptor in the current GDT.
    unsafe {
        asm!("ltr {sel:x}", sel = in(reg) TSS_SELECTOR, options(nostack, preserves_flags));
    }
}

/// Stub for non-x86_64 host builds.
#[cfg(not(target_arch = "x86_64"))]
pub fn ltr_load() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_size_is_104_bytes() {
        assert_eq!(core::mem::size_of::<Tss>(), 104);
    }

    #[test]
    fn iomap_base_equals_104() {
        let t = Tss::new();
        let iomap = t.iomap_base;
        assert_eq!(iomap, 104);
    }

    #[test]
    fn tss_selector_is_0x28() {
        assert_eq!(TSS_SELECTOR, 0x28);
    }

    #[test]
    fn descriptor_access_byte_is_0x89() {
        let (low, _high) = tss_descriptor(0xDEAD_BEEF_CAFE_F00D, 103);
        let access = (low >> 40) & 0xFF;
        assert_eq!(access, 0x89);
    }

    #[test]
    fn descriptor_limit_encoded() {
        let (low, _high) = tss_descriptor(0, 103);
        let limit_lo = low & 0xFFFF;
        let limit_hi = (low >> 48) & 0xF;
        assert_eq!(limit_lo, 103);
        assert_eq!(limit_hi, 0);
    }
}
