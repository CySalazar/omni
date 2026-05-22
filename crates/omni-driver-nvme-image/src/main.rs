//! OMNI OS NVMe bootable driver image — P6.7.8.10.
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
//! Live wiring (P6.7.8.10, extended P6.7.10-pre.4):
//! 1. `find_token(ACTION_TAG_MMIO_MAP, ..)`  — retrieve the MMIO token.
//! 2. `find_token(ACTION_TAG_DMA_MAP, ..)`   — retrieve the DMA token.
//! 3. `find_token(ACTION_TAG_IRQ_ATTACH,..)` — retrieve the IRQ token.
//! 4. `syscall MmioMap`   — map the NVMe BAR0 16 KiB CSR window.
//! 5. `syscall DmaMap`    — install the 4 GiB IOVA arena.
//! 6. `syscall IrqAttach` — bind the admin CQ MSI-X vector.
//! 7. **P6.7.10-pre.4** — `syscall IpcCreateChannel(20)` allocates the
//!    kernel-side BLK channel; the kernel returns a fresh
//!    [`omni-kernel::ipc::ChannelId`](../../omni_kernel/ipc/struct.ChannelId.html)
//!    the driver owns.
//! 8. **P6.7.10-pre.4** — `syscall BlkRegister(76)` records the
//!    `omni.svc.blk.nvme0` → `ChannelId` mapping in the kernel BLK
//!    registry per `OIP-Driver-NVMe-014` § S4 + § S6 step 12.
//! 9. **P6.7.10-pre.4** — `syscall BlkLookup(78)` round-trips the
//!    registration as a defence-in-depth check; mismatch aborts the
//!    driver before any FSM advance so the filesystem service is
//!    guaranteed to find the right channel id at boot.
//! 10. **P6.7.10-pre.17** — `disable_controller` (clears `CC.EN`,
//!     polls `CSTS.RDY = 0`); `program_admin_queue_bases` writes
//!     `AQA` + `ASQ` + `ACQ` per NVMe 1.4 § 3.1.7-9;
//!     `enable_controller` (sets `CC.EN`, polls `CSTS.RDY = 1`).
//!     All three calls go through the `LiveMmioBackend` newtype
//!     which performs raw 32-bit `volatile_write` / `volatile_read`
//!     against the BAR0 user-VA returned by `MmioMap` at step 4.
//! 11. Drive the [`omni_driver_nvme::bringup::BringUp`] 13-step FSM until
//!     `Phase::Ready` (or any terminal `Failed` state).
//! 12. `TaskExit(0)` on success / non-zero sentinel on any failure.
//!
//! ## Standalone execution
//!
//! When this binary is executed without going through `DriverLoad` (a
//! diagnostic scenario), `find_token` returns `None` because the deposit
//! page is not mapped; the image then exits with sentinel codes 10/20/30
//! identifying which token is missing.
//!
//! Pattern mirrors the `omni-driver-net-virtio-image` sibling refactored
//! in P6.7.8.10 and the `omni-driver-e1000e-image` sibling.
//!
//! Build:
//!
//! ```sh
//! cargo build --manifest-path crates/omni-driver-nvme-image/Cargo.toml \
//!             --target x86_64-unknown-none --release
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

use omni_driver_nvme::bringup::{BringUp, Event, Phase};
use omni_driver_nvme::queue::{
    MmioBackend, MmioReadBackend, disable_controller, enable_controller,
    program_admin_queue_bases,
};
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
        panic!("omni-driver-nvme-image: heap alloc requested but no allocator is wired");
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
/// `IpcCreateChannel (20)` — allocates the kernel-side BLK channel.
const SYS_IPC_CREATE_CHANNEL: u64 = 20;
/// `MmioMap (70)`.
const SYS_MMIO_MAP: u64 = 70;
/// `DmaMap (71)`.
const SYS_DMA_MAP: u64 = 71;
/// `IrqAttach (72)`.
const SYS_IRQ_ATTACH: u64 = 72;
/// `BlkRegister (76)` — records the `omni.svc.blk.<disk_slot>` → live
/// `ChannelId` mapping in the kernel BLK registry per
/// `OIP-Driver-NVMe-014` § S4 + § S6 step 12 (P6.7.10-pre.3).
const SYS_BLK_REGISTER: u64 = 76;
/// `BlkLookup (78)` — read-only resolution of `disk_slot → ChannelId`
/// against the kernel BLK registry (P6.7.10-pre.3).
const SYS_BLK_LOOKUP: u64 = 78;

// =============================================================================
// Driver-specific placeholder constants (mirror `manifest.toml`)
// =============================================================================

/// NVMe BAR0 physical base address (QEMU `-device nvme` Q35 default).
const NVME_BAR0_PHYS_BASE: u64 = 0xFEBF_0000;

/// NVMe BAR0 length per OIP-014 § S1 (16 KiB CSR window).
const NVME_BAR0_LEN: u64 = 0x4000;

/// MmioMap flags = 0 (uncached default).
const MMIO_FLAGS_DEFAULT: u64 = 0;

/// DMA arena IOVA base.
const DMA_IOVA_BASE: u64 = 0x0;

/// DMA arena length = 4 GiB per OIP-014 § S1 default.
const DMA_LEN_4_GIB: u64 = 0x1_0000_0000;

/// DMA direction = bidirectional (NVMe reads + writes share the arena).
const DMA_DIR_BIDIR: u64 = 2;

/// Placeholder IRQ line for the NVMe admin completion queue MSI-X vector.
const IRQ_LINE_NVME_ACQ: u64 = 34;

/// Placeholder IPC channel ID the kernel signals on this IRQ vector.
const IPC_CHANNEL_PLACEHOLDER: u64 = 0;

// =============================================================================
// BLK channel constants (P6.7.10-pre.4, OIP-Driver-NVMe-014 § S4)
// =============================================================================

/// Disk slot identifier for the single Phase-1 NVMe controller. Matches
/// the canonical channel name `omni.svc.blk.nvme0` that
/// `crates/omni-kernel/src/services/blk.rs` pre-builds at registration
/// time. The byte slice avoids the heap because this binary cannot
/// allocate (`PanicOnAlloc` global allocator).
const NVME_DISK_SLOT: &[u8] = b"nvme0";

/// BLK channel queue depth. OIP-Driver-NVMe-014 § S6 step 12 freezes
/// the value at 1024 — generous for a single-namespace bring-up and
/// matched by the kernel's per-channel `Vec` reserve.
const BLK_CHANNEL_QUEUE_DEPTH: u64 = 1024;

/// `BackpressurePolicy::Block` — the producer parks on a full queue.
/// Matches `OIP-Driver-NVMe-014` § S4 (`backpressure = true`).
const BLK_CHANNEL_BACKPRESSURE_BLOCK: u64 = 0;

/// Not TEE-bound — the NVMe driver runs in the regular Ring 3 process.
const BLK_CHANNEL_TEE_NOT_BOUND: u64 = 0;

// =============================================================================
// NVMe admin queue constants (P6.7.10-pre.17, OIP-Driver-NVMe-014 § S6)
// =============================================================================

/// Admin Submission Queue depth (OIP-NVMe-014 § S1 default
/// `admin_sq_depth = 64`).
const NVME_ADMIN_SQ_DEPTH: u32 = 64;

/// Admin Completion Queue depth (OIP-NVMe-014 § S1 default
/// `admin_cq_depth = 64`).
const NVME_ADMIN_CQ_DEPTH: u32 = 64;

/// IOVA offset (inside the 4 GiB DMA arena) of the Admin Submission
/// Queue data page. Page-aligned to 4 KiB per NVMe 1.4 § 3.1.9.
const NVME_ASQ_IOVA: u64 = 0x0;

/// IOVA offset of the Admin Completion Queue data page. Placed
/// 4 KiB past `NVME_ASQ_IOVA` so the two queues live in adjacent
/// 4 KiB regions of the DMA arena.
const NVME_ACQ_IOVA: u64 = 0x1000;

/// Poll budget for the `CSTS.RDY` enable/disable transitions. NVMe
/// 1.4 § 3.1.6 says the controller MUST respond within `CAP.TO`
/// 500 ms units; QEMU virtualised NVMe responds within
/// microseconds, so `10_000` iterations is generously above any
/// realistic latency.
const NVME_CSTS_POLL_LIMIT: u32 = 10_000;

// =============================================================================
// LiveMmioBackend — `MmioBackend` + `MmioReadBackend` impl for the
// live driver (P6.7.10-pre.17)
// =============================================================================

/// Thin newtype wrapping the BAR0 user-VA the kernel returned from
/// `MmioMap`. Implements [`MmioBackend`] (volatile_write) and
/// [`MmioReadBackend`] (volatile_read) so the helpers landed in
/// P6.7.10-pre.11..16 drive the live controller without any
/// shared mutable state.
///
/// The struct is `Copy` so the driver can create two independent
/// instances (one passed as the read backend, one as the write
/// backend) to satisfy the two-mutable-reference signature of
/// `disable_controller`/`enable_controller`. No state is held, so
/// the duplication is zero-cost.
#[derive(Clone, Copy)]
struct LiveMmioBackend {
    mmio_va_base: u64,
}

impl MmioBackend for LiveMmioBackend {
    #[inline]
    fn write_doorbell(&mut self, offset: usize, value: u32) {
        // SAFETY: `mmio_va_base + offset` is inside the BAR0 region
        // the kernel mapped via MmioMap; the controller register
        // file is at least `CONTROLLER_REGISTER_REGION_BYTES` long,
        // and OIP-014 § S2.2 step 2 marked the region uncached so
        // the volatile_write reaches the hardware directly.
        unsafe {
            let ptr = (self.mmio_va_base as usize + offset) as *mut u32;
            ptr.write_volatile(value);
        }
    }
}

impl MmioReadBackend for LiveMmioBackend {
    #[inline]
    fn read_register(&mut self, offset: usize) -> u32 {
        // SAFETY: same as `write_doorbell` — region is uncached and
        // owned by the kernel mapping; 32-bit aligned reads are
        // mandated by NVMe 1.4 § 3.0.
        unsafe {
            let ptr = (self.mmio_va_base as usize + offset) as *const u32;
            ptr.read_volatile()
        }
    }
}

// =============================================================================
// TaskExit sentinel codes (mirror the virtio-net image)
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
/// `IpcCreateChannel` returned `u64::MAX` — the legacy MB12 fast-path
/// could not allocate the BLK channel. Distinct from the errno-based
/// sentinels above so triage can tell "channel alloc failed" from
/// "syscall errno N".
const EXIT_IPC_CREATE_FAILED: u64 = 100;
/// Base sentinel: `BlkRegister` returned a non-zero errno. The exit
/// code = `EXIT_BLK_REGISTER_BASE + errno`. POSIX-aligned errnos the
/// kernel surfaces here: `EINVAL = 22` (disk-slot argument shape),
/// `EEXIST = 17` (slot already taken — another driver got there first),
/// `ENOSPC = 28` (registry capacity), `EACCES = 13` (caller does not
/// own the supplied channel id), `EIO = 5` (defensive internal).
const EXIT_BLK_REGISTER_BASE: u64 = 110;
/// `BlkLookup` returned `ENOENT` (`rdx = 2`). Reachable only if the
/// preceding `BlkRegister` silently dropped the entry — defensive
/// sentinel that should never fire in practice.
const EXIT_BLK_LOOKUP_NOT_FOUND: u64 = 131;
/// `BlkLookup` returned a `channel_id` distinct from the one we
/// registered. Reachable only if the kernel registry's
/// `lookup_disk_slot` regressed; treated as a hard failure because
/// the filesystem service would otherwise dispatch BLK requests to
/// the wrong driver.
const EXIT_BLK_LOOKUP_MISMATCH: u64 = 132;
/// `disable_controller` failed (controller did not clear `CSTS.RDY`
/// within the poll budget; see `omni_driver_nvme::queue::QueueError`).
const EXIT_NVME_DISABLE_TIMEOUT: u64 = 200;
/// `program_admin_queue_bases` rejected the depths or base
/// addresses (`AdminDepthOutOfRange` / `QueueBaseMisaligned`).
const EXIT_NVME_ADMIN_QUEUE_INVALID: u64 = 210;
/// `enable_controller` failed (controller did not set `CSTS.RDY`
/// within the poll budget).
const EXIT_NVME_ENABLE_TIMEOUT: u64 = 220;

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

    // Step 2 — `MmioMap (70)`: install the NVMe CSR window (16 KiB).
    let (mmio_va, mmio_errno) = unsafe {
        syscall5(
            SYS_MMIO_MAP,
            NVME_BAR0_PHYS_BASE,
            NVME_BAR0_LEN,
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

    // Step 4 — `IrqAttach (72)`: bind the admin CQ MSI-X vector.
    // ABI: a0 = irq_line, a1 = ipc_channel_id, a2/a3 = cap_ptr/cap_len.
    let (_irq_vec, irq_errno) = unsafe {
        syscall5(
            SYS_IRQ_ATTACH,
            IRQ_LINE_NVME_ACQ,
            IPC_CHANNEL_PLACEHOLDER,
            irq_token.as_ptr() as u64,
            irq_token.len() as u64,
            0,
        )
    };
    if irq_errno != 0 {
        unsafe { sys_exit(EXIT_IRQ_BASE + irq_errno) };
    }

    // Step 4.5 — `IpcCreateChannel (20)`: allocate the kernel-side
    // BLK channel the future filesystem service will attach to. The
    // legacy MB12 fast-path (`send_token_ptr = recv_token_ptr = 0`)
    // returns the channel id in `rax` without requiring a signed
    // capability token; the kernel records the caller as the
    // channel's `owner`, which is exactly the identity
    // `BlkRegister` checks against.
    let (channel_id, _ipc_extra) = unsafe {
        syscall5(
            SYS_IPC_CREATE_CHANNEL,
            BLK_CHANNEL_QUEUE_DEPTH,
            BLK_CHANNEL_BACKPRESSURE_BLOCK,
            BLK_CHANNEL_TEE_NOT_BOUND,
            0,
            0,
        )
    };
    if channel_id == u64::MAX {
        unsafe { sys_exit(EXIT_IPC_CREATE_FAILED) };
    }

    // Step 4.6 — `BlkRegister (76)`: record the
    // `omni.svc.blk.nvme0` → `channel_id` mapping in the kernel BLK
    // registry per OIP-Driver-NVMe-014 § S4 + § S6 step 12. The
    // kernel verifies the caller owns `channel_id` (we just created
    // it above, so the ownership check passes by construction); on
    // success the consumer side can resolve the channel via
    // `BlkLookup (78)`.
    let (_blk_register_rax, blk_register_errno) = unsafe {
        syscall5(
            SYS_BLK_REGISTER,
            NVME_DISK_SLOT.as_ptr() as u64,
            NVME_DISK_SLOT.len() as u64,
            channel_id,
            0,
            0,
        )
    };
    if blk_register_errno != 0 {
        unsafe { sys_exit(EXIT_BLK_REGISTER_BASE + blk_register_errno) };
    }

    // Step 4.7 — `BlkLookup (78)`: defence-in-depth round-trip. If
    // the lookup returns a different channel id (or `ENOENT`) then
    // the kernel registry regressed between insert and read and we
    // abort before any FSM advance — the filesystem service would
    // otherwise route requests to the wrong driver. Reachable only
    // on a kernel bug; sentinel exit codes make the failure easy to
    // grep on the serial log.
    let (looked_up_id, blk_lookup_errno) = unsafe {
        syscall5(
            SYS_BLK_LOOKUP,
            NVME_DISK_SLOT.as_ptr() as u64,
            NVME_DISK_SLOT.len() as u64,
            0,
            0,
            0,
        )
    };
    if blk_lookup_errno != 0 {
        unsafe { sys_exit(EXIT_BLK_LOOKUP_NOT_FOUND) };
    }
    if looked_up_id != channel_id {
        unsafe { sys_exit(EXIT_BLK_LOOKUP_MISMATCH) };
    }

    // Step 4.8 — P6.7.10-pre.17: construct the live MMIO backend
    // pair against the BAR0 user-VA the kernel returned at step 2.
    // Two zero-sized clones satisfy the two-mutable-reference
    // signature of `disable_controller`/`enable_controller`
    // without aliasing — `LiveMmioBackend` holds no state beyond
    // the `mmio_va_base` field (copied by value).
    let mut mmio_write = LiveMmioBackend { mmio_va_base: mmio_va };
    let mut mmio_read = LiveMmioBackend { mmio_va_base: mmio_va };

    // Step 4.9 — `disable_controller`: read CC, clear EN bit, write
    // CC back, poll `CSTS.RDY = 0`. Per OIP-Driver-NVMe-014 § S6
    // step 4 the driver MUST disable the controller before
    // programming AQA / ASQ / ACQ.
    if disable_controller(&mut mmio_write, &mut mmio_read, NVME_CSTS_POLL_LIMIT).is_err() {
        unsafe { sys_exit(EXIT_NVME_DISABLE_TIMEOUT) };
    }

    // Step 4.10 — `program_admin_queue_bases`: write AQA + ASQ +
    // ACQ per NVMe 1.4 § 3.1.7-9. The ASQ + ACQ data pages live
    // at the head of the DMA arena (4 KiB-aligned by construction
    // because the DMA arena base is at IOVA 0 and the pages are
    // 4 KiB-multiple offsets).
    if program_admin_queue_bases(
        &mut mmio_write,
        NVME_ASQ_IOVA,
        NVME_ACQ_IOVA,
        NVME_ADMIN_SQ_DEPTH,
        NVME_ADMIN_CQ_DEPTH,
    )
    .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_ADMIN_QUEUE_INVALID) };
    }

    // Step 4.11 — `enable_controller`: set CC.EN, poll
    // `CSTS.RDY = 1`. OIP-Driver-NVMe-014 § S6 step 6.
    if enable_controller(&mut mmio_write, &mut mmio_read, NVME_CSTS_POLL_LIMIT).is_err() {
        unsafe { sys_exit(EXIT_NVME_ENABLE_TIMEOUT) };
    }

    // Step 5 — Drive the 13-step bring-up FSM through its remaining
    // pure-state phases. With MMIO + DMA + IRQ + BLK + Admin queue
    // pair installed, the FSM can reach `Phase::Ready` via
    // repeated `Event::Advance`.
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
