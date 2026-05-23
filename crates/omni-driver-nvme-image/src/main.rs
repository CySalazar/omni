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
//! 11. **P6.7.10-pre.32** — `program_cc_fields` writes the canonical
//!     `CC` initialisation register (`MPS = 0`, `IOSQES = 6`,
//!     `IOCQES = 4`, `CSS = 0`, `AMS = 0`, `EN = 0`) per NVMe 1.4
//!     § 3.1.5 between `program_admin_queue_bases` and
//!     `enable_controller`. Required so the controller observes the
//!     command-set and queue-entry-size fields BEFORE the `EN`
//!     transition latches them.
//! 12. **P6.7.10-pre.32** — `check_controller_fatal` reads `CSTS`
//!     once immediately after `enable_controller` returns and aborts
//!     the bring-up if `CSTS.CFS = 1` per NVMe 1.4 § 3.1.6. This
//!     tripwire catches the rare case of a controller that enters
//!     the fatal-status state mid-enable handshake but still sets
//!     `CSTS.RDY` before crashing.
//! 13. **P6.7.10-pre.33** — `AdminQueuePair::new` constructs the
//!     alloc-free admin queue pair (SqRing + CqRing + dstrd). The
//!     SQ + CQ data pages live in the DMA arena at the IOVAs the
//!     controller was programmed with in step 4.10; the image
//!     accesses them via `&mut [u8]` / `&[u8]` slices over the
//!     IOVA pointers (passthrough IOMMU → user-VA == IOVA).
//! 14. **P6.7.10-pre.33** — `encode_identify(IdentifyTarget::Controller, …)`
//!     builds the SQE, `AdminQueuePair::submit` enqueues it and
//!     rings the SQ tail doorbell. The image then polls
//!     `AdminQueuePair::drain_completion` for the matching CID
//!     with a bounded `NVME_IDENTIFY_POLL_LIMIT` budget,
//!     validates `is_success()` on the resulting `AdminCqeFields`,
//!     and bails with a distinct sentinel on timeout / non-success
//!     / drain failure. This is the first real admin command the
//!     live image issues end-to-end via syscalls (vs the synthetic
//!     `Event::Advance` ladder the FSM walks in step 16).
//! 15. **P6.7.10-pre.34** — `encode_identify(IdentifyTarget::ActiveNsList, …)`
//!     submits the second admin command. The 4 KiB response page
//!     at `NVME_IDENTIFY_NS_LIST_RESP_IOVA` is parsed alloc-free
//!     via [`ActiveNsListView::new`] + `first_active_nsid()`.
//!     An empty list (controller reports zero active namespaces)
//!     is treated as a hard failure (`EXIT_NVME_NS_LIST_EMPTY`)
//!     because the subsequent `Identify(Namespace)` step has no
//!     NSID to seed.
//! 16. **P6.7.10-pre.35** — `encode_identify(IdentifyTarget::Namespace { nsid }, …)`
//!     submits the third admin command for the first active NSID
//!     discovered in step 15. The 4 KiB response page at
//!     `NVME_IDENTIFY_NS_RESP_IOVA` is parsed via
//!     [`IdentifyNamespace::new`] + [`IdentifyNamespace::validated_byte_size`];
//!     the driver aborts with `EXIT_NVME_NS_UNSUPPORTED_LBADS`
//!     when `LBADS != 12` (sector size not 4 KiB) per OIP-014
//!     § S6 step 10.
//! 17. **P6.7.10-pre.36** — `encode_create_io_cq` + `encode_create_io_sq`
//!     submit the fourth and fifth admin commands to create the
//!     IO queue pair (QID 1, depth 64) per NVMe 1.4 §§ 5.3–5.4.
//!     The IO CQ at `NVME_IO_CQ_IOVA` must be created before the
//!     IO SQ at `NVME_IO_SQ_IOVA` because § 5.4 requires the CQ
//!     to already exist when the SQ references it.
//! 18. **P6.7.10-pre.37** — `encode_read(first_nsid, lba=0, 1, …)`
//!     submits the first NVM Read through the IO queue pair. The
//!     4 KiB data buffer at `NVME_IO_READ_DATA_IOVA` receives one
//!     sector from LBA 0. This is the first IO command (non-admin)
//!     the live image issues end-to-end.
//! 19. **P6.7.10-pre.38** — `encode_write(first_nsid, lba=0, 1, …)`
//!     writes the data buffer back to LBA 0, validating the NVM
//!     Write path through the IO queue pair.
//! 20. **P6.7.10-pre.39** — `encode_flush(first_nsid, …)` commits
//!     the volatile write cache.
//! 21. **P6.7.10-pre.40** — `write_single_discard_range` + `encode_discard`
//!     submits a Dataset Management (Deallocate) command for LBA 0.
//!     Fourth and final IO command type, completing the full BLK
//!     request set.
//! 22. Drive the [`omni_driver_nvme::bringup::BringUp`] 13-step FSM until
//!     `Phase::Ready` (or any terminal `Failed` state).
//! 23. `TaskExit(0)` on success / non-zero sentinel on any failure.
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

use omni_driver_nvme::admin::{
    CIOSQ_QPRIO_MEDIUM, IdentifyTarget, encode_create_io_cq, encode_create_io_sq,
    encode_identify,
};
use omni_driver_nvme::bringup::{BringUp, Event, Phase};
use omni_driver_nvme::identify::{ActiveNsListView, IdentifyNamespace};
use omni_driver_nvme::discard::write_single_discard_range;
use omni_driver_nvme::io::{encode_discard, encode_flush, encode_read, encode_write};
use omni_driver_nvme::queue::{
    AdminQueuePair, MmioBackend, MmioReadBackend, PHASE_1_IOCQES_LOG2, PHASE_1_IOSQES_LOG2,
    PHASE_1_MPS_LOG2, check_controller_fatal, disable_controller, enable_controller,
    program_admin_queue_bases, program_cc_fields,
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

/// Admin doorbell stride (`CAP.DSTRD` field). Phase-1 pins the
/// expected value to `0` (4-byte stride) per NVMe 1.4 § 3.1.1
/// the most common controller default. A future slice will read
/// `CAP.DSTRD` from BAR0 and propagate it dynamically; for
/// `omni-driver-nvme-image` the static value matches both the
/// QEMU virtualised NVMe and every commercial controller's
/// default. P6.7.10-pre.33.
const NVME_ADMIN_DSTRD_DEFAULT: u8 = 0;

/// Backing-page size of the admin SQ + admin CQ in the DMA arena.
/// 64 SQEs × 64 bytes = 4096; 64 CQEs × 16 bytes = 1024. Both
/// queues live inside a single 4 KiB physical page so the IOVAs
/// satisfy the NVMe 1.4 § 3.1.9 page-alignment requirement and
/// the per-queue `&mut [u8]` accessor spans exactly one page.
const NVME_ADMIN_QUEUE_PAGE_BYTES: usize = 4096;

/// IOVA offset (inside the DMA arena) of the response page the
/// controller writes the Identify Controller response into.
/// Placed at offset `0x2000` so it lives in the third 4 KiB page
/// after the ASQ (offset `0x0`) and the ACQ (offset `0x1000`).
/// 4 KiB-aligned by construction per NVMe 1.4 § 5.15 (Identify
/// response is exactly 4 KiB; PRP1 alone covers it; PRP2 is
/// zero). The response itself is not yet parsed by the image —
/// a future slice will use [`omni_driver_nvme::identify::ControllerView`]
/// against this page. P6.7.10-pre.33.
const NVME_IDENTIFY_CTRL_RESP_IOVA: u64 = 0x2000;

/// Poll budget for the Identify Controller completion. Each
/// iteration is a single CSTS-equivalent read of the CQ slot
/// header; QEMU virtualised NVMe completes Identify within tens
/// of microseconds, so `50_000` iterations is generously above
/// any realistic admin-command latency. P6.7.10-pre.33.
const NVME_IDENTIFY_POLL_LIMIT: u32 = 50_000;

/// First CID the image hands out for the Identify Controller
/// command. CID `0` is reserved by `omni_types::nvme` (the
/// `RESERVED_DRIVER_OPAQUE_ID`), so the image starts at `1`
/// — matching the `AdminSession::allocate_cid` skip-on-wrap
/// policy that the host-side reference implementation uses.
const NVME_IDENTIFY_FIRST_CID: u16 = 1;

/// IOVA offset (inside the DMA arena) of the response page the
/// controller writes the `Identify(ActiveNsList)` response into.
/// Placed at offset `0x3000` so it lives in the fourth 4 KiB page
/// after the ASQ (`0x0`), the ACQ (`0x1000`), and the Identify
/// Controller response (`0x2000`). 4 KiB-aligned by construction
/// per NVMe 1.4 § 5.15 (Active Namespace ID list response is
/// exactly 4 KiB; PRP1 alone covers it; PRP2 is zero). The page
/// is parsed by [`ActiveNsListView::new`] in
/// step 4.16.d. P6.7.10-pre.34.
const NVME_IDENTIFY_NS_LIST_RESP_IOVA: u64 = 0x3000;

/// CID the image hands out for the Identify(ActiveNsList)
/// command — `NVME_IDENTIFY_FIRST_CID + 1`. The Phase-1 bring-up
/// issues admin commands strictly serially, so the CID counter
/// is a simple `+1` for each new command; reusing `submit_identify`
/// (the alloc-bound host-side helper) is not possible here because
/// the image runs under `PanicOnAlloc`.
const NVME_IDENTIFY_NS_LIST_CID: u16 = 2;

/// Poll budget for the Identify(ActiveNsList) completion. Same
/// rationale as [`NVME_IDENTIFY_POLL_LIMIT`]: QEMU virtualised
/// NVMe completes admin commands within tens of microseconds,
/// `50_000` iterations is well above any realistic latency.
/// New in P6.7.10-pre.34.
const NVME_IDENTIFY_NS_LIST_POLL_LIMIT: u32 = 50_000;

/// Backing-page size of the `Identify(ActiveNsList)` response —
/// exactly 4 KiB per NVMe 1.4 § 5.15.2 Figure 246. Matches
/// `omni_driver_nvme::identify::IDENTIFY_RESPONSE_BYTES`; pinned
/// locally so the slice construction is alloc-free.
/// New in P6.7.10-pre.34.
const NVME_IDENTIFY_NS_LIST_RESP_BYTES: usize = 4096;

/// IOVA offset (inside the DMA arena) of the response page the
/// controller writes the `Identify(Namespace)` response into.
/// Placed at offset `0x4000` so it lives in the fifth 4 KiB page
/// after the ASQ (`0x0`), ACQ (`0x1000`), Identify Controller
/// response (`0x2000`), and Active Namespace List response
/// (`0x3000`). 4 KiB-aligned by construction per NVMe 1.4 § 5.15
/// (Identify response is exactly 4 KiB; PRP1 alone covers it;
/// PRP2 is zero). New in P6.7.10-pre.35.
const NVME_IDENTIFY_NS_RESP_IOVA: u64 = 0x4000;

/// CID the image hands out for the `Identify(Namespace)` command
/// — `NVME_IDENTIFY_NS_LIST_CID + 1 = 3`. Third admin command in
/// the serial bring-up sequence. New in P6.7.10-pre.35.
const NVME_IDENTIFY_NS_CID: u16 = 3;

/// Poll budget for the `Identify(Namespace)` completion. Same
/// rationale as [`NVME_IDENTIFY_POLL_LIMIT`]: QEMU virtualised
/// NVMe completes admin commands within tens of microseconds.
/// New in P6.7.10-pre.35.
const NVME_IDENTIFY_NS_POLL_LIMIT: u32 = 50_000;

/// Backing-page size of the `Identify(Namespace)` response —
/// exactly 4 KiB per NVMe 1.4 § 5.15.2 Figure 245. New in
/// P6.7.10-pre.35.
const NVME_IDENTIFY_NS_RESP_BYTES: usize = 4096;

// =============================================================================
// IO queue creation constants (P6.7.10-pre.36, OIP-Driver-NVMe-014 § R2)
// =============================================================================

/// IOVA offset (inside the DMA arena) of the IO Completion Queue data
/// page. Placed at offset `0x5000` so it lives in the sixth 4 KiB
/// page after the ASQ (`0x0`), ACQ (`0x1000`), Identify Controller
/// response (`0x2000`), Active Namespace List response (`0x3000`),
/// and Identify Namespace response (`0x4000`). New in P6.7.10-pre.36.
const NVME_IO_CQ_IOVA: u64 = 0x5000;

/// IOVA offset of the IO Submission Queue data page. Placed at
/// `0x6000` (seventh 4 KiB page). New in P6.7.10-pre.36.
const NVME_IO_SQ_IOVA: u64 = 0x6000;

/// IO queue depth for both the IO CQ and IO SQ. Phase-1 pins this
/// to 64 entries per OIP-Driver-NVMe-014 § R2 (matches the admin
/// queue depth for simplicity; production drivers may use up to
/// 65535). New in P6.7.10-pre.36.
const NVME_IO_QUEUE_DEPTH: u16 = 64;

/// IO CQ/SQ Queue Identifier — Phase-1 creates exactly one IO queue
/// pair with QID 1 per OIP-014 § R5. New in P6.7.10-pre.36.
const NVME_IO_QID: u16 = 1;

/// MSI-X interrupt vector the IO CQ completions signal on. Phase-1
/// uses vector 0 (shared with the admin CQ); a future multi-queue
/// slice will assign distinct vectors per IO CQ. New in P6.7.10-pre.36.
const NVME_IO_CQ_IRQ_VECTOR: u16 = 0;

/// CID for the `Create I/O Completion Queue` admin command —
/// `NVME_IDENTIFY_NS_CID + 1 = 4`. Fourth admin command in the
/// serial bring-up sequence. New in P6.7.10-pre.36.
const NVME_CREATE_IO_CQ_CID: u16 = 4;

/// CID for the `Create I/O Submission Queue` admin command —
/// `NVME_CREATE_IO_CQ_CID + 1 = 5`. Fifth admin command. New in
/// P6.7.10-pre.36.
const NVME_CREATE_IO_SQ_CID: u16 = 5;

/// Poll budget for the IO queue creation completions. Same rationale
/// as [`NVME_IDENTIFY_POLL_LIMIT`]. New in P6.7.10-pre.36.
const NVME_CREATE_IO_POLL_LIMIT: u32 = 50_000;

// =============================================================================
// IO read constants (P6.7.10-pre.37, OIP-Driver-NVMe-014 § S6 step 11)
// =============================================================================

/// IOVA offset (inside the DMA arena) of the data buffer the
/// controller writes the NVM Read response into. Placed at `0x7000`
/// (eighth 4 KiB page). New in P6.7.10-pre.37.
const NVME_IO_READ_DATA_IOVA: u64 = 0x7000;

/// IO read data page size — exactly 4 KiB (one sector at LBADS=12).
/// New in P6.7.10-pre.37.
const NVME_IO_READ_DATA_BYTES: usize = 4096;

/// First CID the IO queue pair uses for IO commands. CID 0 is
/// reserved by `omni_types::nvme::RESERVED_DRIVER_OPAQUE_ID`, so
/// the IO path starts at 1 — independent of the admin queue CID
/// counter. New in P6.7.10-pre.37.
const NVME_IO_READ_CID: u16 = 1;

/// Poll budget for the IO read completion. Same rationale as
/// [`NVME_IDENTIFY_POLL_LIMIT`]. New in P6.7.10-pre.37.
const NVME_IO_READ_POLL_LIMIT: u32 = 50_000;

// =============================================================================
// IO write constants (P6.7.10-pre.38, OIP-Driver-NVMe-014 § S6 step 11)
// =============================================================================

/// CID for the NVM Write command. Second IO command in the
/// serial bring-up sequence. New in P6.7.10-pre.38.
const NVME_IO_WRITE_CID: u16 = 2;

/// Poll budget for the IO write completion. New in P6.7.10-pre.38.
const NVME_IO_WRITE_POLL_LIMIT: u32 = 50_000;

// =============================================================================
// IO flush constants (P6.7.10-pre.39)
// =============================================================================

/// CID for the NVM Flush command. Third IO command. New in
/// P6.7.10-pre.39.
const NVME_IO_FLUSH_CID: u16 = 3;

/// Poll budget for the IO flush completion. New in P6.7.10-pre.39.
const NVME_IO_FLUSH_POLL_LIMIT: u32 = 50_000;

// =============================================================================
// IO discard constants (P6.7.10-pre.40)
// =============================================================================

/// IOVA offset (inside the DMA arena) of the 16-byte Range
/// Descriptor buffer the NVM Dataset Management command reads.
/// Placed at `0x8000` (ninth 4 KiB page). New in P6.7.10-pre.40.
const NVME_IO_DISCARD_RANGE_IOVA: u64 = 0x8000;

/// CID for the NVM Dataset Management (Discard) command. Fourth IO
/// command in the serial bring-up sequence. New in P6.7.10-pre.40.
const NVME_IO_DISCARD_CID: u16 = 4;

/// Poll budget for the IO discard completion. New in P6.7.10-pre.40.
const NVME_IO_DISCARD_POLL_LIMIT: u32 = 50_000;

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
/// `program_cc_fields` rejected one of `MPS` / `IOSQES` / `IOCQES`
/// for being outside the 4-bit range per NVMe 1.4 § 3.1.5 — surfaces
/// as `QueueError::AdminDepthOutOfRange`. Reachable only if the
/// Phase-1 constants are corrupted at compile time (the image pins
/// them to spec-mandated values) so this sentinel is defensive
/// against a regression of [`omni_driver_nvme::queue::PHASE_1_MPS_LOG2`]
/// / `PHASE_1_IOSQES_LOG2` / `PHASE_1_IOCQES_LOG2`. New in P6.7.10-pre.32.
const EXIT_NVME_CC_FIELDS_INVALID: u64 = 215;
/// `enable_controller` failed (controller did not set `CSTS.RDY`
/// within the poll budget).
const EXIT_NVME_ENABLE_TIMEOUT: u64 = 220;
/// `check_controller_fatal` returned `true` immediately after
/// `enable_controller` succeeded — the controller set `CSTS.RDY`
/// but also raised `CSTS.CFS` (Controller Fatal Status, sticky per
/// NVMe 1.4 § 3.1.6). The bring-up MUST abort because subsequent
/// admin commands would never complete. Reachable when a flaky
/// controller crashes mid-enable but still ticks the RDY bit.
/// New in P6.7.10-pre.32.
const EXIT_NVME_CONTROLLER_FATAL: u64 = 225;
/// `AdminQueuePair::new` rejected the bring-up SQ/CQ depths or
/// the doorbell stride. Reachable only if the Phase-1 admin queue
/// constants are corrupted at compile time; defensive sentinel
/// against a regression of [`NVME_ADMIN_SQ_DEPTH`] /
/// [`NVME_ADMIN_CQ_DEPTH`]. New in P6.7.10-pre.33.
const EXIT_NVME_ADMIN_PAIR_INVALID: u64 = 230;
/// `AdminQueuePair::submit` failed to enqueue the Identify
/// Controller SQE — either the SQ ring is full (impossible at
/// this stage; the ring starts empty) or the SQ data page is
/// undersized. Defensive. New in P6.7.10-pre.33.
const EXIT_NVME_IDENTIFY_SUBMIT_FAILED: u64 = 235;
/// The Identify Controller poll loop exhausted
/// [`NVME_IDENTIFY_POLL_LIMIT`] iterations without observing a
/// matching CQE. Reachable on a controller that NACKs admin
/// commands silently, or a DMA arena mis-programming that
/// prevents the controller from writing the CQ slot. New in
/// P6.7.10-pre.33.
const EXIT_NVME_IDENTIFY_TIMEOUT: u64 = 240;
/// `AdminQueuePair::drain_completion` surfaced a non-timeout
/// error (`CqPageTooSmall` / `DoorbellOffsetOverflow`).
/// Defensive against a regression of the page-size or
/// doorbell-stride constants. New in P6.7.10-pre.33.
const EXIT_NVME_IDENTIFY_DRAIN_FAILED: u64 = 242;
/// Identify Controller completed but the CQE reports a
/// non-success status word (`SCT != 0` or `SC != 0`). The
/// controller actively refused the command — either CDW10/11
/// shape is wrong or the controller has a serious firmware
/// issue. New in P6.7.10-pre.33.
const EXIT_NVME_IDENTIFY_FAILED: u64 = 245;
/// `AdminQueuePair::submit` failed to enqueue the
/// Identify(ActiveNsList) SQE on the second admin slot. Mirrors
/// `EXIT_NVME_IDENTIFY_SUBMIT_FAILED` semantically; distinct
/// sentinel so serial-log triage can tell which command in the
/// bring-up handshake regressed. New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_SUBMIT_FAILED: u64 = 250;
/// The Identify(ActiveNsList) poll loop exhausted
/// [`NVME_IDENTIFY_NS_LIST_POLL_LIMIT`] iterations without
/// observing a matching CQE. Same root-cause space as
/// `EXIT_NVME_IDENTIFY_TIMEOUT`: silent NACK or DMA-arena
/// mis-programming, scoped to the second admin command.
/// New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_TIMEOUT: u64 = 252;
/// `AdminQueuePair::drain_completion` surfaced a non-timeout
/// error while polling the Identify(ActiveNsList) completion.
/// Defensive against a regression of the page-size or
/// doorbell-stride constants. New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_DRAIN_FAILED: u64 = 254;
/// Identify(ActiveNsList) completed but the CQE reports a
/// non-success status word. The controller actively refused the
/// command — either CDW10/11 shape is wrong (CNS = 0x02 must be
/// honoured by any 1.4-compliant controller) or the controller
/// has a serious firmware issue. New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_FAILED: u64 = 256;
/// [`ActiveNsListView::new`] returned `IdentifyError::PageTooSmall`.
/// Reachable only if the local slice constructor mis-computes the
/// response-page length; the IOVA region the image hands to the
/// controller is sized exactly [`NVME_IDENTIFY_NS_LIST_RESP_BYTES`]
/// (4 KiB) by construction, so this sentinel is purely defensive
/// against a regression of the constant. New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_PARSE_FAILED: u64 = 258;
/// The Active Namespace List parse succeeded but
/// [`ActiveNsListView::first_active_nsid`] returned `None`:
/// the controller reports zero active namespaces, which makes
/// the subsequent `Identify(Namespace)` step impossible. NVMe
/// 1.4 § 5.15.2 permits a controller to expose zero namespaces
/// only as a transient post-format state; reaching this branch
/// during bring-up is a hard failure — the kernel BLK gateway
/// has no namespace to publish. New in P6.7.10-pre.34.
const EXIT_NVME_NS_LIST_EMPTY: u64 = 260;
/// `AdminQueuePair::submit` failed to enqueue the
/// `Identify(Namespace)` SQE on the third admin slot. Mirrors
/// `EXIT_NVME_IDENTIFY_SUBMIT_FAILED` semantically; distinct
/// sentinel so serial-log triage can localise which command in
/// the bring-up handshake regressed. New in P6.7.10-pre.35.
const EXIT_NVME_NS_SUBMIT_FAILED: u64 = 270;
/// The `Identify(Namespace)` poll loop exhausted
/// [`NVME_IDENTIFY_NS_POLL_LIMIT`] iterations without observing
/// a matching CQE. Same root-cause space as
/// `EXIT_NVME_IDENTIFY_TIMEOUT`. New in P6.7.10-pre.35.
const EXIT_NVME_NS_TIMEOUT: u64 = 272;
/// `AdminQueuePair::drain_completion` surfaced a non-timeout
/// error while polling the `Identify(Namespace)` completion.
/// New in P6.7.10-pre.35.
const EXIT_NVME_NS_DRAIN_FAILED: u64 = 274;
/// `Identify(Namespace)` completed but the CQE reports a
/// non-success status word. The controller actively refused the
/// command. New in P6.7.10-pre.35.
const EXIT_NVME_NS_FAILED: u64 = 276;
/// [`IdentifyNamespace::new`] returned
/// `IdentifyError::PageTooSmall`. Purely defensive against a
/// regression of the response-page length constant. New in
/// P6.7.10-pre.35.
const EXIT_NVME_NS_PARSE_FAILED: u64 = 278;
/// [`IdentifyNamespace::validated_byte_size`] returned
/// `IdentifyError::UnsupportedLbads` — the controller's active
/// LBA format does not use 4 KiB sectors (`LBADS != 12`). Per
/// OIP-014 § S6 step 10 the Phase-1 driver rejects any
/// namespace whose sector size differs from the kernel page
/// size. New in P6.7.10-pre.35.
const EXIT_NVME_NS_UNSUPPORTED_LBADS: u64 = 280;
/// `AdminQueuePair::submit` failed to enqueue the `Create I/O
/// Completion Queue` SQE. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_CQ_SUBMIT_FAILED: u64 = 290;
/// The `Create I/O Completion Queue` poll loop exhausted
/// [`NVME_CREATE_IO_POLL_LIMIT`]. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_CQ_TIMEOUT: u64 = 292;
/// `drain_completion` surfaced a non-timeout error while
/// polling the `Create I/O CQ` completion. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_CQ_DRAIN_FAILED: u64 = 294;
/// `Create I/O Completion Queue` completed but the CQE reports a
/// non-success status word. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_CQ_FAILED: u64 = 296;
/// `AdminQueuePair::submit` failed to enqueue the `Create I/O
/// Submission Queue` SQE. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_SQ_SUBMIT_FAILED: u64 = 300;
/// The `Create I/O Submission Queue` poll loop exhausted
/// [`NVME_CREATE_IO_POLL_LIMIT`]. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_SQ_TIMEOUT: u64 = 302;
/// `drain_completion` surfaced a non-timeout error while
/// polling the `Create I/O SQ` completion. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_SQ_DRAIN_FAILED: u64 = 304;
/// `Create I/O Submission Queue` completed but the CQE reports a
/// non-success status word. New in P6.7.10-pre.36.
const EXIT_NVME_CREATE_IO_SQ_FAILED: u64 = 306;
/// `AdminQueuePair::new_for_qid` rejected the IO queue pair
/// parameters. Defensive. New in P6.7.10-pre.37.
const EXIT_NVME_IO_PAIR_INVALID: u64 = 310;
/// `AdminQueuePair::submit` failed to enqueue the NVM Read SQE
/// on the IO SQ. New in P6.7.10-pre.37.
const EXIT_NVME_IO_READ_SUBMIT_FAILED: u64 = 320;
/// The NVM Read poll loop exhausted [`NVME_IO_READ_POLL_LIMIT`].
/// New in P6.7.10-pre.37.
const EXIT_NVME_IO_READ_TIMEOUT: u64 = 322;
/// `drain_completion` surfaced a non-timeout error while polling
/// the NVM Read completion on the IO CQ. New in P6.7.10-pre.37.
const EXIT_NVME_IO_READ_DRAIN_FAILED: u64 = 324;
/// NVM Read completed but the CQE reports a non-success status
/// word. New in P6.7.10-pre.37.
const EXIT_NVME_IO_READ_FAILED: u64 = 326;
/// `AdminQueuePair::submit` failed to enqueue the NVM Write SQE.
/// New in P6.7.10-pre.38.
const EXIT_NVME_IO_WRITE_SUBMIT_FAILED: u64 = 330;
/// The NVM Write poll loop exhausted
/// [`NVME_IO_WRITE_POLL_LIMIT`]. New in P6.7.10-pre.38.
const EXIT_NVME_IO_WRITE_TIMEOUT: u64 = 332;
/// `drain_completion` surfaced a non-timeout error while polling
/// the NVM Write completion on the IO CQ. New in P6.7.10-pre.38.
const EXIT_NVME_IO_WRITE_DRAIN_FAILED: u64 = 334;
/// NVM Write completed but the CQE reports a non-success status
/// word. New in P6.7.10-pre.38.
const EXIT_NVME_IO_WRITE_FAILED: u64 = 336;
/// `AdminQueuePair::submit` failed to enqueue the NVM Flush SQE.
/// New in P6.7.10-pre.39.
const EXIT_NVME_IO_FLUSH_SUBMIT_FAILED: u64 = 340;
/// The NVM Flush poll loop exhausted
/// [`NVME_IO_FLUSH_POLL_LIMIT`]. New in P6.7.10-pre.39.
const EXIT_NVME_IO_FLUSH_TIMEOUT: u64 = 342;
/// `drain_completion` surfaced a non-timeout error while polling
/// the NVM Flush completion on the IO CQ. New in P6.7.10-pre.39.
const EXIT_NVME_IO_FLUSH_DRAIN_FAILED: u64 = 344;
/// NVM Flush completed but the CQE reports a non-success status
/// word. New in P6.7.10-pre.39.
const EXIT_NVME_IO_FLUSH_FAILED: u64 = 346;
/// `write_single_discard_range` failed to fill the Range
/// Descriptor buffer. New in P6.7.10-pre.40.
const EXIT_NVME_IO_DISCARD_RANGE_FAILED: u64 = 350;
/// `AdminQueuePair::submit` failed to enqueue the NVM Dataset
/// Management SQE. New in P6.7.10-pre.40.
const EXIT_NVME_IO_DISCARD_SUBMIT_FAILED: u64 = 352;
/// The NVM Discard poll loop exhausted
/// [`NVME_IO_DISCARD_POLL_LIMIT`]. New in P6.7.10-pre.40.
const EXIT_NVME_IO_DISCARD_TIMEOUT: u64 = 354;
/// `drain_completion` surfaced a non-timeout error while polling
/// the NVM Discard completion. New in P6.7.10-pre.40.
const EXIT_NVME_IO_DISCARD_DRAIN_FAILED: u64 = 356;
/// NVM Discard completed but the CQE reports a non-success status
/// word. New in P6.7.10-pre.40.
const EXIT_NVME_IO_DISCARD_FAILED: u64 = 358;

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
    let mut mmio_write = LiveMmioBackend {
        mmio_va_base: mmio_va,
    };
    let mut mmio_read = LiveMmioBackend {
        mmio_va_base: mmio_va,
    };

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

    // Step 4.11 — P6.7.10-pre.32: `program_cc_fields` writes the
    // canonical CC initialisation register with `EN = 0`, packing
    // `MPS`/`IOSQES`/`IOCQES` per NVMe 1.4 § 3.1.5. Goes between
    // `program_admin_queue_bases` and `enable_controller` so the
    // controller observes the queue-entry-size and command-set
    // selections BEFORE the EN transition latches them. The
    // Phase-1 constants are spec-mandated (`MPS = 0` = 4 KiB host
    // pages, `IOSQES = 6` = 64-byte SQE, `IOCQES = 4` = 16-byte CQE)
    // and the helper rejects out-of-range values; the
    // `EXIT_NVME_CC_FIELDS_INVALID` sentinel is therefore defensive
    // against a regression of the pinned constants.
    if program_cc_fields(
        &mut mmio_write,
        PHASE_1_MPS_LOG2,
        PHASE_1_IOSQES_LOG2,
        PHASE_1_IOCQES_LOG2,
    )
    .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_CC_FIELDS_INVALID) };
    }

    // Step 4.12 — `enable_controller`: set CC.EN, poll
    // `CSTS.RDY = 1`. OIP-Driver-NVMe-014 § S6 step 6.
    if enable_controller(&mut mmio_write, &mut mmio_read, NVME_CSTS_POLL_LIMIT).is_err() {
        unsafe { sys_exit(EXIT_NVME_ENABLE_TIMEOUT) };
    }

    // Step 4.13 — P6.7.10-pre.32: `check_controller_fatal` tripwire.
    // Reads `CSTS` once and aborts the bring-up if `CSTS.CFS = 1`
    // per NVMe 1.4 § 3.1.6. Catches the corner case where the
    // controller raised both `CSTS.RDY` and `CSTS.CFS` in the same
    // register window, which `enable_controller`'s poll loop would
    // accept as success because it only checks the RDY bit. Sticky
    // CFS means any subsequent admin command would block forever;
    // bailing here surfaces the failure cleanly via the sentinel.
    if check_controller_fatal(&mut mmio_read) {
        unsafe { sys_exit(EXIT_NVME_CONTROLLER_FATAL) };
    }

    // Step 4.14 — P6.7.10-pre.33: construct the admin queue pair.
    // `AdminQueuePair::new` is alloc-free — it owns only an
    // `SqRing` + `CqRing` + the doorbell stride; the backing
    // SQ/CQ data pages live in the DMA arena and are accessed
    // via &mut [u8] slices below. Phase-1 dstrd is pinned to 0
    // (4-byte stride) per `NVME_ADMIN_DSTRD_DEFAULT`; a future
    // slice will read `CAP.DSTRD` from BAR0 and propagate it.
    let mut admin_pair = match AdminQueuePair::new(
        NVME_ADMIN_SQ_DEPTH,
        NVME_ADMIN_CQ_DEPTH,
        NVME_ADMIN_DSTRD_DEFAULT,
    ) {
        Ok(p) => p,
        Err(_) => unsafe { sys_exit(EXIT_NVME_ADMIN_PAIR_INVALID) },
    };

    // Step 4.14.b — Acquire &mut [u8] views into the DMA arena
    // pages backing the ASQ + ACQ. Phase-1 IOMMU passthrough
    // means `iova == user_va`, so the user-space pointer for
    // each IOVA equals the IOVA bit pattern reinterpreted as a
    // pointer. The kernel `DmaMap (71)` syscall installed the
    // 4 GiB arena starting at the IOVA the kernel returned in
    // `_dma_iova` (passthrough → equals our requested
    // `DMA_IOVA_BASE = 0x0`). The two slices are non-overlapping
    // by construction (`NVME_ASQ_IOVA = 0x0`,
    // `NVME_ACQ_IOVA = 0x1000`).
    //
    // SAFETY: the DMA arena was just installed by the
    // `DmaMap (71)` syscall at step 3; the kernel guarantees
    // 4 KiB-aligned, zero-initialised pages backing the IOVA
    // range. `program_admin_queue_bases` at step 4.10 wrote
    // these same IOVAs into `ASQ` + `ACQ` so the controller
    // shares the views. The lifetime of the slices ends at
    // `_start`'s `sys_exit`; no other code path holds these
    // pointers.
    let asq_slice: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(NVME_ASQ_IOVA as *mut u8, NVME_ADMIN_QUEUE_PAGE_BYTES)
    };
    let acq_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(NVME_ACQ_IOVA as *const u8, NVME_ADMIN_QUEUE_PAGE_BYTES)
    };

    // Step 4.15 — P6.7.10-pre.33: encode + submit the Identify
    // Controller SQE (NVMe 1.4 § 5.15.1). The response is a
    // 4 KiB structure the controller writes at
    // `NVME_IDENTIFY_CTRL_RESP_IOVA`. PRP2 is zero (single-page
    // response per `IDENTIFY_PRP2_ZERO`).
    let identify_sqe = encode_identify(
        IdentifyTarget::Controller,
        NVME_IDENTIFY_CTRL_RESP_IOVA,
        0,
        NVME_IDENTIFY_FIRST_CID,
    );
    if admin_pair
        .submit(&identify_sqe, &mut mmio_write, asq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_IDENTIFY_SUBMIT_FAILED) };
    }

    // Step 4.15.b — Poll the admin CQ for the matching CID. The
    // loop bounds polling to `NVME_IDENTIFY_POLL_LIMIT` so a
    // misprogrammed controller cannot wedge the bring-up
    // indefinitely. `drain_completion` returns:
    //   - `Ok(Some(fields))` when the current slot has the
    //     expected phase tag and is consumed — the loop matches
    //     on `cid` to skip any stray completion (impossible in
    //     a single-in-flight scenario but defensively coded).
    //   - `Ok(None)` when the slot's phase tag still matches the
    //     previous lap — the controller has not written yet.
    //   - `Err(_)` on a CQ-page bounds or doorbell-stride bug —
    //     reachable only on a constants regression.
    let mut polls: u32 = 0;
    let identify_cqe = loop {
        if polls >= NVME_IDENTIFY_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_IDENTIFY_TIMEOUT) };
        }
        polls = polls.saturating_add(1);
        match admin_pair.drain_completion(&mut mmio_write, acq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IDENTIFY_FIRST_CID => break fields,
            Ok(Some(_)) => {
                // Stray completion with a non-matching CID —
                // impossible in the single-in-flight Identify
                // scenario but defensively skip-and-keep-polling
                // so a future multi-command pre-amble does not
                // accidentally consume the Identify's CQE.
                continue;
            }
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_IDENTIFY_DRAIN_FAILED) },
        }
    };

    // Step 4.15.c — Validate the completion status word. NVMe 1.4
    // § 4.6 success = `SCT = 0` (Generic Command Status) AND
    // `SC = 0` (Successful Completion). Any non-zero status
    // means the controller actively refused the command — exit
    // with a distinct sentinel so the serial log triage can
    // distinguish "controller did not respond" (timeout) from
    // "controller responded but command rejected" (this case).
    if !identify_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_IDENTIFY_FAILED) };
    }

    // Step 4.16 — P6.7.10-pre.34: encode + submit the
    // Identify(ActiveNsList) SQE (NVMe 1.4 § 5.15.2). The response
    // is a 4 KiB page laid out as 1024 little-endian 32-bit NSIDs
    // (ascending, NSID = 0 sentinel terminator). PRP2 is zero
    // (single-page response). This is the second real admin
    // command the live image issues end-to-end: the first
    // (Identify Controller) already validated the queue-pair
    // plumbing in pre.33, so any failure surfaced below is
    // squarely a controller-side or DMA-arena regression scoped to
    // the ActiveNsList command.
    let ns_list_sqe = encode_identify(
        IdentifyTarget::ActiveNsList,
        NVME_IDENTIFY_NS_LIST_RESP_IOVA,
        0,
        NVME_IDENTIFY_NS_LIST_CID,
    );
    if admin_pair
        .submit(&ns_list_sqe, &mut mmio_write, asq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_NS_LIST_SUBMIT_FAILED) };
    }

    // Step 4.16.b — Poll the admin CQ for the matching CID. Same
    // structure as step 4.15.b: bounded budget, skip strays,
    // continue on `Ok(None)`, exit on `Err(_)`. The CQ slot used
    // by this completion is slot 1 (slot 0 is consumed by the
    // Identify Controller completion above; `drain_completion`
    // advanced `expected_head` internally). The phase tag at
    // slot 1 is still 1 (we are on lap 1 and CQ_DEPTH = 64), so a
    // synthetic empty page would land on the `Ok(None)` path.
    let mut ns_list_polls: u32 = 0;
    let ns_list_cqe = loop {
        if ns_list_polls >= NVME_IDENTIFY_NS_LIST_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_NS_LIST_TIMEOUT) };
        }
        ns_list_polls = ns_list_polls.saturating_add(1);
        match admin_pair.drain_completion(&mut mmio_write, acq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IDENTIFY_NS_LIST_CID => break fields,
            Ok(Some(_)) => {
                // Stray completion with a non-matching CID — same
                // defensive skip-and-keep-polling as step 4.15.b.
                continue;
            }
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_NS_LIST_DRAIN_FAILED) },
        }
    };

    // Step 4.16.c — Validate the completion status word. Same
    // semantics as step 4.15.c, with a distinct sentinel so triage
    // can localise which command in the handshake regressed.
    if !ns_list_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_NS_LIST_FAILED) };
    }

    // Step 4.16.d — Parse the 4 KiB response page via
    // [`ActiveNsListView`]. The view is alloc-free and reads the
    // page lazily on `first_active_nsid()`, so the
    // `PanicOnAlloc` global allocator stays untouched. The IOVA
    // region was zero-initialised by the kernel `DmaMap (71)`
    // syscall at step 3; the controller has just DMA-written the
    // NSID array into it. Phase-1 passthrough IOMMU means
    // `user_va == iova`, so the slice constructor reads exactly
    // the bytes the controller wrote.
    //
    // SAFETY: the DMA arena was installed by `DmaMap (71)` at
    // step 3 with `DMA_LEN_4_GIB`, which covers the IOVA range
    // `[NVME_IDENTIFY_NS_LIST_RESP_IOVA, NVME_IDENTIFY_NS_LIST_RESP_IOVA + 4096)`.
    // The controller acknowledged the submission via the
    // matching CQE above, so the DMA write has completed.
    // The slice lifetime ends at `_start`'s `sys_exit`; no other
    // code path holds these bytes.
    let ns_list_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            NVME_IDENTIFY_NS_LIST_RESP_IOVA as *const u8,
            NVME_IDENTIFY_NS_LIST_RESP_BYTES,
        )
    };
    let ns_list_view = match ActiveNsListView::new(ns_list_slice) {
        Ok(v) => v,
        Err(_) => unsafe { sys_exit(EXIT_NVME_NS_LIST_PARSE_FAILED) },
    };
    let first_nsid = match ns_list_view.first_active_nsid() {
        Some(nsid) => nsid,
        None => unsafe { sys_exit(EXIT_NVME_NS_LIST_EMPTY) },
    };

    // Step 4.17 — P6.7.10-pre.35: encode + submit the
    // Identify(Namespace) SQE (NVMe 1.4 § 5.15.2 Figure 245)
    // for the first active NSID discovered in step 4.16.d. The
    // 4 KiB response page at `NVME_IDENTIFY_NS_RESP_IOVA` is
    // parsed via `IdentifyNamespace::new` + `validated_byte_size()`
    // to extract the namespace capacity and validate that the
    // active LBA format uses 4 KiB sectors (`LBADS = 12`) per
    // OIP-014 § S6 step 10.
    let ns_sqe = encode_identify(
        IdentifyTarget::Namespace { nsid: first_nsid },
        NVME_IDENTIFY_NS_RESP_IOVA,
        0,
        NVME_IDENTIFY_NS_CID,
    );
    if admin_pair
        .submit(&ns_sqe, &mut mmio_write, asq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_NS_SUBMIT_FAILED) };
    }

    // Step 4.17.b — Poll the admin CQ for the matching CID. Same
    // structure as steps 4.15.b and 4.16.b: bounded budget, skip
    // strays, continue on `Ok(None)`, exit on `Err(_)`. The CQ
    // slot used by this completion is slot 2 (slots 0 and 1 were
    // consumed by the Identify Controller and ActiveNsList
    // completions above).
    let mut ns_polls: u32 = 0;
    let ns_cqe = loop {
        if ns_polls >= NVME_IDENTIFY_NS_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_NS_TIMEOUT) };
        }
        ns_polls = ns_polls.saturating_add(1);
        match admin_pair.drain_completion(&mut mmio_write, acq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IDENTIFY_NS_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_NS_DRAIN_FAILED) },
        }
    };

    // Step 4.17.c — Validate the completion status word.
    if !ns_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_NS_FAILED) };
    }

    // Step 4.17.d — Parse the 4 KiB response page via
    // `IdentifyNamespace::new` and validate that the active LBA
    // format uses 4 KiB sectors per OIP-014 § S6 step 10. The
    // `validated_byte_size()` call returns the namespace's total
    // byte capacity on success, or `UnsupportedLbads` when the
    // sector size is not 4 KiB — a hard bring-up failure because
    // the kernel BLK gateway cannot translate 512-byte-sector
    // requests to the OMNI OS 4 KiB page model.
    //
    // SAFETY: same as step 4.16.d — the DMA arena was installed
    // by `DmaMap (71)` at step 3 and covers the IOVA range
    // `[NVME_IDENTIFY_NS_RESP_IOVA, NVME_IDENTIFY_NS_RESP_IOVA + 4096)`.
    let ns_resp_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            NVME_IDENTIFY_NS_RESP_IOVA as *const u8,
            NVME_IDENTIFY_NS_RESP_BYTES,
        )
    };
    let ns_view = match IdentifyNamespace::new(ns_resp_slice) {
        Ok(v) => v,
        Err(_) => unsafe { sys_exit(EXIT_NVME_NS_PARSE_FAILED) },
    };
    if ns_view.validated_byte_size().is_err() {
        unsafe { sys_exit(EXIT_NVME_NS_UNSUPPORTED_LBADS) };
    }

    // Step 4.18 — P6.7.10-pre.36: Create I/O Completion Queue.
    // Per NVMe 1.4 § 5.3 the IO CQ MUST be created before the
    // matching IO SQ. Phase-1 creates exactly one IO queue pair
    // (QID 1) per OIP-014 § R2. The CQ data page lives at
    // `NVME_IO_CQ_IOVA` (offset `0x5000`) in the DMA arena;
    // `physically_contiguous = true` because Phase-1 uses PRP
    // mode (single 4 KiB page per queue).
    let create_cq_sqe = encode_create_io_cq(
        NVME_IO_QID,
        NVME_IO_QUEUE_DEPTH,
        NVME_IO_CQ_IOVA,
        NVME_IO_CQ_IRQ_VECTOR,
        true,
        true,
        NVME_CREATE_IO_CQ_CID,
    );
    if admin_pair
        .submit(&create_cq_sqe, &mut mmio_write, asq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_CREATE_IO_CQ_SUBMIT_FAILED) };
    }

    // Step 4.18.b — Poll for the Create IO CQ completion. The CQE
    // lands on CQ slot 3 (slots 0–2 consumed by the three Identify
    // completions above).
    let mut cq_create_polls: u32 = 0;
    let cq_create_cqe = loop {
        if cq_create_polls >= NVME_CREATE_IO_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_CREATE_IO_CQ_TIMEOUT) };
        }
        cq_create_polls = cq_create_polls.saturating_add(1);
        match admin_pair.drain_completion(&mut mmio_write, acq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_CREATE_IO_CQ_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_CREATE_IO_CQ_DRAIN_FAILED) },
        }
    };

    // Step 4.18.c — Validate the completion status word.
    if !cq_create_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_CREATE_IO_CQ_FAILED) };
    }

    // Step 4.19 — P6.7.10-pre.36: Create I/O Submission Queue.
    // Per NVMe 1.4 § 5.4 the IO SQ references the IO CQ created
    // in step 4.18 via `cq_id = NVME_IO_QID`. Queue priority is
    // `MEDIUM` (matches the Phase-1 default in
    // `CreateIoQueuesConfig::phase_1_default`).
    let create_sq_sqe = encode_create_io_sq(
        NVME_IO_QID,
        NVME_IO_QUEUE_DEPTH,
        NVME_IO_SQ_IOVA,
        NVME_IO_QID,
        CIOSQ_QPRIO_MEDIUM,
        true,
        NVME_CREATE_IO_SQ_CID,
    );
    if admin_pair
        .submit(&create_sq_sqe, &mut mmio_write, asq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_CREATE_IO_SQ_SUBMIT_FAILED) };
    }

    // Step 4.19.b — Poll for the Create IO SQ completion. The CQE
    // lands on CQ slot 4.
    let mut sq_create_polls: u32 = 0;
    let sq_create_cqe = loop {
        if sq_create_polls >= NVME_CREATE_IO_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_CREATE_IO_SQ_TIMEOUT) };
        }
        sq_create_polls = sq_create_polls.saturating_add(1);
        match admin_pair.drain_completion(&mut mmio_write, acq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_CREATE_IO_SQ_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_CREATE_IO_SQ_DRAIN_FAILED) },
        }
    };

    // Step 4.19.c — Validate the completion status word.
    if !sq_create_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_CREATE_IO_SQ_FAILED) };
    }

    // Step 4.20 — P6.7.10-pre.37: construct the IO queue pair and
    // issue the first NVM Read (LBA 0, 1 sector = 4 KiB) through
    // it. This is the first IO command the live image issues
    // end-to-end, validating the full data path:
    // `DriverLoad → MmioMap → DmaMap → IrqAttach → admin disable →
    //  admin bases → CC fields → admin enable → Identify Controller →
    //  Identify ActiveNsList → Identify Namespace → Create IO CQ →
    //  Create IO SQ → NVM Read LBA 0`.
    //
    // The IO queue pair (qid = 1) lives at IO SQ IOVA `0x6000` +
    // IO CQ IOVA `0x5000` in the DMA arena. The read data buffer
    // lives at IOVA `0x7000`.
    let mut io_pair = match AdminQueuePair::new_for_qid(
        NVME_IO_QID,
        u32::from(NVME_IO_QUEUE_DEPTH),
        u32::from(NVME_IO_QUEUE_DEPTH),
        NVME_ADMIN_DSTRD_DEFAULT,
    ) {
        Ok(p) => p,
        Err(_) => unsafe { sys_exit(EXIT_NVME_IO_PAIR_INVALID) },
    };

    // Step 4.20.b — Acquire &mut [u8] views into the IO SQ + CQ
    // data pages. Same passthrough IOMMU `iova == user_va` as the
    // admin pair at step 4.14.b.
    //
    // SAFETY: the IO CQ and IO SQ data pages were just installed
    // by the controller via the `Create IO CQ` / `Create IO SQ`
    // admin commands at steps 4.18–4.19. The controller
    // acknowledges their existence via the matching CQEs. Phase-1
    // IOMMU passthrough means `user_va == iova`. The slices are
    // non-overlapping (`IO_CQ_IOVA = 0x5000`, `IO_SQ_IOVA = 0x6000`).
    let io_sq_slice: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(NVME_IO_SQ_IOVA as *mut u8, NVME_IO_READ_DATA_BYTES)
    };
    let io_cq_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(NVME_IO_CQ_IOVA as *const u8, NVME_IO_READ_DATA_BYTES)
    };

    // Step 4.20.c — Encode + submit NVM Read (LBA 0, 1 sector).
    // PRP1 points to the read data buffer at `NVME_IO_READ_DATA_IOVA`;
    // PRP2 is zero (single-sector read fits in one PRP).
    let read_sqe = encode_read(
        first_nsid,
        0,
        1,
        NVME_IO_READ_DATA_IOVA,
        0,
        NVME_IO_READ_CID,
    );
    if io_pair
        .submit(&read_sqe, &mut mmio_write, io_sq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_IO_READ_SUBMIT_FAILED) };
    }

    // Step 4.20.d — Poll the IO CQ for the matching CID.
    let mut io_read_polls: u32 = 0;
    let io_read_cqe = loop {
        if io_read_polls >= NVME_IO_READ_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_IO_READ_TIMEOUT) };
        }
        io_read_polls = io_read_polls.saturating_add(1);
        match io_pair.drain_completion(&mut mmio_write, io_cq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IO_READ_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_IO_READ_DRAIN_FAILED) },
        }
    };

    // Step 4.20.e — Validate the completion status word.
    if !io_read_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_IO_READ_FAILED) };
    }

    // Step 4.21 — P6.7.10-pre.38: NVM Write LBA 0, 1 sector.
    // Writes the same data buffer at `NVME_IO_READ_DATA_IOVA`
    // (which was just filled by the NVM Read at step 4.20) back
    // to LBA 0. This validates the write path through the IO
    // queue pair. The write-then-read pattern is a canonical
    // data-integrity smoke (the bytes round-trip through the
    // controller's backend without corruption).
    let write_sqe = encode_write(
        first_nsid,
        0,
        1,
        NVME_IO_READ_DATA_IOVA,
        0,
        NVME_IO_WRITE_CID,
    );
    if io_pair
        .submit(&write_sqe, &mut mmio_write, io_sq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_IO_WRITE_SUBMIT_FAILED) };
    }

    // Step 4.21.b — Poll the IO CQ for the matching CID.
    let mut io_write_polls: u32 = 0;
    let io_write_cqe = loop {
        if io_write_polls >= NVME_IO_WRITE_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_IO_WRITE_TIMEOUT) };
        }
        io_write_polls = io_write_polls.saturating_add(1);
        match io_pair.drain_completion(&mut mmio_write, io_cq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IO_WRITE_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_IO_WRITE_DRAIN_FAILED) },
        }
    };

    // Step 4.21.c — Validate the completion status word.
    if !io_write_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_IO_WRITE_FAILED) };
    }

    // Step 4.22 — P6.7.10-pre.39: NVM Flush. Commits the volatile
    // write cache for `first_nsid`. Flush carries no PRPs; the
    // controller acknowledges it via a completion CQE. This is the
    // third and final IO command in the Phase-1 bring-up smoke,
    // completing the Read + Write + Flush triad that validates the
    // full NVMe data path.
    let flush_sqe = encode_flush(first_nsid, NVME_IO_FLUSH_CID);
    if io_pair
        .submit(&flush_sqe, &mut mmio_write, io_sq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_IO_FLUSH_SUBMIT_FAILED) };
    }

    // Step 4.22.b — Poll the IO CQ for the matching CID.
    let mut io_flush_polls: u32 = 0;
    let io_flush_cqe = loop {
        if io_flush_polls >= NVME_IO_FLUSH_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_IO_FLUSH_TIMEOUT) };
        }
        io_flush_polls = io_flush_polls.saturating_add(1);
        match io_pair.drain_completion(&mut mmio_write, io_cq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IO_FLUSH_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_IO_FLUSH_DRAIN_FAILED) },
        }
    };

    // Step 4.22.c — Validate the completion status word.
    if !io_flush_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_IO_FLUSH_FAILED) };
    }

    // Step 4.23 — P6.7.10-pre.40: NVM Dataset Management (Discard)
    // LBA 0, 1 sector. The Discard command requires a 16-byte Range
    // Descriptor buffer in the DMA arena at `NVME_IO_DISCARD_RANGE_IOVA`.
    // The helper `write_single_discard_range` fills the buffer; the
    // encoder `encode_discard` references it via PRP1. This is the
    // fourth and final IO command type in the Phase-1 bring-up smoke,
    // completing the full BLK request set (Read + Write + Flush + Discard).
    //
    // SAFETY: same passthrough IOMMU as all other DMA arena slices.
    let discard_range_slice: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(NVME_IO_DISCARD_RANGE_IOVA as *mut u8, 4096)
    };
    if write_single_discard_range(discard_range_slice, 0, 1).is_err() {
        unsafe { sys_exit(EXIT_NVME_IO_DISCARD_RANGE_FAILED) };
    }

    let discard_sqe = encode_discard(
        first_nsid,
        0,
        1,
        NVME_IO_DISCARD_RANGE_IOVA,
        NVME_IO_DISCARD_CID,
    );
    if io_pair
        .submit(&discard_sqe, &mut mmio_write, io_sq_slice)
        .is_err()
    {
        unsafe { sys_exit(EXIT_NVME_IO_DISCARD_SUBMIT_FAILED) };
    }

    // Step 4.23.b — Poll the IO CQ for the matching CID.
    let mut io_discard_polls: u32 = 0;
    let io_discard_cqe = loop {
        if io_discard_polls >= NVME_IO_DISCARD_POLL_LIMIT {
            unsafe { sys_exit(EXIT_NVME_IO_DISCARD_TIMEOUT) };
        }
        io_discard_polls = io_discard_polls.saturating_add(1);
        match io_pair.drain_completion(&mut mmio_write, io_cq_slice) {
            Ok(Some(fields)) if fields.cid == NVME_IO_DISCARD_CID => break fields,
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(_) => unsafe { sys_exit(EXIT_NVME_IO_DISCARD_DRAIN_FAILED) },
        }
    };

    // Step 4.23.c — Validate the completion status word.
    if !io_discard_cqe.is_success() {
        unsafe { sys_exit(EXIT_NVME_IO_DISCARD_FAILED) };
    }

    // Step 5 — Drive the 13-step bring-up FSM through its remaining
    // pure-state phases. With the full NVMe data path validated
    // (admin queues + IO queues + Read + Write + Flush + Discard),
    // the FSM can reach `Phase::Ready` via repeated `Event::Advance`.
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
