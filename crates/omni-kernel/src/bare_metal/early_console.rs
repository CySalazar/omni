//! Pre-allocator console writer to the 16550 UART on COM1 (`0x3f8`).
//!
//! The panic handler emits its [`super::panic::PanicRecord`] byte-by-
//! byte through this module **before** any allocation could be made
//! (the panic path is non-allocating by `OIP-Kernel-012` § S1
//! constraint 1) and **after** [`super::arch::interrupts::disable`]
//! has run. The writer is therefore deliberately minimal: it polls
//! the UART line-status register (LSR) for the THR-empty bit and
//! writes one byte. No buffering, no formatting, no allocation.
//!
//! At K4 the console is also used by `kmain` to print the boot banner
//! and the memory-region count (`OIP-Kernel-005` § S3). That code path
//! is not allocation-sensitive but goes through this module anyway so
//! that there is a single audit point for early-boot console writes.

use super::arch;

/// COM1 base I/O port.
///
/// The 16550 UART register block is COM1 base + offset; this module
/// hardcodes COM1 since it is universally available under QEMU's
/// default `q35` machine model and on every UEFI-capable physical
/// platform that surfaces a legacy serial port.
const COM1: u16 = 0x3f8;

/// Line-status register offset: bit 5 set ⇔ THR (transmit-holding
/// register) is empty and accepts a new byte.
const LSR_OFFSET: u16 = 5;

/// LSR bit indicating the THR is empty.
const LSR_THR_EMPTY: u8 = 1 << 5;

/// Initialise the 16550 UART on COM1 to a known-good state.
///
/// Must be called once, before the first [`write_str`] or [`emit`],
/// from `kernel_entry` (after `interrupts::disable`). The sequence
/// follows the OSDev 16550 canonical init:
///
/// 1. Disable UART interrupts — IRQ4 must not fire before the IDT is
///    installed (and we already ran `interrupts::disable`, but the UART
///    interrupt enable register is separate from RFLAGS.IF).
/// 2. Set baud-rate divisor = 1 (→ 115 200 baud).
/// 3. Set 8N1 line format, clearing DLAB so subsequent writes go to THR.
/// 4. Enable and flush the 14-byte FIFOs.
/// 5. Assert DTR + RTS + OUT2 (required by some multiplexers; no-op on QEMU).
///
/// On non-x86 hosts the underlying [`arch::outb`] is a no-op, so this
/// function compiles and runs harmlessly in host tests.
pub fn init() {
    unsafe {
        arch::outb(COM1 + 1, 0x00); // IER: disable all UART interrupts
        arch::outb(COM1 + 3, 0x80); // LCR: set DLAB to access divisor latch
        arch::outb(COM1 + 0, 0x01); // DLL: divisor low byte  (1 → 115200 baud)
        arch::outb(COM1 + 1, 0x00); // DLM: divisor high byte
        arch::outb(COM1 + 3, 0x03); // LCR: 8-bit, no parity, 1 stop; clears DLAB
        arch::outb(COM1 + 2, 0xC7); // FCR: enable FIFO, clear TX+RX, 14-byte trigger
        arch::outb(COM1 + 4, 0x0B); // MCR: DTR + RTS + OUT2
    }
}

/// Emit a byte slice to COM1 in polled mode.
///
/// This function blocks until every byte is delivered to the UART
/// data register. At `115_200` baud a 1 KiB record drains in ≈ 90 ms;
/// the K3 `PANIC_RECORD_MAX_BYTES` cap of 1024 sizes the worst case
/// against this constraint.
///
/// # Behaviour on non-x86 hosts
///
/// On non-x86 builds (host tests on ARM developer machines) the
/// underlying `outb` is a no-op, so this function returns immediately
/// without doing anything. The host-mode integration tests therefore
/// MUST NOT assert console side-effects; they only exercise the
/// pre-encoding pipeline.
pub fn emit(bytes: &[u8]) {
    for &b in bytes {
        emit_byte(b);
    }
}

/// Emit a fixed-string literal — used as the overflow-marker fallback
/// when [`super::panic::PANIC_RECORD_MAX_BYTES`] is exceeded.
///
/// Identical to [`emit`] but documented separately so that the
/// panic-path call site (in `panic.rs`) makes its intent explicit:
/// this is not a structured payload, just a raw forensics breadcrumb.
pub fn emit_raw(bytes: &[u8]) {
    emit(bytes);
}

/// Emit a single byte: wait for THR-empty, then `outb` to the data
/// register at COM1 base.
fn emit_byte(b: u8) {
    // Spin until LSR bit 5 (THR empty) is set.
    //
    // SAFETY: COM1 LSR (`0x3fd`) is a well-defined, read-only-side-
    // effect-free register on the 16550 UART. The kernel runs in
    // ring 0 with full port-I/O permission.
    while unsafe { arch::inb(COM1 + LSR_OFFSET) } & LSR_THR_EMPTY == 0 {
        core::hint::spin_loop();
    }
    // SAFETY: writing to COM1's data register (`0x3f8`) once the THR
    // is empty is the documented protocol for the 16550 UART. The
    // value `b` is an arbitrary 8-bit payload and is always valid.
    unsafe { arch::outb(COM1, b) };
}

/// Convenience for code paths that want to push a `&str` rather than
/// raw bytes (e.g., the K4 `kmain` banner — `OIP-Kernel-005` § S3).
pub fn write_str(s: &str) {
    emit(s.as_bytes());
}

/// Decimal printer for `usize`.
///
/// Used by the K4 `kmain` banner to report
/// `boot_info.memory_regions.len()` without pulling in any
/// `core::fmt` machinery (a writer trait + buffer that would not fit
/// the bump heap's worst-case path). Buffer is 20 bytes — enough for
/// `u64::MAX` (20 decimal digits). Writes left-to-right.
pub fn write_usize(mut n: usize) {
    if n == 0 {
        emit_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        // `n % 10` is in 0..=9, so the truncating cast to `u8` is
        // exact. We index into `buf` via `i`, which we just
        // decremented within the bounds of `buf.len()`.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::indexing_slicing,
            reason = "n % 10 is 0..=9 (fits u8); i is bounded by buf.len()"
        )]
        {
            buf[i] = b'0' + (n % 10) as u8;
        }
        n /= 10;
    }
    #[allow(clippy::indexing_slicing, reason = "i is bounded by buf.len() above")]
    emit(&buf[i..]);
}
