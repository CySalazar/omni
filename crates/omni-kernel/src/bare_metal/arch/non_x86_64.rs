//! Non-`x86_64` stubs for the bare-metal arch intrinsics.
//!
//! These exist solely to keep `cargo build --workspace --all-features`
//! and the host-mode unit tests at `tests/heap.rs` /
//! `tests/panic_record.rs` compilable on non-x86 developer machines
//! (most relevantly `aarch64-apple-darwin`).
//!
//! The stubs are NEVER linked into the bare-metal binary: the binary
//! is built for `x86_64-unknown-none` per `OIP-Kernel-003` § 4, at
//! which point `super::x86_64` is the active arch module instead.

/// Interrupt control stubs (no-ops on non-x86 hosts).
pub mod interrupts {
    /// No-op stand-in for the `x86_64` `cli` instruction.
    ///
    /// The bare-metal binary never reaches this function (it is
    /// `target_arch = "x86_64"`); this stub exists only so that host
    /// builds of the `bare-metal` feature compile on ARM developer
    /// machines.
    #[inline(always)]
    pub fn disable() {}
}

/// Loop-forever stub. Compiles on any architecture; executed in tests
/// only when a test deliberately drives the panic path on the host.
#[inline]
pub fn halt_forever() -> ! {
    loop {
        // `core::hint::spin_loop()` is the architecture-agnostic
        // back-off hint; on hosts it is a no-op or a `yield`.
        core::hint::spin_loop();
    }
}

/// Port-I/O stubs (panic on host because no test exercises them).
///
/// # Safety
///
/// The host stub panics on invocation. The bare-metal binary uses the
/// `target_arch = "x86_64"` variant instead, so this is never reached
/// in production.
#[inline(always)]
pub unsafe fn outb(_port: u16, _value: u8) {
    // Host stub: the early console is exercised only in bare-metal
    // builds. If a host test ever drives `early_console` directly,
    // it should mock this surface instead of executing it.
}

/// Counterpart to [`outb`] for completeness.
///
/// # Safety
///
/// See [`outb`].
#[inline]
pub unsafe fn inb(_port: u16) -> u8 {
    0
}

/// No-op wait stub for non-x86 host builds.
#[inline(always)]
pub fn wait_secs(_secs: u32) {}

/// No-op ACPI power-off stub for non-x86 host builds.
#[inline(always)]
pub fn acpi_poweroff() {}
