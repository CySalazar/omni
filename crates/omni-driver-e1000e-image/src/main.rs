//! OMNI OS Intel e1000e bootable driver image — P6.7.8.10.
//!
//! `no_std + no_main` ELF entry that the kernel `DriverLoad (73)`
//! syscall ingests per `OIP-Driver-Framework-013` § S5.3 step 9. The
//! kernel calls `spawn_from_elf` against this binary, which lands at
//! `_start` in a freshly minted Ring 3 process. Before transferring
//! control the kernel writes the per-driver capability deposit at the
//! well-known user-VA slot [`omni_driver_shared::DRIVER_CAP_DEPOSIT_VA`]
//! (P6.7.8.9, OIP-013 § S5.3 step 8); the image reads tokens from that
//! window via [`omni_driver_shared::caps::find_token`] and forwards them
//! to the kernel through the `MmioMap (70)` / `DmaMap (71)` /
//! `IrqAttach (72)` syscalls.
//!
//! ## Execution path
//!
//! Live wiring (P6.7.8.10):
//! 1. `find_token(ACTION_TAG_MMIO_MAP, ..)`  — retrieve the MMIO token.
//! 2. `find_token(ACTION_TAG_DMA_MAP, ..)`   — retrieve the DMA token.
//! 3. `find_token(ACTION_TAG_IRQ_ATTACH,..)` — retrieve the IRQ token.
//! 4. `syscall MmioMap`   — map the e1000e BAR0 128 KiB CSR window.
//! 5. `syscall DmaMap`    — install the 4 GiB IOVA arena.
//! 6. `syscall IrqAttach` — bind the combined RX/TX MSI-X vector.
//! 7. Drive the [`omni_driver_e1000e::bringup::BringUp`] 13-step FSM
//!    until `Phase::Ready` (or any terminal `Failed` state).
//! 8. `TaskExit(0)` on success / non-zero sentinel on any failure.
//!
//! ## Standalone execution
//!
//! When this binary is executed without going through `DriverLoad` (a
//! diagnostic scenario), `find_token` returns `None` because the deposit
//! page is not mapped; the image then exits with sentinel codes 10/20/30
//! identifying which token is missing.
//!
//! Pattern mirrors the `omni-driver-nvme-image` sibling refactored in
//! P6.7.8.10 and the `omni-driver-net-virtio-image` sibling.
//!
//! Build:
//!
//! ```sh
//! cargo build --manifest-path crates/omni-driver-e1000e-image/Cargo.toml \
//!             --target x86_64-unknown-none --release
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

use omni_driver_e1000e::bringup::{BringUp, Event, Phase};
use omni_driver_shared::{
    ACTION_TAG_DMA_MAP, ACTION_TAG_IRQ_ATTACH, ACTION_TAG_MMIO_MAP, caps::find_token,
};

// =============================================================================
// Global allocator stub
// =============================================================================

struct PanicOnAlloc;

unsafe impl GlobalAlloc for PanicOnAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        // SAFETY: any reachable allocation is a driver bug — bail loudly.
        panic!("omni-driver-e1000e-image: heap alloc requested but no allocator is wired");
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No-op without a heap.
    }
}

#[global_allocator]
static GLOBAL_ALLOC: PanicOnAlloc = PanicOnAlloc;

// =============================================================================
// Syscall numbers
// =============================================================================

/// `TaskExit (11)`.
const SYS_TASK_EXIT: u64 = 11;
/// `MmioMap (70)`.
const SYS_MMIO_MAP: u64 = 70;
/// `DmaMap (71)`.
const SYS_DMA_MAP: u64 = 71;
/// `IrqAttach (72)`.
const SYS_IRQ_ATTACH: u64 = 72;

// =============================================================================
// Driver-specific placeholder constants (mirror `manifest.toml`)
// =============================================================================

/// e1000e BAR0 physical base address (QEMU `-device e1000e` Q35 default).
const E1000E_BAR0_PHYS_BASE: u64 = 0xFEB0_0000;

/// e1000e BAR0 length per Intel 82574L datasheet § 10.1 (128 KiB CSR window).
const E1000E_BAR0_LEN: u64 = 0x20000;

/// MmioMap flags = 0 (uncached default).
const MMIO_FLAGS_DEFAULT: u64 = 0;

/// DMA arena IOVA base.
const DMA_IOVA_BASE: u64 = 0x0;

/// DMA arena length = 4 GiB per OIP-Driver-Net-015 § S1.
const DMA_LEN_4_GIB: u64 = 0x1_0000_0000;

/// DMA direction = bidirectional (RX descriptors + TX descriptors share arena).
const DMA_DIR_BIDIR: u64 = 2;

/// Placeholder IRQ line for the e1000e combined MSI-X vector.
const IRQ_LINE_E1000E: u64 = 35;

/// Placeholder IPC channel ID the kernel signals on this IRQ vector.
const IPC_CHANNEL_PLACEHOLDER: u64 = 0;

// =============================================================================
// TaskExit sentinel codes
// =============================================================================

/// Successful FSM convergence to `Phase::Ready`.
const EXIT_OK: u64 = 0;
/// FSM converged to a terminal `Failed` state.
const EXIT_FSM_FAILED: u64 = 1;
/// No `MmioMap` token in the deposit window.
const EXIT_NO_MMIO_TOKEN: u64 = 10;
/// No `DmaMap` token in the deposit window.
const EXIT_NO_DMA_TOKEN: u64 = 20;
/// No `IrqAttach` token in the deposit window.
const EXIT_NO_IRQ_TOKEN: u64 = 30;
/// Base sentinel: `MmioMap` syscall returned non-zero errno.
const EXIT_MMIO_BASE: u64 = 40;
/// Base sentinel: `DmaMap` syscall returned non-zero errno.
const EXIT_DMA_BASE: u64 = 60;
/// Base sentinel: `IrqAttach` syscall returned non-zero errno.
const EXIT_IRQ_BASE: u64 = 80;

// =============================================================================
// Raw syscall wrapper
// =============================================================================

/// Issue a `syscall` with the given number and up to 5 arguments. Returns
/// the `(rax, rdx)` pair — the two-register convention used by the
/// driver-framework syscalls per `OIP-Driver-Framework-013` § S2.
#[inline(always)]
unsafe fn syscall5(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (u64, u64) {
    let mut rax: u64 = number;
    let mut rdx_out: u64;
    // SAFETY: `syscall` is the canonical Ring 3 → Ring 0 transition on
    // `x86_64`; rax/rcx/r11 are clobbered by the CPU per the SDM.
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") rax,
            in("rdi") a0,
            in("rsi") a1,
            inout("rdx") a2 => rdx_out,
            in("r10") a3,
            in("r8")  a4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags),
        );
    }
    (rax, rdx_out)
}

/// Issue `TaskExit(code)` — diverges on the bare-metal kernel.
#[inline(always)]
unsafe fn sys_exit(code: u64) -> ! {
    // SAFETY: TaskExit terminates the process; Phase 1 ignores the code value.
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_TASK_EXIT,
            in("rdi") code,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

// =============================================================================
// Driver entry — _start
// =============================================================================

/// ELF entry point. The kernel's `spawn_from_elf` jumps here with
/// `rsp = user_stack_top` and the capability deposit mapped read-only at
/// [`omni_driver_shared::DRIVER_CAP_DEPOSIT_VA`].
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Step 1 — Retrieve the three capability tokens from the deposit.
    let Some(mmio_token) = find_token(ACTION_TAG_MMIO_MAP, |_| true) else {
        // SAFETY: sys_exit diverges.
        unsafe { sys_exit(EXIT_NO_MMIO_TOKEN) };
    };
    let Some(dma_token) = find_token(ACTION_TAG_DMA_MAP, |_| true) else {
        unsafe { sys_exit(EXIT_NO_DMA_TOKEN) };
    };
    let Some(irq_token) = find_token(ACTION_TAG_IRQ_ATTACH, |_| true) else {
        unsafe { sys_exit(EXIT_NO_IRQ_TOKEN) };
    };

    // Step 2 — `MmioMap (70)`: install the e1000e CSR window (128 KiB).
    let (_mmio_va, mmio_errno) = unsafe {
        syscall5(
            SYS_MMIO_MAP,
            E1000E_BAR0_PHYS_BASE,
            E1000E_BAR0_LEN,
            MMIO_FLAGS_DEFAULT,
            mmio_token.as_ptr() as u64,
            mmio_token.len() as u64,
        )
    };
    if mmio_errno != 0 {
        unsafe { sys_exit(EXIT_MMIO_BASE + mmio_errno) };
    }

    // Step 3 — `DmaMap (71)`: install the 4 GiB IOVA arena.
    let (_dma_iova, dma_errno) = unsafe {
        syscall5(
            SYS_DMA_MAP,
            DMA_IOVA_BASE,
            DMA_LEN_4_GIB,
            DMA_DIR_BIDIR,
            dma_token.as_ptr() as u64,
            dma_token.len() as u64,
        )
    };
    if dma_errno != 0 {
        unsafe { sys_exit(EXIT_DMA_BASE + dma_errno) };
    }

    // Step 4 — `IrqAttach (72)`: bind the MSI-X vector to an IPC channel.
    // ABI: a0 = irq_line, a1 = ipc_channel_id, a2/a3 = cap_ptr/cap_len.
    let (_irq_vec, irq_errno) = unsafe {
        syscall5(
            SYS_IRQ_ATTACH,
            IRQ_LINE_E1000E,
            IPC_CHANNEL_PLACEHOLDER,
            irq_token.as_ptr() as u64,
            irq_token.len() as u64,
            0,
        )
    };
    if irq_errno != 0 {
        unsafe { sys_exit(EXIT_IRQ_BASE + irq_errno) };
    }

    // Step 5 — Drive the 13-step bring-up FSM through its remaining
    // pure-state phases. With MMIO + DMA + IRQ installed, the FSM can
    // reach `Phase::Ready` via repeated `Event::Advance`.
    let mut bringup = BringUp::new();
    while !bringup.phase().is_terminal() {
        match bringup.on_event(Event::Advance) {
            Ok(next) => bringup = next,
            Err(_) => break,
        }
    }

    let code = if matches!(bringup.phase(), Phase::Ready) {
        EXIT_OK
    } else {
        EXIT_FSM_FAILED
    };
    // SAFETY: TaskExit never returns.
    unsafe { sys_exit(code) }
}

// =============================================================================
// Panic handler (required by `no_std`)
// =============================================================================

/// On panic, exit with a sentinel non-zero code so the kernel boot log
/// can correlate against the bring-up retry counter.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // SAFETY: TaskExit terminates the process unconditionally.
    unsafe { sys_exit(2) }
}
