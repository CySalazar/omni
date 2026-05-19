//! xAPIC Local APIC driver — MB7 deliverable + MB8 preemption hook.
//!
//! Responsibilities:
//! - Disable the legacy 8259 PIC (remap + mask all IRQs).
//! - Read the LAPIC physical base address from `IA32_APIC_BASE` MSR.
//! - Enable the xAPIC via the Spurious Interrupt Vector Register.
//! - Start a periodic timer at IDT vector 0x20 (~160 ms on QEMU TCG).
//! - Send EOI after each timer tick.
//! - MB8: set `NEED_RESCHED`; at the safe tail of the IRQ stub call
//!   `kernel_check_need_resched`, which invokes the existing cooperative
//!   `yield_current` while the trap frame is still on the task's stack.
//!
//! ### Why the two-stage tick → resched split
//!
//! Keeping the tick handler (`kernel_lapic_timer_tick`) minimal — only
//! `TICK_COUNT++`, EOI, set flag — avoids re-entering the scheduler with
//! the LAPIC EOI still pending. The actual `yield_current` runs *after*
//! EOI, at the very end of the asm stub, where the only state on the
//! stack is the trap frame (caller-saved + CPU-pushed RIP/CS/RFLAGS/RSP/SS).
//! `omni_context_switch` then layers its callee-saved frame on top of
//! that, and the original task is later resumed via the matching `iretq`
//! in this stub — see `docs/changelog.md` for the layout.

#![allow(unsafe_code)]

// All items in this file are x86_64-only.
#[cfg(not(target_arch = "x86_64"))]
compile_error!("bare_metal::lapic is x86_64-only");

use core::arch::global_asm;

// ---------------------------------------------------------------------------
// xAPIC MMIO register offsets (from LAPIC base address)
// ---------------------------------------------------------------------------

const LAPIC_ID: u32 = 0x0020; // Local APIC ID Register (xAPIC: ID in bits 31:24)
const LAPIC_EOI: u32 = 0x00B0; // End-of-Interrupt (write 0 to ACK)
const LAPIC_SIVR: u32 = 0x00F0; // Spurious Interrupt Vector Register
/// xAPIC Interrupt Command Register, low dword (writes here latch and fire the IPI).
const LAPIC_ICR_LO: u32 = 0x0300;
/// xAPIC Interrupt Command Register, high dword (write before [`LAPIC_ICR_LO`]).
const LAPIC_ICR_HI: u32 = 0x0310;
const LAPIC_LVT_TIMER: u32 = 0x0320; // Local Vector Table — Timer entry
const LAPIC_TIMER_ICR: u32 = 0x0380; // Initial Count Register
const LAPIC_TIMER_DCR: u32 = 0x03E0; // Divide Configuration Register

/// Bit 12 of `ICR_LO`: `Delivery Status` (RO). Set while a previously
/// issued IPI is still pending on the APIC bus; the BSP must spin until
/// it clears before writing a new ICR command (Intel SDM Vol 3A § 10.6.1).
const LAPIC_ICR_BUSY_MASK: u32 = 1 << 12;

const LAPIC_ENABLE: u32 = 1 << 8; // SIVR bit 8: enable xAPIC
const LVT_TIMER_PERIODIC: u32 = 1 << 17; // LVT timer mode: periodic

// ---------------------------------------------------------------------------
// Global: virtual address of the mapped LAPIC register window.
// Zero means LAPIC has not been initialised.
// ---------------------------------------------------------------------------

static mut LAPIC_BASE: u64 = 0;

// ---------------------------------------------------------------------------
// MSR helpers (local copies — same implementation as syscall_entry.rs but
// private here to keep the modules independent)
// ---------------------------------------------------------------------------

unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(hi) << 32) | u64::from(lo)
}

// ---------------------------------------------------------------------------
// MMIO helpers (volatile reads/writes — required for memory-mapped registers)
// ---------------------------------------------------------------------------

unsafe fn lapic_read(base: u64, reg: u32) -> u32 {
    unsafe { ((base + u64::from(reg)) as *const u32).read_volatile() }
}

unsafe fn lapic_write(base: u64, reg: u32, val: u32) {
    unsafe { ((base + u64::from(reg)) as *mut u32).write_volatile(val) }
}

// ---------------------------------------------------------------------------
// LAPIC physical base address from IA32_APIC_BASE MSR (0x1B)
// ---------------------------------------------------------------------------

fn lapic_phys_base() -> u64 {
    // Bits [51:12] hold the 4 KiB-aligned physical base address.
    unsafe { rdmsr(0x1B) & 0x000F_FFFF_FFFF_F000 }
}

// ---------------------------------------------------------------------------
// Legacy 8259 PIC: remap to vectors 0x20-0x2F, then mask all IRQs.
//
// This must be done before enabling the LAPIC; otherwise spurious PIC
// interrupts (mapped to exceptions 0x08-0x0F by default) would triple-fault.
// ---------------------------------------------------------------------------

unsafe fn disable_legacy_pic() {
    use super::arch::{inb, outb};

    // ICW1: cascade mode, edge-triggered, need ICW4.
    unsafe {
        outb(0x20, 0x11);
        outb(0xA0, 0x11);
    }
    // ICW2: remap master IRQs 0-7 → vectors 0x20-0x27,
    //        slave IRQs 8-15 → vectors 0x28-0x2F.
    unsafe {
        outb(0x21, 0x20);
        outb(0xA1, 0x28);
    }
    // ICW3: master has slave on IRQ2; slave cascade identity = 2.
    unsafe {
        outb(0x21, 0x04);
        outb(0xA1, 0x02);
    }
    // ICW4: 8086/8088 mode.
    unsafe {
        outb(0x21, 0x01);
        outb(0xA1, 0x01);
    }
    // OCW1: mask all IRQs on both PICs.
    unsafe {
        outb(0x21, 0xFF);
        outb(0xA1, 0xFF);
    }
    // Brief I/O delay to let the PIC settle (dummy read from port 0x80).
    unsafe {
        let _ = inb(0x80);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialise the xAPIC and start a periodic timer at IDT vector 0x20.
///
/// Returns `true` on success, `false` if the CPU reports a zero LAPIC base
/// (should not happen on any `x86_64` hardware or QEMU VM).
///
/// Must be called after:
/// - [`super::idt::idt_init`] (IDT must be loaded before registering 0x20).
/// - Physical memory has been mapped via the bootloader direct-map window.
///
/// # Safety invariant
///
/// Single-CPU, non-preemptive. The `LAPIC_BASE` global is written once here
/// and subsequently read (never written) from the timer interrupt handler.
pub fn lapic_init(phys_offset: u64) -> bool {
    let phys = lapic_phys_base();
    if phys == 0 {
        return false;
    }
    let virt = phys + phys_offset;

    unsafe {
        // Step 1: disable legacy PIC so its IRQs cannot fire.
        disable_legacy_pic();

        // Step 2: enable xAPIC via SIVR; spurious vector = 0xFF.
        lapic_write(virt, LAPIC_SIVR, LAPIC_ENABLE | 0xFF);

        // Step 3: configure timer divide-by-16.
        lapic_write(virt, LAPIC_TIMER_DCR, 0x3);

        // Step 4: LVT timer entry — periodic mode, delivery vector 0x20.
        lapic_write(virt, LAPIC_LVT_TIMER, LVT_TIMER_PERIODIC | 0x20);

        // Step 5: set initial count (~160 ms on QEMU TCG 100 MHz bus / 16).
        lapic_write(virt, LAPIC_TIMER_ICR, 1_000_000);

        // Step 6: register the timer interrupt handler in IDT slot 0x20.
        super::idt::idt_set_vector(0x20, omni_lapic_timer_handler as usize as u64);

        // Step 7: store virtual base for EOI writes in the handler.
        LAPIC_BASE = virt;
    }

    true
}

/// Send an End-of-Interrupt signal to the LAPIC.
///
/// Must be called at the end of every hardware interrupt handler before
/// returning via `iretq`; failing to do so masks all future LAPIC interrupts.
pub fn lapic_eoi() {
    unsafe {
        if LAPIC_BASE != 0 {
            lapic_write(LAPIC_BASE, LAPIC_EOI, 0);
        }
    }
}

/// Send an Inter-Processor Interrupt via the xAPIC Interrupt Command
/// Register (MB14.c.2.c).
///
/// `high` is written to `ICR_HI` first (destination field); `low` is
/// written to `ICR_LO` second, which latches the command and dispatches
/// the IPI onto the APIC bus.
///
/// The function busy-waits for `ICR_LO` bit 12 (`Delivery Status`) to
/// clear before writing the new command, per Intel SDM Vol 3A § 10.6.1.
/// The post-write busy poll is the caller's responsibility — see
/// [`lapic_icr_busy`] — because INIT/SIPI sequencing typically interposes
/// a PIT delay between the busy poll and the next write.
///
/// Returns `false` if [`lapic_init`] has not yet succeeded (LAPIC base
/// not mapped); no MMIO occurs in that case. Returns `true` after the
/// MMIO sequence has been issued.
///
/// # Safety
///
/// Caller is responsible for the semantic correctness of `(low, high)`
/// — i.e. that the encoded command is a valid IPI for the current LAPIC
/// configuration. The MMIO sequence itself is unsafe but well-defined.
#[must_use]
pub fn lapic_send_ipi(low: u32, high: u32) -> bool {
    // SAFETY: single-CPU LAPIC base read; the only writer is `lapic_init`
    // which runs once at boot before any caller of this function.
    let base = unsafe { LAPIC_BASE };
    if base == 0 {
        return false;
    }
    // SAFETY: `base` is the bootloader-mapped LAPIC MMIO window; the
    // four register offsets (`ICR_LO` / `ICR_HI`) live inside that 4 KiB
    // page. Volatile reads/writes are mandatory — LAPIC registers are
    // not cacheable RAM.
    unsafe {
        // Drain any prior pending IPI before issuing this one.
        while (lapic_read(base, LAPIC_ICR_LO) & LAPIC_ICR_BUSY_MASK) != 0 {
            core::hint::spin_loop();
        }
        // Write the destination (HI) first, then the command (LO);
        // writing LO latches and fires the IPI.
        lapic_write(base, LAPIC_ICR_HI, high);
        lapic_write(base, LAPIC_ICR_LO, low);
    }
    true
}

/// Poll the `ICR_LO` `Delivery Status` bit (Intel SDM Vol 3A § 10.6.1).
///
/// Returns `true` while a previously issued IPI is still propagating on
/// the APIC bus and a new write to ICR would be discarded. Returns
/// `false` once the bus is idle. Also returns `false` when [`lapic_init`]
/// has not yet been called (LAPIC base unmapped).
#[must_use]
pub fn lapic_icr_busy() -> bool {
    // SAFETY: same single-CPU invariant as [`lapic_send_ipi`].
    let base = unsafe { LAPIC_BASE };
    if base == 0 {
        return false;
    }
    // SAFETY: see [`lapic_send_ipi`] — `ICR_LO` is a 32-bit MMIO register
    // inside the bootloader-mapped LAPIC window.
    let raw = unsafe { lapic_read(base, LAPIC_ICR_LO) };
    (raw & LAPIC_ICR_BUSY_MASK) != 0
}

/// Read the Local APIC ID of the current CPU (MB14.a).
///
/// Returns `None` if [`lapic_init`] has not yet succeeded (LAPIC MMIO base
/// not mapped). Otherwise reads the 32-bit register at LAPIC offset
/// `0x20` and extracts bits `31:24` (xAPIC ID format per Intel SDM Vol 3A
/// §10.4.6). On x2APIC the full 32-bit value would apply, but xAPIC is
/// what QEMU/Proxmox surface for small VMs and what `lapic_init` enables
/// via the SIVR `LAPIC_ENABLE` bit.
///
/// MMIO accesses must be volatile (the LAPIC register window is not
/// cacheable RAM) — `lapic_read` issues `read_volatile`.
#[must_use]
pub fn read_lapic_id() -> Option<u32> {
    // SAFETY: single-CPU read of a static u64 — the only writer is
    // `lapic_init` which runs once at boot before any caller of this
    // function. After that, `LAPIC_BASE` is treated as read-only.
    let base = unsafe { LAPIC_BASE };
    if base == 0 {
        return None;
    }
    // SAFETY: `base` is the virtual address of the LAPIC MMIO window
    // mapped by the bootloader's direct-map; `lapic_read` performs a
    // volatile 32-bit read from a 4 KiB-aligned page that holds the
    // LAPIC register block.
    let raw = unsafe { lapic_read(base, LAPIC_ID) };
    Some(raw >> 24)
}

// ---------------------------------------------------------------------------
// Timer interrupt handler — IDT vector 0x20
// ---------------------------------------------------------------------------

// Assembly stub: save caller-saved registers, call the tick handler, then
// call the resched trampoline (which may perform a cooperative context
// switch via `omni_context_switch`), restore caller-saved, `iretq`.
//
// Stack alignment check (System V AMD64 ABI):
//   Interrupt entry: CPU pushes SS, RSP, RFLAGS, CS, RIP = 5 × 8 = 40 bytes.
//   Before interrupt, RSP was 16-byte aligned → after CPU push, RSP % 16 = 8.
//   We push 9 caller-saved registers × 8 bytes = 72 bytes → RSP % 16 = 8+72 = 80 ≡ 0.
//   `call` pushes 8-byte return address → RSP % 16 = 8 at function entry. ✓
//   The second `call` (kernel_check_need_resched) starts from the same
//   alignment because the first `call` has fully returned (its RA popped).
global_asm!(
    ".global omni_lapic_timer_handler",
    "omni_lapic_timer_handler:",
    "    push rax",
    "    push rcx",
    "    push rdx",
    "    push rsi",
    "    push rdi",
    "    push r8",
    "    push r9",
    "    push r10",
    "    push r11",
    "    call kernel_lapic_timer_tick",
    "    call kernel_check_need_resched",
    "    pop r11",
    "    pop r10",
    "    pop r9",
    "    pop r8",
    "    pop rdi",
    "    pop rsi",
    "    pop rdx",
    "    pop rcx",
    "    pop rax",
    "    iretq",
);

unsafe extern "C" {
    fn omni_lapic_timer_handler();
}

/// Rust callback invoked on every LAPIC timer tick (IDT vector 0x20).
///
/// Strict subset of work that is safe inside an Interrupt Gate handler:
/// monotonic counter, EOI ack, and a single atomic flag store. The actual
/// reschedule decision happens in [`kernel_check_need_resched`] just
/// before `iretq`.
#[unsafe(no_mangle)]
extern "C" fn kernel_lapic_timer_tick() {
    // `TICK_COUNT` is gated `target_os = "none"` (lives only in the real
    // bare-metal kernel image); the timer ISR itself is only reachable in
    // that build because the IDT vector that drives this stub is set up by
    // `lapic_init()`, which is gated by the same triplet. On host clippy/
    // test builds (`target_os = "linux"`), the body would not compile —
    // skip it. SAFETY (when active): single-CPU; no other writer exists.
    #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
    unsafe {
        crate::TICK_COUNT = crate::TICK_COUNT.wrapping_add(1);
    }
    lapic_eoi();
    // Every tick requests a reschedule for now; quantum-based throttling
    // is an MB9 concern.
    crate::scheduling::NEED_RESCHED.store(true, core::sync::atomic::Ordering::Release);
}

/// Tail-of-interrupt trampoline: if the LAPIC tick set `NEED_RESCHED`, run
/// the cooperative `yield_current` path so the next task is on-CPU before
/// the `iretq` restores the trap frame.
///
/// Re-entrancy: if another scheduler call is already on this CPU's stack
/// (e.g. a cooperative `TaskYield` syscall in flight when the timer fired),
/// `IN_SCHEDULER` is set and we skip the yield to avoid recursing into the
/// scheduler. The flag will be cleared by the outer call and the next tick
/// will pick the resched up.
// The early-return guard at the top is `return;` followed by the unsafe
// block; on host builds (`target_os = "linux"`) the unsafe block is
// `#[cfg]`-ed out, so `return;` becomes the last statement and clippy
// flags it as needless. The early return is the intended IRQ-tail
// pattern (read flags, leave if nothing to do), keep it.
#[allow(clippy::needless_return)]
#[unsafe(no_mangle)]
extern "C" fn kernel_check_need_resched() {
    use core::sync::atomic::Ordering;

    // Two short-circuit guards: (1) nothing requested a reschedule on the
    // last tick — leave; (2) another scheduler call is already on this
    // CPU's stack (cooperative `TaskYield` in flight) — leave to avoid
    // recursing into the scheduler. The flag will be cleared by the
    // outer call and the next tick picks the resched up.
    if !crate::scheduling::NEED_RESCHED.swap(false, Ordering::AcqRel)
        || crate::scheduling::IN_SCHEDULER.load(Ordering::Acquire)
    {
        return;
    }

    // SAFETY: single-CPU, no SMP. `SCHEDULER` is not concurrently aliased
    // — the only other accessor is the cooperative path which is gated by
    // IN_SCHEDULER above. We grab a mutable reference for the duration of
    // a single `yield_current` and release IN_SCHEDULER before returning.
    #[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]
    unsafe {
        use crate::scheduling::Scheduler;
        let sched = &mut *core::ptr::addr_of_mut!(crate::SCHEDULER);
        let Some(cur) = sched.current_task_id() else {
            return;
        };
        crate::scheduling::IN_SCHEDULER.store(true, Ordering::Release);
        let _ = sched.yield_current(cur, crate::scheduling::TaskState::Runnable);
        crate::scheduling::IN_SCHEDULER.store(false, Ordering::Release);
    }
}
