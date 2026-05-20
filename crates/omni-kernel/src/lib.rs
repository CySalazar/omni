//! # `omni-kernel`
//!
//! The OMNI OS microkernel.
//!
//! Responsibilities (and only these):
//!
//! - Memory management (virtual memory, page tables, allocators)
//! - Process and thread scheduling
//! - Inter-process communication (typed message passing)
//! - Capability-based security primitives
//! - Hardware abstraction interfaces (HAL contracts)
//!
//! Everything else — filesystems, drivers, networking stacks, AI runtime —
//! runs as user-space services communicating via IPC. This minimizes the
//! Trusted Computing Base.
//!
//! ## Status
//!
//! Draft v0.2 — module surface and trait skeletons are landed for memory,
//! scheduling, IPC, capabilities, and syscall dispatch. The crate still
//! compiles in `std` mode by default; the `no_std + no_main` bare-metal
//! transition is gated behind the `bare-metal` feature, which switches
//! `lib.rs` (and every module) to `#![no_std]` and disables anything that
//! pulls in libstd. The transition to a real bare-metal binary lands in
//! P6.1–P6.2 per [`/oips/oip-kernel-003.md`](../../../oips/oip-kernel-003.md).
//!
//! ## Design rationale
//!
//! 1. **Microkernel**: smaller TCB → smaller attack surface. Faults in a
//!    service crash that service, not the kernel.
//! 2. **Rust + memory safety**: eliminates entire classes of vulnerabilities
//!    that plague C kernels (use-after-free, buffer overflows, data races).
//! 3. **Capability-based security**: the only way to act on a resource is
//!    to present a valid capability. No ambient authority, no superuser.
//! 4. **Message passing IPC**: typed, async-friendly, encryption-aware.
//! 5. **Verifiability over time**: a small kernel is amenable to formal
//!    methods (in line with seL4 prior art). Long-term goal: formal proofs
//!    for the IPC and capability subsystems.
//!
//! ## Modules
//!
//! - [`memory`] — virtual memory, page tables, allocators.
//! - [`scheduling`] — process and thread scheduling.
//! - [`ipc`] — inter-process communication primitives.
//! - [`capabilities`] — kernel-side capability validation and minting.
//! - [`syscall`] — system call dispatch.

#![doc(html_root_url = "https://docs.omni-os.org/omni-kernel")]
// `no_std` / `no_main` are only meaningful in non-test builds. Tests
// always require `std` (for the test harness) and a `main` (for the
// runner), so we suppress both attributes under `cfg(test)`. Under
// `cargo build --features bare-metal`, the kernel still compiles as
// `no_std + no_main` exactly as P6.1 requires.
#![cfg_attr(all(feature = "bare-metal", not(test)), no_std)]
#![cfg_attr(all(feature = "bare-metal", not(test)), no_main)]
#![warn(missing_docs)]
// `#[cfg(test)]` modules in this crate (arena fixtures in `paging.rs`,
// `elf_loader.rs`, and `memory.rs`) construct synthetic page tables and
// ELF blobs through `std::alloc::Layout`; the assertions themselves rely
// on `unwrap()` / `expect()` to fail the test deterministically when an
// invariant breaks. `clippy::unwrap_used`, `clippy::expect_used`,
// `clippy::panic`, and `clippy::doc_markdown` are silenced for test
// targets only — production code keeps them at workspace-level "warn".
// This `cfg_attr(test, allow(...))` is explicitly whitelisted by ADR-0003.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::doc_markdown
    )
)]
// NOTE: per ADR-0003 (no blanket #![allow] in production crates), every
// `pedantic` / `nursery` / `cargo` / `unsafe_code` blanket previously
// suppressed at crate root has been lifted across Step 7.1–7.4. Each
// remaining intentional violation carries a localised
// `#[allow(<lint>, reason = "...")]` attribute at the offending item or
// — for widespread unsafe-density `bare_metal/` modules — at module level.

// `alloc` is available even in `no_std` mode (the bare-metal kernel
// provides its own allocator). In `std` builds, `alloc` is re-exported
// transparently.
extern crate alloc;

pub mod capabilities;
pub mod driver_manifest;
pub mod ipc;
pub mod kaslr;
pub mod known_issuers;
pub mod memory;
pub mod mm;
#[cfg(feature = "bare-metal")]
pub mod process;
pub mod scheduling;
pub mod syscall;

// Bare-metal runtime: panic handler, global allocator, early console,
// arch intrinsics. Lives only when the `bare-metal` feature is on; the
// inner `#[panic_handler]` and `#[global_allocator]` items are further
// gated `not(test)` to keep `cargo test --all-features` compilable.
//
// Specified by OIP-Kernel-012 (was OIP-Kernel-004 — renumbered at
// Draft → Review on 2026-05-14 per OIP-Process-001 §8.3 to free the
// "004" integer for the canonical OIP-Serde-004).
#[cfg(feature = "bare-metal")]
pub mod bare_metal;

// -----------------------------------------------------------------------------
// Kernel-wide error type
// -----------------------------------------------------------------------------

/// Kernel-side error discriminant.
///
/// Kept deliberately small and PII-safe. Userspace receives errors in
/// `omni_types::OmniError` form via the syscall ABI; this enum is the
/// kernel's internal representation, mapped at the syscall boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelError {
    /// The operation is not yet implemented in this kernel build.
    /// Returned by every scaffold method until its corresponding P6 task
    /// lands.
    NotYetImplemented,
    /// A capability check failed. The caller did not present a valid
    /// capability for the requested operation.
    CapabilityDenied,
    /// A resource is exhausted (out of memory, no free thread slots, IPC
    /// queue full, etc.).
    ResourceExhausted,
    /// Invalid argument from userspace. The syscall layer is supposed to
    /// catch most of these; this variant is for the edge cases the
    /// syscall layer cannot validate without context.
    InvalidArgument,
    /// Internal invariant violation. Indicates a kernel bug.
    Internal,
}

// -----------------------------------------------------------------------------
// Kernel-wide result alias
// -----------------------------------------------------------------------------

/// Standard `Result` type for kernel operations.
pub type KernelResult<T> = Result<T, KernelError>;

// -----------------------------------------------------------------------------
// kmain — kernel main entry, invoked from kernel-runner::kernel_entry
// after BumpHeap::init.
//
// OIP-Kernel-005 § S3. K4 scope is intentionally minimal: print a
// banner (visible signature of successful boot), record the boot_info
// pointer + memory map size, halt forever. Subsystem init order
// (arch::init, memory::init, scheduling::init, ipc::init,
// capabilities::init) lands in K6+.
// -----------------------------------------------------------------------------

// Physical frame allocator — 4 GiB capacity (16 384 words × 64 bits × 4 KiB).
// All frames start used; kmain calls mark_range_free for each Usable region.
// Safety invariant: single-CPU / no-preemption throughout bare-metal P6 scope.
// Must be wrapped in a spinlock when SMP lands (P6.4+).
/// Capacity of the global frame allocator, in u64 bitmap words.
///
/// 16 384 words × 64 bits/word × 4 KiB/frame = 4 GiB of trackable RAM.
/// Bumping this raises the static-memory footprint linearly.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
const FRAME_BITMAP_WORDS: usize = 16384;

#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
static mut FRAME_ALLOC: memory::BitmapFrameAllocator<{ FRAME_BITMAP_WORDS }> =
    memory::BitmapFrameAllocator::new(memory::PhysAddr(0));

// Cooperative round-robin scheduler — MB6.
// Single-CPU, non-preemptive. Same safety invariant as FRAME_ALLOC.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
static mut SCHEDULER: scheduling::RoundRobinScheduler = scheduling::RoundRobinScheduler::new();

// MB14.g — the per-CPU tick counter previously lived here as a single
// `pub static mut TICK_COUNT: u64 = 0;` global, written only by the LAPIC
// timer ISR on the BSP. Once APs began servicing their own timers
// (MB14.f) keeping the global meant either racing the AP writers or
// gating them out via `current_cpu().is_bsp()`. MB14.g moves the counter
// into `PerCpu::tick_count` (one atomic per logical CPU) — see
// `bare_metal::per_cpu::PerCpu::inc_tick`. No external readers of the
// old symbol existed at the time of removal (grep `crate::TICK_COUNT`).

// Idle task — lowest-priority loop; runs when no other task is runnable.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
fn idle_task() -> ! {
    loop {
        // SAFETY: bare-metal ring-0; hlt suspends the CPU until the next
        // interrupt (none enabled in MB6, so this effectively halts forever
        // unless a future milestone enables the LAPIC timer).
        #[allow(unsafe_code, reason = "bare-metal ring-0 hlt; SAFETY comment above")]
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Registers each `Usable` region of the bootloader memory map with the
/// frame allocator, but only after verifying that the region is reachable
/// through the active direct-map at `phys_offset`. A region is included
/// only if both its first and last 4 KiB page translate cleanly via
/// `pager.translate`; otherwise it is skipped entirely.
///
/// Returns `(validated_bytes, skipped_bytes)` — both sums of the raw
/// region sizes, regardless of any subsequent `mark_range_used` reserve.
///
/// This is the MB9 invariant enforcer: every frame the allocator hands
/// out can be written via `phys + phys_offset` without faulting.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
fn register_direct_mapped_regions(
    alloc: &mut memory::BitmapFrameAllocator<{ FRAME_BITMAP_WORDS }>,
    pager: &bare_metal::paging::PageMapper,
    phys_offset: u64,
    boot_info: &bootloader_api::BootInfo,
) -> (u64, u64) {
    use bootloader_api::info::MemoryRegionKind;

    let mut validated: u64 = 0;
    let mut skipped: u64 = 0;

    for region in boot_info.memory_regions.iter() {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let size = region.end.saturating_sub(region.start);
        if size == 0 {
            continue;
        }

        // Last page boundary inside the region: align (end - 1) down to 4 KiB.
        let last_page_start = (region.end - 1) & !0xFFF;

        let start_v = memory::VirtAddr(phys_offset.wrapping_add(region.start));
        let last_v = memory::VirtAddr(phys_offset.wrapping_add(last_page_start));

        if pager.translate(start_v).is_some() && pager.translate(last_v).is_some() {
            alloc.mark_range_free(memory::PhysAddr(region.start), size);
            validated += size;
        } else {
            skipped += size;
        }
    }

    (validated, skipped)
}

/// Kernel main — invoked from the runner's `kernel_entry` after the
/// global heap has been initialised.
///
/// At K4/K5 the function:
///
/// 1. Installs the kernel GDT (replaces the bootloader's temporary GDT).
/// 2. Initialises the `BitmapFrameAllocator` from the bootloader memory map.
/// 3. Prints the canonical banner over the early console — five lines required
///    by the K5 QEMU smoke test.
/// 4. Renders a full graphical boot banner on the GOP framebuffer (UEFI path);
///    falls back to VGA text mode when no framebuffer is available.
/// 5. Runs the 5-minute desktop demo, then issues ACPI S5 power-off.
///
/// # Signature stability
///
/// Per `OIP-Kernel-005` § S3 the first parameter (`boot_info`) is stable
/// for v1.0. The second parameter (`framebuffer`) is an additive extension
/// permitted by OIP-Kernel-005 § S3 note.
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
// `mb8-smoke` short-circuits kmain via `mb8_smoke::run() -> !`, which makes
// the desktop demo + power-off tail unreachable (and `framebuffer` unused).
// Both are intended under that feature.
#[cfg_attr(feature = "mb8-smoke", allow(unreachable_code, unused_variables))]
#[allow(
    clippy::too_many_lines,
    reason = "kmain is the boot orchestrator; subsystem init must stay in single flow"
)]
#[allow(
    clippy::cognitive_complexity,
    reason = "kmain inlines every subsystem init for single-flow boot ordering; an extraction would obscure the deterministic init sequence the orchestrator must enforce"
)]
pub fn kmain(
    boot_info: &'static bootloader_api::BootInfo,
    framebuffer: Option<bare_metal::graphics::FrameBuffer>,
) -> ! {
    use bare_metal::{arch, demo, early_console, gdt, idt, paging, tss};

    // -------------------------------------------------------------------------
    // GDT: install kernel-controlled segment descriptors (replaces bootloader's
    // temporary GDT). Must be the first action after entering kmain.
    // -------------------------------------------------------------------------
    gdt::gdt_init();

    // -------------------------------------------------------------------------
    // TSS init (MB13.h): populate `TSS.ist1` / `TSS.ist2` with the static
    // IST stack tops, then issue `ltr 0x28` so the CPU's task register
    // points at the static TSS. Without `ltr`, a Ring 3 → Ring 0
    // transition cannot resolve `TSS.rsp0` and cascades silently to a
    // triple fault — the MB13.f post-iretq stall root cause.
    //
    // Must run after `gdt::gdt_init` (which writes the TSS descriptor at
    // slots 5+6) and before `idt::idt_init` (whose #DF / #PF entries
    // reference IST=1 / IST=2 respectively).
    // -------------------------------------------------------------------------
    tss::init_ist_stacks();
    tss::ltr_load();

    // -------------------------------------------------------------------------
    // IDT: load the kernel Interrupt Descriptor Table so that synchronous
    // exceptions (#DE, #DF, #GP, #PF) are caught before they triple-fault.
    // `sti` is NOT issued — interrupts remain disabled throughout the demo.
    // -------------------------------------------------------------------------
    idt::idt_init();

    // -------------------------------------------------------------------------
    // Syscall dispatcher (MB4): configure MSR_LSTAR / MSR_STAR / MSR_FMASK
    // and register INT 0x80 as the compatibility entry vector.
    // -------------------------------------------------------------------------
    bare_metal::syscall_entry::syscall_init();

    let region_count = boot_info.memory_regions.iter().count();

    // -------------------------------------------------------------------------
    // Serial output — exact strings required by the K5 smoke-test assertions.
    // Do not rename or reorder these five lines.
    // -------------------------------------------------------------------------
    early_console::write_str("\n[OMNI OS] kmain entered.\n");
    early_console::write_str("[OMNI OS] kernel version: ");
    early_console::write_str(env!("CARGO_PKG_VERSION"));
    early_console::write_str("\n[OMNI OS] memory regions: ");
    early_console::write_usize(region_count);
    early_console::write_str("\n[OMNI OS] halting (K4 scope ends here).\n");

    // -------------------------------------------------------------------------
    // Page-table mapper (MB2): read current CR3, initialise the walker using
    // the bootloader's direct-map offset. Does not write CR3 — the bootloader
    // page tables remain active; the mapper only adds / walks them.
    //
    // Built BEFORE the frame allocator is filled so that we can validate each
    // Usable region against the active direct-map (MB9). `bootloader 0.11`
    // installs the direct-map via huge pages; `PageMapper::translate` is
    // huge-page aware and resolves those entries correctly.
    // -------------------------------------------------------------------------
    let phys_offset_mb2 = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    // P6.7.8.1 — publish the bootloader direct-map offset to the
    // bare-metal global so the driver-framework syscall handlers
    // (`MmioMap`) can rebuild a `PageMapper` without threading the
    // value through the syscall trampoline. Single-shot write at boot.
    bare_metal::set_phys_offset(phys_offset_mb2);
    let cr3_raw = arch::read_cr3();
    // `mut` because MB10's `spawn_kernel_task` will call `pager.map_4k` to
    // map each task's kernel stack into the isolated VA range.
    let mut pager = paging::PageMapper::new(phys_offset_mb2, memory::PhysAddr(cr3_raw & !0xFFF));
    early_console::write_str("[paging] mapper ready  CR3=");
    #[allow(
        clippy::cast_possible_truncation,
        reason = "x86_64 only; usize is u64 on target_os = none x86_64-unknown-none"
    )]
    early_console::write_usize((cr3_raw & !0xFFF) as usize);
    early_console::write_str("\n");

    // -------------------------------------------------------------------------
    // Physical memory map (MB1 + MB9): register Usable regions with the frame
    // allocator, but only those covered by the bootloader's direct-map. A
    // region whose start or last page does not translate is skipped wholesale,
    // guaranteeing every `alloc_frame()` returns a frame writable through
    // `phys + phys_offset` without faulting.
    //
    // Use addr_of_mut! to avoid the Rust-2024 static_mut_refs lint while
    // keeping the single-core safety invariant explicit.
    // -------------------------------------------------------------------------
    // SAFETY: single-core bare-metal, FRAME_ALLOC is not aliased anywhere.
    #[allow(
        unsafe_code,
        reason = "single-core bare-metal aliasing invariant; SAFETY comment above"
    )]
    let alloc = unsafe { &mut *core::ptr::addr_of_mut!(FRAME_ALLOC) };
    let (validated_bytes, skipped_bytes) =
        register_direct_mapped_regions(alloc, &pager, phys_offset_mb2, boot_info);

    // Reserve the low 1 MiB. Independent of the direct-map check: the BIOS
    // area (real-mode IVT, BIOS data, EBDA, video memory) is not safe for
    // kernel storage even where firmware reports it as Usable and the
    // bootloader maps it.
    alloc.mark_range_used(memory::PhysAddr(0), 0x10_0000);

    #[allow(
        clippy::cast_possible_truncation,
        clippy::integer_division,
        reason = "MiB value always fits u32; truncation to whole MiB is intentional"
    )]
    let free_mib = (alloc.free_bytes() / (1024 * 1024)) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::integer_division,
        reason = "MiB value always fits u32; truncation to whole MiB is intentional"
    )]
    let total_mib = (alloc.total_bytes() / (1024 * 1024)) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::integer_division,
        reason = "MiB value always fits u32; truncation to whole MiB is intentional"
    )]
    let validated_mib = (validated_bytes / (1024 * 1024)) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::integer_division,
        reason = "MiB value always fits u32; truncation to whole MiB is intentional"
    )]
    let skipped_mib = (skipped_bytes / (1024 * 1024)) as u32;

    // -------------------------------------------------------------------------
    // Serial memory diagnostic — informational, after K5 lines.
    // -------------------------------------------------------------------------
    early_console::write_str("[mem] ");
    early_console::write_usize(free_mib as usize);
    early_console::write_str(" MiB free / ");
    early_console::write_usize(total_mib as usize);
    early_console::write_str(" MiB total\n");
    early_console::write_str("[paging] validated ");
    early_console::write_usize(validated_mib as usize);
    early_console::write_str(" MiB direct-mapped, skipped ");
    early_console::write_usize(skipped_mib as usize);
    early_console::write_str(" MiB unmapped\n");
    early_console::write_str("[idt] loaded  vectors=#DE #DF #GP #PF\n");
    early_console::write_str("[syscall] LSTAR set  INT80=0x80\n");

    // -------------------------------------------------------------------------
    // Scheduler (MB6): initialise cooperative round-robin scheduler and
    // spawn the idle task using a single 4 KiB kernel stack frame.
    //
    // The kernel-stack frame returned by `alloc_frame()` is guaranteed to
    // live in the bootloader's direct map: MB9's `register_direct_mapped_regions`
    // filters the bitmap to only contain Usable regions whose start and last
    // page are translatable by the active page tables, so writing the stack
    // frame at `phys + phys_offset` cannot fault.
    // -------------------------------------------------------------------------
    // SAFETY: single-CPU, non-preemptive; SCHEDULER and FRAME_ALLOC are not
    // aliased anywhere else at this point. `pager` was constructed above in
    // this same function and is exclusively borrowed across this block.
    #[cfg(target_arch = "x86_64")]
    #[allow(
        unsafe_code,
        reason = "single-core static-mut deref; aliasing invariant in SAFETY comment"
    )]
    unsafe {
        let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
        let fa = &mut *core::ptr::addr_of_mut!(FRAME_ALLOC);
        if let Some(phys) = fa.alloc_frame() {
            match sched.spawn_kernel_task(
                idle_task,
                phys.0,
                &mut pager,
                fa,
                scheduling::PriorityClass::Idle,
            ) {
                Ok(_) => {
                    early_console::write_str("[sched] scheduler init  idle task spawned\n");
                    early_console::write_str("[stack] kernel stack VA range = ");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "x86_64 only; usize is u64 on target_os = none"
                    )]
                    early_console::write_usize(scheduling::KERNEL_STACK_VA_BASE as usize);
                    early_console::write_str(" .. ");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "x86_64 only; usize is u64 on target_os = none"
                    )]
                    early_console::write_usize(scheduling::KERNEL_STACK_VA_END as usize);
                    early_console::write_str(" (slot 0)\n");
                }
                Err(_) => early_console::write_str("[sched] scheduler init  idle spawn FAILED\n"),
            }
        } else {
            early_console::write_str("[sched] scheduler init  no frame for idle stack\n");
        }
    }

    // -------------------------------------------------------------------------
    // Bootstrap kmain task (MB8): register the current execution flow as a
    // scheduler-visible task BEFORE `sti`, so that the first LAPIC timer
    // tick has a valid `current` to save state into. Uses the boot stack
    // in-place (no owned frame); the sentinel `rsp = 0` is overwritten by
    // the first `omni_context_switch`.
    // -------------------------------------------------------------------------
    #[cfg(target_arch = "x86_64")]
    #[allow(
        unsafe_code,
        reason = "single-core static-mut deref; aliasing invariant in SAFETY comment"
    )]
    unsafe {
        let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
        match sched.spawn_bootstrap_task(scheduling::PriorityClass::System) {
            Ok(_) => early_console::write_str("[sched] bootstrap kmain task registered\n"),
            Err(_) => early_console::write_str("[sched] bootstrap kmain task FAILED\n"),
        }
    }

    // -------------------------------------------------------------------------
    // LAPIC (MB7): disable legacy 8259 PIC, enable xAPIC, start periodic timer
    // at IDT vector 0x20. Issues `sti` to enable maskable interrupts.
    // -------------------------------------------------------------------------
    // Variables surfaced from the MB14.a/c.1 block down to the desktop
    // demo so the System Info panel can render the BSP LAPIC ID + total
    // logical-CPU count. Default to "single CPU" so MADT-walk failures
    // do not break the panel rendering.
    let mut sysinfo_cpu_total: usize = 1;
    let mut sysinfo_bsp_apic_id: u32 = 0;

    // MB14 panel — collect CPUID once and cache it so render_sysinfo
    // can render the brand/vendor/feature rows without re-issuing
    // CPUID on every redraw.
    #[cfg(target_arch = "x86_64")]
    bare_metal::cpuinfo::init();

    #[cfg(target_arch = "x86_64")]
    {
        if bare_metal::lapic::lapic_init(phys_offset_mb2) {
            early_console::write_str("[lapic] timer started  vector=0x20\n");

            // MB14.f.2 — surface the LAPIC mode the firmware left us in
            // (xAPIC by default on QEMU/Proxmox; x2APIC if a BIOS opts
            // in pre-kernel). The kernel never flips the bit at runtime;
            // every primitive (`lapic_eoi`, `lapic_send_ipi`,
            // `read_lapic_id`, `kernel_ap_lapic_init`) routes via MSRs
            // when this flag is set, and via xAPIC MMIO otherwise.
            early_console::write_str("[mb14.f] lapic_mode=");
            early_console::write_str(if bare_metal::lapic::is_x2apic_enabled() {
                "x2APIC\n"
            } else {
                "xAPIC\n"
            });

            // MB14.a — seed the BSP per-CPU descriptor. LAPIC base is now
            // mapped (lapic_init wrote LAPIC_BASE) so read_lapic_id can
            // observe the physical ID, which the descriptor stores under
            // cpu_id=0 (BSP is always slot 0 in the per-CPU array).
            if let Some(lid) = bare_metal::lapic::read_lapic_id() {
                sysinfo_bsp_apic_id = lid;
                bare_metal::per_cpu::init_bsp(lid);
                early_console::write_str("[mb14.a] BSP cpu_id=0 lapic_id=");
                early_console::write_usize(lid as usize);
                early_console::write_str("\n");

                // MB14.b — wire IA32_GS_BASE + IA32_KERNEL_GS_BASE to
                // the BSP descriptor address. After this returns, any
                // kernel context can recover the active per-CPU pointer
                // with `mov rax, gs:[0]` (encoded inside
                // `per_cpu::current_cpu()`) and `omni_syscall_entry`
                // has its `swapgs` ready for the first Ring 3 transition.
                bare_metal::per_cpu::init_gs_base(bare_metal::per_cpu::bsp());
                early_console::write_str("[mb14.b] gs_base=");
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "x86_64 only; usize is u64 on target_os = none"
                )]
                early_console::write_usize(bare_metal::per_cpu::bsp().self_ptr() as usize);
                early_console::write_str("\n");

                // MB14.c.1 — enumerate logical CPUs via the ACPI MADT.
                // No APs are started here; the figure is logged for
                // verification and consumed in MB14.c.2 (INIT-SIPI
                // orchestrator) and MB14.e (per-CPU run-queues).
                //
                // The MADT walk is best-effort: if RSDP or the
                // physical-memory window is unavailable, or any table
                // in the chain is malformed, we log the failure and
                // fall through to BSP-only operation (same behaviour
                // as MB14.b).
                //
                // SAFETY: the same invariants the FADT walker depends
                // on (see `arch::find_pm1a_cnt_from_fadt`): the
                // bootloader-supplied direct-map covers all ACPI
                // tables, and `rsdp_addr` / `physical_memory_offset`
                // are valid for this boot.
                let rsdp = boot_info.rsdp_addr.into_option();
                if let (Some(rsdp_phys), Some(off)) =
                    (rsdp, boot_info.physical_memory_offset.into_option())
                {
                    // SAFETY: bootloader-supplied direct-map covers all ACPI
                    // tables; same invariants as `arch::acpi_poweroff_from_fadt`.
                    #[allow(
                        unsafe_code,
                        reason = "ACPI MADT walk via bootloader direct map; SAFETY above"
                    )]
                    let topo_opt = unsafe { bare_metal::mp::enumerate_cpus(rsdp_phys, off) };
                    if let Some(topo) = topo_opt {
                        sysinfo_cpu_total = topo.enabled_count();
                        early_console::write_str("[mb14.c.1] MADT cpus=");
                        early_console::write_usize(topo.len());
                        early_console::write_str(" enabled=");
                        early_console::write_usize(topo.enabled_count());
                        early_console::write_str("\n");
                        for cpu in topo.entries() {
                            early_console::write_str("[mb14.c.1]   apic_id=");
                            early_console::write_usize(cpu.apic_id as usize);
                            early_console::write_str(if cpu.x2apic { " (x2apic)" } else { "" });
                            early_console::write_str(if cpu.enabled {
                                " enabled"
                            } else {
                                " disabled"
                            });
                            early_console::write_str("\n");
                        }

                        // MB14.c.2.a — INIT-SIPI-SIPI orchestrator (dry-run).
                        //
                        // No LAPIC MMIO occurs: the orchestrator iterates the
                        // discovered topology, builds + encodes the canonical
                        // INIT/SIPI/SIPI ICR values for every enabled non-BSP
                        // AP, and discards them. The real-mode trampoline at
                        // physical 0x8000 lands in MB14.c.2.b, after which
                        // MB14.c.2.c will flip this call to `StartApsMode::Live`.
                        //
                        // We pass `trampoline_page = 0x08` (corresponding to
                        // the planned 0x0000_8000 physical address) so the
                        // SIPI vector field is already in its canonical form
                        // for the encoder tests. With `mode = DryRun` the
                        // orchestrator is guaranteed to make no MMIO accesses
                        // regardless of trampoline_page — the value is purely
                        // a label for the log.
                        let report = bare_metal::mp::start_aps(
                            &topo,
                            lid,
                            0x08,
                            bare_metal::mp::StartApsMode::DryRun,
                        );
                        early_console::write_str("[mb14.c.2.a] start_aps targeted=");
                        early_console::write_usize(report.targeted);
                        early_console::write_str(" sequenced=");
                        early_console::write_usize(report.sequenced);
                        early_console::write_str(if report.dry_run {
                            " (dry-run)\n"
                        } else {
                            " (live)\n"
                        });

                        // MB14.c.2.b.1 — exercise the pure-function trampoline
                        // builders on the BSP so any cross-build regression
                        // surfaces in the boot log before MB14.c.2.b.2 starts
                        // emplacing the blob at physical 0x8000. No MMIO, no
                        // physical writes — the builders return owned values
                        // that we immediately drop after counting non-zero
                        // bytes for the serial banner.
                        let blob = bare_metal::mp_trampoline::build_trampoline_blob(
                            0x0000_8000,
                            0x0000_9000,
                            0xFFFF_FFFF_8010_0000,
                        );
                        let mut blob_nonzero = 0usize;
                        for byte in &blob {
                            if *byte != 0 {
                                blob_nonzero += 1;
                            }
                        }
                        let gdt = bare_metal::mp_trampoline::build_temp_gdt();
                        early_console::write_str("[mb14.c.2.b.1] trampoline blob bytes=");
                        early_console::write_usize(blob.len());
                        early_console::write_str(" nonzero=");
                        early_console::write_usize(blob_nonzero);
                        early_console::write_str(" gdt_entries=");
                        early_console::write_usize(gdt.len());
                        early_console::write_str(" (builder dry-run)\n");

                        // MB14.c.2.c — live AP wake.
                        //
                        // When the MADT enumerated more than one CPU we
                        // (a) emplace the trampoline + landing stub at
                        // phys 0x8000, (b) fire INIT-SIPI-SIPI on every
                        // enabled non-BSP AP via the LAPIC ICR, and
                        // (c) busy-poll the ack counter at phys 0x8140
                        // until every targeted AP has entered the
                        // landing stub. The AP then switches CR3 to the
                        // active kernel address space and jumps to
                        // `kmain_ap` (a #[naked] cli; hlt; jmp $-2
                        // park loop pending MB14.c.2.d).
                        //
                        // On BSP-only systems (enabled_count == 1) we
                        // skip the live path entirely: there is no AP
                        // to wake and reserving frames for nothing would
                        // waste low memory.
                        if topo.enabled_count() > 1 {
                            #[allow(
                                unsafe_code,
                                reason = "single-core BSP context; FRAME_ALLOC not aliased"
                            )]
                            let fa = unsafe { &mut *core::ptr::addr_of_mut!(FRAME_ALLOC) };

                            // -----------------------------------------------
                            // MB14.c.2.d — per-AP pre-fire wiring.
                            //
                            // For every enabled non-BSP AP in the topology,
                            // we allocate:
                            //   - a per-AP kernel stack (1 frame, no guard;
                            //     guard-page protection lands with the
                            //     per-CPU scheduler in MB14.e)
                            //   - per-AP IST1 / IST2 stacks (1 frame each)
                            //   - a per-AP `PerCpu` slot (in AP_SLOTS)
                            //   - a per-AP TSS (in AP_TSS)
                            //   - a per-AP TSS GDT descriptor (slot
                            //     7 + 2*(cpu_id - 1))
                            //   - an `AP_RUNTIME_CONTROL` slot entry that
                            //     hands the running AP its `cpu_id`,
                            //     `kstack_top`, `&PerCpu`, and TSS selector.
                            //
                            // The BSP also stamps the kernel GDTR / IDTR
                            // pseudo-descriptors into the control block so
                            // every AP `lgdt` / `lidt` against the live
                            // kernel tables (the trampoline's temp GDT is
                            // immediately replaced — the AP no longer
                            // depends on low memory after this point).
                            // -----------------------------------------------
                            let (gdtr_base, gdtr_limit) = bare_metal::gdt::gdt_base_and_limit();
                            let (idtr_base, idtr_limit) = bare_metal::idt::idt_base_and_limit();
                            bare_metal::mp_ap_entry::install_descriptor_tables(
                                gdtr_base, gdtr_limit, idtr_base, idtr_limit,
                            );

                            let mut ap_index: u32 = 1;
                            let mut ap_kstack_failures: usize = 0;
                            let mut ap_ist_failures: usize = 0;
                            for cpu in topo.entries() {
                                if !cpu.enabled || cpu.apic_id == lid {
                                    continue;
                                }
                                let cpu_id = ap_index;
                                ap_index += 1;
                                // 1) Allocate per-AP kernel stack +
                                //    IST stacks via direct-map (single
                                //    frame each). Bail out of this AP
                                //    on allocator exhaustion — the BSP
                                //    will still wake any AP whose
                                //    wiring landed.
                                let Some(kstk_top) =
                                    bare_metal::mp_ap_entry::allocate_ap_stack_frame(
                                        fa,
                                        phys_offset_mb2,
                                    )
                                else {
                                    ap_kstack_failures += 1;
                                    continue;
                                };
                                let Some(ist1_top) =
                                    bare_metal::mp_ap_entry::allocate_ap_stack_frame(
                                        fa,
                                        phys_offset_mb2,
                                    )
                                else {
                                    ap_ist_failures += 1;
                                    continue;
                                };
                                let Some(ist2_top) =
                                    bare_metal::mp_ap_entry::allocate_ap_stack_frame(
                                        fa,
                                        phys_offset_mb2,
                                    )
                                else {
                                    ap_ist_failures += 1;
                                    continue;
                                };
                                // 2) Populate per-AP TSS.
                                let _ = bare_metal::tss::init_ap_tss(
                                    cpu_id, kstk_top, ist1_top, ist2_top,
                                );
                                // 3) Register PerCpu slot.
                                let Some(slot) =
                                    bare_metal::per_cpu::register_ap(cpu_id, cpu.apic_id)
                                else {
                                    continue;
                                };
                                slot.set_kernel_rsp(kstk_top);
                                // 4) Place TSS descriptor into kernel GDT.
                                let tss_base = bare_metal::tss::ap_tss_addr(cpu_id);
                                let _ = bare_metal::gdt::gdt_set_ap_tss(cpu_id, tss_base);
                                // 5) Stamp AP_RUNTIME_CONTROL.
                                let tss_sel = bare_metal::gdt::tss_selector_for_cpu(cpu_id);
                                let per_cpu_ptr =
                                    core::ptr::from_ref::<bare_metal::per_cpu::PerCpu>(slot) as u64;
                                let _ = bare_metal::mp_ap_entry::register_ap_runtime_slot(
                                    cpu_id,
                                    cpu.apic_id,
                                    kstk_top,
                                    per_cpu_ptr,
                                    tss_sel,
                                );
                                early_console::write_str("[mb14.c.2.d] ap cpu_id=");
                                early_console::write_usize(cpu_id as usize);
                                early_console::write_str(" lapic=");
                                early_console::write_usize(cpu.apic_id as usize);
                                early_console::write_str(" kstk_top=");
                                #[allow(
                                    clippy::cast_possible_truncation,
                                    reason = "bare-metal x86_64 target: usize is u64"
                                )]
                                early_console::write_usize(kstk_top as usize);
                                early_console::write_str(" tss_sel=");
                                early_console::write_usize(tss_sel as usize);
                                early_console::write_str("\n");
                            }
                            if ap_kstack_failures > 0 || ap_ist_failures > 0 {
                                early_console::write_str("[mb14.c.2.d] stack alloc failures kstk=");
                                early_console::write_usize(ap_kstack_failures);
                                early_console::write_str(" ist=");
                                early_console::write_usize(ap_ist_failures);
                                early_console::write_str("\n");
                            }

                            let kmain_ap_va = bare_metal::mp_ap_entry::kmain_ap as usize as u64;
                            match bare_metal::mp_emplacement::place_trampoline_live(
                                fa,
                                &mut pager,
                                cr3_raw & !0xFFF,
                                kmain_ap_va,
                            ) {
                                Ok(emp) => {
                                    early_console::write_str("[mb14.c.2.c] emplaced tramp_paddr=");
                                    early_console::write_usize(emp.trampoline_paddr as usize);
                                    early_console::write_str(" temp_pml4=");
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        reason = "x86_64; usize is u64 on bare-metal target"
                                    )]
                                    early_console::write_usize(emp.temp_pml4_paddr as usize);
                                    early_console::write_str(" kmain_ap_va=");
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        reason = "x86_64; usize is u64 on bare-metal target"
                                    )]
                                    early_console::write_usize(kmain_ap_va as usize);
                                    early_console::write_str("\n");

                                    // Fire INIT-SIPI-SIPI on every enabled
                                    // non-BSP AP, then busy-poll the ack
                                    // counter until each one has entered
                                    // the landing stub.
                                    let live_report = bare_metal::mp::start_aps_live(
                                        &topo,
                                        lid,
                                        bare_metal::mp_emplacement::TRAMPOLINE_SIPI_VECTOR,
                                        phys_offset_mb2,
                                    );
                                    early_console::write_str(
                                        "[mb14.c.2.c] start_aps_live targeted=",
                                    );
                                    early_console::write_usize(live_report.targeted);
                                    early_console::write_str(" sequenced=");
                                    early_console::write_usize(live_report.sequenced);
                                    early_console::write_str(" acked=");
                                    early_console::write_usize(live_report.acked);
                                    if live_report.acked == live_report.targeted {
                                        early_console::write_str(" (all APs online)\n");
                                    } else {
                                        early_console::write_str(" (timeout)\n");
                                    }

                                    // MB14.c.2.d — busy-poll the per-AP
                                    // online ack counter (incremented by
                                    // the kmain_ap asm post-ltr). This is
                                    // separate from the landing-stub ack:
                                    // it confirms that the AP completed
                                    // its `lgdt` / `lidt` / `ltr` sequence
                                    // and is parked in the steady-state
                                    // hlt loop. Bounded budget — if an AP
                                    // triple-faults after the landing
                                    // stub, the count stalls but the BSP
                                    // does not hang.
                                    let ap_target = live_report.acked as u64;
                                    let mut iter: u64 = 0;
                                    let mut online: u64 = 0;
                                    while iter < 200_000_000 {
                                        online = bare_metal::per_cpu::ap_online_ack();
                                        if online >= ap_target {
                                            break;
                                        }
                                        core::hint::spin_loop();
                                        iter = iter.wrapping_add(1);
                                    }
                                    early_console::write_str("[mb14.c.2.d] per-AP init online=");
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        reason = "bare-metal x86_64: usize is u64"
                                    )]
                                    early_console::write_usize(online as usize);
                                    early_console::write_str("/");
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        reason = "bare-metal x86_64: usize is u64"
                                    )]
                                    early_console::write_usize(ap_target as usize);
                                    if online >= ap_target {
                                        early_console::write_str(" (all APs parked)\n");
                                    } else {
                                        early_console::write_str(" (timeout post-ltr)\n");
                                    }
                                }
                                Err(_e) => {
                                    early_console::write_str(
                                        "[mb14.c.2.c] emplacement FAILED — BSP only\n",
                                    );
                                }
                            }
                        } else {
                            early_console::write_str("[mb14.c.2.c] BSP-only — AP wake skipped\n");
                        }
                    } else {
                        early_console::write_str("[mb14.c.1] MADT walk FAILED — BSP only\n");
                    }
                } else {
                    early_console::write_str(
                        "[mb14.c.1] rsdp / phys_offset unavailable — BSP only\n",
                    );
                }
            } else {
                early_console::write_str(
                    "[mb14.a] read_lapic_id FAILED — descriptor left uninit\n",
                );
            }

            // Enable maskable interrupts — timer can fire from this point on.
            // SAFETY: LAPIC is configured; IDT vector 0x20 handler is installed.
            #[allow(unsafe_code, reason = "sti enable interrupts; SAFETY comment above")]
            unsafe {
                core::arch::asm!("sti", options(nomem, nostack));
            }
            early_console::write_str("[lapic] interrupts enabled\n");

            // MB14.d — TLB shootdown smoke. We issue a benign 4 KiB
            // `invlpg` on a kernel-half address (the trampoline page is
            // mapped both in the BSP's CR3 and unchanged by this call,
            // so the invalidation is observable but inert) and broadcast
            // the IPI on vector `0xFD`. The local `invlpg` always runs;
            // the IPI broadcast occurs only when at least one AP is
            // registered. With MB14.e.1 the AP entry stub now executes
            // `sti` before its `hlt` park, so the 0xFD ISR fires on
            // every AP and `ShootdownReport.acked` reaches `targeted`
            // — the `(all APs acked)` suffix replaces the MB14.d-era
            // `(IRR queued ...)` placeholder.
            {
                let report =
                    mm::flush_tlb_range(crate::memory::VirtAddr(0x0000_0000_0000_8000), 0x1000);
                early_console::write_str("[mb14.d] tlb_shootdown vector=0xFD targeted=");
                early_console::write_usize(report.targeted);
                early_console::write_str(" acked=");
                early_console::write_usize(report.acked);
                early_console::write_str(" local_pages=");
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "bare-metal x86_64: usize is u64"
                )]
                early_console::write_usize(report.local_pages as usize);
                if report.targeted == 0 {
                    early_console::write_str(" (BSP-only — no broadcast)\n");
                } else if report.complete() {
                    early_console::write_str(" (all APs acked)\n");
                } else {
                    early_console::write_str(" (timeout — AP ISR did not ack)\n");
                }
            }

            // MB14.e.2 + MB14.e.3 — per-CPU run-queue scaffold smoke.
            //
            // Exercise the per-CPU run-queue API on the BSP: enqueue
            // a sentinel task id, pop it locally, then enqueue a
            // second sentinel and steal it from a different (idle) AP
            // slot. No real task is created — the queue stores raw
            // u64 ids, and the bridge to `RoundRobinScheduler` will
            // land in MB14.f when AP dispatch goes live. This boot-log
            // smoke confirms the queue + lock primitives are usable
            // from the kernel runtime (no double-fault, no panic) on
            // top of the lifecycle exercised by host-side tests.
            {
                use scheduling::PriorityClass;
                let bsp_cpu = bare_metal::per_cpu::bsp().cpu_id();
                let _ = bare_metal::per_cpu_run_queue::enqueue_on_cpu(
                    bsp_cpu,
                    0xE_E_E_E_E_2_u64,
                    PriorityClass::Interactive,
                );
                let popped = bare_metal::per_cpu_run_queue::pop_for_cpu(bsp_cpu);
                let local_ok = popped == Some(0xE_E_E_E_E_2);

                // Stealing fallback: enqueue on cpu_id 0 (BSP), then
                // request from cpu_id 1 (likely AP slot or empty AP
                // slot if no APs enumerated) — `pop_for_cpu_with_stealing`
                // must surface the BSP task via the steal path.
                let _ = bare_metal::per_cpu_run_queue::enqueue_on_cpu(
                    0,
                    0xE_E_E_E_E_3_u64,
                    PriorityClass::Background,
                );
                let stolen = bare_metal::per_cpu_run_queue::pop_for_cpu_with_stealing(1);
                let steal_ok = stolen == Some(0xE_E_E_E_E_3);

                early_console::write_str("[mb14.e] per_cpu_run_queue local=");
                early_console::write_str(if local_ok { "ok" } else { "FAIL" });
                early_console::write_str(" steal=");
                early_console::write_str(if steal_ok { "ok" } else { "FAIL" });
                early_console::write_str("\n");
            }

            // MB14.g — per-CPU tick + need_resched + scheduler routing smoke.
            //
            // Reads the BSP's `PerCpu.tick_count` immediately after the
            // LAPIC timer has been armed; the value will be 0 until the
            // first periodic tick fires post-`sti`, but the accessor
            // must return without faulting (proves `gs:[0]` is live and
            // the descriptor layout matches MB14.g additions). The
            // `request_resched` / `take_resched` round-trip exercises
            // the per-CPU flag without depending on a real ISR. The
            // final block calls `SCHEDULER.enqueue_for_cpu` +
            // `pick_next_for_cpu` so any future refactor that breaks
            // the dual-write contract surfaces at boot — not only in
            // the host-side tests.
            {
                let cpu = bare_metal::per_cpu::current_cpu();
                let tick = cpu.tick_count();
                cpu.request_resched();
                let took = cpu.take_resched();
                let took2 = cpu.take_resched();
                early_console::write_str("[mb14.g] per_cpu tick=");
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "diagnostic write_usize takes usize; tick count fits trivially"
                )]
                early_console::write_usize(tick as usize);
                early_console::write_str(" resched=");
                early_console::write_str(if took && !took2 { "ok" } else { "FAIL" });
                // SAFETY: single-CPU boot path. The static SCHEDULER is
                // not concurrently aliased here — interrupts are still
                // masked (no `sti` yet at this point of `kmain`).
                //
                // The smoke uses a sentinel id outside the
                // `allocate_task_id` sequence so a stale legacy-mirror
                // entry cannot collide with a real task id. We do not
                // populate the TCB pool: `pick_next_for_cpu` only reads
                // the per-CPU dispatch table (and the legacy mirror
                // via retain-by-id), neither of which dereferences the
                // backing TCB. The retain-by-id in `pick_next_for_cpu`
                // sweeps the legacy mirror clean as a side effect.
                #[allow(
                    unsafe_code,
                    reason = "single-CPU access to static mut SCHEDULER before sti"
                )]
                let routed_ok = unsafe {
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let sentinel = scheduling::TaskId(0xFFFF_FFFF_FFFF_EE14);
                    let pushed =
                        sched.enqueue_for_cpu(0, sentinel, scheduling::PriorityClass::Background);
                    let picked = sched.pick_next_for_cpu(0);
                    pushed && picked == Some(sentinel)
                };
                early_console::write_str(" sched_route=");
                early_console::write_str(if routed_ok { "ok" } else { "FAIL" });
                early_console::write_str("\n");
            }

            // MB14.h.1 — AP-side observer dispatcher smoke.
            //
            // Enqueue a sentinel task id on the first registered AP
            // (`cpu_id = 1`) and wait for any AP to observe it. The
            // AP's LAPIC periodic timer (armed in `kernel_ap_lapic_init`)
            // fires the `omni_lapic_timer_handler` stub, which calls
            // `kernel_check_need_resched`; the MB14.h.1 wire (this
            // milestone) routes the AP branch through
            // `bare_metal::ap_dispatch::kernel_ap_dispatch_observe`,
            // which pops a task id from `per_cpu_run_queue` (with
            // work-stealing fallback) and increments that AP's per-CPU
            // counter — observer-mode only, no context switch
            // (MB14.h.2 ADR-0009).
            //
            // Because `pop_for_cpu_with_stealing` may **steal** the
            // sentinel from `cpu_id=1`'s queue to a sibling AP that
            // happened to fire its timer first, the smoke sums
            // observations across every registered AP slot rather than
            // polling slot `1` alone — the question being answered is
            // "did *any* AP observe the queue?", which is the
            // MB14.h.1 reachability invariant.
            //
            // The poll budget is **anchored on BSP tick count**, not on
            // busy-loop iterations: on QEMU TCG (kvm=0) emulated CPU
            // cycles do not match wall-time, so a fixed iteration count
            // can race past the AP's first LAPIC tick before any AP
            // has had the chance to fire. Using
            // `bsp().tick_count()` as the clock source keeps the
            // budget meaningful on both TCG and KVM: after K BSP ticks
            // the APs have had K equivalent ticks too (the LAPIC
            // periodic timer is per-CPU but armed with identical
            // initial-count + divider on every CPU by
            // `kernel_ap_lapic_init`). K = 32 ticks gives ≈ 5 s on
            // QEMU TCG (≈ 160 ms per tick) and ≈ tens of ms on real
            // silicon, an order of magnitude above the first AP tick.
            //
            // If no AP came online (single-CPU dev VM) the smoke logs
            // `BSP-only` and short-circuits — the BSP must not consume
            // the sentinel itself (its resched trampoline runs the
            // legacy `yield_current` path, not the observer).
            {
                use scheduling::PriorityClass;
                if bare_metal::per_cpu::registered_ap_count() > 0 {
                    const AP_DISPATCH_TICK_BUDGET: u64 = 32;
                    let target_cpu_id: u32 = 1;
                    let _ = bare_metal::per_cpu_run_queue::enqueue_on_cpu(
                        target_cpu_id,
                        0xE_E_E_E_E_4_u64,
                        PriorityClass::Background,
                    );
                    let bsp = bare_metal::per_cpu::bsp();
                    let start_tick = bsp.tick_count();
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "MAX_AP_SLOTS = MAX_CPUS - 1 = 31 fits u32 trivially"
                    )]
                    let max_ap = bare_metal::per_cpu::MAX_AP_SLOTS as u32;
                    let observed: u64 = loop {
                        let mut total: u64 = 0;
                        for cpu_id in 1u32..=max_ap {
                            if let Some(slot) = bare_metal::per_cpu::ap_slot(cpu_id) {
                                total = total.saturating_add(slot.dispatch_observations());
                            }
                        }
                        if total > 0 {
                            break total;
                        }
                        if bsp.tick_count().saturating_sub(start_tick) >= AP_DISPATCH_TICK_BUDGET {
                            break total;
                        }
                        core::hint::spin_loop();
                    };
                    early_console::write_str("[mb14.h.1] ap_dispatch observed=");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "diagnostic write_usize takes usize; observation count fits trivially"
                    )]
                    early_console::write_usize(observed as usize);
                    if observed > 0 {
                        early_console::write_str(" (ok)\n");
                    } else {
                        early_console::write_str(" (timeout — AP did not observe)\n");
                    }
                } else {
                    early_console::write_str("[mb14.h.1] ap_dispatch BSP-only — no AP enrolled\n");
                }
            }

            // MB14.h.2 — cross-CPU context switch primitives smoke.
            //
            // Exercise the three new APIs introduced by MB14.h.2 from
            // the BSP so a regression in any of them surfaces as a
            // boot-time `FAIL` rather than a silent triple-fault at
            // the next AP timer tick:
            //
            // 1. `try_acquire_sched_lock` / `release_sched_lock` —
            //    mutual exclusion on the global SCHED_LOCK.
            // 2. `PerCpu::enter_scheduler` / `leave_scheduler` —
            //    per-CPU recursion guard round-trip.
            // 3. `tss::set_rsp0_for_cpu(0, _)` — BSP-side write that
            //    must succeed unconditionally; an out-of-range AP
            //    cpu_id must be rejected.
            //
            // None of the calls cross-CPU here; the bare-metal AP
            // dispatcher (`kernel_ap_dispatch_observe`) is the path
            // that combines all three in production.
            {
                let lock_ok =
                    scheduling::try_acquire_sched_lock() && !scheduling::try_acquire_sched_lock();
                scheduling::release_sched_lock();
                let bsp = bare_metal::per_cpu::bsp();
                let guard_ok = bsp.enter_scheduler() && !bsp.enter_scheduler();
                bsp.leave_scheduler();
                let tss_ok = bare_metal::tss::set_rsp0_for_cpu(0, 0xFFFF_C000_0000_0000)
                    && !bare_metal::tss::set_rsp0_for_cpu(0xFFFF, 0xDEAD_BEEF);
                early_console::write_str("[mb14.h.2] sched_lock=");
                early_console::write_str(if lock_ok { "ok" } else { "FAIL" });
                early_console::write_str(" per_cpu_in_sched=");
                early_console::write_str(if guard_ok { "ok" } else { "FAIL" });
                early_console::write_str(" set_rsp0_for_cpu=");
                early_console::write_str(if tss_ok { "ok" } else { "FAIL" });
                early_console::write_str("\n");
            }
        } else {
            early_console::write_str("[lapic] LAPIC init FAILED — running without timer\n");
        }
    }

    // -------------------------------------------------------------------------
    // MB8 smoke (feature-gated): spawn two tight-loop tasks that never yield
    // cooperatively, then enter a halt loop. Any 'A'/'B' interleaving on the
    // serial port proves that the LAPIC timer is preempting them.
    //
    // This branch never returns; the desktop demo + power-off below are
    // unreachable when the feature is on. Without the feature the kernel
    // falls through to the regular boot path.
    // -------------------------------------------------------------------------
    #[cfg(all(
        target_arch = "x86_64",
        target_os = "none",
        feature = "mb8-smoke",
        not(test)
    ))]
    bare_metal::mb8_smoke::run(&mut pager);

    // ELF64 parser probe (MB5): parse a minimal embedded test binary to verify
    // the parser is functional before any real userspace binary arrives.
    {
        use bare_metal::elf_loader;
        // A 120-byte hand-crafted ELF64 binary: ET_EXEC, EM_X86_64,
        // one PT_LOAD segment at 0x4000_0000, entry=0x4000_0000.
        static TEST_ELF: [u8; 120] = [
            0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02, 0x00, 0x3E, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        if let Ok(elf) = elf_loader::Elf64::parse(&TEST_ELF) {
            early_console::write_str("[elf] probe OK  entry=");
            #[allow(
                clippy::cast_possible_truncation,
                reason = "x86_64 only; usize is u64 on target_os = none"
            )]
            early_console::write_usize(elf.entry_point() as usize);
            early_console::write_str("\n");
        } else {
            early_console::write_str("[elf] probe FAILED\n");
        }
    }

    // -------------------------------------------------------------------------
    // MB11 user-probe (feature-gated): spawn a Ring 3 process that issues
    // `WriteConsole("hello\n")` + `TaskExit(0)`, then transfer to user mode
    // via the `iretq` trampoline. `TaskExit` halts the CPU, so the desktop
    // demo below is unreachable under this feature.
    //
    // Smoke output expected (in addition to existing K5/LAPIC/sched lines):
    //   [user] address space activated cr3 = 0x...
    //   [user] entering Ring 3 rip = 0x40000000
    //   hello
    //   [user] exit=0
    // -------------------------------------------------------------------------
    #[cfg(all(
        target_arch = "x86_64",
        target_os = "none",
        feature = "mb11-userprobe",
        not(test)
    ))]
    #[allow(
        unsafe_code,
        reason = "single-core static-mut deref + Ring 3 entry; SAFETY in block"
    )]
    {
        use bare_metal::userprobe;
        // SAFETY: single-core; SCHEDULER/FRAME_ALLOC not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
            let fa = &mut *core::ptr::addr_of_mut!(FRAME_ALLOC);
            match userprobe::spawn_userprobe(&mut pager, fa, sched) {
                Ok(task_id) => {
                    early_console::write_str("[user] userprobe spawned  task_id=");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "x86_64 only; usize is u64 on target_os = none"
                    )]
                    early_console::write_usize(task_id.0 as usize);
                    early_console::write_str("\n");
                    if let Some(pcb) = sched.process(task_id) {
                        early_console::write_str("[user] address space activated cr3 = ");
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "x86_64 only; usize is u64 on target_os = none"
                        )]
                        early_console::write_usize(pcb.address_space.pml4_phys.0 as usize);
                        early_console::write_str("\n");
                        early_console::write_str("[user] entering Ring 3 rip = ");
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "x86_64 only; usize is u64 on target_os = none"
                        )]
                        early_console::write_usize(pcb.user_entry as usize);
                        early_console::write_str("\n");
                        bare_metal::usermode::enter_user_mode(
                            pcb.user_entry,
                            pcb.user_stack_top,
                            bare_metal::usermode::USER_RFLAGS,
                            pcb.address_space.pml4_phys.0,
                            pcb.task.kernel_stack_va + scheduling::KERNEL_STACK_SIZE,
                        );
                    } else {
                        early_console::write_str("[user] PCB lookup FAILED\n");
                    }
                }
                Err(_) => early_console::write_str("[user] userprobe spawn FAILED\n"),
            }
        }
    }

    // -------------------------------------------------------------------------
    // MB12-userprobe — cross-process IPC smoke (Track B MB12)
    //
    // Spawns two Ring 3 processes:
    //   - receiver: `IpcReceive(ch=1, buf, 64, blocking=1)` → `WriteConsole` → `TaskExit`
    //   - sender:   `IpcSend(ch=1, kind=3, "ping", 4)` → `TaskExit`
    //
    // The channel is pre-created (open, no capability subject set) by
    // `spawn_userprobe_mb12`. After registering both tasks, `kmain` spawns
    // a bootstrap TCB for itself and `yield_current(Terminated)` to hand
    // the CPU over to the scheduler — the scheduler's MB12.0a/b path then
    // does the CR3 + TSS.rsp0 + iretq trampoline into the first
    // user-vergine task.
    //
    // Expected serial trace (interleaving depends on FIFO order):
    //   [mb12] receiver task_id=N + sender task_id=M + channel id pre-created
    //   ping              (receiver writes after IpcReceive completes)
    //   [user] exit=0     (sender)
    //   [user] exit=0     (receiver)
    //
    // Mutually exclusive with `mb11-userprobe`: when both features are
    // enabled in the same build, the MB11 block above runs first and
    // halts before reaching this code (TaskExit + halt_forever).
    // -------------------------------------------------------------------------
    #[cfg(all(
        target_arch = "x86_64",
        target_os = "none",
        feature = "mb12-userprobe",
        not(feature = "mb11-userprobe"),
        not(test)
    ))]
    #[allow(
        unsafe_code,
        reason = "single-core static-mut deref + Ring 3 entry; SAFETY in block"
    )]
    {
        use bare_metal::userprobe_mb12;
        use scheduling::{PriorityClass, Scheduler, TaskState};
        // SAFETY: single-core; SCHEDULER/FRAME_ALLOC not aliased.
        unsafe {
            let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
            let fa = &mut *core::ptr::addr_of_mut!(FRAME_ALLOC);
            match userprobe_mb12::spawn_userprobe_mb12(&mut pager, fa, sched) {
                Ok((receiver_id, sender_id)) => {
                    early_console::write_str("[mb12] receiver task_id=");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "x86_64 only; usize is u64 on target_os = none"
                    )]
                    early_console::write_usize(receiver_id.0 as usize);
                    early_console::write_str("\n[mb12] sender   task_id=");
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "x86_64 only; usize is u64 on target_os = none"
                    )]
                    early_console::write_usize(sender_id.0 as usize);
                    early_console::write_str("\n[mb12] channel 1 pre-created\n");

                    // Register the currently-executing `kmain` flow as a
                    // bootstrap task so `yield_current` has a `current` to
                    // save context for. The yield to `Terminated` keeps
                    // kmain off the run queue forever; the scheduler then
                    // dispatches the first user process via the MB12.0a/b
                    // first-dispatch path.
                    let _ = sched.spawn_bootstrap_task(PriorityClass::System);
                    if let Some(kmain_id) = sched.current_task_id() {
                        early_console::write_str("[mb12] handing off to user tasks\n");
                        let _ = sched.yield_current(kmain_id, TaskState::Terminated);
                    }
                    // If we ever return here (no runnable task picked),
                    // fall through to halt_forever below.
                    early_console::write_str("[mb12] all user tasks finished\n");
                }
                Err(_) => early_console::write_str("[mb12] spawn FAILED\n"),
            }
        }
        // Silence the desktop-demo arguments — when `mb12-userprobe`
        // is enabled the desktop never runs, so its inputs would
        // otherwise trip unused-variable warnings.
        let _ = framebuffer;
        let _ = region_count;
        let _ = free_mib;
        let _ = total_mib;
        let _ = phys_offset_mb2;
        // Same: the MB14.a sysinfo carries (BSP LAPIC ID + enabled CPU
        // count) are surfaced only to `render_sysinfo` in the desktop
        // path. Silence them on the mb12-userprobe build to keep the
        // workspace warning-clean.
        let _ = sysinfo_cpu_total;
        let _ = sysinfo_bsp_apic_id;
        // After both user processes terminate (or on spawn failure),
        // park the kernel. `halt_forever` diverges (`-> !`) so the
        // subsequent desktop block becomes unreachable on this build.
        bare_metal::arch::halt_forever();
    }

    // -------------------------------------------------------------------------
    // Graphical desktop — blocks until the user requests power-off, then
    // draws the power-off overlay before returning.
    //
    // Unreachable when `mb12-userprobe` is on (the MB12 block above
    // ends in `halt_forever` / `-> !`); silence the lint locally so
    // the rest of the kmain body stays warning-clean.
    // -------------------------------------------------------------------------
    #[allow(
        unreachable_code,
        reason = "mb12-userprobe path diverges before reaching the desktop"
    )]
    demo::run_desktop(
        framebuffer,
        region_count,
        free_mib,
        total_mib,
        phys_offset_mb2,
        sysinfo_cpu_total,
        sysinfo_bsp_apic_id,
    );

    // -------------------------------------------------------------------------
    // ACPI S5 power-off. Use FADT path when RSDP + physical memory map are
    // available (UEFI boot via bootloader 0.11); fall back to PCI scan +
    // hardcoded ports if ACPI tables are unmapped.
    // -------------------------------------------------------------------------
    let rsdp = boot_info.rsdp_addr.into_option();
    let phys_off = boot_info.physical_memory_offset.into_option();
    match (rsdp, phys_off) {
        (Some(rsdp_phys), Some(offset)) => {
            // SAFETY: bootloader maps all physical memory at `offset`;
            // RSDP and ACPI tables are within that window.
            #[allow(
                unsafe_code,
                reason = "ACPI table walk via bootloader direct map; SAFETY above"
            )]
            unsafe {
                arch::acpi_poweroff_from_fadt(rsdp_phys, offset);
            }
        }
        _ => arch::acpi_poweroff(),
    }
    arch::halt_forever()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod sanity {
    use super::KernelError;

    #[test]
    fn kernel_error_is_small() {
        // The error enum should fit in 1 or 2 bytes so it can be returned
        // efficiently from syscall fast-paths.
        assert!(core::mem::size_of::<KernelError>() <= 2);
    }
}
