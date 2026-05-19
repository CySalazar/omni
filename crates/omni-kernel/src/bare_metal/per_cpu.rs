//! Per-CPU descriptor scaffold — MB14.a foundation for MP/AP enable,
//! extended in MB14.b with `IA32_GS_BASE` / `IA32_KERNEL_GS_BASE` wiring
//! and a GS-relative [`current_cpu`] accessor.
//!
//! ## Scope (MB14.a + MB14.b)
//!
//! - **Single CPU.** Exactly one [`PerCpu`] static (`BSP`) is initialised
//!   from [`init_bsp`] after [`super::lapic::lapic_init`] has succeeded.
//! - **`GS_BASE` per-CPU pointer (MB14.b).** [`init_gs_base`] writes the
//!   address of the supplied descriptor into the two GS-base MSRs:
//!   `IA32_GS_BASE` (`0xC000_0101`, active in kernel mode) and
//!   `IA32_KERNEL_GS_BASE` (`0xC000_0102`, shadow swapped in on
//!   `swapgs`). The descriptor stores a self-pointer at offset 0 so a
//!   single `mov rax, gs:[0]` returns the active `&'static PerCpu`.
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
//! Both require a stable per-CPU descriptor reachable by a constant-time
//! instruction sequence. MB14.b makes that sequence a single `gs:[0]`
//! load, agnostic to which CPU is executing the code path.
//!
//! ## Why atomics
//!
//! The four fields are `core::sync::atomic` even though MB14.b touches
//! only the BSP. The descriptor is intentionally `Sync` so that future
//! AP code can populate sibling slots — in an `[PerCpu; MAX_CPUS]`
//! array — without taking a global lock on the cold paths.

#![allow(
    unsafe_code,
    reason = "static descriptor access + MSR writes + gs:[0] inline asm; MB14.b is single-CPU, BSP-only"
)]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Sentinel: the descriptor has not yet been seeded by [`init_bsp`].
/// Chosen as `u32::MAX` so it cannot collide with a valid xAPIC ID
/// (8-bit field, max 255).
pub const CPU_ID_UNINIT: u32 = u32::MAX;

/// `IA32_GS_BASE` — active GS base while running in kernel mode.
/// Holds the per-CPU pointer between `swapgs` flips on Ring 3 → Ring 0
/// transitions (and the inverse before sysretq / iretq returns).
#[cfg(target_arch = "x86_64")]
const MSR_GS_BASE: u32 = 0xC000_0101;

/// `IA32_KERNEL_GS_BASE` — shadow GS base. `swapgs` exchanges this with
/// the active `GS_BASE`, so userspace can keep its own value while the
/// kernel keeps the per-CPU pointer parked here.
#[cfg(target_arch = "x86_64")]
const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Per-CPU descriptor. One instance per logical CPU; MB14.a allocated
/// only the BSP slot.
///
/// `#[repr(C)]` is mandatory: the `self_ptr` field MUST live at offset 0
/// so a `gs:[0]` load returns `&PerCpu` after [`init_gs_base`].
///
/// Field semantics:
///
/// - `self_ptr`: address of `self` reachable via `gs:[0]` once
///   [`init_gs_base`] has wired the MSRs. `0` before `init_gs_base`.
/// - `cpu_id`: dense, 0-based kernel-local identifier. BSP is always 0.
/// - `lapic_id`: physical LAPIC ID as read from LAPIC register 0x20.
///   May be sparse (e.g., 0, 2, 4, … on some NUMA topologies).
/// - `is_bsp`: true on the Bootstrap Processor, false on Application
///   Processors. Used by the IPI broadcast logic to skip self.
#[derive(Debug)]
#[repr(C)]
pub struct PerCpu {
    self_ptr: AtomicU64,
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
            self_ptr: AtomicU64::new(0),
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

    /// Address that `gs:[0]` resolves to after [`init_gs_base`].
    ///
    /// Returns `0` until `init_gs_base` runs. Primarily used by tests
    /// and the boot serial dump to confirm the MSR was wired.
    #[must_use]
    pub fn self_ptr(&self) -> u64 {
        self.self_ptr.load(Ordering::Acquire)
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
/// On bare-metal `x86_64`, this dereferences the self-pointer parked at
/// `gs:[0]` by [`init_gs_base`] — a single instruction, constant time,
/// agnostic to which CPU is executing. On host / non-x86_64 builds the
/// function returns the BSP singleton (the only descriptor that exists
/// in those test environments).
#[must_use]
pub fn current_cpu() -> &'static PerCpu {
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    {
        let ptr: *const PerCpu;
        // SAFETY: `init_gs_base` ran during boot, parking `&BSP` at
        // offset 0 of the GS segment. The pointer is `'static` because
        // it aliases `static BSP`. Future AP code will park its own
        // per-CPU descriptor pointer the same way before enabling
        // interrupts, so the load remains valid on every CPU.
        unsafe {
            core::arch::asm!(
                "mov {ptr}, gs:[0]",
                ptr = out(reg) ptr,
                options(nomem, nostack, preserves_flags),
            );
            &*ptr
        }
    }
    #[cfg(not(all(target_arch = "x86_64", target_os = "none")))]
    {
        &BSP
    }
}

/// Convenience: the BSP descriptor specifically. Useful for AP code that
/// wants to address the BSP without going through [`current_cpu`].
#[must_use]
pub fn bsp() -> &'static PerCpu {
    &BSP
}

/// MB14.b — wire the GS-base MSRs to the supplied descriptor.
///
/// Writes the descriptor's address into `IA32_GS_BASE` (active in
/// kernel mode) and `IA32_KERNEL_GS_BASE` (shadow for `swapgs`), and
/// stamps the self-pointer at offset 0 of the descriptor so that
/// `gs:[0]` resolves to `&pc` from any kernel context.
///
/// Both MSRs receive the same value so that:
///
/// - During kernel boot (before any Ring 3 entry) the active GS base
///   is the per-CPU pointer, allowing `current_cpu()` to work the
///   moment this call returns.
/// - After the first Ring 3 → Ring 0 transition (syscall entry has
///   `swapgs` as its first instruction), the swap brings the per-CPU
///   pointer from the shadow MSR back into the active slot — leaving
///   userspace's GS base preserved in the shadow until sysretq swaps
///   it back. Both starting values being identical means a misordered
///   `swapgs` (e.g., during early panic before user mode is reached)
///   still leaves the kernel with a valid per-CPU pointer.
///
/// # Safety
///
/// Caller must hold a `&'static` reference to `pc`. MSR writes are
/// privileged (Ring 0) but otherwise side-effect-free: they affect
/// only the addressing base used by GS-segment overrides on this
/// logical CPU.
#[cfg(target_arch = "x86_64")]
pub fn init_gs_base(pc: &'static PerCpu) {
    let pc_ptr = core::ptr::from_ref::<PerCpu>(pc) as u64;
    // Stamp the self-pointer first so it is observable the instant the
    // MSRs are armed. `Release` pairs with the `Acquire` load in
    // `current_cpu`'s consumers (e.g., a future scheduler tick that
    // reads `cpu_id` after observing `self_ptr`).
    pc.self_ptr.store(pc_ptr, Ordering::Release);

    // SAFETY: both MSRs are documented in Intel SDM Vol 3A §10.4 /
    // §3.4.4 and accept any 64-bit value as the GS base. The pointer
    // is a valid kernel address (`pc` is `&'static`). We are running
    // in Ring 0 at boot time, single-CPU, with no preemption.
    unsafe {
        wrmsr(MSR_GS_BASE, pc_ptr);
        wrmsr(MSR_KERNEL_GS_BASE, pc_ptr);
    }
}

/// No-op stub for non-x86_64 host builds.
#[cfg(not(target_arch = "x86_64"))]
pub fn init_gs_base(pc: &'static PerCpu) {
    let pc_ptr = core::ptr::from_ref::<PerCpu>(pc) as u64;
    pc.self_ptr.store(pc_ptr, Ordering::Release);
}

/// Privileged write to model-specific register `msr`.
///
/// # Safety
///
/// Ring 0 only. The caller must ensure `msr` accepts the value and is
/// architecturally defined (no reserved bits set).
#[cfg(target_arch = "x86_64")]
unsafe fn wrmsr(msr: u32, value: u64) {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "MSR encoding splits the 64-bit value into lo:eax / hi:edx by spec"
    )]
    let lo = value as u32;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "right-shift then `as u32` is the canonical MSR hi-half encoding"
    )]
    let hi = (value >> 32) as u32;
    // SAFETY: `wrmsr` is a ring-0 instruction; caller invariants above.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
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
        assert_eq!(pc.self_ptr(), 0);
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
        // Host build: `current_cpu()` collapses to `&BSP`; bare-metal
        // build dereferences `gs:[0]`. Both must agree on identity.
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

    /// MB14.b — the self-pointer is the first field of [`PerCpu`] so a
    /// `gs:[0]` load on bare-metal returns `&PerCpu`. This test pins the
    /// layout: the address of `self_ptr` must equal the address of the
    /// containing struct.
    #[test]
    fn self_ptr_field_at_offset_zero() {
        let pc = PerCpu::new_uninit();
        let struct_addr = core::ptr::addr_of!(pc) as usize;
        let field_addr = core::ptr::addr_of!(pc.self_ptr) as usize;
        assert_eq!(struct_addr, field_addr);
    }

    /// MB14.b — host stub of [`init_gs_base`] stamps the self-pointer.
    /// Bare-metal builds additionally wire two MSRs; the field-stamp
    /// half is verifiable here.
    #[test]
    fn init_gs_base_stamps_self_pointer() {
        init_gs_base(bsp());
        let stamped = bsp().self_ptr();
        let expected = core::ptr::from_ref::<PerCpu>(bsp()) as u64;
        assert_eq!(stamped, expected);
    }
}
