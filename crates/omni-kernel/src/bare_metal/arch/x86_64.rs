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

// ---------------------------------------------------------------------------
// RTC-based busy-wait
// ---------------------------------------------------------------------------

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

unsafe fn cmos_read(reg: u8) -> u8 {
    // Bit 7 of the address port controls NMI; keep it clear.
    // SAFETY: forwarded to caller's safety contract.
    unsafe {
        outb(CMOS_ADDR, reg & 0x7F);
        inb(CMOS_DATA)
    }
}

fn rtc_update_in_progress() -> bool {
    // SAFETY: CMOS status register A (0x0A) is a well-defined read target.
    (unsafe { cmos_read(0x0A) } & 0x80) != 0
}

// ---------------------------------------------------------------------------
// ACPI S5 power-off via PCI config-space discovery
// ---------------------------------------------------------------------------
//
// Previous approach (RSDP scan in BIOS ROM) caused a guru-meditation fault:
// VirtualBox places the RSDT above 1 GiB of physical memory, outside the
// identity-mapped window that bootloader v0.9 provides. The PCI config-space
// approach uses only I/O ports 0xCF8/0xCFC and therefore never touches
// memory above 1 GiB, making it page-fault-safe.

/// Write a 16-bit word to an x86 I/O port (`out dx, ax`).
#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax",
             in("dx") port,
             in("ax") value,
             options(nomem, nostack, preserves_flags));
    }
}

/// Write a 32-bit dword to an x86 I/O port (`out dx, eax`).
#[inline]
unsafe fn outl(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax",
             in("dx") port,
             in("eax") value,
             options(nomem, nostack, preserves_flags));
    }
}

/// Read a 32-bit dword from an x86 I/O port (`in eax, dx`).
#[inline]
unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    unsafe {
        asm!("in eax, dx",
             out("eax") v,
             in("dx") port,
             options(nomem, nostack, preserves_flags));
    }
    v
}

/// Read 32 bits from PCI configuration space via the CF8/CFC access mechanism.
#[inline]
unsafe fn pci_cfg_read32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off & 0xFC) as u32);
    unsafe {
        outl(0xCF8, addr);
        inl(0xCFC)
    }
}

/// Trigger ACPI S5 (soft power-off).
///
/// Scans PCI bus 0 for the Intel PIIX4 PM controller (vendor 0x8086,
/// device 0x7113, always at function 3), reads PMBASE from config offset
/// 0x40, and writes the SeaBIOS S5 sleep value (SLP_TYP=5, SLP_EN=1 →
/// 0x3400) to PM1a_CNT (PMBASE + 4).
///
/// Falls back to the VirtualBox/QEMU default hardcoded address (PMBASE =
/// 0x4000, PM1a_CNT = 0x4004) if the PIIX4 is not found on the scanned
/// device slots.
///
/// Uses **only I/O-port accesses** (0xCF8/0xCFC/PM1a_CNT) — no memory
/// reads — so it cannot trigger a page fault in the identity-mapped
/// long-mode environment that bootloader v0.9 provides.
pub fn acpi_poweroff() {
    let pmbase = unsafe { find_piix4_pmbase() }.unwrap_or(0x4000_u32);
    // PIIX4 PMBA bits [31:6] are the base address; bit 0 = I/O space type.
    let pm1a_cnt = (pmbase & !1_u32) as u16 + 4;
    // SeaBIOS \_S5: SLP_TYP_A = 5 → PM1_CNT bits[12:10]=5, SLP_EN=bit[13]
    // → (5 << 10) | (1 << 13) = 0x1400 | 0x2000 = 0x3400
    unsafe { outw(pm1a_cnt, 0x3400) };
    // If still executing the write had no effect; caller falls through to
    // halt_forever.
}

/// Scan PCI bus 0 for the PIIX4 PM controller and return its PMBASE.
///
/// VirtualBox places the PIIX4 at bus=0, device=1 (or device=7 in some
/// configurations), function=3. Scanning all 32 devices is safe because
/// reading a non-existent device returns 0xFFFF_FFFF (no device present).
unsafe fn find_piix4_pmbase() -> Option<u32> {
    for dev in 0_u8..32 {
        // PIIX4 PM controller: Intel (0x8086), device ID 0x7113.
        if unsafe { pci_cfg_read32(0, dev, 3, 0) } == 0x7113_8086 {
            // PMBASE at PCI config offset 0x40.
            return Some(unsafe { pci_cfg_read32(0, dev, 3, 0x40) });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// RTC-based busy-wait
// ---------------------------------------------------------------------------

/// Spin-wait for `secs` seconds using the CMOS Real-Time Clock.
///
/// The RTC seconds register advances once per second regardless of
/// interrupt state, making it safe to call after `interrupts::disable`.
/// Accuracy: ±1 second (single-second resolution of the RTC register).
/// Works on QEMU `pc` and `q35` machine types and on VirtualBox.
pub fn wait_secs(secs: u32) {
    if secs == 0 {
        return;
    }

    // Status register B bit 2: 1 = binary mode, 0 = BCD mode.
    // SAFETY: CMOS register 0x0B is safe to read in ring 0.
    let is_binary = unsafe { cmos_read(0x0B) } & 0x04 != 0;

    let decode = |raw: u8| -> u32 {
        if is_binary {
            raw as u32
        } else {
            ((raw >> 4) as u32) * 10 + (raw & 0x0F) as u32
        }
    };

    // Wait for any in-progress update to finish before sampling start time.
    while rtc_update_in_progress() {
        core::hint::spin_loop();
    }
    // SAFETY: CMOS register 0x00 (seconds) is safe to read in ring 0.
    let start = decode(unsafe { cmos_read(0x00) });
    let mut prev = start;
    let mut elapsed: u32 = 0;

    while elapsed < secs {
        // Spin until the RTC is not in an update cycle.
        while rtc_update_in_progress() {
            core::hint::spin_loop();
        }
        let curr = decode(unsafe { cmos_read(0x00) });
        if curr != prev {
            elapsed += if curr > prev {
                curr - prev
            } else {
                // Wrapped past 59 → 0.
                (60 - prev) + curr
            };
            prev = curr;
        }
        core::hint::spin_loop();
    }
}
