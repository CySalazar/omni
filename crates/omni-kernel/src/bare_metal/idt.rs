//! Interrupt Descriptor Table (IDT) for `x86_64` long mode (Track B, MB3).
//!
//! Installs a minimal IDT that catches the four most critical synchronous
//! exceptions before they cause silent triple-faults:
//!
//! | Vector | Mnemonic | Name               | Error code? |
//! |--------|----------|--------------------|-------------|
//! | 0      | #DE      | Divide Error       | No          |
//! | 8      | #DF      | Double Fault       | Yes (always 0) |
//! | 13     | #GP      | General Protection | Yes         |
//! | 14     | #PF      | Page Fault         | Yes         |
//!
//! All four handlers write the exception vector and (where present) the
//! error code to the early console, then halt the CPU forever. The IDT
//! is loaded with `lidt` but interrupts are NOT enabled with `sti` in
//! this release — the IDT only catches synchronous (fault/trap) exceptions.
//!
//! ## Calling convention
//!
//! `x86_64` stable Rust does not expose `extern "x86-interrupt"` on the
//! stable channel (it is a nightly-only ABI). The handlers are therefore
//! implemented as `extern "C"` functions invoked from minimal `global_asm!`
//! stubs that set up the correct argument registers before the call.
//!
//! ## Pattern
//!
//! Follows the exact same pattern as [`super::gdt`] — a static descriptor
//! table, a pseudo-descriptor struct (`Idtr` / `Gdtr`), and an `_init()`
//! function that loads the table via a privileged instruction.

use super::early_console;

// -----------------------------------------------------------------------
// IDT entry (16 bytes in x86_64 long mode)
// -----------------------------------------------------------------------

/// A single `x86_64` IDT entry for a 64-bit interrupt gate.
///
/// Layout (all fields little-endian):
///
/// | Bytes  | Field         | Description                          |
/// |--------|---------------|--------------------------------------|
/// | 0–1    | offset_low    | Handler VA bits \[15:0\]               |
/// | 2–3    | selector      | Code segment selector (0x08)         |
/// | 4      | ist_and_zero  | IST index in bits \[2:0\]; rest zero   |
/// | 5      | type_and_attr | P=1, DPL=0, type=0xE (interrupt gate)|
/// | 6–7    | offset_mid    | Handler VA bits \[31:16\]              |
/// | 8–11   | offset_high   | Handler VA bits \[63:32\]              |
/// | 12–15  | _reserved     | Must be zero                         |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist_and_zero: u8,
    type_and_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    _reserved: u32,
}

impl IdtEntry {
    /// Creates a not-present (all-zero) sentinel entry.
    ///
    /// Using `const fn` allows `[IdtEntry::missing(); 256]` in a static
    /// without requiring runtime initialization.
    #[must_use]
    pub const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist_and_zero: 0,
            type_and_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            _reserved: 0,
        }
    }

    /// Creates a 64-bit interrupt gate for `handler` using `selector`.
    ///
    /// - `type_and_attr` = `0x8E` (P=1, DPL=00, type=0xE = 64-bit interrupt gate)
    /// - IST index = 0 (no IST switching)
    #[must_use]
    pub fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist_and_zero: 0x00,
            type_and_attr: 0x8E, // P=1, DPL=0, type=0xE
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: ((handler >> 32) & 0xFFFF_FFFF) as u32,
            _reserved: 0,
        }
    }
}

// -----------------------------------------------------------------------
// Exception frame pushed by the CPU on the stack
// -----------------------------------------------------------------------

/// The interrupt/exception stack frame pushed by the CPU before calling
/// the handler stub. Fields ordered from lowest to highest stack address.
#[derive(Debug)]
#[repr(C)]
pub struct ExceptionFrame {
    /// Instruction pointer of the faulting instruction.
    pub rip: u64,
    /// Code segment selector at the time of the fault.
    pub cs: u64,
    /// RFLAGS at the time of the fault.
    pub rflags: u64,
    /// Stack pointer at the time of the fault.
    pub rsp: u64,
    /// Stack segment at the time of the fault.
    pub ss: u64,
}

// -----------------------------------------------------------------------
// IDTR pseudo-descriptor
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

// -----------------------------------------------------------------------
// Static IDT — 256 entries, all initially "missing"
// -----------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];

#[cfg(target_arch = "x86_64")]
const KERNEL_CS: u16 = 0x08;

// -----------------------------------------------------------------------
// ISR assembly stubs (x86_64 only, Intel syntax)
// -----------------------------------------------------------------------
//
// Each stub aligns the stack (System V AMD64 ABI §3.2.2) before calling
// the extern "C" Rust handler. Exceptions without an error code have 5
// items on the stack (40 bytes; 8 mod 16 from a 16-aligned interrupt);
// we add 8 bytes of padding to restore 0 mod 16 alignment before `call`.
// Exceptions with an error code have 6 items (48 bytes; 0 mod 16), so
// we pop the error code into RSI (2nd arg) and add 8 bytes of padding.
//
// Stubs end with `ud2` (undefined instruction) — the Rust handlers call
// `halt_forever()` and never return, so this is a safety net.
//
// Notes on `lea rdi, [rsp+8]`:  after `sub rsp, 8`, RSP points to the
// 8-byte alignment pad; the ExceptionFrame starts 8 bytes above at RSP+8.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    // ---- #DE: Divide Error — no error code ----
    ".global isr_de",
    "isr_de:",
    "    sub rsp, 8",
    "    lea rdi, [rsp + 8]",
    "    call kernel_handle_de",
    "    ud2",
    // ---- #DF: Double Fault — error code (always 0) ----
    ".global isr_df",
    "isr_df:",
    "    pop rsi",
    "    sub rsp, 8",
    "    lea rdi, [rsp + 8]",
    "    call kernel_handle_df",
    "    ud2",
    // ---- #GP: General Protection Fault — error code ----
    ".global isr_gp",
    "isr_gp:",
    "    pop rsi",
    "    sub rsp, 8",
    "    lea rdi, [rsp + 8]",
    "    call kernel_handle_gp",
    "    ud2",
    // ---- #PF: Page Fault — error code ----
    ".global isr_pf",
    "isr_pf:",
    "    pop rsi",
    "    sub rsp, 8",
    "    lea rdi, [rsp + 8]",
    "    call kernel_handle_pf",
    "    ud2",
);

// Extern declarations so that Rust can take the address of each stub.
#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn isr_de();
    fn isr_df();
    fn isr_gp();
    fn isr_pf();
}

// -----------------------------------------------------------------------
// Exception handlers (extern "C" so stubs can call them)
// -----------------------------------------------------------------------

#[unsafe(no_mangle)]
extern "C" fn kernel_handle_de(frame: *const ExceptionFrame) {
    early_console::write_str("\n[OMNI OS EXCEPTION] #DE Divide Error\n");
    log_frame(frame);
    super::arch::halt_forever()
}

#[unsafe(no_mangle)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "error code fits usize on any supported target"
)]
extern "C" fn kernel_handle_df(frame: *const ExceptionFrame, error_code: u64) {
    early_console::write_str("\n[OMNI OS EXCEPTION] #DF Double Fault  code=");
    early_console::write_usize(error_code as usize);
    early_console::write_str("\n");
    log_frame(frame);
    super::arch::halt_forever()
}

#[unsafe(no_mangle)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "error code fits usize on any supported target"
)]
extern "C" fn kernel_handle_gp(frame: *const ExceptionFrame, error_code: u64) {
    early_console::write_str("\n[OMNI OS EXCEPTION] #GP General Protection  code=");
    early_console::write_usize(error_code as usize);
    early_console::write_str("\n");
    log_frame(frame);
    super::arch::halt_forever()
}

#[unsafe(no_mangle)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "error code fits usize on any supported target"
)]
extern "C" fn kernel_handle_pf(frame: *const ExceptionFrame, error_code: u64) {
    // Snapshot CR2 (faulting linear address) before anything else can
    // touch it — `early_console::write_*` are pure I/O, but be defensive.
    let cr2 = super::arch::read_cr2();
    early_console::write_str("\n[OMNI OS EXCEPTION] #PF Page Fault  code=");
    early_console::write_usize(error_code as usize);
    early_console::write_str("  cr2=");
    early_console::write_usize(cr2 as usize);
    early_console::write_str("\n");
    log_frame(frame);
    super::arch::halt_forever()
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "register values fit usize on x86_64"
)]
fn log_frame(frame: *const ExceptionFrame) {
    if frame.is_null() {
        return;
    }
    // SAFETY: the stub guarantees the pointer is valid — it points to
    // the CPU-pushed exception frame on the kernel stack.
    let f = unsafe { &*frame };
    early_console::write_str("  rip=");
    early_console::write_usize(f.rip as usize);
    early_console::write_str("  cs=");
    early_console::write_usize(f.cs as usize);
    early_console::write_str("  rflags=");
    early_console::write_usize(f.rflags as usize);
    early_console::write_str("\n");
}

// -----------------------------------------------------------------------
// idt_init — loads the IDT via `lidt`
// -----------------------------------------------------------------------

/// Installs the kernel IDT and loads it with `lidt`.
///
/// Must be called from `kmain` after [`super::gdt::gdt_init`]. Does NOT
/// issue `sti` — interrupts remain disabled; only synchronous (fault)
/// exceptions are handled.
///
/// # Vectors installed
///
/// | Vector | Handler          |
/// |--------|-----------------|
/// | 0      | `kernel_handle_de` (#DE) |
/// | 8      | `kernel_handle_df` (#DF) |
/// | 13     | `kernel_handle_gp` (#GP) |
/// | 14     | `kernel_handle_pf` (#PF) |
#[cfg(target_arch = "x86_64")]
pub fn idt_init() {
    use core::arch::asm;

    // SAFETY: single-core bare-metal; IDT is not aliased by any other
    // code path at this point in initialisation.
    let idt = unsafe { &mut *core::ptr::addr_of_mut!(IDT) };

    idt[0] = IdtEntry::interrupt_gate(isr_de as usize as u64, KERNEL_CS);
    idt[8] = IdtEntry::interrupt_gate(isr_df as usize as u64, KERNEL_CS);
    idt[13] = IdtEntry::interrupt_gate(isr_gp as usize as u64, KERNEL_CS);
    idt[14] = IdtEntry::interrupt_gate(isr_pf as usize as u64, KERNEL_CS);

    let idtr = Idtr {
        limit: (core::mem::size_of_val(idt) - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };

    // SAFETY: Ring-0 bare-metal. `lidt` is a privileged but otherwise
    // side-effect-free instruction that installs the IDT. The table we
    // pass is valid for 64-bit long mode (256 × 16-byte interrupt gates).
    unsafe {
        asm!(
            "lidt [{idtr}]",
            idtr = in(reg) core::ptr::addr_of!(idtr) as u64,
            options(nostack, preserves_flags),
        );
    }
}

/// No-op stub for non-x86_64 hosts (developer machines on ARM, etc.).
#[cfg(not(target_arch = "x86_64"))]
pub fn idt_init() {}

// -----------------------------------------------------------------------
// idt_set_vector — install a single interrupt gate after idt_init
// -----------------------------------------------------------------------

/// Installs a single 64-bit interrupt gate at `vector` in the IDT.
///
/// Can be called after [`idt_init`] to register additional vectors. Used
/// by `syscall_entry::syscall_init` to register the `INT 0x80` fallback.
#[cfg(target_arch = "x86_64")]
pub fn idt_set_vector(vector: usize, handler: u64) {
    // SAFETY: single-core bare-metal; IDT is not aliased elsewhere.
    let idt = unsafe { &mut *core::ptr::addr_of_mut!(IDT) };
    idt[vector] = IdtEntry::interrupt_gate(handler, KERNEL_CS);
}

/// No-op stub for non-x86_64 host builds.
#[cfg(not(target_arch = "x86_64"))]
pub fn idt_set_vector(_vector: usize, _handler: u64) {}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_entry_is_all_zero() {
        let e = IdtEntry::missing();
        assert_eq!(e.offset_low, 0);
        assert_eq!(e.selector, 0);
        assert_eq!(e.ist_and_zero, 0);
        assert_eq!(e.type_and_attr, 0);
        assert_eq!(e.offset_mid, 0);
        assert_eq!(e.offset_high, 0);
        #[allow(
            clippy::used_underscore_binding,
            reason = "_reserved is the literal field name in the x86_64 SDM IDT layout"
        )]
        let r = e._reserved;
        assert_eq!(r, 0);
    }

    #[test]
    fn interrupt_gate_size_is_16_bytes() {
        assert_eq!(core::mem::size_of::<IdtEntry>(), 16);
    }

    #[test]
    fn interrupt_gate_sets_selector_and_type() {
        let entry = IdtEntry::interrupt_gate(0, 0x08);
        assert_eq!(entry.selector, 0x08);
        assert_eq!(entry.type_and_attr, 0x8E);
        assert_eq!(entry.ist_and_zero, 0x00);
        #[allow(
            clippy::used_underscore_binding,
            reason = "_reserved is the literal field name in the x86_64 SDM IDT layout"
        )]
        let r = entry._reserved;
        assert_eq!(r, 0);
    }

    #[test]
    fn interrupt_gate_encodes_low_handler_bits() {
        let handler: u64 = 0xABCD_EF12_3456_7890;
        let entry = IdtEntry::interrupt_gate(handler, 0x08);
        // bits [15:0]  = 0x7890, bits [31:16] = 0x3456, bits [63:32] = 0xABCD_EF12
        assert_eq!(entry.offset_low, 0x7890u16);
        assert_eq!(entry.offset_mid, 0x3456u16);
        assert_eq!(entry.offset_high, 0xABCD_EF12u32);
    }

    #[test]
    fn interrupt_gate_reconstructs_full_address() {
        let handler: u64 = 0x0000_CAFE_BABE_BEEF;
        let entry = IdtEntry::interrupt_gate(handler, 0x08);
        let recovered = u64::from(entry.offset_low)
            | (u64::from(entry.offset_mid) << 16)
            | (u64::from(entry.offset_high) << 32);
        assert_eq!(recovered, handler);
    }

    #[test]
    fn idt_array_size() {
        assert_eq!(core::mem::size_of::<[IdtEntry; 256]>(), 256 * 16);
    }

    #[test]
    fn exception_frame_size() {
        // 5 × u64 = 40 bytes.
        assert_eq!(core::mem::size_of::<ExceptionFrame>(), 40);
    }
}
