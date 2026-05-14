//! `x86_64` implementation of the bare-metal arch intrinsics.
//!
//! All routines here are `unsafe` at the asm level but expose safe
//! wrappers. They are the **only** site in the kernel that may emit
//! raw inline assembly — every other module reaches port I/O and
//! control-flow termination through this module.

use core::arch::asm;

/// Interrupt control primitives.
pub mod interrupts {
    use super::asm;

    /// Disable hardware interrupts (`cli`).
    ///
    /// Used as the FIRST step of the panic handler so that a nested
    /// interrupt cannot reenter the panic path while it is writing
    /// the static `PanicRecord` buffer.
    #[inline]
    pub fn disable() {
        // SAFETY: `cli` is a privileged but otherwise side-effect-free
        // instruction that masks maskable interrupts in the RFLAGS
        // register. The kernel runs in ring 0 (the bootloader hands us
        // CPL=0 per UEFI hand-off), so this is always permitted.
        unsafe {
            asm!("cli", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Halt the CPU forever (`hlt` in a loop).
///
/// The function returns `!`. Callers MUST never expect control flow
/// past this point: the CPU will execute `hlt` until the next
/// interrupt (which, given `interrupts::disable()` above, never
/// arrives for maskable IRQs; an NMI would re-enter `hlt` immediately
/// on resume).
///
/// This is the panic-path terminator and also the K4 `kmain` post-
/// banner terminator.
#[inline]
pub fn halt_forever() -> ! {
    loop {
        // SAFETY: `hlt` halts the CPU until the next external
        // interrupt. It has no memory effects and no register
        // clobbers.
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Write a single byte to an x86 I/O port (`out dx, al`).
///
/// Used by the early console to talk to the 16550 UART at COM1
/// (`0x3f8`). Public because [`super::super::early_console`] needs
/// to invoke it; not stable API for the rest of the kernel.
///
/// # Safety
///
/// The caller MUST ensure that `port` is a port the kernel is
/// permitted to touch. At v0.2 only ports `0x3f8..=0x3ff` (COM1
/// register block) are used.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    // SAFETY: forwarded to the caller's safety contract.
    unsafe {
        asm!("out dx, al",
             in("dx") port,
             in("al") value,
             options(nomem, nostack, preserves_flags));
    }
}

/// Read a single byte from an x86 I/O port (`in al, dx`).
///
/// # Safety
///
/// Same caveat as [`outb`].
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: forwarded to the caller's safety contract.
    unsafe {
        asm!("in al, dx",
             out("al") value,
             in("dx") port,
             options(nomem, nostack, preserves_flags));
    }
    value
}
