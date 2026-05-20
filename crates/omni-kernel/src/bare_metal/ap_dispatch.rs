//! MB14.h.1 — AP-side observer dispatcher.
//!
//! ## Scope
//!
//! Companion to [`super::per_cpu_run_queue`] and [`super::per_cpu`].
//! Provides the **observer-mode** AP dispatcher invoked from the LAPIC
//! timer IRQ tail (`kernel_check_need_resched` in [`super::lapic`]) on
//! every Application Processor. The dispatcher:
//!
//! 1. Resolves the current CPU id via the `gs:[0]` per-CPU pointer.
//! 2. Pops a single task id from the local run-queue, falling back to
//!    work-stealing across siblings
//!    ([`super::per_cpu_run_queue::pop_for_cpu_with_stealing`]).
//! 3. If a task id was popped, increments the per-CPU
//!    `dispatch_observations` counter (see
//!    [`super::per_cpu::PerCpu::inc_dispatch_observation`]) and **drops**
//!    the id.
//!
//! Observer mode does **not** perform a context switch — the popped id
//! is discarded. The counter exists so the BSP boot-time smoke can
//! confirm the AP timer ISR reached the dispatcher and that the
//! `per_cpu_run_queue` table is reachable from Ring 0 on the AP. The
//! real cross-CPU context switch arrives in MB14.h.2 (see
//! [ADR-0009](../../../../docs/adr/0009-mb14h-ap-dispatch-loop.md)).
//!
//! ## Why observer mode first
//!
//! A live AP-side context switch needs every one of:
//!
//! - **Per-CPU IST stacks** stable under cross-CPU pre-emption (the
//!   current MB14.c.2.d wiring allocates one IST per AP but the kernel
//!   half is shared by reference across PML4s, so the TSS.rsp0 update
//!   path has to be CPU-local).
//! - **`mov cr3` from the AP timer trampoline** correctly bracketed by
//!   per-CPU `IN_SCHEDULER` flags so a concurrent BSP `yield_current`
//!   does not race the AP's scheduler call.
//! - **`tasks` / `processes` access** serialised against the BSP. Today
//!   `SCHEDULER` is a `static mut` with a recursion guard that assumes a
//!   single execution context.
//!
//! Lighting up all three at once raises the triple-fault probability on
//! Proxmox to the point where a serial-only post-mortem becomes the
//! primary debug surface. The observer-mode step proves the AP can
//! observe a queue entry deterministically without touching any of the
//! shared-mutable scheduler state, which collapses the MB14.h.2 risk to
//! the context-switch primitives alone.
//!
//! ## Invocation
//!
//! Called from `kernel_check_need_resched` in [`super::lapic`] on the AP
//! branch (the BSP keeps the legacy cooperative `yield_current` path).
//! The function is `extern "C"` so the IRQ-tail trampoline can reach it
//! via a direct `call` if a future refactor pulls the resched logic out
//! of `lapic.rs`.

#![cfg_attr(
    not(all(target_arch = "x86_64", target_os = "none")),
    allow(
        dead_code,
        reason = "host stub keeps the symbol resolvable from cargo test --workspace"
    )
)]

#[cfg(all(target_arch = "x86_64", target_os = "none", not(test)))]
use super::per_cpu;
#[cfg(all(target_arch = "x86_64", target_os = "none", not(test)))]
use super::per_cpu_run_queue;

/// MB14.h.1 — observer-mode AP dispatcher.
///
/// Pop one task id from the calling CPU's run-queue (with work-stealing
/// fallback); if successful, record the observation on the per-CPU
/// counter and discard the id. Always returns immediately.
///
/// `extern "C"` because the IRQ-tail trampoline targets this symbol via
/// `call kernel_ap_dispatch_observe` on the AP path; the host stub keeps
/// the symbol resolvable from `cargo test --workspace --all-features`.
#[cfg(all(target_arch = "x86_64", target_os = "none", not(test)))]
#[unsafe(no_mangle)]
pub extern "C" fn kernel_ap_dispatch_observe() {
    let cpu = per_cpu::current_cpu();
    let cpu_id = cpu.cpu_id();
    // Defence-in-depth: the BSP must never call into the observer
    // branch (the resched trampoline keeps the BSP on the cooperative
    // `yield_current` path). If a future refactor accidentally wires
    // the BSP through here, drop the dispatch silently so the legacy
    // path's run-queues stay authoritative.
    if cpu.is_bsp() {
        return;
    }
    if let Some(_picked) = per_cpu_run_queue::pop_for_cpu_with_stealing(cpu_id) {
        // Observer-mode discard. The id is intentionally not re-enqueued
        // — the BSP smoke task is a sentinel that is allowed to be
        // consumed exactly once per boot. MB14.h.2 will replace this
        // discard with a real `yield_current` / `context_switch` call.
        cpu.inc_dispatch_observation();
    }
}

/// Host-stub for non-bare-metal builds — keeps the symbol resolvable
/// from `cargo test --workspace --all-features`.
#[cfg(not(all(target_arch = "x86_64", target_os = "none", not(test))))]
pub extern "C" fn kernel_ap_dispatch_observe() {}

// =====================================================================
// Host-side tests
// =====================================================================
//
// The runtime observer call is gated on bare-metal x86_64 because it
// dereferences `gs:[0]` and would `#GP` from a userland test binary.
// What we *can* test here is the per-CPU counter contract surrounding
// the observer (`PerCpu::inc_dispatch_observation` round-trip) — the
// `per_cpu` tests cover that surface directly. This module's
// host-stub exists only to keep the symbol resolvable from non-bare-
// metal builds; running it does nothing.

#[cfg(test)]
mod tests {
    use super::*;

    /// Host stub returns without panicking.
    #[test]
    fn host_stub_is_callable() {
        kernel_ap_dispatch_observe();
        kernel_ap_dispatch_observe();
    }
}
