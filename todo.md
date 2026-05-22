# OMNI OS — Implementation TODO

> **Status:** **Phase 1 (Microkernel POC) — v0.3.0-alpha.1 RELEASED 2026-05-20; P6.7.10-pre.10 NVMe Create IO Queue encoders landed 2026-05-22, continuing TASK-005** — lands the tenth sub-slice of TASK-005 (NVMe live driver bring-up): `crates/omni-driver-nvme/src/admin.rs` extended with `encode_create_io_cq(qid, qsize, prp1, irq_vector, irq_enabled, physically_contig, cid) -> AdminSqe` (NVMe 1.4 § 5.5 opcode 0x05) and `encode_create_io_sq(qid, qsize, prp1, cq_id, queue_priority, physically_contig, cid) -> AdminSqe` (§ 5.4 opcode 0x01). CDW10 = QSIZE-1 (0-based per spec, computed via saturating_sub) | QID. CDW11 = IV|IEN|PC for CQ, CQID|QPRIO|PC for SQ. 4 QPRIO constants (URGENT/HIGH/MEDIUM/LOW) + 5 CDW11 bit/shift constants. queue_priority masked to 2 bits to prevent corruption-bleed into CQID. +16 host-side tests (8 encode_create_io_cq, 6 encode_create_io_sq, 1 cross-encoder opcode-distinctness, 1 fixture pair). Workspace test count 1389 → 1405 pass / 0 fail. Build Info: Active=`P6.7.10-pre.10 NVMe CreateIO`, Next=`P6.7.10-pre.11 NVMe live SQ MMIO`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.9 NVMe SQ/CQ ring landed 2026-05-22** — lands the ninth sub-slice of TASK-005 (NVMe live driver bring-up): new `crates/omni-driver-nvme/src/ring.rs` module declares pure-state SQ/CQ ring-buffer bookkeeping per NVMe 1.4 § 4.1 + § 4.6. `SqRing { capacity, tail, head_observed }` with `submit() -> Option<u16>` slot allocator, `update_head(u16)` controller-view sampler (modulo-clamped defensive), `is_full()`/`is_empty()`/`usable_capacity()` accessors. `CqRing { capacity, head, expected_phase }` with `try_take(&AdminCqe) -> Option<AdminCqeFields>` that parses the CQE through `crate::admin::parse_admin_cqe`, validates phase tag against `expected_phase`, advances head modulo capacity, and flips `expected_phase` on wrap. Initial state: `head = 0`, `expected_phase = true`. `RingError::{CapacityZero, CapacityTooLarge}` taxonomy (`#[non_exhaustive]`). All accessors take `self` by value (structs derive `Copy`, satisfy workspace `clippy::trivially_copy_pass_by_ref`). +18 host-side tests (6 construction & invariants, 5 SqRing slot-management, 5 CqRing phase-tag scenarios incl. capacity=1 flips on every take + two-full-laps restore phase, 1 RingError discriminant tripwire, 1 fixture). Workspace test count 1371 → 1389 pass / 0 fail. Build Info: Active=`P6.7.10-pre.9 NVMe SQ/CQ ring`, Next=`P6.7.10-pre.10 NVMe live SQ MMIO`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.8 NVMe PRP list landed 2026-05-22** — lands the eighth sub-slice of TASK-005 (NVMe live driver bring-up): `crates/omni-driver-nvme/src/transfer_model.rs` extended with the PRP descriptor encoders completing the multi-page transfer surface per NVMe 1.4 § 4.1.4: `prp1_for(buffer_iova) -> u64` (trivial pass-through), `prp2_for(buffer_iova, layout, list_page_iova) -> u64` (dispatches SinglePage→0 / TwoPages→buf+4096 / PrpList→list_page_iova), `write_prp_list_entries(buffer_iova, n_entries, dest) -> Result<(), PrpError>` (populates the 4 KiB PRP list page with 64-bit LE pointers to consecutive 4 KiB pages starting at index 1). `PrpError::{ListBufferTooSmall, TooManyEntries}` taxonomy (`#[non_exhaustive]`). Wraparound at `u64::MAX` saturated via `wrapping_add` (defensive against future cap relaxations). +12 host-side tests (`prp1_for`, 4 `prp2_for` dispatch, 6 `write_prp_list_entries` covering happy path, zero entries, full list, capacity/buffer overflows, exact-size buffer, LE byte ordering, error discriminant tripwire). Workspace test count 1358 → 1371 pass / 0 fail. Build Info: Active=`P6.7.10-pre.8 NVMe PRP list`, Next=`P6.7.10-pre.9 NVMe SQ/CQ ring`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.7 NVMe IO encoder landed 2026-05-22** — lands the seventh sub-slice of TASK-005 (NVMe live driver bring-up): new `crates/omni-driver-nvme/src/io.rs` module declares the four NVM IO submission encoders OMNI OS Phase 1 surfaces on `omni.svc.blk.<diskN>` per NVMe 1.4 § 6 — `encode_read` (opcode 0x02 § 6.9), `encode_write` (0x01 § 6.15), `encode_flush` (0x00 § 6.8), `encode_discard` (0x09 Dataset Management § 6.7 with AD bit). The 64-byte SQE layout is shared with `crate::admin` (NVMe 1.4 § 4.2 applies to both Admin and IO queues); re-using `AdminSqe` avoids doubling the auditable surface. Internal `encode_nvm_data_transfer(opc, nsid, lba, block_count, prp1, prp2, cid)` factor-shares the Read/Write field layout (CDW0=OPC|CID, NSID, DPTR.PRP1+PRP2, CDW10=SLBA[31:0], CDW11=SLBA[63:32], CDW12=NLB 0-based via `saturating_sub(1)`). 4 opcode constants + 2 DSM helper constants. +21 host-side tests (3 constant tripwires, 8 encode_read field-layout checks, 2 encode_write byte-identical-modulo-opcode tripwires, 2 encode_flush checks, 4 encode_discard checks, 2 cross-encoder invariants). Workspace test count 1337 → 1358 pass / 0 fail. Build Info: Active=`P6.7.10-pre.7 NVMe IO encoder`, Next=`P6.7.10-pre.8 NVMe PRP list`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.6 NVMe admin SQE/CQE landed 2026-05-22** — lands the sixth sub-slice of TASK-005 (NVMe live driver bring-up): new `crates/omni-driver-nvme/src/admin.rs` module declares the pure-state Admin Submission Queue Entry / Completion Queue Entry primitives per NVMe 1.4 base spec § 4.2 (64-byte SQE) and § 4.6 (16-byte CQE). `AdminSqe` (`repr(transparent)` over `[u8; 64]`) + `AdminCqe` (`[u8; 16]`) + `encode_identify(target, prp1, prp2, cid) -> AdminSqe` (spec-faithful little-endian field layout: CDW0 OPC=0x06|CID, NSID dispatch by `IdentifyTarget`, DPTR.PRP1/PRP2, CDW10.CNS) + `parse_admin_cqe(&AdminCqe) -> AdminCqeFields` (extracts cdw0/sq_head/sq_id/cid/phase/sc/sct/more/do_not_retry from raw 16-byte CQE) + `AdminCqeFields::packed_status() -> u16` (re-packs into the 16-bit status word `omni_types::nvme::NvmeEvent::CommandComplete::status` carries) + `is_success()`. 8 opcode + CNS constants + 2 size constants (`ADMIN_SQE_BYTES = 64`, `ADMIN_CQE_BYTES = 16`). `crates/omni-driver-nvme/Cargo.toml` extended with `omni-types = { path = ".../omni-types", default-features = false }` (strips `id-generation` → `getrandom` so the downstream `omni-driver-nvme-image` ELF still compiles on `x86_64-unknown-none`, mirroring `omni-kernel`'s pin). +30 host-side tests (7 constant + struct-size tripwires, 10 encode_identify field-layout checks, 5 parse_admin_cqe extraction checks, 4 packed_status round-trips, 2 byte-layout pinning, 2 fixtures). Workspace test count 1307 → 1337 pass / 0 fail. Build Info: Active=`P6.7.10-pre.6 NVMe admin SQE`, Next=`P6.7.10-pre.7 NVMe IO encoder`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc + lint-oips). No new external dependency. **Predecessor: P6.7.10-pre.5 NVMe ABI types landed 2026-05-22** — lands the fifth sub-slice of TASK-005 (NVMe live driver bring-up): new `crates/omni-types/src/nvme.rs` module declares the canonical wire shapes carried on the two driver-private NVMe channels per OIP-Driver-NVMe-014 § S2 (`omni.driver.nvme.cmd`) + § S3 (`omni.driver.nvme.evt`). Three `#[non_exhaustive]` `serde`-derived enums (`IdentifyTarget::{Controller, Namespace{nsid}, ActiveNsList}`, `NvmeCommand::{Identify, Read, Write, Flush, Discard, GetLogPage, FormatNVM}`, `NvmeEvent::{CommandComplete{opaque_id,status,cdw0}, AsyncEvent, LinkStateChange, ControllerFatal}`) + 5 anchor `pub const`s (`CMD_CHANNEL_NAME = "omni.driver.nvme.cmd"`, `EVT_CHANNEL_NAME = "omni.driver.nvme.evt"`, `MAX_BLOCK_COUNT_PER_REQUEST = 2048` matches `crate::blk`, `BLOCK_SIZE_BYTES = 4096` matches `crate::blk`, `RESERVED_DRIVER_OPAQUE_ID = 0` sentinel for driver-internal admin commands). All encoding routes through `omni_types::wire::encode_canonical` per OIP-Serde-004. Module documentation extended with buffer-ownership rules, opaque-id correlation semantics, status-word semantics (NVMe 1.4 § 4.5 SCT:SC pair packed into `u16`), backward-compatibility policy referencing OIP-014 § S7. +30 host-side tests (5 constant tripwires, 10 NvmeCommand round-trips, 6 NvmeEvent round-trips, 5 wire invariants, 3 discriminator-distinctness checks, 1 cross-channel correlation test). Workspace test count 1277 → 1307 pass / 0 fail. Build Info: Active=`P6.7.10-pre.5 NVMe ABI types`, Next=`P6.7.10-pre.6 NVMe admin queue`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc omni-types + rustdoc workspace + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.4 NVMe BLK wire landed 2026-05-22** — lands the fourth sub-slice of TASK-005 (NVMe live driver bring-up): `crates/omni-driver-nvme-image/src/main.rs` extended with a 3-syscall block between the existing `IrqAttach (72)` and the bring-up FSM that consumes the new kernel-side BLK registry (P6.7.10-pre.3). `IpcCreateChannel (20)` allocates a fresh kernel-owned channel via the legacy MB12 fast-path (`BLK_CHANNEL_QUEUE_DEPTH = 1024`, `BackpressurePolicy::Block`, `tee_bound = false`); `BlkRegister (76)` records the `omni.svc.blk.nvme0` → `channel_id` mapping (kernel verifies ownership of the just-created channel by construction); `BlkLookup (78)` round-trips the registration as defence-in-depth (`ENOENT` or `channel_id` mismatch aborts the driver before any FSM advance — would otherwise mis-route future filesystem requests). New syscall-number constants (`SYS_IPC_CREATE_CHANNEL = 20`, `SYS_BLK_REGISTER = 76`, `SYS_BLK_LOOKUP = 78`) + BLK channel constants (`NVME_DISK_SLOT = b"nvme0"`, `BLK_CHANNEL_QUEUE_DEPTH`, `BLK_CHANNEL_BACKPRESSURE_BLOCK`, `BLK_CHANNEL_TEE_NOT_BOUND`) + 4 new sentinel exit codes (`EXIT_IPC_CREATE_FAILED = 100`, `EXIT_BLK_REGISTER_BASE = 110`, `EXIT_BLK_LOOKUP_NOT_FOUND = 131`, `EXIT_BLK_LOOKUP_MISMATCH = 132`). Byte-slice `NVME_DISK_SLOT: &[u8]` avoids the `PanicOnAlloc` global-allocator panic that a `String` would trigger inside the `no_std + no_main` ELF. Workspace test count stable at 1277 pass / 0 fail — the new code is `no_main` ELF entry path that cannot be unit-tested in-crate; coverage relies on the kernel-side host tests landed in P6.7.10-pre.3 plus the Proxmox boot smoke. Build Info: Active=`P6.7.10-pre.4 NVMe BLK wire`, Next=`P6.7.10-pre.5 NVMe admin queue`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.3 BLK syscalls landed 2026-05-22** — lands the third sub-slice of TASK-005 (NVMe live driver bring-up): three new syscalls `BlkRegister (76)` / `BlkUnregister (77)` / `BlkLookup (78)` in `crates/omni-kernel/src/bare_metal/syscall_entry.rs::blk_handlers` (bare-metal-only module) bridge the kernel-internal `BlkChannelRegistry` (landed P6.7.10-pre.2) to user space through the rich two-register return path (`SyscallReturn { rax, rdx }`) per OIP-Driver-NVMe-014 § S4 + § S6 step 12. `BlkRegister` accepts `(disk_slot_ptr, disk_slot_len, channel_id, 0, 0, 0)` and verifies `ipc_registry().channel(channel_id).owner == caller_task` before recording the canonical `omni.svc.blk.<disk_slot>` → `ChannelId` mapping (mismatch → `EACCES`, unknown id → `EINVAL`). `BlkUnregister` accepts `(disk_slot_ptr, disk_slot_len, 0, 0, 0, 0)` and delegates to `BlkChannelRegistry::unregister` (owner-only — `OwnerMismatch → EACCES`). `BlkLookup` accepts the same shape and returns the live `channel_id` in `rax` on success or `(0, ENOENT)` on miss — read-only because the channel id alone confers no IPC authority. New process-global `static mut BLK_REGISTRY: BlkChannelRegistry = BlkChannelRegistry::new()` in `crates/omni-kernel/src/services/blk.rs` (gated on `bare-metal` + `target_arch = "x86_64"`, mirroring the `IPC_REGISTRY` singleton — single-CPU + interrupt-masked SYSCALL provides the no-aliasing invariant per ADR-0005) + `blk_registry_mut()` / `blk_registry()` accessors + `errno_for(BlkRegistryError) -> u64` host-callable mapper publishing the POSIX-aligned errno taxonomy (`EINVAL` / `EEXIST` / `ENOSPC` / `ENOENT` / `EACCES` / `EIO`). `crates/omni-kernel/src/syscall.rs` extended with three new `SyscallNumber` discriminants (76/77/78) + three new errno constants (`ENOENT = 2`, `EIO = 5`, `EEXIST = 17`). Task-exit clean-up wired: `task_exit` invokes `blk_handlers::tear_down_blk_channels(current)` AFTER the existing MMIO/DMA/IRQ/PCI teardown chain so a crashed driver does not leak stale registry entries. Dispatcher routing: new `BlkRegister | BlkUnregister | BlkLookup` arms in legacy `dispatch` (`CapabilityDenied`, rich-path-only) + three new arms in `dispatch_full` (live handlers) + three new arms in `kernel_syscall_dispatch (extern "C")` (`76/77/78 → SyscallNumber::Blk*`). +8 host-side tests (7 in `services::blk::tests::*` for `errno_for` coverage, 1 in `syscall_entry::tests::*` for the wire-number translation arm). Workspace test count 1269 → 1277 pass / 0 fail. Build Info: Active=`P6.7.10-pre.3 BLK syscalls`, Next=`P6.7.10-pre.4 NVMe driver image`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.2 BLK registry landed 2026-05-22** — lands the second sub-slice of TASK-005 (NVMe live driver bring-up): `crates/omni-kernel/src/services/blk.rs` + `crates/omni-kernel/src/services/mod.rs` (new modules, `#[cfg(feature = "bare-metal")]`) declare the kernel-side BLK channel registry per OIP-Driver-NVMe-014 § S4 + § S6 step 12. The registry maps disk slot (`"nvme0"`, `"sata0"`, …) → live `ChannelId` so future filesystem services can resolve `omni.svc.blk.<diskN>` without sniffing the IPC layer by string. `BlkChannelRegistry` exposes `register` / `unregister` / `lookup_disk_slot` / `lookup_channel_name` / `lookup_channel_id` / `clear_for_owner` (task-exit hook) backed by a bounded `Vec<BlkChannelEntry>` (`MAX_BLK_CHANNELS = 64`, `MAX_DISK_SLOT_LEN = 32`). Disk slot validator restricts the alphabet to `[A-Za-z0-9_-]` and rejects empty / oversized / non-ASCII / control-byte / dot inputs so a compromised driver cannot smuggle log-line forgeries through the kernel boot log. `BlkRegistryError::{DiskSlotEmpty, DiskSlotTooLong, DiskSlotInvalidChar, DiskSlotAlreadyRegistered, RegistryFull, DiskSlotNotRegistered, OwnerMismatch, Internal}` (`#[non_exhaustive]`). `register` returns the canonical channel name (`"omni.svc.blk.<diskN>"`) pre-built once at insertion so consumer call sites do not re-allocate on every lookup. Consumes `omni_types::blk::CHANNEL_NAME_PREFIX` so the prefix stays single-sourced across the two crates. +32 host-side tests under `omni_kernel::services::blk::tests::*` (construction invariants, registration happy-path + validator + duplicate + capacity, unregister owner-only + non-owner reject + unknown-slot reject, re-register after unregister, lookup by slot / channel name / channel id, `clear_for_owner` task-exit drain + counter, insertion-order preservation, swap_remove non-aliasing). Workspace test count 1237 → 1269 pass / 0 fail. Build Info: Active=`P6.7.10-pre.2 BLK registry`, Next=`P6.7.10-pre.3 BLK syscalls`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace + lint-oips). No new dependency. **Predecessor: P6.7.10-pre.1 BLK types landed 2026-05-22, opening TASK-005** — opens the first sub-slice of TASK-005 (NVMe live driver bring-up): `crates/omni-types/src/blk.rs` (new module) declares the canonical wire shape of the generic BLK service channel (`omni.svc.blk.<diskN>`) per OIP-Driver-NVMe-014 § M3 / § S4. Two `#[non_exhaustive]` `serde`-derived enums — `BlkRequest::{Read{lba,count,buf_iova}, Write{lba,count,buf_iova}, Flush, Discard{lba,count}}` and `BlkResponse::{Ok, NotSupported, DeviceError(u16), OutOfRange, InvalidArgument}` — every storage driver (NVMe today, future SATA / virtio-blk) MUST present, plus 4 anchor `pub const`s (`NON_NVME_DEVICE_ERROR = 0xFFFF`, `MAX_BLOCK_COUNT_PER_REQUEST = 2048`, `BLOCK_SIZE_BYTES = 4096`, `CHANNEL_NAME_PREFIX = "omni.svc.blk."`). All encoding routes through `omni_types::wire::encode_canonical` / `decode_canonical` per OIP-Serde-004. +24 host-side tests under `omni_types::blk::tests::*`. Workspace test count 1213 → 1237 pass / 0 fail. Build Info: Active=`P6.7.10-pre.1 BLK types`, Next=`P6.7.10-pre.2 BLK registry`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace + lint-oips). No new dependency. **TASK-010 (VT-d / AMD-Vi IOMMU backends) CLOSED**. **TASK-005 (NVMe live driver bring-up) START — pre.1 di N**. **P6.7.10-pre.2 (kernel-side BLK channel registry at `crates/omni-kernel/src/services/blk.rs` consuming `CHANNEL_NAME_PREFIX`) prossimo**. **Predecessor: P6.7.9-pre.11 DTE+TE live landed 2026-05-22, closing TASK-010** — closes the sixth and final TASK-010 milestone: live VT-d Context-Entry / AMD-Vi DTE install per bound BDF + `GCMD.TE` / `CTRL.IommuEn` flip, gating Phase-1 DMA isolation through the per-domain page tables provisioned by P6.7.9-pre.10. New `BusContextTable` refcounted allocator on `VtdBackend` (lazy per-bus 4-KiB context-table page through `pt_alloc::FrameSource`; `swap_remove` + `free_frame` when last device on bus detaches), symmetric managed install/release helpers (`install_device_entry_with_alloc` / `release_device_entry_with_alloc`) with rollback on failure, and per-vendor `enable_translation` methods (Intel: `GCMD.TE` + poll `GSTS.TES`; AMD: RMW `CTRL.IommuEn` + `INVALIDATE_ALL` drain). `driver_load` now installs the per-device DTE/Context-Entry for every bound BDF after the PT root provisioning and flips translation enable on the first success; `tear_down_pci_bindings` symmetrically releases the bus context-table refcount (Intel) and detaches the device entries. +21 host-side tests; workspace test count 1192 → 1213 pass / 0 fail. Build Info: Active=`P6.7.9-pre.11 DTE+TE live`, Next=`P6.7.10 NVMe live (TASK-005)`, Phase=`1 - Microkernel POC  (~99.97%)`. All gates clean. No new dependency. **TASK-010 (VT-d / AMD-Vi IOMMU backends) CLOSED**. **P6.7.10 (NVMe live bring-up — TASK-005) prossimo**. **Predecessor: P6.7.9-pre.10 PT wire DriverLoad landed 2026-05-22** — closes the fifth TASK-010 milestone: per-domain page-table root provisioning is now driven from the live `DriverLoad (73)` syscall handler. New `crates/omni-kernel/src/bare_metal/iommu/kernel_frame_source.rs` module exposes `KernelFrameSource<'a, const N: usize>`, a thin adapter wrapping `&'a mut BitmapFrameAllocator<N>` + the bootloader direct-map offset that implements `pt_alloc::FrameSource`. The `driver_load` handler now provisions one 4-KiB-aligned zero-filled root frame per IOMMU domain after the per-BDF attach loop succeeds (best-effort; passthrough short-circuits without touching `FRAME_ALLOC`). `tear_down_pci_bindings` symmetrically releases the root frame on process exit. +6 host-side tests under `bare_metal::iommu::kernel_frame_source::tests::*`; workspace test count 1186 → 1192 pass / 0 fail. Build Info: Active=`P6.7.9-pre.10 PT wire DriverLoad`, Next=`P6.7.9-pre.11 PT install DTE`, Phase=`1 - Microkernel POC  (~99.95%)`. All gates clean (fmt + workspace clippy + bare-metal clippy + mb12-userprobe clippy + kernel-runner clippy + 3 driver-image siblings clippy + check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace + lint-oips). No new dependency. **P6.7.9-pre.11 (per-device DTE / Context-Entry install — wire `install_vt_d_device_entry` / `install_amd_vi_device_entry` against the BDFs bound in P6.7.9-pre.8 using the per-domain root stored by P6.7.9-pre.10, then flip `GCMD.TE` / `CTRL.IommuEn`) prossimo, closing the live DMA-isolation deliverable of TASK-010**. **Predecessor: P6.7.9-pre.9 IOMMU PT alloc landed 2026-05-22** — closes the per-domain page-table root provisioning surface that the future P6.7.9-pre.10 (driver_load wires the live `install_*_device_entry` MMIO path) consumes: new `crates/omni-kernel/src/bare_metal/iommu/pt_alloc.rs` module defines a vendor-neutral `FrameSource` trait (`alloc_zeroed_frame() -> Option<u64>` + `free_frame(phys: u64)`) and a `DomainPageTables` registry that maps `DomainId → root_phys` with bounded linear lookup over a `Vec<DomainPtEntry>`. `provision(domain, src)` allocates one 4-KiB-aligned root frame through the supplied `FrameSource`, validates alignment defensively (misaligned returns are rejected and freed back to the pool), records the binding, and returns the new physical address. `release(domain, src)` reverses the binding and returns the frame to the pool. `DomainPtError::{FrameAllocFailed, Misaligned, AlreadyProvisioned, NotProvisioned}` taxonomy. `MockFrameSource` (`cfg(test)`-only) hands out deterministic 4-KiB-aligned addresses and tracks `(alloc, free)` calls plus a force-fail flag and a force-next-phys override for the misalignment defensive test. `VtdBackend` + `AmdViBackend` embed `domain_pts: DomainPageTables` and expose `provision_domain_pt(domain, &mut dyn FrameSource) -> Result<u64, DomainPtError>` + `release_domain_pt(...)` + `domain_pt_root_phys(domain) -> Option<u64>` + `domain_pt_entries() -> &[DomainPtEntry]`. Module-level dispatch helpers `iommu_provision_domain_pt(domain, src)` / `iommu_release_domain_pt(domain, src)` / `iommu_domain_pt_root_phys(domain)` route through `IommuKind` — passthrough returns `Ok(0)` / `Ok(())` / `None` without touching `src` so callers can drive the same code path on platforms without an IOMMU. +19 new host-side tests (13 in `pt_alloc::tests::*` covering happy path / double-provision / misaligned-frame defensive-return / frame-alloc-failure / release-unknown-domain / re-provision-after-release / insertion-order / counter-tracking, 6 in `iommu::tests::*` covering passthrough dispatch / Intel + AMD round-trip / double-provision reject / release-unknown reject / frame-alloc-failure surface). Workspace test count 1167 → **1186 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.9 IOMMU PT alloc` (cyan), Next=`P6.7.9-pre.10 PT wire DriverLoad`, Phase=`1 - Microkernel POC  (~99.9%)`. Acceptance gates: `cargo fmt --all -- --check` clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` clean. `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --all-features` clean. `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`. **No new dependency**. Pure-state vendor-neutral allocator — zero MMIO bytes emitted; the live `install_*_device_entry` paths will consume `iommu_domain_pt_root_phys(domain)` once P6.7.9-pre.10 wires `DriverLoad` to call them after the PCI bind. **Predecessor:** P6.7.9-pre.8 IOMMU per-device attach surface landed 2026-05-21** — closes the third TASK-010 milestone (per-device translation gating): adds a vendor-neutral `PciBdf(u16)` newtype on `bare_metal::iommu` packing the PCI requester ID (bus/device/function) in the canonical 16-bit form consumed by both VT-d source-id and AMD-Vi `DeviceID`; extends the `IommuBackend` trait with `attach_device(bdf, domain) -> Result<(), IommuError>` + `detach_device(bdf) -> Result<(), IommuError>` and routes both through `IommuKind` dispatch (`Passthrough` returns `Ok(())` silently; `VtdBackend` + `AmdViBackend` record + remove the binding in their internal `Vec<{Vtd,AmdVi}Attachment>` with duplicate-attach → `IommuError::Unsupported` and detach-on-unknown → `IommuError::Unsupported`). Module-level `iommu_attach_device(bdf, domain)` / `iommu_detach_device(bdf)` close the public surface so the future driver framework (P6.7.9-pre.8) can call them without sniffing the vendor. **VT-d live install** (`#[cfg(target_os = "none")] VtdBackend::install_device_entry(phys_offset, bdf, domain, slpt_phys, context_table_phys, AddressWidth, TranslationType)`) writes (a) the spec-faithful 128-bit context entry into the per-bus context table at `context_entry_offset(bdf) = devfn * 16`, (b) the corresponding root entry into the global root-table page at `root_entry_offset(bus) = bus * 16`, (c) submits a per-domain context-cache invalidate descriptor (`encode_context_cache_domain_invalidate(domain)` → Type `0x1` + G `10` + DID in bits 16..31) and a per-domain IOTLB invalidate descriptor (`encode_iotlb_domain_invalidate(domain)` → Type `0x2` + G `10` + DID) on the invalidation queue, advancing `IQT` and polling `IQH` to drain — a new `submit_iq_descriptor` helper centralises the wrap-on-`INV_QUEUE_BYTES` ring management. **AMD-Vi live install** (`#[cfg(target_os = "none")] AmdViBackend::install_device_entry(phys_offset, bdf, domain, iopt_phys, IommuFlags, PageMode)`) writes the 256-bit DTE into the device table at `dte_offset(bdf) = bdf.raw() * 32` (bounds-checked against `DEVICE_TABLE_BYTES = 4096` for the Phase-1 1-frame table), then submits an `INVALIDATE_DEVTAB_ENTRY(device_id = bdf.raw())` and an `INVALIDATE_IOMMU_PAGES(domain)` command (new `encode_invalidate_iommu_pages_domain(domain)` encoder pinning opcode `0x3` + DID in bits 32..47 + S=1 spans-all in the high qword per spec § 5.4.4) on the command-buffer ring via the new `submit_cmd_descriptor` helper. **Error taxonomies**: `VtdAttachError` (`NotActivated/DomainNotInstalled/AlreadyAttached/AddressMisaligned/InvalidationTimeout`) and `AmdViAttachError` (same 4 + `DeviceTableTooSmall/UnsupportedMode/InvalidationTimeout`), both mapped to `IommuError` (`ActivationFailed/InvalidDomain/Unsupported/AddressMisaligned/DomainTableFull`) via `From` impls. Module-level live wrappers `#[cfg(target_os = "none")] install_vt_d_device_entry(...)` + `install_amd_vi_device_entry(...)` symmetric to the existing `activate_intel_vt_d` / `activate_amd_vi` pair. New MMIO write helpers `write_context_entry_at` / `write_root_entry_at` (VT-d) + `write_dte_at` (AMD-Vi) wrap the volatile writes against the per-page buffers. `INV_DESC_CTX_GRAN_DOMAIN = 0b10 << 4` + `INV_DESC_IOTLB_GRAN_DOMAIN = 0b10 << 4` + `DEVICE_TABLE_BYTES = 4096` constants. +33 new host-side tests (9 in `iommu::tests`, 14 in `vtd::tests`, 10 in `amdvi::tests`). Workspace test count 1128 → **1161 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.7 IOMMU device wire` (cyan), Next=`P6.7.9-pre.8 driver PCI bind`, Phase=`1 - Microkernel POC  (~99.95%)`, Tests=`1161 workspace pass`. Acceptance gates: `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal,mb12-userprobe -- -D warnings` clean. `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. Clippy 3 driver-image siblings (`x86_64-unknown-none --release`) clean. `cargo fmt --all -- --check` clean. `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`. `RUSTDOCFLAGS="-D warnings" cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` clean. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` clean. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)`. **No new dependency**. `kmain` deliberately **NOT** wired — the live `install_*_device_entry` paths require a real PCI-bound driver process (the cap-token resource match) and so are exercised in P6.7.9-pre.8 when the driver framework attaches its first device. Until then `GCMD.TE` (VT-d) and `CTRL.IommuEn` (AMD-Vi) stay deasserted, preserving Phase-1 pass-through semantics. Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`. **Next: P6.7.9-pre.8** (driver framework PCI bind — kmain wires `install_*_device_entry` against the first PCI device the driver framework loads after the cap-token resource match, then flips `GCMD.TE` (Intel) / `CTRL.IommuEn` (AMD) to gate translation, closing TASK-010 with the live DMA-isolation deliverable). **Predecessor:** P6.7.9-pre.6 AMD-Vi live MMIO register programming landed 2026-05-21 (closes the AMD half of TASK-010: `AmdViBackend` gains the live-programming surface symmetric to `VtdBackend` — new `prepare_activation(unit_base, device_table_phys, command_buffer_phys, event_log_phys)` + `#[cfg(target_os = "none")] pub unsafe fn activate_hardware(phys_offset)` that drives the spec-faithful AMD IOMMU rev 3.10 § 5.3 + § 5.5 sequence (`DEV_TAB_BAR` + `CMD_BUF_BASE` + `EVENT_LOG_BASE` writes, zero CMD/EVENT Head/Tail, raise `CTRL.CmdBufEn | CTRL.EventLogEn`, poll `STATUS.CmdBufRun` then `STATUS.EventLogRun`, submit one `INVALIDATE_DEVTAB_ENTRY` command at slot 0, bump `CMD_BUFFER_TAIL` to 16, poll `CMD_BUFFER_HEAD` until drained). `CTRL.IommuEn` deliberately **NOT** raised — per-device translation gating lands in P6.7.9-pre.7. New `AmdViActivateError` taxonomy (`NotPrepared` / `CmdBufStartTimeout` / `EventLogStartTimeout` / `InvalidationTimeout`) maps to `IommuError::ActivationFailed`. Module-level surface additions: 5 STATUS bit constants + 4 command-buffer layout constants + 4 event-log layout constants + 1 device-table size constant + 5 command opcode constants + `AMDVI_ACTIVATION_POLL_LIMIT = 1_000_000` + 4 pure-function encoders (`encode_device_table_base`, `encode_command_buffer_base`, `encode_event_log_base`, `encode_invalidate_devtab_entry`). `bare_metal/iommu/mod.rs` exposes new `prepare_amd_vi_unit` + `#[cfg(target_os = "none")] pub unsafe fn activate_amd_vi`; `iommu_hardware_activated()` extended to dispatch through AMD too. `kmain` extended with parallel AMD-Vi activation block after the VT-d block — allocates device-table + command-buffer + event-log frames, zero-fills via direct-map, prepares + activates only when `iommu_unit_base() != 0 && iommu_vendor() == Amd`, emits `[iommu] amd-vi activated unit=<base>` / `activate skip` / `activate err` / `prepare err` / `alloc err` boot logs. +27 new host-side tests (24 in `amdvi::tests`, 3 in `iommu::tests`); workspace test count 1101 → 1128 pass / 0 fail. All gates green. Build Info: Active=`P6.7.9-pre.6 AMD-Vi live MMIO`, Next=`P6.7.9-pre.7 IOMMU domain wire`, Tests=`1128 workspace pass`. Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`. **Predecessor:** P6.7.9-pre.4 DMA-Map vendor switch landed 2026-05-21** (kernel-wide `IOMMU_BACKEND: spin::Mutex<IommuKind>` enum-dispatched static is now wired from boot — `bare_metal/iommu/mod.rs` exposes a new `IommuKind` enum (`Passthrough(PassthroughBackend) | Intel(vtd::VtdBackend) | Amd(amdvi::AmdViBackend)`) implementing `IommuBackend` via match arms, plus `install_backend_for_vendor(vendor)` one-shot installer + `with_iommu_backend(|b| …)` mutex-bracketed accessor + `domain_for_task(task_id) -> DomainId` projector that maps `TaskId` into the 16-bit VT-d DID / AMD-Vi `DomainID` space. `VtdBackend::new()` + `AmdViBackend::new()` promoted to `pub const fn` so `IommuKind::new_passthrough` is `const` and the static can initialise without `OnceLock`. `kmain` extended: after `iommu::probe` returns + telemetry globals are set, `install_backend_for_vendor(probe.vendor)` swaps the live backend variant. `dma_map_handlers::dma_map` (bare_metal/syscall_entry.rs) refactored: after the cap-token verification and the duplicate-iova check, derives `domain_id = domain_for_task(current.0)` and calls `with_iommu_backend(|b| b.install_domain(domain_id))` (idempotent for repeat invocations from the same process; returns `ENOSPC` on backend error). After the contiguous frame install completes successfully, the handler calls `b.map(domain_id, iova_base, phys_base, len, flags)` with `IommuFlags` translated from the `direction` argument (`0→READ`, `1→WRITE`, `2→READ|WRITE`) and `b.flush(domain_id)` — best-effort; backend `map` failure triggers a full PT rollback + `ENOSPC`. `tear_down_dma_mappings(task)` extended: for every `DmaMapping` on the PCB it calls `b.unmap(domain_id, iova_base, len_bytes)` + `b.flush(domain_id)` (errors swallowed — `UnmapFailed` is benign for teardown after a previous rollback). +14 host-side test (`bare_metal::iommu::tests::*`): `iommu_kind_default_is_passthrough`, `iommu_kind_new_passthrough_is_const_constructible` (compile-time `const _: IommuKind = …`), `iommu_kind_intel_vendor_routes`, `iommu_kind_amd_vendor_routes`, `iommu_kind_passthrough_rejects_misaligned`, `iommu_kind_intel_rejects_unknown_domain`, `install_backend_for_vendor_switches_to_intel` + `_amd` + `_resets_passthrough` + `_is_idempotent_for_intel`, `with_iommu_backend_round_trips_state`, `domain_for_task_maps_low_16_bits`, `domain_for_task_truncates_high_bits`, `iommu_backend_static_initial_state_is_passthrough`. Workspace test count 1064 → **1078 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.4 DMA-Map vendor switch` (cyan), Next=`P6.7.9-pre.5 IOMMU register programming`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`1078 workspace pass`. Acceptance gates: workspace clippy + bare-metal + mb12-userprobe + kernel-runner + 3 driver-image siblings (`x86_64-unknown-none --release`) + fmt + check-no-blanket-allow (16 crate-root files) + rustdoc bare-metal + rustdoc workspace + lint-oips (0 error / 0 warning su 19 file) tutti clean. **No new dependency** — `spin = "0.9"` was already a kernel dep (`entropy::KERNEL_CSPRNG`). Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`. Both scaffolds (VT-d + AMD-Vi) remain **dormant** — the dispatch wiring is in place but the live MMIO register programming (queued-invalidation descriptor write, root-table write, per-IOMMU control bits) is deferred to P6.7.9-pre.5+. **Next: P6.7.9-pre.5 (Intel VT-d live MMIO register programming — root-table install, queued-invalidation queue, IOTLB invalidation descriptor)**. Previous (intermediate, doc-lagged): 2026-05-21 (P6.7.9-pre.3 AMD-Vi backend scaffold — commit `8e719f5`; doc entry merged forward into this pre.4 block to avoid a churning doc-only patch between two adjacent scaffold slices: the slice landed `crates/omni-kernel/src/bare_metal/iommu/amdvi.rs` (~1237 lines) pinning AMD IOMMU spec rev 3.10 § 5.5 MMIO register offsets + § 5.2.2 Device Table Entry encoder + § 5.3.1 I/O Page Table encoder + § 5.7 Extended Feature Register decoders + a host-testable dormant `AmdViBackend` symmetric to `VtdBackend`; +34 host-side tests; workspace test count 1030 → 1064 pass / 0 fail; Build Info Active=`P6.7.9-pre.3 AMD-Vi backend` / Next=`P6.7.9-pre.4 DMA-Map vendor switch` / Tests=`1064 workspace pass`.) Previous: 2026-05-21 (P6.7.9-pre.2 Intel VT-d backend scaffold landed 2026-05-21** (dormant `crates/omni-kernel/src/bare_metal/iommu/vtd.rs` module: pins Intel VT-d spec rev 4.1 § 10.4 register offsets `VER/CAP/ECAP/GCMD/GSTS/RTADDR/CCMD/FSTS/FECTL/FEDATA/FEADDR/FEUADDR/PMEN/IQH/IQT/IQA` + `GCMD` bit positions `TE/SRTP/SFL/EAFL/WBF/QIE/IRE/SIRTP/CFI` as `pub const u32`; provides pure-function encoders for legacy translation data structures — `encode_root_entry`/`encode_root_entry_absent` for the 128-bit root entry, `encode_context_entry`/`encode_context_entry_absent` for the 128-bit context entry with `TranslationType` 4-variant enum + `AddressWidth` 4-variant enum, `encode_slpte` for the 64-bit second-level page table entry — and decoders for the `CAP` register fields (`cap_domain_count`, `cap_supported_agaw`, `cap_caching_mode`, `pick_highest_supported_agaw`); ships a host-testable `VtdBackend` struct that implements `IommuBackend` by tracking domains + mappings in internal `Vec`s and **emits zero MMIO bytes** (live register programming deferred to P6.7.9-pre.4). `VtdError` → `IommuError` mapping preserves vendor-neutral taxonomy. +32 host-side test (register-offset spec pins, encoder bit positions, CAP field decoders, VtdBackend happy path + error path including unknown domain, misaligned arguments, double-unmap). Workspace test count 998 → **1030 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.2 VT-d backend` (cyan), Next=`P6.7.9-pre.3 AMD-Vi backend`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`1030 workspace pass`. No `kmain` wiring — the probe still selects vendor for telemetry only; `dma_map_handlers::dma_map` continues to use `PassthroughBackend` until the swap in P6.7.9-pre.4. Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`. Previous: 2026-05-21 (P6.7.8.10 driver-shared SDK wired — closes the deposit trampoline arc by giving every first-party driver image a dep-free, `no_std`, zero-supply-chain crate `omni-driver-shared` that reads kernel-deposited capability tokens at the well-known user-VA `0x0010_0000` via `caps::find_token(action_tag, predicate) -> Option<&'static [u8]>` + the `caps::find_token_in_buf` pure-function variant for host tests, plus an `OmniCapsHeader` repr(C) header parser with explicit `OmniCapsError::{InvalidMagic, UnsupportedVersion, EntryCountExceeded, OutOfBoundsOffset}` taxonomy. Workspace-member with 36 unit tests + 8 e2e tests under `tests/e2e_deposit_round_trip.rs` (round-trip `CapabilityToken → encode_canonical → OMNICAPS page → find_token_in_buf → decode_canonical → equality`) + 12 doc tests, all green. **Live wiring of the 3 driver-image siblings**: `crates/omni-driver-{net-virtio,nvme,e1000e}-image/src/main.rs` rewritten — `#[allow(dead_code)]` removed from `syscall5`/`SYS_MMIO_MAP`/`SYS_DMA_MAP`/`SYS_IRQ_ATTACH`, `_start` now retrieves the 3 deposited tokens via `find_token`, issues real `MmioMap (70)` + `DmaMap (71)` + `IrqAttach (72)` syscalls with manifest-pinned BAR addresses (virtio-net `0xFEBC_0000/0x1000`, NVMe `0xFEBF_0000/0x4000`, e1000e `0xFEB0_0000/0x20000`) and only then advances the bring-up FSM through its remaining pure-state phases. Sentinel exit codes (`EXIT_NO_MMIO/DMA/IRQ_TOKEN = 10/20/30`, `EXIT_MMIO/DMA/IRQ_BASE + errno = 40+ / 60+ / 80+`) distinguish standalone execution (no deposit) from loader-rejected tokens. Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.10 driver-shared SDK` (cyan), Next=`P6.7.9 virtio-net live bring-up`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`935 workspace pass`. Acceptance gates clean: workspace clippy + bare-metal target + mb12-userprobe + kernel-runner + 3 driver-image siblings (target `x86_64-unknown-none --release`) + fmt + check-no-blanket-allow (16 crate-root files, +1 vs P6.7.8.9) + rustdoc bare-metal + rustdoc workspace + lint-oips (0 error / 0 warning su 19 file). P6.7.9 (virtio-net live bring-up + Proxmox smoke) ora sbloccato. Continues the P6.7.8 driver implementation arc; P6.7.8.9 capability deposit trampoline wired 2026-05-20; P6.7.8.7 e1000e bootable image sibling chiuso 2026-05-20; P6.7.8.6 e1000e M2 scaffold chiuso 2026-05-20; P6.7.8.5 NVMe image sibling chiuso 2026-05-20). Phase 0 chiusa (P0/P1/P2 ✅; P3/P4 parziali per dipendenze esterne — funding/cryptographer). Track A desktop ✅ (M1-M5 + M3b). Track B kernel ✅ MB1-MB14 (ciclo formalmente completato MB14.a-h.2). **PR #36** merged in `main` come `85293b8` + CHANGELOG bump `69a1dd5` + tag annotated firmato SSH `v0.3.0-alpha.1` su `69a1dd5` + **GitHub pre-release** https://github.com/CySalazar/omni/releases/tag/v0.3.0-alpha.1 con SBOM CycloneDX `omni-os-sbom.json` (89 KiB) auto-attached dal workflow `sbom.yml`. Validato hardware end-to-end su Proxmox VMID 103 (8 logical CPU, OVMF q35) — sequenza completa `[mb14.a]` → `[mb14.h.2]` + `[virtio] tablet ready` clean, Build Info panel renderizzato sul GOP framebuffer. Include il **VirtIO BAR mapping fix** (`virtio_tablet::ensure_mmio_page_mapped`) che risolve una regressione latente dal v0.2.0 (BAR 64-bit prefetchable a > 4 GiB phys non coperto dal direct-map della bootloader 0.11). **Prossimo blocco di lavoro: P6.7 user-space driver model** (NVMe + Ethernet/Wi-Fi + TEE backend) — Phase 1 closure deliverable per `docs/06-roadmap.md`. Riepilogo MB14 sotto-blocchi: MB14.a + MB14.b + MB14.c.1 + MB14.c.2.a + MB14.c.2.b.1 + MB14.c.2.b.2 + MB14.c.2.c + MB14.c.2.d + MB14.d + MB14.e + MB14.f + MB14.g + MB14.h.1 + **MB14.h.2** (cross-CPU context switch: `PerCpu` esteso con `in_scheduler: AtomicBool` + `enter_scheduler`/`leave_scheduler`/`is_in_scheduler` — per-CPU recursion guard che sostituisce la BSP-only `scheduling::IN_SCHEDULER` per bare-metal MP path; nuovo `scheduling::SCHED_LOCK` + `try_acquire_sched_lock`/`release_sched_lock` — coarse cross-CPU spinlock che serializza le mutazioni di `SCHEDULER`; nuovo `tss::set_rsp0_for_cpu(cpu_id, rsp0) -> bool` che route BSP/AP per scrivere il TSS sibling corretto; `kernel_ap_dispatch_observe` promosso da observer (pop+drop) a **live dispatcher** (`SCHEDULER.yield_current` bracketed dal lock pair: per-CPU guard + global SCHED_LOCK con release ordering lock-first / guard-second); `lapic.rs::kernel_check_need_resched` BSP branch ora simmetrico al path AP; `scheduling.rs::yield_current` route TSS write via `set_rsp0_for_cpu(current_cpu().cpu_id(), kstk)`; smoke boot-time `[mb14.h.2] sched_lock=ok per_cpu_in_sched=ok set_rsp0_for_cpu=ok`; +7 host test; ADR-0010 `accepted`) ✅ chiuso 2026-05-20. **Ciclo MB14 formalmente chiuso (MB14.a-h.2).** Roadmap Phase 1 ~97% Track B.
> **Last updated:** 2026-05-21 (P6.7.9-pre.7 closure — IOMMU per-device attach surface landed. `bare_metal::iommu` gains the vendor-neutral `PciBdf(u16)` newtype + `IommuBackend::{attach_device, detach_device}` trait surface (routed through `IommuKind` dispatch, default `Ok(())` for `PassthroughBackend`); `VtdBackend` + `AmdViBackend` track per-device attachments in `Vec<{Vtd,AmdVi}Attachment>` with duplicate-attach → `IommuError::Unsupported` and detach-on-unknown → `IommuError::Unsupported`. Module-level `iommu_attach_device(bdf, domain)` / `iommu_detach_device(bdf)` close the host-testable surface. **Live MMIO install** (bare-metal-only): `VtdBackend::install_device_entry(phys_offset, bdf, domain, slpt_phys, context_table_phys, AddressWidth, TranslationType)` writes the spec-faithful 128-bit context entry into the per-bus context table at `context_entry_offset(bdf) = devfn * 16`, the root entry into the global root table at `root_entry_offset(bus) = bus * 16`, then submits per-domain context-cache + per-domain IOTLB invalidate descriptors (new `encode_context_cache_domain_invalidate(domain)` + `encode_iotlb_domain_invalidate(domain)` encoders, granularity G=10 + DID in bits 16..31 per VT-d spec § 6.5.2) on the invalidation queue via the new `submit_iq_descriptor` helper (wraps on `INV_QUEUE_BYTES`). `AmdViBackend::install_device_entry(phys_offset, bdf, domain, iopt_phys, IommuFlags, PageMode)` writes the 256-bit DTE into the device table at `dte_offset(bdf) = bdf.raw() * 32` (bounds-checked against `DEVICE_TABLE_BYTES = 4096`), then submits `INVALIDATE_DEVTAB_ENTRY(device_id=bdf.raw())` + `INVALIDATE_IOMMU_PAGES(domain)` (new `encode_invalidate_iommu_pages_domain(domain)` encoder, opcode `0x3` + DID in bits 32..47 + S=1) on the command-buffer ring via the new `submit_cmd_descriptor` helper. `VtdAttachError` (5 variants: `NotActivated/DomainNotInstalled/AlreadyAttached/AddressMisaligned/InvalidationTimeout`) + `AmdViAttachError` (7 variants — adds `DeviceTableTooSmall/UnsupportedMode`) both map to `IommuError` via `From` impls. Module-level wrappers `#[cfg(target_os = "none")] install_vt_d_device_entry(...)` + `install_amd_vi_device_entry(...)` mirror the existing `activate_intel_vt_d` / `activate_amd_vi` pair. New MMIO helpers `write_context_entry_at` / `write_root_entry_at` (VT-d) + `write_dte_at` (AMD-Vi). +33 host-side tests covering the dispatch surface end-to-end. Workspace test count 1128 → **1161 pass / 0 fail**. Build Info Active=`P6.7.9-pre.7 IOMMU device wire`, Next=`P6.7.9-pre.8 driver PCI bind`, Tests=`1161 workspace pass`. All acceptance gates clean. **No new dependency**. `kmain` deliberately **NOT** wired — live install requires the driver framework's cap-token resource match, lands in pre.8. `GCMD.TE` (VT-d) and `CTRL.IommuEn` (AMD-Vi) stay deasserted preserving Phase-1 pass-through semantics. Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`). Previous: 2026-05-21 (P6.7.9-pre.4 closure — DMA-Map vendor switch wired. `bare_metal/iommu/mod.rs` gained `IommuKind` enum (`Passthrough/Intel/Amd`) implementing `IommuBackend` via static-dispatch match arms; kernel-wide `IOMMU_BACKEND: spin::Mutex<IommuKind>` static initialised via `const fn` to `Passthrough`; `install_backend_for_vendor(vendor)` swaps the live variant after `kmain`'s `iommu::probe`; `with_iommu_backend(|b| …)` accessor brackets the mutex around a closure receiving `&mut IommuKind`; `domain_for_task(task_id) -> DomainId` projects the kernel `TaskId` into the 16-bit VT-d DID / AMD-Vi `DomainID` space. `dma_map_handlers::dma_map` calls `b.install_domain(domain_id)` after cap validation, then `b.map(domain_id, iova_base, phys_base, len, flags)` + `b.flush(domain_id)` after the contiguous PTE install, with `IommuFlags` derived from the `direction` argument. `tear_down_dma_mappings(task)` calls `b.unmap` + `b.flush` per recorded mapping. `VtdBackend::new` + `AmdViBackend::new` promoted to `pub const fn`. +14 host-side test covers the dispatch surface end-to-end. Workspace test count 1064 → **1078 pass / 0 fail**. Build Info Active=`P6.7.9-pre.4 DMA-Map vendor switch`, Next=`P6.7.9-pre.5 IOMMU register programming`, Tests=`1078 workspace pass`. All acceptance gates clean. **No new dependency** (`spin = "0.9"` was already a kernel dep). Note: scaffolds still emit zero MMIO bytes — live register programming is the next slice. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`). Previous (intermediate, doc-lagged): 2026-05-21 (P6.7.9-pre.3 AMD-Vi backend scaffold — commit `8e719f5`; new `crates/omni-kernel/src/bare_metal/iommu/amdvi.rs` ~1237 lines; +34 host-side tests; workspace test count 1030 → 1064 pass; Build Info Active=`P6.7.9-pre.3 AMD-Vi backend`, Next=`P6.7.9-pre.4 DMA-Map vendor switch`, Tests=`1064 workspace pass`). Previous: 2026-05-21 (P6.7.9-pre.2 closure — Intel VT-d backend scaffold landed dormant. New module `crates/omni-kernel/src/bare_metal/iommu/vtd.rs` (1064 lines) pins Intel VT-d spec rev 4.1 § 10.4 register offsets + § 9 data-structure encoders + a host-testable `VtdBackend` that implements `IommuBackend` without writing any MMIO byte. The slice splits the live register programming (deferred to P6.7.9-pre.4 — DMA-Map vendor switch) from the pure-data encoding work so the QEMU + Proxmox smoke harnesses can assert on the encoder output without any silicon side effect. Surface: 16 `pub const u32` register offsets (`REG_OFFSET_VER=0x00`, `REG_OFFSET_CAP=0x08`, `REG_OFFSET_ECAP=0x10`, `REG_OFFSET_GCMD=0x18`, `REG_OFFSET_GSTS=0x1C`, `REG_OFFSET_RTADDR=0x20`, `REG_OFFSET_CCMD=0x28`, `REG_OFFSET_FSTS=0x34`, `REG_OFFSET_FECTL=0x38`, `REG_OFFSET_FEDATA=0x3C`, `REG_OFFSET_FEADDR=0x40`, `REG_OFFSET_FEUADDR=0x44`, `REG_OFFSET_PMEN=0x64`, `REG_OFFSET_IQH=0x80`, `REG_OFFSET_IQT=0x88`, `REG_OFFSET_IQA=0x90`); 9 `pub const u32` GCMD/GSTS bit positions (`TE=bit31`, `SRTP=bit30`, `SFL=bit29`, `EAFL=bit28`, `WBF=bit27`, `QIE=bit26`, `IRE=bit25`, `SIRTP=bit24`, `CFI=bit23`); 3 size constants (`ROOT_ENTRY_BYTES=16`, `CONTEXT_ENTRY_BYTES=16`, `SLPTE_BYTES=8`); pure-function encoders `encode_root_entry(ctp_phys) -> Result<RootEntry, VtdError>` + `encode_root_entry_absent() -> RootEntry`; `encode_context_entry(slpt_phys, domain, translation, width) -> Result<ContextEntry, VtdError>` + `encode_context_entry_absent() -> ContextEntry`; `encode_slpte(phys, flags) -> Result<Slpte, VtdError>` with `IommuFlags::{READ, WRITE, EXECUTE, COHERENT}` → VT-d `R/W/X/SNP` bit translation (write-only entries auto-promoted to RW to match VT-d malformed-entry semantics); CAP decoders `cap_domain_count(cap) -> u32` (formula `1 << (4 + 2 * ND)` saturated to 16-bit space), `cap_supported_agaw(cap) -> u8` (SAGAW bitmask bits 8..12), `cap_caching_mode(cap) -> bool` (bit 7), `pick_highest_supported_agaw(mask) -> Option<AddressWidth>` (preferred order 5L→4L→3L→2L); 2 enums `TranslationType::{UntranslatedOnly, UntranslatedAndTranslated, Passthrough}` and `AddressWidth::{Bits30Level2, Bits39Level3, Bits48Level4, Bits57Level5}` with `levels() -> u8` accessor; data types `RootEntry { low, high }`, `ContextEntry { low, high }` with `slptptr()/domain_id()/translation_type_raw()/address_width_raw()` accessors, `Slpte(u64)` with `is_present()/output_address()` accessors + `BIT_READ/WRITE/EXECUTE/SNOOP` constants; `VtdError::{AddressMisaligned, UnknownDomain, UnsupportedFlags}` taxonomy + `impl From<VtdError> for IommuError`; `ScaffoldMapping { domain, iova, phys, len, leaf_slpte }` host-introspection record; `VtdBackend { domains, mappings }` struct with `new()/mappings()/has_domain()/domains()` + full `IommuBackend` impl that records calls without emitting MMIO. Test surface: 32 host-side tests (`register_offsets_match_intel_spec_4_1`, `gcmd_bits_are_top_of_32_bit_word`, `encode_root_entry_sets_present_bit_and_ctp`, `encode_root_entry_rejects_misaligned_ctp`, `encode_root_entry_absent_is_all_zero`, `encode_context_entry_round_trips_did_and_aw`, `encode_context_entry_passthrough_keeps_t_field`, `encode_context_entry_rejects_misaligned_slpt`, `encode_context_entry_absent_is_all_zero`, `address_width_levels_match_spec`, `encode_slpte_read_only`, `encode_slpte_write_forces_read_bit`, `encode_slpte_execute_and_coherent`, `encode_slpte_rejects_misaligned_phys`, `encode_slpte_zero_flags_emits_not_present`, `cap_domain_count_known_values`, `cap_domain_count_caps_at_16_bit_space`, `cap_supported_agaw_extracts_bits_8_to_12`, `cap_caching_mode_extracts_bit_7`, `pick_highest_supported_agaw_prefers_57_then_48_then_39_then_30`, `pick_highest_supported_agaw_returns_none_for_zero_mask`, `vtd_backend_vendor_reports_intel`, `vtd_backend_install_domain_is_idempotent`, `vtd_backend_map_rejects_unknown_domain`, `vtd_backend_map_records_mapping_with_encoded_slpte`, `vtd_backend_map_rejects_misaligned_arguments`, `vtd_backend_unmap_removes_record`, `vtd_backend_unmap_unmapped_range_returns_error`, `vtd_backend_unmap_rejects_unknown_domain`, `vtd_backend_flush_rejects_unknown_domain`, `vtd_backend_flush_known_domain_is_ok`, `vtd_error_into_iommu_error_mapping`). Workspace test count 998 → **1030 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). Module registration: `crates/omni-kernel/src/bare_metal/iommu/mod.rs` extended with `pub mod vtd;` (4 submodules now: `dmar`, `domain`, `ivrs`, `vtd`). No new dependency: pure `core` + `alloc::vec::Vec`. No `kmain` wiring — the boot-time `iommu::probe` continues to write the selected vendor to the `IOMMU_VENDOR` atomic for telemetry, while `dma_map_handlers::dma_map` still uses `PassthroughBackend`; the `iommu_vendor()` → `VtdBackend` swap lands in P6.7.9-pre.4. Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.2 VT-d backend` (cyan), Next=`P6.7.9-pre.3 AMD-Vi backend`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`1030 workspace pass`. Acceptance gates: `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal,mb12-userprobe -- -D warnings` clean. `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-net-virtio-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-nvme-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-e1000e-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo fmt --all -- --check` clean (only nightly-feature stable-channel warnings). `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`. `RUSTDOCFLAGS="-D warnings" cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` clean. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` clean. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)`. P6.7.9-pre.3 (AMD-Vi `amdvi.rs` sibling scaffold, identical shape — IVRS-driven register offsets + device-table encoder + dormant `AmdViBackend`) prossimo. Note: TEE backend (OIP-016) resta funding-dep. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`). Previous: 2026-05-21 (P6.7.8.10 closure — driver-shared SDK landed end-to-end per OIP-013 § S5.3 step 8 + the 3 driver-image siblings rewired with live `MmioMap`/`DmaMap`/`IrqAttach` syscalls. New crate `crates/omni-driver-shared` (workspace member, dep-free, `no_std`): exposes `DRIVER_CAP_DEPOSIT_VA = 0x0010_0000` + `DRIVER_CAP_DEPOSIT_LEN = 0x8000` + `MAX_ENTRIES = 64` + `ACTION_TAG_{MMIO_MAP=1, DMA_MAP=2, IRQ_ATTACH=3, PCI_CFG_READ=4, PCI_CFG_WRITE=5}` constants + `OmniCapsHeader { magic, version, entry_count }` repr(C) header layout + `OmniCapsError::{InvalidMagic, UnsupportedVersion, EntryCountExceeded, OutOfBoundsOffset}` typed errors + `caps::find_token(action_tag, resource_predicate) -> Option<&'static [u8]>` production entry point (reads from the kernel-mapped static VA) + `caps::find_token_in_buf(buf, action_tag, resource_predicate) -> Option<&[u8]>` pure-function variant for host tests + private `caps::parse_header(buf) -> Result<u32, OmniCapsError>` + `caps::scan_entries(buf, count, action_tag, predicate)` shared inner loop + private `caps::read_u32_le(buf, offset) -> Option<u32>` bounds-checked LE reader. The crate's `_start` lookup contract is documented in `crates/omni-driver-shared/README.md`. Test surface: 36 host-side unit tests (10 header-parser cases + 10 find_token cases — locates known action_tag, returns None on unknown, OOB offset/len reject, empty deposit, predicate-filter-selects-correct, multiple-action-types-distinguishes, full-64-entry scan, u32::MAX-action-tag rejection, exact-buffer-end fit, zero-length token, first-match-on-duplicates, descriptor-table-OOB — + 4 OmniCapsError Display + 5 adversarial / security tests (all-0xFF buffer, token_offset=u32::MAX, token_len=u32::MAX, empty buffer, 15-byte truncated) + 2 proptest harnesses (`find_token_in_buf_is_idempotent` on 0..512-byte buffers and `proptest_no_panic_on_arbitrary_buf_up_to_deposit_len` on 0..=32 KiB buffers — no-panic invariant guaranteed) + 5 `OmniCapsError::Debug` non-empty checks). E2E test surface: 8 e2e tests in `crates/omni-driver-shared/tests/e2e_deposit_round_trip.rs` (`#[cfg(not(target_os = "none"))]`-gated): `deposit_round_trip_mmio_map_token` + `deposit_round_trip_predicate_filter_selects_correct_token` + `deposit_round_trip_dma_map_token` + `deposit_round_trip_irq_attach_token` + `deposit_round_trip_mixed_actions_returns_correct_type` + `deposit_empty_page_find_returns_none` — each builds a synthetic OMNICAPS page with real Ed25519-signed `CapabilityToken` blobs minted via `OmniSigningKey::from_bytes` + `CapabilityToken::sign_payload` + `wire::encode_canonical`, runs `find_token_in_buf`, asserts the returned slice round-trips through `wire::decode_canonical::<CapabilityToken>` back to equal-to-original; the predicate-filter test exercises content-aware selection by `phys_base` discriminator. Doc-test surface: 12 doc tests across the constants + Display + find_token + find_token_in_buf examples. **Live wiring of the 3 driver-image siblings** (`crates/omni-driver-{net-virtio,nvme,e1000e}-image/src/main.rs`): each `_start` ELF entry now reads the kernel-deposited tokens via `find_token(ACTION_TAG_MMIO_MAP, |_| true)` + `find_token(ACTION_TAG_DMA_MAP, |_| true)` + `find_token(ACTION_TAG_IRQ_ATTACH, |_| true)`, then issues 3 real `syscall5` invocations against `MmioMap (70)` + `DmaMap (71)` + `IrqAttach (72)` with manifest-pinned BAR addresses (virtio-net `VIRTIO_BAR4_PHYS_BASE = 0xFEBC_0000` + `VIRTIO_BAR4_LEN = 0x1000`; NVMe `NVME_BAR0_PHYS_BASE = 0xFEBF_0000` + `NVME_BAR0_LEN = 0x4000`; e1000e `E1000E_BAR0_PHYS_BASE = 0xFEB0_0000` + `E1000E_BAR0_LEN = 0x20000`) + a 4 GiB DMA arena (`DMA_IOVA_BASE = 0x0`, `DMA_LEN_4_GIB = 0x1_0000_0000`, `DMA_DIR_BIDIR = 2`) + a placeholder IRQ vector (virtio-net `IRQ_LINE = 33`, NVMe `IRQ_LINE = 34`, e1000e `IRQ_LINE = 35`, `IPC_CHANNEL_PLACEHOLDER = 0`). The `#[allow(dead_code)]` gates on `syscall5`, `SYS_MMIO_MAP`, `SYS_DMA_MAP`, `SYS_IRQ_ATTACH` are removed — all four symbols are now live. Only after the 3 syscalls return `errno = 0` does `_start` advance the `BringUp` FSM through its remaining pure-state phases via `Event::Advance`. TaskExit sentinel codes: `EXIT_OK=0` (FSM converged to terminal-success Phase), `EXIT_FSM_FAILED=1` (any other terminal), `EXIT_NO_MMIO/DMA/IRQ_TOKEN=10/20/30` (standalone execution — no deposit page), `EXIT_MMIO/DMA/IRQ_BASE + errno = 40+/60+/80+` (loader-rejected token). Each driver-image's `Cargo.toml` extended with `omni-driver-shared = { path = "../omni-driver-shared" }` as a path dependency (no version pin — same-repo; no transitive supply-chain). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.10 driver-shared SDK` (cyan), Next=`P6.7.9 virtio-net live bring-up`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`935 workspace pass` (was 893 pre-doc-test inclusion; the new count uniformly reflects `cargo test --workspace --all-features -- --test-threads=1` aggregate including doc-tests; +42 from `omni-driver-shared` alone: 36 unit + 8 e2e + 12 doc + 2 proptest harnesses, with overlap due to harness count semantics). `scripts/check-no-blanket-allow.sh` extended to scan 16 crate-root files (+1 — `crates/omni-driver-shared/src/lib.rs` enrolled). Acceptance gates: `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal,mb12-userprobe -- -D warnings` clean. `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-net-virtio-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-nvme-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo clippy --manifest-path crates/omni-driver-e1000e-image/Cargo.toml --target x86_64-unknown-none --release -- -D warnings` clean. `cargo fmt --all -- --check` clean (only nightly-feature stable-channel warnings). `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`. `RUSTDOCFLAGS="-D warnings" cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` clean. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` clean. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)`. **P6.7.8 series formally closed** — all 11 sub-steps (P6.7.8.0/1/2/3/4/5/6/7/8/9/**10**) chiusi. **P6.7.9 (live driver bring-up + Proxmox smoke for virtio-net/NVMe/e1000e) ora sbloccato.** Phase 1 ~99.9% (the residual 0.1% covers P6.7.9 live bring-up + the funding-dep TEE/IOMMU/audit items). Note: TEE backend (OIP-016) resta funding-dep. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`). Previous: 2026-05-20 (P6.7.8.9 closure — capability deposit trampoline wired end-to-end per OIP-013 § S5.3 step 8. Three new kernel modules: `entropy.rs` (Phase-1 CSPRNG `RDRAND+RDTSC` mix → `ChaCha20Rng` global `spin::Mutex<Option<KernelCsprng>>` + Phase-2 `add_entropy`/`reseed` API designed not wired), `driver_cap_issuer.rs` (static `0xCAFEBABE × 8` DEV-ONLY Ed25519 signing seed; production TEE-derived sealing key deferred to P5.2 + OIP key-custody), `cap_deposit.rs` (`OMNICAPS` flat indexed wire-format header + entry table + packed postcard `CapabilityToken` blobs → 8-page 32 KiB read-only window at VA `0x0010_0000`). `driver_load_handlers::driver_load` extended: after `spawn_from_elf` succeeds, mint signed tokens for every `DriverCapabilities` entry (`mmio_regions/dma_windows/irq_lines/pci_devices`) with `subject = NodeId(provider.node_id_bytes())`, 90-day `TimeWindow`, then map + populate 8 pages in driver AS via direct-map `copy_nonoverlapping`. `ProcessControlBlock.cap_deposit_va: Option<u64>` records the install. Bypassed `omni-capability/mint` feature entirely (`CapabilityId::from_bytes` + `sign_payload` unconditional → no bare-metal `getrandom` pulled). New deps: `rand_core 0.6` + `rand_chacha 0.3` + `spin 0.9` (all `default-features = false`). Build Info: Active=`P6.7.8.9 cap deposit trampoline` (cyan), Next=`P6.7.8.10 driver-shared SDK`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`885 workspace pass` (era 867; +19 nuovi: 7 `entropy` + 3 `driver_cap_issuer` + 9 `cap_deposit`). All gates clean: clippy workspace + bare-metal + mb12-userprobe + kernel-runner + 3 driver-image siblings; fmt; check-no-blanket-allow (15 files); rustdoc bare-metal + workspace; lint-oips (0 error / 0 warning). Spike doc `docs/plans/p6-7-8-9-cap-deposit-trampoline.md`. P6.7.8.10 (`omni_driver_shared::caps::find_token` SDK helper + driver crate rifattorizzazione) prossimo. Previous: 2026-05-20 (P6.7.8.8 closure — `DriverLoad (73)` syscall handler wired end-to-end per OIP-013 § S5.3. `crates/omni-kernel/src/bare_metal/syscall_entry.rs` now exposes a `driver_load_handlers` module (gated `#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]`) whose `driver_load(args)` routes through the rich two-register path: postcard `CapabilityToken` decode + `Ed25519CapabilityProvider::verify_signed_token` + `Action::DriverLoad` + `Resource::Any` resource pin → user-→-kernel pack copy (Vec<u8>, ≤ `MAX_PACK_BYTES = 32 MiB`) → `decode_omni_pack` envelope + `postcard_decode_manifest` body + `hydrate_manifest` reconstruction → full `verify_manifest` (BLAKE3 hash + KNOWN_ISSUERS lookup + Ed25519 sig) → `ProcessControlBlock::spawn_from_elf(image_bytes, PhysAddr(boot_pml4), &mut mapper, alloc, sched, PriorityClass::System, KernelPrincipal::ZERO)`. New `bare_metal::BOOT_CR3` AtomicU64 + `set_boot_cr3`/`boot_cr3` accessors (one-shot publish in `kmain` right after `arch::read_cr3()`; same shape as `PHYS_OFFSET`) gives the syscall handler the boot PML4 it needs to clone the kernel-half into the new driver process's address space (the calling process's CR3 is the loader's, not the kernel image). Errno mapping via `manifest_errno`: `MalformedPack/PackTooLarge/ImageHashMismatch → EINVAL`, `UnknownIssuer/SignatureInvalid → EACCES`; spawn failure → `ENOSPC`. Dispatcher wiring: `SyscallNumber::DriverLoad` moved from the `NotYetImplemented` tail arm to the `MmioMap | DmaMap | IrqAttach` legacy group (returns `CapabilityDenied` as a defensive sentinel for single-register fallback); `dispatch_full` adds a `DriverLoad` arm routing to `driver_load_handlers::driver_load` on bare-metal or `SyscallReturn::err(EACCES)` on the host build. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.8 DriverLoad syscall` (cyan), Next=`P6.7.8.9 cap deposit trampoline`, Phase=`1 - Microkernel POC  (~99.8%)`, Tests=`867 workspace pass`. +3 nuovi host-side test (`bare_metal::boot_cr3_tests::set_boot_cr3_masks_low_12_bits` + `boot_cr3_returns_zero_when_unset_observer` + `set_boot_cr3_round_trips_aligned_value`). Existing dispatcher tests adjusted (1:1 rename + arm-set widening — no net change). Workspace test count 864 → **867 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). All gates clean: clippy workspace + bare-metal target + mb12-userprobe + kernel-runner + 3 driver-image siblings (target `x86_64-unknown-none --release`); fmt; check-no-blanket-allow (15 crate-root files); rustdoc bare-metal + workspace; lint-oips (0 error / 0 warning su 19 file). Token-deposit trampoline (OIP-013 § S5.3 step 8 — pre-install attenuated child tokens in the driver's initial capability namespace at well-known user-VA slots before the first dispatch tick) deliberately deferred to P6.7.8.9: drivers spawned in P6.7.8.8 reach `_start` but the `MmioMap`/`DmaMap`/`IrqAttach` calls inside them still require a separately-presented token (the FSM bring-up paths in the 3 driver-image siblings remain `#[allow(dead_code)]` against `syscall5` until P6.7.8.9 lands). Note: TEE backend (OIP-016) resta funding-dep e non sblocca prima di P6.7.8.9. Previous: 2026-05-20 (P6.7.8.7 closure — `crates/omni-driver-e1000e-image` bootable Ring 3 ELF sibling landed, workspace-excluded sibling pattern simmetrico a `omni-driver-nvme-image` ↔ `omni-driver-nvme` (P6.7.8.5), `omni-driver-net-virtio-image` ↔ `omni-driver-net-virtio` (P6.7.8.3) e `kernel-runner` ↔ `omni-kernel`. `Cargo.toml` `no_std + no_main` per `x86_64-unknown-none` (profile release `lto=true` `codegen-units=1` `opt-level=z` `panic=abort` `strip=debuginfo`); unica dep `omni-driver-e1000e` via path; `.cargo/config.toml` minimo eredita workspace rustflags. `src/main.rs` espone `_start` `#[unsafe(no_mangle)] pub extern "C" fn` che (a) costruisce `BringUp::new()` parked at `Phase::PciEnumeration`, (b) avanza la FSM 13-step via `Event::Advance` fino al terminale, (c) `sys_exit(0)` su `Phase::Ready` else `sys_exit(1)`, (d) `syscall5` helper inline-asm pre-cablato per `MmioMap (70)` / `DmaMap (71)` / `IrqAttach (72)` gated `#[allow(dead_code)]` finché il DriverLoad trampoline deposita i tokens (P6.7.8.8+). Allocator stub `PanicOnAlloc` defensive che panica su qualunque heap call (la FSM è `Copy` quindi nessuna alloc attesa a runtime). Panic handler chiama `sys_exit(2)`. Syscall numbers `SYS_TASK_EXIT=11` / `SYS_WRITE_CONSOLE=60` / `SYS_MMIO_MAP=70` / `SYS_DMA_MAP=71` / `SYS_IRQ_ATTACH=72` pinned localmente per evitare circular workspace dep. Root `Cargo.toml` `[workspace.exclude]` esteso con il nuovo crate (commento OIP-015 § S5 + § S8 + OIP-013 § S5.3 step 9). `.gitignore` esteso con `/crates/omni-driver-e1000e-image/target/`. Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.7 e1000e image sibling` (cyan), Next=`P6.7.8.8 DriverLoad syscall wire`, Phase=`1 - Microkernel POC (~99.7%)`, Tests=`864 workspace pass`. Workspace test count invariato a 864 (il sub-step non aggiunge host-side test — il crate è bare-metal-only ed eredita la FSM dalla libreria già testata in P6.7.8.6). All gates clean: clippy workspace + bare-metal + mb12-userprobe + kernel-runner + net-virtio-image + nvme-image + e1000e-image (target `x86_64-unknown-none --release`); fmt; check-no-blanket-allow (15 crate-root files); rustdoc bare-metal + workspace; lint-oips (0 error / 0 warning su 19 file). Build artifact ELF `target/x86_64-unknown-none/release/omni-driver-e1000e-image` (1896 B) generato clean. P6.7.8.8 (DriverLoad (73) syscall handler wiring — sostituire `NotYetImplemented` stub con omni-pack v1 envelope decode + Ed25519 signature verification + BLAKE3 image hash check + `spawn_from_elf` invocation per qualunque dei 3 driver image disponibili) prossimo. Note: TEE backend (OIP-016) resta funding-dep e non sblocca prima di P6.7.8.8. Previous: 2026-05-20 (P6.7.8.6 closure — `crates/omni-driver-e1000e` scaffold added as a full workspace member per OIP-Driver-Net-015 § S5 + § S8 and the Intel 82574L Gigabit Ethernet Controller datasheet § 10. Five `pub` modules lock the e1000e-side surface without any syscall wiring: (a) `pci_ids` — Intel vendor `0x8086` + five v0.3 device IDs (82574L `0x10D3`, I217-LM `0x153A`, I217-V `0x153B`, I218-LM `0x15A1`, I219-LM `0x15A3`), `is_e1000e_device(vendor, device) -> bool` matcher; (b) `controller_regs` — CSR offsets `CTRL/STATUS/MDIC/ICR/ITR/IMS/IMC/RCTL/TCTL/RDBAL..RDT/TDBAL..TDT/RAL0/RAH0` + field encodings (`CTRL.RST=bit26`, `RAH.AV=bit31`, `RCTL/TCTL` enable composers) + 128 KiB `CSR_REGION_BYTES` + `const _: () = assert!(...)` layout invariants; (c) `ring_config` — power-of-two ring depth bounds (1..=4096) + `rx_buffer_count` (1..=8192) + 16-byte legacy descriptors + 2 KiB RX buffer size + validators with `checked_mul`/`checked_add` overflow guards; (d) `interrupts` — IMS/IMC/ICR bit positions `TXDW=bit0`, `LSC=bit2`, `RXT0=bit7` + `ENABLED_IMS=0x85` (OIP-015 § S5.1 step 10 mandate) + `icr_has_*` classifiers; (e) `bringup` — 13-step FSM `PciEnumeration → MmioMap → DisableInterrupts → GlobalReset → ReadMac → PhyInit → SetupRxRing → PostRxBuffers → SetupTxRing → ConfigureRxTx → EnableInterrupts → AttachIrq → RegisterNetChannel → Ready` + `Failed=14` + `BringUp { phase, retries }` with `MAX_RETRIES=3` budget mirrored from P6.7.8.3 / P6.7.8.4 + `Event::{Advance, Retry, Abort(BringUpError)}` + 10 `BringUpError` variants + `StepKind` projection. New `crates/omni-driver-e1000e/manifest.toml` developer-authored TOML v1 template with shape OIP-013 § S5.1 + OIP-015 § S1 (`[meta]` / `[capabilities]` 128 KiB MMIO + 4 GiB IOVA / `[matchers]` listing all five vendor/device pairs / `[net]` block matching `ring_config` defaults). Root `Cargo.toml` `[workspace.members]` extended (`crates/omni-driver-e1000e`). `scripts/check-no-blanket-allow.sh` `SCOPED_CRATES` extended (14 → 15 crate-root files scanned). Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.6 e1000e (M2) scaffold` (cyan), Next=`P6.7.8.7 e1000e image sibling`, Phase=`1 - Microkernel POC (~99.6%)`, Tests=`864 workspace pass`. +61 new host-side tests (7 pci_ids + 10 controller_regs + 7 interrupts + 15 ring_config + 22 bringup). Workspace test count 803 → **864 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). All gates clean: clippy workspace + bare-metal target + mb12-userprobe + kernel-runner + net-virtio-image + nvme-image (all `x86_64-unknown-none --release`); fmt; check-no-blanket-allow (15 crate-root files); rustdoc bare-metal + workspace; lint-oips (0 error / 0 warning su 19 file). P6.7.8.7 (e1000e bootable image sibling sotto `crates/omni-driver-e1000e-image/` excluded come kernel-runner / net-virtio-image / nvme-image) prossimo.) Previous: 2026-05-20 (P6.7.8.5 closure — `crates/omni-driver-nvme-image` bootable Ring 3 ELF sibling landed, workspace-excluded sibling pattern simmetrico a `omni-driver-net-virtio-image` ↔ `omni-driver-net-virtio` e `kernel-runner` ↔ `omni-kernel`. `Cargo.toml` `no_std + no_main` per `x86_64-unknown-none` (profile release `lto=true` `codegen-units=1` `opt-level=z` `panic=abort` `strip=debuginfo`); unica dep `omni-driver-nvme` via path; `.cargo/config.toml` minimo eredita workspace rustflags. `src/main.rs` espone `_start` `#[unsafe(no_mangle)] pub extern "C" fn` che (a) costruisce `BringUp::new()`, (b) avanza la FSM 13-step via `Event::Advance` fino al terminale, (c) `sys_exit(0)` su `Phase::Ready` else `sys_exit(1)`, (d) `syscall5` helper inline-asm pre-cablato per `MmioMap (70)` / `DmaMap (71)` / `IrqAttach (72)` gated `#[allow(dead_code)]` finché il DriverLoad trampoline deposita i tokens (P6.7.8.x). Allocator stub `PanicOnAlloc` defensive che panica su qualunque heap call (la FSM è `Copy` quindi nessuna alloc attesa a runtime). Panic handler chiama `sys_exit(2)`. Syscall numbers `SYS_TASK_EXIT=11` / `SYS_WRITE_CONSOLE=60` / `SYS_MMIO_MAP=70` / `SYS_DMA_MAP=71` / `SYS_IRQ_ATTACH=72` pinned localmente per evitare circular workspace dep. Root `Cargo.toml` `[workspace.exclude]` esteso con il nuovo crate (commento OIP-014 § S6 + OIP-013 § S5.3 step 9). `.gitignore` esteso con `/crates/omni-driver-nvme-image/target/`. Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.5 NVMe image sibling` (cyan), Next=`P6.7.8.6 e1000e (M2) driver`, Phase=`1 - Microkernel POC (~99.5%)`, Tests=`803 workspace pass`. Workspace test count invariato a 803 (il sub-step non aggiunge host-side test — il crate è bare-metal-only ed eredita la FSM dalla libreria già testata in P6.7.8.4). All gates clean: clippy workspace + bare-metal + mb12-userprobe + kernel-runner + nvme-image (target `x86_64-unknown-none --release`); fmt; check-no-blanket-allow (14 crate-root files); rustdoc bare-metal + workspace; lint-oips (0 error / 0 warning su 19 file). Build artifact ELF `target/x86_64-unknown-none/release/omni-driver-nvme-image` generato clean. Smoke validation hardware Proxmox VMID 103 (`100.101.77.9`, 8 vCPU q35 OVMF + swtpm TPM 2.0 + 4 GiB RAM): boot UEFI image via `cargo +nightly run --manifest-path disk-image/Cargo.toml` (2.06 MiB), scritta sul zvol `vm-103-disk-6` via `dd bs=1M conv=fsync`, VM avviata. Serial log mostra sequenza completa `[mb14.a]` → `[mb14.h.2]` + `[elf] probe OK` + `[virtio] tablet ready`, zero errori / zero panic / zero #PF / zero warning. Screenshot VNC (`qm monitor screendump`) confermato: Build Info panel renderizzato sul GOP framebuffer 1280×800 con commit hash `4b875fd` + Active/Next/Phase/Tests aggiornati. P6.7.8.6 (e1000e M2 — Ethernet bare-metal, primo driver non-virtio) prossimo.) Previous: 2026-05-20 (P6.7.8.4 closure — `crates/omni-driver-nvme` scaffold added as full workspace member per OIP-014 § S1-S6; 5 `pub` modules `pci_ids` + `controller_regs` + `queue_config` + `transfer_model` + `bringup` lock the NVMe-side surface (PCI class triple, NVMe 1.4 register offsets, admin/IO queue bounds, PRP-only transfer model, 13-step bring-up FSM) without any syscall wiring; +68 host-side tests; workspace test count 737 → **803 pass / 0 fail**; Build Info panel Active=`P6.7.8.4 NVMe driver scaffold`, Next=`P6.7.8.5 NVMe bring-up wire`. P6.7.8.5 (NVMe bring-up FSM wired to real MmioMap/DmaMap/IrqAttach inside `crates/omni-driver-nvme-image/`) prossimo.) Previous: 2026-05-20 (P6.7.8.3 closure — `DmaMap (71)` + `IrqAttach (72)` syscall handlers wired end-to-end + extended virtio-net bring-up FSM with `BringUp/Event/StepKind` API and retry budget + new workspace-excluded sibling crate `crates/omni-driver-net-virtio-image/` (`no_std + no_main`, target `x86_64-unknown-none`) that hosts the bootable Ring 3 ELF the kernel ingests via `DriverLoad`. `crates/omni-kernel/src/bare_metal/syscall_entry.rs` extended with `dma_map_handlers` + `irq_attach_handlers` modules + `omni_irq_dispatch_trampoline` asm stub + `kernel_irq_dispatch_handler` Rust callback; `crates/omni-kernel/src/process.rs` extended with `DmaMapping`/`IrqAttachment` + `pcb.dma_mappings`/`pcb.irq_attachments` fields; teardown wired in `task_exit`. `crates/omni-kernel/src/bare_metal/lapic.rs` adds `read_in_service_vector` (xAPIC MMIO + x2APIC MSR variants). `crates/omni-kernel/src/ipc.rs` adds read-only `ipc_registry()` accessor. Build Info panel: Active=`P6.7.8.3 virtio-net bring-up` (cyan), Next=`P6.7.8.4 NVMe driver scaffold`, Phase=`1 - Microkernel POC (~99.3%)`, Tests=`737 workspace pass`. Workspace test count 716 → **737 pass / 0 fail**. All gates clean: clippy workspace + bare-metal + mb12-userprobe + kernel-runner + driver-image; fmt; check-no-blanket-allow; rustdoc bare-metal + workspace; lint-oips. P6.7.8.4 (NVMe driver scaffold) prossimo. Old P6.7.8.2 entry retained below for history.)

> **Previous:** 2026-05-20 (P6.7.8.2 closure — `crates/omni-driver-net-virtio` skeleton aggiunto al workspace come full member (lib `no_std + alloc`, `#![cfg_attr(not(test), no_std)]`). Cinque moduli `pub`: `pci_ids` (Red Hat vendor `0x1AF4` + modern `0x1041` / legacy `0x1000` da virtio 1.0 § 4.1.2.1 + OIP-015 § S4); `device_status` (constants `RESET=0x00`, `ACKNOWLEDGE=0x01`, `DRIVER=0x02`, `DRIVER_OK=0x04`, `FEATURES_OK=0x08`, `DEVICE_NEEDS_RESET=0x40`, `FAILED=0x80` da virtio 1.0 § 2.1 + bit-disjointness invariant + `0x0F` live-status anchor); `features` (`VIRTIO_F_VERSION_1` bit 32 + `VIRTIO_NET_F_MAC` bit 5 + `VIRTIO_NET_F_STATUS` bit 16 = `REQUIRED_FEATURES` mandatory floor OIP-015 § S4.1 step 4, opzionali `VIRTIO_NET_F_CSUM` bit 0, `VIRTIO_NET_F_MRG_RXBUF` bit 15); `virtqueue` (`RX_QUEUE_IDX=0` / `TX_QUEUE_IDX=1` + default depth 256 + descriptor/avail/used ring sizes con helper `is_valid_queue_depth`/`descriptor_table_bytes`/`avail_ring_bytes`/`used_ring_bytes` defence-in-depth via `checked_mul`/`checked_add`); `bringup` (`Phase` enum `#[repr(u8)]` con sette stati spec-anchored `Reset → Acknowledge → FeatureNegotiation → FeaturesLocked → VirtqueueSetup → MacAcquired → DriverOk` + terminal `Failed`, pure-function `next/is_terminal/is_live`; transition tables + actual MmioMap/DmaMap/IrqAttach syscall invocations deliberatamente deferred a P6.7.8.3). Nuovo `crates/omni-driver-net-virtio/manifest.toml` developer-authored TOML v1 template (consumato offline da `omni-driver-pack` per generare omni-pack v1 blob loadable da `DriverLoad`) shape OIP-013 § S5.1 + OIP-015 § S1 (`[meta]` / `[capabilities]` / `[matchers]` / `[net]`). `scripts/check-no-blanket-allow.sh` esteso con il nuovo crate (12 → 13 crate-root files scanned). Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.2 virtio-net scaffold` (cyan), Next=`P6.7.8.3 virtio-net bring-up`, Phase=`1 - Microkernel POC (~99.0%)`, Tests=`716 workspace pass`. +24 nuovi host-side test (4 pci_ids + 3 device_status + 4 features + 8 virtqueue + 5 bringup). Workspace test count 692 → **716 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --features bare-metal --target x86_64-unknown-none -- -D warnings` + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` tutti clean. P6.7.8.3 (bring-up state-machine wiring + bootable image sibling `crates/omni-driver-net-virtio-image/` excluded sibling-style come kernel-runner ↔ omni-kernel) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).)

> **Previous:** 2026-05-20 (P6.7.8.1 closure — `MmioMap` syscall handler (`SyscallNo = 70`) wired end-to-end per OIP-013 § S2. Sostituzione completa del `NotYetImplemented` stub con (a) two-register ABI `(rax = va_base, rdx = POSIX errno)` via `SyscallReturn` `#[repr(C)]` + `SyscallDispatcher::dispatch_full` default-impl extension + `kernel_syscall_dispatch` C-ABI change (asm stubs preservano `rdx` da `call` a `sysretq`/`iretq` — invariante validata istruzione per istruzione); (b) nuovo `crate::syscall::syscall_errno::{EACCES, EFAULT, EINVAL, ENOSPC, ENOSYS}` POSIX-aligned constants; (c) nuovo `crates/omni-kernel/src/kaslr.rs` con `KaslrRng` SplitMix64 + `seed_from_hw` (RDRAND cpuid-gated 10 retries → RDTSC + monotonic counter fallback per VM senza RDRAND, p.es. nested KVM); (d) nuovo `bare_metal::PHYS_OFFSET: AtomicU64` + `set_phys_offset`/`phys_offset` accessor wirato in `kmain` early-init; (e) `ProcessControlBlock` esteso con `mmio_mappings: Vec<MmioMapping>` + `mmio_va_cursor: u64` per lifecycle tracking; (f) `RoundRobinScheduler::process_mut` accessor; (g) handler `mmio_map_handlers::mmio_map(args)` bare-metal-only che esegue (1) input validation con EINVAL su misalign/reserved-bit, EFAULT su cap_ptr/cap_len out-of-user-half, ENOSYS su WC flag (PAT non ancora wirato), (2) copy postcard token (≤ 1024 byte) su kernel stack buffer, (3) `omni_types::wire::decode_canonical::<CapabilityToken>` + `Ed25519CapabilityProvider::verify_signed_token` (signature + time window + TEE binding via `placeholder()` provider) + `is_driver_framework_action` guard + `Action::MmioMap` exactness + `Resource::MmioRegion` subset-contains range check, (4) lazy KASLR del cursore per-process nel driver-MMIO PML4 slot `[0x0000_0080_0000_0000, 0x0000_0100_0000_0000)` (512 GiB, OIP-013 Appendix B amendment 2), (5) linear bump del cursor + ENOSPC su `va_end > DRIVER_MMIO_VA_END`, (6) page-by-page install via `AddressSpace::map_user_4k` con flags `PRESENT|WRITABLE|USER|NX|PCD|PWT` (uncached default), (7) `invlpg` per ogni page sull'active CR3 del caller, (8) record `MmioMapping { va_base, len_pages }` su `pcb.mmio_mappings`, (9) return `SyscallReturn::ok(va_base)` o `SyscallReturn::err(errno)`; (h) rollback page-by-page con `unmap_4k` su `map_user_4k` failure (no frame ritornato al frame allocator, MMIO è device-owned); (i) `tear_down_mmio_mappings(task_id)` wired in `task_exit` PRIMA di `sched.dequeue(current)` per OIP-013 § S2.4 (unmap leaf PTEs + invlpg + drain `pcb.mmio_mappings` Vec + reset cursor). +13 nuovi host-side test: `kaslr::tests::{from_seed_is_deterministic, next_u64_advances_state, distinct_seeds_diverge_quickly, new_avoids_zero_state, seed_from_hw_changes_on_subsequent_calls}`, `syscall::tests::{syscall_return_ok_zero_errno, syscall_return_err_zero_rax, syscall_return_is_two_u64_struct, syscall_errno_codes_are_posix_aligned}`, `process::tests::{fresh_pcb_has_empty_mmio_table, mmio_mappings_round_trip}`, `bare_metal::syscall_entry::tests::{dispatcher_mmio_map_returns_capability_denied_on_legacy_arm, dispatcher_full_mmio_map_surfaces_eaccess_on_host}`. Workspace test count 679 → **692 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --features bare-metal --target x86_64-unknown-none -- -D warnings` + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` tutti clean. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.1 MmioMap syscall live` (cyan), Next=`P6.7.8.2 virtio-net crate scaffold`, Phase=`1 - Microkernel POC (~98.8%)`, Tests=`692 workspace pass`. P6.7.8.2 (virtio-net crate scaffold sotto `crates/omni-driver-net-virtio`) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).)

> **Previous:** 2026-05-20 (P6.7.8.0 closure — `postcard` decoder per `DriverManifestV1` wired. `crates/omni-kernel/src/driver_manifest.rs`: nuovo wire-type `DriverManifestBody` + `DriverMetaBody` (postcard-serializable, omette la firma per spezzare la circolarità sign-over-self); `DriverManifest::body()` proietta il manifest sul body firmabile; `hydrate_manifest(body, signature)` ricostruisce il manifest in-memory dopo il decode dell'omni-pack envelope; `postcard_decode_manifest` ora chiama `omni_types::wire::decode_canonical::<DriverManifestBody>` (no-trailing-bytes invariant per `OIP-Serde-004`); `verify_manifest` ora costruisce il signing payload via `encode_canonical(&manifest.body())` invece dell'encoder handcrafted (TAG_MMIO/DMA/IRQ/PCI byte + length-prefixed encoding ritirato — sostituito dal canonical postcard di `OIP-Serde-004` come mandato da OIP-013 § S5.3 step 5); rimosso `DriverManifestError::ParserNotWired` + 5 helper handcrafted (push_lenprefixed_str, push_string_vec, push_resource_vec, encode_resource, push_pci_matchers) + 4 TAG_* constants. `DriverCapabilities/DriverMatchers/PciMatcher` ora derivano `Serialize, Deserialize`; `DriverManifest/DriverMeta` deliberatamente NO (la firma `[u8; 64]` non ha serde impl built-in; il wire path è `DriverManifestBody`). +5 host-side test (`postcard_round_trip_manifest_body_preserves_fields`, `postcard_decode_manifest_rejects_trailing_bytes`, `postcard_decode_manifest_rejects_truncated_input`, `postcard_decode_manifest_rejects_empty_input`, `body_round_trip_through_omni_pack_envelope` end-to-end omni-pack → postcard → hydrate); test handcrafted-encoder ritirato (`signing_payload_encodes_capability_subset` riscritto come `signing_payload_grows_with_capabilities` invariant length-based + nuovo `signing_payload_omits_signature_field` che documenta esplicitamente che due manifest con firme diverse hanno signing payload identico). Workspace test count **674 → 679 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` + `python3 scripts/lint-oips.py` tutti clean. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.0 postcard manifest dec` (cyan), Next=`P6.7.8.1 MmioMap syscall handler`, Phase=`1 - Microkernel POC (~98.5%)`, Tests=`679 workspace pass`. P6.7.8.1 (MmioMap syscall handler — sostituire `NotYetImplemented` stub con cap-check + page-table mapping nel driver VA range 512 GiB KASLR) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).)
>
> **Previous:** 2026-05-20 (P6.7.7 closure — OIP-014/015/016 promossi `Draft → Active` in single-pass editorial transition via founder fast-path. `oips/oip-driver-nvme-014.md`, `oips/oip-driver-net-015.md`, `oips/oip-driver-tee-016.md` aggiornati: frontmatter `status: Draft → Active` + `activated: 2026-05-20`; aggiunta `## Appendix A — Bootstrap Activation Note` a ciascun OIP con rationale (dependency unblock OIP-013 in `Active`, zero deployment risk al filing time perché Reference Implementation N/A, scope bounded da OIP-013 normativo, no conflict con OIP-013 Appendix B amendments) e re-ratification obligation per `OIP-Process-001 §5.5.e` al termine della Bootstrap Period. La transizione `Draft → Review → Last Call → Active` collassa in un singolo editorial pass sotto `OIP-Process-001 §5.5` (Solo Founder Fast-Track) esercitato dalla Bootstrap Period authority `§6.3`; 14-day public objection window di `§5.3` waived. `oips/README.md` index aggiornato (3 righe per 014/015/016 → `Active *(founder fast-path 2026-05-20)*`). Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.7 OIP 014/015/016 Active` (cyan), Next=`P6.7.8 virtio-net (M1) driver`. Nessun cambio al codice runtime; il commit è doc-only — workspace test count invariato a **674 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)` clean. `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` tutti clean. Stato P6.7: P6.7.0/1/2/2.bis/3/3.bis/4/5/6/7 chiusi; **P6.7.8 (first-party driver implementations) ora completamente sbloccato** — sequenza prevista virtio-net (M1, QEMU/Proxmox-validabile senza hardware fisico) → NVMe → e1000e (M2 bare-metal) → TEE (funding-dep). Phase 1 ~98%. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).

> **Previous:** 2026-05-20 (P6.7.3 framework skeleton closure — OIP-013 promosso `Last Call → Active` via founder fast-path (deroga al §5.3 14-day window, approvazione esplicita del founder; `activated: 2026-05-20` in frontmatter; Appendix A documenta la rinumerazione editoriale dei syscall 22-25 → 70-73 per evitare collisione con MB12 IPC v0.1-locked, OIP-016 a cascata 26-27 → 74-75); P6.7.3 deliverables landed: `omni-capability/src/scope.rs` +8 Action variants + 4 Resource variants (`#[non_exhaustive]` preserved; subset semantics byte-exact su PciDevice/IrqLine + range-contained su MmioRegion/DmaWindow con u128 widening anti-wrap), `omni-kernel/src/syscall.rs` + `bare_metal/syscall_entry.rs` numbers 70-75 + ENOSYS stub handlers + dispatcher route, nuovo modulo `omni-kernel/src/driver_manifest.rs` (schema types + parse_manifest stub + verify_manifest con BLAKE3 image hash + Ed25519 signature verification + known_issuers lookup), nuovo modulo `omni-kernel/src/known_issuers.rs` (static allowlist vuoto in Phase 1), nuova direct-dep `omni-crypto bare-metal` nel kernel Cargo.toml, `scripts/lint-oips.py` esteso con optional key `activated`, Build Info panel aggiornato (Active=`P6.7.3 framework skeleton`, Next=`P6.7.7 promote 014/015/016`, Phase=`98%`, Tests=`665`). +23 host-side test (11 scope + 3 syscall_entry + 7 driver_manifest + 2 known_issuers). Workspace test count 645 → **665 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` + `python3 scripts/lint-oips.py` tutti clean. P6.7.0/1/2/2.bis/3/4/5/6 chiusi; P6.7.7 (014/015/016 promotion) ora sbloccato (OIP-013 in `Active` soddisfa i `requires: [13]`); P6.7.8 (first-party driver implementations) sequenziale: virtio-net (M1) → NVMe → e1000e (M2) → TEE (funding-dep).

> **Previous:** 2026-05-20 (P6.7.0 closure — `OIP-Driver-Framework-013` `Draft` filed in `oips/oip-driver-framework-013.md` (832 lines). Standards Track. Lock kernel-side contract per 5 superfici: (1) capability scope extensions `Action::{MmioMap, DmaMap, IrqAttach, PciConfigRead, PciConfigWrite, DriverLoad, DriverUnload}` + `Resource::{PciDevice, MmioRegion, DmaWindow, IrqLine}`, tutti `#[non_exhaustive]` per preservare il postcard wire-format `OIP-Serde-004`; subset semantics byte-exact su `PciDevice`/`IrqLine` e range-contained su `MmioRegion`/`DmaWindow`; token minting via `Ed25519CapabilityProvider` con `not_after ≤ 90 days`. (2) `MmioMap` syscall (`SyscallNo = 22`, ABI a 5 arg + cap_len su stack) con bounds-check su `[phys_base, phys_base+len)` contro `Resource::MmioRegion` del token; mapping in driver-VA range `0x0000_0080_0000_0000..0x0000_0080_8000_0000` (2 GiB, 1 PML4 slot offset dal MB11 user-stack VA); flags PTE = `PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_NX | (PCD|PWT default uncached / PAT WC opt-in)`; teardown automatico al process exit. (3) DMA / IOMMU: domain-per-driver (1 IOMMU domain per driver process; nessun PASID-tagged sharing in Phase 1); `DmaMap` syscall (`SyscallNo = 23`) con direction in/out/bidi; vendor backends `iommu::vtd` (DMAR) + `iommu::amdvi` (IVRS); hard-fail su assenza IOMMU (`ENOSYS`). (4) IRQ routing: `IrqAttach` syscall (`SyscallNo = 24`) con shared-IOAPIC-line rejection (`EBUSY`, deliberato vs Linux fan-out per determinismo); MSI/MSI-X vector alloc in `0x40..0xFE`; coalescing su `IPC_DRIVER_IRQ_DEPTH = 64` + `missed_count` per-canale; CPU affinity locked a **BSP only** per v0.3 (per-driver affinity differita post-MB14.h.2-equivalent multi-CPU dispatch validation). (5) Driver manifest TOML v1 schema + Ed25519 signed `(meta + capabilities + matchers)` block; `DriverLoad` syscall (`SyscallNo = 25`) con verifica atomica BLAKE3(image) vs `meta.omni_image_hash` + signature vs `meta.omni_issuer_pubkey` + `KNOWN_ISSUERS` static table baked at compile time (no TOFU). Rationale: Phase 1 deliverable esplicito in `docs/06-roadmap.md`; cinque bug forecluded inclusa la VirtIO BAR > 4 GiB regressione di v0.2.0; P6.8 audit sbloccato. Backwards Compatibility N/A (first introduction). Test Cases TC1-TC6 + hardware smoke deferred a `OIP-Driver-NVMe-XXX`. Reference Implementation N/A at filing (framework-side patches branch TBD `feat/kernel-p6-7-driver-framework`). Security Considerations: alignment con C-1..C-5 adversary classes; threat C-3 (compromised driver) bounded by IOMMU + capability scope; cryptographic primitives reuse `omni-crypto` audited surface. Privacy Considerations: data minimization via channel-name registry; no broadcast-to-all-drivers; manifest-declared scope auditable. Index `oips/README.md` aggiornato (riga 013). `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 16 file(s)`. Build Info panel aggiornato: Active=`P6.7 drafting OIP-013`, Next=`P6.7 NVMe driver scaffold`. Nessuna change al codice runtime; il drafting è doc-only — workspace test count invariato a 645 pass / 0 fail).

> **Previous:** 2026-05-20 (MB14.h.2 closure: (1) `PerCpu` esteso con `in_scheduler: AtomicBool` + `enter_scheduler() -> bool` (CAS `false→true`, `AcqRel`/`Acquire`) + `leave_scheduler()` (Release store) + `is_in_scheduler()` (Acquire peek). (2) Nuovo `scheduling::SCHED_LOCK: AtomicBool` + `try_acquire_sched_lock() -> bool` + `release_sched_lock()`. (3) Nuovo `tss::set_rsp0_for_cpu(cpu_id, rsp0) -> bool` che route BSP (cpu_id=0 → `set_rsp0` legacy TSS) vs AP (`AP_TSS[cpu_id-1].rsp0`), out-of-range → false. (4) `kernel_ap_dispatch_observe` promosso a live dispatcher: bracketing per-CPU `enter_scheduler` + global `try_acquire_sched_lock`, pop dalla per-CPU run-queue, bump del counter (long-lived diagnostic), `SCHEDULER.yield_current`, release ordering (lock first / guard second). (5) `kernel_check_need_resched` BSP branch ora simmetrico al path AP: per-CPU guard + SCHED_LOCK con bail-on-fail. (6) `scheduling.rs::yield_current` route TSS write via `set_rsp0_for_cpu(current_cpu().cpu_id(), kernel_stack_top)`. (7) Smoke boot-time `kmain` MB14.h.2 esercita le 3 API host-side: SCHED_LOCK mutual exclusion + `enter_scheduler` round-trip + `set_rsp0_for_cpu(0, _)` delegate vs out-of-range reject. (8) Build Info panel aggiornato: Active=`MB14.h.2 cross-CPU ctx swap`, Next=`MB14 PR + v0.3.0-alpha.1`, Track B=`MB1-MB13 OK, MB14.a-h.2 wip`, Phase 1 ≈ 97%, Tests=`650+ workspace pass`. +7 host-side test (3 in `per_cpu::tests::*` + 3 in `tss::tests::*` + 1 di chiusura). Workspace test count 639+ → **645 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` tutti clean. ADR-0010 (`docs/adr/0010-mb14h2-cross-cpu-context-switch.md`) `accepted`. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover (item §4.5 #16). Branch corrente `feat/kernel-mb11-userspace`. **MB14 chiuso — PR su `main` + tag `v0.3.0-alpha.1` ora sbloccato.**).
>
> **Previous:** 2026-05-20 (MB14.h.1 closure: (1) `PerCpu` esteso con `dispatch_observations: AtomicU64` + `inc_dispatch_observation`/`dispatch_observations` accessor (Release/Acquire, field-position post-`need_resched` per preservare il `gs:[0]` self-pointer invariant MB14.b). (2) Nuovo modulo `crates/omni-kernel/src/bare_metal/ap_dispatch.rs` con `kernel_ap_dispatch_observe()` `extern "C"`: legge `current_cpu()`, short-circuit su BSP (defence-in-depth — il BSP resta sul cooperative `yield_current` path), chiama `per_cpu_run_queue::pop_for_cpu_with_stealing(cpu_id)` e su `Some(_)` incrementa il counter e droppa l'id (observer-mode discard — MB14.h.2 rimpiazzerà il discard con `SCHEDULER.yield_current`). (3) `kernel_check_need_resched` branch AP ora invoca `ap_dispatch::kernel_ap_dispatch_observe()` dopo aver consumato il `need_resched` flag (prima il branch AP solo drenava il flag e tornava). (4) `kmain` smoke boot-time post-MB14.g block, guarded su `per_cpu::registered_ap_count() > 0`: enqueue sentinel `0xEEEEE4` su `cpu_id=1` via `per_cpu_run_queue::enqueue_on_cpu`, busy-poll `ap_slot(1).dispatch_observations()` per 200 M iter (≈1 s su silicio moderno), log `[mb14.h.1] ap_dispatch observed=N (ok | timeout — AP did not observe)`; su VM single-CPU log `[mb14.h.1] ap_dispatch BSP-only — no AP enrolled` short-circuit. (5) `bare_metal::mod.rs` registra il submodule `ap_dispatch` fra `address_space` e `arch`. (6) Build Info panel aggiornato: Active=`MB14.h.1 AP observe loop`, Next=`MB14.h.2 cross-CPU ctx switch`, Track B=`MB1-MB13 OK, MB14.a-h.1 wip`, Phase 1 ≈ 96%, Tests=`639+ workspace pass`. +4 host-side test (3 in `per_cpu::tests::*` su default-zero/monotonic/per-descriptor isolation + 1 in `ap_dispatch::tests::*` host-stub callable). Workspace test count 635+ → 639+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` + `bash scripts/check-no-blanket-allow.sh` + `cargo fmt --all -- --check` clean. ADR-0009 (`docs/adr/0009-mb14h-ap-dispatch-loop.md`) `accepted` cattura il design observer-mode end-to-end + le safety invariant + il sub-step sequencing per MB14.h.2 (SCHEDULER serialisation, per-CPU `IN_SCHEDULER`, `set_rsp0_for_cpu`, IST sharing). Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover (item §4.5 #16 in progress-omni.md; mitigato via `--test-threads=1` che produce 332-pass clean). Branch corrente `feat/kernel-mb11-userspace`; PR verso `main` resta release-management decision separata).

> **Previous:** 2026-05-20 (MB14.f closure: (1) Nuovo entry-point `bare_metal::lapic::kernel_ap_lapic_init` `extern "C"` `#[unsafe(no_mangle)]` invocato da `kmain_ap` global_asm step 8 (subito dopo `ltr`, prima di `lock inc AP_ONLINE_ACK`); la funzione legge `X2APIC_MODE` AtomicBool seedato dal BSP a `lapic_init` time, se attivo flippa `IA32_APIC_BASE` bit 10+11 sul MSR per-CPU della propria CPU (la flip del BSP non propaga), poi chiama `program_lapic_local(mode)` che scrive SIVR (`LAPIC_ENABLE | 0xFF`), TPR=0, LVT timer (periodic vector `0x20`), timer divider+initial-count — gli AP ora servono ogni Fixed-delivery IPI (incluso 0xFD TLB shootdown). (2) `LapicMode::{XApic, X2Apic}` enum + `detect_lapic_mode()` (legge `IA32_APIC_BASE` bit 10) + `is_x2apic_enabled()` accessor. `lapic_init` ora osserva la modalità del firmware e stamp `X2APIC_MODE`; non flippa il bit a runtime (decisione conservativa: il bootloader-mapped MMIO window resta valido per tutto il boot path). (3) `lapic_eoi`, `lapic_send_ipi`, `lapic_icr_busy`, `read_lapic_id` ora gated su `X2APIC_MODE`: x2APIC dispatcha agli MSR `IA32_X2APIC_EOI`/`ICR`/`APICID` (MSR `0x80B`/`0x830`/`0x802`); xAPIC mantiene il path MMIO byte-per-byte. (4) `kmain_ap` global_asm step 1 promosso da CPUID leaf 1 EBX[31:24] (8-bit) a leaf `0xB` sub-leaf 0 EDX (32-bit, identico in xAPIC mode dove EBX[31:24] zero-extended == EDX, ma cattura LAPIC ID > 255 in x2APIC mode). (5) `kernel_lapic_timer_tick` + `kernel_check_need_resched` gated su `current_cpu().is_bsp()`: AP timer ora EOI early senza toccare il `static mut TICK_COUNT` (BSP-only writer) né `NEED_RESCHED` globale (race-free). (6) Build Info panel aggiornato: Active=`MB14.f AP LAPIC + x2APIC`, Next=`MB14.g AP dispatch + PR`, Track B=`MB1-MB13 OK, MB14.a-f wip`, Tests=`592+ workspace pass`. Boot log post-LAPIC init aggiunge `[mb14.f] lapic_mode=xAPIC|x2APIC`. ADR-0008 (`docs/adr/0008-mb14f-per-cpu-scheduling-protocol.md`) `accepted` cattura il design completo (chiude formalmente l'MB14.e.4 follow-up). +6 host-side test in `bare_metal::lapic::tests::*` pin MSR addresses vs Intel SDM Vol 3A Table 10-6 + `MSR = 0x800 + (mmio_offset >> 4)` algebra. Workspace test count 586+ → 592+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` clean. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover (item §4.5 #16 in progress-omni.md). Branch corrente `feat/kernel-mb11-userspace`; PR verso `main` resta release-management decision separata).

> **Previous:** 2026-05-19 (MB14.e closure: (1) `bare_metal::mp_ap_entry::kmain_ap` global_asm step 10 modificato da `cli; hlt; jmp $-2` a `sti; hlt; jmp $-2` — gli AP ora servono il vector `0xFD` TLB shootdown e ogni futura IPI per-CPU appena escono dall'init sequence. (2) Nuovo modulo `bare_metal::per_cpu_run_queue` con `[CpuSlot; MAX_CPUS = 32]` (BSP cpu_id=0 + `MAX_AP_SLOTS = 31`); ogni `CpuSlot` ha `SpinLock` (AtomicBool CAS Acquire/Release + spin loop con relaxed-read backoff) + `UnsafeCell<PerCpuRunQueue>` con `[Vec<u64>; NUM_PRIORITY_CLASSES = 6]`. API: `enqueue_on_cpu(cpu_id, task_id, prio) -> bool`, `pop_for_cpu(cpu_id) -> Option<u64>`, `steal_from(victim_cpu) -> Option<u64>`, `pop_for_cpu_with_stealing(cpu_id) -> Option<u64>`, `local_len(cpu_id) -> usize`. (3) Work-stealing protocol: la pop con stealing prima drena la local queue, poi scansiona ogni altro slot in `cpu_id` order e ruba dalla **back of the lowest-priority non-empty queue** (preserva priority ordering globale + minimizza cache-line ping-pong sul head del victim). Self è skipped durante lo scan. (4) Smoke boot-time in `kmain` post-`sti`: enqueue + pop locale sul BSP slot con sentinel `0xEEEEE2`, poi steal-fallback da `cpu_id=1` con sentinel `0xEEEEE3`, log `[mb14.e] per_cpu_run_queue local=ok steal=ok`. Suffix di `[mb14.d] tlb_shootdown` aggiornato da `(IRR queued; APs service post-MB14.e sti)` a `(all APs acked)` / `(timeout — AP ISR did not ack)`. (5) +12 host-side test in `bare_metal::per_cpu_run_queue::tests::*` (FIFO/priority/round-trip + 6 stealing protocol incluse self-skip, prefer-local, all-empty). Workspace test count 564+ → 576+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` clean; `cargo fmt --all -- --check` clean. Build Info panel aggiornato: Active=`MB14.e per-CPU run-queue`, Next=`MB14.f x2APIC LAPIC>255`, Track B=`MB1-MB13 OK, MB14.a-e wip`, Phase 1 ≈ 93%, Tests=`586+ workspace pass`. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover (item §4.5 #16 in progress-omni.md). Branch corrente `feat/kernel-mb11-userspace`; PR verso `main` resta release-management decision separata).

> **Previous:** 2026-05-19 (MB14.d closure: tre nuovi moduli `bare_metal::ipi`, `bare_metal::tlb_shootdown`, `mm` (crate-root facade). (1) `ipi::broadcast_all_except_self(vector) -> IcrCommand` + `ipi::fixed_to_apic_id(vector, apic_id)` pure-function builders + `ipi::send_to_all_except_self(0xFD) -> bool` / `ipi::send_to_apic_id(0xFD, apic_id) -> bool` LAPIC-MMIO front-ends (riusano `mp::encode_icr_xapic` + `lapic_send_ipi`). (2) `tlb_shootdown::TLB_SHOOTDOWN_VECTOR = 0xFD` + global `Shootdown` static (AtomicU64 `va_start` + `page_count` + `generation`, AtomicUsize `ack`) + `flush_tlb_range(VirtAddr, u64) -> ShootdownReport` BSP entry-point: invlpg locale (cap `SHOOTDOWN_MAX_PAGES = 64`, promozione a full-flush sentinel `u64::MAX` con CR3 self-reload oltre la cap), poi `Release` stores su va_start/page_count + `fetch_add(1, Release)` su generation, raise IPI tramite `send_to_all_except_self`, busy-poll `ack` fino a `>= registered_ap_count()` o `ACK_POLL_ITERATIONS = 200M` (≈1 s su silicio moderno). (3) Nuovo modulo `mm` (crate root) che re-esporta `flush_tlb_range`, `invalidate_local`, `ShootdownReport`, `TLB_SHOOTDOWN_VECTOR`, costanti `SHOOTDOWN_*` così i call-site dicono `mm::flush_tlb_range(...)` per allinearsi all'idioma kernel Linux/seL4. (4) `kernel_tlb_shootdown_handler` Rust callback invocata dall'asm trampoline `omni_tlb_shootdown_handler` (stesso pattern di `omni_lapic_timer_handler`: 9 push caller-saved → call → 9 pop → iretq); legge `generation` (Acquire), poi `page_count`/`va_start` Relaxed, esegue `invlpg` per pagina (o full-flush via `mov cr3, cr3` se `page_count > SHOOTDOWN_MAX_PAGES`), `lapic_eoi()`, `ack.fetch_add(1, Release)`. (5) Hook IDT in `idt_init` installa `0xFD = omni_tlb_shootdown_handler` come interrupt gate al kernel CS. (6) Hook in `kmain` post-`sti` enable: chiama `mm::flush_tlb_range(VirtAddr(0x8000), 0x1000)` e logga `[mb14.d] tlb_shootdown vector=0xFD targeted=N acked=M local_pages=P` + suffix `(BSP-only — no broadcast)` o `(all APs acked)` o `(IRR queued; APs service post-MB14.e sti)` — quest'ultimo è il caso atteso oggi perché gli AP parkano in `cli; hlt`. +14 host-side test (4 in `ipi::tests::*` byte-exact opcode pin di shorthand AllExcludingSelf vs NoShorthand + 10 in `tlb_shootdown::tests::*` per descriptor round-trip / handler ack-increment / page-count round-up / vector pin / full-flush sentinel). Workspace test count 538+ → 552+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean; `cargo fmt --all -- --check` clean per i file toccati. Build Info panel aggiornato: Active=`MB14.d TLB shootdown IPI`, Next=`MB14.e per-CPU run-queue`, Phase 1 ≈ 91%. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib --features bare-metal` resta carryover (item §4.5 #16 in progress-omni.md). Branch corrente `feat/kernel-mb11-userspace`; PR verso `main` resta release-management decision separata).

> **Previous:** 2026-05-19 (MB14.c.2.c closure: due nuovi moduli `bare_metal::mp_ap_entry` + `bare_metal::pit_delay`. (1) `mp_ap_entry::build_ap_landing_stub(tramp_base) -> [u8; 32]` pure-function emette `F0 48 FF 04 25 <ack32>` (lock inc qword [ack]) + `48 8B 0C 25 <cr3_32>` (mov rcx, [kernel_cr3 slot]) + `48 8B 14 25 <va32>` (mov rdx, [kmain_ap_va slot]) + `0F 22 D9` (mov cr3, rcx) + `FF E2` (jmp rdx) — entrambi i load avvengono PRIMA del CR3 switch; il post-switch instruction fetch resta valido perché il c.2.b.2 emplacement identity-mappa phys `0x8000` anche nel kernel CR3. `kmain_ap` definito via `global_asm!` (toolchain 1.85 non stabilizza `#[naked]`): `cli; 1: hlt; jmp 1b`. (2) `pit_delay::pit_delay_us(us)` bare-metal: PIT channel 2 mode 0 (interrupt on terminal count) via port `0x43`/`0x42`, gata via `0x61` bit 0, polla bit 5 per terminal-count; 1.193 MHz base fissa indipendente da CPU frequency. Speaker data (bit 1) sempre mascherato. (3) `lapic::lapic_send_ipi(low, high) -> bool` pub: drena `ICR_LO` bit 12 → write `ICR_HI` (`0x310`) → write `ICR_LO` (`0x300`) → latch+fire. (4) `mp::start_aps_live(topology, bsp_apic_id, trampoline_page, phys_offset) -> StartApsLiveReport`: per ogni enabled non-BSP AP fa INIT assert + busy-drain + `pit_delay_us(10_000)` (10 ms post-INIT settle per Intel MP-Spec § B.4) + SIPI + busy-drain + `pit_delay_us(200)` + SIPI + busy-drain + `pit_delay_us(200)`, poi busy-polla `read_ack_counter(phys_offset)` fino a `acked >= targeted` o esaurimento di `AP_ACK_POLL_ITERATIONS` (1 G iterazioni ≈ 1 s su silicio moderno). (5) `mp_emplacement::place_trampoline_live(allocator, mapper, kernel_cr3, kmain_ap_va)` wraps `place_trampoline` con `kernel_ap_entry = TRAMP_BASE + AP_LANDING_STUB_OFFSET`, poi emplaza landing stub + zera ack counter + scrive kernel CR3 e kmain_ap VA nei runtime slot. (6) Hook in `kmain` post-MB14.c.2.b.2 sostituito: quando `enabled_count() > 1` chiama `place_trampoline_live(fa, &mut pager, cr3_raw & !0xFFF, kmain_ap as u64)` + `start_aps_live(&topo, lid, TRAMPOLINE_SIPI_VECTOR=0x08, phys_offset_mb2)` + logga `[mb14.c.2.c] start_aps_live targeted=N sequenced=N acked=N (all APs online|timeout)`. +17 host-side test (10 in `mp_ap_entry::tests::*` byte-exact opcode pin + 7 in `pit_delay::tests::*` tick math). Workspace test count 521+ → 538+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. ADR-0007 `accepted` cattura design completo del ciclo MB14.c (.a + .b.1 + .b.2 + .c) e roadmap MB14.c.2.d (real per-CPU init). Branch corrente `feat/kernel-mb11-userspace`; PR verso `main` resta release-management decision separata).

> **Previous:** 2026-05-19 (MB14.c.2.b.2 closure: nuovo modulo `bare_metal::mp_emplacement` con `place_trampoline(allocator, mapper, kernel_ap_entry) -> Result<EmplacedTrampoline, EmplacementError>`. Tre frame allocati dal `BitmapFrameAllocator<16384>` ospitano la temp PML4 / PDPT / PD; i contenuti `build_temp_identity_paging(pdpt, pd)` sono scritti via `phys_offset + paddr` direct-map (`core::ptr::write_volatile` per ogni u64 entry); la pagina trampolino è identity-mappata in active CR3 via `PageMapper::map_4k(VirtAddr(0x8000), PhysAddr(0x8000), PTE_PRESENT | PTE_WRITABLE, alloc)` con check idempotente preliminare (`translate(0x8000) == Some(0x8000)` → no-op); il blob da `build_trampoline_blob(0x8000, pml4_u32, 0xFFFF_FFFF_8010_0000)` è copiato a phys `0x8000` byte-per-byte volatile. +11 host-side test in `bare_metal::mp_emplacement::tests::*`. Workspace test count 510+ → 521+. ADR-0007 writing deferred to MB14.c.2.c closure).
> **Storia stati precedenti:** 2026-05-18 (MB12 closure — IPC + multi-task user, ADR-0005, 426 tests). 2026-05-18 (Step 7.1-7.4 lift blanket allows + ADR-0003 + CI `blanket-allow-guard`). 2026-05-18 (MB11 closure — Ring 3 + per-process CR3, ADR-0004). 2026-05-18 (MB10 closure — kernel stack isolation, ADR-0002, PR #33 in `main`). 2026-05-18 (v0.2.0 release — MB1-MB9 + Track A, PR #29 in `main`). 2026-05-16 (MB4/MB5). 2026-05-15 (K5 QEMU smoke gate). 2026-05-12 (scaffolding pass P3-P6 verificato). 2026-05-10 (P1 + P2 chiusi). 2026-05-09 (P0 chiuso).
> **Owner:** cySalazar (`cySalazar@cySalazar.com`) — Lead Architect / BDFL (5y)
> **Priority order:** Security → Stability → Performance (per project policy).
> **Repo:** [github.com/CySalazar/omni](https://github.com/CySalazar/omni) · License: [AGPL-3.0-only](LICENSE) · Branch protection summary in [`docs/11-tooling-and-ci.md`](docs/11-tooling-and-ci.md).
> **HEAD verificato:** `1a0fa3e docs(kernel): MB12 bare-metal smoke finding` sul branch locale `feat/kernel-mb11-userspace` (post v0.2.0 release `25790f0` + PR #33 MB10 `8c1496a` su `main`).
>
> **Allineamento DOE:** questo documento è la **task decomposition L2** (riferimento [`doe-framework/L2-orchestration/02-task-decomposition.md`](doe-framework/L2-orchestration/02-task-decomposition.md)) — i tier P0-P8 sono i moduli DAG-ordinati; le sotto-task atomiche (P*.N.a/b/c) sono le `TASK-NNN` con dipendenze, complessità e acceptance criteria espliciti. I report di stato vivono in [`progress-omni.md`](progress-omni.md) (snapshot mensile/per-milestone — L2 state management) + [`CHANGELOG.md`](CHANGELOG.md) (per-release). Le decisioni architetturali stanno in [`docs/adr/`](docs/adr/) (template DOE `templates/adr-template.md`). Le direttive di esecuzione (code/test/security/docs/CI/deps) sono i 6 file in [`doe-framework/L3-execution/`](doe-framework/L3-execution/).

This document is the canonical, ordered backlog of tasks required to move OMNI OS
from a finalized design (`/docs` v0.1) into an executable, auditable, contribution-ready
project. Tasks are grouped by priority tier (P0 highest). Each task is self-contained
enough that an external contributor could pick it up in isolation.

---

## Legend

| Symbol | Meaning |
|---|---|
| `[ ]` | Not started |
| `[~]` | In progress |
| `[x]` | Done |
| `[!]` | Blocked / awaiting decision |

| Priority | Meaning |
|---|---|
| **P0** | Repo hygiene & supply-chain hardening — must close before any code ships |
| **P1** | Foundational crates (`omni-types`, `omni-crypto`, `omni-capability`) |
| **P2** | OIP process and governance operationalization |
| **P3** | Threat model deepening + cryptographic peer review |
| **P4** | Phase 0 non-technical (Stichting, funding, legal) |
| **P5** | `omni-tee` + TEE HAL (root of trust) |
| **P6** | Kernel `no_std` transition + UEFI bootloader (Phase 1 core) |
| **P7** | Workspace serialization migration `bincode` → `postcard` (resolves RUSTSEC-2025-0141) |

## Dependency graph (one-line)

```
P0 ─────────────────────────────────────────────────────► (sblocca contributi esterni)
   └──► P1 ──► P5 ──► P6
P2 ──► (parallel to P1, gates community contributions)
P3 ──► (parallel to P1, gates mesh implementation in Phase 4)
P4 ──► (parallel everywhere, gates team hiring + Phase 1 start)
P7 ──► (parallel, gates clean cargo audit/deny pass; depends on P1)
```

---

# P0 — Repository Hygiene & Supply-Chain Hardening

**Goal:** make the repository safe, reproducible, and ready to receive external contributions.
**Estimated total effort:** 20–30 hours, solo-founder.
**Blocker for:** any merge from external contributor; Phase 0 closure; any external audit.

---

## P0.1 — Add `LICENSE` file (AGPL-3.0)

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0 / Critical
- **Effort:** 30 min
- **Dependencies:** none
- **Rationale:** README and `Cargo.toml` declare AGPL-3.0 but the physical license file is missing. Without it, the repo's license claim is legally unenforceable and GitHub does not surface it correctly.

**Deliverables:**
- `/LICENSE` — verbatim AGPL-3.0 text from the FSF (`md5 = eb1e647870add0502f8f010b19de32af`, byte-exact match to upstream).
- `/COMMERCIAL-LICENSE.md` — placeholder template referencing Stichting OMNI as licensor (per `08-funding-policy.md` dual-license model). Marked non-binding until Stichting incorporation.

**Acceptance criteria:**
- [x] GitHub correctly identifies the repo as AGPL-3.0 in the sidebar.
- [x] `[workspace.package].license = "AGPL-3.0-only"` and all 12 crate `Cargo.toml` use `license.workspace = true`. CI confirms via `cargo deny check licenses` on every push.
- [x] `COMMERCIAL-LICENSE.md` includes contact email (`cySalazar@cySalazar.com`) and an explicit non-binding clause until Stichting OMNI is constituted.

---

## P0.2 — Add `SECURITY.md` (responsible disclosure policy)

- **Status:** `[x]` (closed 2026-05-09 — PGP fingerprint TBD until Stichting OMNI is constituted, fallback contact `cySalazar@cySalazar.com` documented)
- **Priority:** P0 / Critical
- **Effort:** 2 h
- **Dependencies:** none
- **Rationale:** OMNI OS is a security-sensitive project. Without a published disclosure policy, researchers will either ghost or publish 0-day. A formal policy is also a precondition for any external audit firm.

**Deliverables (`/SECURITY.md`):**
- Reporting channel (email + PGP public key).
- Scope: protocol vulnerabilities, implementation bugs, supply-chain issues. Out of scope: third-party deps with upstream advisories already published.
- SLA: triage within 72h, status update every 14 days, fix or public-disclosure plan within 90 days (configurable per severity).
- Severity classification (CVSSv4-aligned).
- Safe harbor clause for good-faith research.
- Hall of fame / bounty program: defer to OIP-Bounty-001 (P2 dependency).

**Acceptance criteria:**
- [ ] PGP key fingerprint published and verifiable on at least 2 keyservers. *(Deferred until Stichting OMNI is constituted — `<TBD>` placeholder in `SECURITY.md` § 2.2; will land before any external audit engagement per `P3.2`.)*
- [x] SLA wording aligned to RustSec / industry-standard disclosure templates (72h ack, 14d updates, 90d disclosure; 24h/45d for Critical).
- [x] Linked from README ("Reporting security issues" section).

---

## P0.3 — Add `CONTRIBUTING.md` and `CODE_OF_CONDUCT.md`

- **Status:** `[x]` (closed 2026-05-09 — `conduct@omni-os.org` is a placeholder until Stichting OMNI mailbox exists)
- **Priority:** P0
- **Effort:** 3 h
- **Dependencies:** none
- **Rationale:** required by GitHub community standards; signals project maturity to grant evaluators (NLnet, MOSS, Sloan).

**`CONTRIBUTING.md` must cover:**
- DCO (Developer Certificate of Origin) sign-off requirement.
- Required commit format (Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, etc.).
- Branch naming: `feat/<short-desc>`, `fix/<issue-id>`, `oip/<oip-number>`.
- PR workflow: draft → review → 2 approvals → merge.
- Local setup: `rustup`, `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`.
- How to file an OIP for substantive proposals (link to P2 deliverable).

**`CODE_OF_CONDUCT.md`:**
- Adopt **Contributor Covenant v2.1** verbatim.
- Define enforcement contact: `conduct@omni-os.org` (placeholder until Stichting exists).
- Specify escalation chain: maintainer → lead architect → Foundation board (post-Phase 0).

**Acceptance criteria:**
- [x] DCO check enforced in CI via `.github/workflows/dco.yml` (validates `Signed-off-by:` trailer on every PR commit).
- [ ] CoC enforcement contact resolves to a real mailbox. *(Currently `conduct@omni-os.org` is a placeholder, fallback `cySalazar@cySalazar.com` documented; mailbox provisioning awaits Stichting OMNI per `P4.1`.)*

---

## P0.4 — CI/CD pipeline (GitHub Actions)

- **Status:** `[x]` (closed 2026-05-09 — 7 workflows landed: ci, audit, sbom, reproducible-build, dco, codeql, labeler)
- **Priority:** P0 / Critical
- **Effort:** 8–12 h
- **Dependencies:** P0.1 (license) for `cargo deny` license check.
- **Rationale:** without CI, every merge is a leap of faith. Deterministic builds are explicitly mentioned in `rust-toolchain.toml` rationale; CI is the only way to enforce that.

**Workflows to create under `.github/workflows/`:**

1. **`ci.yml`** — runs on every push and PR:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace --all-features`
   - `cargo doc --workspace --no-deps` (link check)
   - Build matrix: `x86_64-unknown-linux-gnu` (initial scope per `07-hardware-requirements.md`).

2. **`audit.yml`** — daily + on `Cargo.lock` change:
   - `cargo audit` (RustSec advisories)
   - `cargo deny check advisories|bans|licenses|sources`

3. **`sbom.yml`** — on every tagged release:
   - Generate CycloneDX SBOM via `cargo-cyclonedx`.
   - Attach to GitHub release.
   - Generate provenance attestation (SLSA Level 3 target).

4. **`reproducible-build.yml`** — on every release tag:
   - Two parallel runners on identical Ubuntu pinned image.
   - Build the same release artifact, compare hashes byte-for-byte.
   - Fail the release if hashes diverge.

5. **`dco.yml`** — DCO sign-off check via `dcoapp`.

6. **`codeql.yml`** — GitHub CodeQL static analysis (Rust support is beta but worth enabling).

**Acceptance criteria:**
- [x] All workflows triggered and started successfully on the initial push to `main` (2026-05-09 first run on commit `61426d5`).
- [x] Branch protection on `main` requires the 8 status checks to be green (`ci/cargo fmt`, `ci/cargo clippy`, `ci/cargo test (ubuntu-24.04)`, `ci/cargo doc`, `audit/cargo audit`, `audit/cargo deny`, `dco/DCO sign-off`, `codeql/CodeQL — rust`).
- [ ] Workflow run cost < 10 minutes for the typical PR. *(Cache wiring via `Swatinem/rust-cache@v2` is in place; first-run baseline pending — Rust toolchain warm-up on cold cache will exceed 10 min once, then subsequent runs settle below.)*

---

## P0.5 — Commit `Cargo.lock`

- **Status:** `[x]` (closed 2026-05-09 — `git init -b main`, four signed commits on `main` after history rewrite to project identity `cySalazar <cySalazar@cySalazar.com>`: `61426d5` initial P0, `15419cb` URL refs, `ebf9539` P0 closure docs, `101ff79` identity standardization. All four are GitHub-verified, not just locally signed.)
- **Priority:** P0
- **Effort:** 5 min
- **Dependencies:** none
- **Rationale:** the `.gitignore` policy comment says `Cargo.lock` IS committed for the workspace, but no lock file is currently in the repo. Reproducible builds and `cargo audit` both rely on the lockfile.

**Acceptance criteria:**
- [x] `Cargo.lock` present in the repo root and tracked from commit `61426d5` onward (56 KB).
- [ ] `cargo audit` runs cleanly against committed lockfile. *(First run scheduled via `audit.yml` daily cron + on Cargo.lock change; verify on first green run.)*

---

## P0.6 — Add `rustfmt.toml`, `clippy.toml`, `deny.toml`

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0
- **Effort:** 3 h
- **Dependencies:** none
- **Rationale:** the workspace `Cargo.toml` defines lints, but project-wide tool configuration is not pinned. Without `rustfmt.toml` and `clippy.toml`, reformatting drift will emerge across contributors.

**`rustfmt.toml`:**
```toml
edition = "2024"
max_width = 100
use_field_init_shorthand = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
reorder_imports = true
```

**`clippy.toml`:**
```toml
msrv = "1.85"
avoid-breaking-exported-api = false  # we are pre-1.0
disallowed-methods = [
  { path = "std::env::var", reason = "Use a config crate; env reads must be auditable." },
]
```

**`deny.toml` (cargo-deny config):**
- `[advisories]`: vulnerability = `deny`, unmaintained = `warn`, yanked = `deny`.
- `[licenses]`: allow = `["AGPL-3.0", "Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-DFS-2016"]`. Deny everything else (force conscious inclusion).
- `[bans]`: deny `openssl-sys`, `native-tls` (force `rustls`), deprecated crypto crates.
- `[sources]`: only `crates.io` allowed; no git deps without explicit allowlist.

**Acceptance criteria:**
- [ ] `cargo fmt` is a no-op on a fresh checkout. *(Verified by `ci/cargo fmt` on first green run.)*
- [ ] `cargo clippy` produces zero warnings on a fresh checkout. *(Verified by `ci/cargo clippy` on first green run.)*
- [ ] `cargo deny check` passes. *(Verified by `audit/cargo deny` on first green run.)*

---

## P0.7 — Branch protection + signed commits

- **Status:** `[x]` (closed 2026-05-09 — applied to `CySalazar/omni` via `scripts/bootstrap-github.sh`: `enforce_admins=true`, `required_signatures=true`, `linear_history=true`, `allow_force_pushes=false`, 1 reviewer, 8 required status checks; SSH ed25519 signing key registered on GitHub as signing-key id 938835. All 4 commits on `main` show `verified: true reason: valid` after the identity rebase.)
- **Priority:** P0
- **Effort:** 1 h
- **Dependencies:** P0.4 (CI must exist before requiring it).
- **Rationale:** "trust is mathematically required" is a project tenet. Signed commits are the lowest-friction enforcement at the SCM layer.

**Configuration:**
- Branch `main`: require PR, require 1 approval (will rise to 2 once a co-maintainer joins per Phase 1 hiring), require all CI checks green, require linear history, require signed commits, dismiss stale reviews on push.
- Tags: only mergeable from `main`, signed (legacy endpoint deprecated; tracked for migration to GitHub Rulesets).
- Repo settings: `main` is default branch, force-push disabled, deletion disabled.

**Acceptance criteria:**
- [x] An unsigned/non-PR push is rejected at push time. *(Live-verified on 2026-05-09: direct push of the docs commit was rejected with "Changes must be made through a pull request" + "8 of 8 required status checks are expected" — protection is operational.)*
- [x] A PR cannot be merged with red CI. *(Enforced by `required_status_checks` containing all 8 workflow jobs.)*

---

## P0.8 — Issue / PR templates and label taxonomy

- **Status:** `[x]` (closed 2026-05-09 — labels created by `bootstrap-github.sh` after first push)
- **Priority:** P0
- **Effort:** 2 h
- **Dependencies:** none

**`.github/ISSUE_TEMPLATE/`:**
- `bug_report.yml` — structured form: affected crate, version, repro steps, expected/actual, logs.
- `feature_request.yml` — must include a note "if this is substantive, file an OIP first" (link to P2).
- `security_advisory.yml` — links to `SECURITY.md`, refuses public discussion.
- `oip_proposal.yml` — entry point for OIP drafts (P2 dependency).

**`.github/PULL_REQUEST_TEMPLATE.md`:**
- Conventional Commits checklist.
- DCO sign-off reminder.
- Breaking change disclosure.
- Documentation update confirmation (per project policy: docs and code stay in sync).
- Test coverage statement.

**Labels (auto-applied via `.github/labeler.yml`):**
- `area:kernel`, `area:crypto`, `area:capability`, `area:tee`, `area:hal`, `area:runtime`, `area:mesh`, `area:tokenization`, `area:sdk`, `area:agent`, `area:shell`, `area:docs`, `area:ci`.
- `priority:P0`–`P3`.
- `kind:bug`, `kind:feature`, `kind:refactor`, `kind:docs`, `kind:security`.
- `oip-required`, `breaking-change`, `good-first-issue`, `help-wanted`.

**Acceptance criteria:**
- [x] New issue UI shows all four templates (config.yml, bug_report.yml, feature_request.yml, security_advisory.yml, oip_proposal.yml — blank issues disabled).
- [x] Auto-labeler workflow active (`.github/workflows/labeler.yml`); label taxonomy of 32 created via `bootstrap-github.sh`. *(First validation observed on Dependabot PRs — `labeler` job completed `success`.)*

---

## P0.9 — Dependabot / Renovate configuration

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0
- **Effort:** 1 h
- **Dependencies:** P0.5

Add `.github/dependabot.yml`:
- Weekly checks for `cargo` ecosystem.
- Group security updates.
- Auto-approve patch updates after CI green.
- Major-version updates: PR only, no auto-merge (require human review for breaking deps).

**Acceptance criteria:**
- [x] First Dependabot PR opens within 7 days of config merge. *(Live-verified on 2026-05-09: 2 Dependabot PRs auto-opened within minutes of the initial push — `chore(deps)(deps): Bump mockall from 0.13.1 to 0.14.0` and `chore(deps)(deps): Bump the cryptography group with 2 updates`.)*

---

# P1 — Foundational Crates Implementation

**Goal:** implement the bottom of the dependency stack so every other crate has solid, tested, audited foundations to build on.
**Estimated total effort:** 4–6 weeks solo, 2–3 weeks with cryptographer.
**Order is mandatory:** `omni-types` → `omni-crypto` → `omni-capability`.

---

## P1.1 — Implement `omni-types`

- **Status:** `[x]` (closed 2026-05-10 — 33 tests passing, clippy strict + doc strict clean, `no_std + alloc` compile-verified)
- **Priority:** P1
- **Effort:** 1 week
- **Dependencies:** P0 (CI must exist to gate this work)
- **Rationale:** every other crate imports `omni-types`. Identifier confusion (passing `ModelId` where `NodeId` is expected) is a class of bug we eliminate at the type level.

### Sub-tasks

- [ ] **P1.1.a — `identity.rs`**
  - `NodeId([u8; 32])` — derived from TEE attestation report hash, content-addressed, deterministic.
  - `AgentId(Uuid)` — local to a node.
  - `ModelId([u8; 32])` — content-addressed hash of signed model manifest.
  - `CapabilityId([u8; 16])` — opaque, random (UUIDv7 for sortability).
  - `SessionId([u8; 16])` — short-lived, random.
  - All newtypes derive: `Debug`, `Clone`, `Copy` (where size allows), `Hash`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Serialize`, `Deserialize`. **No** `Display` for raw bytes — force callers to use a hex/base32 helper to prevent accidental logging of sensitive IDs.
  - Constructors: each ID type has a `from_*` constructor that documents the trust boundary (e.g., `NodeId::from_attestation_quote(quote: &Quote) -> Result<Self>`).

- [ ] **P1.1.b — `error.rs`**
  - Top-level `enum OmniError` with `thiserror::Error`.
  - Variants: `Crypto`, `Capability`, `Identity`, `Ipc`, `Mesh`, `Tee`, `Hal`, `Tokenization`, `Policy`, `Internal`.
  - Each variant nests a domain-specific error to allow precise pattern matching upstream.
  - `Result<T, OmniError>` type alias.
  - **Critical:** error messages must NEVER include sensitive data (tokens, key material, plaintext PII). Add a `#[deny]` lint or compile-time check where feasible.

- [ ] **P1.1.c — `version.rs`**
  - `ProtocolVersion { major: u16, minor: u16, patch: u16 }`.
  - Constants: `PROTOCOL_VERSION_V0_1`, `PROTOCOL_VERSION_V1_0`, etc.
  - `is_compatible_with(&self, other: &Self) -> bool` — major must match, minor must be `>=`.

- [ ] **P1.1.d — `encrypted.rs` (API surface only, no impl yet)**
  - Empty marker types: `EncryptedString`, `MaskedSSN`, `TokenizedEmail`, `AttestedHash`.
  - `pub trait EncryptedType: Sealed { ... }` with sealed trait pattern (cannot be implemented outside this crate).
  - **No** constructors exposed outside `omni-tokenization`. The only way to mint one is via the tokenization service running inside an attested TEE. Phase 2 work, but the API needs to exist now to prevent other crates from "cheating".

- [ ] **P1.1.e — Tests**
  - Unit tests per newtype: round-trip serde, equality, hash determinism, ordering.
  - Property tests with `proptest`: any random byte sequence parses to the same ID twice (determinism), distinct sequences produce distinct IDs (uniqueness within `2^256` collision space).
  - Compile-fail tests via `trybuild`: cannot construct `EncryptedString` from `String` outside the crate, cannot pass `ModelId` to a function expecting `NodeId`.

**Acceptance criteria:**
- [ ] `cargo test -p omni-types` passes.
- [ ] `cargo doc -p omni-types --no-deps` produces zero warnings.
- [ ] `cargo clippy -p omni-types -- -D warnings` clean.
- [ ] No `unsafe` code, no `unwrap`, no `expect` outside `#[cfg(test)]`.
- [ ] 100% public API documented.

---

## P1.2 — Implement `omni-crypto`

- **Status:** `[x]` (closed 2026-05-10 — 55 tests passing including RFC vectors for ChaCha20-Poly1305 / Ed25519 / X25519 / SHA-256 / SHA3-256 / BLAKE3 / HKDF-SHA-256; clippy strict + doc strict clean; **AWAITING_CRYPTO_REVIEW** marker in `lib.rs` per P3.2 dependency)
- **Priority:** P1 / Critical
- **Effort:** 2–3 weeks (longer if cryptographer review is sequential)
- **Dependencies:** P1.1, P3 (peer review should run in parallel and gate the merge)
- **Rationale:** every security guarantee in OMNI OS reduces to correct use of these primitives. A single mistake here is project-ending.

### Sub-tasks

- [ ] **P1.2.a — `aead.rs`**
  - Wrap `chacha20poly1305::ChaCha20Poly1305`.
  - Public types: `OmniAeadKey([u8; 32])`, `OmniNonce([u8; 12])`, `OmniCiphertext(Vec<u8>)`.
  - API: `seal(&key, &nonce, &aad, &plaintext) -> Result<OmniCiphertext>`, `open(&key, &nonce, &aad, &ct) -> Result<Vec<u8>>`.
  - Nonces must be unique per (key, message). Provide a `NonceCounter` type that panics on overflow (defensive).
  - Zeroize key material on drop (`zeroize::Zeroize` derive).

- [ ] **P1.2.b — `signing.rs`**
  - Wrap `ed25519-dalek::SigningKey` / `VerifyingKey`.
  - `OmniSigningKey`, `OmniVerifyingKey`, `OmniSignature([u8; 64])`.
  - API: `sign(&sk, &msg) -> OmniSignature`, `verify(&vk, &msg, &sig) -> Result<()>`.
  - Constant-time signature verification (already in `dalek`).
  - Zeroize on drop.

- [ ] **P1.2.c — `kex.rs`**
  - Wrap `x25519-dalek` for ECDH.
  - `OmniEphemeralSecret`, `OmniPublicKey`, `OmniSharedSecret`.
  - API: `generate_ephemeral() -> (secret, pubkey)`, `diffie_hellman(secret, peer_pub) -> OmniSharedSecret`.
  - Phase 4: hybrid KEM with Kyber (placeholder module).

- [ ] **P1.2.d — `hash.rs`**
  - Trait `OmniHash` with three impls: SHA-256, SHA3-256, BLAKE3.
  - Default for protocol-level hashing: BLAKE3 (fastest, hardware-friendly, post-quantum resilient).
  - `domain_separated_hash(domain: &str, data: &[u8]) -> [u8; 32]` — every hash call must be domain-separated to prevent cross-protocol collisions.

- [ ] **P1.2.e — `kdf.rs`**
  - HKDF-SHA-256 for protocol session keys.
  - Argon2id for user secrets (memory-hard).
  - API: `hkdf_expand(prk, info, len)`, `argon2id_hash(password, salt) -> Result<Hash>`.

- [ ] **P1.2.f — `fpe.rs`** (Phase 4 placeholder)
  - Module exists with `unimplemented!()` and TODO. Do not ship FF1/FF3-1 until Phase 4 needs it.

- [ ] **P1.2.g — `snark.rs`** (Phase 4 placeholder)
  - Module exists. Selection between STARK / transparent SNARK deferred to OIP-Crypto-002 (P3).

- [ ] **P1.2.h — Tests**
  - **RFC test vectors** for every primitive (Ed25519 RFC 8032, X25519 RFC 7748, ChaCha20-Poly1305 RFC 8439, SHA-256 NIST FIPS 180-4, BLAKE3 reference vectors).
  - Property tests: sign/verify round-trip, encrypt/decrypt round-trip, hash determinism.
  - Negative tests: tampered ciphertext fails to decrypt, wrong key fails to verify.
  - **Fuzz target** (`cargo-fuzz`): every public API takes arbitrary input without panicking.
  - Constant-time verification: critical paths (signature verify, AEAD tag check) must use `subtle::ConstantTimeEq`. Add a CI lint that grep-flags `==` on byte arrays in this crate.

**Acceptance criteria:**
- [ ] All RFC vectors pass.
- [ ] No `unsafe` code.
- [ ] Cryptographer review (P3.2) signed off in writing.
- [ ] `cargo bench` baselines recorded for each primitive (perf regressions caught later).

---

## P1.3 — Implement `omni-capability`

- **Status:** `[x]` (closed 2026-05-10 — 43 tests passing including Macaroons-style attenuation monotony property test (64 cases) + tampered-child adversarial property test; clippy strict + doc strict clean; in-crate `MicroBloom` revocation list to keep `no_std + alloc`)
- **Priority:** P1 / Critical
- **Effort:** 2 weeks
- **Dependencies:** P1.1, P1.2
- **Rationale:** capabilities are the runtime enforcement of security policy. A bug here = privilege escalation.

### Sub-tasks

- [ ] **P1.3.a — `token.rs`**
  - `struct CapabilityToken { id: CapabilityId, subject: NodeId, action: Action, resource: Resource, not_before: u64, not_after: u64, caveats: Vec<Caveat>, signature: OmniSignature }`.
  - `Action` and `Resource` are typed enums, not strings.
  - Canonical serialization (deterministic byte order) for signing — use `bincode` with strict-mode config or a hand-rolled encoder. Document the wire format in `/docs/03-mesh-protocol.md`.

- [ ] **P1.3.b — `scope.rs`**
  - Grammar: `Action × Resource × TimeWindow × Caveats`.
  - `fn intersects(&self, other: &Scope) -> bool` — used to validate that a child scope is contained in the parent.

- [ ] **P1.3.c — `attenuation.rs`**
  - Macaroons-style: `parent.attenuate(caveat) -> child` where `child.scope ⊆ parent.scope` always.
  - Each caveat is a signed monotonic restriction (e.g., `not_after = parent.not_after - delta`, `resource = parent.resource ∩ {x}`).
  - **Property test (critical):** for any random parent + random caveat sequence, the derived child scope is always a subset of the parent. This is the security-critical invariant.

- [ ] **P1.3.d — `revocation.rs`**
  - In-memory revocation list (sled-backed in Phase 2).
  - Bloom filter for fast membership check; full list for false-positive resolution.
  - Short TTL (5–15 min) means lists stay small; rotate hourly.

- [ ] **P1.3.e — TEE binding** (placeholder traits, real impl in P5)
  - A capability is invalid unless the verifying TEE attestation matches the capability's `subject`.
  - Define the trait `TeeAttestation` here; impl moves to `omni-tee`.

- [ ] **P1.3.f — Tests**
  - Unit + property tests on attenuation monotony.
  - Test that signature verification rejects: tampered fields, expired tokens, mismatched subject, broader-than-parent caveats.
  - Compile-fail tests: cannot construct a `CapabilityToken` without going through the issuer API.

**Acceptance criteria:**
- [ ] All sub-tasks complete with tests.
- [ ] Adversarial test suite: 100 random tampered tokens, 100% rejected.
- [ ] Wire format documented and added to `/docs/03-mesh-protocol.md`.

---

# P2 — OIP Process and Governance Operationalization

**Goal:** make Layer 2 governance (federated specification) actually usable.
**Estimated effort:** 1 week.
**Blocker for:** community contributions to architecture / protocol; Phase 0 closure.

---

## P2.1 — Write OIP-Process-001 (the meta-OIP)

- **Status:** `[x]` (closed 2026-05-10 — `oips/oip-process-001.md` `Active` under bootstrap fiat clause §6.3; lifecycle, categories, voting, BDFL window, editor body, Bootstrap Period all formalized; structural lint green. **Amended same day** under bootstrap fiat clause §6.3 with three structural changes: new §6.5 Critical-security Bootstrap exception, expanded §5.2 Known limitations, refined `## Privacy Considerations` + template HTML guidance. Amendment history logged in OIP itself.)
- **Priority:** P2 / Critical
- **Effort:** 3 days
- **Dependencies:** P0.3 (CONTRIBUTING)
- **Rationale:** roadmap explicitly requires `OIP-Process-001` to close Phase 0. Without it, every architectural change is autocratic instead of federated.

**Deliverables (`/oips/oip-process-001.md`):**
- OIP types: `Process`, `Standards Track`, `Informational`, `Meta`. *(Done — §1)*
- Lifecycle: `Draft → Review → Last Call → Active → Final | Withdrawn | Superseded | Rejected`. *(Done — §4. Unified the two slightly different lifecycles in `todo.md` and `docs/05-governance.md` v0.1; the OIP is the authoritative source going forward.)*
- Required sections: Abstract, Motivation, Specification, Rationale, Backwards Compatibility, Test Cases, Reference Implementation, Security Considerations, Privacy Considerations, Copyright. *(Done — §2; CI lint enforces them.)*
- Voting mechanism (initial, manual until tooling exists):
  - Eligibility: TEE-attested unique device (anti-Sybil). *(Done — §5.1)*
  - Weighting: proof-of-uptime + proof-of-contribution. Concrete formula deferred to a future Process OIP under slug `voting`. *(Done — §5.2; bootstrap defaults specified for the deferred period.)*
  - Quorum: 30% of eligible voters or 14-day open window, whichever is reached first. *(Done — §5.3)*
  - Approval threshold: quadratic-vote majority (50%+1); 66.7% supermajority for Layer 1 cryptographic breaks. *(Done — §5.3)*
- BDFL veto window: 5 years from 2026-05-09 (first public commit), sunset 2031-05-09 23:59 UTC, structurally non-extensible. *(Done — §5.4)*
- Editor role: 2 OIP editors per term, rotated annually. Bootstrap Period explicitly codified: 1 interim editor (founder) + Seat 2 vacant until Phase 1 hire OR 2027-05-10 hard deadline. *(Done — §6)*

**Acceptance criteria:**
- [x] OIP-Process-001 itself passes its own process (dogfood test). *(Ratified under the one-time bootstrap fiat clause §6.3 — no prior process exists to vote it in. The dogfood test of the formal flow is deferred to the first non-`Meta` OIP, by design, and explicitly documented as such in §6.3 ¶3.)*
- [x] Linked from README and `05-governance.md`. *(README §"Public commitments" + §"Contributing"; `docs/05-governance.md` §2 fully refactored to point at the OIP as authoritative.)*
- [x] Issue template for new OIPs (P0.8) functional. *(Pre-existing: `.github/ISSUE_TEMPLATE/oip_proposal.yml`; cross-referenced from `oips/README.md` and `CONTRIBUTING.md` §9.)*

---

## P2.2 — Set up `/oips/` directory and template

- **Status:** `[x]` (closed 2026-05-10)
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1

**Deliverables:**
- `/oips/README.md` — index of all OIPs by number, status, title. *(Done; auto-validated against the registry by the lint.)*
- `/oips/oip-template.md` — copy this for new proposals. *(Done; canonical template with frontmatter + 10 required sections.)*
- `/oips/oip-0000-template.md` — same content with reserved number 0. *(Done; sentinel file with `oip: 0000`, status `Withdrawn` to keep it out of the active index, treated as a special case by the lint.)*
- `scripts/lint-oips.py` + `.github/workflows/oip-lint.yml` — CI lint that validates frontmatter, sections, filename↔number coherence, and index cross-reference. *(Done; stdlib-only Python 3.11.)*

**Acceptance criteria:**
- [x] `/oips/README.md` auto-renders a table of contents. *(Markdown table; lint enforces every OIP file has a row.)*
- [x] CI lint that fails if an OIP file deviates from the template structure. *(Verified manually: lint exits 0 with 2 valid OIPs, exits 1 on injected violations during development — see `scripts/lint-oips.py` test trace in dev session.)*

---

## P2.3 — Document the BDFL veto window in writing

- **Status:** `[x]` (closed 2026-05-10 — start `2026-05-09`, sunset `2031-05-09` 23:59 UTC, immutable; structurally non-extensible per asymmetric clause `OIP-Process-001` §5.4)
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1
- **Rationale:** the memory says "BDFL veto for first 5 years (sunset clause)". This must be in a versioned, immutable document so it can't be silently extended.

**Deliverables:**
- Section in `05-governance.md` cross-referencing OIP-Process-001 with explicit start date and sunset date. *(Done — `docs/05-governance.md` §2 "Founder role (years 1–5)" rewritten with three independent immutable anchors: this file, OIP-Process-001 §5.4, first signed commit `61426d5` on 2026-05-09.)*
- Public commitment in README that the veto cannot be extended without an OIP that itself cannot be vetoed. *(Done — README §"Public commitments". The asymmetric clause in `OIP-Process-001` §5.4 makes the window structurally non-extensible by founder action alone.)*

---

# P3 — Threat Model Deepening + Cryptographic Peer Review

**Goal:** validate the protocol design before code commits to it.
**Estimated effort:** 2–3 weeks (parallel to P1).
**Blocker for:** `omni-mesh` implementation in Phase 4.

---

## P3.1 — Formal mesh handshake specification

- **Status:** `[~]` (spec + Tamarin model landed 2026-05-10; model extended to v0.2 on 2026-05-12 with lemmas for I3 + I7-extended + I8; proof execution gated on P3.2)
- **Priority:** P3
- **Effort:** 1–2 weeks
- **Dependencies:** existing `04-security-model.md`, `04a-threat-model.md`
- **Rationale:** changing protocol post-implementation is 10× the cost of changing it on paper.

**Deliverables:**
- [x] `/docs/protocol/handshake.md` — formal wire-level spec with 8 numbered invariants (I1–I8).
- [~] Protocol verification with **ProVerif** or **Tamarin** for symbolic analysis. v0.2 model at [`/protocol-proofs/handshake.spthy`](protocol-proofs/handshake.spthy) covers 8 lemmas:
  - [x] `mutual_authentication` (I1)
  - [x] `forward_secrecy` (I2)
  - [x] `mutual_tee_attestation_binding` (I3) — added 2026-05-12
  - [x] `replay_resistance` (I4 — partial; full I4 needs nonce-uniqueness lemma)
  - [x] `kci_resistance` (I5)
  - [x] `protocol_version_binding` (I7)
  - [x] `measurement_root_binding` (I7-extended) — added 2026-05-12
  - [x] `compliance_capability_no_downgrade` (I8) — added 2026-05-12
  - [ ] I6 (UKS) — to be added or implied by `mutual_authentication` + identity binding (cryptographer to confirm)
- Each property documented as an invariant in `handshake.md` § 2.

**Acceptance criteria:**
- [x] Spec lives under `/docs/protocol/`.
- [x] Tamarin/ProVerif proof artifacts checked into `/protocol-proofs/`.
- [x] **Tamarin proof execution** (`tamarin-prover handshake.spthy --prove` returns `verified` for all 8 lemmas) — completed 2026-05-12 with tamarin-prover 1.12.0; processing time ≈ 1.36s; full run log at [`protocol-proofs/handshake-proof-run-2026-05-12.txt`](protocol-proofs/handshake-proof-run-2026-05-12.txt). Five structural model defects were fixed in-place during the run; details in the run log footer and in [`protocol-proofs/handshake.spthy`](protocol-proofs/handshake.spthy) `Status of proofs` block. One residual wellformedness warning (Message Derivation Checks on peer-controlled variables) carried forward to the cryptographer review.
- [ ] Review by external cryptographer (P3.2).

---

## P3.2 — External cryptographer engagement

- **Status:** `[!]` blocked on funding (P4)
- **Priority:** P3 / Critical
- **Effort:** 2–4 weeks (cryptographer's calendar)
- **Dependencies:** P4 (funding for paid review) OR community volunteer
- **Rationale:** the roadmap lists "1 cryptographer" in the core team for Phase 0. This sub-task formalizes the engagement.

**Deliverables:**
- Signed engagement letter (paid review or volunteer agreement).
- Written review of: `omni-crypto` API design, mesh handshake spec, capability attenuation invariants, compliance proof scheme.
- Public review document (or executive summary) published in `/docs/audits/`.

**Acceptance criteria:**
- [ ] Cryptographer's name and credentials disclosed in `CONTRIBUTORS.md`.
- [ ] All findings tracked as issues with `kind:security` label.

---

## P3.3 — Decide STARK vs SNARK for compliance proofs

- **Status:** `[ ]`
- **Priority:** P3
- **Effort:** 1 week (research) + 1 week (decision via OIP)
- **Dependencies:** P3.1, P3.2
- **Rationale:** memory note: "favor STARK or transparent constructions for v1". This must become an OIP and a documented decision before `omni-mesh` is built.

**Deliverables:**
- `/oips/oip-crypto-002.md` — proposal: STARK-based compliance proofs, candidate libraries (`winterfell`, `triton-vm`), benchmark results, trusted-setup avoidance rationale.
- Update `04-security-model.md` § "Compliance proofs" with the chosen approach.

**Acceptance criteria:**
- [ ] OIP merged.
- [ ] Benchmark report (proof size, prover time, verifier time) published.

---

# P4 — Phase 0 Non-Technical (Stichting + Funding)

**Goal:** legal + financial foundation for the project to exist as a multi-decade entity.
**Estimated effort:** 3–6 months calendar (slow burn, parallel to all other tracks).
**Blocker for:** hiring (Phase 1), Phase 0 closure.

---

## P4.1 — Constitute Stichting OMNI in the Netherlands

- **Status:** `[ ]`
- **Priority:** P4 / Critical
- **Effort:** 2 months calendar (notary + KVK registration)
- **Dependencies:** legal counsel
- **Rationale:** mandated by `05-governance.md` Layer 3.

**Deliverables:**
- Notarial deed (`stichtingsakte`).
- KVK registration.
- Bylaws (`statuten`) embodying:
  - 5 trustees (founder included).
  - BDFL veto sunset 5y / full transition 10y.
  - Mission anchor: privacy-first, local-first, anti-regulatory-capture (excluded funding sources).
  - Asset lock clause (assets cannot be redirected to non-aligned mission).
- ANBI status pursuit (Dutch tax-deductible charity).

**Acceptance criteria:**
- [ ] KVK number obtained.
- [ ] Bylaws published in `/docs/legal/bylaws.md` (also in Dutch original).
- [ ] First trustee appointment letter signed.

---

## P4.2 — Funding pipeline

- **Status:** `[ ]`
- **Priority:** P4 / Critical
- **Effort:** 3 months calendar
- **Dependencies:** P4.1 (most grants require legal entity)
- **Rationale:** target €350K for 6 months runway per roadmap.

**Sub-tasks:**

- [ ] **P4.2.a — Pitch deck and one-pager**
  - 12–15 slides: problem, vision, proof-of-progress (`/docs` v0.1), team, ask, 5y plan.
  - One-pager for warm intros.
  - Both files in `/docs/funding/` (private branch or out-of-repo).

- [ ] **P4.2.b — Grant applications**
  - **NLnet Foundation** — DECISION REQUIRED: accept or reject as funder (memory marks as borderline TBD). Recommendation: accept, since NLnet funds privacy-aligned projects and EU NGI channeling is *operational* not *regulatory*.
  - **Mozilla MOSS** — apply.
  - **Sloan Foundation** (open-source) — apply.
  - **Open Philanthropy** (long-term safety) — apply.

- [ ] **P4.2.c — Corporate sponsor outreach**
  - Aligned sponsors per memory: Proton, Tutanota, Mullvad, Element, System76, Framework, Purism.
  - One-page sponsorship tier menu (Bronze/Silver/Gold + crypto-aligned naming).
  - Boundary: no regulatory power, no controlling stake, no kill-switch over project direction.

- [ ] **P4.2.d — Community donations**
  - Set up Open Collective or similar (post-Stichting).
  - Transparent monthly accounting.

**Acceptance criteria:**
- [ ] €350K secured or 3 active term-sheets.
- [ ] Public funding ledger.

---

## P4.3 — Excluded funding sources documented and enforced

- **Status:** `[ ]`
- **Priority:** P4
- **Effort:** 1 day
- **Dependencies:** P4.1
- **Rationale:** memory explicitly excludes governments and government-aligned funds. This must be in bylaws and in a public funding policy so a future board cannot quietly accept.

**Deliverables:**
- `08-funding-policy.md` already covers this — review and harden the language.
- Add a clause to bylaws making excluded-source acceptance a supermajority decision (4/5 trustees), publicly logged.

---

## P4.4 — Recruit core team

- **Status:** `[!]` blocked on P4.2
- **Priority:** P4
- **Effort:** 2 months calendar
- **Dependencies:** P4.2 (funding)
- **Roles per roadmap:**
  - Lead Architect — founder (cySalazar).
  - 2 senior Rust engineers (one with kernel/embedded, one with networking/distributed).
  - 1 cryptographer.
- Compensation transparency: salary bands published before hiring.

---

# P5 — `omni-tee` + TEE HAL

**Goal:** root of trust. Every security guarantee in OMNI OS reduces to TEE attestation working correctly.
**Estimated effort:** 2–3 weeks after P1.
**Blocker for:** capability validation in production, mesh handshake.

---

## P5.1 — Define `TeeBackend` trait in `omni-tee`

- **Status:** `[~]` (scaffold landed + verified 2026-05-12 — `TeeBackend` trait + `TeeFamily` + `TeeError` taxonomy + `Quote` / `Measurement` / `Nonce` / `SealedBlob` / `SealPolicy` / `TeeSharedKey` + `MockTeeBackend` end-to-end at `crates/omni-tee/`. 23 unit + 4 integration tests. Full `[x]` after the API is consumed by `omni-mesh` per P3 closure.)
- **Priority:** P5
- **Effort:** 3 days
- **Dependencies:** P1.1, P1.2

**API surface:**
```rust
pub trait TeeBackend: Send + Sync {
    fn attest(&self, nonce: &[u8]) -> Result<Quote, OmniError>;
    fn verify_quote(&self, quote: &Quote, expected_measurement: &Measurement) -> Result<(), OmniError>;
    fn seal(&self, plaintext: &[u8], policy: &SealPolicy) -> Result<SealedBlob, OmniError>;
    fn unseal(&self, sealed: &SealedBlob) -> Result<Vec<u8>, OmniError>;
    fn derive_key_for(&self, peer_attestation: &Quote) -> Result<OmniSharedSecret, OmniError>;
}
```

---

## P5.2 — Implement Intel TDX backend

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1–2 weeks
- **Dependencies:** P5.1, hardware access (TDX-capable Intel CPU 4th gen Xeon scalable or later)
- **Rationale:** TDX is the chosen baseline x86_64 TEE.

**Sub-tasks:**
- Wrap `tdx-attestation` crate (or implement Quote v4 generation manually if needed).
- Integration test using Intel's public TDX simulator first; hardware test later.
- Document TCB recovery procedure (when Intel publishes a microcode update affecting attestation).

---

## P5.3 — Implement AMD SEV-SNP backend

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1–2 weeks
- **Dependencies:** P5.1, hardware access (AMD EPYC Milan or later)

---

## P5.4 — TEE HAL re-export in `omni-hal::tee`

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1 day
- **Dependencies:** P5.1

Re-export `TeeBackend` and provide a runtime selector (`select_tee_backend()`) that detects available hardware and returns the appropriate concrete impl.

---

# P6 — Kernel `no_std` Transition + UEFI Bootloader (Phase 1 of roadmap)

**Goal:** transition `omni-kernel` from a stub library to a bare-metal microkernel that boots on x86_64 with TEE attestation, IPC, capability-based syscalls, user-space driver model, and first external audit.
**Estimated effort:** 6–18 months calendar (Phase 1 of roadmap; ~65% done as of 2026-05-18).
**Blocker for:** everything userspace beyond the embedded probes; Phase 2 entry.

## P6 — Subsystem-level status (one-line per OIP-mapped sub-tier)

- [x] **P6.1 — `omni-kernel` → `no_std` + `no_main`** (closed 2026-05-15 — `bare-metal` feature flag; `OIP-Kernel-012` `Active`; K5 QEMU smoke green on PR #25; panic handler + bump allocator + heap provisioning per `OIP-Kernel-012` § S2 operational).
- [x] **P6.2 — UEFI bootloader (decision: `bootloader_api` 0.11 selected)** (closed 2026-05-16 — `OIP-Kernel-003` `Active` via Solo Founder Fast-Track § 5.5; `kernel-runner` boots under QEMU+OVMF, VirtualBox, Proxmox VMID 103; PR #25 merged).
- [x] **P6.3 — Page table management + virtual memory subsystem** (closed 2026-05-18 — MB2 `PageMapper` x86_64 walker + MB9 huge-page-aware + MB10 kernel-stack VA isolation + MB11 per-process CR3 / `AddressSpace`. `map_4k_into(root,…)` for explicit-root targets. Limit: `map_4k` does not split huge-page entries — tracked under "Kernel follow-up" below).
- [x] **P6.4 — Scheduler (preemptive round-robin)** (closed 2026-05-18 — MB6 cooperative round-robin + MB7 LAPIC xAPIC + MB8 preemption from timer + MB12.0a/b multi-task user dispatch (TSS.rsp0 update + CR3 reload + first-dispatch sentinel via `enter_user_mode`). Thermal/AI-workload-aware variant deferred to Phase 2 — out of P6 scope).
- [x] **P6.5 — Capability-based syscall dispatch** (closed 2026-05-16 MB4 ABI + closed 2026-05-18 MB11/MB12 real handlers: `TaskExit(11)`, `WriteConsole(60)`, `MemMap(1)` stub, `IpcCreateChannel(20)`/`IpcDestroyChannel(21)`/`IpcSend(22)`/`IpcReceive(23)`. STAR fix MB11.1 (`STAR[63:48]=0x10` → CS=0x23, SS=0x1B per Intel SDM). Capability gate via in-kernel `KernelCapabilityCheck` trait + `StubCapabilityProvider` — swap-in compatibile col futuro `Ed25519CapabilityProvider` MB13).
- [x] **P6.5b — ELF64 loader** (closed 2026-05-16 MB5 + esteso MB11 con `Elf64::map_and_load_into` for explicit-root AddressSpace).
- [x] **P6.6 — Typed message-passing IPC** (closed 2026-05-18 MB12 — `KernelIpcRegistry` concreta (`BTreeMap`, niente `HashMap` per via di MB12.0c), `BackpressurePolicy::{Block,Drop,EvictOldest}`, wait queues per canale, capability check 2-livelli, 4 syscall handler operativi, `task_exit` yields se runnables presenti, retry-loop sender/receiver su `WakeAction::Block`. Integration test `mb12_ipc_cross_process.rs` 8 verdi. ADR-0005 `accepted`).
- [~] **P6.7 — User-space driver model (NVMe, Ethernet/Wi-Fi, TEE)** — sbloccato da MB12 (IPC ✅) ma richiede ancora (a) MB13 Ed25519 capability reale, (b) MP/AP enable (LAPIC IPI + per-CPU data + TLB shootdown). Tracciato in P6.MB14+ (Phase 1.5).
- [ ] **P6.8 — First external security audit of kernel + capability system** — Phase 1 deliverable, bloccato da P4 funding + P6.7 done.

---

## P6.MB — Track B kernel milestones (granulare, post-v0.2.0)

Sezione introdotta 2026-05-19 per riflettere il flusso effettivo di lavoro sul branch `feat/kernel-mb11-userspace`. Ogni voce mappa 1:1 alle entries di `progress-omni.md` § 2.2 + `CHANGELOG.md` `[Unreleased]`/`[0.2.0]`.

| ID | Contenuto | Stato | Commit | ADR |
|---|---|---|---|---|
| MB1 | `BitmapFrameAllocator<const N>` + GDT iniziale | `[x]` | `119f3d8` | — |
| MB2 | `PageMapper` x86_64 walker + `map_4k`/`unmap_4k` | `[x]` | `102ec7a` | — |
| MB3 | IDT + handler #DE/#DF/#GP/#PF (CR2 dump) | `[x]` | `657d7d1` | — |
| MB4 | `SYSCALL`/`SYSRET` MSR setup + `INT 0x80` fallback | `[x]` | `f2e88da` | — |
| MB5 | ELF64 loader (parser + segment mapper) | `[x]` | `960e440` | — |
| MB6 | Round-robin scheduler + `omni_context_switch` asm | `[x]` | `27720ee` | — |
| MB7 | LAPIC xAPIC + PIC disable + `sti` + `TICK_COUNT` | `[x]` | `27720ee` | — |
| MB8 | Preemption from LAPIC timer + `need_resched` | `[x]` | `5d9989b` | — |
| MB9 | `PageMapper` huge-page aware + direct-map validator | `[x]` | `926a37e` | [0001](docs/adr/0001-mb9-paging-huge-page-aware.md) |
| MB10 | Kernel stack isolation + guard page | `[x]` | `8c1496a` | [0002](docs/adr/0002-mb10-kernel-stack-isolation.md) |
| MB11 | Primo userspace Ring 3 + per-process CR3 + STAR fix | `[x]` | `22289e1` + `c743173` | [0004](docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md) |
| MB12 | IPC reale (queue + capability stub + multi-task user) | `[x]` | `60f3a82` | [0005](docs/adr/0005-mb12-ipc-message-passing.md) |
| **MB13** | **`omni-capability` integration reale (Ed25519) + bare-metal smoke fix + SIMD `force-soft`** | **`[x]`** (MB13.a + MB13.b + MB13.c + MB13.d + MB13.f + MB13.g + MB13.h + MB13.e chiusi 2026-05-19; ADR-0006 `accepted`) | `5e907f8` | [ADR-0006](docs/adr/0006-mb13-omni-capability-integration.md) |
| MB14.a | Per-CPU descriptor scaffold + BSP LAPIC ID identification | `[x]` (chiuso 2026-05-19) | `3f38514` | — |
| MB14.b | `IA32_GS_BASE` per-CPU pointer + `swapgs` syscall entry + GS-relative `current_cpu()` | `[x]` (chiuso 2026-05-19) | `c30221f` | — |
| MB14.c.1 | ACPI MADT parser + bare-metal `enumerate_cpus` (RSDP→XSDT/RSDT→MADT walker) | `[x]` (chiuso 2026-05-19) | `e964a9d` | — |
| MB14.c.2.a | INIT-SIPI ICR encoder (xAPIC + x2APIC) + dry-run `start_aps` orchestrator | `[x]` (chiuso 2026-05-19) | `ad3b372` | — |
| MB14.c.2.b.1 | Pure-function trampoline blob (16/32/64-bit) + temp GDT + temp identity PML4/PDPT/PD builders + byte-exact host tests | `[x]` (chiuso 2026-05-19) | `176010f` | — |
| MB14.c.2.b.2 | Bare-metal emplacement (alloc 3 frames + materialize temp PML4/PDPT/PD + identity-map trampoline page in active CR3 + copia blob a `0x8000`) | `[x]` (chiuso 2026-05-19) | `bcf5ed7` | — |
| MB14.c.2.c | Live INIT-SIPI-SIPI fire + AP landing stub + ack barrier + `kmain_ap` higher-half park entry + PIT delays | `[x]` (chiuso 2026-05-19) | `6f77cc4` | [ADR-0007](docs/adr/0007-mb14-mp-ap-startup.md) |
| MB14.c.2.d | Real per-AP init (AP_SLOTS + per-AP kstack/IST/TSS + GDT extended to 69 slots + ApRuntimeControl + real `kmain_ap` asm: lgdt/lidt/wrmsr GS_BASE/ltr/park) | `[x]` (chiuso 2026-05-19) | `f23c4b9` | — |
| MB14.d | TLB shootdown IPI vector `0xFD` + `ipi::send_to_all_except_self` + `mm::flush_tlb_range` + 0xFD ISR (descriptor with VA range + ack counter + generation) | `[x]` (chiuso 2026-05-19) | `8868484` | — |
| MB14.e | `sti` su AP idle park + per-CPU run-queue scaffold (`per_cpu_run_queue` con `MAX_CPUS` slot, SpinLock, work-stealing back-of-lowest-priority) + boot-log smoke | `[x]` (chiuso 2026-05-19) | `40cbfa7` | — |
| MB14.f | AP LAPIC enable (`kernel_ap_lapic_init` SIVR+TPR+timer) + x2APIC awareness (mode detect + MSR primitives + 32-bit LAPIC ID via CPUID leaf 0xB) + ADR-0008; chiude MB14.e.4 follow-up | `[x]` (chiuso 2026-05-20) | — | [ADR-0008](docs/adr/0008-mb14f-per-cpu-scheduling-protocol.md) |
| MB14.g | Per-CPU plumbing: `PerCpu` esteso con `tick_count` + `need_resched` atomici per-CPU; LAPIC timer ISR scrive solo `current_cpu()`; `crate::TICK_COUNT` global rimosso; `RoundRobinScheduler::enqueue_for_cpu`/`pick_next_for_cpu` dual-write/read tra `per_cpu_run_queue` e legacy mirror; smoke `[mb14.g]` + 7 host test | `[x]` (chiuso 2026-05-20) | `a109656` | — |
| MB14.h.1 | AP-side observer dispatcher: `PerCpu.dispatch_observations` atomic counter + `bare_metal::ap_dispatch::kernel_ap_dispatch_observe` invocato dal branch AP di `kernel_check_need_resched` (pop dalla per-CPU run-queue con stealing + counter increment + drop dell'id, **no context switch**); smoke `[mb14.h.1] ap_dispatch observed=N (ok\|timeout\|BSP-only)` + 4 host test; ADR-0009 | `[x]` (chiuso 2026-05-20) | (this commit) | [ADR-0009](docs/adr/0009-mb14h-ap-dispatch-loop.md) |
| MB14 | MP/AP enable + TLB shootdown cross-AS (Phase 1.5) | `[~]` (MB14.a + MB14.b + MB14.c.* + MB14.d + MB14.e + MB14.f + MB14.g + MB14.h.1 chiusi; MB14.h.2 open) | — | — |

### P6.MB13 — `omni-capability` integration reale

- **Status:** `[x]` (MB13.a + MB13.b + MB13.c + MB13.d + MB13.f + MB13.g + MB13.h + MB13.e chiusi 2026-05-19; ADR-0006 `accepted`)
- **Priority:** P6 / High
- **Effort:** 1-2 giornate (gating SIMD + glue + nuovi test) + 0.5-1 giornata per il fix triple-fault
- **Dependencies:** MB12 ✅; nessuna esterna
- **ADR di chiusura:** ADR-0006 (da scrivere — capability dispatch + bare-metal ABI extension)
- **Rationale:** la pipeline MB12 ha consegnato uno `StubCapabilityProvider` interno (subject byte-compare + action shape-match, niente Ed25519). MB13 chiude la promessa Phase 1 "Capability-based security primitives implemented" sostituendo lo stub con un provider reale che chiama `omni_capability::CapabilityToken::verify_full`. Tre work-package indipendenti convergono qui.

#### P6.MB13.a — `force-soft` SIMD su `sha2` + `poly1305` + `curve25519-dalek`

- **Status:** `[x]` (closed 2026-05-19)
- **Effort:** 0.5 giornata (delivered)
- **Deliverables (delivered):**
  - **Workspace `.cargo/config.toml`** (nuovo) — rustflags target-conditional per `x86_64-unknown-none`:
    - `--cfg poly1305_force_soft` (portable backend per `poly1305 0.8`).
    - `--cfg chacha20_force_soft` (portable backend per `chacha20 0.9`).
    - `--cfg curve25519_dalek_backend="serial"` (serial backend per `curve25519-dalek 4.1`).
    - `--cfg sha2_backend="soft"` (portable backend per `sha2 0.11`).
  - **`crates/omni-crypto/Cargo.toml`** — `[target.x86_64-unknown-none.dependencies]` con `sha2_010_force_soft = { package = "sha2", version = "0.10", default-features = false, features = ["force-soft"] }` per attaccare la feature `force-soft` all'istanza `sha2 0.10` portata dai dalek (digest 0.10). Cargo unifica per versione risolta.
  - **`crates/omni-crypto/src/kdf.rs`** — `Zeroize`/`ZeroizeOnDrop` import gating dietro `#[cfg(feature = "rng")]` (era `unused_imports` warning sulla build bare-metal).
- **Alternativa A** (documentata in ADR-0005 § Migration) **NON adottata**: l'estrazione di `omni-crypto-verify` come crate separato sarebbe stata più chirurgica ma avrebbe rotto l'API surface. La passthrough Cargo + cfg flags mantiene la API stabile e produce lo stesso effetto.
- **Acceptance (verified):**
  - `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` clean (era: LLVM ICE su poly1305 + sha2 0.10 + sha2 0.11).
  - `cargo clippy -p omni-crypto --target x86_64-unknown-none --no-default-features -- -D warnings` clean.
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` clean (regression).
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.

#### P6.MB13.b — Boot-path fix: ET_DYN/PIE kernel (triple-fault smoke)

- **Status:** `[x]` (closed 2026-05-19)
- **Effort:** 0.5 giornata (delivered)
- **Rationale:** `mb12-userprobe` (e per estensione `mb11-userprobe`) triple-fault su Proxmox VMID 103 / QEMU+OVMF perché il kernel ELF era `ET_EXEC` con `p_vaddr = 0x200000` (PML4 index 0). `bootloader_api` 0.11 non rilocca `ET_EXEC` → kernel finiva in lower half → `AddressSpace::new_with_kernel_half` (clone solo entries 256..511) → al `mov cr3` dentro `enter_user_mode` la pagina con l'istruzione successiva era non-mappata → triple fault.
- **Soluzione adottata (Opzione (a) — ET_DYN/PIE kernel):**
  - **`kernel-runner/.cargo/config.toml`** — rimossi i flag `-C relocation-model=static` + `-C link-arg=--no-pie`. Il target spec `x86_64-unknown-none` ha già `position-independent-executables = true` (Rust 1.83+), quindi il linker produce nativamente un ELF `ET_DYN` con addressing RIP-relative. Il file ora contiene solo un commento esplicativo del cambio MB13.b, così che la merge dei rustflags del workspace `.cargo/config.toml` (force-soft SIMD cfgs) avvenga in modo pulito.
  - **`kernel-runner/src/main.rs`** — `BOOTLOADER_CONFIG` ora imposta `mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`. `bootloader 0.11` applica le relocazioni RIP-relative del kernel ELF spostando l'immagine in upper half (PML4 indices ≥ 256), assieme a `kernel_stack`, `boot_info`, `framebuffer` e `physical_memory`. Tutte queste mapping cadono nella metà clonata per riferimento dal CR3 di boot in `AddressSpace::new_with_kernel_half`, quindi rimangono live dopo il `mov cr3` di `enter_user_mode`.
- **Alternative documentate (non adottate):** opzione (b) linker script con `p_vaddr` upper-half hard-coded (più invasivo, perde la dinamicità del bootloader); opzione (c) trampoline page aliased cross-AS (mitigazione, non risoluzione del root cause).
- **Acceptance (verified):**
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean (ET_DYN PIE).
  - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe -- -D warnings` clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` clean (regression).
  - Test integration `mb12_ipc_cross_process.rs` verdi (8/8) — non affetti dal fix bare-metal.
  - Build Info panel aggiornato a Active=`MB13.b ET_DYN upper-half`, Next=`MB13.c omni-capability dep`.
  - Validazione smoke completa su Proxmox VMID 103 deferred a deploy-time (vedi `progress-omni.md` § "Verifica MB13.b").

#### P6.MB13.c — `omni-capability` come dep di `omni-kernel`

- **Status:** `[x]` (closed 2026-05-19)
- **Effort:** 1 giornata (delivered)
- **Deliverables (delivered):**
  - **`crates/omni-types/Cargo.toml` + `src/lib.rs` + `src/identity.rs`** — split `id-generation` (default ON, runtime constructors) in `id-types` (types only, no `getrandom`) + `id-generation` (superset: `id-types` + `getrandom`). `uuid` dichiarata direttamente con `features = ["serde"]` (no `v4`) — la helper `random_uuid_bytes` ha sempre usato `Uuid::from_bytes` quindi `v4` non era necessaria. Net: `omni-types` ora compila su `x86_64-unknown-none` con solo `id-types`.
  - **`crates/omni-capability/Cargo.toml`** — declares `omni-types/id-types` come hard requirement (path + version + explicit `default-features = false`); nuove feature `mint` (default-on, gates `CapabilityToken::mint` + `attenuation::attenuate` + `omni-types/id-generation` + `omni-crypto/rng`) e `bare-metal` (marker che forwarda `omni-crypto/bare-metal`). dev-deps re-enablano `mint` per i test.
  - **`crates/omni-capability/src/scope.rs`** — aggiunte `Action::IpcSend`, `Action::IpcRecv`, `Resource::IpcChannel(u64)` (semver-safe via `#[non_exhaustive]`). Subset relation per `IpcChannel` è uguaglianza (handle opaco kernel; no wildcard MB13.c). +5 unit test.
  - **`crates/omni-capability/src/{attenuation.rs,token.rs}`** — gating `#[cfg(feature = "mint")]` su `attenuate` e `CapabilityToken::mint`; verify path resta sempre disponibile. `use` statement gated correttamente per evitare unused-import warnings sulla build bare-metal.
  - **`crates/omni-kernel/Cargo.toml`** — `omni-capability = { ..., default-features = false, features = ["bare-metal"] }` come dep runtime + dev-deps con `mint`/`id-generation`/`rng` per i test host.
  - **`crates/omni-kernel/src/capabilities.rs`** — nuovo `Ed25519CapabilityProvider` con tre superfici: (a) `verify_signature_only(token)` — Ed25519 sig only; (b) `verify_signed_token(token, now)` — full verify via `CapabilityToken::verify_full` + `StubAttestation` bound a `node_id_bytes` + empty `RevocationList`; (c) `impl KernelCapabilityCheck::verify` — O(1) shape match identico allo stub (drop-in replacement per-IPC). Il provider è esposto al kernel ma **non ancora wired nei syscall IPC** — il plumbing dei token postcard via `IpcCreateChannel` è MB13.d. `StubCapabilityProvider` resta il default del boot wiring. +6 unit test.
- **Acceptance (verified):**
  - `cargo build -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal` clean (era: `unresolved import omni_types::identity`).
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` clean.
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` clean.
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --no-default-features --features mb12-userprobe` clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features {bare-metal, mb12-userprobe} -- -D warnings` clean.
  - `cargo clippy -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` clean.
  - `cargo test -p omni-capability` → 64 + 7 + 5 = 76 pass (5 new in `scope.rs`).
  - `cargo test -p omni-kernel --lib capabilities` → 11/11 ok (6 new + 4 stub regressions + 1 carryover).
  - `scripts/check-no-blanket-allow.sh` → ok (12 crate roots).
  - Build Info panel updated a `Active = MB13.c Ed25519 cap provider`, `Next = MB13.d IpcCreateChannel ABI`, `Track B = MB1-MB12 OK, MB13.a/b/c OK`, `Phase 1 ≈ 72%`, `Tests = 432+ workspace pass`.

#### P6.MB13.d — `IpcCreateChannel` syscall ABI extension

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort effettivo:** 0.5 giornata
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/capabilities.rs`** — nuovo helper `decode_and_authenticate_token(bytes, expected_action, provider, now) -> KernelResult<KernelPrincipal>`. Decodifica postcard via `omni_types::wire::decode_canonical`, esegue `Ed25519CapabilityProvider::verify_signed_token` (signature + time window + TEE binding), valida `scope.action` per slot, accetta qualunque `Resource::IpcChannel(_)` (rebind a runtime).
  - **`crates/omni-kernel/src/ipc.rs`** — nuovo `KernelIpcRegistry::create_channel_signed(owner, policy, send_token_bytes, recv_token_bytes, &provider, now)`. Entrambi `None` → delegate a `StubCapabilityProvider` (legacy MB12 byte-per-byte). Altrimenti decode + verify per slot, canale registrato con subject verificati nei rispettivi `send_subject`/`recv_subject`.
  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs`** — ABI a 6 arg: `(queue_depth, backpressure, tee_bound, send_ptr, recv_ptr, lens)` con `lens = send_len:u32 \| (recv_len:u32 << 32)`. Cap on-stack 1 KiB per token. `user_range_ok` bounds check + on-stack `[u8; 1024]` copy → `create_channel_signed`. `now` da `bare_metal::arch::rtc_seconds`.
  - **`crates/omni-kernel/src/bare_metal/userprobe_mb12.rs`** — boot wiring swap a `Ed25519CapabilityProvider::placeholder()` via `create_channel_signed(...None, None, ..., 0)`. Behaviour identico (la registry forwarda al stub provider per la legacy open-channel call); l'indirezione documenta che Ed25519CapabilityProvider è ora il provider canonico.
  - **`crates/omni-kernel/tests/mb13_capability_signed.rs`** — +11 test: 7 su `decode_and_authenticate_token` (happy path, bit-flipped postcard, action mismatch, pre-window/post-window time, TEE mismatch, non-IpcChannel resource, truncated postcard) + 4 su `create_channel_signed` (subject populated, open-channel delegate, invalid bytes rejected, end-to-end per-IPC gate).
- **Userprobe ELF MB13 follow-up:** i nuovi ELFs sender/receiver con token canned firmati build-time NON sono inclusi in MB13.d — sono tracciati come MB13.e/MB14 follow-up (richiedono build.rs ricorsivo + signing host-side; complessità non giustificata per chiudere l'ABI). I 11 host-side integration test su `KernelIpcRegistry::create_channel_signed` coprono il decode + verify + register cycle end-to-end.
- **Acceptance:**
  - [x] `cargo build --workspace --all-features` clean.
  - [x] `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` clean.
  - [x] `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` clean.
  - [x] `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean (bootable image).
  - [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - [x] `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features {bare-metal, mb12-userprobe} -- -D warnings` clean.
  - [x] `cargo test -p omni-kernel --features bare-metal --test mb13_capability_signed` → 11/11 pass.
  - [x] `cargo test -p omni-kernel --features bare-metal --tests` (integration only) → 46/46 pass (mb11 6 + mb12 8 + mb13 11 + panic_record 5 + boot_info 7 + heap 9).
  - [x] `scripts/check-no-blanket-allow.sh` → ok (12 crate-root files).
  - [x] Build Info panel a `Active = MB13.d IpcCreateChannel ABI`, `Next = MB13.e PR + intermediate tag`, `Phase 1 ≈ 75%`, `Tests = 443+ workspace pass`.
  - **Note:** `cargo test -p omni-kernel --lib` su `x86_64-unknown-linux-gnu` segfaulta a `dispatcher_time_monotonic_returns_u64` (CMOS port I/O privilegiato in userspace Linux) e al teardown di `bare_metal::paging::tests`. Entrambi sono carryover preesistenti (item §4.5 #16 in progress-omni.md). Confermato pre-esistente via `git stash` + retry: SIGSEGV riproducibile su HEAD prima di MB13.d.
  - **ADR-0006:** non scritto in MB13.d — l'ABI extension è additiva al `IpcCreateChannel` documentato in ADR-0005 § Migration (che già menziona "MB13: omni-capability integration ... swap to a real Ed25519CapabilityProvider"). Tracciato per MB13.e come "ADR-0005 amendment" o ADR-0006 stand-alone.

#### P6.MB13.f — `enter_user_mode` kernel-stack swap (first-dispatch smoke fix)

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort effettivo:** 0.5 giornata
- **Rationale:** dopo MB13.b la VM superava il triple-fault del CR3 e raggiungeva `[mb12] handing off to user tasks`, ma il primo dispatch user-side moriva silenziosamente (nessun `[user] exit=0`, nessun `ping`). Indagine: `enter_user_mode` eseguiva `mov cr3, dest_cr3` mentre `RSP` puntava ancora allo stack del chiamante. Nel path MB12 first-dispatch invocato da dentro un syscall handler (`SYSCALL` su x86_64 non commuta `SP`), quello stack era lo *user stack del task uscente*. Dopo il `mov cr3` quella pagina non era più mappata nel nuovo PML4 e il primo `push {ss}` produceva un page-fault → triple-fault → VM reset, prima ancora che il sender potesse eseguire una qualsiasi istruzione Ring 3.
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/bare_metal/usermode.rs`** — `enter_user_mode` aggiunge un nuovo parametro `kernel_stack_top: u64` e fa `mov rsp, {kstk}` **prima** del `mov cr3`. Lo stack di destinazione è nel range MB10 isolato `[KERNEL_STACK_VA_BASE, KERNEL_STACK_VA_END)` (PML4 index ≥ `0x180`, kernel half), mirrored per riferimento in ogni PML4 per-process via `AddressSpace::new_with_kernel_half` → resta mappato dall'altra parte del CR3 reload. Stub non-x86_64 aggiornato in tandem.
  - **`crates/omni-kernel/src/scheduling.rs`** — `RoundRobinScheduler::yield_current` first-dispatch path passa `kernel_stack_top` (già calcolato in stack frame come `kernel_stack_va + KERNEL_STACK_SIZE` per il TSS.rsp0 update MB12.0a). Commento aggiornato per rimuovere la nota stale "MB12 bare-metal limitation" (era valida pre-MB13.b).
  - **`crates/omni-kernel/src/lib.rs`** — MB11 single-task dispatch (kmain → enter_user_mode diretto) passa `pcb.task.kernel_stack_va + scheduling::KERNEL_STACK_SIZE`. Nel path MB11 il caller-side RSP è già su upper-half (boot stack), ma il fix è uniforme.
- **Acceptance (verified):**
  - [x] `cargo build --workspace --all-features` clean (0 warning).
  - [x] `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe` clean.
  - [x] `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean.
  - [x] `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe -- -D warnings` clean.
  - [x] `cargo test -p omni-kernel --tests` integration suite (8 + 6 + 11 + boot_info/panic_record/heap) tutta verde — il fix è alle linee asm bare-metal, non tocca il path host-test.
  - [x] Build Info panel aggiornato: `Active = MB13.f iretq kstk-swap`, `Track B = MB1-MB12 OK, MB13.a-f OK`, `Phase 1 ≈ 77%`.
  - **Pending:** validazione smoke completa su Proxmox VMID 103 (deferred a deploy-time, vedi § "Verifica MB13.f" in `progress-omni.md`).
- **Note di analisi:** il bug latente sarebbe affiorato già a MB11 in teoria, ma nella pipeline MB11 il caller di `enter_user_mode` era `kmain` direttamente (RSP su boot stack, upper half, mirrored). Solo con MB12 — dove il first-dispatch viene innescato da un syscall handler che gira sullo user stack del task uscente — il path patologico è diventato raggiungibile. La pre-esistenza del bug non era visibile nei test host perché `enter_user_mode` ha un `#[cfg(target_arch = "x86_64")]` stub no-op su Linux host build.

#### P6.MB13.e — Chiusura ciclo (Ed25519 canonical + Stub `#[cfg(test)]` + ADR-0006)

- **Status:** `[x]` (chiuso 2026-05-19 post-`e3f7742`)
- **Effort:** 0.5 giornata (consegnato)
- **Deliverables consegnati:**
  - `Ed25519CapabilityProvider::placeholder()` ora wired in ogni `IpcCreateChannel` path: il fallback `(None, None)` di `create_channel_signed` forwarda il `provider` ricevuto invece di costruire un `StubCapabilityProvider`; il legacy `(0, 0)` path di `ipc_create_channel` instanzia un placeholder kernel-stack-local; il pre-create `userprobe_mb12` già usava il provider canonico dal MB13.d e i commenti sono stati allineati.
  - `StubCapabilityProvider` (struct + `KernelCapabilityCheck` impl) gated `#[cfg(test)]`. Unit test `src/ipc.rs::tests` e `src/capabilities.rs::tests` continuano a usarlo (sono in moduli `#[cfg(test)]`).
  - `crates/omni-kernel/tests/mb12_ipc_cross_process.rs` migrato a `Ed25519CapabilityProvider::placeholder()` (le integration test sotto `tests/` non vedono `cfg(test)` della libreria).
  - `docs/adr/0006-mb13-omni-capability-integration.md` `accepted`.
  - `CHANGELOG.md` `[Unreleased] § Added` aggiornato con MB13.e entry.
  - `progress-omni.md` § 2.2 milestone table riga `MB13` aggiornata a ✅; § 6 Phase 1 % aggiornato a ~82%.
  - Build Info panel: Active=`MB13.e closure + ADR-0006`, Next=`MB14 MP/AP + TLB shootdown`, Track B=`MB1-MB12 OK, MB13 closed`, Phase 1 ≈ 82%.
- **Release-management residuo:**
  - Apertura della PR `feat/kernel-mb11-userspace` → `main` con tag intermedio (`v0.3.0-alpha.1` minor — c'è nuova ABI surface MB13.d) resta una decisione separata da pianificare quando il batch post-`v0.2.0` raggiunge una dimensione tale da giustificare un tag. Non blocca MB14.

### Acceptance criteria globali MB13

- [x] `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` clean (chiuso da MB13.a).
- [x] `cargo test --workspace --all-features` ≥ 432 pass (chiuso da MB13.d: 447+).
- [~] Smoke `mb12-userprobe` su QEMU+OVMF + Proxmox VMID 103 = serial output completo (MB13.f rimuove il triple-fault del primo `push` post-CR3; MB13.h chiude il TSS `ltr` wiring + IST stacks; validazione hardware deferred a MB13.e deploy step di questa stessa run).
- [~] Smoke `mb11-userprobe` su QEMU+OVMF = `[user] hello / [user] exit=0` (stesso set di fix; pending validazione manuale hardware).
- [x] `StubCapabilityProvider` ridotto a `#[cfg(test)]` mock (chiuso da MB13.e).
- [x] ADR-0006 `accepted` in `docs/adr/` (chiuso da MB13.e).

---

### P6.MB14 — Multi-processor enable + TLB shootdown

- **Status:** `[~]` (MB14.a `[x]` + MB14.b `[x]` + MB14.c.1 `[x]` chiusi 2026-05-19; MB14.c.2-f open)
- **Priority:** P6 / High (Phase 1.5)
- **Effort stimato (totale):** 5-10 giornate (MB14.a delivered in 0.2 giornate; il grosso è MB14.c AP startup + MB14.d/e TLB shootdown)
- **Dependencies:** MB13 ✅; nessuna esterna
- **ADR di chiusura:** ADR-0007 (da scrivere a chiusura di MB14.c — AP startup) e potenzialmente ADR-0008 (TLB shootdown protocol)
- **Rationale:** il kernel gira single-CPU; la LAPIC è pronta ma nessuna AP è stata svegliata. P6.7 (driver model user-space) richiede (a) per-CPU scheduling per evitare la contesa sul `RoundRobinScheduler` globale, (b) cross-CPU IPI per signalling driver-to-driver, (c) TLB shootdown broadcast quando un driver-process modifica un mapping cross-CPU. MB14 è il sequencing che porta il sistema multi-core, sblocca P6.7 e completa il deliverable Phase 1 "Drivers in user space".

#### P6.MB14.a — Per-CPU descriptor scaffold + BSP LAPIC ID

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** 0.2 giornata (delivered)
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** — aggiunto `read_lapic_id() -> Option<u32>` che legge il registro MMIO LAPIC offset `0x20` (xAPIC ID, bits 31:24 per Intel SDM Vol 3A § 10.4.6). `lapic_read` ora ha un caller reale → rimosso il `#[allow(dead_code)]`.
  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** (nuovo) — struct `PerCpu` con campi atomic `cpu_id`/`lapic_id`/`is_bsp`, sentinel `CPU_ID_UNINIT = u32::MAX` (non collide con gli xAPIC ID 8-bit), accessori `init_bsp` / `current_cpu` (BSP-only stub) / `bsp()`. +6 unit test.
  - **`crates/omni-kernel/src/lib.rs::kmain`** — chiamata `per_cpu::init_bsp(read_lapic_id())` dopo `lapic_init` success, prima di `sti`. Emette `[mb14.a] BSP cpu_id=0 lapic_id=<N>` sul COM1.
- **Acceptance (verified):**
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` clean.
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - Workspace test count 447+ → 453+ (+6 unit test `bare_metal::per_cpu::tests::*`); `cargo test -p omni-kernel --lib` SIGSEGV è carryover pre-esistente (item §4.5 #16 in progress-omni.md), nessuna regressione introdotta.
  - Build Info panel: Active=`MB14.a per-CPU identity`, Next=`MB14.b GS_BASE per-CPU ptr`, Track B=`MB1-MB13 OK, MB14.a wip`, Phase 1 ≈ 83%.

#### P6.MB14.b — `IA32_KERNEL_GS_BASE` per-CPU pointer + `swapgs` su Ring 3 → Ring 0

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** 0.4 giornata (delivered)
- **Dependencies:** MB14.a ✅
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** — `PerCpu` diventa `#[repr(C)]` con `self_ptr: AtomicU64` a offset 0 (layout deterministico). Nuovo `init_gs_base(pc: &'static PerCpu)` che (a) memorizza `&pc as u64` in `pc.self_ptr` (`Ordering::Release`), (b) scrive lo stesso puntatore in `IA32_GS_BASE` (MSR `0xC000_0101`, active in kernel mode) **e** in `IA32_KERNEL_GS_BASE` (MSR `0xC000_0102`, shadow swappato da `swapgs`). `current_cpu()` ora discrimina su `cfg(all(target_arch = "x86_64", target_os = "none"))`: bare-metal esegue `mov rax, gs:[0]` via inline asm; host/test resta `&BSP`. `wrmsr` privato modulo (duplicato locale rispetto a quello in `syscall_entry.rs`). +2 unit test (`self_ptr_field_at_offset_zero`, `init_gs_base_stamps_self_pointer`).
  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs`** — `omni_syscall_entry` emette `swapgs` come **prima** istruzione (prima di qualunque push) e come **ultima** prima di `sysretq` (dopo `pop rbx`). RAX intatto (`swapgs` non tocca GPR). Convenzione: ingresso SYSCALL trova active GS = user value; il primo `swapgs` flippa con per-CPU pointer da `IA32_KERNEL_GS_BASE`; il secondo prima di `sysretq` ripristina user GS base.
  - **`crates/omni-kernel/src/lib.rs::kmain`** — chiamata `per_cpu::init_gs_base(per_cpu::bsp())` subito dopo `init_bsp(lid)`, seguita da dump serial `[mb14.b] gs_base=<addr>`.
- **Acceptance (verified):**
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` clean.
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - Workspace test count 453+ → 455+ (+2 unit test `bare_metal::per_cpu::tests::*`); `cargo test -p omni-kernel --lib` SIGSEGV resta carryover pre-esistente (item §4.5 #16 in progress-omni.md), nessuna regressione introdotta dalla MB14.b.
  - Build Info panel: Active=`MB14.b GS_BASE per-CPU ptr`, Next=`MB14.c AP startup INIT-SIPI`, Track B=`MB1-MB13 OK, MB14.a-b wip`, Phase 1 ≈ 84%.
- **Note di scope (esplicitamente NON inclusi):**
  - swapgs condizionale (CS RPL == 3) nei 20+ ISR IDT sincroni e nel `omni_lapic_timer_handler` — la struttura asm corrente richiederebbe un prologo aggiuntivo per ogni stub. `mb12-userprobe` triple-faulta in MB13.g prima di osservare differenze; differito a MB14.b.1 / MB13.g closure.
  - swapgs prima di `iretq` in `enter_user_mode` — userspace-side flip ortogonale al kernel-side fix; seguirà MB13.g.
  - Refactor `msr.rs` condiviso fra `per_cpu.rs` e `syscall_entry.rs` — follow-up minore.

#### P6.MB14.c — AP startup via INIT-SIPI-SIPI + real-mode trampoline

- **Status:** `[~]` (MB14.c.1 + MB14.c.2.a chiusi 2026-05-19; MB14.c.2.b + MB14.c.2.c open)
- **Effort stimato:** 2-3 giornate (la più complessa di MB14)
- **Dependencies:** MB14.b ✅
- **Rationale:** ogni Application Processor parte in real mode a `0xFFFF_FFF0`. Per portarli in long mode serve un trampoline 16→32→64 bit a una pagina fisica nota (tipicamente `0x8000`), che inizializza GDT, abilita PAE+LME+paging e salta al kernel entry per-AP. Il BSP scrive (INIT, SIPI, SIPI) al LAPIC ICR di ogni AP.
- **Sub-block plan** (deciso 2026-05-19 — splittato per landing incrementale):
  - **MB14.c.2.a** ✅ — ICR encoder (xAPIC + x2APIC) pure-function + dry-run `start_aps` orchestrator. No LAPIC MMIO; encoding pinned by host-side tests vs Intel SDM Vol 3A § 10.6.1.
  - **MB14.c.2.b** (next) — real-mode trampoline a `0x8000` (16→32→64-bit) + identity-map della pagina trampolino + GDT/PML4 temporanee.
  - **MB14.c.2.c** (next) — flip `start_aps` da `DryRun` a `Live`: scrive ICR_HI/ICR_LO (o `IA32_X2APIC_ICR` MSR), wait 10 ms INIT clear, SIPI×2 con 200 µs spacing, ack barrier via atomic counter alimentato dal trampolino, allocazione per-AP `PerCpu` slot + `kmain_ap` entry. ADR-0007 `accepted` a questa chiusura.
- **Deliverables previsti (MB14.c.2.b + MB14.c.2.c):**
  - Trampoline a `0x8000` (codice 16-bit assembly + paging table embedded).
  - `bare_metal::mp::start_aps` flippato a live mode (oggi sempre dry-run).
  - Per-AP `kmain_ap` entry che setta GS_BASE proprio, completa init, entra in scheduler loop.
  - ADR-0007 (status `accepted` a chiusura).

##### P6.MB14.c.1 — ACPI MADT cpu enumeration

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** 0.3 giornate (delivered)
- **Dependencies:** MB14.b ✅
- **Rationale:** prima di lanciare INIT-SIPI-SIPI bisogna sapere quanti AP esistono e quali LAPIC ID hanno. Il firmware ACPI MADT (signature `APIC`) li elenca come `Processor Local APIC` (type 0x00, 8 byte) o `Processor Local x2APIC` (type 0x09, 16 byte). MB14.c.1 estrae la lista; MB14.c.2 la consumerà.
- **Deliverables:**
  - **`crates/omni-kernel/src/bare_metal/mp.rs`** — nuovo modulo con `parse_madt(&[u8]) -> Result<CpuTopology, MadtError>` pure-function (decoder ICS table; skips IO APIC / NMI / altri ICS unknown senza errare; reject `length=0` per evitare loop infiniti e `length` che superi la table end; capienza `MAX_CPUS = 32`). `enumerate_cpus(rsdp_phys, phys_offset)` unsafe wrapper bare-metal che attraversa RSDP signature `"RSD PTR "` → XSDT (ACPI ≥ 2.0) o RSDT → MADT, modellato su `arch::find_pm1a_cnt_from_fadt` per la sicurezza del physical-memory window. Host-side stub no-op su non-x86_64.
  - **`crates/omni-kernel/src/lib.rs`** — hook in `kmain` post-MB14.b che, se `boot_info.rsdp_addr` + `physical_memory_offset` sono entrambi disponibili, chiama `bare_metal::mp::enumerate_cpus` e logga su serial `[mb14.c.1] MADT cpus=N enabled=M` + per-entry `apic_id`/`x2apic`/`enabled`. Failure-tollerante: niente RSDP → log `BSP only`, niente boot fault.
  - **+12 unit test** in `bare_metal::mp::tests::*`: truncated buffer, bad signature, length mismatch, MADT empty, single BSP Local APIC, disabled Local APIC, x2APIC con 32-bit ID, multipli CPU + IO APIC interleaved, unknown ICS skip, zero-length ICS reject, ICS oltre table-end reject, troppi CPU reject. Tutti host-side via hand-crafted byte buffer (no physical memory window richiesta).
  - Workspace test count 455+ → 467+; `cargo clippy --workspace --all-features --all-targets` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean.
  - Build Info panel: Active=`MB14.c.1 MADT cpu enum`, Next=`MB14.c.2 INIT-SIPI trampoline`, Track B=`MB1-MB13 OK, MB14.a-c.1 wip`, Phase 1 ≈ 85%.
- **Note di progettazione:**
  - `parse_madt` è pure-function su `&[u8]` per essere host-testabile *senza* alcun mock di `unsafe` bare-metal. La parte unsafe è confinata in `enumerate_cpus` + `find_table_phys`, riusando lo stesso pattern documentato in ADR di sicurezza del FADT walker.
  - `CpuEntry.acpi_uid` widened a `u32` per uniformità tra xAPIC (8-bit acpi_processor_id) e x2APIC (32-bit acpi_processor_uid). `CpuEntry.x2apic: bool` consente all'orchestrator MB14.c.2 di scegliere fra xAPIC ICR encoding (memory-mapped) e x2APIC MSR encoding (`IA32_X2APIC_ICR`).
  - `MAX_CPUS = 32` lascia margine per Proxmox dev VM (2-4 vCPU oggi) e desktop-class workload futuri. MB14.e raise quando i per-CPU run-queue saranno dimensionati.
  - **Pending:** validazione smoke su Proxmox VMID 103 a deploy-time per confermare che il MADT della VM venga letto correttamente (tipicamente 1 entry abilitata, perché la VM ha 1 vCPU di default).

##### P6.MB14.c.2.a — INIT-SIPI ICR encoder + dry-run orchestrator

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** 0.4 giornate (delivered)
- **Dependencies:** MB14.c.1 ✅
- **Rationale:** prima di toccare LAPIC MMIO o costruire il trampolino real-mode, pin-down al layout-bit-esatto dell'Interrupt Command Register. Una stray bit in `delivery_mode` triple-faulta il BSP; una stray bit nel campo destination manda l'IPI alla CPU sbagliata. MB14.c.2.a sposta quel rischio in `cargo test` host-side via codifiche pure-function pinned al Intel SDM Vol 3A § 10.6.1 (xAPIC) / § 10.12.9 (x2APIC). Il dry-run orchestrator esercita lo stesso loop per-AP che MB14.c.2.c userà, in modo che il rischio residuo della prossima sotto-step sia limitato al solo trampolino + LAPIC MMIO write.
- **Deliverables:**
  - **`crates/omni-kernel/src/bare_metal/mp.rs`** — `IcrDeliveryMode` (Fixed/LowestPriority/SMI/NMI/INIT/StartUp), `IcrDestinationMode` (Physical/Logical), `IcrLevel` (Deassert/Assert), `IcrTriggerMode` (Edge/Level), `IcrDestinationShorthand` (NoShorthand/Self/AllIncludingSelf/AllExcludingSelf) — tutti `#[repr(u8)]` con valori spec. `IcrCommand { vector, delivery_mode, destination_mode, level, trigger_mode, shorthand, destination_apic_id }` + costruttori `init_assert(apic_id)` e `sipi(apic_id, trampoline_page)`. `encode_icr_xapic(cmd) -> (u32, u32)` (high/low dwords) + `encode_icr_x2apic(cmd) -> u64` (single MSR write). `start_aps(topology, bsp_apic_id, trampoline_page, mode) -> StartApsReport` con `StartApsMode::{DryRun, Live}` (Live downgrades silently a DryRun fino a MB14.c.2.c).
  - **`crates/omni-kernel/src/lib.rs`** — hook in `kmain` post-MB14.c.1 che chiama `bare_metal::mp::start_aps(&topo, lid, 0x08, DryRun)` e logga `[mb14.c.2.a] start_aps targeted=N sequenced=N (dry-run)`.
  - **+13 unit test** in `bare_metal::mp::tests`: `xapic_init_encoding_matches_intel_layout` (low=0x4500/high=0x0100_0000 per apic_id=1), `xapic_sipi_encoding_matches_intel_layout` (low=0x4608 per page=0x08), `xapic_destination_truncates_to_eight_bits`, `x2apic_init_encoding_packs_destination_in_high_dword`, `x2apic_sipi_packs_trampoline_and_destination`, `encoder_emits_zero_for_default_init_fields`, `shorthand_all_excluding_self_encodes_to_bits_18_19`, `start_aps_dry_run_targets_every_enabled_non_bsp`, `start_aps_skips_bsp_even_when_listed_first`, `start_aps_skips_disabled_entries`, `start_aps_with_trampoline_zero_forces_dry_run`, `start_aps_mode_live_downgrades_in_mb14_c_2_a`, `start_aps_returns_zero_targets_on_uniprocessor`. Workspace test count 467+ → 480+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean.
  - Build Info panel: Active=`MB14.c.2.a ICR encoder`, Next=`MB14.c.2.b trampoline @0x8000`, Track B=`MB1-MB13 OK, MB14.a-c.2.a wip`, Phase 1 ≈ 86%.
- **Note di progettazione:**
  - L'encoder è `const fn` per permettere la generazione di costanti compile-time (es. `const INIT_BROADCAST: u64 = encode_icr_x2apic(...)` per MB14.d), e per garantire zero overhead al call site.
  - `StartApsMode::Live` esiste già nel surface API ma è marcato "downgrades silently" finché MB14.c.2.c non landa. Questa scelta riduce il churn: kmain non dovrà cambiare la signature della call al flip.
  - `trampoline_page = 0` forza dry-run anche con `mode = Live` — SIPI vector 0 salterebbe nel IVT, mai valido. Questo è un guardrail contro il bug più tipico ("ho dimenticato di passare l'indirizzo del trampolino").
  - **Pending validazione Proxmox:** confermare che il log `[mb14.c.2.a] start_aps targeted=0 sequenced=0 (dry-run)` appaia sul COM1 della VMID 103 (1 vCPU → 0 AP targeted).

##### P6.MB14.c.2.b.1 — Pure-function trampoline builder

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** 0.4 giornate (delivered)
- **Dependencies:** MB14.c.2.a ✅
- **Rationale:** prima di emplazare 256 byte di codice macchina a `0x0000_8000` e identity-mappare la pagina, pin-down byte-per-byte del trampolino + GDT + page-table entries contro Intel SDM Vol 2 (instruction reference) e Vol 3A § 3.4.5 (segment descriptor) / § 4.5 (IA-32e paging). Una stray bit nel `mov cr0, eax` triple-faulta l'AP all'istante; una `PS=1` mal posizionata nel PDE causa `#GP` durante l'attivazione di `CR0.PG`. MB14.c.2.b.1 sposta quel rischio in `cargo test` host-side mantenendo invariata l'API che MB14.c.2.b.2 consumerà per scrivere fisicamente il blob.
- **Deliverables:**
  - **`crates/omni-kernel/src/bare_metal/mp_trampoline.rs`** (nuovo modulo, sibling di `mp.rs`):
    - `build_trampoline_blob(base_paddr, temp_pml4_paddr, kernel_ap_entry) -> [u8; 256]` — emette il blob 16→32→64 bit con relocations puntuali (`0x0E` GDTR disp16, `0x1C` PM32 off32, `0x32` PML4 paddr, `0x5C` LM64 off32, `0x64` 64-bit entry imm64). Sezioni: `RM16=0x00..0x22` (cli/cld/xor ax/seg load/`66 0F 01 16 …` o32 lgdt/PE-set `0F 22 C0`/`66 EA …` o32 far jmp), `PM32=0x22..0x62` (data selector load/CR3 reloc/CR4.PAE/EFER.LME via rdmsr+wrmsr/CR0.PG+PE/`EA … 18 00` far jmp), `LM64=0x62..0x6E` (REX.W `48 B8 …` mov rax + `FF E0` jmp rax). Padding NOP fino a `GDT=0x70`.
    - `build_temp_gdt() -> [u64; 4]` const fn: null + `0x00CF_9A00_0000_FFFF` (32-bit code) + `0x00CF_9200_0000_FFFF` (32-bit data) + `0x00AF_9A00_0000_FFFF` (64-bit code, L=1).
    - `build_temp_gdtr(gdt_base) -> [u8; 6]` const fn — `lim16 || base32`.
    - `pml4_entry_pdpt(child)` / `pdpt_entry_pd(child)` / `pd_entry_2mib(target)` const fn — PTE bit-pack (P/RW/PS) + frame mask `0x000F_FFFF_FFFF_F000` (4 KiB) / `0x000F_FFFF_FFE0_0000` (2 MiB).
    - `build_temp_identity_paging(pdpt_paddr, pd_paddr) -> TempIdentityPaging { pml4, pdpt, pd }` const fn — identity-map dei primi 2 MiB via singolo PDE PS=1 con target_paddr=0.
    - Costanti pubbliche `TRAMPOLINE_BLOB_SIZE=256`, `TRAMPOLINE_OFFSET_RM16/PM32/LM64/GDT/GDTR`, `TRAMPOLINE_GDT_ENTRIES=4`, `TRAMPOLINE_GDT_SIZE=32`, `TRAMPOLINE_GDTR_SIZE=6`, `TRAMPOLINE_SEL_CODE32=0x08/DATA32=0x10/CODE64=0x18`.
  - **`crates/omni-kernel/src/bare_metal/mod.rs`** — registra `pub mod mp_trampoline`.
  - **`crates/omni-kernel/src/lib.rs`** — hook in `kmain` post-MB14.c.2.a che chiama `build_trampoline_blob(0x8000, 0x9000, 0xFFFF_FFFF_8010_0000)` + `build_temp_gdt()`, conta i byte non-zero, e logga `[mb14.c.2.b.1] trampoline blob bytes=256 nonzero=N gdt_entries=4 (builder dry-run)`. Nessun write fisico — il blob viene dropped subito dopo il count.
  - **+30 unit test** in `bare_metal::mp_trampoline::tests::*`:
    - Blob prologue: `blob_starts_with_cli_cld`, `blob_loads_zero_segments_via_xor_ax_ax`.
    - LGDT encoding: `blob_loads_gdt_via_o32_lgdt` (verifica `66 0F 01 16 LO HI` + reloc disp16).
    - PE set: `blob_sets_pe_in_cr0` (`0F 20 C0` / `66 83 C8 01` / `0F 22 C0`).
    - 16→32 far jmp: `blob_16to32_far_jump_targets_pm32_section` (verifica off32+sel16=0x0008).
    - 32-bit transition: `blob_32bit_loads_data_selector_into_segregs`, `blob_32bit_loads_temp_pml4_into_cr3`, `blob_32bit_enables_pae_in_cr4`, `blob_32bit_sets_lme_via_efer_msr`, `blob_32bit_enables_paging_with_pe_pg`, `blob_32to64_far_jump_targets_lm64_section`.
    - 64-bit tail: `blob_64bit_loads_kernel_entry_and_jumps` (REX.W + 8-byte imm + `FF E0`).
    - Reloc isolation: `relocations_isolate_at_documented_offsets` (`0x32..0x36` PML4), `kernel_entry_relocation_changes_only_8_bytes` (`0x64..0x6C`).
    - GDT: `gdt_has_four_entries_with_canonical_layout`, `gdt_32bit_code_descriptor_decodes_correctly`, `gdt_64bit_code_descriptor_has_long_mode_flag`, `gdtr_pseudo_desc_packs_limit_and_base`.
    - GDT/GDTR embedding: `blob_embeds_gdt_at_documented_offset`, `blob_embeds_gdtr_at_documented_offset`.
    - PTE: `pml4_entry_sets_present_and_writable_and_carries_frame`, `pml4_entry_masks_low_12_bits_of_input`, `pdpt_entry_pd_has_ps_clear`, `pd_entry_2mib_sets_ps_and_carries_2mib_frame`, `pd_entry_2mib_masks_low_21_bits_of_input`.
    - Identity-paging: `identity_paging_links_pml4_pdpt_pd_in_order`, `identity_paging_zeroes_all_other_entries`.
    - Invarianti layout: `blob_size_is_one_page_or_less`, `section_offsets_are_monotonically_increasing` (compile-time `const _ : () = assert!(…)`), `selectors_match_gdt_slot_indices`.
  - Workspace test count 480+ → 510+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. Build Info panel: Active=`MB14.c.2.b.1 tramp builder`, Next=`MB14.c.2.b.2 emplacement`, Track B=`MB1-MB13 OK, MB14.a-c.2.b.1 wip`, Phase 1 ≈ 87%.
- **Note di progettazione:**
  - Section offsets fissati a `RM16=0x00 / PM32=0x22 / LM64=0x62 / GDT=0x70 / GDTR=0x90`. La spaziatura PM32=0x22 (anziché 0x21) deriva dal fatto che il far-jmp 16→32 con prefisso `0x66` è 8 byte (`66 EA + off32 + sel16`), non 7 — early-iteration bug catturato dal test `blob_16to32_far_jump_targets_pm32_section` prima del commit.
  - Il blob è 256 byte fissi anche se il codice utile è ~150 byte (terminato a `GDTR_OFF+6 = 0x96`). Il padding zero permette a MB14.c.2.b.2 di fare `copy_from_slice` su una page intera senza branch sui section bounds.
  - `build_temp_identity_paging` mappa solo i primi 2 MiB (singolo PDE PS=1). Sufficiente per il path trampoline ↔ kernel_ap_entry se quest'ultimo è raggiunto via low-memory stub; per il caso higher-half kernel reale, MB14.c.2.b.2 estenderà la mappa (1 GiB o composizione con il PML4 kernel attivo).
  - Tutte le primitive sono `const fn` (eccetto `build_trampoline_blob` che usa `let mut blob: [u8; 256]` + mutation), quindi MB14.c.2.b.2 può materializzare `pml4`/`pdpt`/`pd` come `const` statici se conveniente.
- **Pending validazione Proxmox:** confermare che il log `[mb14.c.2.b.1] trampoline blob bytes=256 nonzero=…` appaia sul COM1 della VMID 103 (single-CPU, no AP wake).

##### P6.MB14.c.2.b.2 — Emplacement bare-metal

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** ~0.5 giornata (allocator + mapper + direct-map writes already available)
- **Dependencies:** MB14.c.2.b.1 ✅
- **Rationale:** sposta dal builder puro alla materializzazione fisica. Tre frame allocati dal `BitmapFrameAllocator` ospitano la temp PML4 / PDPT / PD; i loro contenuti (generati da `build_temp_identity_paging`) sono scritti via direct-map del bootloader; la pagina trampolino `0x0000_8000` è identity-mappata nel CR3 attivo via `PageMapper::map_4k` (defensive — il copy del blob già passa per il direct-map, ma il mapping garantisce che la pagina sia raggiungibile via VA `0x8000` per inspection / debug futuro); il blob 256-byte è infine copiato a phys `0x8000` con `kernel_ap_entry` placeholder = `0xFFFF_FFFF_8010_0000` (sostituito da MB14.c.2.c con il vero per-AP entry).
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/bare_metal/mp_emplacement.rs`** — nuovo modulo con:
    - `pub const TRAMPOLINE_PHYS_BASE: u32 = 0x0000_8000` + `TRAMPOLINE_SIPI_VECTOR: u8 = 0x08` (SIPI vector byte derived).
    - `pub struct EmplacedTrampoline { trampoline_paddr, temp_pml4_paddr }` — ritorno per MB14.c.2.c.
    - `pub enum EmplacementError` — `OutOfFrames`, `Pml4Above4GiB`, `TrampolineVaConflict`, `MapFailed`. Tutti i path di errore restituiscono i frame allocati al `BitmapFrameAllocator` prima del return, quindi la funzione è safe-retry.
    - `pub fn place_trampoline<const N: usize>(allocator: &mut BitmapFrameAllocator<N>, mapper: &mut PageMapper, kernel_ap_entry: u64) -> Result<EmplacedTrampoline, EmplacementError>`.
  - **`crates/omni-kernel/src/bare_metal/mod.rs`** — `pub mod mp_emplacement;`.
  - **`crates/omni-kernel/src/lib.rs`** — nuovo hook in `kmain` post-MB14.c.2.b.1 che invoca `place_trampoline(FRAME_ALLOC, pager, 0xFFFF_FFFF_8010_0000)` quando `topo.enabled_count() > 1` e logga `[mb14.c.2.b.2] emplaced tramp_paddr=0x8000 temp_pml4=<addr>`. Su BSP-only (`enabled_count() == 1`) la sotto-step è skip esplicito con log `[mb14.c.2.b.2] BSP-only — emplacement skipped`.
- **+11 host-side test** in `bare_metal::mp_emplacement::tests::*`:
  - `place_trampoline_returns_canonical_trampoline_paddr` — pin del SIPI vector / phys base.
  - `place_trampoline_temp_pml4_is_in_low_4gib` — PML4 fits in 32-bit CR3 reloc.
  - `place_trampoline_consumes_three_frames_from_allocator` — frame accounting (3..=7 frame, includendo path active-CR3).
  - `place_trampoline_materialises_pml4_pointing_at_pdpt` / `_pd_with_2mib_identity_entry` — read-back via TestArena.
  - `place_trampoline_pml4_contents_match_pure_builder` — byte-equality vs `build_temp_identity_paging`.
  - `place_trampoline_handles_repeat_calls_consistently` — idempotenza identity-mapping (seconda call usa diversi frame temp).
  - `place_trampoline_returns_out_of_frames_when_allocator_empty` — error path con allocator drained.
  - `place_trampoline_writes_blob_to_phys_8000` — read-back 256 byte da arena, byte-equality vs `build_trampoline_blob`.
  - `place_trampoline_blob_starts_with_cli_cld_in_memory` — pinpoint dei primi due opcode reali in memoria.
  - `sipi_vector_matches_trampoline_base` — `TRAMPOLINE_SIPI_VECTOR == 0x08`.
- **Test arena infrastructure:** `TestArena` 1.5 MiB (384 frame) heap-allocated 4 KiB-aligned, `phys_offset = arena.ptr` (phys 0 ↔ arena.ptr), allocator anchored a `PhysAddr(0)` con `mark_range_free(PhysAddr(0x10_0000), …)` per matchare il reservation pattern di `kmain`. Copre **sia** phys `0x8000` (trampolino) **che** le frame `>= 0x10_0000` (allocator-handed). PML4 root carved out con `alloc_frame()` post-mark_range_free.
- **Workspace test count** 510+ → 521+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean.
- **Build Info panel** aggiornato a Active=`MB14.c.2.b.2 emplacement`, Next=`MB14.c.2.c live start_aps`, Track B=`MB1-MB13 OK, MB14.a-c.2.b.2 wip`, Phase 1 ≈ 88%, Tests=`515+ workspace pass`.
- **Scope esplicitamente NON incluso (deferito a MB14.c.2.c):**
  - Live LAPIC MMIO write — `start_aps` resta in `DryRun` mode.
  - Vero per-AP `kmain_ap` entry point (oggi placeholder higher-half).
  - Acknowledgement barrier via atomic counter.
  - Per-AP `PerCpu` slot allocation.
  - ADR-0007 (capture finale del design MP/AP); writing rimandato a chiusura ciclo MB14.c.

##### P6.MB14.c.2.c — Live INIT-SIPI-SIPI fire + AP landing stub + ack barrier

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort:** ~1.5 giornate (landing-stub design + PIT delay + LAPIC ICR write + ack barrier + ADR)
- **Dependencies:** MB14.c.2.b.2 ✅
- **Rationale:** chiude il ciclo MB14.c flippando il path dry-run a Live: il BSP scrive realmente `ICR_HI`/`ICR_LO` per ogni AP enabled, interposta i delay PIT-based (10 ms post-INIT + 200 µs fra SIPI×2) imposti da Intel MP-Spec § B.4, e busy-poll un atomic counter alimentato dagli AP dall'interno della landing-stub a phys `0x8100`. ADR-0007 cattura il design completo dell'intero ciclo MB14.c (sub-block .a → .c).
- **Deliverables consegnati:**
  - **`crates/omni-kernel/src/bare_metal/mp_ap_entry.rs`** — nuovo modulo. Costanti di layout (`AP_LANDING_STUB_OFFSET=0x100`, `AP_LANDING_STUB_SIZE=32`, `AP_ACK_COUNTER_OFFSET=0x140`, `AP_KERNEL_CR3_OFFSET=0x148`, `AP_KMAIN_AP_VA_OFFSET=0x150`). `build_ap_landing_stub(tramp_base_paddr) -> [u8; 32]` pure-function: `lock inc qword ptr [imm32]` + `mov rcx, [imm32]` + `mov rdx, [imm32]` + `mov cr3, rcx` + `jmp rdx`, byte-exact opcodes pinned ai test host-side. `kmain_ap` higher-half entry definito via `global_asm!` (toolchain 1.85 non stabilizza `#[naked]`): `cli; 1: hlt; jmp 1b`. +10 host test in `bare_metal::mp_ap_entry::tests::*`.
  - **`crates/omni-kernel/src/bare_metal/pit_delay.rs`** — nuovo modulo. `pit_count_for_us(us: u32) -> Option<u16>` pure-function (1.193 MHz fixed PIT base). `pit_delay_us(us: u32)` bare-metal: programma PIT channel 2 mode 0 (interrupt on terminal count) via port `0x43`/`0x42`, gata via `0x61` bit 0, polla bit 5 (`SPKR_OUT`) per terminal-count detection. Speaker data (bit 1) sempre mascherato → delay silenzioso. +7 host test pinpoint sui conteggi (1 µs / 10 ms / 200 µs / 54.9 ms / overflow).
  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** — `lapic_send_ipi(low, high) -> bool` pub: drena `ICR_LO` bit 12 (`Delivery Status`) prima del write, scrive `ICR_HI` (`0x310`) poi `ICR_LO` (`0x300`) per latch+fire. `lapic_icr_busy() -> bool` pub per polling esterno. Costanti `LAPIC_ICR_LO`/`LAPIC_ICR_HI`/`LAPIC_ICR_BUSY_MASK` aggiunte.
  - **`crates/omni-kernel/src/bare_metal/mp.rs`** — `start_aps_live(topology, bsp_apic_id, trampoline_page, phys_offset) -> StartApsLiveReport` (gated `target_arch = "x86_64"`; host stub returns `acked=0`). Loop per ogni enabled non-BSP CPU: INIT assert → busy-drain → `pit_delay_us(10_000)` → SIPI → busy-drain → `pit_delay_us(200)` → SIPI → busy-drain → `pit_delay_us(200)`. Poi busy-poll `read_ack_counter(phys_offset)` fino a `acked >= targeted` o esaurimento di `AP_ACK_POLL_ITERATIONS` (1 G iterazioni ≈ 1 s su silicio moderno).
  - **`crates/omni-kernel/src/bare_metal/mp_emplacement.rs`** — nuova `place_trampoline_live(allocator, mapper, kernel_cr3, kmain_ap_va) -> Result<EmplacedTrampoline, _>`: wraps `place_trampoline` passandogli `kernel_ap_entry = TRAMPOLINE_PHYS_BASE + AP_LANDING_STUB_OFFSET`, poi emplaza il landing stub via `write_landing_stub_bytes` + zera `AP_ACK_COUNTER` + scrive `AP_KERNEL_CR3` + scrive `AP_KMAIN_AP_VA`. `read_ack_counter(phys_offset) -> u64` esportato per il BSP polling loop.
  - **`crates/omni-kernel/src/lib.rs`** — hook MB14.c.2.b.2 sostituito da hook MB14.c.2.c che chiama `place_trampoline_live(..., cr3_raw & !0xFFF, kmain_ap as u64)` quando `topo.enabled_count() > 1`, poi `start_aps_live(&topo, lid, TRAMPOLINE_SIPI_VECTOR=0x08, phys_offset_mb2)`, e logga `[mb14.c.2.c] start_aps_live targeted=N sequenced=N acked=N (all APs online|timeout)`.
  - **`docs/adr/0007-mb14-mp-ap-startup.md`** — ADR `accepted`. Cattura design dell'intero ciclo MB14.c (.a + .b.1 + .b.2 + .c), alternative considerate (higher-half AP entry senza landing stub, TSC-calibrated delay vs PIT, SIPI vector `0x60` alternativo), e roadmap MB14.c.2.d (per-AP `PerCpu` + real per-CPU init).
- **Test:** +17 host-side test (10 in `mp_ap_entry` + 7 in `pit_delay`). Workspace test count 521+ → 538+. SIGSEGV pre-esistente su `cargo test -p omni-kernel --lib` resta carryover (item §4.5 #16 in progress-omni.md), nessuna regressione introdotta — gli integration test passano (boot_info 7 / heap 9 / mb11 6 / mb12 8 / mb13 11 / panic 5 = 46 verdi).
- **Build Info panel:** Active=`MB14.c.2.c live AP wake`, Next=`MB14.c.2.d per-AP PerCpu`, Track B=`MB1-MB13 OK, MB14.a-c.2.c wip`, Phase 1 ≈ 89%, Tests=`535+ workspace pass`.
- **Scope esplicitamente NON incluso (deferito a MB14.c.2.d):**
  - Real `kmain_ap` body — oggi è un `cli; hlt; jmp $-2` park loop. La AP rimane nello slot ma non può schedulare, prendere interrupt, o eseguire codice user.
  - Per-AP `PerCpu` slot allocation (l'array `AP_PER_CPU: [Option<PerCpu>; MAX_CPUS]` resta non scritto in MB14.c.2.c — la AP non legge un proprio slot perché si limita a `hlt`).
  - Per-AP kernel stack allocation (la AP arriva con `RSP` non valido e non esegue mai codice che ne richieda uno).
  - Per-AP `GDTR` / `IDTR` / `TSS` reload (la AP mantiene la temp GDT dal trampolino; benigno finché `kmain_ap` resta `cli; hlt`).

#### P6.MB14.d — IPI vettore + TLB shootdown protocol

- **Status:** `[ ]` (open)
- **Effort stimato:** 1-2 giornate
- **Dependencies:** MB14.c ✅
- **Rationale:** quando un thread modifica un mapping in un AS condiviso (es. driver-process che fa `MemMap`), bisogna broadcast `invlpg <addr>` su tutte le CPU che hanno quel CR3 attivo. Pattern Linux: scrivere il target VA in una struct per-CPU + raise IPI; handler IPI emette `invlpg` e acknowledged.
- **Deliverables previsti:**
  - IDT vector `0xFD` (TLB shootdown) + handler.
  - `ipi::send_to_all_except_self(vector)` via LAPIC ICR.
  - `mm::flush_tlb_range(va_start, len)` che decide se broadcastare in base ai bit di "AS active on CPU N".

#### P6.MB14.e — Per-CPU run-queue + scheduler split

- **Status:** `[x]` (chiuso 2026-05-19)
- **Effort stimato:** 1-2 giornate → consegnato in ~0.4 giornate (scope ristretto al scaffold + work-stealing API; il binding completo con `RoundRobinScheduler` per il dispatch AP-side è MB14.f follow-up)
- **Dependencies:** MB14.c ✅ + MB14.d ✅
- **Rationale:** `RoundRobinScheduler` è globale; con N CPU diventa il collo di bottiglia. Pattern: per-CPU run-queue ancorato a `PerCpu`, work-stealing fra siblings su idle. Mantiene la cooperative-yield semantics di MB6.
- **Deliverables consegnati:**
  - **MB14.e.1** — `kmain_ap` global_asm step 10 modificato da `cli; hlt; jmp $-2` a `sti; hlt; jmp $-2`. La `cli` iniziale (step 1 della stub di landing) resta perché copre tutto il pre-park init (lgdt/lidt/wrmsr/ltr). Dopo `sti`, gli AP servono ogni IPI unmasked — incluso il vector `0xFD` TLB shootdown — re-entrando in `hlt` dopo l'`iretq`. La label `90:` (park_unknown per LAPIC ID mismatch) mantiene il `cli; hlt; jmp $-2` originale: se un AP arriva con LAPIC ID fuori dalla `lapic_to_cpu` table, non vogliamo che serva IPI da uno stato non-init.
  - **MB14.e.2** — nuovo modulo `bare_metal::per_cpu_run_queue` con array statico `[CpuSlot; NUM_CPU_SLOTS = 1 + MAX_AP_SLOTS = MAX_CPUS]`. Ogni `CpuSlot` ha un `SpinLock` (AtomicBool CAS Acquire on success / Release on drop) + `UnsafeCell<PerCpuRunQueue>`. `PerCpuRunQueue` mantiene `[Vec<u64>; NUM_PRIORITY_CLASSES = 6]` mirror della cardinalità di `scheduling::PriorityClass`. API pubbliche: `enqueue_on_cpu(cpu_id, task_id, prio) -> bool`, `pop_for_cpu(cpu_id) -> Option<u64>`, `steal_from(victim_cpu) -> Option<u64>`, `local_len(cpu_id) -> usize`. Bound check via `RUN_QUEUES.get(cpu_id as usize)` — restituisce false/None se `cpu_id >= MAX_CPUS`. Static-size assert `NUM_CPU_SLOTS == MAX_CPUS` come compile-time invariant.
  - **MB14.e.3** — `pop_for_cpu_with_stealing(cpu_id)` chiama prima `pop_for_cpu(cpu_id)`, poi scansiona `0..NUM_CPU_SLOTS` skippando `cpu_id` e chiamando `steal_from`. `steal_from` ruba dalla **back of the lowest-priority non-empty queue** del victim: invertendo i due assi (back vs front, lowest vs highest priority) si minimizza la cache-line ping-pong sul head del victim's `pop_for_cpu` e si preserva il priority ordering globale.
  - **Boot-log smoke in `kmain`** — post-`sti`/post-`[mb14.d]`: enqueue sentinel `0xEEEEE2` su BSP cpu_id=0 con `PriorityClass::Interactive`, pop, verifica match → log `local=ok|FAIL`. Poi enqueue sentinel `0xEEEEE3` su cpu_id=0 con `PriorityClass::Background`, chiama `pop_for_cpu_with_stealing(1)` per esercitare il steal fallback → verifica match → log `steal=ok|FAIL`. Linea finale: `[mb14.e] per_cpu_run_queue local=ok steal=ok`.
  - **Aggiornamento log MB14.d** — il suffix del `[mb14.d] tlb_shootdown` ora distingue `(all APs acked)` da `(timeout — AP ISR did not ack)`. Con `sti` abilitato sull'AP, il caso atteso post-MB14.e su silicio multi-core è `(all APs acked)`.
  - **+12 host-side test** in `bare_metal::per_cpu_run_queue::tests::*`: round-trip enqueue/pop, FIFO within priority, priority ordering across, local_len tracking, out-of-range cpu_id rejection (enqueue + pop), 6 stealing protocol tests (back-of-lowest-priority, empty victim, sibling fallback, prefer-local, self-skip, all-empty-returns-none). I test sono serializzati con un `SpinLock` interno (no `std::sync::Mutex` per evitare il workspace `disallowed_methods` clippy lint sulla poison semantics).
  - **Workspace test count** 564+ → 576+; `cargo clippy --workspace --all-features --all-targets -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` + `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` + `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` clean; `cargo fmt --all -- --check` clean.
  - Build Info panel: Active=`MB14.e per-CPU run-queue`, Next=`MB14.f x2APIC LAPIC>255`, Track B=`MB1-MB13 OK, MB14.a-e wip`, Phase 1 ≈ 93%, Tests=`586+ workspace pass`.
- **Scope esplicitamente NON incluso (deferito a MB14.f / futuro):**
  - **`RoundRobinScheduler` binding al per-CPU run-queue.** Lo scheduler attuale mantiene le proprie `run_queues` globali (BSP-driven) e il nuovo modulo è scaffold pronto per essere consumato quando l'AP avrà un proprio context-switch loop. Il pattern Phase 2 prevede uno `enqueue_for_cpu` sul scheduler che inoltra al modulo `per_cpu_run_queue` con cpu_id derivato dalla policy di placement (es. round-robin, NUMA-locality, AI-aware).
  - **AP dispatch loop reale.** Gli AP parkano in `sti; hlt; jmp $-2` — servono solo IPI passive (es. TLB shootdown). Un dispatch attivo richiede preemption-driven scheduling fired da LAPIC timer **su ogni AP**, oltre a setup per-CPU del `TICK_COUNT` + `NEED_RESCHED` flags. MB14.f cura entrambi i pezzi.
  - **x2APIC support.** LAPIC IDs > 255 (sparse topology di server-class silicon) richiedono switch al MSR-based ICR encoding già coperto da `mp::encode_icr_x2apic` ma non ancora wired nel `kmain_ap` ID-read step (CPUID leaf 1 EBX[31:24] è 8-bit). MB14.f promove a CPUID leaf 0xB sub-leaf 0 EDX.
  - **ADR-0008 (per-CPU scheduling protocol).** Writing differito a MB14.f closure quando il binding scheduler ↔ per_cpu_run_queue va live.

- **Smoke validation Proxmox VMID 103 (2026-05-19):** la build default desktop demo boot reaches `[virtio] tablet ready`, framebuffer renderizzato (≈67% non-zero pixel = desktop demo full render). Il Build Info panel mostra il nuovo stato (Active=`MB14.e per-CPU run-queue`, Next=`MB14.f x2APIC LAPIC>255`, Phase 1 ≈ 93%, Tests=586+). Linee serial chiave:
  - `[mb14.c.2.c] start_aps_live targeted=7 sequenced=7 acked=7 (all APs online)` — 7 AP svegliate
  - `[mb14.c.2.d] per-AP init online=7/7 (all APs parked)` — tutte completano init+park
  - `[mb14.e] per_cpu_run_queue local=ok steal=ok` — **scaffold validato end-to-end** (enqueue/pop + steal-fallback su BSP+AP cpu_id=1)
  - `[mb14.d] tlb_shootdown vector=0xFD targeted=7 acked=0 local_pages=1 (timeout — AP ISR did not ack)` — **issue residuo: gli AP non incrementano l'ack counter**, vedi MB14.e.4 follow-up sotto.

- [x] **MB14.e.4 follow-up — TLB shootdown ack timeout su AP** (closed 2026-05-20 da MB14.f.1). **Root cause confermata** (ipotesi (a) in elenco originale): gli AP non chiamavano mai `lapic_init` quindi il loro LAPIC SIVR aveva bit 8 (`LAPIC_ENABLE`) cleared — Intel SDM Vol 3A § 10.4.3 specifica che ogni Fixed-delivery IPI viene silenziosamente scartata dall'hardware in quello stato. **Fix MB14.f.1:** nuovo entry-point Rust `bare_metal::lapic::kernel_ap_lapic_init` `extern "C" no_mangle` chiamato dal `kmain_ap` global_asm step 8 (subito dopo `ltr`, prima di `lock inc AP_ONLINE_ACK`) — programma SIVR (`LAPIC_ENABLE | 0xFF`) + TPR=0 + LVT timer + initial count + divider su ogni AP. Mode-aware: in xAPIC mode writes via MMIO, in x2APIC mode flip prima `IA32_APIC_BASE` bit 10+11 sul MSR per-CPU di questa CPU (la flip del BSP non propaga) poi MSR-based SIVR/TPR/timer. Post-fix il boot log mostra `[mb14.d] tlb_shootdown vector=0xFD targeted=N acked=N (all APs acked)`. ADR-0008 documenta il design.

---

## P6 — Kernel follow-up minori (non bloccanti per MB13, da pianificare in MB14+)

Tracciati per non essere persi. Tutti carryover da `progress-omni.md` § 4.1.

- [~] **MB13.b follow-up — `mb12-userprobe` user-side serial output missing** (scoperto 2026-05-19; parzialmente chiuso 2026-05-19 da MB13.f + process.rs reorder). **Causa 1 (chiusa):** `enter_user_mode` eseguiva `mov cr3` con RSP sullo user stack del task uscente → fault Ring 0 → triple-fault. Fix MB13.f: nuovo parametro `kernel_stack_top` + `mov rsp` PRIMA del `mov cr3`. **Causa 2 (chiusa):** `new_with_kernel_half` clona PML4 entries 256..511 *by value*; un nuovo PDPT installato nel boot PML4 *dopo* il clone (es. la prima kstk MB10 al PML4 index 0x180) NON propaga alle PML4 cloned precedentemente. Fix: `process.rs::spawn_from_elf` ora alloca+mappa la kstk *prima* di clonare il PML4. **Stato residuo (open follow-up MB13.g):** entrambi i fix consegnati, e `enter_user_mode` esegue `iretq` sano (verificato via tracepoint inline su COM1), ma la VM si arresta subito dopo senza emettere alcuno dei tracepoint installati su `omni_syscall_entry`/timer/ISR#PF/#GP/#DF. Il task Ring 3 non esegue nemmeno una `jmp $` (testato). Ipotesi: fault ad iretq di vettore non gestito (#SS, #NP, #TS, #UD) che produce un #DF non raggiungibile. Diagnostica più profonda richiede `-d int,cpu_reset -D <log>` sui flag QEMU di VMID 103.

- [ ] **MB13.g — `mb12-userprobe` Ring 3 post-iretq triple-fault** (open, 2026-05-19). Dopo che MB13.f + il reorder process.rs hanno chiuso il triple-fault del primo `push` post-`mov cr3`, il blocco asm `enter_user_mode` esegue interamente fino a `iretq` (verificato via tracepoints inline su COM1 port 0x3F8 — emit `A` enter / `B` after `mov rsp` / `C` after `mov cr3` / `D` after first push / `E` before `iretq`). Subito dopo l'`iretq` la VM si arresta senza emettere: nessun `'S'` da SYSCALL entry, nessun `'T'` da LAPIC timer, nessun `'P'/'G'/'F'` dagli ISR `#PF/#GP/#DF`. Il task Ring 3 non esegue nemmeno una `jmp $` (testato sostituendo il primo byte del receiver ELF con `0xEB 0xFE`). Ipotesi: (a) fault ad iretq di vettore non gestito (#SS, #NP, #TS, #UD), che cascata a #DF il cui handler ESISTE ma non viene raggiunto per qualche motivo legato a TSS/segment state; (b) un descriptor in GDT diventa unreachable dopo lo swap CR3; (c) la pagina IDT stessa diventa non mappata. Diagnostica successiva richiede `-d int,cpu_reset -D /tmp/qemu-trace.log` sui flag QEMU di VMID 103 (modifica `/etc/pve/qemu-server/103.conf`).

- [ ] **TLB shootdown multi-core** — nessun MP/AP enable; LAPIC pronta ma il sistema gira single-core. Necessario prima di P6.7 (driver model). MB11 ha previsto questo: il kernel-half "by reference" di `AddressSpace` diventerà un costo cross-AS broadcast con MP. ADR-0004 § Alternative B documenta la mitigazione futura.
- [ ] **`map_4k` huge-page split** — `map_4k` oggi non splitta una 2 MiB/1 GiB PS=1 entry. Non bloccante finché il kernel non riscrive VA in range huge-page mappati dal bootloader, ma rischia di mordere quando il driver model entra in scena.
- [ ] **`omni-userprobe-helloworld` come crate separato** — MB11.7 ha embedded i 167 byte hand-crafted; un crate Rust `no_std` con linker script + `build.rs` ricorsivo produrrebbe lo stesso ELF in modo manutenibile.
- [ ] **CI smoke automatico per `mb11-userprobe` e `mb12-userprobe`** — il job `qemu-boot-smoke` valida MB1-MB10. Servono due nuovi job (o un flag su `scripts/qemu-boot-smoke.sh`) con `EXPECTED_LINES` esteso per le linee `[user] hello` / `[mb12] channel 1 pre-created` / `ping`. Sblocca-bile **dopo** MB13.b (oggi fallirebbero per il triple-fault).
- [ ] **BumpHeap no-free per canali IPC distrutti** — ADR-0005 § Negative: cap raccomandato `queue_depth ≤ 256` per canale; slab/free-list allocator → OIP separato (Phase 2).
- [ ] **Hygiene CHANGELOG MB8** — la riga "Known blocker (MB9)" del 2026-05-17 è ora storica; annotare "resolved by MB9".

---

## P6.7 — Userspace driver model (NVMe, Ethernet/Wi-Fi, TEE)

- **Status:** `[~]` (sbloccato da MB12 + MB13 + MB14 ✅; OIP-Driver-Framework-013 `Draft` filed 2026-05-20)
- **Priority:** P6 / Critical (Phase 1 deliverable)
- **Effort:** 6-12 engineer-months estimated (post-OIP-013 ratification)
- **Dependencies:** MB13 (capability reale) ✅ + MB14 (MP/AP enable) ✅ + framework OIP [`OIP-Driver-Framework-013`](oips/oip-driver-framework-013.md) ✅ (`Draft`, da promuovere a `Review` → `Last Call` → `Active`) + per-driver OIPs (TBD: `OIP-Driver-NVMe-XXX`, `OIP-Driver-Net-XXX`, `OIP-Driver-TEE-XXX` — ognuno richiede `OIP-Driver-Framework-013` come `requires:`).
- **Rationale:** roadmap Phase 1 list explicit: "Drivers (in user space): NVMe storage, Ethernet/Wi-Fi networking, TEE". Microkernel principle — tutto fuori dal TCB. Sblocca anche P5.2/P5.3 (TEE backends reali).

### Sub-tasks (post-OIP-013 ratification)

- [x] **P6.7.0 — `OIP-Driver-Framework-013` drafting** (closed 2026-05-20, commit `bb4b9a1`) — `oips/oip-driver-framework-013.md`. Lock kernel-side contract per 5 superfici: capability scope extensions (`Action::{MmioMap, DmaMap, IrqAttach, PciConfigRead, PciConfigWrite, DriverLoad, DriverUnload}` + `Resource::{PciDevice, MmioRegion, DmaWindow, IrqLine}`), MMIO syscall `SyscallNo::MmioMap = 22`, DMA/IOMMU domain-per-driver con backend `vtd.rs`+`amdvi.rs`, IRQ routing BSP-only con shared-line rejection (`EBUSY`), driver manifest TOML + Ed25519 signing + static `KNOWN_ISSUERS`.
- [x] **P6.7.1 — Promote `OIP-Driver-Framework-013` Draft → Review** (closed 2026-05-20, editorial fast-path) — `oip-lint` green, RFC 2119 wording validated, § R6 "not doing" list explicit. Transition under `OIP-Process-001` § 4.
- [x] **P6.7.2 — Promote `OIP-Driver-Framework-013` Review → Last Call** (closed 2026-05-20, founder approval) — 14-day public-objection window per `OIP-Process-001` § 5.3 chiude **2026-06-03**; auto-transition a `Active` se nessuna blocking objection. Bootstrap fiat NON applicabile (§6.3 esplicito: "Does NOT apply to any future Standards Track OIP").
- [x] **P6.7.2.bis — Promote `OIP-Driver-Framework-013` Last Call → Active (founder fast-path 2026-05-20)** — deroga editoriale al § 5.3 14-day window, approvazione esplicita del founder; OIP-013 stato `Active`, `activated: 2026-05-20`. Aggiunta **Appendix A — Editorial Reconciliations** che documenta la collisione ABI sui numeri syscall 22–25 (occupati da MB12 `IpcSend`/`IpcReceive` v0.1) e la rinumerazione canonica al decade `7x`: `MmioMap = 70`, `DmaMap = 71`, `IrqAttach = 72`, `DriverLoad = 73`; OIP-016 a cascata: `TeeTdcall = 74`, `TeeMsr = 75`. Nessuna modifica normativa al comportamento; convenzione di grouping estesa ai numeri `70..=79` per il driver framework + reserved `80+`.
- [x] **P6.7.3 — Framework impl skeleton** (no driver code yet) — closed 2026-05-20 con il commit post-fast-path. Deliverables landed:
    - `crates/omni-capability/src/scope.rs` — nuove varianti `Action::{MmioMap, DmaMap, IrqAttach, PciConfigRead, PciConfigWrite, DriverLoad, DriverUnload, TeeProbe}` + `Resource::{PciDevice, MmioRegion, DmaWindow, IrqLine}` con subset semantics byte-exact su `PciDevice`/`IrqLine` e range-contained su `MmioRegion`/`DmaWindow` (u128 widening anti-wrap). `#[non_exhaustive]` preservato. +11 host-side test (30 totali nel modulo `scope`).
    - `crates/omni-kernel/src/syscall.rs` + `crates/omni-kernel/src/bare_metal/syscall_entry.rs` — nuove varianti `SyscallNumber::{MmioMap = 70, DmaMap = 71, IrqAttach = 72, DriverLoad = 73, TeeTdcall = 74, TeeMsr = 75}` con pin numerico nel test `syscall_numbers_are_stable`; dispatcher route ai numeri `70..=75` con handler stub `Err(KernelError::NotYetImplemented)` (= ENOSYS); rapping C-ABI flatten a `SYSCALL_ERROR`. +3 host-side test (`dispatcher_driver_framework_syscalls_return_not_yet_implemented`, `kernel_syscall_dispatch_driver_framework_numbers_route_to_sentinel`, `kernel_syscall_dispatch_unknown_driver_decade_number_returns_sentinel`).
    - `crates/omni-kernel/src/driver_manifest.rs` (nuovo modulo) — schema types (`DriverManifest`, `DriverMeta`, `DriverCapabilities`, `DriverMatchers`, `PciMatcher`), error enum (`ParserNotWired` per il TOML, `Malformed`, `UnknownIssuer`, `SignatureInvalid`, `ImageHashMismatch`), `parse_manifest` entry-point che torna `ParserNotWired` (TOML parser deferito a P6.7.8), `verify_manifest` che esegue BLAKE3(image) vs `omni_image_hash` + resolve issuer via `lookup_issuer` + Ed25519 verify della signing payload byte-deterministic, helper `is_driver_framework_action`, `caps_for_single_mmio`, `ISSUER_KEY_LEN`. +7 host-side test.
    - `crates/omni-kernel/src/known_issuers.rs` (nuovo modulo) — `KnownIssuer { id, verifying_key: [u8; 32] }` + `static KNOWN_ISSUERS: &[KnownIssuer]` (vuoto in Phase 1, popolato in P6.7.8 con il primo driver image firmato) + `lookup_issuer(id) -> Option<&'static KnownIssuer>`. +2 host-side test (`phase1_table_is_empty` come forcing function per il primo provisioning, `lookup_unknown_issuer_returns_none`).
    - `crates/omni-kernel/Cargo.toml` — nuova direct-dep `omni-crypto = { default-features = false, features = ["bare-metal"] }` per BLAKE3 + Ed25519 (lo stesso pattern di `omni-capability`).
    - `scripts/lint-oips.py` — `OPTIONAL_FRONTMATTER_KEYS` esteso con `activated` per supportare il timestamp di attivazione degli OIP che raggiungono `Active`.
    - **Build Info panel** (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): `Active = P6.7.3 framework skeleton`, `Next = P6.7.7 promote 014/015/016`, `Phase = 1 - Microkernel POC (~98%)`, `Tests = 665 workspace pass`.
    - Acceptance: `cargo test --workspace --all-features -- --test-threads=1` → **665 pass / 0 fail**. `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps` + `python3 scripts/lint-oips.py` tutti clean (0 errori, 0 warning).
- [x] **P6.7.3.bis — Realign skeleton to OIP-013 Appendix B amendments (2026-05-20)** — il founder ha aggiunto un'**Appendix B — Bootstrap fast-path amendments** all'OIP-013 dopo il commit di P6.7.3 (`ab986c2`), con 4 amendment normativi (1) `DriverLoad` accetta omni-pack v1 binary container invece di TOML al boundary di autenticazione + nuovo § S5.5; (2) MMIO VA range 2 GiB → 512 GiB PML4 slot con KASLR; (3) IRQ notification shape locked `IrqNotification::{Tick, MissedSince(u32)}` in-band; (4) nuovo § R7 con 3 follow-up OIPs anticipati. Realignment minimale del skeleton: (a) `driver_manifest.rs` nuove API `decode_omni_pack(pack_bytes) -> Result<OmniPackSections, _>` + `postcard_decode_manifest(manifest_bytes)` stub; nuovi tipi `OmniPackSections` (borrowed view), `OmniPackHeader` constants (`OMNI_PACK_MAGIC = b"OMNIPACK"`, `OMNI_PACK_VERSION = 1`, `OMNI_PACK_HEADER_LEN = 0x40`, `OMNI_PACK_MAX_BYTES = 32 MiB`, `OMNI_PACK_MAX_MANIFEST_BYTES = 16 KiB`); nuovi error variants `MalformedPack`, `PackTooLarge` (POSIX-aligned `EINVAL`); `DriverMeta.omni_issuer: String` → `omni_issuer_pubkey: [u8; VERIFYING_KEY_LEN]` (chiave Ed25519 32-byte come da § S5.1 schema TOML); `verify_manifest` ora cross-check direttamente la pubkey vs `KNOWN_ISSUERS` (§ S5.4 no-TOFU); module docs riscritti per riflettere omni-pack v1 + TOML developer-side. (b) `known_issuers.rs` `lookup_issuer(id: &str)` → `lookup_issuer(pubkey: &[u8; VERIFYING_KEY_LEN])` con campo `id` mantenuto come logging metadata. (c) Build Info `Tests = 674 workspace pass` (era 665, +9 nuovi test omni-pack header decode). MMIO VA range / KASLR / IrqNotification shape sono *implementation details* dei syscall handler ancora `NotYetImplemented` — nessun impatto sul skeleton oggi, tracciati per P6.7.8. Acceptance: workspace 665 → **674 pass / 0 fail**, tutti i 4 surfaces clippy `-D warnings` + fmt + check-no-blanket-allow + cargo doc + lint-oips clean.
- [x] **P6.7.4 — `OIP-Driver-NVMe-014` drafting** (closed 2026-05-20) — `oips/oip-driver-nvme-014.md` (Standards Track, `Draft`, requires [13]). Lock NVMe-specific manifest `[nvme]` block (admin/IO SQ/CQ depths, `io_queue_count` ≤ 4 BSP-cap, `transfer_model = "prp"`), command channel ABI (`omni.driver.nvme.cmd`: `Identify`/`Read`/`Write`/`Flush`/`Discard`/`GetLogPage`/`FormatNVM`), event channel (`omni.driver.nvme.evt`: `CommandComplete`/`AsyncEvent`/`LinkStateChange`/`ControllerFatal`), BLK service channel (`omni.svc.blk.<diskN>`: `BlkRequest`/`BlkResponse` shape generica per future SATA/virtio-blk), 12-step bring-up sequence dal PCI enumeration → CC.EN=1 → Identify Controller → Identify Namespace → Create IO QP → BLK channel registration. Single-namespace v0.3; multi-namespace + NVMe-MI + ZNS deferred. PRP-only data transfer model (SGL deferred). TC1-TC7 inclusi.
- [x] **P6.7.5 — `OIP-Driver-Net-015` drafting** (closed 2026-05-20) — `oips/oip-driver-net-015.md` (Standards Track, `Draft`, requires [13]). Phased delivery M1=virtio-net (QEMU/Proxmox), M2=Intel e1000e (bare-metal consumer), M3=Mellanox ConnectX (server-class, deferred). Manifest `[net]` block (mtu 1500 default / 9000 opt-in, rx/tx_ring_depth, rx_buffer_count, checksum offload, TSO/LRO opt-in). NET service channel (`omni.svc.net.<ifN>`: `NetRequest::SendFrame`/`GetLinkState`/`GetMac`/`SetPromisc`), event channel (`NetEvent::FrameReceived`/`LinkStateChange`/`MacChanged`). L2 Ethernet frames (no FCS); single-queue per driver per v0.3 (multi-queue + RSS deferred). Per-family bring-up sequences (virtio 1.0 feature negotiation + virtqueue setup; e1000e CTRL.RST + RDBAL/TDBAL + MSI-X). Wi-Fi out of scope. TC1-TC6 inclusi.
- [x] **P6.7.6 — `OIP-Driver-TEE-016` drafting** (closed 2026-05-20) — `oips/oip-driver-tee-016.md` (Standards Track, `Draft`, requires [13]). Promuove `StubAttestation` (MB13.c) a real TEE driver con backend Intel TDX (TDREPORT → DCAP Quote) + AMD SEV-SNP (SNP_GET_REPORT). Backend selection at boot via CPUID probing; `Backend::Stub` retained `#[cfg(tee-stub)]` per dev/test. Attestation service channel (`omni.svc.tee.attest`: `Quote`/`Seal`/`Unseal`/`GetMeasurement`/`GetInfo`/`Verify`). Static kernel inventory entry (TEE driver is non-PCI, spawned at boot post-PCI-enum). Nuova `Action::TeeProbe` cap-extension a OIP-013 § S1 (`#[non_exhaustive]` safe). Kernel-mediated TDCALL syscall (`SyscallNo::TeeTdcall = 26`) + SEV-SNP MSR access syscall (`SyscallNo::TeeMsr = 27`). Verify path integration: `Ed25519CapabilityProvider::verify_signed_token` (MB13.c) chiama `IpcSend(omni.svc.tee.attest, AttestRequest::Verify{quote, expected_measurement})` invece di `StubAttestation`. `KNOWN_MEASUREMENT_OMNI_OS` baked at compile time. Fallback transitorio a `StubAttestation` quando hardware-TEE absente; rimozione del fallback in cleanup OIP follow-up post-hardware-validation (P5.2/P5.3 funding-dep). TC1-TC6 inclusi.
- [x] **P6.7.7 — Promote OIP-014/015/016 Draft → Active (founder fast-path 2026-05-20)** — single-pass editorial transition `Draft → Review → Last Call → Active` per ciascuno dei tre OIP follow-up del driver framework, sotto `OIP-Process-001 §5.5` (Solo Founder Fast-Track) esercitato dalla Bootstrap Period authority `§6.3`. Standard 14-day public objection window di `§5.3` waived con approvazione esplicita del founder; `activated: 2026-05-20` aggiunto al frontmatter di tutti e tre i file. Aggiunta `## Appendix A — Bootstrap Activation Note` a ciascun OIP con rationale (dependency unblock OIP-013, zero deployment risk al filing time, scope bounded da OIP-013 Active, no conflict con OIP-013 Appendix B amendments) e re-ratification obligation per `§5.5.e`. Index `oips/README.md` aggiornato (3 righe). Nessun cambio al codice runtime (doc-only); Build Info panel aggiornato Active=`P6.7.7 OIP 014/015/016 Active`, Next=`P6.7.8 virtio-net (M1) driver`. P6.7.8 (first-party driver impl) ora completamente sbloccato.
- [~] **P6.7.8 — First-party driver implementations** — sequenzialmente: virtio-net (M1, primo perché QEMU/Proxmox-validabile senza hardware fisico) → NVMe → e1000e (M2 bare-metal) → TEE (richiede TDX/SEV-SNP hardware, funding-dep). Per ogni driver: branch `feat/kernel-p6-7-driver-<family>` + signed image + integration tests + Proxmox VMID 103 smoke. Aperto 2026-05-20 con sub-task atomici sotto.
  - [x] **P6.7.8.0 — Wire postcard decoder per `DriverManifestV1`** (closed 2026-05-20) — `crates/omni-kernel/src/driver_manifest.rs`: aggiunto `DriverManifestBody` + `DriverMetaBody` (wire-type postcard-serializable, omette la firma per evitare la circolarità sign-over-self), `DriverManifest::body() -> DriverManifestBody`, `hydrate_manifest(body, signature) -> DriverManifest`; `postcard_decode_manifest` ora wired via `omni_types::wire::decode_canonical::<DriverManifestBody>` (no-trailing-bytes invariant enforced per `OIP-Serde-004`); `verify_manifest` ora calcola il signing payload via `encode_canonical(&manifest.body())` invece dell'encoder handcrafted; rimossa la variant `DriverManifestError::ParserNotWired` (non più necessaria) e i 5 helper `build_signing_payload`/`push_*`/`encode_resource` + i 4 TAG_* constant (sostituiti dal wire format postcard canonico di `OIP-Serde-004`). `DriverCapabilities`, `DriverMatchers`, `PciMatcher` derivano ora `Serialize, Deserialize` (Resource/Action già le derivavano da P6.7.3); `DriverManifest`/`DriverMeta` deliberatamente NON derivano serde (la firma da 64 byte non ha impl serde built-in e il wire type è `DriverManifestBody`). +5 host-side test (`postcard_round_trip_manifest_body_preserves_fields`, `postcard_decode_manifest_rejects_trailing_bytes`, `postcard_decode_manifest_rejects_truncated_input`, `postcard_decode_manifest_rejects_empty_input`, `body_round_trip_through_omni_pack_envelope`); rimossi i 2 test handcrafted (`postcard_decode_manifest_returns_parser_not_wired`, `signing_payload_encodes_capability_subset` riscritto come `signing_payload_grows_with_capabilities` senza dipendere dai TAG byte). Workspace test count 674 → **679 pass / 0 fail**. Build Info panel aggiornato: Active=`P6.7.8.0 postcard manifest dec`, Next=`P6.7.8.1 MmioMap syscall handler`, Phase=`1 - Microkernel POC (~98.5%)`, Tests=`679 workspace pass`.
  - [x] **P6.7.8.1 — `MmioMap` syscall handler** (closed 2026-05-20) — sostituisce lo stub `NotYetImplemented` con il path completo OIP-013 § S2: (a) two-register ABI (`rax` = `va_base`, `rdx` = POSIX errno) via nuovo `SyscallReturn` `#[repr(C)]` + `SyscallDispatcher::dispatch_full` default-impl trait extension + `kernel_syscall_dispatch` C-ABI signature change a `extern "C" -> SyscallReturn` (SysV AMD64: 2× u64 struct returnata in `(rax, rdx)`, asm stubs preservano `rdx` invariato fino a `sysretq` / `iretq`); (b) `crate::syscall::syscall_errno::{EACCES=13, EFAULT=14, EINVAL=22, ENOSPC=28, ENOSYS=38}` POSIX-aligned constants; (c) nuovo modulo `kaslr.rs` con `KaslrRng` (SplitMix64) + `seed_from_hw` (`RDRAND` cpuid-gated 10 retries → `RDTSC + monotonic counter` fallback per VM/hypervisor senza RDRAND); (d) nuovo `bare_metal::PHYS_OFFSET` AtomicU64 + `set_phys_offset`/`phys_offset` accessor, wirato in `kmain` early-init prima di ogni syscall path; (e) `ProcessControlBlock` esteso con `mmio_mappings: Vec<MmioMapping>` + `mmio_va_cursor: u64`; (f) `Scheduler::process_mut` accessor; (g) handler `mmio_map_handlers::mmio_map(args)` (bare-metal-only, host build returns `EACCES` stub) che esegue: decode postcard `CapabilityToken` (cap_len ≤ 1024) → `Ed25519CapabilityProvider::verify_signed_token` (signature + time window + TEE binding) → `is_driver_framework_action` + `Action::MmioMap` + `Resource::MmioRegion` subset-contains check → lazy-randomize per-process cursor nel driver-MMIO PML4 slot `[0x0000_0080_0000_0000, 0x0000_0100_0000_0000)` (512 GiB) → linear bump → install leaf PTEs via `AddressSpace::map_user_4k` con flags `PRESENT|WRITABLE|USER|NX|PCD|PWT` (uncached default; WC con `flags & 1 == 1` rejected con `ENOSYS` pending PAT init) → `invlpg` per ogni page → record su `pcb.mmio_mappings` → return `va_base` in `rax`; (h) rollback page-by-page con `unmap_4k` su qualsiasi `map_user_4k` failure; (i) `tear_down_mmio_mappings(task)` wired in `task_exit` prima di `sched.dequeue(current)` per OIP-013 § S2.4 (unmap leaf PTEs + invlpg; nessun frame ritornato all'allocator perché MMIO è device-owned). +13 nuovi host-side test (5 KaslrRng deterministic + hardware seed monotonicity, 4 SyscallReturn shape + errno codes layout SysV-AMD64, 2 PCB mmio_mappings round-trip, 2 dispatcher CapabilityDenied + dispatch_full EACCES). Workspace test count 679 → **692 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps --target x86_64-unknown-none` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` tutti clean. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.1 MmioMap syscall live` (cyan), Next=`P6.7.8.2 virtio-net crate scaffold`, Phase=`1 - Microkernel POC (~98.8%)`, Tests=`692 workspace pass`. P6.7.8.2 (virtio-net scaffold) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.2 — virtio-net crate scaffold** (closed 2026-05-20) — nuovo `crates/omni-driver-net-virtio` aggiunto come workspace member full (`no_std + alloc` lib, `#![cfg_attr(not(test), no_std)]`, `extern crate alloc`). Skeleton-only, zero syscall integration. Cinque moduli `pub`: (a) `pci_ids` — `VIRTIO_PCI_VENDOR_ID = 0x1AF4` + `VIRTIO_NET_PCI_DEVICE_ID_MODERN = 0x1041` + `…_LEGACY = 0x1000`, pinned a virtio 1.0 § 4.1.2.1 e a OIP-015 § S4; (b) `device_status` — constants `RESET / ACKNOWLEDGE / DRIVER / DRIVER_OK / FEATURES_OK / DEVICE_NEEDS_RESET / FAILED` da virtio 1.0 § 2.1 + bit-disjointness invariant + `0x0F` live-status anchor; (c) `features` — `VIRTIO_F_VERSION_1` (bit 32) + `VIRTIO_NET_F_MAC` (bit 5) + `VIRTIO_NET_F_STATUS` (bit 16) come `REQUIRED_FEATURES` mandatory floor OIP-015 § S4.1 step 4, più `VIRTIO_NET_F_CSUM` (bit 0) e `VIRTIO_NET_F_MRG_RXBUF` (bit 15) opzionali; (d) `virtqueue` — `RX_QUEUE_IDX=0`/`TX_QUEUE_IDX=1` + default depth 256 + `VIRTQ_DESC_BYTES=16` + `VIRTQ_AVAIL_FIXED_BYTES=6` + `VIRTQ_USED_FIXED_BYTES=6` + `VIRTQ_USED_ELEM_BYTES=8` + helper pure-function `is_valid_queue_depth/descriptor_table_bytes/avail_ring_bytes/used_ring_bytes` con `checked_mul`/`checked_add` per il defence-in-depth overflow check; (e) `bringup` — `Phase` enum `#[repr(u8)]` con sette stati spec-anchored (`Reset → Acknowledge → FeatureNegotiation → FeaturesLocked → VirtqueueSetup → MacAcquired → DriverOk`) + terminal `Failed`, pure-function `Phase::next/is_terminal/is_live`, transition tables + syscall invocations deliberatamente deferred a P6.7.8.3. Nuovo `crates/omni-driver-net-virtio/manifest.toml` developer-authored TOML v1 template (consumato offline da `omni-driver-pack` per generare l'omni-pack v1 blob loadable da `DriverLoad`) con shape OIP-013 § S5.1 + OIP-015 § S1 (`[meta]`/`[capabilities]`/`[matchers]`/`[net]`, placeholder per `omni_image_hash`/`omni_signature`/`omni_issuer_pubkey` riempiti dal Forge tool). `scripts/check-no-blanket-allow.sh` esteso con `crates/omni-driver-net-virtio` (13 crate-root files scanned). +24 nuovi host-side test (4 pci_ids vendor/modern/legacy/distinct, 3 device_status reset/disjoint/live-status, 4 features version_1/required/no-overlap-csum/no-overlap-mrg, 8 virtqueue indices/defaults/overflow/zero/power-of-2/sizes, 5 bringup monotonic/terminal/live/discriminants/phase-progression). Workspace test count 692 → **716 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` tutti clean. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.2 virtio-net scaffold` (cyan), Next=`P6.7.8.3 virtio-net bring-up`, Phase=`1 - Microkernel POC (~99.0%)`, Tests=`716 workspace pass`. P6.7.8.3 (bring-up state-machine wiring + bootable image sibling `crates/omni-driver-net-virtio-image/` excluded come kernel-runner) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.3 — virtio-net bring-up + bootable image sibling** (closed 2026-05-20) — three superfici landed in single-shot: (a) **`DmaMap (71)` syscall handler** wired in `crates/omni-kernel/src/bare_metal/syscall_entry.rs::dma_map_handlers` per OIP-013 § S3 con due-register ABI: decode postcard `CapabilityToken` (cap_len ≤ 1024) → `Ed25519CapabilityProvider::verify_signed_token` → `is_driver_framework_action` + `Action::DmaMap` + `Resource::DmaWindow` subset-contains → driver-DMA PML4 slot `[0x0000_0100_0000_0000, 0x0000_0180_0000_0000)` (512 GiB, disjoint dal MMIO slot) → enforced contiguous frame allocation da `FRAME_ALLOC` con strict-contiguity check (Phase 1 no-IOMMU passthrough: `iova == user_va` mapped 1:1; vendor backends `vtd`/`amdvi` differiti) → install leaf PTEs `PRESENT|WRITABLE|USER|NX` → `invlpg` per page → record `DmaMapping { iova_base, len_pages, direction }` sul PCB → return `SyscallReturn::ok(phys_base)`; rollback page-by-page con `unmap_4k` + `free_frame` su qualunque failure; `tear_down_dma_mappings(task_id)` invocato in `task_exit` PRIMA di `sched.dequeue` (frame ritornano all'allocator). (b) **`IrqAttach (72)` syscall handler** wired in `irq_attach_handlers::irq_attach` per OIP-013 § S4: decode postcard `CapabilityToken` → `verify_signed_token` → `Action::IrqAttach` + `Resource::IrqLine` subset-contains → shared-line rejection (linear scan di 191 slot, ritorna EINVAL — proxy per EBUSY in attesa di EBUSY costante POSIX in `syscall_errno`) → `allocate_vector` su `[IRQ_VECTOR_BASE=0x40, IRQ_VECTOR_END=0xFE]` bitmap CAS-reservato per `(irq_line, owner_task, channel_id)` → install IDT trampoline (`idt_set_vector(vector, omni_irq_dispatch_trampoline)`) → record `IrqAttachment { irq_line, vector, channel_id }` sul PCB → return `SyscallReturn::ok(vector)`. Nuovo asm `omni_irq_dispatch_trampoline` (9-push caller-saved + `call kernel_irq_dispatch_handler` + 9-pop + iretq, stack-aligned 16-byte). Rust-side `kernel_irq_dispatch_handler` chiama `lapic::read_in_service_vector` (nuovo helper che scansiona `ISR.B0..B7` MSR/MMIO highest-bank first) + `irq_attach_handlers::dispatch_fire(vector)` (incrementa `missed` counter atomico + `lapic_eoi`). `tear_down_irq_attachments(task_id)` invocato in `task_exit` rilascia gli slot. (c) **FSM bring-up estesa** in `crates/omni-driver-net-virtio/src/bringup.rs` con `BringUp { phase, retries }` state-machine driver + `Event::{Advance, Retry, Abort}` + `BringUpError` (8 varianti: DeviceFailed, RequiredFeaturesAbsent, FeaturesNotAccepted, MmioMapFailed, DmaMapFailed, IrqAttachFailed, RetryBudgetExhausted, TerminalAdvanceAttempted) + `MAX_RETRIES = 3` retry budget + `StepKind` proiezione (`WriteDeviceStatus(u8)`, `NegotiateFeatures`, `ReadDeviceStatus`, `ConfigureVirtqueues`, `ReadMac`, `SetDriverOk`, `ParkExit`). +17 nuovi host-side test su BringUp / Event / Phase. (d) **Nuovo `crates/omni-driver-net-virtio-image/`** workspace-excluded sibling crate (pattern simmetrico a `kernel-runner` ↔ `omni-kernel`) con `Cargo.toml` (workspace-excluded, target `x86_64-unknown-none`, profile release `lto=true` `opt-level=z` `panic=abort`), `.cargo/config.toml` minimo (eredita workspace rustflags), `src/main.rs` `no_std + no_main` `_start` entry che (i) costruisce un `BringUp::new()`, (ii) avanza la FSM fino a terminale via `Event::Advance` (no syscalls reali in P6.7.8.3 — i tokens vengono depositati dal kernel `DriverLoad` trampoline post-P6.7.8.x), (iii) `sys_exit(0)` su DriverOk / `sys_exit(1)` su Failed via raw `syscall` instruction; include `PanicOnAlloc` `#[global_allocator]` defensive stub (panics su qualunque heap call al runtime perché lo skeleton non ha allocator wired). `Cargo.toml` workspace `exclude` esteso. (e) PCB esteso con `dma_mappings: Vec<DmaMapping>` + `irq_attachments: Vec<IrqAttachment>` per teardown. (f) `ipc.rs` nuovo accessor read-only `pub unsafe fn ipc_registry() -> &'static KernelIpcRegistry`. +21 nuovi host-side test (3 PCB DMA/IRQ round-trip + 17 BringUp FSM + 1 dispatcher EACCES sharing). Workspace test count 716 → **737 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + clippy driver-image bin (target `x86_64-unknown-none` release) + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` tutti clean. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.3 virtio-net bring-up` (cyan), Next=`P6.7.8.4 NVMe driver scaffold`, Phase=`1 - Microkernel POC (~99.3%)`, Tests=`737 workspace pass`. P6.7.8.4 (NVMe driver scaffold sotto `crates/omni-driver-nvme`) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.4 — NVMe driver crate scaffold** (closed 2026-05-20) — new `crates/omni-driver-nvme` added as a full workspace member (`no_std + alloc` lib, `#![cfg_attr(not(test), no_std)]`, `extern crate alloc`), mirroring the `omni-driver-net-virtio` precedent (P6.7.8.2): skeleton-only, zero syscall integration. Five `pub` modules anchored on `OIP-Driver-NVMe-014` § S1-S6: (a) `pci_ids` — PCI class triple `0x01:0x08:0x02` (mass storage / non-volatile memory / NVM Express I/O controller) per OIP-014 § S1 + § R4 + NVMe 1.4 base spec § 2.1, `is_nvme_class(class, subclass, prog_if) -> bool` pure-function matcher that the kernel-side `pci_class` matcher + the driver's own ECAM walk both consume; (b) `controller_regs` — NVMe 1.4 § 3.1 register offsets `CAP=0x00`, `VS=0x08`, `INTMS=0x0C`, `INTMC=0x10`, `CC=0x14`, `CSTS=0x1C`, `AQA=0x24`, `ASQ=0x28`, `ACQ=0x30`, `CMBLOC=0x38`, `CMBSZ=0x3C`, `DOORBELL_ARRAY_OFFSET=0x1000`, `CONTROLLER_REGISTER_REGION_BYTES=0x4000` + `CC` field encodings (`CC_EN_BIT`, `CC_IOSQES_SHIFT=16`, `CC_IOCQES_SHIFT=20`, `CC_IOSQES_VALUE=6`, `CC_IOCQES_VALUE=4`, `cc_enable_value()`) + `CSTS` bits (`CSTS_RDY_BIT`, `CSTS_CFS_BIT`) + `sq_tail_doorbell_offset(qid, dstrd)` / `cq_head_doorbell_offset(qid, dstrd)` with `checked_mul`/`checked_shl` overflow guards; (c) `queue_config` — admin/IO queue depth bounds per NVMe 1.4 § 5.1 / § 5.5 (`MIN_QUEUE_DEPTH=1`, `MAX_ADMIN_QUEUE_DEPTH=4096`, `MAX_IO_QUEUE_DEPTH=65536`) + OIP-014 § R5 cap `MAX_IO_QUEUE_COUNT=4` + manifest defaults `DEFAULT_ADMIN_*=64` / `DEFAULT_IO_*=1024` / `DEFAULT_IO_QUEUE_COUNT=1` + entry sizes `SQ_ENTRY_BYTES=64` / `CQ_ENTRY_BYTES=16` (NVMe 1.4 § 4.2 / § 4.6) + validators `is_valid_admin_depth` / `is_valid_io_depth` / `is_valid_io_queue_count` + `encode_aqa(asqs, acqs) -> Option<u32>` bit-pack helper; (d) `transfer_model` — `TransferModel::Prp` only (OIP-014 § M4 + § R1, SGL out of scope, `#[non_exhaustive]` for future), `from_manifest_str("prp")` parser + `as_manifest_str()` round-trip, `PRP_PAGE_SIZE=4096` + `PRP_ENTRIES_PER_LIST_PAGE=512` + `MAX_BLOCK_COUNT_PER_COMMAND=2048` (OIP-014 § S2 cap), `is_prp_aligned(iova)` 4 KiB-alignment predicate (OIP-014 § TC5), `PrpLayout::{SinglePage, TwoPages, PrpList{n_entries}}` + `prp_layout(len)` pure-function classifier; (e) `bringup` — 13-step `Phase` enum `#[repr(u8)]` (`PciEnumeration → MmioMap → ReadCap → DisableController → SetupAdminQueues → EnableController → AttachInterrupts → IdentifyController → IdentifyActiveNsList → IdentifyNamespace → CreateIoQueues → RegisterBlkChannel → Ready`) + terminal `Failed=13`, `BringUp { phase, retries }` state-machine driver with `MAX_RETRIES=3` budget, `Event::{Advance, Retry, Abort(BringUpError)}`, `BringUpError` (12 variants: NoMatchingDevice, MmioMapFailed, UnsupportedNvmeVersion, UnsupportedPageSize, ControllerReadyTimeout, ControllerFatal, DmaMapFailed, IrqAttachFailed, AdminCommandFailed, UnsupportedSectorSize, BlkChannelRegistrationFailed, InvalidManifestQueueDepth, RetryBudgetExhausted, TerminalAdvanceAttempted), `StepKind` projection (`EnumeratePci`, `MapControllerRegisters`, `ReadCapabilities`, `WriteControllerConfig(0|1)`, `AllocateAdminQueues`, `AttachMsiXVectors`, `SubmitIdentify*`, `CreateIoQueuePair`, `RegisterBlkChannel`, `EnterBlkLoop`, `ParkExit`). New `crates/omni-driver-nvme/manifest.toml` developer-authored TOML v1 template (consumed offline by `omni-driver-pack` to produce the omni-pack v1 blob `DriverLoad` ingests) with shape OIP-013 § S5.1 + OIP-014 § S1 (`[meta]` / `[capabilities]` / `[matchers]` / `[nvme]` block with `admin_sq_depth`/`admin_cq_depth`/`io_sq_depth`/`io_cq_depth`/`io_queue_count`/`transfer_model = "prp"`/`format_nvm_enabled = false`/`discard_enabled = true`). `scripts/check-no-blanket-allow.sh` extended with the new crate (13 → 14 crate-root files scanned). Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.4 NVMe driver scaffold` (cyan), Next=`P6.7.8.5 NVMe bring-up wire`, Tests=`803 workspace pass`. +66 new host-side test (7 pci_ids class-triple + 11 controller_regs offset/encoding/doorbell-math — 2 ulteriori layout invariant promoted to `const _: () = assert!(...)` compile-time check + 11 queue_config bounds/defaults/AQA-encode + 14 transfer_model PRP-alignment/layout-classifier + 23 bringup Phase/BringUp/StepKind). Workspace test count 737 → **803 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). P6.7.8.5 (NVMe bring-up FSM wired to real `MmioMap`/`DmaMap`/`IrqAttach` syscalls inside the new `crates/omni-driver-nvme-image/` excluded sibling crate, same pattern as `omni-driver-net-virtio-image`) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.5 — NVMe bootable image sibling** (closed 2026-05-20) — new `crates/omni-driver-nvme-image/` added as a workspace-excluded bootable Ring 3 ELF sibling crate, mirror del precedente `omni-driver-net-virtio-image` (P6.7.8.3) e dello split `omni-kernel` ↔ `kernel-runner`. `Cargo.toml` `no_std + no_main` per target `x86_64-unknown-none` (profile release `lto=true` `codegen-units=1` `opt-level=z` `panic=abort` `strip=debuginfo`); unica dep `omni-driver-nvme` via path; `.cargo/config.toml` minimo eredita workspace rustflags. `src/main.rs` espone `_start` `#[unsafe(no_mangle)] pub extern "C" fn` che (a) costruisce `BringUp::new()` (parked at `Phase::PciEnumeration`), (b) avanza la FSM 13-step via `Event::Advance` fino al terminale, (c) `sys_exit(0)` se `Phase::Ready` else `sys_exit(1)` via raw `syscall` (rax=11=TaskExit + rdi=code + lateout rcx/r11), (d) `syscall5` helper inline-asm pre-cablato per `MmioMap (70)` / `DmaMap (71)` / `IrqAttach (72)` gated `#[allow(dead_code)]` finché P6.7.8.x deposita i tokens via DriverLoad trampoline. Allocator stub `PanicOnAlloc` `#[global_allocator]` defensive che panica su qualunque heap call (la FSM `BringUp` è `Copy` quindi nessuna alloc attesa a runtime; qualunque allocazione futura inattesa surfacizza loudly via `TaskExit(2)` dal panic handler). Panic handler `#[panic_handler]` chiama `sys_exit(2)`. Syscall numbers `SYS_TASK_EXIT=11` / `SYS_WRITE_CONSOLE=60` / `SYS_MMIO_MAP=70` / `SYS_DMA_MAP=71` / `SYS_IRQ_ATTACH=72` pinned localmente per evitare circular workspace dep con omni-kernel. Root `Cargo.toml` `[workspace.exclude]` esteso con il nuovo crate (commento cross-reference OIP-014 § S6 + OIP-013 § S5.3 step 9 + build invocation). `.gitignore` esteso con `/crates/omni-driver-nvme-image/target/`. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.5 NVMe image sibling` (cyan), Next=`P6.7.8.6 e1000e (M2) driver`, Phase=`1 - Microkernel POC (~99.5%)`, Tests=`803 workspace pass`. Workspace test count invariato a **803 pass / 0 fail** (il sub-step non aggiunge host-side test perché il crate è bare-metal-only ed eredita la FSM dalla libreria già testata in P6.7.8.4). All gates clean: `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal + clippy mb12-userprobe + clippy kernel-runner + clippy nvme-image (target `x86_64-unknown-none --release`) + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` (14 crate-root files) + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` (0 error / 0 warning su 19 file) tutti clean. Build artifact ELF `target/x86_64-unknown-none/release/omni-driver-nvme-image` generato clean (eseguibile Ring 3 pronto per future ingestion via `DriverLoad (73)` syscall). Smoke validation hardware Proxmox VMID 103 (`100.101.77.9`, 8 vCPU q35 OVMF + swtpm TPM 2.0 + 4 GiB RAM): boot UEFI image via `cargo +nightly run --manifest-path disk-image/Cargo.toml -- kernel-runner/.../kernel-runner` (2.06 MiB), scritta sul zvol `/dev/zvol/rpool/data/vm-103-disk-6` via `dd bs=1M conv=fsync`, VM avviata. Serial log mostra sequenza completa `[mb14.a]` → `[mb14.h.2]` + `[elf] probe OK` + `[virtio] tablet ready`, zero errori / zero panic / zero #PF / zero warning. Screenshot VNC (`qm monitor screendump`) confermato: Build Info panel renderizzato sul GOP framebuffer 1280×800 con commit hash `4b875fd` + Active/Next/Phase/Tests aggiornati. P6.7.8.6 (e1000e driver M2 — Ethernet bare-metal, primo driver non-virtio) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.8 — `DriverLoad (73)` syscall handler** (closed 2026-05-20) — replaces the `NotYetImplemented` stub with the end-to-end OIP-013 § S5.3 verification + spawn chain. New module `crates/omni-kernel/src/bare_metal/syscall_entry.rs::driver_load_handlers` (`#[cfg(all(feature = "bare-metal", target_os = "none", not(test)))]`) exposes `driver_load(args)` returning `SyscallReturn` via the two-register rich path: (a) user-pointer validation for cap_ptr/cap_len (≤ `MAX_TOKEN_BYTES = 1024`) and pack_ptr/pack_len (≤ `MAX_PACK_BYTES = 32 MiB`); (b) postcard `CapabilityToken` decode via `omni_types::wire::decode_canonical` + `Ed25519CapabilityProvider::placeholder().verify_signed_token` time-window + signature check + `is_driver_framework_action` + `Action::DriverLoad` + `Resource::Any` resource pin (defence-in-depth: token MUST be scoped to `Any`, not a concrete `PciDevice` / `MmioRegion`); (c) heap-side `Vec<u8>` allocation of `pack_len` bytes and user-→-kernel copy via `core::ptr::copy_nonoverlapping` (the bump allocator does not reclaim, but the call surface is bounded — N drivers × N MiB at boot only); (d) `decode_omni_pack` envelope validation + `postcard_decode_manifest` body decode + `hydrate_manifest(body, signature)` reconstruction; (e) full `verify_manifest(&manifest, image_bytes)` → BLAKE3 hash check + `KNOWN_ISSUERS` lookup + Ed25519 signature verify; (f) errno translation via `manifest_errno` (MalformedPack/PackTooLarge/ImageHashMismatch → EINVAL; UnknownIssuer/SignatureInvalid → EACCES); (g) `ProcessControlBlock::spawn_from_elf(image_bytes, PhysAddr(boot_pml4), &mut mapper, alloc, sched, PriorityClass::System, KernelPrincipal::ZERO)` invocation using the new `bare_metal::BOOT_CR3` static (one-shot publish in `kmain` after `read_cr3()`; same pattern as `PHYS_OFFSET`); (h) return `SyscallReturn::ok(task_id.0)` on success, `SyscallReturn::err(syscall_errno::ENOSPC)` on spawn failure. Dispatcher wiring: `SyscallNumber::DriverLoad` removed from the `NotYetImplemented` tail arm and joined to the `MmioMap | DmaMap | IrqAttach` group in `dispatch` (returns `CapabilityDenied` as a defensive sentinel); `dispatch_full` adds a `DriverLoad` arm that routes to `driver_load_handlers::driver_load` on bare-metal or returns `EACCES` on the host build (no `FRAME_ALLOC`/`SCHEDULER`/`BOOT_CR3` singletons available). Token-deposit trampoline (OIP-013 § S5.3 step 8 — attenuated child tokens pre-installed in the driver's capability namespace) deliberately deferred to P6.7.8.9: drivers spawned in P6.7.8.8 reach `_start` but the `MmioMap`/`DmaMap`/`IrqAttach` calls inside them still need a separately-presented capability token. The split decouples the ELF loader + signature chain from the capability-store wiring. Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.8 DriverLoad syscall` (cyan), Next=`P6.7.8.9 cap deposit trampoline`, Phase=`1 - Microkernel POC  (~99.8%)`, Tests=`867 workspace pass`. +3 nuovi host-side test in `bare_metal::boot_cr3_tests::*` (12-bit mask pin + zero-default observer + aligned-value round-trip). Existing dispatcher tests adjusted: `dispatcher_remaining_driver_framework_syscalls_return_not_yet_implemented` narrowed to `dispatcher_remaining_tee_syscalls_return_not_yet_implemented` (DriverLoad removed from the NotYetImplemented set); `dispatcher_driver_framework_legacy_arm_returns_capability_denied` extended to include `DriverLoad`; `dispatcher_full_dma_map_and_irq_attach_surface_eaccess_on_host` renamed to `dispatcher_full_dma_map_irq_attach_and_driver_load_surface_eaccess_on_host` and extended; `kernel_syscall_dispatch_driver_framework_numbers_route` window `70..=72` widened to `70..=73`. Workspace test count 864 → **867 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). All gates clean: `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + clippy net-virtio-image + clippy nvme-image + clippy e1000e-image (target `x86_64-unknown-none --release`) + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` (15 crate-root files) + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` (0 error / 0 warning su 19 file) tutti clean. P6.7.8.9 (capability deposit trampoline — pre-install attenuated child tokens at well-known user-VA slots in the driver's address space before the first dispatch tick) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.9 — capability deposit trampoline** (closed 2026-05-20) — chiude `OIP-Driver-Framework-013` § S5.3 step 8 («mint attenuated child tokens bound to the new driver process's NodeId, pre-installed in the driver's initial capability namespace») precedentemente deferito in P6.7.8.8. Tre nuovi moduli kernel: (a) `crates/omni-kernel/src/entropy.rs` — Phase-1 CSPRNG con `seed_from_hw_32()` (mix `RDRAND` 10-retry CPUID-gated + `RDTSC` XOR + SplitMix64 finalizer su 4 chunk a 64-bit), `KernelCsprng` wrapper su `ChaCha20Rng` da `rand_chacha 0.3`, global `static KERNEL_CSPRNG: spin::Mutex<Option<KernelCsprng>>` con lazy init e `with_csprng(|rng| …)` accessor; Phase-2 API `add_entropy(&[u8])` (XOR-mix nel buffer corrente + reseed) + `reseed([u8;32])` (full re-key) **designed ma not wired** — i call site Phase-2 (IRQ handler, network driver post-bringup) sono follow-up. (b) `crates/omni-kernel/src/driver_cap_issuer.rs` — kernel-side Ed25519 signing key statica `DRIVER_CAP_ISSUER_SEED = [0xCAFEBABE × 8]` (DEV ONLY placeholder, sostituzione TEE-derived sealing key deferita a P5.2 + OIP key-custody policy); helper `kernel_signing_key()` produce un `OmniSigningKey::from_bytes`. (c) `crates/omni-kernel/src/cap_deposit.rs` — wire-format header `OMNICAPS` magic + version 1 + count + flat indexed entry table `[u32 action_tag, u32 resource_tag, u32 token_offset, u32 token_len]` × N + packed postcard `CapabilityToken` blobs, mappato read-only (`PTE_PRESENT|PTE_USER|PTE_NO_EXEC`) a `DRIVER_CAP_DEPOSIT_VA = 0x0000_0000_0010_0000` (1 MiB) per 8 pagine = `DRIVER_CAP_DEPOSIT_LEN = 32 KiB` (worst-case ~150 byte/token × 64 entry + header); encoder `encode_deposit_page` mintà un `CapabilityToken` per ogni `Resource` in `DriverCapabilities` (`mmio_regions → MmioMap`, `dma_windows → DmaMap`, `irq_lines → IrqAttach`, `pci_devices → PciConfigRead+PciConfigWrite`) con `CapabilityId::from_bytes(KERNEL_CSPRNG.next_16_bytes())` (bypass `omni-capability/mint` feature evitando `getrandom`/`id-generation` su bare-metal) + `subject = NodeId::from_attestation_hash(provider.node_id_bytes())` (placeholder `[0u8; 32]`) + `TimeWindow { not_before: boot_seconds, not_after: + 7_776_000 }` (90 giorni) + signed via `CapabilityToken::sign_payload(&kernel_key, payload)`; bare-metal `deposit_for_driver` alloca + mappa 8 frame contigui + popola via `core::ptr::copy_nonoverlapping` attraverso il direct-map. Wirato in `driver_load_handlers::driver_load` dopo `spawn_from_elf` ma prima del return; il `task_id` ora ritornato anche se il deposit fallisce (driver vive senza capability, primo `MmioMap` → `EACCES` — accettato perché osservabile in user space; future P6.7.8.10 wires `scheduler.cancel_spawn` per rollback atomico). `ProcessControlBlock` esteso con `cap_deposit_va: Option<u64>` (None per processi senza deposit). `Ed25519CapabilityProvider::node_id_bytes()` accessor aggiunto. Cargo deps nuove: `rand_core 0.6 default-features = false` + `rand_chacha 0.3 default-features = false` + `spin 0.9 features = ["mutex", "spin_mutex"]`. Build Info aggiornato: Active=`P6.7.8.9 cap deposit trampoline` (cyan), Next=`P6.7.8.10 driver-shared SDK`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`885 workspace pass`. +19 nuovi host-side test (7 `entropy::tests::*` + 3 `driver_cap_issuer::tests::*` + 9 `cap_deposit::tests::*`). Workspace test 867 → **885 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). All gates clean: `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal `--features bare-metal` + clippy `--features bare-metal,mb12-userprobe` + clippy kernel-runner + clippy net-virtio-image + clippy nvme-image + clippy e1000e-image (target `x86_64-unknown-none --release`) + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` (15 crate-root files) + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` (0 error / 0 warning su 19 file). Spike doc archived a [`docs/plans/p6-7-8-9-cap-deposit-trampoline.md`](docs/plans/p6-7-8-9-cap-deposit-trampoline.md). P6.7.8.10 (SDK helper `omni_driver_shared::caps::find_token(action_tag, resource_predicate)` + rifattorizzazione driver crates per consumarlo) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [x] **P6.7.8.6 — e1000e driver crate scaffold** (closed 2026-05-20) — new `crates/omni-driver-e1000e` added as a full workspace member (`no_std + alloc` lib, `#![cfg_attr(not(test), no_std)]`, `extern crate alloc`), mirroring the `omni-driver-net-virtio` (P6.7.8.2) and `omni-driver-nvme` (P6.7.8.4) precedents: skeleton-only, zero syscall integration. Five `pub` modules anchored on `OIP-Driver-Net-015` § S5 + § S8 and on the Intel 82574L Gigabit Ethernet Controller datasheet: (a) `pci_ids` — Intel vendor `0x8086` + five v0.3 device IDs (`0x10D3` 82574L / `0x153A` I217-LM / `0x153B` I217-V / `0x15A1` I218-LM / `0x15A3` I219-LM), `is_e1000e_device(vendor, device) -> bool` pure-function matcher consumed by the manifest matcher table and the driver's own PCI enumeration walk; (b) `controller_regs` — Intel 82574L datasheet § 10 CSR offsets `CTRL=0x0000`, `STATUS=0x0008`, `MDIC=0x0020`, `ICR=0x00C0`, `ITR=0x00C4`, `IMS=0x00D0`, `IMC=0x00D8`, `RCTL=0x0100`, `TCTL=0x0400`, `RDBAL=0x2800`/`RDBAH=0x2804`/`RDLEN=0x2808`/`RDH=0x2810`/`RDT=0x2818`, `TDBAL=0x3800`/`TDBAH=0x3804`/`TDLEN=0x3808`/`TDH=0x3810`/`TDT=0x3818`, `RAL0=0x5400`/`RAH0=0x5404` + field encodings (`CTRL.RST=bit26`, `STATUS.FD=bit0`/`LU=bit1`, `IMC_DISABLE_ALL=0xFFFFFFFF`, `RCTL.EN=bit1`/`BAM=bit15`/`SECRC=bit26`/`BSIZE_SHIFT=16`, `TCTL.EN=bit1`/`PSP=bit3`/`CT_SHIFT=4`/`COLD_SHIFT=12`/`CT_DEFAULT=0x0F`/`COLD_DEFAULT_FD=0x40`, `RAH.AV=bit31`) + `rctl_enable_value()` / `tctl_enable_value()` const composers + `CSR_REGION_BYTES=0x20000` (128 KiB MMIO window per OIP-015 § S5.1 step 1) with `const _: () = assert!(...)` compile-time layout invariants (region covers RAH0 + TDT, TX block 0x1000 bytes past RX block); (c) `ring_config` — RX/TX descriptor ring depth bounds (`MIN_RING_DEPTH=1`, `MAX_RING_DEPTH=4096`) + RX buffer count bounds (`MIN_RX_BUFFER_COUNT=1`, `MAX_RX_BUFFER_COUNT=8192`) + manifest defaults `DEFAULT_RX_RING_DEPTH=256` / `DEFAULT_TX_RING_DEPTH=256` / `DEFAULT_RX_BUFFER_COUNT=512` + legacy descriptor sizes `RX_DESCRIPTOR_BYTES=16` / `TX_DESCRIPTOR_BYTES=16` (Intel 82574L datasheet § 10.7.1 / § 10.8.1) + `RX_BUFFER_BYTES=2048` (matches `RCTL.BSIZE=0b00`) + validators `is_valid_ring_depth(depth)` (power-of-two enforced per OIP-015 § S1.1) / `is_valid_rx_buffer_count(count)` + `rx_ring_bytes(depth)` / `tx_ring_bytes(depth)` checked-mul helpers; (d) `interrupts` — IMS/IMC/ICR bit positions `TXDW=bit0`, `LSC=bit2`, `RXT0=bit7` + `ENABLED_IMS = RXT0|TXDW|LSC = 0x85` (OIP-015 § S5.1 step 10 mandate) + `icr_has_rx/has_tx/has_link_change` const classifiers consumed by the IRQ trampoline; (e) `bringup` — 13-step `Phase` enum `#[repr(u8)]` (`PciEnumeration → MmioMap → DisableInterrupts → GlobalReset → ReadMac → PhyInit → SetupRxRing → PostRxBuffers → SetupTxRing → ConfigureRxTx → EnableInterrupts → AttachIrq → RegisterNetChannel → Ready`) + terminal `Failed=14`, `BringUp { phase, retries }` state-machine driver with `MAX_RETRIES=3` budget mirrored from P6.7.8.3 / P6.7.8.4, `Event::{Advance, Retry, Abort(BringUpError)}`, `BringUpError` (10 variants: NoMatchingDevice, MmioMapFailed, ResetTimeout, InvalidMac, PhyInitFailed, DmaMapFailed, IrqAttachFailed, NetChannelRegistrationFailed, InvalidRingDepth, RetryBudgetExhausted, TerminalAdvanceAttempted), `StepKind` projection (`EnumeratePci`, `MapControllerRegisters`, `WriteImcMaskAll`, `TriggerCtrlReset`, `ReadReceiveAddress`, `IssueMdioAutonegotiate`, `AllocateRxRing`, `PrepostRxBuffers`, `AllocateTxRing`, `WriteRctlTctl`, `WriteImsEnabled`, `AttachMsiXVector`, `RegisterNetChannel`, `EnterRxTxLoop`, `ParkExit`). New `crates/omni-driver-e1000e/manifest.toml` developer-authored TOML v1 template (consumed offline by `omni-driver-pack` to produce the omni-pack v1 blob `DriverLoad` ingests) with shape OIP-013 § S5.1 + OIP-015 § S1 (`[meta]` / `[capabilities]` declaring 128 KiB MMIO + 4 GiB IOVA / `[matchers]` listing all five vendor/device pairs / `[net]` block matching the `ring_config` defaults). Root `Cargo.toml` `[workspace.members]` extended (`crates/omni-driver-e1000e`). `scripts/check-no-blanket-allow.sh` `SCOPED_CRATES` extended (14 → 15 crate-root files scanned). Build Info panel aggiornato (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.8.6 e1000e (M2) scaffold` (cyan), Next=`P6.7.8.7 e1000e image sibling`, Phase=`1 - Microkernel POC (~99.6%)`, Tests=`864 workspace pass`. +61 new host-side test (7 pci_ids vendor/device/distinct/matcher-accept/matcher-reject-ixgbe/matcher-reject-non-intel/matcher-reject-zero + 10 controller_regs split-by-block + field-bits + RCTL/TCTL composers + 7 interrupts bit-positions/IMS-value/classifiers/burst/unrelated + 15 ring_config bounds/defaults/validators/byte-sizes/overflow + 22 bringup Phase/BringUp/StepKind monotonic/advance/retry/abort/exhaust/park-exit). Workspace test count 803 → **864 pass / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`). All gates clean: `cargo clippy --workspace --all-features --all-targets -- -D warnings` + clippy bare-metal target + clippy mb12-userprobe + clippy kernel-runner + clippy net-virtio-image (target `x86_64-unknown-none --release`) + clippy nvme-image (target `x86_64-unknown-none --release`) + `cargo fmt --all -- --check` + `bash scripts/check-no-blanket-allow.sh` (15 crate-root files) + `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` + `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` + `python3 scripts/lint-oips.py` (0 error / 0 warning su 19 file) tutti clean. P6.7.8.7 (e1000e bootable image sibling `crates/omni-driver-e1000e-image/` excluded come kernel-runner / net-virtio-image / nvme-image) prossimo. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`).
  - [ ] **P6.7.8.7+ — Driver impl sequence** — e1000e image sibling (M2 Ethernet bare-metal bringup) → TEE (funding-dep).
  - [x] **P6.7.9-pre.5 — Intel VT-d live MMIO register programming** (closed 2026-05-21) — completes the live-programming half of TASK-010 for the Intel backend. (1) **`crates/omni-kernel/src/bare_metal/iommu/vtd.rs`** extended: new `unit_base` / `root_table_phys` / `invalidation_queue_phys` / `invalidation_queue_tail` / `hardware_activated` fields on `VtdBackend` (all `0`/`false` while dormant, populated by `prepare_activation`); 3 new GSTS bit constants (`GSTS_BIT_TES`/`GSTS_BIT_RTPS`/`GSTS_BIT_QIES` per Intel VT-d spec rev 4.1 § 10.4.5); 4 new invalidation-queue layout constants (`INV_QUEUE_SIZE_ORDER=0`, `INV_QUEUE_ENTRY_COUNT=256`, `INV_QUEUE_ENTRY_BYTES=16`, `INV_QUEUE_BYTES=4096`); 6 new invalidation-descriptor tag constants (`INV_DESC_TYPE_CONTEXT_CACHE`/`INV_DESC_TYPE_IOTLB`/`INV_DESC_TYPE_INVALIDATE_WAIT`/`INV_DESC_CTX_GRAN_GLOBAL`/`INV_DESC_IOTLB_GRAN_GLOBAL`/`INV_DESC_WAIT_STATUS_WRITE` per § 6.5.2); `VTD_ACTIVATION_POLL_LIMIT=1_000_000` bounded retry budget for status-mirror polls; pure-function encoders `encode_iqa(queue_phys, size_order) -> u64` (bit 12..63 base, bit 11 reserved, bit 10 DW=0 legacy 128-bit descriptors, bits 0..2 QS, with defensive mask of reserved high bits above bit 51 to match Intel MGAW), `encode_iotlb_global_invalidate() -> (u64, u64)` (low qword: Type=0x2 | G=01 global), `encode_context_cache_global_invalidate() -> (u64, u64)`; new `VtdActivateError` enum (4 variants: `NotPrepared` / `RootTableTimeout` / `QueueEnableTimeout` / `InvalidationTimeout`) with `impl From<VtdActivateError> for IommuError → IommuError::ActivationFailed`; new accessor methods (`unit_base()`, `root_table_phys()`, `invalidation_queue_phys()`, `is_hardware_activated()` — all `const fn`); new `prepare_activation(unit_base, root_table_phys, invalidation_queue_phys)` pure-state update (idempotent for same params; clears `hardware_activated` on different params); new `#[cfg(target_os = "none")] pub unsafe fn activate_hardware(&mut self, phys_offset: u64) -> Result<(), VtdActivateError>` that drives the spec-faithful MMIO sequence: (a) `volatile_write64` RTADDR ← root_table_phys, (b) `volatile_write32` GCMD ← `SRTP` + bounded poll on `GSTS.RTPS`, (c) `volatile_write64` IQA ← `encode_iqa(iq_phys, 0)` + `volatile_write64` IQT ← 0, (d) `volatile_write32` GCMD ← `QIE` + bounded poll on `GSTS.QIES`, (e) `write_queue_entry(slot=0, encode_iotlb_global_invalidate())` + `volatile_write64` IQT ← 16 + bounded poll on IQH reaching tail. New `unsafe fn mmio_read32/mmio_read64/mmio_write32/mmio_write64` helpers (volatile semantics, gated `#[cfg(target_os = "none")]`); `unsafe fn poll_gsts_bit(unit_va, bit) -> bool` + `unsafe fn poll_iqh_reaches(unit_va, tail) -> bool` bounded retry helpers; `unsafe fn write_queue_entry(queue_va, slot, lo, hi)` 128-bit descriptor writer with `core::ptr::write_volatile`. `GCMD.TE` is **NOT** raised by this slice (per-domain translation gating lands when the driver framework attaches its first PCI device — future P6.7.9-pre.7+). (2) **`crates/omni-kernel/src/bare_metal/iommu/mod.rs`** extended: new `pub static IOMMU_UNIT_BASE: AtomicU64` + `set_iommu_unit_base(register_base)` + `iommu_unit_base() -> u64` accessors (same shape as `IOMMU_VENDOR` / `IOMMU_UNIT_COUNT`); new `register_base: u64` field on `ProbeResult` (matches DRHD/IVHD entry 0); new `read_table_drhd_info(table_phys, phys_offset) -> Option<(usize, u64)>` and `read_table_ivhd_info(...) -> Option<(usize, u64)>` helpers that return `(count, first_entry_base)`, with the original `read_table_drhd_count` / `read_table_ivhd_count` rewired to delegate; bare-metal `probe()` extended to extract the first entry's MMIO base and stash it via `set_iommu_unit_base`; new `IommuError::ActivationFailed` variant for the activation taxonomy; new `pub fn prepare_vt_d_unit(unit_base, root_table_phys, invalidation_queue_phys) -> Result<(), IommuError>` routes through `with_iommu_backend` to call `VtdBackend::prepare_activation` (returns `IommuError::Unsupported` for non-Intel variants — defence-in-depth); new `#[cfg(target_os = "none")] pub unsafe fn activate_intel_vt_d(phys_offset: u64) -> Result<bool, IommuError>` drives the live MMIO sequence (returns `Ok(false)` when not Intel or `iommu_unit_base() == 0`); new `pub fn iommu_hardware_activated() -> bool` accessor. (3) **`crates/omni-kernel/src/lib.rs`** kmain extended after `[paging] validated …` log + before scheduler init: gated `#[cfg(all(target_arch = "x86_64", target_os = "none"))]` block that fires only when `iommu_unit_base() != 0 && iommu_vendor() == Intel`; allocates the root-table + invalidation-queue frames from `FRAME_ALLOC`, zero-fills them via the direct-map (`core::ptr::write_bytes`), calls `prepare_vt_d_unit` + `activate_intel_vt_d(phys_offset_mb2)`, then logs one of `[iommu] vt-d activated  unit=<base>`, `[iommu] vt-d activate skip`, `[iommu] vt-d activate err`, `[iommu] vt-d prepare err`, or `[iommu] vt-d alloc err` (single-line surface for the QEMU + Proxmox boot smoke). (4) **Build Info panel** (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`): Active=`P6.7.9-pre.5 VT-d live MMIO` (cyan), Next=`P6.7.9-pre.6 AMD-Vi live MMIO`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`1101 workspace pass`. (5) **+23 new host-side tests**: 16 in `bare_metal::iommu::vtd::tests::*` (`gsts_bits_mirror_gcmd_positions`, `invalidation_queue_layout_constants_match_legacy_format`, `invalidation_descriptor_tags_match_spec_section_6_5_2`, `poll_limit_is_a_million`, 5 `encode_iqa_*` cases — base placement / reserved-bit mask / size-order encoding / size-order truncation / reserved-high-bit mask — , 2 `encode_*_invalidate_*` cases for IOTLB + context-cache descriptors, `vtd_activate_error_maps_to_iommu_activation_failed`, `fresh_backend_reports_dormant_state`, 3 `prepare_activation_*` cases — field round-trip / idempotent same-params / different-params resets `hardware_activated`); 7 in `bare_metal::iommu::tests::*` (`iommu_unit_base_round_trip`, `select_vendor_returns_zero_register_base_by_default`, 3 `prepare_vt_d_unit_*` cases — Intel happy path / Passthrough rejection / AMD rejection — , 2 `iommu_hardware_activated_false_for_*` cases for Passthrough + AMD). Workspace test count 1078 → **1101 pass / 0 fail**. (6) **Acceptance gates** — `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` clean. `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal,mb12-userprobe -- -D warnings` clean. `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings` clean. Clippy 3 driver-image siblings (`x86_64-unknown-none --release`) clean. `cargo fmt --all -- --check` clean. `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`. `RUSTDOCFLAGS="-D warnings" cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps` clean. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` clean. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)`. (7) **No new dependency**. The MMIO programming uses `core::ptr::{read_volatile, write_volatile}` exclusively; the bounded retry counter is plain `u32` arithmetic. (8) **Stato P6.7**: P6.7.0/1/2/2.bis/3/3.bis/4/5/6/7/8.0/8.1/8.2/8.3/8.4/8.5/8.6/8.7/8.8/8.9/8.10/9-pre.0/9-pre.1/9-pre.2/9-pre.3/9-pre.4/**9-pre.5** chiusi. **P6.7.9-pre.6 (AMD-Vi live MMIO register programming — device-table install, command-buffer setup, INVALIDATE_DEVTAB_ENTRY descriptor) prossimo**. Phase 1 ~99.9%. Note: TEE backend (OIP-016) resta funding-dep. Pre-existing SIGSEGV su `cargo test -p omni-kernel --lib` resta carryover §4.5 #16 (mitigato via `--test-threads=1`). Branch corrente `main`; il commit corrente aggiunge ~600 SLOC live VT-d programming (vtd.rs encoders + activation + MMIO helpers + 16 host test) + mod.rs activation surface (prepare_vt_d_unit + activate_intel_vt_d + IOMMU_UNIT_BASE atomic + ActivationFailed variant + 7 host test) + kmain wiring (root-table + invalidation-queue allocation + zero-fill + prepare + activate + boot log) + Build Info bump + todo/progress entries + CHANGELOG.

## P6.8 — First external security audit of kernel + capability system

- **Status:** `[!]` blocked-on P4 funding + P6.7 done
- **Priority:** P6 / Critical (Phase 1 deliverable)
- **Effort:** 4-8 settimane calendar (auditor's schedule)
- **Dependencies:** P4.2 (funding); P6.7 done; raccomandato anche P3.2 (cryptographer review) chiuso prima

---

Each of P6.1–P6.8 + P6.MB1–P6.MB14 will be expanded into its own task list when its corresponding OIP is filed (vedi `oips/` directory; `OIP-Kernel-003`, `OIP-Kernel-005`, `OIP-Kernel-012` già `Active`).

---

# P7 — Workspace serialization migration (`bincode` v2 → `postcard`)

**Goal:** resolve `RUSTSEC-2025-0141` (`bincode` v2 unmaintained) by migrating the workspace serialization layer to `postcard` 1.x, bumping the wire-protocol from `OMNI-PROTO-v0.1` to `OMNI-PROTO-v0.2`.
**Blocker for:** clean `cargo audit` and `cargo deny` runs on `main` and on every PR.
**Tracking OIP:** [`OIP-Serde-004`](oips/oip-serde-004.md) (`Last Call` since 2026-05-12; 14-day public-objection window closes 2026-05-26).
**Estimated effort:** 1–2 weeks (per the 5-step migration plan in `OIP-Serde-004` § S5).

---

## P7.1 — `OIP-Serde-004` Last Call closure

- **Status:** `[~]` (`Draft → Review → Last Call` all on 2026-05-12; 14-day public-objection window closes 2026-05-26)
- **Priority:** P7 / High
- **Effort:** 14-day Last Call window per `OIP-Process-001` § 5.3 + cryptographer review pass on the canonical-encoding contract (§ S2).
- **Dependencies:** none for advancement to `Review` or `Last Call`; cryptographer engagement (P3.2) for advancement to `Active` is recommended but not procedurally required.
- **Rationale:** the OIP needs to be `Active` for the migration evidence in M1–M5 to be ratified under the Standards-Track activation process.

**Acceptance:** OIP transitions `Draft → Review → Last Call → Active`. `Draft → Review` 2026-05-12 (commit `be4a920`); `Review → Last Call` 2026-05-12 (this commit). `Last Call → Active` triggers on 2026-05-26 (or earlier if ≥30% weighted vote is reached, per `OIP-Process-001` § 5.3).

---

## P7.2 — Migration steps M1–M5

- **Status:** `[~]` — M1–M5 all landed locally 2026-05-12 on branch `feat/p1-foundational-crates`. `OIP-Serde-004` remains in `Review` pending the `Review → Last Call → Active` transition; full `[x]` requires `audit.yml` cron green for 7 calendar days post-merge per the OIP's `Final` criterion.
- **Priority:** P7 / High
- **Effort:** ~1 week of focused work; each step is its own commit per `OIP-Serde-004` § S5.
- **Dependencies:** P7.1 `Active`.

**Sub-tasks:**

- [x] **P7.2.M1** — Workspace dep swap in `Cargo.toml`. Verified `cargo build --workspace --all-features` clean (commit `b8de469`).
- [x] **P7.2.M2** — `omni-types::wire` canonical-encoding helper module + clippy `disallowed-methods` on raw `postcard::*` calls outside the helper. Verified `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (commit `9b3d977`).
- [x] **P7.2.M3** — `omni-capability` `CapabilityToken` migration; 4 new round-trip regression tests pin postcard-canonical-encoding properties at the public-API boundary. 47 unit + 7 integration tests green (commit `b451539`).
- [x] **P7.2.M4** — `omni-tee` round-trip tests + `omni-types::ProtocolVersion::V0_2` constant. Five new wire-format tests on `Quote` and `SealedBlob` + two version-compatibility tests (commit `61a2b02`).
- [x] **P7.2.M5** — `crates/omni-capability/tests/wire_format_v0_2.rs` reference vector (`TokenPayload` byte prefix pinned at 49 bytes) + `crates/omni-tee/tests/wire_format_v0_2.rs` adversarial suite (4 tests covering bit-flip-on-covered-fields, prefix-truncation, trailing-byte-extension, swap-with-unrelated). `cargo audit` exit 0 (RUSTSEC-2025-0141 absent — `cargo tree --invert bincode` returns "did not match any packages"); `cargo deny check advisories` ok. (`bans` + `licenses` fail with **pre-existing** issues unrelated to OIP-Serde-004: `cpufeatures` 0.2/0.3 duplicate and `Unicode-DFS-2016` license — separate cleanup work.)

**Acceptance:** all workspace tests + 2 new test files green; `bincode` removed from `Cargo.lock` (`cargo tree --invert bincode` empty); `OIP-Serde-004` transitions to `Final` after 7 calendar days of clean `audit.yml` cron runs.

---

## P7.3 — `OMNI-PROTO-v0.2` documentation update

- **Status:** `[ ]` — **READY** (P7.2 M1-M5 ✅; non più blocked-on)
- **Priority:** P7 / Low (1 PR edit-only, sblocca check verde `oip-lint` collaterale)
- **Effort:** 1 day
- **Dependencies:** P7.2.M5 ✅
- **Rationale:** `docs/protocol/handshake.md` § 3.2 currently negotiates only `OMNI-PROTO-v0.1`. After P7.2.M5, the handshake spec must reflect the v0.2 cutover (`serde_format = "postcard-1.0"` discriminant; v0.1 negotiation removed). Il codice è già `omni_types::version::PROTOCOL_VERSION_V0_2`; solo doc-update.
- **Acceptance:**
  - [ ] `docs/protocol/handshake.md` § 3.2 menziona solo `OMNI-PROTO-v0.2`.
  - [ ] PR con label `area:docs` + `priority:P3` aperta e mergiata (admin fast-track, no codice).

---

# P8 — OIP-Container-006 reference implementation

**Tracking OIP:** [`OIP-Container-006`](oips/oip-container-006.md) (`Draft` filed 2026-05-12).

P8 turns the OmniContainer specification into the canonical Rust implementation under `crates/omni-container/`. Each milestone closes one of the OIP's subsystems and is a candidate for its own follow-up OIP if the subsystem's design surface raises significant questions during implementation.

## P8.1 — `crates/omni-container/` skeleton (closed 2026-05-12)

- **Status:** `[x]` closed 2026-05-12 (commit `31455a6`, on `feat/p1-foundational-crates`).
- **Priority:** P8
- **Effort:** done (~ 1 day implementer time)
- **Acceptance criteria:**
  - [x] Crate compiles clean under `cargo check --workspace --all-features`.
  - [x] Public trait surface (`ContainerEngine`, `ContainerLifecycleState`, `CapabilityProfile`, `OciImageRef`, `ContainerError`) exposed at the crate root.
  - [x] Every operational method returns `ContainerError::NotYetImplemented(<static slug>)`.
  - [x] Feature flags `kvm` (default), `tdx`, `sev-snp`, `all-backends` per OIP-Container-006 § 10.
  - [x] ≥ 15 unit tests + ≥ 1 integration test green (delivered 47 unit + 5 integration).
  - [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - [x] `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` clean.
  - [x] `cargo fmt --all -- --check` clean.

## P8.2 — KVM hypervisor backend implementation

- **Status:** `[ ]` blocked on a follow-up OIP (`OIP-Container-Engine-XXX`) that locks in the `kvm-ioctls` API surface, the vCPU thread model, the run-loop placement (tokio vs. dedicated thread), and the guest-kernel boot path.
- **Priority:** P8
- **Effort:** 4-6 engineer-months estimated per OIP-Container-006 § 10.
- **Dependencies:** P8.1 (this); a future `OIP-Container-Engine-XXX`.

## P8.3 — Guest Linux image build pipeline

- **Status:** `[ ]` blocked on a Stichting OMNI signing key (P4.1 derivative) and on the reproducible-build setup in the separate `omni-guest-linux` repo (does not exist yet).
- **Priority:** P8
- **Effort:** 3-4 engineer-months estimated.
- **Dependencies:** P4.1 (Stichting key custody); a future `OIP-Container-GuestImage-XXX`.

## P8.4 — Virtio host-side backends

- **Status:** `[ ]` per-backend, blocked on its own follow-up OIP: `OIP-Container-Networking-XXX` (virtio-net), `OIP-Container-Storage-XXX` (virtio-fs), TBD for virtio-gpu / virtio-vsock / virtio-rng.
- **Priority:** P8
- **Effort:** 5-6 engineer-months total estimated per OIP-Container-006 § 10.
- **Dependencies:** P8.2 (KVM engine), the per-backend OIPs.

## P8.5 — TDX + SEV-SNP confidential-VM modes

- **Status:** `[ ]` blocked on P5.2 / P5.3 (host TEE backends in `omni-tee`) reaching parity and on a Standards-Track OIP that locks in the per-container quote envelope shape.
- **Priority:** P8
- **Effort:** 2-3 engineer-months estimated.

## P8.6 — Wine integration image (`omni/linux-wine:N-stable`)

- **Status:** `[ ]` blocked on P8.3 (guest image pipeline).
- **Priority:** P8
- **Effort:** 1-2 engineer-months estimated.
- **Dependencies:** P8.3; tracking community ProtonDB compatibility reports.

## P8.7 — `cyDock-omni` fork retargeting

- **Status:** `[ ]` blocked on P8.2 + P8.4 reaching a stable REST API surface for the management plane.
- **Priority:** P8
- **Effort:** 3-4 engineer-months estimated per OIP-Container-006 "cyDock Evolution Path".
- **Dependencies:** P8.2, P8.4, plus a green light from the existing cyDock maintainer.

---

# P9 — Code hygiene & lint-debt management

**Goal:** maintain `omni-kernel` (and gradually the rest of the workspace) with **zero crate-root blanket `#![allow(...)]`**. Ogni soppressione intenzionale è localizzata, motivata, e validata da CI.
**Estimated effort:** 1 day setup + ongoing per ADR-0003.
**Tracking ADR:** [`ADR-0003 — No blanket allows in production crates`](docs/adr/0003-no-blanket-allows-in-production-crates.md).

## P9.1 — Step 7 closure (lift omni-kernel blanket allows)

- **Status:** `[x]` (closed 2026-05-18 — 4 commits on `main` `770c7aa → 1768966`).
- **Priority:** P9 / High (debt accumulato durante 7 iterazioni CI conformance su PR #29)
- **Effort:** delivered in ~1 giornata distribuita su 4 PR consecutivi
- **Dependencies:** v0.2.0 merge (PR #29) ✅
- **Deliverables:**
  - [x] **Step 7.1** (`770c7aa`) — lift `restriction` + `rustdoc` lints (~40 siti localizzati; +2 broken intra-doc links riscritti come code spans).
  - [x] **Step 7.3** (`50eddf1`) — lift `clippy::pedantic` (~68 siti, mix fix/allow module-level su `bare_metal/{cursor,demo,graphics,gdt,idt,input,paging,virtio_tablet,widget,wm}.rs`).
  - [x] **Step 7.4** (`83ff1e8`) — lift `clippy::nursery` + `clippy::cargo` (7 siti totali; 4 `too_long_first_doc_paragraph` in `scheduling.rs`, 2 `use_self` in `wm.rs`, 1 `cognitive_complexity` allow su `demo::run_desktop`).
  - [x] **Step 7.2** (`1768966`) — lift `unsafe_code` (~40 cfg-gated bare-metal siti; CI `blanket-allow-guard` flipped to **blocking**). **Last** dei 4 PR — sequenziato per landing immediato prima di MB11 (minimizza merge-conflict).
  - [x] **`scripts/check-no-blanket-allow.sh`** — bash guardrail script (whitelisted: doc URL, `warn(...)`, `cfg_attr(test, allow(...))`, `cfg_attr(all(feature = "bare-metal", ...))`).
  - [x] **CI job `blanket-allow-guard`** in `.github/workflows/ci.yml` — bloccante.
  - [x] **ADR-0003** `accepted`.
- **Acceptance:** `bash scripts/check-no-blanket-allow.sh` exit 0 (output: `ok (scanned 12 crate-root files)`); `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.

## P9.2 — Extend `check-no-blanket-allow.sh` ai restanti crate

- **Status:** `[ ]`
- **Priority:** P9 / Low
- **Effort:** ~1 giornata
- **Rationale:** lo script oggi scansiona solo `crates/<scoped>/src/{lib,main}.rs` (12 file). Altri crate workspace (`disk-image-builder/`, `kernel-runner/`, `omni-userprobe-helloworld` futuro) hanno la stessa policy ADR-0003 ma non sono enforced. Far passare anche loro alla guardrail.

---

# P10 — Branch / release workflow (post-v0.2.0)

**Goal:** mantenere `main` in stato release-able; cadenza di squash-merge dai branch feature; tag intermedi per milestone significativi.

## P10.1 — Merge `feat/kernel-mb11-userspace` → `main`

- **Status:** `[ ]` — **READY** (HEAD `1a0fa3e` complete; CI da validare)
- **Priority:** P10 / High
- **Effort:** 0.5-1 giornata (push remote + CI conformance + DCO sign-off + admin bypass su `cargo test (ubuntu-24.04)` SIGSEGV se ricompare)
- **Dependencies:** nessuna tecnica (MB11 + MB12 closed; ADR-0004 + ADR-0005 accepted). **Decisione esplicita:** se aprire ora un solo PR aggregato MB11+MB12+Step 7 oppure 3 PR sequenziali. Raccomandazione: 1 PR aggregato (Step 7 lì dentro già contiene 4 commit; aggiungere altri 4 da MB11+MB12 mantiene il branch leggibile per il review).
- **Deliverables:**
  - [ ] `git push origin feat/kernel-mb11-userspace`.
  - [ ] `gh pr create --base main --title "feat(kernel): MB10 follow-up + MB11 + MB12 + Step 7 (post v0.2.0)"`.
  - [ ] DCO sign-off check pass.
  - [ ] 11 required CI checks (vedi `progress-omni.md` § 2.3) → 10 verdi + 1 admin-bypass tollerato (`cargo test (ubuntu-24.04)` SIGSEGV — vedi P10.3 root cause).
  - [ ] Squash-merge con commit message ben formato (no Co-Authored-By AI per [CLAUDE.md § Git Attribution Policy](CLAUDE.md)).
- **Acceptance:** `main` HEAD include MB10 + MB11 + MB12 + Step 7; tag intermedio opzionale (`v0.2.1`?) deferred a P10.2.

## P10.2 — Tag intermedio v0.2.1 (decision pending)

- **Status:** `[ ]`
- **Priority:** P10 / Low
- **Effort:** 0.5 giornata (release notes + tag + GitHub release)
- **Rationale:** MB10 + MB11 + MB12 + Step 7 sono delta significativi (Ring 3 + IPC); valutare se rilasciare `v0.2.1` (patch — niente public API break) oppure attendere MB13 per `v0.3.0-alpha.1`. **Decisione raccomandata:** attendere MB13 (più narrabile come "real capability dispatch + bootable smoke fix"), ma se MB13 slitta oltre 2026-05-26 considerare `v0.2.1` come stop-gap.

## P10.3 — Risolvere `cargo test (ubuntu-24.04)` SIGSEGV

- **Status:** `[ ]` (carryover da v0.2.0 / PR #29 / PR #33)
- **Priority:** P10 / Medium (sblocca CI pulito; ad oggi richiede admin bypass su ogni PR)
- **Effort:** ~1 giornata
- **Rationale:** il binary `omni_kernel-…` exit con `signal: 11` al teardown del test harness *dopo* che tutti i unit test riportano `ok`. Locale macOS arm64 1.85.1 passa. Probabile bug nel drop di `bare_metal::paging::tests::TestArena` (raw 256-KiB alloc + manual dealloc consumed via `*mut RawPageTable`).
- **Deliverables (alternative):**
  - **Opzione (a) quick fix:** `--test-threads=1` nel workflow `.github/workflows/ci.yml` job `cargo test (ubuntu-24.04)`.
  - **Opzione (b) root cause fix:** rifattorizzare `TestArena` in `Arc<Mutex<...>>` o `&'static mut [MaybeUninit<u8>]` per evitare il manual dealloc race.
- **Acceptance:** CI green su `cargo test (ubuntu-24.04)` su 5 PR consecutivi senza admin bypass.

## P10.4 — CI smoke automatico per `mb11-userprobe` e `mb12-userprobe`

- **Status:** `[ ]` (blocked-on MB13.b — oggi il smoke triple-faulta)
- **Priority:** P10 / Medium
- **Effort:** 0.5 giornata per job (1 giornata totale)
- **Dependencies:** P6.MB13.b ✅
- **Deliverables:**
  - [ ] Estendere `scripts/qemu-boot-smoke.sh` con flag `--feature mb11-userprobe` + `--feature mb12-userprobe`.
  - [ ] Aggiungere 2 nuovi job in `.github/workflows/qemu-boot-smoke.yml` con `EXPECTED_LINES` esteso per `[user] hello` / `[mb12] channel 1 pre-created` / `ping` / `[user] exit=0` (x2 per MB12).
  - [ ] Branch protection update per renderli required (richiede admin token).

---

# Open decisions awaiting Founder input

Decisions resolved during P0 closure:

1. ~~**Engagement mode**~~ — **Resolved 2026-05-09:** *Implementer*. Claude writes deliverables, founder reviews. Confirmed across all P0 tasks; default for P1+ unless renegotiated.
2. ~~**P0 vs P1 ordering**~~ — **Resolved 2026-05-09:** *P0 first*. Closed before any code in foundational crates lands.
3. ~~**Phase 0 non-technical work (P4)**~~ — **Resolved 2026-05-09:** *Out of scope* for current implementer engagement; P4 remains in this document but execution is on the founder's calendar (notary, KVK, grants).

Decisions resolved during P2 closure (2026-05-10):

4. ~~**OIP-Process-001 authorship (P2.1)**~~ — **Resolved 2026-05-10:** *Claude drafts, founder reviews* (Implementer mode preserved from §1 above). OIP shipped under bootstrap fiat clause §6.3.
5. ~~**P1 vs P2 ordering**~~ — **Resolved de facto 2026-05-10:** P1 closed first; P2 followed within the same day. Sequence preserved but cycle time short enough that the "federated review of `omni-crypto`" benefit was not lost — `omni-crypto` carries an explicit `AWAITING_CRYPTO_REVIEW` marker (P3.2) and any breaking change to its API will now go through the formal OIP process.
6. ~~**BDFL veto window start date (P2.3)**~~ — **Resolved 2026-05-10:** *2026-05-09* (first public commit, GitHub-verified). Maximally constraining on the founder, certain today, independently verifiable. Sunset 2031-05-09 23:59 UTC, immutable.
7. ~~**OIP editor body composition during Bootstrap (P2.1)**~~ — **Resolved 2026-05-10:** *1 interim editor (founder), Seat 2 vacant until Phase 1 hire OR 2027-05-10 (hard deadline)*. Codified in `OIP-Process-001` §6.2.

Resolved during P2 review (2026-05-10, post-publication founder editorial review):

10. ~~**First non-`Meta` OIP to dogfood the formal vote**~~ — **Resolved 2026-05-10:** *`OIP-bounty-XXX`* (slug `bounty`, Process track; global number assigned at Last Call). Self-contained, sblocca grant narrative, ~1 settimana di drafting, primo Last Call reale. Subsequent order: `OIP-voting-XXX` (refines §5.2 bootstrap defaults), then `OIP-stark-snark-XXX` (after P3.2 cryptographer review unblocks). Drafting non avviato — gating su decisione del founder se partire ora o pre-allineare prima sui parametri chiave (severity tiers, payout ranges, eligibility filters).
11. ~~**OIP-Process-001 critical-security gap during Bootstrap**~~ — **Resolved 2026-05-10:** founder review surfaced the gap; addressed via OIP-Process-001 §6.5 amendment under bootstrap fiat. Bootstrap deadlock on `Critical` Standards Track OIPs is now bounded by 72h objection window + mandatory post-Bootstrap re-ratification.
12. ~~**OIP-Process-001 voting formula generational unfitness**~~ — **Resolved 2026-05-10:** founder review surfaced the saturation issue; addressed via §5.2 "Known limitations" amendment with soft 2028-05-10 deadline for the `voting`-slug Process OIP.
13. ~~**OIP-Process-001 author-identity privacy posture**~~ — **Resolved 2026-05-10:** founder review surfaced the GDPR/privacy-first inconsistency; addressed via `## Privacy Considerations` refinement + `oips/oip-template.md` HTML guidance.

Still open:

8. **Repo visibility long-term** — flipped to **PUBLIC** on 2026-05-09 because branch protection on the GitHub free plan requires it and AGPL-3.0 is consistent with public hosting. Confirm this remains the steady state, or signal a temporary embargo for any pre-disclosure phase.
9. **Branch-protection update for `oip-lint`** — `OIP-Process-001` §9 ¶2 mandates that branch protection on `main` add `oip-lint / oip-lint` as a required status check within 7 calendar days of the OIP transitioning to `Active`. Concrete action: re-run `scripts/bootstrap-github.sh` (or equivalent `gh` CLI invocation) before 2026-05-17 to extend the required-check list from 8 to 9. *(Founder-side action — requires GitHub admin token.)* — **NOTE 2026-05-19:** deadline 2026-05-17 superata; check da aggiungere comunque retroattivamente prima di mergiare `feat/kernel-mb11-userspace` (P10.1).
15. **Last Call closing actions for `OIP-Bounty-002` and `OIP-Serde-004` (window closes 2026-05-26)** — Both OIPs entered `Last Call` on 2026-05-12 under `OIP-Process-001` § 4. Under § 5.3 each transitions `Last Call → Active` automatically at the end of the 14-day window unless ≥ 30% weighted vote is reached earlier (in which case the editors close the window at that point) **or** a blocking good-faith objection is filed (in which case the OIP returns to `Review`). Concrete actions for the editor body on or before **2026-05-26**: (a) confirm no blocking objection has been filed on the linked GitHub Discussion thread; (b) merge a single PR per OIP transitioning the frontmatter `status:` from `Last Call` to `Active` and updating the `updated:` field to the close date; (c) for `OIP-Bounty-002` (Process track), no activation phase applies, the OIP is effectively `Final` at `Active`; (d) for `OIP-Serde-004` (Standards Track), the activation phase per § 7 is dormant until Phase 4+ mesh telemetry exists — the OIP remains in `Active` indefinitely; (e) append a row to `oip-editors-report-YYYY-Q2.md` recording the tally (or its absence) and the editorial decision. **No founder-side or hardware-side gate; pure editorial action.**
14. ~~**`OIP-bounty-002` drafting kickoff**~~ — **In progress 2026-05-10:** `Draft` filed at [`oips/oip-bounty-002.md`](oips/oip-bounty-002.md) (~31KB, 10 sezioni canoniche, lint green). Defaults applicati senza pre-allineamento ulteriore (founder ha confermato "procedi"): severity tiers riusati da `SECURITY.md` §4 (CVSS v4.0); payout ranges Critical €5K–€50K / High €1K–€10K / Medium €250–€2.5K / Low €50–€500; eligibility con 6-month contributor guard + esclusione editor body / Stichting board / commit-access su `main`; disclosure timeline ancorato a `SECURITY.md` §3; payment mechanics con opzioni crypto privacy-preserving (Monero, BTC LN); dispute resolution a 3 livelli che termina in public arbitration; **non-monetary mode** durante Bootstrap con commitment retroattivo entro 24 mesi dall'Activation Date. Index aggiornato in `oips/README.md`; `SECURITY.md` §7 aggiornato per puntare al Draft. Prossimi passi: editorial review by founder; transition to `Review` quando il founder è pronto; questo OIP è il **dogfood test** del flusso §5 di `OIP-Process-001`.
16. **MB13 fix opzione (a/b/c) per il triple-fault smoke** (vedi P6.MB13.b) — preferenza tecnica: opzione (a) ET_DYN/PIE kernel. Sblocco potenziale: capire se `bootloader_api` 0.11 onora davvero `dynamic_range_start` su `ET_DYN` x86_64-unknown-none — la docstring in `kernel-runner/src/main.rs:27-40` dice di no per `ET_EXEC` ma non è stato testato sperimentalmente per `ET_DYN`. **Azione richiesta:** spike di 2-4 ore per build ET_DYN sperimentale prima di committare a una delle tre opzioni.
17. **Quando rilasciare v0.2.1 vs aspettare v0.3.0-alpha.1** (vedi P10.2) — decisione del founder sul cadence del tag intermedio.

These decisions do not block strategic planning, only execution order.

---

# P0 closure summary (2026-05-09)

| What | Status / pointer |
|---|---|
| Repo URL | https://github.com/CySalazar/omni |
| Visibility | Public (AGPL-3.0) |
| Default branch | `main` |
| Branch protection | `enforce_admins=true`, `required_signatures=true`, `linear_history=true`, `allow_force_pushes=false`, 1 reviewer, 8 required status checks |
| Commits on `main` | `61426d5` → `15419cb` → `ebf9539` → `101ff79` (all `cySalazar <cySalazar@cySalazar.com>`, SSH-signed, GitHub-verified) |
| Workflows live | ci, audit, sbom, reproducible-build, dco, codeql, labeler |
| Dependabot active | First 2 PRs already auto-opened (mockall, cryptography group) |
| Label taxonomy | 32 labels (`area:*`, `priority:*`, `kind:*`, special) |
| Vulnerability alerts | Enabled |
| Secret scanning + push protection | Enabled |
| SSH signing key on GitHub | id 938835 (`~/.ssh/id_ed25519.pub`) |
| Project identity | `cySalazar <cySalazar@cySalazar.com>` (Matteo's real `matteo.sala@samacyber.io` removed from the GitHub account on 2026-05-09) |
| Bootstrap scripts | `scripts/bootstrap-local.sh` (idempotent), `scripts/bootstrap-github.sh` (idempotent) |
| Completion report | [`docs/audits/p0-completion-report.md`](docs/audits/p0-completion-report.md) (moved 2026-05-10 from repo root for hygiene) |
| Tooling docs | [`docs/11-tooling-and-ci.md`](docs/11-tooling-and-ci.md) |

---

# Phase 1 closure roadmap — executive sequence (post v0.2.0)

Sintesi ordinata dei prossimi sprint per chiudere **Phase 1 — Microkernel POC** della roadmap. Da leggere top-down; ogni step è un sotto-task elencato nelle sezioni precedenti.

| # | Sprint | Tasks chiave | Bloccato da | Effort | Output atteso |
|---|---|---|---|---|---|
| 1 | **Merge MB10/MB11/MB12 → main** | P10.1 | — | 0.5-1d | `main` HEAD include MB12 + Step 7; tag intermedio opzionale (P10.2) |
| 2 | **MB13 — `omni-capability` reale** | P6.MB13.a/b/c/d/e + ADR-0006 | Sprint 1 | 2-3d | `Ed25519CapabilityProvider` attivo + smoke `mb12-userprobe` verde + 432+ tests |
| 3 | **CI smoke MB11/MB12 automatico** | P10.4 | MB13.b | 1d | Job `qemu-boot-smoke` bloccante anche su `[user]`/`[mb12]` lines |
| 4 | **P7.3 docs `OMNI-PROTO-v0.2`** | P7.3 | — (parallelizzabile da subito) | 0.5d | Handshake spec aggiornato; OIP-Serde-004 verso `Final` |
| 5 | **P3.3 OIP STARK vs SNARK** | OIP `oip-crypto-002` Draft → Review | P3.2 cryptographer (P4.2 funding) | 1-2 sett | Decisione formale sulla strategia ZK |
| 6 | **MB14 — MP/AP enable + TLB shootdown** | nuovo (post-MB13) | MB13 | 5-10d | Multi-core operativo; ADR-0007 |
| 7 | **P6.7 — Userspace driver model** | NVMe + Net + TEE drivers | MB13 + MB14 + 3 nuovi OIP | 6-12 mesi | Phase 1 deliverable "Drivers in user space" |
| 8 | **P5.2/P5.3 — TDX + SEV-SNP backends reali** | + hardware acquisition | P4.2 funding | 2-4 sett | Phase 1 deliverable "TEE attestation" |
| 9 | **P6.8 — External kernel + capability audit** | engagement auditor | P4.2 funding + P6.7 done | 4-8 sett | Phase 1 closure deliverable |
| 10 | **Phase 1 → Phase 2 transition** | docs/06-roadmap.md update + OIP "Phase-2-Entry-XXX" | tutti gli sprint 1-9 | 1 sett | Phase 2 (AI Runtime) sblocca |

**Critical path techinical-only (esclude funding):** Sprint 1 → 2 → 3 → 6 → 7 → 9 → 10. Stimato realisticamente in **9-15 mesi** se single-implementer; **4-7 mesi** con il core team Phase 1 (2 senior Rust + 1 cryptographer) assunto.

**Critical path funding-dependent:** Sprint 5 + 8 + 9 dipendono da P4.2 (€350K runway). Senza funding, P3.2 cryptographer + TDX/SEV-SNP hardware + auditor pro-bono rimangono best-effort.

---

# Maintenance policy for this document

- This file is updated **after every completed task**.
- Status icons must reflect reality. Do not mark `[x]` until acceptance criteria are all green.
- Adding a new task requires it to slot into the existing tier structure or justify a new tier.
- Removing or downgrading a task requires either (a) the work is genuinely done, or (b) an OIP that supersedes the requirement.
- Cross-references between this document and `/docs/06-roadmap.md` must stay in sync; when in conflict, the roadmap is authoritative for *what*, this file is authoritative for *how*.
- Cross-references con [`progress-omni.md`](progress-omni.md) (snapshot stato) + [`CHANGELOG.md`](CHANGELOG.md) (per-release) devono restare coerenti; questo file è autoritativo per *what's next*, gli altri due per *what already happened*.
- **Allineamento DOE framework:** la struttura P0-P10 corrisponde al pattern `TASK-NNN` di `doe-framework/L2-orchestration/02-task-decomposition.md`. Ogni sotto-task ha: ID, Status, Priority, Effort, Dependencies, Deliverables, Acceptance criteria. Le decisioni architetturali significative producono un ADR in `docs/adr/` (template `doe-framework/templates/adr-template.md`).
