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
pub mod ipc;
pub mod memory;
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

/// LAPIC timer tick counter — incremented on every IDT vector 0x20 interrupt.
///
/// Written exclusively from [`bare_metal::lapic::kernel_lapic_timer_tick`]
/// (single-CPU, non-preemptive — no synchronisation needed at this stage).
#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
pub static mut TICK_COUNT: u64 = 0;

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
    #[cfg(target_arch = "x86_64")]
    {
        if bare_metal::lapic::lapic_init(phys_offset_mb2) {
            early_console::write_str("[lapic] timer started  vector=0x20\n");
            // Enable maskable interrupts — timer can fire from this point on.
            // SAFETY: LAPIC is configured; IDT vector 0x20 handler is installed.
            #[allow(unsafe_code, reason = "sti enable interrupts; SAFETY comment above")]
            unsafe {
                core::arch::asm!("sti", options(nomem, nostack));
            }
            early_console::write_str("[lapic] interrupts enabled\n");
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
