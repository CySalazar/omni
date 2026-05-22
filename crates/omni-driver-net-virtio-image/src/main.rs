//! OMNI OS virtio-net bootable driver image — P6.7.8.10.
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
//! 4. `syscall MmioMap`   — map the virtio-net BAR4 region.
//! 5. `syscall DmaMap`    — install the 4 GiB IOVA arena.
//! 6. `syscall IrqAttach` — bind RX/TX MSI-X vector to an IPC channel.
//! 7. Drive the [`omni_driver_net_virtio::bringup::BringUp`] FSM until
//!    `Phase::DriverOk` (or any terminal `Failed` state).
//! 8. `TaskExit(0)` on success / non-zero sentinel on any failure.
//!
//! ## Standalone execution
//!
//! When this binary is executed without going through `DriverLoad` (a
//! diagnostic scenario), `find_token` returns `None` because the deposit
//! page is not mapped; the image then exits with sentinel codes 10/20/30
//! identifying which token is missing. This is the expected behaviour
//! and surfaces loudly so the absence of the loader path is unambiguous.
//!
//! Build:
//!
//! ```sh
//! cargo build --manifest-path crates/omni-driver-net-virtio-image/Cargo.toml \
//!             --target x86_64-unknown-none --release
//! ```
//!
//! The resulting `target/x86_64-unknown-none/release/omni-driver-net-virtio-image`
//! ELF is fed to `omni-driver-pack` along with the `manifest.toml` template
//! to produce the `omni-pack v1` blob the kernel verifies.

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

use omni_driver_net_virtio::bringup::{BringUp, Event, Phase};
use omni_driver_shared::{
    ACTION_TAG_DMA_MAP, ACTION_TAG_IRQ_ATTACH, ACTION_TAG_MMIO_MAP, caps::find_token,
};

// =============================================================================
// Global allocator stub
// =============================================================================
//
// The `omni-driver-net-virtio` library has `extern crate alloc` because
// future bring-up code will need `alloc::vec::Vec` for the RX buffer
// table. The P6.7.8.10 wiring does not allocate at runtime — the
// bring-up FSM is `Copy`, the syscall wrappers use `[u64; 6]` on the
// stack, `find_token` returns a slice into the kernel-mapped deposit
// page, and we never instantiate any heap container.

struct PanicOnAlloc;

unsafe impl GlobalAlloc for PanicOnAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        // SAFETY: any reachable allocation is a driver bug — bail loudly
        // via the panic handler which `TaskExit`s the process.
        panic!("omni-driver-net-virtio-image: heap alloc requested but no allocator is wired");
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Unreachable in practice (no `alloc` ever succeeds), but a no-op
        // dealloc is the only sound option without an allocator.
    }
}

#[global_allocator]
static GLOBAL_ALLOC: PanicOnAlloc = PanicOnAlloc;

// =============================================================================
// Syscall numbers (mirrors `omni_kernel::syscall::SyscallNumber` — pinned
// here so the image does not pull in the kernel crate, which would create
// a circular workspace dep).
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
// Driver-specific placeholder constants
// =============================================================================
//
// These mirror the values declared in `crates/omni-driver-net-virtio/manifest.toml`
// for the v0.3 virtio-net driver. In a real DriverLoad path the kernel
// minted a capability token whose `Resource::MmioRegion` covers exactly
// `[phys_base, phys_base + len)`; if the loader's PCI enumeration
// produced different concrete values, the manifest in question would
// have been re-generated and re-signed before reaching `DriverLoad`. So
// these constants are the contract surface between the offline
// `omni-driver-pack` tool and the on-device bring-up.
//
// For v0.3 (QEMU `-device virtio-net-pci`) the typical BAR4 layout
// places the modern virtio configuration window at `0xFEBC_0000` with a
// 4 KiB stride. The DMA window covers a 4 GiB IOVA arena per
// `OIP-Driver-Net-015` § S1.

/// virtio-net BAR4 physical base address (Q35 default).
const VIRTIO_BAR4_PHYS_BASE: u64 = 0xFEBC_0000;

/// virtio-net BAR4 length (1 page covers Common + Notify + ISR + Device
/// regions in the modern layout; manifests may declare more).
const VIRTIO_BAR4_LEN: u64 = 0x1000;

/// MmioMap flags = 0 (uncached default, no opt-in WC).
const MMIO_FLAGS_DEFAULT: u64 = 0;

/// DMA arena IOVA base.
const DMA_IOVA_BASE: u64 = 0x0;

/// DMA arena length = 4 GiB per OIP-Driver-Net-015 § S1.
const DMA_LEN_4_GIB: u64 = 0x1_0000_0000;

/// DMA direction = bidirectional (RX + TX share the arena in v0.3).
const DMA_DIR_BIDIR: u64 = 2;

/// Placeholder IRQ line for the virtio-net combined MSI-X vector.
const IRQ_LINE_VIRTIO_NET: u64 = 33;

/// Placeholder IPC channel ID the kernel signals on this IRQ vector.
const IPC_CHANNEL_PLACEHOLDER: u64 = 0;

// =============================================================================
// TaskExit sentinel codes
// =============================================================================

/// Successful FSM convergence to `Phase::DriverOk`.
const EXIT_OK: u64 = 0;
/// FSM converged to a terminal `Failed` state.
const EXIT_FSM_FAILED: u64 = 1;
/// No `MmioMap` token in the deposit window (standalone execution).
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
// Raw syscall wrapper (System V AMD64 ABI: rax=number, rdi/rsi/rdx/r10/r8/r9
// = a0..a5; rcx + r11 clobbered by `syscall`).
// =============================================================================

/// Issue a `syscall` with the given number and up to 5 arguments. Returns
/// the `(rax, rdx)` pair — the two-register convention used by the
/// driver-framework syscalls per `OIP-Driver-Framework-013` § S2.
#[inline(always)]
unsafe fn syscall5(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (u64, u64) {
    let mut rax: u64 = number;
    let mut rdx_out: u64;
    // SAFETY: `syscall` is the canonical Ring 3 → Ring 0 transition on
    // `x86_64`; rax/rcx/r11 are clobbered by the CPU per the SDM. The
    // kernel's `omni_syscall_entry` preserves the rest of the GPR
    // file across the call.
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

/// Issue `TaskExit(code)` — diverges on the bare-metal kernel. The
/// kernel's `task_exit` handler never returns to user space (it dequeues
/// the task and yields); the trailing `loop {}` is defensive against a
/// hypothetical kernel bug that lets the syscall return.
#[inline(always)]
unsafe fn sys_exit(code: u64) -> ! {
    // SAFETY: TaskExit invariant: `code` is just a numeric exit-status
    // bag; the kernel does not interpret it semantically in Phase 1.
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
/// `rsp = user_stack_top` and the capability deposit window mapped
/// read-only at [`omni_driver_shared::DRIVER_CAP_DEPOSIT_VA`].
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Step 1 — Retrieve the three capability tokens from the deposit.
    // Absence of a token means the image was launched outside of
    // `DriverLoad` (standalone) — emit a sentinel exit so the caller
    // distinguishes "no loader" from "loader rejected the token".
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

    // Step 2 — `MmioMap (70)`: install the device CSR window.
    let (_mmio_va, mmio_errno) = unsafe {
        syscall5(
            SYS_MMIO_MAP,
            VIRTIO_BAR4_PHYS_BASE,
            VIRTIO_BAR4_LEN,
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
            IRQ_LINE_VIRTIO_NET,
            IPC_CHANNEL_PLACEHOLDER,
            irq_token.as_ptr() as u64,
            irq_token.len() as u64,
            0,
        )
    };
    if irq_errno != 0 {
        unsafe { sys_exit(EXIT_IRQ_BASE + irq_errno) };
    }

    // Step 5 — Drive the bring-up FSM through its remaining pure-state
    // phases. With MMIO + DMA + IRQ all installed, the FSM's
    // `Phase::DriverOk` is reachable via repeated `Event::Advance`.
    let mut bringup = BringUp::new();
    while !bringup.phase().is_terminal() {
        match bringup.on_event(Event::Advance) {
            Ok(next) => bringup = next,
            Err(_) => break,
        }
    }

    let code = if matches!(bringup.phase(), Phase::DriverOk) {
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
