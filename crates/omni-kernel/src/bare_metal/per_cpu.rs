//! Per-CPU descriptor scaffold — MB14.a foundation for MP/AP enable.
//!
//! This module is the first atomic step of MB14 (multi-processor support).
//! It introduces a single per-CPU descriptor ([`PerCpu`]) seeded by the
//! Bootstrap Processor (BSP) at boot, plus an accessor that returns the
//! descriptor for the current logical CPU.
//!
//! ## Scope (MB14.a — minimal)
//!
//! - **Single CPU.** Exactly one [`PerCpu`] static (`BSP`) is initialised
//!   from [`init_bsp`] after [`super::lapic::lapic_init`] has succeeded.
//! - **No `GS_BASE`-relative addressing yet.** [`current_cpu`] returns
//!   the `BSP` descriptor unconditionally. Writing the per-CPU pointer
//!   into `IA32_KERNEL_GS_BASE` and switching on `swapgs` is the next
//!   atomic step (MB14.b).
//! - **No AP startup.** Only the BSP's LAPIC ID is recorded. Application
//!   processors land in MB14.c (INIT-SIPI-SIPI handshake with a real-mode
//!   trampoline).
//!
//! ## Why now
//!
//! MB13 closed the `omni-capability` integration cycle (ADR-0006) and the
//! kernel is single-CPU end-to-end. The driver model (P6.7) needs:
//! 1. cross-CPU TLB shootdown (broadcast `invlpg`), which presupposes
//!    a way to identify the local CPU and address sibling cores;
//! 2. per-CPU schedulers / run-queues, which need a per-CPU storage slot
//!    addressable in constant time from any kernel context.
//!
//! Both require a stable per-CPU descriptor. MB14.a installs the scaffold
//! without yet enabling APs, so the bare-metal smoke remains identical to
//! the post-MB13 baseline — only an extra `[mb14.a] BSP …` serial line
//! is emitted after `lapic_init`.
//!
//! ## Why atomics
//!
//! The three fields are `core::sync::atomic` even though MB14.a touches
//! only the BSP. The descriptor is intentionally `Sync` so that future
//! AP code can populate sibling slots — in an `[PerCpu; MAX_CPUS]`
//! array — without taking a global lock on the cold paths.

#![allow(
    unsafe_code,
    reason = "static descriptor access; MB14.a is single-CPU, BSP-only"
)]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Sentinel: the descriptor has not yet been seeded by [`init_bsp`].
/// Chosen as `u32::MAX` so it cannot collide with a valid xAPIC ID
/// (8-bit field, max 255).
pub const CPU_ID_UNINIT: u32 = u32::MAX;

/// Per-CPU descriptor. One instance per logical CPU; MB14.a allocates
/// only the BSP slot.
///
/// Field semantics:
///
/// - `cpu_id`: dense, 0-based kernel-local identifier. BSP is always 0.
/// - `lapic_id`: physical LAPIC ID as read from LAPIC register 0x20.
///   May be sparse (e.g., 0, 2, 4, … on some NUMA topologies).
/// - `is_bsp`: true on the Bootstrap Processor, false on Application
///   Processors. Used by the IPI broadcast logic to skip self.
#[derive(Debug)]
pub struct PerCpu {
    cpu_id: AtomicU32,
    lapic_id: AtomicU32,
    is_bsp: AtomicBool,
}

impl PerCpu {
    /// Construct an uninitialised descriptor (suitable for a `static`).
    /// All identifier fields are seeded with [`CPU_ID_UNINIT`].
    #[must_use]
    pub const fn new_uninit() -> Self {
        Self {
            cpu_id: AtomicU32::new(CPU_ID_UNINIT),
            lapic_id: AtomicU32::new(CPU_ID_UNINIT),
            is_bsp: AtomicBool::new(false),
        }
    }

    /// Dense kernel-local CPU identifier (BSP = 0).
    #[must_use]
    pub fn cpu_id(&self) -> u32 {
        self.cpu_id.load(Ordering::Acquire)
    }

    /// Physical LAPIC ID for this logical CPU.
    #[must_use]
    pub fn lapic_id(&self) -> u32 {
        self.lapic_id.load(Ordering::Acquire)
    }

    /// `true` iff this descriptor belongs to the Bootstrap Processor.
    #[must_use]
    pub fn is_bsp(&self) -> bool {
        self.is_bsp.load(Ordering::Acquire)
    }

    /// `true` once the descriptor has been populated by [`init_bsp`] (or
    /// in the future by the AP startup path).
    #[must_use]
    pub fn is_initialised(&self) -> bool {
        self.cpu_id() != CPU_ID_UNINIT
    }

    /// Seed the descriptor in-place. `Release` stores so a later
    /// `Acquire` read in another execution context observes all three
    /// fields populated together.
    fn seed(&self, cpu_id: u32, lapic_id: u32, is_bsp: bool) {
        self.lapic_id.store(lapic_id, Ordering::Release);
        self.is_bsp.store(is_bsp, Ordering::Release);
        self.cpu_id.store(cpu_id, Ordering::Release);
    }
}

/// Singleton Bootstrap Processor descriptor.
///
/// MB14.a only writes this slot. MB14.c will introduce a sibling array
/// for Application Processors.
static BSP: PerCpu = PerCpu::new_uninit();

/// Seed the BSP descriptor with `lapic_id` (typically read via
/// [`super::lapic::read_lapic_id`]).
///
/// Idempotent: the BSP's `cpu_id` is always 0 and `is_bsp` is always
/// true, so a redundant call is harmless. Subsequent reads see the
/// last-written values.
pub fn init_bsp(lapic_id: u32) {
    BSP.seed(0, lapic_id, true);
}

/// Reference to the descriptor for the CPU currently running this code.
///
/// MB14.a single-CPU stub: returns `BSP` unconditionally. MB14.b will
/// swap the implementation to dereference the per-CPU pointer held in
/// `IA32_KERNEL_GS_BASE` (loaded via `swapgs` on Ring 3 → Ring 0).
#[must_use]
pub fn current_cpu() -> &'static PerCpu {
    &BSP
}

/// Convenience: the BSP descriptor specifically. Useful for AP code that
/// wants to address the BSP without going through [`current_cpu`].
#[must_use]
pub fn bsp() -> &'static PerCpu {
    &BSP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_uninit_marks_descriptor_as_uninitialised() {
        let pc = PerCpu::new_uninit();
        assert_eq!(pc.cpu_id(), CPU_ID_UNINIT);
        assert_eq!(pc.lapic_id(), CPU_ID_UNINIT);
        assert!(!pc.is_bsp());
        assert!(!pc.is_initialised());
    }

    #[test]
    fn seed_populates_all_three_fields() {
        let pc = PerCpu::new_uninit();
        pc.seed(0, 7, true);
        assert_eq!(pc.cpu_id(), 0);
        assert_eq!(pc.lapic_id(), 7);
        assert!(pc.is_bsp());
        assert!(pc.is_initialised());
    }

    #[test]
    fn seed_ap_does_not_set_bsp_flag() {
        let pc = PerCpu::new_uninit();
        pc.seed(3, 12, false);
        assert_eq!(pc.cpu_id(), 3);
        assert!(!pc.is_bsp());
        assert!(pc.is_initialised());
    }

    #[test]
    fn init_bsp_seeds_global_descriptor() {
        // The global BSP is shared across tests; this test asserts only
        // the invariants that must hold post-seed regardless of who ran
        // first (cpu_id == 0, is_bsp == true). The lapic_id value is
        // implementation-detail of whatever test seeded it last; we
        // assert that the descriptor is marked initialised.
        init_bsp(0);
        let cpu = current_cpu();
        assert_eq!(cpu.cpu_id(), 0);
        assert!(cpu.is_bsp());
        assert!(cpu.is_initialised());
    }

    #[test]
    fn current_cpu_returns_bsp_singleton() {
        let a = current_cpu() as *const PerCpu;
        let b = bsp() as *const PerCpu;
        assert_eq!(a, b);
    }

    #[test]
    fn cpu_id_uninit_sentinel_does_not_collide_with_xapic_ids() {
        // xAPIC IDs are 8-bit (max 255); CPU_ID_UNINIT must not be a
        // value any real LAPIC could legitimately report. Static
        // assertion via const_assert — clippy::assertions_on_constants
        // (`assert!(true)`) fires on runtime asserts of pure constants.
        const _: () = assert!(CPU_ID_UNINIT > 0xFF);
    }
}
