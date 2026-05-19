//! MB14.c.2.c — Application Processor landing stub + higher-half entry.
//!
//! Companion to [`mp_trampoline`](super::mp_trampoline) /
//! [`mp_emplacement`](super::mp_emplacement). The trampoline brings each AP
//! from 16-bit real mode to 64-bit long mode and jumps to a caller-supplied
//! `kernel_ap_entry`. MB14.c.2.c routes that jump through a small low-memory
//! landing stub that:
//!
//! 1. Atomically increments an ack counter mapped in the temp paging window.
//! 2. Loads the BSP's kernel `CR3` (the active kernel address space).
//! 3. Switches `CR3` to the kernel address space.
//! 4. Jumps to a higher-half [`kmain_ap`] entry point with the kernel
//!    address space active.
//!
//! Step 2 reads `CR3` and the [`kmain_ap`] VA from per-call slots emplaced
//! by the BSP at well-known offsets inside the trampoline page (`0x8148`
//! and `0x8150`). Both reads happen **before** the `mov cr3, rax`, so the
//! values reach a register while the temp PML4 still identity-maps the
//! trampoline page. After the switch, the next instruction (`jmp rcx`) is
//! still fetched from `RIP` ≈ `0x813X` because the BSP's kernel address
//! space also identity-maps the trampoline page (the c.2.b.2 emplacement
//! installs that mapping defensively for exactly this reason).
//!
//! [`kmain_ap`] is a `#[naked]` `extern "C"` function: no prologue,
//! no stack accesses. The AP arrives with no stack and the trampoline never
//! sets `RSP`; the `cli; hlt; jmp $-2` body is stack-free by construction.
//! Per-AP stack allocation lands in MB14.c.2.d together with the per-AP
//! `PerCpu` wiring.
//!
//! ## Layout inside the trampoline page (phys `0x0000_8000`)
//!
//! | Offset    | Size  | Content                                           |
//! |----------:|------:|---------------------------------------------------|
//! | `0x000`   | 256 B | Trampoline blob (see [`mp_trampoline`])           |
//! | `0x100`   | 32 B  | AP landing stub (this module)                     |
//! | `0x140`   | 8  B  | `AP_ACK_COUNTER` — `AtomicU64`, BSP reads it      |
//! | `0x148`   | 8  B  | `AP_KERNEL_CR3` — BSP writes pre-fire             |
//! | `0x150`   | 8  B  | `AP_KMAIN_AP_VA` — BSP writes pre-fire            |
//!
//! ## References
//!
//! - Intel SDM Vol 2 — `MOV CRn`, `JMP r/m64`, `LOCK ADD` opcodes
//! - Intel SDM Vol 3A § 4.10 — TLB invalidation on `MOV CR3`
//! - Intel SDM Vol 3A § 8.4   — MP Initialization Protocol

#![allow(
    unsafe_code,
    reason = "naked AP entry + raw landing-stub byte writer; both are MB14.c.2.c primitives"
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "every `as u8` extracts a single byte from a wider integer that has been explicitly bit-shifted to isolate it"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "every stub index is a compile-time constant within AP_LANDING_STUB_SIZE"
)]

// =====================================================================
// Layout constants — offsets relative to the trampoline page base.
// =====================================================================

/// Offset of the AP landing stub within the trampoline page.
///
/// The trampoline blob occupies `0x000..0x100` (see
/// [`mp_trampoline::TRAMPOLINE_BLOB_SIZE`]); the landing stub sits in the
/// next aligned slot so the two never overlap.
///
/// [`mp_trampoline::TRAMPOLINE_BLOB_SIZE`]: super::mp_trampoline::TRAMPOLINE_BLOB_SIZE
pub const AP_LANDING_STUB_OFFSET: usize = 0x100;

/// Length of the AP landing stub in bytes.
///
/// 32 bytes is enough for the four-instruction sequence emitted by
/// [`build_ap_landing_stub`] (`lock inc` + two `mov r64, [mem]` + `mov cr3`
/// + `jmp r/m64`). The trailing bytes are zero-padded.
pub const AP_LANDING_STUB_SIZE: usize = 32;

/// Offset of the `AP_ACK_COUNTER` slot (8-byte, little-endian) within the
/// trampoline page. The BSP zeroes this before firing INIT-SIPI; each AP
/// `lock inc`s it once before switching `CR3`.
pub const AP_ACK_COUNTER_OFFSET: usize = 0x140;

/// Offset of the `AP_KERNEL_CR3` slot (8-byte, little-endian) within the
/// trampoline page. Holds the physical address the AP will load into
/// `CR3` to enter the kernel address space.
pub const AP_KERNEL_CR3_OFFSET: usize = 0x148;

/// Offset of the `AP_KMAIN_AP_VA` slot (8-byte, little-endian) within the
/// trampoline page. Holds the virtual address of [`kmain_ap`] in the
/// higher-half kernel mapping.
pub const AP_KMAIN_AP_VA_OFFSET: usize = 0x150;

// =====================================================================
// Pure-function landing-stub builder.
// =====================================================================

/// Build the 32-byte AP landing stub.
///
/// `tramp_base_paddr` is the physical address of the trampoline page
/// (always [`super::mp_emplacement::TRAMPOLINE_PHYS_BASE`] = `0x0000_8000`
/// in MB14.c.2.c). The four slot addresses are derived from it.
///
/// The stub is **position-dependent**: the absolute addresses of the
/// `AP_ACK_COUNTER`, `AP_KERNEL_CR3`, and `AP_KMAIN_AP_VA` slots are
/// embedded as 32-bit displacements in `mov r64, [mem32]` instructions
/// (with REX.W + 0xA1 / 0x8B opcodes — see below). Slots must therefore
/// fit in 32 bits, which holds for any low-memory trampoline placement.
///
/// ## Instruction sequence
///
/// ```text
///   F0 48 FF 04 25 <ack32>     ; lock inc qword ptr [ack32]
///   48 8B 0C 25 <cr3_32>       ; mov rcx, [cr3_32]      (kernel CR3)
///   48 8B 14 25 <vaslot32>     ; mov rdx, [vaslot32]    (kmain_ap VA)
///   0F 22 D9                   ; mov cr3, rcx           (switch AS)
///   FF E2                      ; jmp rdx                (enter kmain_ap)
/// ```
///
/// The sequence has been picked so that **both** runtime slots reach a
/// register **before** the `mov cr3` clobbers the address space. After
/// the switch the next byte fetched is the `jmp rdx` opcode at
/// `tramp_base + 0x11D`, which the BSP's kernel CR3 must also map (the
/// c.2.b.2 emplacement identity-maps the trampoline page in active CR3
/// for exactly this reason).
#[must_use]
pub fn build_ap_landing_stub(tramp_base_paddr: u32) -> [u8; AP_LANDING_STUB_SIZE] {
    let mut s = [0u8; AP_LANDING_STUB_SIZE];

    let ack_paddr = tramp_base_paddr.wrapping_add(AP_ACK_COUNTER_OFFSET as u32);
    let cr3_paddr = tramp_base_paddr.wrapping_add(AP_KERNEL_CR3_OFFSET as u32);
    let va_paddr = tramp_base_paddr.wrapping_add(AP_KMAIN_AP_VA_OFFSET as u32);

    // -----------------------------------------------------------------
    // 0x00  F0 48 FF 04 25 <imm32>   lock inc qword ptr [imm32]
    //   F0      = LOCK prefix
    //   48      = REX.W (64-bit operand)
    //   FF /0   = INC r/m64; ModR/M 04 = mod=00 reg=0 (/0=INC) rm=100 (SIB)
    //   25      = SIB scale=00 index=100 (none) base=101 (disp32 absolute)
    //   imm32   = absolute physical address of AP_ACK_COUNTER slot
    // -----------------------------------------------------------------
    s[0x00] = 0xF0;
    s[0x01] = 0x48;
    s[0x02] = 0xFF;
    s[0x03] = 0x04;
    s[0x04] = 0x25;
    s[0x05] = ack_paddr as u8;
    s[0x06] = (ack_paddr >> 8) as u8;
    s[0x07] = (ack_paddr >> 16) as u8;
    s[0x08] = (ack_paddr >> 24) as u8;

    // -----------------------------------------------------------------
    // 0x09  48 8B 0C 25 <imm32>   mov rcx, [imm32]
    //   48      = REX.W
    //   8B      = MOV r64, r/m64
    //   0C      = ModR/M mod=00 reg=001 (RCX) rm=100 (SIB)
    //   25      = SIB scale=00 index=100 (none) base=101 (disp32 absolute)
    //   imm32   = absolute physical address of AP_KERNEL_CR3 slot
    // -----------------------------------------------------------------
    s[0x09] = 0x48;
    s[0x0A] = 0x8B;
    s[0x0B] = 0x0C;
    s[0x0C] = 0x25;
    s[0x0D] = cr3_paddr as u8;
    s[0x0E] = (cr3_paddr >> 8) as u8;
    s[0x0F] = (cr3_paddr >> 16) as u8;
    s[0x10] = (cr3_paddr >> 24) as u8;

    // -----------------------------------------------------------------
    // 0x11  48 8B 14 25 <imm32>   mov rdx, [imm32]
    //   14 = ModR/M reg=010 (RDX) rm=100 (SIB) — same SIB byte.
    // -----------------------------------------------------------------
    s[0x11] = 0x48;
    s[0x12] = 0x8B;
    s[0x13] = 0x14;
    s[0x14] = 0x25;
    s[0x15] = va_paddr as u8;
    s[0x16] = (va_paddr >> 8) as u8;
    s[0x17] = (va_paddr >> 16) as u8;
    s[0x18] = (va_paddr >> 24) as u8;

    // -----------------------------------------------------------------
    // 0x19  0F 22 D9   mov cr3, rcx
    //   0F 22  = MOV CR, r64 family
    //   D9     = ModR/M mod=11 reg=011 (CR3) rm=001 (RCX)
    // -----------------------------------------------------------------
    s[0x19] = 0x0F;
    s[0x1A] = 0x22;
    s[0x1B] = 0xD9;

    // -----------------------------------------------------------------
    // 0x1C  FF E2   jmp rdx
    //   FF /4  = JMP r/m64; ModR/M E2 = mod=11 reg=100 (/4) rm=010 (RDX)
    // -----------------------------------------------------------------
    s[0x1C] = 0xFF;
    s[0x1D] = 0xE2;

    // 0x1E..0x20 zero padding (NOPs would be equivalent; the AP never
    // executes past `jmp rdx`).
    s
}

// =====================================================================
// Higher-half AP entry — naked, no stack accesses.
// =====================================================================

// Bare-metal AP entry: defined via `global_asm!` so we avoid the
// unstable `#[naked]` attribute on Rust 1.85. The body is functionally
// equivalent to a naked `cli; 1: hlt; jmp 1b` — no prologue, no stack
// accesses, never returns. The `#[no_mangle]` symbol `kmain_ap` is
// exposed for the BSP to discover its address via `kmain_ap as u64`.
//
// MB14.c.2.d will replace this body with a real per-CPU init sequence
// (per-AP `PerCpu`, kernel stack, IDT, GDT, `swapgs` of `GS_BASE`).
//
// Section: emit into `.text.kmain_ap` so the linker keeps the symbol
// alive even when LTO sees no Rust caller (the only caller is the
// landing stub via an absolute physical-memory pointer, which LTO
// cannot reason about).
#[cfg(all(target_arch = "x86_64", target_os = "none", not(test)))]
core::arch::global_asm!(
    ".section .text.kmain_ap, \"ax\", @progbits",
    ".global kmain_ap",
    ".type kmain_ap, @function",
    "kmain_ap:",
    "    cli",
    "1:  hlt",
    "    jmp 1b",
);

// Higher-half landing point for every Application Processor in
// MB14.c.2.c. The AP arrives here with:
//
// - `CR3` = the BSP's kernel address space (loaded by the landing stub
//   from the AP_KERNEL_CR3 slot).
// - No stack. `RSP` is whatever the firmware left at AP reset; we do
//   not touch it.
// - No IDT loaded on this CPU. Interrupts are masked (CLI from the
//   trampoline's first instruction); the HLT loop above relies on
//   maskable interrupts staying disabled.
// - The temp GDT from the trampoline page still in `GDTR`. The AP
//   never returns from this function, so reloading a real per-CPU GDT
//   is deferred to MB14.c.2.d.
//
// Note: `extern` blocks cannot carry rustdoc; document via this comment.
#[cfg(all(target_arch = "x86_64", target_os = "none", not(test)))]
unsafe extern "C" {
    /// AP entry point — defined via the `global_asm!` block above.
    pub fn kmain_ap() -> !;
}

/// Host-stub for non-bare-metal builds.
#[cfg(not(all(target_arch = "x86_64", target_os = "none", not(test))))]
#[allow(
    dead_code,
    reason = "host stub keeps the symbol resolvable from `cargo test --workspace --all-features` builds"
)]
pub extern "C" fn kmain_ap() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

// =====================================================================
// Host-side tests
// =====================================================================

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "tests panic on bounds violation by design — surfaces builder regressions as test failures, not silent wrong bytes"
)]
mod tests {
    use super::*;

    /// Canonical trampoline base used by every MB14.c.2.* test.
    const TRAMP: u32 = 0x0000_8000;

    #[test]
    fn stub_starts_with_lock_inc_ack_counter() {
        let s = build_ap_landing_stub(TRAMP);
        // F0 48 FF 04 25 <imm32>
        assert_eq!(s[0x00], 0xF0, "LOCK prefix");
        assert_eq!(s[0x01], 0x48, "REX.W");
        assert_eq!(s[0x02], 0xFF, "INC opcode");
        assert_eq!(s[0x03], 0x04, "ModR/M /0 + SIB-mode");
        assert_eq!(s[0x04], 0x25, "SIB disp32 absolute");
        let imm = u32::from_le_bytes([s[0x05], s[0x06], s[0x07], s[0x08]]);
        assert_eq!(
            imm,
            TRAMP + AP_ACK_COUNTER_OFFSET as u32,
            "ack-counter disp32 must point at AP_ACK_COUNTER slot"
        );
    }

    #[test]
    fn stub_loads_kernel_cr3_before_cr3_switch() {
        let s = build_ap_landing_stub(TRAMP);
        // 48 8B 0C 25 <imm32> = mov rcx, [imm32]
        assert_eq!(&s[0x09..0x0D], &[0x48, 0x8B, 0x0C, 0x25]);
        let imm = u32::from_le_bytes([s[0x0D], s[0x0E], s[0x0F], s[0x10]]);
        assert_eq!(
            imm,
            TRAMP + AP_KERNEL_CR3_OFFSET as u32,
            "CR3 disp32 must point at AP_KERNEL_CR3 slot"
        );
    }

    #[test]
    fn stub_loads_kmain_ap_va_before_cr3_switch() {
        let s = build_ap_landing_stub(TRAMP);
        // 48 8B 14 25 <imm32> = mov rdx, [imm32]
        assert_eq!(&s[0x11..0x15], &[0x48, 0x8B, 0x14, 0x25]);
        let imm = u32::from_le_bytes([s[0x15], s[0x16], s[0x17], s[0x18]]);
        assert_eq!(
            imm,
            TRAMP + AP_KMAIN_AP_VA_OFFSET as u32,
            "kmain_ap-VA disp32 must point at AP_KMAIN_AP_VA slot"
        );
    }

    #[test]
    fn stub_switches_cr3_via_rcx() {
        let s = build_ap_landing_stub(TRAMP);
        // 0F 22 D9 = mov cr3, rcx
        assert_eq!(&s[0x19..0x1C], &[0x0F, 0x22, 0xD9]);
    }

    #[test]
    fn stub_jumps_to_rdx_after_cr3_switch() {
        let s = build_ap_landing_stub(TRAMP);
        // FF E2 = jmp rdx
        assert_eq!(&s[0x1C..0x1E], &[0xFF, 0xE2]);
    }

    #[test]
    fn stub_tail_is_zero_padded() {
        // Bytes after `jmp rdx` are never executed; pin the padding so a
        // future refactor cannot smuggle live bytes past the jump.
        let s = build_ap_landing_stub(TRAMP);
        for (i, b) in s.iter().enumerate().skip(0x1E) {
            assert_eq!(*b, 0, "stub byte {i:#x} must stay zero");
        }
    }

    #[test]
    fn stub_size_matches_constant() {
        let s = build_ap_landing_stub(TRAMP);
        assert_eq!(s.len(), AP_LANDING_STUB_SIZE);
    }

    #[test]
    fn slot_offsets_do_not_overlap_with_blob_or_stub() {
        // Trampoline blob at [0x000..0x100), landing stub at
        // [0x100..0x120), slots at [0x140..0x158). The 32-byte gap
        // between stub end and first slot is reserved for future
        // expansion (e.g. per-AP stack-top pointers in MB14.c.2.d).
        const _STUB_DOES_NOT_OVERLAP_BLOB: () =
            assert!(AP_LANDING_STUB_OFFSET >= 256);
        const _STUB_FITS_BEFORE_SLOTS: () =
            assert!(AP_LANDING_STUB_OFFSET + AP_LANDING_STUB_SIZE <= AP_ACK_COUNTER_OFFSET);
        const _SLOTS_ARE_8_BYTE_ALIGNED: () = assert!(
            AP_ACK_COUNTER_OFFSET % 8 == 0
                && AP_KERNEL_CR3_OFFSET % 8 == 0
                && AP_KMAIN_AP_VA_OFFSET % 8 == 0
        );
        const _SLOTS_ARE_DISTINCT_AND_ORDERED: () = assert!(
            AP_ACK_COUNTER_OFFSET < AP_KERNEL_CR3_OFFSET
                && AP_KERNEL_CR3_OFFSET < AP_KMAIN_AP_VA_OFFSET
        );
    }

    #[test]
    fn slot_offsets_reach_within_one_page() {
        const _ALL_SLOTS_IN_PAGE: () = assert!(AP_KMAIN_AP_VA_OFFSET + 8 <= 4096);
    }

    #[test]
    fn ack_counter_disp_changes_when_base_changes() {
        // Pin: the ack-counter disp32 isolates to a single 4-byte
        // window. A different trampoline base must change only that
        // window plus the two other slot disp fields.
        let a = build_ap_landing_stub(0x0000_8000);
        let b = build_ap_landing_stub(0x0000_9000);
        // Bytes 0x00..0x04 (LOCK / REX / opcode / ModR/M / SIB) stay.
        assert_eq!(&a[0x00..0x05], &b[0x00..0x05]);
        // 0x05..0x09 (ack disp32) differ.
        assert_ne!(&a[0x05..0x09], &b[0x05..0x09]);
    }
}
