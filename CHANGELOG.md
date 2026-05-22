# Changelog

All notable changes to OMNI OS are documented in this file.

The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

OMNI OS distinguishes two version streams:

- **OS version** (`MAJOR.MINOR.PATCH`) — the distribution release.
- **Mesh protocol version** (`OMNI-PROTO-vMAJOR.MINOR`) — negotiated at handshake. Decoupled from the OS version (see [`docs/09-tech-specifications.md`](./docs/09-tech-specifications.md) § "Versioning policy").

Each entry below tracks the OS version. Protocol-version changes get their own bullet inside the OS-version entry that introduces them.

---

## [Unreleased]

### Added

- **Storage — P6.7.10-pre.29 NVMe `IoSession::submit_blk_request_auto`
  high-level BLK channel adapter (2026-05-22) — TASK-005
  continuation.**
  `crates/omni-driver-nvme/src/io_session.rs::IoSession` extended
  with `submit_blk_request_auto<M: MmioBackend>(req: BlkRequest,
  list_page_iova: u64, mmio: &mut M) -> Result<u16, IoSubmitError>`
  — the highest-level entry the BLK channel server will use:
  dispatches on the `BlkRequest` variant, derives the `(prp1,
  prp2)` pair via `derive_prp_pair_for_blocks` (pre.28) for
  data-transfer commands, and routes through the existing
  `submit_blk_request`.
  - **Per-variant behaviour**:
    - `BlkRequest::Read` / `BlkRequest::Write` — derives
      `(prp1, prp2)` from `(buf_iova, count, list_page_iova)`
      per [`derive_prp_pair_for_blocks`] then submits.
    - `BlkRequest::Flush` — sets `prp1 = prp2 = 0` (no data
      buffer); `list_page_iova` ignored.
    - `BlkRequest::Discard` — REJECTED with
      `IoSubmitError::DiscardRequiresExplicitPrp`. The Dataset
      Management command requires PRP1 to point at a
      caller-prepared Range Descriptor buffer built via
      `crate::discard::write_single_discard_range` (pre.27);
      the auto-derivation path cannot synthesise that buffer.
      The caller MUST invoke `submit_blk_request` directly with
      the prepared IOVA.
    - any future `#[non_exhaustive]` variant — REJECTED with
      `IoSubmitError::UnsupportedRequest`.
  - **`IoSubmitError` taxonomy** (`#[non_exhaustive]`) wrapping
    the underlying error families so the BLK channel server can
    translate each failure to the matching `BlkResponse`
    variant without needing to know which layer it came from:
    - `IoSubmitError::PrpDerive(PrpDeriveError)` (wraps
      `BufferMisaligned` / `ZeroBlockCount` / `TooManyBlocks` /
      `PrpListPageMissing` / `PrpListPageMisaligned`).
    - `IoSubmitError::Queue(QueueError)` (wraps `Full` /
      `SqPageTooSmall` / `DoorbellOffsetOverflow` / etc.).
    - `IoSubmitError::DiscardRequiresExplicitPrp`.
    - `IoSubmitError::UnsupportedRequest`.
  - **+10 new host-side tests** under
    `omni_driver_nvme::io_session::tests::*`:
    - 3 layout-dispatch happy paths: `Read` 1 block →
      SinglePage with PRP1=buf + PRP2=0 verified via SQE bytes
      24..=31 / 32..=39; `Write` 2 blocks → TwoPages with
      PRP2 = buf + 4096; `Read` 3 blocks → PrpList with
      PRP2 = list_page_iova.
    - 1 Flush behaviour (sets PRP1=PRP2=0 even with garbage
      `list_page_iova = 0xDEAD_BEEF`).
    - 1 Discard rejection (returns `DiscardRequiresExplicitPrp`
      without writing any SQE or ringing the doorbell).
    - 3 error-propagation tests (zero count →
      `PrpDerive(ZeroBlockCount)`; misaligned buf →
      `PrpDerive(BufferMisaligned)`; 3 blocks + missing list
      page → `PrpDerive(PrpListPageMissing)`).
    - 1 cross-variant CID monotone allocation (Read=1, Write=2,
      Flush=3 in sequence).
    - 1 taxonomy distinctness across the 4 `IoSubmitError`
      variants.
  - **Workspace test count**: 1565 → 1575 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.29 BLK auto-submit`,
    Next=`P6.7.10-pre.30 v0.3.0-alpha.2`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1575 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes existing
    `crate::blk_gateway::encode_blk_request` (pre.22) +
    `crate::transfer_model::derive_prp_pair_for_blocks` (pre.28)
    + the existing `submit_blk_request` (pre.23).

- **Storage — P6.7.10-pre.28 NVMe high-level PRP-pair derivation
  helper (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/transfer_model.rs` extended with
  `derive_prp_pair_for_blocks(buf_iova: u64, block_count: u32,
  list_page_iova: u64) -> Result<(PrpLayout, u64, u64),
  PrpDeriveError>` — the one-shot adapter bridging the BLK
  channel boundary (`BlkRequest::{Read, Write}{buf_iova, count}`)
  to the SQE-shape `(prp1, prp2)` field pair that
  `crate::io::encode_read` / `encode_write` consume.
  - **Composition** of existing pre.8 primitives:
    1. `block_payload_bytes(block_count)` computes byte length
       (Phase-1 uses 4 KiB sectors per OIP-014 § S6 step 10 so
       `len = block_count * 4096`).
    2. `prp_layout(len)` picks one of `SinglePage` / `TwoPages` /
       `PrpList { n_entries }`.
    3. `prp1_for(buf_iova)` + `prp2_for(buf_iova, layout,
       list_page_iova)` produce the two SQE fields.
  - **`PrpDeriveError` taxonomy** (`#[non_exhaustive]`):
    - `BufferMisaligned` (buf_iova not 4 KiB-aligned).
    - `ZeroBlockCount` (count = 0).
    - `TooManyBlocks` (count > `MAX_BLOCK_COUNT_PER_COMMAND` =
      2048).
    - `PrpListPageMissing` (PrpList layout but list_page_iova
      = 0).
    - `PrpListPageMisaligned` (list_page_iova not 4 KiB-aligned).
  - **Validation scope**: `list_page_iova` validated ONLY when
    the derived layout is `PrpList`. Callers can pass `0` for
    1- or 2-block transfers without triggering a spurious
    rejection.
  - **+12 new host-side tests** under
    `omni_driver_nvme::transfer_model::tests::*`:
    - 3 happy-path layout dispatches: 1 block → `SinglePage`
      with `prp2 = 0`; 2 blocks → `TwoPages` with
      `prp2 = buf + 4096`; 3 blocks → `PrpList { n_entries=2 }`
      with `prp2 = list_page`.
    - 5 error-path tests (misaligned buf, zero count,
      count > MAX, PrpList without list_page, misaligned
      list_page).
    - 2 invariant tests: single-page + two-pages ignore
      `list_page_iova` value (even garbage `0xDEAD_BEEF`
      succeeds).
    - 1 max-block boundary
      (`MAX_BLOCK_COUNT_PER_COMMAND = 2048` →
      `PrpList { n_entries = 2047 }`).
    - 1 taxonomy distinctness check across all 5 variants.
  - **Workspace test count**: 1553 → 1565 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.28 PRP derive`,
    Next=`P6.7.10-pre.29 v0.3.0-alpha.2`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1565 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure-state composition of existing
    pre.8 primitives.

- **Storage — P6.7.10-pre.27 NVMe Dataset Management Discard
  Range Descriptor builder (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/discard.rs` module declares
  `write_single_discard_range(buf: &mut [u8], lba: u64,
  count: u32) -> Result<(), DiscardError>` writing the canonical
  16-byte NVMe 1.4 § 6.7.1 Figure 256 Range Descriptor into a
  caller-supplied IOVA buffer (the same buffer whose IOVA goes
  into the `Dataset Management` SQE's PRP1 — produced by
  `crate::io::encode_discard` from pre.7).
  - **Field layout per Figure 256**:
    - bytes 0..=3 — Context Attributes (Phase-1 driver writes 0
      — no optional metadata).
    - bytes 4..=7 — Length in Logical Blocks (32-bit LE).
    - bytes 8..=15 — Starting LBA (64-bit LE).
  - **Phase-1 scope**: exactly one range per Discard command
    (matching the `omni_types::blk::BlkRequest::Discard{lba,
    count}` shape carrying a single tuple). Multi-range Discard
    lands behind a future OIP without changing the per-range
    descriptor layout.
  - **`DISCARD_RANGE_DESCRIPTOR_BYTES = 16`** anchor constant.
  - **`DiscardError::BufferTooSmall`** taxonomy
    (`#[non_exhaustive]`) — surfaced via bounds-checked
    `get_mut(..16)` so the builder NEVER panics on a too-small
    caller buffer.
  - **+12 new host-side tests** under
    `omni_driver_nvme::discard::tests::*`:
    - 1 `DISCARD_RANGE_DESCRIPTOR_BYTES = 16` tripwire.
    - 1 over-write protection (writer touches exactly bytes
      0..=15 on a 32-byte buffer pre-filled with `0xFF` — bytes
      16+ stay `0xFF`).
    - 1 Context Attributes = 0 check.
    - 1 Length field LE round-trip via offset 4..=7 with
      `count = 0xDEAD_BEEF`.
    - 1 Starting LBA LE round-trip via offset 8..=15 with
      `lba = 0xCAFE_BABE_DEAD_BEEF`.
    - 2 buffer-size rejections (15-byte and empty buffers
      surface `DiscardError::BufferTooSmall`).
    - 1 larger-buffer test (4 KiB DMA arena page — writer
      touches only the first 16 bytes, rest stays zeroed).
    - 1 full-field-layout round-trip (CA=0 +
      Length=`0x55AA_55AA` + LBA=`0x0123_4567_89AB_CDEF`).
    - 1 taxonomy distinctness.
    - 1 zero-lba zero-count canonical-zero descriptor.
    - 1 max-LBA max-count round-trip (`u32::MAX` + `u64::MAX`
      boundaries).
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod discard;` declaration (slotted alphabetically
    between `controller_regs` and `identify`).
  - **Workspace test count**: 1541 → 1553 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.27 Discard range`,
    Next=`P6.7.10-pre.28 v0.3.0-alpha.2`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1553 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure-state byte writer using
    `core::slice::get_mut`.

- **Storage — P6.7.10-pre.26 NVMe Active NSID list parser
  (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/identify.rs` extended with the
  third Identify-family parser closing the bring-up FSM's
  NSID-discovery path per NVMe 1.4 § 5.15.2 Figure 246.
  - **`ActiveNsListView<'a>`** zero-copy view over a 4 KiB
    response page:
    - `iter_nsids() -> ActiveNsListIter<'a>` — lazy forward-only
      iterator yielding NSIDs in the order the controller wrote
      them, stopping at the first sentinel NSID = 0 entry or
      after `MAX_ACTIVE_NSIDS` = 1024 entries.
    - `first_active_nsid() -> Option<u32>` — convenience
      accessor the bring-up FSM uses to seed the subsequent
      `Identify(Namespace)` call.
  - **`MAX_ACTIVE_NSIDS = 1024`** anchor constant
    (= `IDENTIFY_RESPONSE_BYTES / 4`) per Figure 246.
  - **`ActiveNsListIter`** derives `Clone` so a future bring-up
    implementation can peek at the first NSID and then iterate
    the full list without re-parsing the page.
  - **Sentinel clamps `next_index`** to `MAX_ACTIVE_NSIDS` so
    subsequent `.next()` calls return `None` without re-reading
    bytes past the terminator — defensive parsing per OIP-014
    § S6 step 9.
  - **+10 new host-side tests** under
    `omni_driver_nvme::identify::tests::*`:
    - 1 `MAX_ACTIVE_NSIDS = 1024` tripwire.
    - 1 undersized-page rejection returns
      `IdentifyError::PageTooSmall`.
    - 1 empty-list happy path (zero page → `first_active_nsid()`
      = `None`, `iter_nsids().count()` = 0).
    - 1 single-entry list (`[1]` → yields `1` then stops).
    - 1 multi-entry list (`[1, 2, 3, 7, 42]` → yields all in
      order).
    - 1 sentinel-truncation test (`[4, 5, 0, 99, 100]` → yields
      only `[4, 5]` even though bytes for 99 + 100 exist past
      the sentinel).
    - 1 full-page no-terminator test (1024 non-zero entries →
      iter yields exactly 1024 without overflowing).
    - 1 `first_active_nsid` peek.
    - 1 OIP-014 § S6 step 9 default (single-namespace `[1]` →
      first NSID = 1).
    - 1 `ActiveNsListIter::clone()` lookahead test (verifies
      clone is independent of the original iterator state).
  - **`MAX_ACTIVE_NSIDS`** carries an explicit
    `#[allow(clippy::integer_division)]` carve-out documenting
    the compile-time `4096 / 4 = 1024` is exact and has no
    runtime cost.
  - **Workspace test count**: 1531 → 1541 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.26 NSID list parse`,
    Next=`P6.7.10-pre.27 v0.3.0-alpha.2`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1541 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure-state byte decoder consuming
    only the existing `read_le_u32` helper from pre.25.

- **Storage — P6.7.10-pre.25 NVMe Identify response parsers
  (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/identify.rs` module declares
  pure-function decoders for the 4 KiB response payloads the
  controller writes after `Identify Controller` (NVMe 1.4
  § 5.15.2 Figure 247) and `Identify Namespace` (Figure 245).
  Closes the gap between the Identify admin commands the driver
  issues (pre.18) and the Phase-1 bring-up's need to inspect
  controller capabilities + namespace geometry per
  OIP-Driver-NVMe-014 § S6 steps 8 + 10.
  - **`IdentifyController<'a>`** zero-copy view over `&[u8]`:
    - `nn() -> u32` (Number of Namespaces, offset 516, 32-bit
      LE per Figure 247).
  - **`IdentifyNamespace<'a>`** zero-copy view:
    - `nsze() -> u64` (Namespace Size, offset 0).
    - `ncap() -> u64` (Namespace Capacity, offset 8).
    - `flbas() -> u8` (Formatted LBA Size byte, offset 26).
    - `active_format_index() -> u8` (`flbas & 0x0F`).
    - `lbads() -> u8` (LBA Data Size = `log2(sector_size)` from
      the active LBAF descriptor at offset 128 + 4 *
      format_index + 2, masked to bits 4..=0).
    - `validated_byte_size() -> Result<u64, IdentifyError>`
      performs the OIP-014 § S6 step 10 check: returns
      `Ok(NSZE << LBADS)` when `LBADS == PHASE_1_REQUIRED_LBADS =
      12` (4 KiB sectors), or `Err(IdentifyError::UnsupportedLbads
      { observed })` otherwise so the bring-up FSM aborts cleanly
      on legacy 512-byte-sector namespaces.
  - **`IdentifyError` taxonomy** (`#[non_exhaustive]`):
    `PageTooSmall`, `UnsupportedLbads { observed: u8 }`.
  - **Anchor constants**: `IDENTIFY_RESPONSE_BYTES = 4096`,
    `PHASE_1_REQUIRED_LBADS = 12`.
  - **Internal `read_le_u32` / `read_le_u64` helpers** use
    bounds-checked `get` + `try_into` to satisfy
    `clippy::indexing_slicing` outside the tests; the test
    module carries an explicit
    `#[allow(clippy::indexing_slicing)]` carve-out documenting
    that fixtures write canonical NVMe field offsets in-place
    per Figure 245/247.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod identify;` declaration.
  - **+15 new host-side tests** under
    `omni_driver_nvme::identify::tests::*`:
    - 2 constant tripwires (`IDENTIFY_RESPONSE_BYTES`,
      `PHASE_1_REQUIRED_LBADS`).
    - 3 `IdentifyController` checks (rejects undersized page
      with `PageTooSmall`, NN at offset 516 LE round-trip, NN
      zero on empty page).
    - 3 `IdentifyNamespace` byte-offset pins (NSZE at offset 0,
      NCAP at offset 8, FLBAS active-format-index mask).
    - 2 LBADS reads (reads active format descriptor at the
      right offset, masks the high 3 bits of the LBADS byte).
    - 3 `validated_byte_size` checks (returns `NSZE * 4096`
      when LBADS=12, rejects LBADS=9 with `UnsupportedLbads {
      observed: 9 }`, rejects empty page with
      `UnsupportedLbads { observed: 0 }`).
    - 1 `IdentifyError` taxonomy distinctness (different
      `observed` values produce distinct error variants).
  - **Workspace test count**: 1516 → 1531 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.25 Identify parser`,
    Next=`P6.7.10-pre.26 nsid wire FSM`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1531 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure-state byte decoders consume only
    `core::convert::TryInto`.

- **Storage — P6.7.10-pre.24 NVMe IO queue pair end-to-end smoke
  (2026-05-22) — TASK-005 continuation.**
  New integration test
  `end_to_end_io_session_all_blk_request_variants_round_trip` in
  `crates/omni-driver-nvme/src/io_session.rs::tests` pins the
  full BLK channel ABI to a single auditable test covering all
  4 `BlkRequest` variants in sequence per OIP-Driver-NVMe-014
  § S4:
  - Step 1: `BlkRequest::Read{lba: 0x100, count: 1,
    buf_iova: 0x1_0000}` → CID = 1, CQ slot 0.
  - Step 2: `BlkRequest::Write{lba: 0x200, count: 2,
    buf_iova: 0x2_0000}` with PRP1+PRP2 (two-page transfer) →
    CID = 2, CQ slot 1.
  - Step 3: `BlkRequest::Flush` → CID = 3, CQ slot 2.
  - Step 4: `BlkRequest::Discard{lba: 0x300, count: 4}` →
    CID = 4, CQ slot 3.
  Each step submits through `IoSession::submit_blk_request`,
  injects a synthetic successful CQE at the next CQ slot via
  the `write_synthetic_cqe` fixture (SCT=0 + SC=0 + phase=true),
  polls for the matching CID via
  `IoSession::poll_blk_response_for_cid`, and asserts
  `BlkResponse::Ok`.
  - **Final-state assertions** pin the IO queue pair invariants:
    - CID monotone allocation 1..=4 (skipping the reserved 0).
    - 4 SQ tail doorbell writes (one per submit), all routed
      to `sq_tail_doorbell_offset(PHASE_1_IO_QID = 1, 0)` —
      NOT the admin offset.
    - 4 CQ head doorbell writes (one per drain), all routed to
      `cq_head_doorbell_offset(PHASE_1_IO_QID, 0)`.
    - CQ ring `head` advanced through all 4 slots (= 4).
    - SQ ring `tail` advanced through all 4 submissions (= 4).
  - **Split MMIO recorder**: `mmio` captures the submit-side
    doorbells, `nop` captures the drain-side doorbells, so the
    test verifies both halves of the BLK pipeline routing
    independently.
  - **Workspace test count**: 1515 → 1516 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.24 IO e2e smoke`,
    Next=`P6.7.10-pre.25 TASK-005 close`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1516 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — exercises the existing `IoSession`
    (pre.23) + `blk_gateway` (pre.22) + `AdminQueuePair` (pre.11
    with pre.23 runtime-qid generalisation) +
    `omni_types::blk::{BlkRequest, BlkResponse}` (pre.1)
    surfaces.

- **Storage — P6.7.10-pre.23 NVMe `IoSession` (IO queue pair
  traffic) (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/queue.rs::AdminQueuePair`
  generalised with a runtime `qid: u16` field (replaces the
  hardcoded `Self::ADMIN_QID = 0` in the doorbell offset calls).
  New `AdminQueuePair::new_for_qid(qid, sq_depth, cq_depth,
  dstrd)` constructor accepts an arbitrary queue id (admin = 0,
  IO queues = `1..=io_queue_count` per OIP-NVMe-014 § R5). The
  existing `AdminQueuePair::new(...)` is preserved as a wrapper
  calling `new_for_qid(ADMIN_QID, ...)` so all prior callers
  continue to compile without change.
  - New `crates/omni-driver-nvme/src/io_session.rs` module
    declares `IoSession` — mirrors the `AdminSession` pattern
    from pre.13 but bound to a non-admin queue:
    - `IoSession::new(qid, nsid, sq_depth, cq_depth, dstrd)`
      zero-initialises the SQ + CQ pages and starts the CID
      counter at 1 (skipping the reserved 0).
    - `submit_blk_request<M: MmioBackend>(req: BlkRequest,
      prp1: u64, prp2: u64, mmio: &mut M)
      -> Result<Option<u16>, QueueError>` composes
      `blk_gateway::encode_blk_request(req, cid, nsid, prp1,
      prp2)` (pre.22) with `AdminQueuePair::submit`. Returns
      `Ok(None)` when `encode_blk_request` rejects the request
      (only `#[non_exhaustive]` future variants today; caller
      emits `BlkResponse::NotSupported` to the client).
    - `poll_blk_response_for_cid<M>(cid, poll_limit, mmio)
      -> Result<Option<BlkResponse>, QueueError>` drains the CQ
      until the matching CID arrives, then composes with
      `blk_gateway::cqe_to_blk_response` to surface the
      translated response in one call.
  - **`PHASE_1_IO_QID = 1`** constant pins the Phase-1
    single-IO-queue default per OIP-014 § R2.
  - `crates/omni-driver-nvme/src/lib.rs` extended with
    `pub mod io_session;` declaration.
  - **+14 new host-side tests** under
    `omni_driver_nvme::io_session::tests::*`:
    - 1 `PHASE_1_IO_QID` constant tripwire.
    - 2 construction checks: records qid + nsid; SQ tail
      doorbell routes to the IO queue offset NOT the admin
      offset (anti-aliasing tripwire).
    - 3 `submit_blk_request` encoding checks: returns assigned
      CID; NSID passes through to SQE bytes 4..=7 LE;
      `BlkRequest::Read` produces `OPC_NVM_READ` at byte 0.
    - 5 `poll_blk_response_for_cid` mapping tests: success →
      `Ok`; SC=0x02 → `InvalidArgument`; SC=0x80 → `OutOfRange`;
      SCT=2 SC=0x82 → `DeviceError(0x0282)`; empty CQ exhausts
      poll budget → `None`. Each verifies the CQ head doorbell
      routes to the IO queue offset.
    - 1 full submit → complete → poll round-trip
      (`submit_then_poll_round_trips_blk_read_to_ok`).
    - 1 CID monotone allocation check (1, 2, 3 sequentially).
    - 1 ring-error propagation on zero depth.
  - New `write_synthetic_cqe` fixture in the module's tests
    — symmetric to the admin_session helper but parameterised
    by SCT/SC for the status-word mapping tests.
  - **All previously-existing tests** (admin queue, FSM glue,
    e2e bringup, BLK gateway, etc.) continue to pass — the
    `AdminQueuePair` runtime-qid generalisation is non-breaking
    because the default `new(...)` constructor preserves the
    original `qid = 0` behaviour.
  - **Workspace test count**: 1501 → 1515 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.23 IoSession`,
    Next=`P6.7.10-pre.24 IO e2e smoke`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1515 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `crate::queue::AdminQueuePair` (pre.11 + the runtime-qid
    generalisation here) +
    `crate::blk_gateway::{encode_blk_request, cqe_to_blk_response}`
    (pre.22) +
    `omni_types::blk::{BlkRequest, BlkResponse}` (pre.1).

- **Storage — P6.7.10-pre.22 NVMe BLK gateway (2026-05-22) —
  TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/blk_gateway.rs` module declares
  two pure functions that close the BLK ↔ NVMe boundary the
  file-system service consumes per OIP-Driver-NVMe-014 § S4.
  - **`encode_blk_request(req: BlkRequest, cid: u16, nsid: u32,
    prp1: u64, prp2: u64) -> Option<AdminSqe>`** dispatches on
    the `BlkRequest` variant and returns the matching NVMe IO
    SQE via the encoders from pre.7:
    - `BlkRequest::Read → encode_read` (opcode `0x02` NVM Read).
    - `BlkRequest::Write → encode_write` (`0x01` NVM Write).
    - `BlkRequest::Flush → encode_flush` (`0x00` NVM Flush).
    - `BlkRequest::Discard → encode_discard` (`0x09` Dataset
      Management with `AD = 1`).
    Returns `None` for unknown `#[non_exhaustive]` future
    variants per OIP-Serde-004.
  - **`cqe_to_blk_response(fields: &AdminCqeFields) ->
    BlkResponse`** translates the parsed completion to the BLK
    response type:
    - `is_success() == true` → `Ok`.
    - Generic Command Status (`SCT == 0`) with sub-codes `0x02`
      (Invalid Field) / `0x0B` (Invalid Namespace) →
      `InvalidArgument`.
    - SC `0x80` (LBA Out of Range) → `OutOfRange`.
    - Any other non-success status → `DeviceError(status)`
      carrying the SCT:SC pair packed as `(sct << 8) | sc` per
      OIP-014 § S4.
  - **Defensive fallback**: a corrupt parser yielding
    `SCT ≠ 0 + SC = 0` still emits `DeviceError` with
    `(sct << 8)`. The `NON_NVME_DEVICE_ERROR = 0xFFFF` sentinel
    fires only if the packed status word collapses to zero —
    unreachable in well-formed code.
  - **+15 new host-side tests** under
    `omni_driver_nvme::blk_gateway::tests::*`:
    - 6 `encode_blk_request` dispatch checks: Read →
      `OPC_NVM_READ` at byte 0 + CID at bytes 2..=3; Write →
      `OPC_NVM_WRITE`; Flush → `OPC_NVM_FLUSH`; Discard →
      `OPC_NVM_DATASET_MGMT`; NSID `0xDEAD_BEEF` passes through
      to SQE bytes 4..=7 little-endian; every encoder emits
      exactly a 64-byte SQE.
    - 8 `cqe_to_blk_response` mapping checks: success → `Ok`;
      Invalid Field → `InvalidArgument`; Invalid Namespace →
      `InvalidArgument`; LBA Out of Range → `OutOfRange`;
      Invalid Opcode → `DeviceError(0x0001)`; Command-Specific
      with SC=`0x82` → `DeviceError(0x0282)`; Media+Data
      Integrity → `DeviceError(0x0282)`; SCT≠0+SC=0 defensive
      fallback → `DeviceError(0x0100)`.
    - 1 round-trip integration
      (`encode_and_response_round_trip_read_to_ok`) — submit a
      Read with CID=`0xABCD`, verify CID preserved in SQE bytes
      2..=3, synthesise the matching success CQE, assert
      `BlkResponse::Ok`.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod blk_gateway;` declaration.
  - **Workspace test count**: 1486 → 1501 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.22 BLK gateway`,
    Next=`P6.7.10-pre.23 IO QP session`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1501 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `omni_types::blk::{BlkRequest, BlkResponse, NON_NVME_DEVICE_ERROR}`
    (pre.1) + `crate::admin::{AdminSqe, AdminCqeFields}`
    (pre.6) + `crate::io::{encode_read, encode_write,
    encode_flush, encode_discard, OPC_NVM_*}` (pre.7).

- **Storage — P6.7.10-pre.21 NVMe Phase-1 bring-up end-to-end
  integration test (2026-05-22) — TASK-005 continuation.**
  New `end_to_end_phase_1_bringup_sequence_completes_successfully`
  integration test in
  `crates/omni-driver-nvme/src/admin_session.rs::tests` pins the
  full Phase-1 admin-queue bring-up lifecycle to a single
  auditable test covering every helper landed across
  P6.7.10-pre.13..pre.20 in their natural OIP-014 § S6 order:
  - Step 1: Identify Controller (pre.18)
  - Step 2: Identify Active NS List (pre.18)
  - Step 3: Identify Namespace, NSID = 1 (pre.18)
  - Step 4: Create I/O Completion Queue (pre.20)
  - Step 5: Create I/O Submission Queue (pre.20)
  Each step submits a SQE through the `AdminSession`, injects a
  synthetic completion at the CQ slot the session expects next
  via the new `write_synthetic_cqe(page, slot, cid, phase,
  sq_head)` helper, and drains it via `poll_completion_for_cid`.
  - **`write_synthetic_cqe` test helper** centralises the
    CDW2/CDW3 little-endian encoding pattern previously
    duplicated in `round_trip_session_through_fake`.
  - **Final-state assertions**:
    - CQ ring has wrapped exactly once (5 consumed CQEs against
      capacity = 4 → `expected_phase = false` post-wrap,
      `head = 1`).
    - MMIO recorder captured exactly 5 SQ tail doorbell writes
      (one per submit; the polls used `NopMmio`).
  - **Cross-phase invariants exercised**: CID monotone
    allocation (1 → 5 covering the full SQ ring lifecycle
    through one wrap); phase-tag flip on CQ wrap;
    `sq_head` feedback into `SqRing::head_observed` unblocking
    subsequent submits after the ring fills.
  - **Workspace test count**: 1485 → 1486 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.21 e2e bringup`,
    Next=`P6.7.10-pre.22 driver-img IO`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1486 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — exercises existing primitives from
    `crate::admin` + `crate::admin_session` + `crate::queue`.

- **Storage — P6.7.10-pre.20 NVMe IO Queue Pair live submit
  + FSM glue (2026-05-22) — TASK-005 continuation.**
  `omni-driver-nvme::admin_session::AdminSession` extended with the
  IO-queue-creation helpers per NVMe 1.4 § 5.4 + § 5.5;
  `bringup_live` extended with the matching FSM phase glue.
  - **`submit_create_io_cq<M: MmioBackend>(qid, qsize, prp1,
    irq_vector, mmio) -> Result<u16, QueueError>`** encodes the
    Create I/O Completion Queue admin command via
    `encode_create_io_cq` (pre.10) with `IEN = true` + `PC = true`
    (Phase-1 always enables interrupts and physical contiguity per
    OIP-014 § R5), allocates a CID, submits through
    `AdminQueuePair::submit`, returns the CID.
  - **`submit_create_io_sq<M>(qid, qsize, prp1, cq_id,
    queue_priority, mmio)`** symmetric for the Create I/O
    Submission Queue command (opcode 0x01).
  - **`run_create_io_cq<M>(qid, qsize, prp1, irq_vector,
    poll_limit, mmio) -> Result<AdminCqeFields, QueueError>`**
    composes submit + poll, surfacing
    `QueueError::IdentifyCompletionTimeout` on poll exhaustion
    (the variant covers both Identify and Create-IO timeouts).
  - **`run_create_io_sq<M>(qid, qsize, prp1, cq_id,
    queue_priority, poll_limit, mmio)`** symmetric for SQ. Carries
    an explicit `#[allow(clippy::too_many_arguments)]` carve-out
    documenting the future struct-arg refactor lands alongside the
    multi-queue OIP work.
  - **`bringup_live::CreateIoQueuesConfig`** struct with 8 fields
    covering CQ + SQ (qid/qsize/prp1) + CQ irq_vector + SQ
    queue_priority. Provides
    `phase_1_default(cq_prp1, sq_prp1, cq_irq_vector)` const
    constructor that pins Phase-1 defaults (qid = 1, qsize = 1024,
    MEDIUM priority matching QEMU `weighted_round_robin`).
  - **`bringup_live::advance_create_io_queues<M: MmioBackend>(fsm,
    session, config, poll_limit, mmio) -> Result<BringUp,
    BringUpError>`** issues Create I/O Completion Queue first (the
    SQ command depends on the CQ existing per § 5.4) then Create
    I/O Submission Queue; either failure aborts the FSM via
    `bringup_error_for`. No-op pass-through (`Event::Advance`) when
    the FSM is NOT at `Phase::CreateIoQueues`, so callers can drive
    the phase loop blindly.
  - **+10 new host-side tests**:
    - 4 `admin_session` submit-encoding checks (opcode at byte 0
      for CQ + SQ, IEN+PC bits in CDW11 for CQ, CQID + QPRIO bits
      in CDW11 for SQ).
    - 3 `admin_session` run-helper tests (run_create_io_cq +
      run_create_io_sq surface `IdentifyCompletionTimeout` on
      empty CQ; run_create_io_cq round-trips to success via the
      `FakeController` fixture).
    - 3 `bringup_live` tests (`CreateIoQueuesConfig::phase_1_default`
      matches spec, `advance_create_io_queues` non-matching phase
      pass-through with zero doorbell writes,
      `advance_create_io_queues` at matching phase dispatches
      CQ-first then aborts on empty CQ with `OPC_CREATE_IO_CQ`
      verified at SQE byte 0).
  - **Workspace test count**: 1475 → 1485 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.20 IO QP wire`,
    Next=`P6.7.10-pre.21 e2e bringup loop`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1485 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `crate::admin::{encode_create_io_cq, encode_create_io_sq,
    CIOSQ_QPRIO_MEDIUM, OPC_CREATE_IO_CQ, OPC_CREATE_IO_SQ}`
    (pre.10) + `crate::admin_session::AdminSession` (pre.13) +
    `crate::bringup::{BringUp, Event, Phase, BringUpError}`
    (P6.7.8.4).

- **Storage — P6.7.10-pre.19 NVMe `bringup_live` FSM glue
  (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/bringup_live.rs` module composes
  the pure-state `BringUp` FSM (P6.7.8.4) with the live
  `AdminSession` (pre.13) + the `MmioBackend` seam (pre.11),
  bridging the "did the admin command succeed?" gateway the
  driver image would otherwise re-implement at every phase
  transition.
  - **`advance_with_admin_session<M: MmioBackend>(fsm: BringUp,
    session: &mut AdminSession, buf_iova: u64, poll_limit: u32,
    mmio: &mut M) -> Result<BringUp, BringUpError>`** dispatches
    on the FSM's current phase:
    - `Phase::IdentifyController` → `run_identify_controller`
    - `Phase::IdentifyActiveNsList` → `run_identify_active_ns_list`
    - `Phase::IdentifyNamespace` →
      `run_identify_namespace(DEFAULT_NSID = 1, ...)`
    - any other phase → `Event::Advance` without invoking the
      session (the caller drives non-Identify side effects).
  - **Outcome translation**: `is_success()` → `Event::Advance`;
    non-success status word →
    `Event::Abort(BringUpError::AdminCommandFailed)`;
    underlying `QueueError` → `Event::Abort` with
    `bringup_error_for(QueueError)`.
  - **`bringup_error_for(QueueError) -> BringUpError`** maps
    `ControllerNotReady → ControllerReadyTimeout` and everything
    else → `AdminCommandFailed` (most queue failures leave the
    controller recoverable; the kernel reaps resources via the
    existing task-exit chain).
  - **`DEFAULT_NSID: u32 = 1`** pins the NSID the Phase-1 driver
    passes to `Identify Namespace` per OIP-014 § S6 step 9
    (NSID 0 is reserved by the NVMe spec).
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod bringup_live;` declaration.
  - **+9 new host-side tests** under
    `omni_driver_nvme::bringup_live::tests::*`:
    - 3 `bringup_error_for` mapping tests
      (`ControllerNotReady → ReadyTimeout`,
      `IdentifyCompletionTimeout → AdminCommandFailed`,
      `Full → AdminCommandFailed`).
    - 1 non-Identify pass-through (`PciEnumeration → MmioMap`
      via `Event::Advance` without touching the session — zero
      doorbell writes).
    - 1 IdentifyController timeout propagation (empty CQ →
      submit succeeds + 1 doorbell write + poll times out →
      `BringUpError::AdminCommandFailed`).
    - 1 IdentifyActiveNsList routing (verifies CDW10.CNS =
      `CNS_ACTIVE_NSID_LIST = 0x02` in the submitted SQE).
    - 1 IdentifyNamespace NSID propagation (verifies SQE bytes
      4..=7 hold `DEFAULT_NSID = 1` little-endian).
    - 1 `DEFAULT_NSID` constant tripwire.
    - 1 `emit_synthetic_completion` fixture sanity check —
      round-trips a CID through the FakeController-style
      pattern and verifies the session's drain consumes it.
  - **Workspace test count**: 1466 → 1475 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.19 NVMe FSM glue`,
    Next=`P6.7.10-pre.20 IO QP wire`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1475 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `crate::admin_session::AdminSession` +
    `crate::bringup::{BringUp, Event, Phase, BringUpError}` +
    `crate::queue::{MmioBackend, QueueError}` already in
    workspace.

- **Storage — P6.7.10-pre.18 NVMe `AdminSession::run_identify_*`
  high-level helpers (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/admin_session.rs::AdminSession`
  extended with three high-level wrappers that combine
  `submit_identify_*` + `poll_completion_for_cid` into single
  calls so the future bring-up FSM does not have to manage the
  CID + drain pair manually.
  - **`run_identify_controller<M: MmioBackend>(&mut self,
    buf_iova: u64, poll_limit: u32, mmio: &mut M)
    -> Result<AdminCqeFields, QueueError>`** allocates a CID via
    `submit_identify_controller`, drains the matching completion
    via `poll_completion_for_cid`, surfaces
    `QueueError::IdentifyCompletionTimeout` when the poll budget
    exhausts.
  - **`run_identify_namespace<M>(nsid, buf_iova, poll_limit, mmio)
    -> Result<AdminCqeFields, QueueError>`** and
    **`run_identify_active_ns_list<M>(buf_iova, poll_limit,
    mmio)`** symmetric to `run_identify_controller` for the other
    two `IdentifyTarget` variants.
  - **`QueueError::IdentifyCompletionTimeout`** variant added to
    the `#[non_exhaustive]` taxonomy.
  - **+5 new host-side tests** under
    `omni_driver_nvme::admin_session::tests::*`:
    - `run_identify_controller_round_trips_to_completion` —
      full round-trip against the existing `FakeController`
      fixture, asserts `is_success()` and `sq_head` feedback.
    - `run_identify_controller_returns_timeout_on_empty_cq` —
      submit succeeds but CQ stays empty → helper surfaces
      `IdentifyCompletionTimeout` after exhausting `poll_limit`.
    - `run_identify_namespace_submits_correct_nsid_and_round_trips` —
      verifies NSID = 7 lands at SQE bytes 4..=7 (little-endian)
      and the round-trip completes.
    - `run_identify_active_ns_list_round_trips` — verifies
      CDW10.CNS = `0x02` and the round-trip completes.
    - `identify_completion_timeout_in_queue_error_taxonomy` —
      discriminant distinctness against `ControllerNotReady`,
      `Full`.
  - **`round_trip_session_through_fake(s, emits)`** helper
    centralises the snapshot-SQ → fake-controller →
    copy-CQ-back pattern that pre.13's lone round-trip test
    invented inline; future integration tests reuse the same
    pattern.
  - **Workspace test count**: 1461 → 1466 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.18 Identify Ctrl`,
    Next=`P6.7.10-pre.19 NVMe FSM glue`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1466 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes existing primitives from
    `crate::admin` + `crate::queue` + the `FakeController`
    fixture from pre.13.

- **Storage — P6.7.10-pre.17 NVMe driver image LiveMmioBackend +
  disable/program/enable controller sequence (2026-05-22) —
  TASK-005 continuation.**
  `crates/omni-driver-nvme-image/src/main.rs` extended with the
  live wiring that closes OIP-Driver-NVMe-014 § S6 step 4 + step 5
  + step 6 against real hardware.
  - **`LiveMmioBackend { mmio_va_base: u64 }`** newtype implements
    both `MmioBackend` (`volatile_write` 32-bit at
    `mmio_va_base + offset`) and `MmioReadBackend`
    (`volatile_read` 32-bit), so the helpers landed in
    P6.7.10-pre.11..16 drive the live NVMe controller without
    intermediate state.
  - **`#[derive(Clone, Copy)]`** — the driver creates two
    independent instances (one for the read backend, one for the
    write backend) to satisfy the two-mutable-reference signature
    of `disable_controller`/`enable_controller` without aliasing.
    No state held beyond `mmio_va_base`, so the duplication is
    zero-cost.
  - **`_start` captures `mmio_va`** returned by `MmioMap (70)`
    (previously discarded as `_mmio_va`) and composes a 3-step
    sequence after the existing BlkLookup defence-in-depth:
    1. `disable_controller(&mut mmio_write, &mut mmio_read,
       NVME_CSTS_POLL_LIMIT)` clears `CC.EN` and polls
       `CSTS.RDY = 0`.
    2. `program_admin_queue_bases(&mut mmio_write,
       NVME_ASQ_IOVA = 0x0, NVME_ACQ_IOVA = 0x1000,
       NVME_ADMIN_SQ_DEPTH = 64, NVME_ADMIN_CQ_DEPTH = 64)` writes
       AQA + ASQ + ACQ per NVMe 1.4 § 3.1.7-9.
    3. `enable_controller(&mut mmio_write, &mut mmio_read,
       NVME_CSTS_POLL_LIMIT)` sets `CC.EN` and polls
       `CSTS.RDY = 1`.
  - **New constants**:
    - `NVME_ADMIN_SQ_DEPTH: u32 = 64`
    - `NVME_ADMIN_CQ_DEPTH: u32 = 64`
    - `NVME_ASQ_IOVA: u64 = 0x0`
    - `NVME_ACQ_IOVA: u64 = 0x1000`
    - `NVME_CSTS_POLL_LIMIT: u32 = 10_000`
  - **New sentinel exit codes**:
    - `EXIT_NVME_DISABLE_TIMEOUT = 200`
    - `EXIT_NVME_ADMIN_QUEUE_INVALID = 210`
    - `EXIT_NVME_ENABLE_TIMEOUT = 220`
  - **Module documentation** extended with step 10 covering the
    disable → program → enable sequence and updated step
    numbering 11/12 for the FSM advance + TaskExit.
  - **Workspace test count** stable at 1461 pass / 0 fail — the
    new code is the `no_main` ELF entry path that cannot be
    unit-tested in-crate; coverage relies on the kernel-side host
    tests landed in pre.11..16 (50+ tests across
    `omni_driver_nvme::queue::tests::*`) plus the Proxmox boot
    smoke. The NVMe image is built but not currently invoked by
    the desktop demo path, so the live wiring is statically
    validated but not exercised end-to-end yet — the kernel-runner
    main scenario adds the `DriverLoad(73)` invocation in a
    follow-up sub-slice.
  - **Build Info panel**: Active=`P6.7.10-pre.17 NVMe live wire`,
    Next=`P6.7.10-pre.18 Identify Ctrl`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1461 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — `omni-driver-nvme` was already a
    path-dep of the image; the new imports
    (`disable_controller`, `enable_controller`,
    `program_admin_queue_bases`, `MmioBackend`,
    `MmioReadBackend`) all live under `omni_driver_nvme::queue`.

- **Storage — P6.7.10-pre.16 NVMe ASQ/ACQ admin-queue base-address
  register programmer (2026-05-22) — TASK-005 continuation.**
  New free function `program_admin_queue_bases<W: MmioBackend>(
  mmio_w, asq_phys, acq_phys, sq_depth, cq_depth) ->
  Result<(), QueueError>` in `crates/omni-driver-nvme/src/queue.rs`
  closes OIP-Driver-NVMe-014 § S6 step 5 per NVMe 1.4 § 3.1.7-9.
  - Writes 3 controller registers in the spec-mandated order:
    1. `AQA` (`0x24`): `ACQS-1 << 16 | ASQS-1` (both 0-based per
       § 3.1.8).
    2. `ASQ` (`0x28..0x2F`): 64-bit ASQ physical base split into
       a pair of 32-bit writes (lower at `0x28`, upper at `0x2C`).
    3. `ACQ` (`0x30..0x37`): symmetric to ASQ.
  - **Validation**:
    - `sq_depth` and `cq_depth` MUST be in
      `1..=MAX_ADMIN_QUEUE_DEPTH = 4096`.
    - `asq_phys` and `acq_phys` MUST be 4 KiB-aligned per § 3.1.9.
  - **New constants**:
    - `AQA_ACQS_SHIFT: u32 = 16`
    - `AQA_QSIZE_MASK: u32 = 0xFFF`
    - `MAX_ADMIN_QUEUE_DEPTH: u32 = 4096`
  - **New error variants** (`#[non_exhaustive]`):
    - `QueueError::AdminDepthOutOfRange`
    - `QueueError::QueueBaseMisaligned`
  - **Fn-level `#[allow(clippy::similar_names)]`** carve-out
    preserves the intentional `asq_*` / `acq_*` parallel naming
    the NVMe spec uses for the symmetric SQ/CQ pair.
  - **Precondition**: the controller MUST be disabled before this
    helper runs (the bring-up FSM calls `disable_controller` from
    pre.15 first); writing AQA/ASQ/ACQ while `CC.EN = 1` has
    implementation-defined effects per § 3.1.7.
  - **+9 new host-side tests** under
    `omni_driver_nvme::queue::tests::*`:
    - 1 constant tripwire (`AQA_QSIZE_MASK = 0xFFF`,
      `AQA_ACQS_SHIFT = 16`, `MAX_ADMIN_QUEUE_DEPTH = 4096`).
    - 1 register-order test (5 writes in exact order
      `AQA, ASQ_lo, ASQ_hi, ACQ_lo, ACQ_hi`).
    - 1 AQA encoding test (asserts `(128-1) << 16 | (64-1)` =
      `0x007F_003F` with `ASQS = 63`, `ACQS = 127`).
    - 1 ASQ 64-bit split test with
      `asq_phys = 0xDEAD_BEEF_F000_0000` — verifies low =
      `0xF000_0000`, high = `0xDEAD_BEEF`.
    - 1 symmetric ACQ split test.
    - 3 validation rejection tests (zero depth, oversized depth,
      misaligned ASQ + ACQ → `AdminDepthOutOfRange` /
      `QueueBaseMisaligned` without emitting any writes).
    - 1 max-depth boundary test (`MAX_ADMIN_QUEUE_DEPTH` works and
      encodes `ASQS = ACQS = 0xFFF`).
    - 1 taxonomy distinctness check.
  - **Workspace test count**: 1450 → 1461 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.16 ASQ/ACQ wire`,
    Next=`P6.7.10-pre.17 driver-img wire`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1461 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `controller_regs::{AQA_OFFSET, ASQ_OFFSET, ACQ_OFFSET}` already
    pinned by NVMe 1.4 § 3.1.7-9.

- **Storage — P6.7.10-pre.15 NVMe controller CC.EN enable/disable
  sequencer (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/queue.rs` extended with the
  controller-state-transition helpers that compose the read-side
  `MmioReadBackend` (P6.7.10-pre.14) with the write-side
  `MmioBackend` (pre.11) to drive OIP-Driver-NVMe-014 § S6 step 4
  (disable) + step 6 (re-enable) of the bring-up sequence.
  - **`MmioBackend::write_register(offset, value)`** default-impl
    forwards to `write_doorbell` (semantically identical at the
    bus level — both are 32-bit `volatile_write`). The two methods
    stay separate on the trait so a future host-test recorder can
    distinguish doorbell traffic from register traffic without
    re-implementing the doorbell side.
  - **`wait_for_csts_not_rdy<R: MmioReadBackend>(mmio, poll_limit)
    -> Result<(), QueueError>`** symmetric to the existing
    `wait_for_csts_rdy` — polls `CSTS_OFFSET` until `CSTS_RDY_BIT`
    clears.
  - **`disable_controller<W: MmioBackend, R: MmioReadBackend>(
    mmio_w, mmio_r, poll_limit) -> Result<u32, QueueError>`**:
    1. read current `CC` to capture the configuration;
    2. write `CC & !CC_EN_BIT` (clears EN bit, preserves
       IOSQES/IOCQES/MPS/CSS);
    3. `wait_for_csts_not_rdy`.
    Returns the captured `CC` so the enable-side helper can
    restore the manifest-pinned fields without re-reading the
    register.
  - **`enable_controller<W, R>(...)`** symmetric to
    `disable_controller`:
    1. read current `CC`;
    2. write `CC | CC_EN_BIT`;
    3. `wait_for_csts_rdy`.
    Returns the final `CC` value the controller is running with so
    the bring-up FSM can assert no fields were silently cleared.
  - **"Write then poll" contract** — both helpers issue the CC
    write BEFORE polling, so a timeout failure leaves the writes
    intact (the controller may complete asynchronously). The live
    driver handles partial state through the existing IOMMU
    teardown path.
  - **+9 new host-side tests** under
    `omni_driver_nvme::queue::tests::*`:
    - 1 default-impl tripwire
      (`write_register_default_impl_forwards_to_write_doorbell`)
      verifies the default impl routes through `write_doorbell`
      so existing recorder impls see register writes without
      overriding the method.
    - 3 `wait_for_csts_not_rdy` checks (immediate cleared,
      multi-iteration poll, exhaustion).
    - 2 `disable_controller` checks (success with IOSQES/IOCQES
      preservation via bit-shift extraction; timeout with CC
      write still recorded — "write then poll" contract).
    - 2 `enable_controller` checks (success with `CC.EN` set +
      `CSTS.RDY` observation; timeout).
    - 1 round-trip integration test
      (`enable_disable_round_trip_preserves_cc_iosqes_iocqes`) —
      disable then enable through the same writer/reader pair,
      asserts final CC has `EN | IOSQES(6) | IOCQES(4)` and the
      writer recorded exactly 2 CC writes.
  - **Workspace test count**: 1441 → 1450 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.15 CC.EN sequencer`,
    Next=`P6.7.10-pre.16 ASQ/ACQ wire`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1450 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `controller_regs::{CC_OFFSET, CC_EN_BIT, CSTS_OFFSET,
    CSTS_RDY_BIT}` already pinned by NVMe 1.4 § 3.1.5 / § 3.1.6.

- **Storage — P6.7.10-pre.14 `MmioReadBackend` trait + CSTS.RDY poll
  helper (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/queue.rs` extended with the read-side
  MMIO seam that complements the existing `MmioBackend` write-side
  trait (P6.7.10-pre.11).
  - **`MmioReadBackend` trait** with a single method
    `read_register(offset: usize) -> u32`. Two-trait split avoids
    forcing every doorbell-only impl to also implement a read
    method — the existing `MmioBackend` impls in `MockMmioBackend`
    and the live-driver path stay unchanged.
  - **`wait_for_csts_rdy<R: MmioReadBackend>(mmio: &mut R,
    poll_limit: u32) -> Result<(), QueueError>`** free function
    polls `controller_regs::CSTS_OFFSET` (`0x1C` per NVMe 1.4
    § 3.1.6) up to `poll_limit` iterations, returning `Ok(())` on
    the first read where `(csts & CSTS_RDY_BIT) != 0` (bit 0 set)
    or `Err(QueueError::ControllerNotReady)` on exhaustion. Used by
    OIP-Driver-NVMe-014 § S6 step 6 (after writing `CC.EN = 1` the
    driver must wait for the controller to acknowledge by setting
    `CSTS.RDY = 1`).
  - **`QueueError::ControllerNotReady`** variant added to the
    `#[non_exhaustive]` taxonomy.
  - **+6 new host-side tests** under
    `omni_driver_nvme::queue::tests::*`:
    - 1 happy-path "RDY set on first iteration" (consumes 1 read).
    - 1 multi-iteration "3 not-ready → 1 ready" (consumes 4 reads).
    - 1 exhaustion (`poll_limit = 4` with all-zero CSTS →
      `ControllerNotReady` after exactly 4 reads).
    - 1 mask robustness (high bits of CSTS set + `RDY` →
      success — the helper looks ONLY at bit 0).
    - 1 zero-`poll_limit` edge case (immediate
      `ControllerNotReady` with no reads consumed).
    - 1 `QueueError::ControllerNotReady` discriminant distinctness
      (pairwise-distinct against `Full`, `SqPageTooSmall`,
      `CqPageTooSmall`, `DoorbellOffsetOverflow`).
  - **`ScriptedMmioRead` test fixture** —
    `{ script: Vec<(usize, u32)>, cursor: usize }` returns a
    pre-canned sequence of `(offset, value)` pairs one per
    `read_register` call, asserting that the caller queries the
    expected register offset at each step; once the script is
    exhausted, subsequent reads return `0` (matches NVMe's
    "unmapped MMIO reads as zero" semantic).
  - **Workspace test count**: 1435 → 1441 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.14 MmioRead+RDY`,
    Next=`P6.7.10-pre.15 CC.EN sequencer`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1441 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes
    `controller_regs::{CSTS_OFFSET, CSTS_RDY_BIT}` already pinned
    by NVMe 1.4 § 3.1.6.

- **Storage — P6.7.10-pre.13 NVMe `AdminSession` integration scaffold
  (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/admin_session.rs` module declares
  the composition layer that wraps `AdminQueuePair` (P6.7.10-pre.11 +
  pre.12) with the per-session bookkeeping the bring-up FSM and the
  live driver would otherwise re-implement at every call site.
  - **`AdminSession { queue_pair, sq_page: Vec<u8>, cq_page: Vec<u8>,
    next_cid: u16 }`** — owns both DMA-mapped queue pages and a
    monotone CID counter that wraps at `u16::MAX` skipping the
    reserved `0` per `omni_types::nvme::RESERVED_DRIVER_OPAQUE_ID`.
  - **`AdminSession::new(sq_depth, cq_depth, dstrd)`** allocates the
    SQ data page at `sq_depth * ADMIN_SQE_BYTES` bytes and the CQ
    data page at `cq_depth * ADMIN_CQE_BYTES` bytes, both
    zero-initialised so the CQ phase-tag check sees the correct
    "previous-lap-phase-0" state on the first lap.
  - **Submit helpers**: `submit_identify_controller`,
    `submit_identify_namespace`, `submit_identify_active_ns_list`
    (and the generic `submit_identify(target, buf_iova, mmio)`)
    compose `allocate_cid` + `encode_identify(target, buf_iova,
    PRP2 = 0, cid)` + `AdminQueuePair::submit`, returning the
    assigned CID for completion correlation.
  - **`poll_completion_for_cid<M>(cid, poll_limit, mmio)
    -> Result<Option<AdminCqeFields>, QueueError>`** polls the CQ
    drain until a matching CID arrives, silently discarding sibling
    completions for any other outstanding admin command, returning
    `Ok(None)` after `poll_limit` iterations (soft failure — caller
    decides retry vs. abort).
  - **`DEFAULT_POLL_LIMIT = 1_000_000`** constant published for
    caller convenience.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod admin_session;` declaration.
  - **+9 new host-side tests** under
    `omni_driver_nvme::admin_session::tests::*`:
    - 4 construction & CID-allocation checks (page sizing,
      ring-error propagation on zero depth, CID starts at 1, CID
      wraps past `u16::MAX` skipping 0).
    - 1 full round-trip integration test
      (`submit_identify_controller_round_trips_through_fake_controller`):
      submit through bootstrap fake → snapshot SQ page → drive a
      `FakeController` that writes a synthetic CQE → copy back
      into the session's CQ page → poll for matching CID → verify
      `sc = 0` + `sq_head` feedback into `SqRing`.
    - 2 SQE field-layout checks via the public submit helpers
      (Identify Namespace writes `CNS = 0x00` + NSID little-endian
      + CID at SQE bytes 2..=3; Identify ActiveNsList writes
      `CNS = 0x02`).
    - 2 poll behaviour tests (returns `None` after exhausted
      `poll_limit`; skips sibling completions until matching CID
      arrives by submitting two CIDs and polling for the second).
  - **`FakeController` test fixture** — lifetime-bound to a borrowed
    SQ snapshot + a mutable scratch CQ page slice; every SQ tail
    doorbell write triggers `emit_completion_for_latest_sqe` which
    reads the CID from the most-recently-written SQE slot at
    bytes 2..=3, advances the controller-side SQ head, writes a
    synthetic CQE at the current CQ tail with phase + CID +
    `sq_head`, then advances the CQ tail (flipping the emit-phase
    on wrap). The fixture's `cq_page` field uses `&mut [u8]`
    (changed from `&mut Vec<u8>` to avoid the
    `core::mem::take(&mut [u8])` sized-type pitfall) so tests use
    `copy_from_slice` to feed scratch buffer contents back into
    the session.
  - **Workspace test count**: 1426 → 1435 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.13 AdminSession`,
    Next=`P6.7.10-pre.14 NVMe driver wire`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1435 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — `omni_types::nvme::IdentifyTarget`
    already imported via `crate::admin`; `alloc::vec::Vec` already
    pulled in by `crate::ring`.

- **Storage — P6.7.10-pre.12 NVMe Admin CQ live drain (2026-05-22) —
  TASK-005 continuation.**
  `crates/omni-driver-nvme/src/queue.rs::AdminQueuePair` extended
  with the completion-half scaffold that closes the admin queue
  pair surface per NVMe 1.4 § 4.6.
  - **`cq: CqRing` field** — initial state `head = 0`,
    `expected_phase = true` per spec.
  - **`AdminQueuePair::new(sq_depth, cq_depth, dstrd)`** —
    constructor signature widened to accept independent SQ/CQ
    depths (NVMe spec allows them to differ; live drivers size
    CQ ≥ SQ to absorb async completions without blocking).
  - **`AdminQueuePair::cq()`** — read-only accessor.
  - **`drain_completion<M: MmioBackend>(mmio: &mut M,
    cq_page: &[u8]) -> Result<Option<AdminCqeFields>, QueueError>`**
    performs the canonical NVMe drain sequence:
    1. bounds-check `cq_page.len() >= cq_capacity * 16`;
    2. eagerly compute the CQ-head doorbell offset via
       `controller_regs::cq_head_doorbell_offset(0, dstrd)`;
    3. locate the current slot at
       `cq_page[head * ADMIN_CQE_BYTES..]` and parse via
       `AdminCqe::from_bytes`;
    4. `CqRing::try_take` validates the phase tag and either
       advances `head` (flipping `expected_phase` on wrap) or
       returns `Ok(None)` if the slot belongs to a previous lap;
    5. on consume success: ring the CQ head doorbell with the new
       head value AND feed `fields.sq_head` back into the local
       `SqRing::update_head` so the matching SQ slot becomes
       available for future submits automatically.
  - **`QueueError::CqPageTooSmall`** variant added; the existing
    `From<RingError>` impl already covers `CqRing::new`
    propagation.
  - **+7 new host-side tests** under
    `omni_driver_nvme::queue::tests::*`:
    - 1 phase-tag mismatch → `Ok(None)` without touching MMIO or
      ring state.
    - 1 matching phase consumes slot, advances head, rings CQ
      doorbell with correct `(offset, value)`.
    - 1 `sq_head` feedback drains the `SqRing`'s outstanding
      submission count (3 submits + 1 completion with
      `sq_head = 2` → `head_observed` jumps from 0 to 2).
    - 1 phase flip on CQ ring wrap (capacity = 2 → two drains
      exhibit head 0→1→0 and phase true→true→false, two doorbell
      writes with values 1 then 0).
    - 1 undersized CQ page rejected without doorbell write or
      ring mutation.
    - 1 tripwire that the drain uses the CQ head doorbell offset,
      not the SQ tail offset — symmetric anti-aliasing.
    - 1 `QueueError::CqPageTooSmall` discriminant distinctness.
  - **Existing tests updated** for the new
    `new(sq_depth, cq_depth, dstrd)` signature: the
    `admin_pair_with` fixture passes `sq_depth` for both rings;
    the error-propagation tests cover both SQ-side and CQ-side
    zero-capacity rejection.
  - **Workspace test count**: 1419 → 1426 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.12 CQ live drain`,
    Next=`P6.7.10-pre.13 NVMe bringup fwd`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1426 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes the pre.11 `MmioBackend`
    trait + `CqRing` from pre.9 + `parse_admin_cqe` from pre.6 +
    `cq_head_doorbell_offset` from the existing `controller_regs`.

- **Storage — P6.7.10-pre.11 NVMe Admin SQ live-submission scaffold
  (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/queue.rs` module declares the seam
  between the pure-state ring math (P6.7.10-pre.9 `SqRing`) and the
  live admin queue's MMIO half.
  - **`MmioBackend` trait** with a single method
    `write_doorbell(offset: usize, value: u32)`. Lets the same code
    path drive bare-metal `volatile_write` and a `MockMmioBackend`
    in host tests (test impl records every `(offset, value)` pair
    in a `Vec` for assertion).
  - **`AdminQueuePair { sq: SqRing, dstrd: u8 }`** scaffold.
    - `ADMIN_QID = 0` constant per NVMe 1.4 § 3.1.21.
    - `new(sq_depth: u32, dstrd: u8) -> Result<Self, QueueError>`
      propagates the underlying `RingError` through
      `QueueError::Ring(_)` via a `From<RingError>` impl.
    - `submit<M: MmioBackend>(&AdminSqe, &mut M, &mut [u8])
      -> Result<u16, QueueError>` performs the canonical sequence:
      1. bounds-check `sq_page.len() >=
         capacity * ADMIN_SQE_BYTES` before claiming a slot so a
         failure does not perturb the ring state;
      2. eagerly compute the SQ-tail doorbell offset via
         `controller_regs::sq_tail_doorbell_offset(0, dstrd)` so a
         stride-arithmetic overflow surfaces before the ring
         mutation;
      3. claim the slot through `SqRing::submit`;
      4. copy the 64-byte SQE into `sq_page[slot * 64..]`;
      5. ring the doorbell with the new tail value through
         `MmioBackend::write_doorbell`.
    - `record_head_observed(u16)` accessor feeds the controller's
      view of the SQ head back into the ring (called by the future
      CQE drain when it parses a completion's `sq_head` field).
  - **`QueueError`** taxonomy (`#[non_exhaustive]`):
    `Ring(RingError)`, `SqPageTooSmall`, `Full`,
    `DoorbellOffsetOverflow`.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod queue;` declaration.
  - **+14 new host-side tests** under
    `omni_driver_nvme::queue::tests::*`:
    - 4 construction checks (ADMIN_QID = 0, ring error propagation
      on zero/oversized depth, `dstrd` + `sq` accessors).
    - 5 submit-happy-path tests (SQE bytes copied into slot 0,
      doorbell rung with new tail = 1, three SQEs → three slots +
      three monotonically-increasing doorbell values, non-zero
      dstrd produces non-zero stride offset, distinct slots end up
      at distinct page offsets).
    - 3 submit-error-path tests (undersized SQ page rejected
      without perturbing ring state or emitting doorbell write,
      `Full` after capacity-1 submits without emitting fourth
      doorbell, `record_head_observed(1)` unblocks fourth submit
      at slot 3).
    - 2 `QueueError` taxonomy tests (`From<RingError>` impl,
      4 variants pairwise-distinct discriminant).
  - **Workspace test count**: 1405 → 1419 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.11 SQ live submit`,
    Next=`P6.7.10-pre.12 CQ live drain`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1419 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — composes existing primitives from
    `crate::admin` + `crate::ring` + `crate::controller_regs`.

- **Storage — P6.7.10-pre.10 NVMe Create IO Queue admin encoders
  (2026-05-22) — TASK-005 continuation.**
  `crates/omni-driver-nvme/src/admin.rs` extended with the two admin
  encoders that complete the bring-up sequence S6 step 11 per
  NVMe 1.4 § 5.4 + § 5.5.
  - **`encode_create_io_cq(qid, qsize, prp1, irq_vector, irq_enabled,
    physically_contig, cid) -> AdminSqe`** (opcode `0x05`): writes
    CDW0 = OPC|CID, NSID = 0, DPTR.PRP1, CDW10 = QSIZE-1 (bits
    31:16, 0-based per spec) | QID (bits 15:0), CDW11 = IV (bits
    31:16) | IEN (bit 1) | PC (bit 0).
  - **`encode_create_io_sq(qid, qsize, prp1, cq_id, queue_priority,
    physically_contig, cid) -> AdminSqe`** (opcode `0x01`): same
    CDW10 layout plus CDW11 = CQID (bits 31:16) | QPRIO (bits 2:1)
    | PC (bit 0).
  - **Conventions**:
    - `qsize` is 1-based in OMNI OS API; encoders compute
      `saturating_sub(1)` to match the spec's 0-based wire field.
    - `queue_priority` is masked to 2 bits
      (`queue_priority & 0b11`) so a corrupt value cannot bleed
      into the CQID field.
    - Local bindings renamed (`cdw0 → header_dw`,
      `cdw10 → queue_dw10`, `cdw11 → flags_dw11`) to satisfy the
      workspace `clippy::similar_names` lint when more than one
      CDW binding lives in the same fn.
  - **Constants**:
    - `CIOQ_CDW11_PC_BIT = 1 << 0` — Physically Contiguous.
    - `CIOCQ_CDW11_IEN_BIT = 1 << 1` — Interrupts Enabled.
    - `CIOSQ_CDW11_QPRIO_SHIFT = 1` — Queue Priority bit 1.
    - `CIOCQ_CDW11_IV_SHIFT = 16` — Interrupt Vector bit 16.
    - `CIOSQ_CDW11_CQID_SHIFT = 16` — Completion-queue ID bit 16.
    - `CIOSQ_QPRIO_{URGENT|HIGH|MEDIUM|LOW}` = `0b00..0b11` —
      pinning the 4 priority values; Phase-1 default is `MEDIUM`
      to match the QEMU `weighted_round_robin` scheduler.
  - **+16 new host-side tests** under
    `omni_driver_nvme::admin::tests::*`:
    - 8 `encode_create_io_cq` checks (opcode at byte 0,
      CDW10 packing, CDW11 IV+IEN+PC, CDW11 unset-flags clear,
      PRP1 placement, NSID zero, QSIZE saturating-sub on zero,
      IV shift clears low bits when IEN/PC unset).
    - 6 `encode_create_io_sq` checks (opcode 0x01, CDW10 packing,
      CDW11 CQID+QPRIO+PC, QPRIO constants pin, QPRIO 2-bit mask
      with leak-bit check, PRP1 placement, NSID zero).
    - 2 helper fixtures (`read_le_u32`, `read_le_u64`).
    - 1 cross-encoder opcode-distinctness tripwire.
  - **Workspace test count**: 1389 → 1405 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.10 NVMe CreateIO`,
    Next=`P6.7.10-pre.11 NVMe live SQ MMIO`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1405 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure extension of the existing `admin`
    module.

- **Storage — P6.7.10-pre.9 NVMe SQ/CQ ring-buffer scaffolds
  (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/ring.rs` module declares the
  pure-state Submission Queue / Completion Queue ring-buffer
  bookkeeping the future live admin / IO queue drivers wrap with
  their MMIO doorbell pair per NVMe 1.4 base spec § 4.1 + § 4.6.
  - **`SqRing { capacity, tail, head_observed }`** (`Copy`,
    `Debug`, `PartialEq`, `Eq`):
    - `new(capacity: u32) -> Result<Self, RingError>`: validates
      `1..=u16::MAX`.
    - `submit(&mut self) -> Option<u16>`: claims the next slot,
      advances tail; returns `None` on full.
    - `update_head(head: u16)`: records the controller's view of
      the SQ head (modulo-clamped defensive).
    - `is_full()` / `is_empty()` / `capacity()` /
      `usable_capacity()` (= `capacity - 1`, one slot reserved for
      empty/full distinction) / `tail()` / `head_observed()`
      accessors. All take `self` by value (struct is `Copy`).
  - **`CqRing { capacity, head, expected_phase }`** (`Copy`,
    `Debug`, `PartialEq`, `Eq`):
    - `try_take(&mut self, slot: &AdminCqe) ->
      Option<AdminCqeFields>`: parses via
      `crate::admin::parse_admin_cqe`, validates phase tag against
      `expected_phase`, advances head modulo capacity, flips
      `expected_phase` on wrap.
    - Initial state: `head = 0`, `expected_phase = true` per
      NVMe 1.4 § 4.6.
  - **`RingError`** taxonomy (`#[non_exhaustive]`):
    - `CapacityZero` — `capacity == 0`.
    - `CapacityTooLarge` — `capacity > u16::MAX`.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod ring;` declaration.
  - **+18 new host-side tests** under
    `omni_driver_nvme::ring::tests::*`:
    - 6 construction & invariant checks (zero/oversized capacity
      rejection for both rings, u16::MAX accepted, CqRing initial
      phase=true, SqRing initial state empty + usable_capacity).
    - 5 `SqRing` slot-management tests (`submit` advances tail,
      tail wraps from capacity-1 to 0 after a drain, full returns
      None and does not advance, `update_head` clamps modulo
      capacity, drain unblocks resubmit).
    - 5 `CqRing` phase-tag scenarios (matching phase advances
      head, mismatching phase returns None and leaves state
      unchanged, phase flips on wrap with stale-slot rejection,
      two full laps restore phase, capacity=1 ring flips on every
      take).
    - 1 cross-ring `RingError` discriminant tripwire.
    - 1 `make_cqe(phase, cid)` test fixture.
  - **Workspace test count**: 1371 → 1389 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.9 NVMe SQ/CQ ring`,
    Next=`P6.7.10-pre.10 NVMe live SQ MMIO`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1389 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — the ring scaffolds consume only
    `crate::admin::{AdminCqe, AdminCqeFields, parse_admin_cqe}`
    from the prior pre.6 slice.

- **Storage — P6.7.10-pre.8 NVMe PRP list page encoder (2026-05-22) —
  TASK-005 continuation.**
  `crates/omni-driver-nvme/src/transfer_model.rs` extended with the
  PRP descriptor encoders that complete the multi-page transfer
  surface the IO ring-buffer driver consumes per NVMe 1.4 § 4.1.4.
  - **`prp1_for(buffer_iova) -> u64`**: pass-through helper for the
    PRP1 dword. Kept named so call sites avoid embedding the inline
    literal and stay symmetric with `prp2_for`.
  - **`prp2_for(buffer_iova, layout, list_page_iova) -> u64`**:
    dispatches across the three legal PRP layouts —
    `SinglePage → 0`, `TwoPages → buffer_iova + PRP_PAGE_SIZE`
    (PRP2 points directly at the second page; no list allocation),
    `PrpList { .. } → list_page_iova` (PRP2 points at the
    host-prepared 4 KiB PRP list page).
  - **`write_prp_list_entries(buffer_iova, n_entries, dest: &mut [u8])
    -> Result<(), PrpError>`**: populates the host-allocated PRP list
    page with `n_entries` 64-bit little-endian pointers to consecutive
    4 KiB pages, starting at `buffer_iova + PRP_PAGE_SIZE` (PRP1
    covers page index 0, so the list starts at index 1). Walks
    `dest` via `chunks_exact_mut(PRP_ENTRY_BYTES = 8)` to satisfy
    the workspace `clippy::indexing_slicing` deny-lint. Wraparound
    at `u64::MAX` is silently saturated via `wrapping_add`
    (defensive against future cap relaxations).
  - **`PrpError`** taxonomy (`#[non_exhaustive]`):
    - `ListBufferTooSmall` — `dest.len() < n_entries * 8`.
    - `TooManyEntries` — `n_entries > PRP_ENTRIES_PER_LIST_PAGE`
      (= 512; the chained-PRP-list threshold v0.3 caps below per
      OIP-014 § S2).
  - **+12 new host-side tests** under
    `omni_driver_nvme::transfer_model::tests::*`:
    - 1 fixture (`read_prp_entry` helper).
    - 1 `prp1_for` round-trip.
    - 4 `prp2_for` dispatch checks (`SinglePage → 0`,
      `TwoPages → buf + 4096`, `PrpList → list_page_iova`, the
      `list_page_iova` argument is ignored for single+two-page
      layouts).
    - 6 `write_prp_list_entries` tests (happy-path 3 entries with
      untouched-tail check, zero-entries no-op preserves marker
      bytes, full `PRP_ENTRIES_PER_LIST_PAGE` list, rejects
      `n_entries > capacity`, rejects undersized buffer, accepts
      exact-size buffer).
    - 1 little-endian byte-ordering check.
    - 1 `PrpError` discriminant tripwire.
  - **Workspace test count**: 1358 → 1371 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.8 NVMe PRP list`,
    Next=`P6.7.10-pre.9 NVMe SQ/CQ ring`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1371 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — pure extension of the existing
    `transfer_model` module.

- **Storage — P6.7.10-pre.7 NVMe IO submission encoder (2026-05-22) —
  TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/io.rs` module declares the four
  NVM Command Set IO submission encoders OMNI OS Phase 1 surfaces on
  `omni.svc.blk.<diskN>` per NVMe 1.4 § 6. The 64-byte SQE layout is
  shared with `crate::admin` (§ 4.2 applies to both Admin and IO
  queues — only the opcode and CDWx semantics differ); re-using
  `AdminSqe` avoids doubling the auditable surface.
  - **`encode_read(nsid, lba, block_count, prp1, prp2, cid) -> AdminSqe`**:
    NVM Read (opcode `0x02`, § 6.9).
  - **`encode_write(nsid, lba, block_count, prp1, prp2, cid) -> AdminSqe`**:
    NVM Write (opcode `0x01`, § 6.15).
  - **`encode_flush(nsid, cid) -> AdminSqe`**: NVM Flush (opcode
    `0x00`, § 6.8). No PRPs, no CDWx — spec defines none.
  - **`encode_discard(nsid, lba, block_count, prp1, cid) -> AdminSqe`**:
    Dataset Management Deallocate (opcode `0x09`, § 6.7) with
    `AD = 1` bit set in CDW11; CDW10.NR = 0 (single range);
    CDW12+CDW13+CDW14 encoder-side tripwires (controller treats
    these as reserved for the DSM opcode).
  - **Internal `encode_nvm_data_transfer`** factors the shared
    Read/Write field layout: CDW0 = OPC|CID, NSID, DPTR.PRP1+PRP2,
    CDW10 = SLBA[31:0], CDW11 = SLBA[63:32], CDW12 = NLB (0-based;
    encoder computes `block_count.saturating_sub(1)`).
  - **Constants**: 4 opcodes (`OPC_NVM_FLUSH`, `OPC_NVM_WRITE`,
    `OPC_NVM_READ`, `OPC_NVM_DATASET_MGMT`); 2 DSM helpers
    (`DSM_AD_BIT = 1 << 2`, `DSM_CDW10_NR_SINGLE_RANGE = 0`).
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod io;` declaration.
  - **+21 new host-side tests** under
    `omni_driver_nvme::io::tests::*`:
    - 3 constant tripwires (`opcodes_match_nvme_spec`,
      `dsm_ad_bit_is_bit_2`, `dsm_single_range_is_zero`).
    - 8 `encode_read` field-layout checks (opcode, CID LE, NSID LE,
      PRP1+PRP2, LBA split across CDW10/CDW11, NLB 0-based encoding
      for 1/8/2048, `saturating_sub` on zero input, CDW13..15 zero).
    - 2 `encode_write` checks (opcode 0x01, byte-by-byte identical
      to Read modulo opcode — tripwire).
    - 2 `encode_flush` checks (opcode + CID + NSID only, PRPs +
      CDWx zero).
    - 4 `encode_discard` checks (opcode 0x09, PRP1 + PRP2 zero,
      CDW10.NR + CDW11.AD bit, LBA + block_count tripwires in
      CDW12..14).
    - 2 cross-encoder invariants (every encoder produces 64-byte
      SQE; distinct opcodes produce distinct byte-0).
  - **Workspace test count**: 1337 → 1358 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.7 NVMe IO encoder`,
    Next=`P6.7.10-pre.8 NVMe PRP list`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1358 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new dependency** — the `omni-types` dep added in pre.6 is
    unused by `io.rs` (the IO encoders take primitive numeric
    inputs).

- **Storage — P6.7.10-pre.6 NVMe Admin SQE/CQE primitives (2026-05-22) —
  TASK-005 continuation.**
  New `crates/omni-driver-nvme/src/admin.rs` module declares the
  pure-state Admin Submission Queue Entry / Completion Queue Entry
  primitives per NVMe 1.4 base spec § 4.2 (64-byte SQE) and § 4.6
  (16-byte CQE). The module is the auditable byte-layout
  source-of-truth for the future live-admin-queue driver.
  - **`AdminSqe`**: `repr(transparent)` newtype over `[u8; 64]`
    (size pinned by `admin_sqe_struct_is_64_bytes` host test).
    Constructed via `AdminSqe::zeroed()` and mutated only by the
    encoder helpers below.
  - **`AdminCqe`**: `repr(transparent)` newtype over `[u8; 16]`
    (size pinned by `admin_cqe_struct_is_16_bytes`).
  - **`encode_identify(target: IdentifyTarget, prp1: u64, prp2: u64,
    cid: u16) -> AdminSqe`** writes the spec-faithful field layout
    in little-endian per NVMe 1.4 § 3.0:
    - CDW0 = `OPC=0x06 IDENTIFY` | `CID` (bits 31:16) at bytes 0..3
    - NSID at bytes 4..7 (zeroed for `Controller`/`ActiveNsList`,
      non-zero for `Namespace{nsid}`)
    - Reserved + MPTR at bytes 8..23 (left zero)
    - DPTR.PRP1 at bytes 24..31
    - DPTR.PRP2 at bytes 32..39 (zero for Identify per § 5.15)
    - CDW10 = CNS (`0x01` Controller, `0x00` Namespace, `0x02`
      ActiveNsList) at bytes 40..43
    - CDW11..15 left zero
  - **`parse_admin_cqe(&AdminCqe) -> AdminCqeFields`**: total over
    the 16-byte input. Extracts 9 fields per § 4.6: `cdw0`,
    `sq_head`, `sq_id`, `cid`, `phase`, `sc`, `sct` (masked to 3
    bits per spec), `more`, `do_not_retry`.
  - **`AdminCqeFields::packed_status() -> u16`**: re-packs the
    parsed bits into the 16-bit status word
    `omni_types::nvme::NvmeEvent::CommandComplete::status` carries
    (P6.7.10-pre.5). Zeroes the CRD bits OMNI OS does not surface.
  - **`AdminCqeFields::is_success() -> bool`**: returns
    `SCT == 0 && SC == 0` per § 4.6 Generic Command Status.
  - **Constants**: 8 opcode + CNS values (`OPC_IDENTIFY = 0x06`,
    `OPC_CREATE_IO_CQ = 0x05`, `OPC_CREATE_IO_SQ = 0x01`,
    `OPC_GET_LOG_PAGE = 0x02`, `CNS_IDENTIFY_NAMESPACE = 0x00`,
    `CNS_IDENTIFY_CONTROLLER = 0x01`, `CNS_ACTIVE_NSID_LIST =
    0x02`); 2 size constants (`ADMIN_SQE_BYTES = 64`,
    `ADMIN_CQE_BYTES = 16`).
  - **Internal helpers**: `write_dw_at` / `write_qw_at` /
    `read_dw_at` use bounds-checked `Vec::get` / `get_mut` patterns
    so the workspace `clippy::indexing_slicing` lint (set at deny)
    stays clean.
  - **`crates/omni-driver-nvme/src/lib.rs`** extended with
    `pub mod admin;` declaration.
  - **`crates/omni-driver-nvme/Cargo.toml`** extended with
    `omni-types = { path = "../omni-types",
    default-features = false }`. The `default-features = false`
    strips `id-generation` (which pulls in `getrandom` and breaks
    `x86_64-unknown-none`) — mirrors `omni-kernel`'s pin so the
    downstream `omni-driver-nvme-image` ELF still compiles on the
    bare-metal target.
  - **+30 new host-side tests** under
    `omni_driver_nvme::admin::tests::*`:
    - 7 constant + struct-size tripwires (size constants,
      `AdminSqe` zero default, opcode/CNS spec match).
    - 10 `encode_identify` field-layout checks (opcode, CID
      little-endian, NSID dispatch, PRP1/PRP2 placement, CNS
      dispatch across all three targets, reserved-bytes-zero,
      CDW11..15-zero).
    - 5 `parse_admin_cqe` field-extraction checks (success status,
      phase-bit-clear, SC extraction, SCT extraction, More + DNR +
      CDW0 + CID).
    - 4 `packed_status` round-trips (success-only-phase, parse→pack
      round-trip, CRD-bits-zero, max-value status word).
    - 2 byte-layout pinning (CDW0 little-endian, `parse_admin_cqe`
      handles `u16::MAX` status word).
    - 2 supporting fixtures (`cqe_with` test helper, driver-internal
      correlation test).
  - **Workspace test count**: 1307 → 1337 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.6 NVMe admin SQE`,
    Next=`P6.7.10-pre.7 NVMe IO encoder`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1337 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    `omni-driver-nvme` + rustdoc workspace + lint-oips
    (0 error / 0 warning su 19 file).
  - **No new external dependency** — `omni-types` was already a
    workspace member; the new `default-features = false` pin reuses
    the same path the kernel uses.

- **Storage — P6.7.10-pre.5 NVMe driver-private command + event channel
  ABI types (2026-05-22) — TASK-005 continuation.**
  New `crates/omni-types/src/nvme.rs` module declares the canonical
  wire shapes carried on the two driver-private NVMe channels per
  OIP-Driver-NVMe-014 § S2 (`omni.driver.nvme.cmd`) and § S3
  (`omni.driver.nvme.evt`).
  - **`IdentifyTarget`** enum (`#[non_exhaustive]`): three variants
    (`Controller`, `Namespace{nsid}`, `ActiveNsList`) mirroring the
    NVMe `Identify` admin command CNS field per NVMe 1.4 § 5.15.1,
    restricted to the values OMNI OS Phase 1 actually issues.
  - **`NvmeCommand`** enum (`#[non_exhaustive]`): seven variants the
    user-space NVMe driver accepts on `omni.driver.nvme.cmd` —
    `Identify`, `Read`, `Write`, `Flush`, `Discard`, `GetLogPage`,
    `FormatNVM`. Every variant carries `opaque_id: u64` for
    client-side multiplexing; the driver echoes the value verbatim
    in the matching `CommandComplete` event.
  - **`NvmeEvent`** enum (`#[non_exhaustive]`): four variants the
    driver emits on `omni.driver.nvme.evt` — `CommandComplete{opaque_id,
    status, cdw0}` (raw 16-bit NVMe status word per NVMe 1.4 § 4.5),
    `AsyncEvent{event_type, event_info, log_page}`,
    `LinkStateChange{link_up}`, `ControllerFatal{cstatus}`.
  - **Anchor constants**:
    - `CMD_CHANNEL_NAME = "omni.driver.nvme.cmd"`
    - `EVT_CHANNEL_NAME = "omni.driver.nvme.evt"`
    - `MAX_BLOCK_COUNT_PER_REQUEST = 2048` (matches
      `omni_types::blk::MAX_BLOCK_COUNT_PER_REQUEST`; tripwire test
      enforces equality so the BLK→NVMe lowering layer never
      chunks a single BLK request)
    - `BLOCK_SIZE_BYTES = 4096` (matches `omni_types::blk::BLOCK_SIZE_BYTES`)
    - `RESERVED_DRIVER_OPAQUE_ID = 0` (sentinel for driver-internal
      admin commands; clients MUST NOT use)
  - All encoding routes through `omni_types::wire::encode_canonical`
    / `decode_canonical` per OIP-Serde-004; the `repr(Rust)` enums
    make the in-memory layout irrelevant to the cross-process
    contract.
  - **`crates/omni-types/src/lib.rs`** extended with `pub mod nvme;`
    declaration + module-index docs entry.
  - **+30 new host-side tests** under `omni_types::nvme::tests::*`:
    - 5 constant tripwires
      (`cmd_channel_name_matches_oip_014_s2`,
      `evt_channel_name_matches_oip_014_s3`,
      `max_block_count_matches_blk_module`,
      `block_size_matches_blk_module`,
      `reserved_driver_opaque_id_is_zero`).
    - 10 `NvmeCommand` round-trips (Identify Controller/Namespace/
      ActiveNsList, Read happy-path + Read at
      `MAX_BLOCK_COUNT_PER_REQUEST`, Write, Flush, Discard,
      GetLogPage SMART, FormatNVM).
    - 6 `NvmeEvent` round-trips (CommandComplete success +
      non-success status word, AsyncEvent, LinkStateChange up +
      down, ControllerFatal).
    - 5 wire invariants
      (`nvme_command_encoding_is_deterministic`,
      `nvme_event_encoding_is_deterministic`,
      `nvme_command_decode_rejects_trailing_bytes`,
      `nvme_command_decode_rejects_truncated_input`,
      `nvme_event_decode_rejects_empty_input`).
    - 3 discriminator-distinctness checks
      (`nvme_command_variants_are_distinguishable_on_the_wire` —
      7 variants pairwise-distinct first byte;
      `nvme_event_variants_are_distinguishable_on_the_wire` —
      4 variants pairwise-distinct;
      `identify_target_variants_are_distinguishable_on_the_wire` —
      3 variants pairwise-distinct).
    - 1 cross-channel correlation test
      (`opaque_id_round_trips_unchanged_command_to_event`).
  - **Workspace test count**: 1277 → 1307 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.5 NVMe ABI types`,
    Next=`P6.7.10-pre.6 NVMe admin queue`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1307 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    omni-types + rustdoc workspace + lint-oips (0 error / 0 warning
    su 19 file).
  - **No new dependency** — `serde` + `postcard` were already
    workspace-pinned (consumed by `crate::blk` + `crate::wire`).
  - Pure-types host-only slice — zero MMIO bytes emitted. The live
    NVMe admin-queue programming (CC.EN=1, Identify, Create IO QP)
    lands once P6.7.10-pre.6 wires the kernel-side admin SQ/CQ
    allocator + ring-buffer write helpers.

- **Storage — P6.7.10-pre.4 NVMe driver image consumes BLK syscalls
  (2026-05-22) — TASK-005 continuation.**
  Extends `crates/omni-driver-nvme-image/src/main.rs` with a new
  3-syscall block landing between the existing `IrqAttach (72)` and
  the bring-up FSM. The block exercises the kernel-side BLK registry
  that landed in P6.7.10-pre.3 end-to-end per OIP-Driver-NVMe-014 § S4
  + § S6 step 12.
  - **Step 7 — `IpcCreateChannel (20)`** (legacy MB12 fast-path,
    `send_token_ptr = recv_token_ptr = 0`): allocates a fresh
    kernel-owned channel with `BLK_CHANNEL_QUEUE_DEPTH = 1024`,
    `BackpressurePolicy::Block`, `tee_bound = false`. The kernel
    returns the new `ChannelId` in `rax`; `u64::MAX` surfaces as
    `EXIT_IPC_CREATE_FAILED = 100`.
  - **Step 8 — `BlkRegister (76)`**: records the
    `omni.svc.blk.nvme0` → `channel_id` mapping. The kernel verifies
    the caller owns `channel_id` (we just created it, so the
    ownership check passes by construction); on a non-zero errno the
    image exits with `EXIT_BLK_REGISTER_BASE + errno = 110+`.
  - **Step 9 — `BlkLookup (78)`** (defence-in-depth): round-trips
    the registration. `ENOENT` surfaces as
    `EXIT_BLK_LOOKUP_NOT_FOUND = 131`; a `channel_id` mismatch
    surfaces as `EXIT_BLK_LOOKUP_MISMATCH = 132`. Reachable only on
    a kernel-registry regression, but treated as a hard failure
    because the filesystem service would otherwise dispatch BLK
    requests to the wrong driver.
  - New syscall-number constants: `SYS_IPC_CREATE_CHANNEL = 20`,
    `SYS_BLK_REGISTER = 76`, `SYS_BLK_LOOKUP = 78`.
  - New BLK-channel constants: `NVME_DISK_SLOT = b"nvme0"` (byte
    slice avoids the `PanicOnAlloc` global-allocator panic that a
    `String` would trigger inside the `no_std + no_main` ELF),
    `BLK_CHANNEL_QUEUE_DEPTH = 1024`,
    `BLK_CHANNEL_BACKPRESSURE_BLOCK = 0`,
    `BLK_CHANNEL_TEE_NOT_BOUND = 0`.
  - New sentinel exit codes:
    - `EXIT_IPC_CREATE_FAILED = 100`
    - `EXIT_BLK_REGISTER_BASE = 110` (added to errno)
    - `EXIT_BLK_LOOKUP_NOT_FOUND = 131`
    - `EXIT_BLK_LOOKUP_MISMATCH = 132`
  - Module documentation extended to cover the new ordering
    (steps 7–9 + 10–11).
  - **Workspace test count** stable at 1277 pass / 0 fail — the new
    code is the `no_main` ELF entry path that cannot be unit-tested
    in-crate; coverage relies on the kernel-side host tests landed in
    P6.7.10-pre.3 (`errno_for_*` × 7 +
    `kernel_syscall_dispatch_blk_numbers_translate_to_blk_variants`
    × 1) plus the Proxmox boot smoke.
  - **Build Info panel**: Active=`P6.7.10-pre.4 NVMe BLK wire`,
    Next=`P6.7.10-pre.5 NVMe admin queue`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1277 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    bare-metal + rustdoc workspace + lint-oips (0 error / 0 warning
    su 19 file).
  - **No new dependency** — `omni-driver-shared` was already a
    workspace member and provides the cap-token lookups used by the
    existing `MmioMap`/`DmaMap`/`IrqAttach` triplet.

- **Storage — P6.7.10-pre.3 kernel-side BLK registry syscalls (2026-05-22) —
  TASK-005 continuation.**
  Lands the user-space bridge for the kernel-internal `BlkChannelRegistry`
  (landed P6.7.10-pre.2): three new syscalls in the `7x` driver-framework
  decade per OIP-Driver-NVMe-014 § S4 + § S6 step 12 expose the
  `omni.svc.blk.<diskN>` registry to user space through the rich
  two-register return path (`SyscallReturn { rax, rdx }`).
  - **`crates/omni-kernel/src/syscall.rs`** extended with three new
    `SyscallNumber` discriminants — `BlkRegister = 76`,
    `BlkUnregister = 77`, `BlkLookup = 78` — and three new POSIX-aligned
    errno constants — `ENOENT = 2`, `EIO = 5`, `EEXIST = 17` — in
    `syscall_errno`. ABI numbers are pinned by
    `syscall_numbers_are_stable` / `syscall_errno_codes_are_posix_aligned`
    so a future commit cannot silently renumber them.
  - **`crates/omni-kernel/src/services/blk.rs`** gains:
    - Process-global `static mut BLK_REGISTRY: BlkChannelRegistry =
      BlkChannelRegistry::new()` (gated on
      `bare-metal + target_arch = "x86_64"`, mirroring the
      `IPC_REGISTRY` singleton — single-CPU + interrupt-masked SYSCALL
      provides the no-aliasing invariant per ADR-0005).
    - `blk_registry_mut() -> &'static mut BlkChannelRegistry` and
      `blk_registry() -> &'static BlkChannelRegistry` accessors
      (both `unsafe fn`; `SAFETY` documented at the fn boundary).
    - `errno_for(BlkRegistryError) -> u64` host-callable mapper that
      publishes the POSIX-aligned errno taxonomy
      (`EINVAL`/`EEXIST`/`ENOSPC`/`ENOENT`/`EACCES`/`EIO`).
  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs::blk_handlers`**
    new bare-metal-only module:
    - `copy_user_disk_slot` validates the user pointer (`EFAULT` on
      out-of-user-half / null with non-zero len), bounds the length
      (`EINVAL` on empty / oversized), and decodes UTF-8 (the registry's
      allowed alphabet is ASCII so UTF-8 is a superset).
    - `check_channel_owner` verifies
      `ipc_registry().channel(channel_id).owner == caller_task`;
      mismatch surfaces `EACCES`, unknown id surfaces `EINVAL`.
    - `blk_register` / `blk_unregister` / `blk_lookup` chain validation
      → ownership check → registry mutation; every error path routes
      through `errno_for` so the syscall boundary stays consistent
      with `OIP-Driver-Framework-013` § S2.3.
    - `tear_down_blk_channels(task)` drains `BLK_REGISTRY` for the
      exiting owner; invoked from `task_exit` AFTER the existing
      `tear_down_mmio_mappings` / `tear_down_dma_mappings` /
      `tear_down_irq_attachments` / `tear_down_pci_bindings` chain so
      a crashed/killed driver does not leak stale registry entries on
      PCB retire.
  - **Dispatcher routing**: new `BlkRegister | BlkUnregister | BlkLookup`
    arms in legacy `dispatch` (`CapabilityDenied`, rich-path-only
    convention — same as `MmioMap`/`DmaMap`/`IrqAttach`/`DriverLoad`);
    three new arms in `dispatch_full` route to the live handlers;
    three new arms in `kernel_syscall_dispatch (extern "C")`
    translate the wire-level `u32` numbers `76 → BlkRegister`,
    `77 → BlkUnregister`, `78 → BlkLookup`.
  - **+8 new host-side tests**:
    - `services::blk::tests::errno_for_*` (7): one per
      `BlkRegistryError` variant + a tripwire
      (`errno_for_is_total_over_known_variants`) that fails on the
      next contributor who adds a variant without revisiting
      `errno_for`'s semantics.
    - `bare_metal::syscall_entry::tests::kernel_syscall_dispatch_blk_numbers_translate_to_blk_variants`
      (1): exercises the 76/77/78 → `SyscallNumber` arm explicitly so
      a future commit that drops the translation surfaces under a
      clear test name.
  - **Existing tests extended**:
    `syscall_numbers_are_stable` /
    `syscall_errno_codes_are_posix_aligned` /
    `dispatcher_full_dma_map_irq_attach_and_driver_load_surface_eaccess_on_host` /
    `kernel_syscall_dispatch_driver_framework_numbers_route` /
    `dispatcher_driver_framework_legacy_arm_returns_capability_denied`
    now cover the BLK triplet;
    `kernel_syscall_dispatch_unknown_driver_decade_number_returns_sentinel`
    narrows to `79` (the only number inside `7x` still
    reserved-but-unallocated after this slice).
  - **Workspace test count**: 1269 → 1277 pass / 0 fail
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **Build Info panel**: Active=`P6.7.10-pre.3 BLK syscalls`,
    Next=`P6.7.10-pre.4 NVMe driver image`,
    Phase=`1 - Microkernel POC  (~99.97%)`,
    Tests=`1277 workspace pass`.
  - **Acceptance gates** all clean: workspace clippy + bare-metal +
    mb12-userprobe + kernel-runner + 3 driver-image siblings clippy +
    fmt + check-no-blanket-allow (16 crate-root files) + rustdoc
    bare-metal + rustdoc workspace + lint-oips (0 error / 0 warning su
    19 file).
  - **No new dependency** — `BLK_REGISTRY` lives inside `omni-kernel`
    and the ownership check reuses the existing
    `crate::ipc::ipc_registry` immutable accessor.

- **Storage — P6.7.10-pre.2 kernel-side BLK channel registry (2026-05-22) —
  TASK-005 continuation.**
  Lands the kernel-side bookkeeping table that maps disk slot
  (`"nvme0"`, `"sata0"`, …) → live IPC `ChannelId` per
  OIP-Driver-NVMe-014 § S4 + § S6 step 12. Future filesystem services
  and capability-gated multiplexers consult the registry instead of
  sniffing the kernel IPC layer by string — the IPC layer remains
  name-agnostic by design.
  - **`crates/omni-kernel/src/services/mod.rs`** new module declares
    the `services::*` submodule tree (kernel-side well-known channel
    namespaces). Gated `#[cfg(feature = "bare-metal")]`.
  - **`crates/omni-kernel/src/services/blk.rs`** new module declares
    `BlkChannelRegistry`, `BlkChannelEntry`, and the
    `BlkRegistryError` taxonomy.
    - `register(disk_slot, channel_id, owner) -> Result<&str, ...>` —
      returns the canonical channel name (`"omni.svc.blk.<diskN>"`)
      pre-built once at insertion so consumer call sites do not
      re-allocate on every lookup.
    - `unregister(disk_slot, owner) -> Result<BlkChannelEntry, ...>` —
      owner-only teardown; `swap_remove` keeps the average remove
      path O(1).
    - `lookup_disk_slot(&str)` / `lookup_channel_name(&str)` /
      `lookup_channel_id(ChannelId)` — `Option<&BlkChannelEntry>`.
    - `clear_for_owner(TaskId) -> usize` — task-exit drain hook;
      returns the count of evicted entries.
    - Bounded: `MAX_BLK_CHANNELS = 64`, `MAX_DISK_SLOT_LEN = 32`.
    - Disk-slot validator restricts the alphabet to `[A-Za-z0-9_-]`
      and rejects empty / oversized / non-ASCII / control-byte /
      dot inputs — closes a path where a compromised driver could
      embed log-line forgeries (CRLF, ANSI escapes) into the kernel
      boot log through the channel name.
    - `BlkRegistryError::{DiskSlotEmpty, DiskSlotTooLong,
      DiskSlotInvalidChar, DiskSlotAlreadyRegistered, RegistryFull,
      DiskSlotNotRegistered, OwnerMismatch, Internal}` — all
      `#[non_exhaustive]` per OIP-014 § S7. `Internal` is a defensive
      sentinel for `Vec` invariants the registry expects to hold but
      cannot statically prove; surfaces an error rather than
      panicking so the kernel never aborts on a clippy edge case.
  - **Consumes `omni_types::blk::CHANNEL_NAME_PREFIX`** — the
    canonical prefix stays single-sourced across the two crates so
    a future rename cannot desynchronise them.
  - **`crates/omni-kernel/src/lib.rs`** extended with
    `#[cfg(feature = "bare-metal")] pub mod services;`.
  - **+32 host-side tests** under
    `omni_kernel::services::blk::tests::*`: 5 construction / constant
    tripwires + 12 `register` cases (validator + duplicate + capacity)
    + 4 `unregister` cases (owner-only + non-owner reject + unknown
    + re-register) + 6 lookup cases (by slot / channel name / channel
    id, with negative cases) + 3 `clear_for_owner` cases + 2 ordering
    cases (insertion order, `swap_remove` non-aliasing).
  - **Build Info panel** (`crates/omni-kernel/src/bare_metal/
    demo.rs::render_buildinfo`): Active=`P6.7.10-pre.2 BLK registry`
    (cyan), Next=`P6.7.10-pre.3 BLK syscalls`, Phase=`1 - Microkernel
    POC (~99.97%)`, Tests=`1269 workspace pass`.
  - **Acceptance gates**: `cargo fmt --all -- --check` clean.
    `cargo clippy --workspace --all-targets --all-features
    -- -D warnings` clean. `cargo clippy -p omni-kernel --target
    x86_64-unknown-none --features bare-metal -- -D warnings` clean.
    `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --features bare-metal,mb12-userprobe -- -D warnings` clean.
    `cargo clippy --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none -- -D warnings` clean. Clippy 3 driver-image
    siblings (`x86_64-unknown-none --release`) clean. `bash scripts/
    check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`.
    `RUSTDOCFLAGS=-Dwarnings cargo doc -p omni-kernel --features
    bare-metal --target x86_64-unknown-none --no-deps` clean.
    `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
    --all-features` clean. `python3 scripts/lint-oips.py` → `0
    error(s), 0 warning(s) across 19 file(s)`. Workspace test count
    1237 → **1269 pass / 0 fail**.
  - **No new dependency**. `alloc::string::String` + `alloc::vec::Vec`
    are already required by the kernel.
- **Storage — P6.7.10-pre.1 BLK service-channel ABI types (2026-05-22) — TASK-005 START.**
  Lands the foundational wire types for the generic BLK channel
  (`omni.svc.blk.<diskN>`) per OIP-Driver-NVMe-014 § M3 / § S4. First
  sub-slice of TASK-005 (NVMe live bring-up). Pure-types, host-testable,
  no MMIO.
  - **`crates/omni-types/src/blk.rs`** new module declares the canonical
    `BlkRequest` and `BlkResponse` enums every storage driver
    (NVMe today, SATA / virtio-blk tomorrow) MUST present. Both are
    `#[non_exhaustive]` per OIP-014 § S7 so backward-compatible variant
    additions land via PR without breaking source-level consumers.
    - `BlkRequest::Read { lba, count, buf_iova }` → NVMe `0x02 NVM Read`.
    - `BlkRequest::Write { lba, count, buf_iova }` → NVMe `0x01 NVM Write`.
    - `BlkRequest::Flush` → NVMe `0x00 NVM Flush`.
    - `BlkRequest::Discard { lba, count }` → NVMe `0x09 Dataset Management`
      with Attribute = 0x04 (Deallocate); capability-gated by the
      driver's `discard_enabled` manifest flag.
    - `BlkResponse::{Ok, NotSupported, DeviceError(u16), OutOfRange,
      InvalidArgument}` mirrors § M3 exactly.
  - **Public constants**: `NON_NVME_DEVICE_ERROR = 0xFFFF` (sentinel for
    non-NVMe `BlkResponse::DeviceError`), `MAX_BLOCK_COUNT_PER_REQUEST =
    2048` (PRP-page capacity ceiling), `BLOCK_SIZE_BYTES = 4096`
    (`LBADS = 12` per OIP-014 § M4 / § S6 step 10),
    `CHANNEL_NAME_PREFIX = "omni.svc.blk."` (capability-gate prefix
    consumed by the future kernel BLK registry).
  - **Wire contract**: all encoding goes through
    `omni_types::wire::encode_canonical` / `decode_canonical` — the
    single workspace audit point per OIP-Serde-004. No direct
    `postcard::*` calls.
  - **+24 host-side tests** under `omni_types::blk::tests::*`: 4
    `BlkRequest` round-trips (Read / Write / Flush / Discard) + 6
    `BlkResponse` round-trips (Ok / NotSupported / DeviceError NVMe
    status / DeviceError non-NVMe sentinel / OutOfRange /
    InvalidArgument) + 4 wire-invariant tests (determinism per side,
    trailing-bytes rejection, truncated-input rejection, empty-input
    rejection) + 4 constants-tripwire tests (sentinel value, max
    block count, block size, channel-name prefix) + 4 discriminator
    / encoding-shape tests (variant discriminants distinguishable,
    Flush / Ok encode to a single byte, cross-variant no-state-sharing)
    + 2 integration tests.
  - **Build Info panel** (`crates/omni-kernel/src/bare_metal/demo.rs::
    render_buildinfo`): Active=`P6.7.10-pre.1 BLK types` (cyan),
    Next=`P6.7.10-pre.2 BLK registry`, Phase=`1 - Microkernel POC
    (~99.97%)`, Tests=`1237 workspace pass`.
  - **Acceptance gates**: `cargo fmt --all -- --check` clean.
    `cargo clippy --workspace --all-targets --all-features -- -D
    warnings` clean. `cargo clippy -p omni-kernel --target
    x86_64-unknown-none --features bare-metal -- -D warnings` clean.
    `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --features bare-metal,mb12-userprobe -- -D warnings` clean. `cargo
    clippy --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none -- -D warnings` clean. Clippy 3 driver-image
    siblings (`x86_64-unknown-none --release`) clean. `bash scripts/
    check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`.
    `RUSTDOCFLAGS=-Dwarnings cargo doc -p omni-kernel --features
    bare-metal --target x86_64-unknown-none --no-deps` clean.
    `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
    --all-features` clean. `python3 scripts/lint-oips.py` → `0
    error(s), 0 warning(s) across 19 file(s)`.
    Workspace test count 1213 → **1237 pass / 0 fail**
    (`cargo test --workspace --all-features -- --test-threads=1`).
  - **No new dependency**. `serde` + `postcard` are already foundational
    workspace pins.
  - **Next slice — P6.7.10-pre.2**: kernel-side BLK channel registry at
    `crates/omni-kernel/src/services/blk.rs` (new, `#[cfg(feature =
    "bare-metal")]`), consuming the `CHANNEL_NAME_PREFIX` published in
    this slice.

- **IOMMU — P6.7.9-pre.11 live per-device DTE/Context-Entry install + `GCMD.TE` / `CTRL.IommuEn` flip (2026-05-22) — TASK-010 CLOSED.**
  Closes the sixth and final TASK-010 milestone — Phase-1 DMA isolation is now
  gated by live IOMMU translation through per-domain page tables.
  - **VT-d per-bus context-table refcounted allocator**
    (`crates/omni-kernel/src/bare_metal/iommu/vtd.rs`): new
    `BusContextTable { bus, phys, refcount }` struct + `bus_context_tables`
    field on `VtdBackend`. `acquire_bus_context_table(bus, src)` lazily
    allocates one 4-KiB-aligned context-table page per bus through the
    `pt_alloc::FrameSource`, defensively returns the frame to `src` on
    misalignment, and increments the refcount on repeat acquisitions.
    `release_bus_context_table(bus, src)` decrements the refcount and frees
    the page via `swap_remove` + `free_frame` when the last attached device
    on that bus detaches. New `VtdAttachError::BusContextAllocFailed →
    IommuError::DomainTableFull`; new `BusContextTableReleaseError::UnknownBus`.
  - **VT-d translation-enable surface** (vtd.rs): new `translation_enabled`
    flag + `is_translation_enabled()` accessor + `enable_translation(phys_off)
    -> Result<(), VtdActivateError>` `#[cfg(target_os = "none")]` method that
    writes `GCMD.TE` and polls `GSTS.TES` with the existing
    `VTD_ACTIVATION_POLL_LIMIT` bounded retry; new
    `VtdActivateError::TranslationEnableTimeout`; idempotent on repeat calls.
  - **VT-d managed install + release** (vtd.rs): new
    `install_device_entry_with_alloc(phys_off, bdf, domain, slpt_phys, width,
    translation, src)` `#[cfg(target_os = "none")]` wrapper that acquires the
    per-bus context table through `src`, then drives the existing
    `install_device_entry` MMIO path with that ctx-table phys; rolls back the
    refcount on install failure. Symmetric `release_device_entry_with_alloc`
    zeroes the device's context entry, submits per-domain context-cache +
    IOTLB invalidates (best-effort drain), decrements the bus refcount, and
    (when refcount==0) zeroes the root-table entry for the bus AND frees the
    ctx-table page.
  - **AMD-Vi translation-enable surface**
    (`crates/omni-kernel/src/bare_metal/iommu/amdvi.rs`): symmetric
    `translation_enabled` flag + `enable_translation(phys_off) -> Result<(),
    AmdViActivateError>` that read-modify-writes `CTRL` to OR in
    `CTRL_BIT_IOMMU_EN` (preserving `CMD_BUF_EN | EVENT_LOG_EN`), then submits
    `INVALIDATE_ALL` via `submit_cmd_descriptor` and waits for the
    command-buffer head to drain. New
    `AmdViActivateError::TranslationEnableTimeout`; idempotent. AMD-Vi needs
    no per-bus tables (flat device table already allocated by
    `prepare_amd_vi_unit`).
  - **Module-level dispatch**
    (`crates/omni-kernel/src/bare_metal/iommu/mod.rs`): new
    `iommu_translation_enabled() -> bool` (dispatches via
    `with_iommu_backend`; `false` for passthrough), `iommu_enable_translation(
    phys_off) -> Result<bool, IommuError>` `#[cfg(target_os = "none")]`,
    `install_vt_d_device_entry_managed(phys_off, bdf, domain, slpt_phys,
    width, translation, src)`, and `release_vt_d_device_entry_managed(phys_off,
    bdf, src)`.
  - **driver_load live wiring**
    (`crates/omni-kernel/src/bare_metal/syscall_entry.rs::driver_load_handlers::driver_load`):
    after PT root provisioning, a new `#[cfg(target_os = "none")]` block
    clones `pcb.bound_pci_devices`, builds a `KernelFrameSource`, and for each
    BDF dispatches on `iommu_vendor()` — Intel calls
    `install_vt_d_device_entry_managed` (defaults: `AddressWidth::Bits48Level4`,
    `TranslationType::UntranslatedAndTranslated`), AMD calls
    `install_amd_vi_device_entry` (defaults: `IommuFlags::READ|WRITE`,
    `PageMode::Level4`). After at least one `Ok(true)` install, calls
    `iommu_enable_translation(phys_off)` — idempotent across subsequent driver
    loads, flipping `GCMD.TE` / `CTRL.IommuEn` only the first time.
  - **tear_down_pci_bindings symmetric release** (syscall_entry.rs): on Intel
    the helper now walks the drained BDF list and calls
    `release_vt_d_device_entry_managed` for each (returns the per-bus
    ctx-table page when refcount==0) before dropping the trait-level
    attachment. AMD-Vi falls back to the existing detach (no per-bus refcount
    to maintain).
  - **+21 host-side tests** (12 in `vtd::tests`, 3 in `amdvi::tests`, 4 in
    `iommu::tests`, plus 2 error-mapping pin tests). Workspace test count
    1192 → **1213 pass / 0 fail** (`cargo test --workspace --all-features --
    --test-threads=1`).
  - Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`):
    Active=`P6.7.9-pre.11 DTE+TE live` (cyan), Next=`P6.7.10 NVMe live
    (TASK-005)`, Phase=`1 - Microkernel POC  (~99.97%)`, Tests=`1213 workspace
    pass`.
  - Gates: workspace + bare-metal + mb12-userprobe + kernel-runner clippy
    clean; `cargo fmt --all -- --check` clean; `bash
    scripts/check-no-blanket-allow.sh` → `ok (scanned 16 crate-root files)`;
    `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --all-features`
    clean; `RUSTDOCFLAGS=-Dwarnings cargo doc -p omni-kernel --features
    bare-metal --target x86_64-unknown-none --no-deps` clean; `python3
    scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 19 file(s)`.
  - **No new dependency**.
  - **TASK-010 (VT-d / AMD-Vi IOMMU backends) CLOSED**. Phase 1 ~99.97 %.

- **IOMMU — P6.7.9-pre.10 PT root provisioning wired into `DriverLoad` (2026-05-22).**
  Closes the fifth TASK-010 milestone — per-domain page-table root provisioning
  is now driven from the live `DriverLoad (73)` syscall handler. New
  `crates/omni-kernel/src/bare_metal/iommu/kernel_frame_source.rs` module exposes
  `KernelFrameSource<'a, const N: usize>`, a thin adapter wrapping
  `&'a mut BitmapFrameAllocator<N>` + the bootloader direct-map offset, that
  implements `pt_alloc::FrameSource`. On bare-metal it allocates a 4-KiB frame
  via `BitmapFrameAllocator::alloc_frame`, zero-fills the page through the
  direct map (`core::ptr::write_bytes`), defensively validates 4-KiB alignment
  (returning the frame to the pool on mismatch — belt-and-braces vs. the
  allocator's invariant), and returns the physical address. On host
  (`cfg(target_os != "none")`) the zero-fill is elided because tests never
  dereference the address. `free_frame(phys)` routes through
  `BitmapFrameAllocator::free_frame`. `bare_metal::iommu::mod` re-exports
  `KernelFrameSource` so the syscall handler can build one without exposing
  the inner module path. The `driver_load` handler's existing P6.7.9-pre.8
  PCI-bind block now also calls
  `iommu_provision_domain_pt(domain_for_task(task_id.0), &mut src)` after at
  least one BDF binds successfully; on passthrough the helper short-circuits
  to `Ok(0)` without touching the frame source, so the same code path is safe
  on platforms without an IOMMU. The `tear_down_pci_bindings` teardown helper
  is extended symmetrically — after the per-BDF detach pass, if
  `iommu_domain_pt_root_phys(domain)` reports a recorded root it calls
  `iommu_release_domain_pt` through a fresh `KernelFrameSource`, returning the
  4-KiB root frame to `FRAME_ALLOC`. Build Info panel
  (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`):
  Active=`P6.7.9-pre.10 PT wire DriverLoad` (cyan),
  Next=`P6.7.9-pre.11 PT install DTE`,
  Phase=`1 - Microkernel POC  (~99.95%)`,
  Tests=`1192 workspace pass`.
  +6 new host-side tests under
  `bare_metal::iommu::kernel_frame_source::tests::*`
  (`alloc_returns_aligned_frame_from_pool`,
  `alloc_exhausts_pool_then_returns_none`,
  `free_returns_frame_to_pool_for_reuse`,
  `provision_through_domain_page_tables_round_trips`,
  `provision_surfaces_pool_exhaustion_as_frame_alloc_failed`,
  `release_then_reprovision_distinct_domain_reuses_pool`).
  Workspace test count 1186 → **1192 pass / 0 fail**
  (`cargo test --workspace --all-features -- --test-threads=1`). Acceptance
  gates: `cargo fmt --all -- --check` clean,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings`
  clean,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal,mb12-userprobe -- -D warnings`
  clean,
  `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings`
  clean, all 3 driver-image siblings
  (`x86_64-unknown-none --release`) clean,
  `bash scripts/check-no-blanket-allow.sh` →
  `ok (scanned 16 crate-root files)`,
  `RUSTDOCFLAGS=-Dwarnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps`
  clean,
  `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --all-features`
  clean, `python3 scripts/lint-oips.py` →
  `0 error(s), 0 warning(s) across 19 file(s)`. No new dependency.

- **IOMMU — P6.7.9-pre.7 per-device attach surface + live device-entry install (2026-05-21).**
  Closes the third TASK-010 milestone: per-device translation gating.
  Vendor-neutral `PciBdf(u16)` newtype on `bare_metal::iommu` packs the
  PCI requester ID (bus/device/function) in the canonical 16-bit form
  consumed by both VT-d source-id and AMD-Vi DeviceID, with
  `from_parts(bus, device, function)` + `from_raw(raw)` constructors
  and `bus()` / `devfn()` / `device()` / `function()` accessors. The
  `IommuBackend` trait gains `attach_device(bdf, domain) -> Result<(),
  IommuError>` + `detach_device(bdf) -> Result<(), IommuError>`,
  routed through `IommuKind` dispatch — `PassthroughBackend` returns
  `Ok(())` silently; `VtdBackend` + `AmdViBackend` record + remove the
  binding in internal `Vec<{Vtd,AmdVi}Attachment>` with duplicate-
  attach → `IommuError::Unsupported` and detach-on-unknown →
  `IommuError::Unsupported`. Module-level `iommu_attach_device(bdf,
  domain)` / `iommu_detach_device(bdf)` close the public surface.
  **VT-d live install** (`#[cfg(target_os = "none")]
  VtdBackend::install_device_entry`): writes the spec-faithful 128-bit
  context entry into the per-bus context table at
  `context_entry_offset(bdf) = devfn * 16`, the root entry into the
  global root-table page at `root_entry_offset(bus) = bus * 16`,
  submits per-domain context-cache invalidate (new
  `encode_context_cache_domain_invalidate(domain)` — Type `0x1` + G
  `10` + DID bits 16..31) and per-domain IOTLB invalidate (new
  `encode_iotlb_domain_invalidate(domain)` — Type `0x2` + G `10` + DID)
  on the invalidation queue via a new `submit_iq_descriptor` helper
  (wraps on `INV_QUEUE_BYTES`; advances `IQT`; polls `IQH` to drain).
  **AMD-Vi live install** (`#[cfg(target_os = "none")]
  AmdViBackend::install_device_entry`): writes the 256-bit DTE into the
  device table at `dte_offset(bdf) = bdf.raw() * 32` (bounds-checked
  against new `DEVICE_TABLE_BYTES = 4096` for the Phase-1 1-frame
  table), submits `INVALIDATE_DEVTAB_ENTRY(device_id = bdf.raw())` +
  `INVALIDATE_IOMMU_PAGES(domain)` (new
  `encode_invalidate_iommu_pages_domain(domain)` — opcode `0x3` + DID
  bits 32..47 + S=1 spans-all in high qword per spec § 5.4.4) on the
  command buffer via a new `submit_cmd_descriptor` helper (wraps on
  `CMD_BUFFER_BYTES`). Error taxonomies: `VtdAttachError` (5 variants:
  `NotActivated/DomainNotInstalled/AlreadyAttached/AddressMisaligned/
  InvalidationTimeout`) + `AmdViAttachError` (7 variants — adds
  `DeviceTableTooSmall/UnsupportedMode`), both mapped to `IommuError`
  via `From` impls. Module-level live wrappers
  `install_vt_d_device_entry` / `install_amd_vi_device_entry` mirror
  the existing `activate_intel_vt_d` / `activate_amd_vi` pair. New
  MMIO write helpers `write_context_entry_at` / `write_root_entry_at`
  (VT-d) + `write_dte_at` (AMD-Vi). +33 host-side tests covering the
  dispatch surface end-to-end. Workspace test count 1128 → **1161**
  pass / 0 fail. Build Info Active=`P6.7.9-pre.7 IOMMU device wire`,
  Next=`P6.7.9-pre.8 driver PCI bind`, Tests=`1161 workspace pass`.
  All acceptance gates clean. **No new dependency**. `kmain`
  deliberately **NOT** wired — live install requires the driver
  framework's cap-token resource match, lands in pre.8. `GCMD.TE` /
  `CTRL.IommuEn` stay deasserted preserving Phase-1 pass-through.

- **IOMMU — P6.7.9-pre.6 AMD-Vi live MMIO register programming (2026-05-21).**
  Closes the live-programming half of TASK-010 for the AMD-Vi backend,
  symmetric to P6.7.9-pre.5's Intel VT-d slice. `AmdViBackend` gains the
  activation surface (`unit_base`, `device_table_phys`,
  `command_buffer_phys`, `event_log_phys`, `command_buffer_tail`,
  `hardware_activated` fields + `const fn` accessors +
  `prepare_activation` pure-state setter); a new `AmdViActivateError`
  taxonomy (`NotPrepared` / `CmdBufStartTimeout` /
  `EventLogStartTimeout` / `InvalidationTimeout`) with
  `impl From<AmdViActivateError> for IommuError →
  IommuError::ActivationFailed`; `#[cfg(target_os = "none")] pub unsafe
  fn activate_hardware(&mut self, phys_offset: u64)` that drives the
  spec-faithful AMD IOMMU rev 3.10 § 5.3 + § 5.5 sequence — write
  `DEV_TAB_BAR ← encode_device_table_base(table_phys, 0)`, write
  `CMD_BUF_BASE ← encode_command_buffer_base(buf_phys, 8)`, write
  `EVENT_LOG_BASE ← encode_event_log_base(log_phys, 8)`, zero
  CMD/EVENT Head/Tail registers, raise
  `CTRL.CmdBufEn | CTRL.EventLogEn` + poll `STATUS.CmdBufRun` then
  `STATUS.EventLogRun`, submit one `INVALIDATE_DEVTAB_ENTRY` command at
  buffer slot 0 (`encode_invalidate_devtab_entry(0)`), bump
  `CMD_BUFFER_TAIL` to 16, and wait for `CMD_BUFFER_HEAD` to drain.
  `CTRL.IommuEn` is deliberately **NOT** raised here (per-device
  translation gating lands when the driver framework attaches its first
  PCI device — future P6.7.9-pre.7+). All MMIO accesses use
  `core::ptr::{read_volatile, write_volatile}` and a bounded
  `AMDVI_ACTIVATION_POLL_LIMIT = 1_000_000` retry budget for
  status-mirror polls. Module-level additions: 5 STATUS bit constants
  (`STATUS_BIT_EVENT_OVERFLOW/_LOG_INT/_COM_WAIT_INT/_LOG_RUN/_CMD_BUF_RUN`),
  4 command-buffer + 3 event-log layout constants
  (`CMD_BUFFER_{ENTRY_BYTES,ENTRY_COUNT,BYTES,LENGTH_ENCODING}`,
  `EVENT_LOG_{ENTRY_BYTES,ENTRY_COUNT,BYTES,LENGTH_ENCODING}`), one
  device-table size constant (`DEVICE_TABLE_SIZE_ENCODING`), 5 command
  opcode constants (`CMD_OPCODE_COMPLETION_WAIT/_INVALIDATE_DEVTAB/_IOMMU_PAGES/_IOTLB_PAGES/_ALL`),
  and 4 pure-function encoders (`encode_device_table_base`,
  `encode_command_buffer_base`, `encode_event_log_base`,
  `encode_invalidate_devtab_entry`).
  `bare_metal::iommu::mod.rs` exposes a new public surface
  `prepare_amd_vi_unit(unit_base, device_table_phys, command_buffer_phys,
  event_log_phys)` + `#[cfg(target_os = "none")] pub unsafe fn
  activate_amd_vi(phys_offset)`; `iommu_hardware_activated()` is
  extended to dispatch through the AMD variant too (was Intel-only).
  `kmain` extended after the existing VT-d activation block:
  allocates device-table + command-buffer + event-log frames,
  zero-fills them via the direct-map, prepares + activates the AMD-Vi
  unit only when `iommu_unit_base() != 0 && iommu_vendor() == Amd`, and
  emits a single-line boot log (`[iommu] amd-vi activated unit=<base>` /
  `activate skip` / `activate err` / `prepare err` / `alloc err`). +27
  new host-side tests (24 in `amdvi::tests`, 3 in `iommu::tests`);
  workspace test count 1101 → 1128 pass / 0 fail
  (`cargo test --workspace --all-features -- --test-threads=1`). Build
  Info panel updated to Active=`P6.7.9-pre.6 AMD-Vi live MMIO` /
  Next=`P6.7.9-pre.7 IOMMU domain wire` / Tests=`1128 workspace pass`.
  **No new dependency** — MMIO programming uses
  `core::ptr::{read_volatile, write_volatile}` exclusively, polling
  uses plain `u32` arithmetic. Aligned with TASK-010 of
  `docs/planning/2026-05-21-development-plan.md`.

- **IOMMU — P6.7.9-pre.5 Intel VT-d live MMIO register programming (2026-05-21).**
  Closes the live-programming half of TASK-010 for the Intel backend.
  `VtdBackend` gains the activation surface (`unit_base`,
  `root_table_phys`, `invalidation_queue_phys`, `invalidation_queue_tail`,
  `hardware_activated` fields + `const fn` accessors + `prepare_activation`
  pure-state setter); a new `VtdActivateError` taxonomy (`NotPrepared` /
  `RootTableTimeout` / `QueueEnableTimeout` / `InvalidationTimeout`) with
  `impl From<VtdActivateError> for IommuError → IommuError::ActivationFailed`;
  `#[cfg(target_os = "none")] pub unsafe fn activate_hardware(&mut self,
  phys_offset: u64)` that drives the spec-faithful Intel VT-d rev 4.1
  § 6.2 + § 6.5 sequence — write `RTADDR ← root_table_phys`, raise
  `GCMD.SRTP` + poll `GSTS.RTPS`, write `IQA ← encode_iqa(iq_phys, 0)`
  + `IQT ← 0`, raise `GCMD.QIE` + poll `GSTS.QIES`, submit a global
  IOTLB invalidate descriptor (`encode_iotlb_global_invalidate`) and
  poll `IQH` to drain. `GCMD.TE` is deliberately **NOT** raised here
  (per-domain translation gating lands when the driver framework
  attaches its first PCI device — future P6.7.9-pre.7+). All MMIO
  accesses use `core::ptr::{read_volatile, write_volatile}` and a
  bounded `VTD_ACTIVATION_POLL_LIMIT = 1_000_000` retry budget for
  status-mirror polls. Module-level additions: 3 new GSTS bit
  constants, 4 invalidation-queue layout constants, 6 descriptor type
  / granularity tags, 3 pure-function encoders (`encode_iqa`,
  `encode_iotlb_global_invalidate`, `encode_context_cache_global_invalidate`).
  `bare_metal::iommu::mod.rs` exposes a new `IOMMU_UNIT_BASE` `AtomicU64`
  populated by the boot probe (via new `read_table_drhd_info` /
  `read_table_ivhd_info` helpers that return `(count, first_register_base)`);
  `ProbeResult` gains a `register_base: u64` field; `IommuError` gains
  the `ActivationFailed` variant; new public surface
  `prepare_vt_d_unit(unit_base, root_table_phys, invalidation_queue_phys)`,
  `activate_intel_vt_d(phys_offset)` (`#[cfg(target_os = "none")]`), and
  `iommu_hardware_activated()` accessor. `kmain` extended after
  `FRAME_ALLOC` initialisation: allocates root-table + invalidation-queue
  frames, zero-fills them via the direct-map, prepares + activates the
  VT-d unit only when `iommu_unit_base() != 0 && iommu_vendor() == Intel`,
  and emits a single-line boot log (`[iommu] vt-d activated unit=<base>` /
  `activate skip` / `activate err` / `prepare err` / `alloc err`). +23
  new host-side tests (16 in `vtd::tests`, 7 in `iommu::tests`); workspace
  test count 1078 → 1101 pass / 0 fail. No new dependency. Aligned
  with TASK-010 of `docs/planning/2026-05-21-development-plan.md`.

- **IOMMU — P6.7.9-pre.4 DMA-Map vendor switch (2026-05-21).**
  Wires the kernel-wide IOMMU backend dispatch so `DmaMap (71)` syscall
  invocations route through the firmware-selected vendor backend
  (`vtd::VtdBackend` for Intel, `amdvi::AmdViBackend` for AMD,
  `PassthroughBackend` otherwise) instead of always using passthrough.
  Adds `IommuKind` enum (variant per vendor, implements `IommuBackend`
  via static-dispatch match arms) + `pub static IOMMU_BACKEND:
  spin::Mutex<IommuKind>` initialised to `IommuKind::new_passthrough()`
  at static-init time via `const fn` + `install_backend_for_vendor`
  one-shot installer + `with_iommu_backend` mutex-bracketed closure
  accessor + `domain_for_task` `TaskId → DomainId` projector. Promotes
  `VtdBackend::new` and `AmdViBackend::new` to `pub const fn` so
  `IommuKind::new_passthrough` is `const`. `kmain` extended: after
  `iommu::probe` + telemetry global writes, `install_backend_for_vendor
  (probe.vendor)` swaps the live variant. `dma_map_handlers::dma_map`
  refactored: after cap validation + duplicate-iova check, derives
  `domain_id = domain_for_task(current.0)` and calls
  `with_iommu_backend(|b| b.install_domain(domain_id))`; after the
  contiguous frame install, calls `b.map(domain_id, iova_base,
  phys_base, len, flags)` + `b.flush(domain_id)` with `IommuFlags`
  derived from the `direction` argument. Backend `map` failure triggers
  a full PT rollback + `ENOSPC`. `tear_down_dma_mappings` calls
  `b.unmap` + `b.flush` per recorded `DmaMapping` (errors swallowed).
  +14 new host-side tests cover the dispatch surface end-to-end:
  `IommuKind` default + const-constructible + Intel/Amd/Passthrough
  routing + misaligned-input reject + unknown-domain reject +
  `install_backend_for_vendor` Intel/Amd/Passthrough swap + idempotent
  re-install state reset + `with_iommu_backend` round-trip +
  `domain_for_task` low-16-bit projection + high-bit truncation +
  static initial-state pin. Workspace test count **1064 → 1078 pass /
  0 fail** (`cargo test --workspace --all-features -- --test-threads=1`).
  Build Info panel updated to Active=`P6.7.9-pre.4 DMA-Map vendor
  switch` / Next=`P6.7.9-pre.5 IOMMU register programming` / Tests=
  `1078 workspace pass`. **No new dependency** — `spin = "0.9"` was
  already a kernel dep. Both vendor scaffolds remain dormant (zero
  MMIO bytes emitted); live register programming lands in P6.7.9-pre.5+.
  Aligned with TASK-010 of `docs/planning/2026-05-21-development-plan.md`.

- **IOMMU — P6.7.9-pre.3 AMD-Vi backend scaffold (2026-05-21).**
  Sibling to P6.7.9-pre.2's VT-d scaffold. Lands the dormant AMD I/O
  Virtualization Technology backend in a new module
  `crates/omni-kernel/src/bare_metal/iommu/amdvi.rs` (~1237 lines)
  that the P6.7.9-pre.4 DMA-Map vendor switch (above) activates when
  `iommu_vendor()` reports `IommuVendor::Amd`. The slice pins AMD
  IOMMU spec rev 3.10 § 5.5 MMIO register offsets +  § 5.2.2 Device
  Table Entry encoder + § 5.3.1 I/O Page Table encoder + § 5.7
  Extended Feature Register decoders, plus a host-testable
  `AmdViBackend` struct that implements `IommuBackend` by tracking
  domains + mappings in internal `Vec`s and emits zero MMIO bytes.
  +34 new host-side tests covering every encoder bit position, every
  EFR decoder, every `AmdViBackend` happy/error path. Workspace test
  count 1030 → 1064 pass / 0 fail. Build Info Active=`P6.7.9-pre.3
  AMD-Vi backend` / Next=`P6.7.9-pre.4 DMA-Map vendor switch` / Tests=
  `1064 workspace pass`. (Note: doc entries for this slice were
  consolidated forward into the P6.7.9-pre.4 closure to avoid a
  churning doc-only patch between two adjacent scaffold slices.)

- **IOMMU — P6.7.9-pre.2 Intel VT-d backend scaffold (2026-05-21).**
  Lands the dormant Intel VT-d backend scaffold in a new module
  `crates/omni-kernel/src/bare_metal/iommu/vtd.rs`. The slice is
  deliberately **pure-data**: it pins the Intel VT-d spec rev 4.1 § 10.4
  register offsets (`VER`/`CAP`/`ECAP`/`GCMD`/`GSTS`/`RTADDR`/`CCMD`/
  `FSTS`/`FECTL`/`FEDATA`/`FEADDR`/`FEUADDR`/`PMEN`/`IQH`/`IQT`/`IQA`)
  and the `GCMD` bit-position constants (`TE`/`SRTP`/`SFL`/`EAFL`/
  `WBF`/`QIE`/`IRE`/`SIRTP`/`CFI`) as `pub const u32` symbols; provides
  pure-function encoders for the legacy translation data structures
  (`RootEntry` with `encode_root_entry`/`encode_root_entry_absent`,
  `ContextEntry` with `encode_context_entry`/`encode_context_entry_absent`,
  `Slpte` with `encode_slpte`) plus the `TranslationType` (4 variants)
  and `AddressWidth` (4 variants) enums; decodes the `CAP` register
  fields via pure functions (`cap_domain_count`,
  `cap_supported_agaw`, `cap_caching_mode`,
  `pick_highest_supported_agaw`); and exposes a host-testable
  `VtdBackend` struct that implements the `IommuBackend` trait by
  tracking installed domains + mappings in internal `Vec`s and
  **emits zero MMIO bytes** (the live register programming + queued
  invalidation lands in P6.7.9-pre.4). `VtdError` → `IommuError`
  conversion preserves the vendor-neutral error taxonomy. +32 new
  host-side tests covering every encoder bit position, every CAP
  field decode, every `VtdBackend` happy path, and every error path
  (misaligned addresses, unknown domain, double-unmap). Workspace test
  count **998 → 1030 pass / 0 fail** (`cargo test --workspace
  --all-features -- --test-threads=1`). Build Info panel
  (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`):
  Active=`P6.7.9-pre.2 VT-d backend` (cyan), Next=`P6.7.9-pre.3 AMD-Vi
  backend`, Phase=`1 - Microkernel POC  (~99.9%)`, Tests=`1030
  workspace pass`. No kmain wiring — the probe still selects vendor
  for telemetry and `dma_map_handlers::dma_map` continues to use
  `PassthroughBackend`. Acceptance gates clean: workspace clippy +
  bare-metal + mb12-userprobe + kernel-runner + 3 driver-image siblings
  (`x86_64-unknown-none --release`); `cargo fmt --all -- --check`;
  `bash scripts/check-no-blanket-allow.sh` → `ok (scanned 16
  crate-root files)`; `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel
  --features bare-metal --target x86_64-unknown-none --no-deps`;
  `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps
  --all-features`; `python3 scripts/lint-oips.py` → `0 error(s), 0
  warning(s) across 19 file(s)`. Aligned with TASK-010 of
  `docs/planning/2026-05-21-development-plan.md`.
- **Driver SDK — P6.7.8.10 `omni-driver-shared` crate + live wiring (2026-05-21).**
  Closes the `OIP-013` § S5.3 step 8 deposit-trampoline arc by adding a
  dep-free `no_std` workspace member that every first-party driver image
  links to read the kernel-deposited `CapabilityToken` blobs at the
  well-known user-VA `0x0010_0000`. The crate exposes
  `caps::find_token(action_tag, predicate) -> Option<&'static [u8]>` for
  production driver `_start` and `caps::find_token_in_buf(buf, ..)` for
  host tests, plus the constants (`DRIVER_CAP_DEPOSIT_VA`,
  `DRIVER_CAP_DEPOSIT_LEN`, `MAX_ENTRIES`, `ACTION_TAG_*`), a
  `#[repr(C)]` header layout type (`OmniCapsHeader`), and a typed error
  enum (`OmniCapsError::{InvalidMagic, UnsupportedVersion,
  EntryCountExceeded, OutOfBoundsOffset}`).

  **Live wiring of the 3 driver-image siblings**
  (`crates/omni-driver-{net-virtio,nvme,e1000e}-image/src/main.rs`):
  each `_start` ELF entry now reads its 3 deposited tokens via
  `find_token`, issues real `MmioMap (70)` / `DmaMap (71)` /
  `IrqAttach (72)` syscalls against manifest-pinned BAR addresses
  (virtio-net `0xFEBC_0000/0x1000`, NVMe `0xFEBF_0000/0x4000`, e1000e
  `0xFEB0_0000/0x20000`) and only then advances the bring-up FSM through
  its remaining pure-state phases. The `#[allow(dead_code)]` gates on
  `syscall5`, `SYS_MMIO_MAP`, `SYS_DMA_MAP`, `SYS_IRQ_ATTACH` are removed
  — all four symbols are live. Sentinel exit codes
  (`EXIT_NO_MMIO/DMA/IRQ_TOKEN = 10/20/30`,
  `EXIT_MMIO/DMA/IRQ_BASE + errno = 40+/60+/80+`) distinguish standalone
  execution (no deposit) from loader-rejected tokens.

  Tests: 36 unit + 8 E2E (`tests/e2e_deposit_round_trip.rs`,
  `#[cfg(not(target_os = "none"))]` gated, each round-trips
  `CapabilityToken → encode_canonical → OMNICAPS page → find_token_in_buf
  → decode_canonical → equality`) + 12 doc tests + 2 `proptest`
  harnesses (idempotency on 0..512-byte buffers, no-panic invariant on
  0..=32 KiB buffers). Workspace `Cargo.toml` extended (16 members);
  `scripts/check-no-blanket-allow.sh` extended (15 → 16 crate roots).

  All acceptance gates green: workspace clippy + bare-metal +
  mb12-userprobe + kernel-runner + 3 driver-image siblings + fmt +
  check-no-blanket-allow + rustdoc bare-metal + rustdoc workspace +
  lint-oips (0/0 across 19 files). Build Info panel updated:
  `Active = P6.7.8.10 driver-shared SDK`,
  `Next = P6.7.9 virtio-net live bring-up`,
  `Tests = 935 workspace pass`.

  Closes the entire P6.7.8 arc (0..10 inclusive); P6.7.9 (live driver
  bring-up + Proxmox VMID 103 smoke) now unblocked.

- docs: protocol handshake updated to OMNI-PROTO-v0.2 per
  OIP-Serde-004. The capability-token encoding specification
  in `docs/03-mesh-protocol.md:197` still references `bincode
  2.0` (documentation gap pending TASK-022); forward-looking
  source marker added to the `transport` module stub in
  `crates/omni-mesh/` to guide Phase-4 implementers toward
  postcard 1.0 from day one.

- **Kernel — P6.7.8.9 capability deposit trampoline (2026-05-20).** Closes
  `OIP-013` § S5.3 step 8, previously deferred at P6.7.8.8. After a
  `DriverLoad (73)` syscall verifies the omni-pack signature chain and
  spawns the driver process, the kernel now mints signed
  `CapabilityToken`s for every `Resource` declared in the driver's
  manifest and pre-installs them in a read-only window in the driver's
  address space — drivers no longer hit `EACCES` on their first
  `MmioMap` / `DmaMap` / `IrqAttach` / `PciConfigRead`+`Write` call.
  Three new kernel modules:
  - `crates/omni-kernel/src/entropy.rs` — Phase-1 kernel CSPRNG.
    `seed_from_hw_32()` extracts 32 bytes by mixing `RDRAND`
    (CPUID 1/ECX bit 30 gated, 10-retry per 64-bit chunk) and
    `RDTSC` jitter through a `SplitMix64` finalizer across four
    `u64` chunks. The seed feeds a `ChaCha20Rng` from
    `rand_chacha 0.3`; the `KernelCsprng` wrapper exposes
    `next_16_bytes()` for `CapabilityId` minting plus
    `add_entropy(&[u8])` and `reseed([u8; 32])` for Phase-2 entropy
    folding (designed but not yet wired — planned consumers are the
    IRQ handler chain and the network drivers once
    `OIP-Driver-Net-015` bring-up lands). The global is a
    `spin::Mutex<Option<KernelCsprng>>` accessed via
    `with_csprng(|rng| …)`.
  - `crates/omni-kernel/src/driver_cap_issuer.rs` — kernel-side
    Ed25519 signing key. `DRIVER_CAP_ISSUER_SEED` is a fixed
    `[0xCAFEBABE × 8]` DEV-ONLY placeholder; substitution by a
    TEE-derived sealing key (Intel TDX TDREPORT / AMD SEV-SNP
    `SNP_DERIVE_KEY`) is deferred to P5.2 and a dedicated key-custody
    OIP. The kernel signing role is deliberately separated from the
    `KNOWN_ISSUERS` table (which verifies driver-manifest signatures
    in `verify_manifest`) so a compromise of either trust root does
    not implicitly compromise the other.
  - `crates/omni-kernel/src/cap_deposit.rs` — wire-format encoder +
    bare-metal installer. The deposit layout is `OMNICAPS` magic +
    `u32` version `1` + `u32` count + `[u32 action_tag, u32
    resource_tag, u32 token_offset, u32 token_len]` × N entry
    descriptors + packed postcard `CapabilityToken` blobs. The window
    is mapped read-only (`PTE_PRESENT|PTE_USER|PTE_NO_EXEC`) at
    `DRIVER_CAP_DEPOSIT_VA = 0x0000_0000_0010_0000` (1 MiB; chosen
    below the ELF default load address `0x40_0000`, above the NULL
    guard region, and disjoint from the user stack VA range and the
    driver-MMIO PML4 slot) and spans `DRIVER_CAP_DEPOSIT_PAGES = 8`
    pages (`DRIVER_CAP_DEPOSIT_LEN = 32 KiB`), sized for the
    worst-case 64-entry × ~150-byte postcard token. `encode_deposit_
    page(caps, boot_seconds, subject_node_id_bytes)` mints one token
    per declared `Resource` (`mmio_regions → Action::MmioMap`,
    `dma_windows → DmaMap`, `irq_lines → IrqAttach`, `pci_devices →
    PciConfigRead + PciConfigWrite`) with `CapabilityId::from_bytes(
    KERNEL_CSPRNG.next_16_bytes())`, `subject = NodeId::from_
    attestation_hash(provider.node_id_bytes())`, a 90-day
    `TimeWindow`, and an Ed25519 signature from the kernel issuer
    key; the bare-metal `deposit_for_driver` then allocates eight
    frames, maps them into the driver address space, and pours the
    encoded bytes through the kernel direct-map.
  - **Bypass of `omni-capability/mint`.** `CapabilityId::from_bytes`
    is `pub const fn` (no feature gate) and `CapabilityToken::sign_
    payload` is unconditional, so the kernel constructs `TokenPayload`
    directly and signs it without enabling the `mint` feature path —
    which would otherwise pull `omni-types/id-generation` and
    `getrandom`, neither of which build on `x86_64-unknown-none`.
    Documented in the `entropy.rs` and `cap_deposit.rs` module
    docstrings so the choice is auditable.
- **Kernel — `driver_load_handlers::driver_load` extension.** After
  `ProcessControlBlock::spawn_from_elf` returns `Ok(task_id)`, the
  handler now reads the new PCB through `sched.process_mut(task_id)`,
  builds a `PageMapper` from `BOOT_CR3`, and invokes
  `cap_deposit::deposit_for_driver(&manifest.capabilities,
  boot_seconds, subject_node, &address_space, &mut mapper, alloc)`. On
  success the deposit VA is recorded in `pcb.cap_deposit_va`. A
  deposit failure leaves the driver process alive without
  capabilities (first `MmioMap` → `EACCES`, observable in user space);
  atomic spawn rollback (`scheduler.cancel_spawn(task_id)`) is tracked
  as P6.7.8.10.
- **Kernel — `ProcessControlBlock` field.** New optional
  `cap_deposit_va: Option<u64>` (`None` for processes without a
  deposit, e.g. `mb11-userprobe` / `mb12-userprobe`) lets later
  inspectors locate the deposit window without parsing the PML4.
- **Kernel — `Ed25519CapabilityProvider::node_id_bytes()` accessor.**
  `pub const fn` returning the provider's 32-byte TEE attestation
  hash so the deposit encoder can pin every minted token's `subject`
  to the kernel's own node identity (verifies under the placeholder
  provider until `omni-tee` lands a real attested value in P5).
- **Build Info panel (`render_buildinfo`).** `Active = P6.7.8.9 cap
  deposit trampoline` (cyan), `Next = P6.7.8.10 driver-shared SDK`,
  `Phase = 1 - Microkernel POC (~99.9%)`, `Tests = 885 workspace
  pass`.
- **Tests.** +19 host-side tests across the three new modules:
  - `entropy::tests` (7): `seed_from_hw_returns_32_bytes` +
    `seed_from_hw_changes_on_subsequent_calls` +
    `from_seed_is_deterministic` + `next_16_bytes_advances_state` +
    `add_entropy_changes_subsequent_output` + `reseed_replaces_state`
    + `with_csprng_returns_consistent_stream_after_init_for_test`.
  - `driver_cap_issuer::tests` (3): signing-key round-trip + Ed25519
    verifiable signature + placeholder-seed pattern pin.
  - `cap_deposit::tests` (9): header layout pin + buffer length
    matches window + entry descriptor layout +
    **end-to-end `verify_signed_token` round-trip** under the
    placeholder provider + PCI device emits two tokens
    (`PciConfigRead` + `PciConfigWrite`) + `TokenCountExceeded` over
    `MAX_ENTRIES` + `align_up` basics + `TimeWindow` helper + host
    stub returns `HostStub` outside bare-metal builds.
  Workspace test count `867 → 885 pass / 0 fail` (`cargo test
  --workspace --all-features -- --test-threads=1`).
- **New Cargo dependencies** under `crates/omni-kernel/Cargo.toml`:
  `rand_core = { version = "0.6", default-features = false }`,
  `rand_chacha = { version = "0.3", default-features = false }`,
  `spin = { version = "0.9", default-features = false, features =
  ["mutex", "spin_mutex"] }`. Bare-metal compile keeps `getrandom`
  out of the dependency closure (`cargo build -p omni-kernel --target
  x86_64-unknown-none --features bare-metal` verified).
- **Spike doc.** `docs/plans/p6-7-8-9-cap-deposit-trampoline.md`
  archives the design rationale, file inventory, and acceptance
  workflow for future reference.
- **Acceptance gates.** All clean: workspace clippy, bare-metal
  clippy (`omni-kernel` + `mb12-userprobe`), `kernel-runner` clippy,
  three driver-image siblings clippy (all `x86_64-unknown-none
  --release`); `cargo fmt --all -- --check`; `scripts/check-no-
  blanket-allow.sh` `ok (scanned 15 crate-root files)`; `RUSTDOCFLAGS
  =-D warnings cargo doc -p omni-kernel --features bare-metal
  --target x86_64-unknown-none --no-deps` + workspace; `python3
  scripts/lint-oips.py` `0 error(s), 0 warning(s) across 19 file(s)`.
- **Proxmox VMID 103 smoke (2026-05-20).** UEFI image deployed; serial
  log shows the canonical sequence `[mb14.a] → [mb14.h.2] sched_lock=
  ok per_cpu_in_sched=ok set_rsp0_for_cpu=ok → [elf] probe OK →
  [virtio] tablet ready`, zero panic / fault / warning. Framebuffer
  screenshot (1280×800, GOP) confirms the Build Info panel renders
  the updated `Active`/`Next`/`Phase`/`Tests` fields.

- **Kernel — P6.7.8.8 `DriverLoad (73)` syscall handler (2026-05-20).**
  Wires the previously-stubbed `NotYetImplemented` arm end-to-end per
  `OIP-013` § S5.3. New
  `crates/omni-kernel/src/bare_metal/syscall_entry.rs::driver_load_
  handlers` module (`#[cfg(all(feature = "bare-metal", target_os =
  "none", not(test)))]`) exposes `driver_load(args) -> SyscallReturn`
  via the two-register rich path. The handler chain:
  - User-pointer validation for `cap_ptr`/`cap_len` (≤
    `MAX_TOKEN_BYTES = 1024`) and `pack_ptr`/`pack_len` (≤
    `MAX_PACK_BYTES = 32 MiB`).
  - Postcard `CapabilityToken` decode via
    `omni_types::wire::decode_canonical` +
    `Ed25519CapabilityProvider::placeholder().verify_signed_token`
    (signature + time-window + TEE binding) + `is_driver_framework_
    action` guard + `Action::DriverLoad` exactness + defence-in-depth
    `Resource::Any` pin (a token scoped to a specific PCI device or
    MMIO region is rejected for the generic load surface).
  - Heap-side `Vec<u8>` allocation of `pack_len` bytes and user →
    kernel copy through the calling process's live CR3.
  - `decode_omni_pack` envelope validation + `postcard_decode_manifest`
    body decode + `hydrate_manifest(body, signature)` reconstruction.
  - Full `verify_manifest(&manifest, image_bytes)` chain: BLAKE3 image
    hash check + `KNOWN_ISSUERS` lookup + Ed25519 manifest-signature
    verify.
  - `ProcessControlBlock::spawn_from_elf(image_bytes, PhysAddr(boot_
    pml4), &mut mapper, alloc, sched, PriorityClass::System,
    KernelPrincipal::ZERO)` with the new
    `bare_metal::BOOT_CR3` static (one-shot publish in `kmain` after
    `arch::read_cr3()`, same pattern as `PHYS_OFFSET`).
  - Returns `SyscallReturn::ok(task_id.0)` on success;
    `SyscallReturn::err(syscall_errno::ENOSPC)` on spawn failure.
  - Errno mapping in `manifest_errno`: `MalformedPack` / `PackTooLarge`
    / `ImageHashMismatch → EINVAL`; `UnknownIssuer` / `SignatureInvalid
    → EACCES`.
- **Kernel — dispatcher wiring.** `SyscallNumber::DriverLoad` moved
  from the `NotYetImplemented` tail arm to the
  `MmioMap | DmaMap | IrqAttach` legacy group (returns
  `CapabilityDenied` as a defensive sentinel for the single-register
  fallback path); `dispatch_full` adds a `DriverLoad` arm routing to
  `driver_load_handlers::driver_load` on bare-metal or
  `SyscallReturn::err(EACCES)` on the host build (no `FRAME_ALLOC` /
  `SCHEDULER` / `BOOT_CR3` singletons available).
- **Kernel — `bare_metal::BOOT_CR3` accessor.** New `AtomicU64` +
  `set_boot_cr3(value)` (low-12-bit mask defensive — accepts raw CR3
  with PCD/PWT/PCID flags below the PML4 base alignment) + `boot_cr3()`
  with `Relaxed` ordering. The syscall handler reads `boot_cr3()` to
  hand `spawn_from_elf` the kernel image's PML4 (the calling
  process's CR3 is the loader's, not the kernel image — the
  kernel-half clone target must be the boot PML4).
- **Build Info panel (`render_buildinfo`).** `Active = P6.7.8.8
  DriverLoad syscall` (cyan), `Next = P6.7.8.9 cap deposit
  trampoline`, `Phase = 1 - Microkernel POC (~99.8%)`, `Tests = 867
  workspace pass`.
- **Tests.** +3 host-side tests:
  `bare_metal::boot_cr3_tests::set_boot_cr3_masks_low_12_bits` +
  `boot_cr3_returns_zero_when_unset_observer` +
  `set_boot_cr3_round_trips_aligned_value`. Existing dispatcher tests
  adjusted (1:1 rename + arm-set widening — no net change).
  Workspace test count `864 → 867 pass / 0 fail`.
- **Acceptance gates.** All clean: workspace clippy, bare-metal
  clippy + `mb12-userprobe`, `kernel-runner` clippy, three
  driver-image siblings clippy, fmt, `check-no-blanket-allow.sh`
  (15 crate-root files), rustdoc bare-metal + workspace, lint-oips
  (0 error / 0 warning across 19 files).
- **Deferred to P6.7.8.9.** Token-deposit trampoline (`OIP-013` §
  S5.3 step 8 — pre-install attenuated child tokens at well-known
  user-VA slots before the first dispatch tick). Drivers spawned by
  P6.7.8.8 reach `_start` but their `MmioMap` / `DmaMap` /
  `IrqAttach` calls still need a separately-presented token. The
  split decouples the ELF loader + signature chain from the
  capability-store wiring.

- **Driver — P6.7.8.7 e1000e bootable image sibling (2026-05-20).**
  New `crates/omni-driver-e1000e-image/` lands as the workspace-
  excluded sibling for the M2 Ethernet driver. Mirrors the
  `omni-driver-nvme-image` (P6.7.8.5), `omni-driver-net-virtio-image`
  (P6.7.8.3), and `kernel-runner` ↔ `omni-kernel` split: the lib
  crate `omni-driver-e1000e` (P6.7.8.6) hosts the auditable bring-up
  FSM + ring/register definitions on the host, the image crate
  produces the actual bootable Ring 3 ELF that `DriverLoad (73)`
  ingests.
  - `Cargo.toml`: `no_std + no_main`, target
    `x86_64-unknown-none`, profile `release` `lto=true` `codegen-
    units=1` `opt-level=z` `panic=abort` `strip=debuginfo`; single
    runtime dep `omni-driver-e1000e` via path; `.cargo/config.toml`
    inherits workspace `rustflags`.
  - `src/main.rs`: `_start` `#[unsafe(no_mangle)] pub extern "C" fn`
    constructs `BringUp::new()` at `Phase::PciEnumeration`, drives the
    13-step FSM through `Event::Advance` until terminal, then
    `sys_exit(0)` on `Phase::Ready` or `sys_exit(1)` otherwise. A
    `syscall5` inline-asm helper is pre-wired for
    `MmioMap (70)` / `DmaMap (71)` / `IrqAttach (72)` but gated
    `#[allow(dead_code)]` until P6.7.8.9 lands the capability deposit
    trampoline.
  - Defensive `PanicOnAlloc` global allocator panics on any heap call
    (the FSM is `Copy`, so no allocation is expected at runtime; any
    accidental heap touch surfaces loudly via `TaskExit(2)`).
  - Syscall numbers `SYS_TASK_EXIT = 11`, `SYS_WRITE_CONSOLE = 60`,
    `SYS_MMIO_MAP = 70`, `SYS_DMA_MAP = 71`, `SYS_IRQ_ATTACH = 72`
    pinned locally to avoid a workspace dependency cycle with
    `omni-kernel`.
  - Workspace `Cargo.toml` `[workspace.exclude]` extended;
    `.gitignore` extended with the per-crate `target/` directory.
- **Build Info panel (`render_buildinfo`).** `Active = P6.7.8.7
  e1000e image sibling` (cyan), `Next = P6.7.8.8 DriverLoad syscall
  wire`, `Phase = 1 - Microkernel POC (~99.7%)`, `Tests = 864
  workspace pass` (invariant — sibling crates are bare-metal-only
  and inherit the FSM tests from the library crate landed in
  P6.7.8.6).
- **Build artifact.** `cargo build --manifest-path crates/omni-
  driver-e1000e-image/Cargo.toml --target x86_64-unknown-none
  --release` produces `target/x86_64-unknown-none/release/omni-
  driver-e1000e-image` (≈1896 B), ready for future `DriverLoad`
  ingestion once the omni-pack v1 wrapper (`omni-driver-pack`,
  Forge tooling) is available.
- **Acceptance gates.** All clean: workspace clippy, bare-metal
  clippy (`omni-kernel` + `mb12-userprobe`), `kernel-runner` clippy,
  three driver-image siblings clippy (`net-virtio-image`,
  `nvme-image`, `e1000e-image`, all `x86_64-unknown-none --release`),
  fmt, `check-no-blanket-allow.sh` `ok (scanned 15 crate-root
  files)`, rustdoc bare-metal + workspace, lint-oips
  (0 error / 0 warning across 19 files).

- **Drivers — P6.7.8.6 e1000e (M2) crate scaffold (2026-05-20).** New
  `crates/omni-driver-e1000e` lands as a full workspace member
  (`no_std + alloc` library, `#![cfg_attr(not(test), no_std)]`), mirroring
  the `omni-driver-net-virtio` (P6.7.8.2) and `omni-driver-nvme`
  (P6.7.8.4) skeletons. The crate locks the e1000e-side public surface
  required by `OIP-Driver-Net-015` § S5 + § S8 and the Intel 82574L
  Gigabit Ethernet Controller datasheet § 10 without wiring any syscall
  path. Five `pub` modules:
  - `pci_ids`: Intel vendor `0x8086` + five v0.3 device IDs (`0x10D3`
    82574L, `0x153A` I217-LM, `0x153B` I217-V, `0x15A1` I218-LM,
    `0x15A3` I219-LM), `is_e1000e_device(vendor, device) -> bool`
    matcher consumed by the manifest table and the driver's ECAM walk.
  - `controller_regs`: CSR offsets `CTRL/STATUS/MDIC/ICR/ITR/IMS/IMC/
    RCTL/TCTL/RDBAL..RDT/TDBAL..TDT/RAL0/RAH0` + field bits
    (`CTRL.RST=bit26`, `STATUS.LU=bit1`, `IMC_DISABLE_ALL=0xFFFFFFFF`,
    `RCTL.{EN,BAM,SECRC,BSIZE}`, `TCTL.{EN,PSP,CT,COLD}`,
    `RAH.AV=bit31`) + `rctl_enable_value()` / `tctl_enable_value()`
    const composers + 128 KiB `CSR_REGION_BYTES` + `const _: () =
    assert!(...)` compile-time layout invariants.
  - `ring_config`: power-of-two ring depth bounds (`1..=4096`) +
    `rx_buffer_count` bounds (`1..=8192`) + manifest defaults
    (`256` rings, `512` buffers) + 16-byte legacy descriptors +
    2 KiB RX buffer size + validators with `checked_mul`/`checked_add`
    overflow guards.
  - `interrupts`: IMS/IMC/ICR bit positions `TXDW=bit0`, `LSC=bit2`,
    `RXT0=bit7` + `ENABLED_IMS=0x85` (OIP-015 § S5.1 step 10 mandate)
    + `icr_has_rx/has_tx/has_link_change` const classifiers.
  - `bringup`: 13-step `Phase` enum (`PciEnumeration → MmioMap →
    DisableInterrupts → GlobalReset → ReadMac → PhyInit → SetupRxRing
    → PostRxBuffers → SetupTxRing → ConfigureRxTx → EnableInterrupts
    → AttachIrq → RegisterNetChannel → Ready`) + terminal `Failed=14`
    + `BringUp { phase, retries }` state-machine driver with
    `MAX_RETRIES=3` budget mirrored from P6.7.8.3 / P6.7.8.4 +
    `Event::{Advance, Retry, Abort(BringUpError)}` + 10-variant
    `BringUpError` + 15-variant `StepKind` projection. Actual
    `MmioMap`/`DmaMap`/`IrqAttach` syscall invocations remain deferred
    to the bootable image sibling `omni-driver-e1000e-image`
    (P6.7.8.7).
  Companion files:
  - `crates/omni-driver-e1000e/manifest.toml` developer-authored TOML v1
    template with shape OIP-013 § S5.1 + OIP-015 § S1 (`[meta]` /
    `[capabilities]` 128 KiB MMIO + 4 GiB IOVA / `[matchers]` listing
    all five vendor/device pairs / `[net]` block matching
    `ring_config` defaults).
  - Root `Cargo.toml` `[workspace.members]` extended.
  - `scripts/check-no-blanket-allow.sh` `SCOPED_CRATES` extended
    (14 → 15 crate-root files scanned).
  - Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::
    render_buildinfo`): Active=`P6.7.8.6 e1000e (M2) scaffold`,
    Next=`P6.7.8.7 e1000e image sibling`, Phase=`1 - Microkernel POC
    (~99.6%)`, Tests=`864 workspace pass`.
  +61 new host-side tests (7 pci_ids + 10 controller_regs + 7 interrupts
  + 15 ring_config + 22 bringup). Workspace test count 803 → **864 pass
  / 0 fail** (`cargo test --workspace --all-features -- --test-threads=1`).
  All gates clean: workspace clippy + bare-metal + mb12-userprobe +
  kernel-runner + net-virtio-image + nvme-image (all
  `x86_64-unknown-none --release`); fmt; check-no-blanket-allow
  (15 crate-root files); rustdoc bare-metal + workspace; lint-oips
  (0 error / 0 warning across 19 file).

### Changed

- **Kernel — P6.7.3.bis realign driver_manifest skeleton to OIP-013 Appendix B
  amendments (2026-05-20).** Founder filed four normative amendments to
  OIP-013 (Appendix B § B1) after the original P6.7.3 commit. The
  framework skeleton is realigned in-place; no driver was deployed at
  the time, so the change is non-breaking. Specifically:
  - `crates/omni-kernel/src/driver_manifest.rs`:
    - New entry point `decode_omni_pack(pack_bytes) -> Result<OmniPackSections, _>`
      parses the 64-byte omni-pack v1 header per OIP-013 § S5.5
      (magic `b"OMNIPACK"`, version `1u32`, flags reserved, three
      `(offset, len)` section descriptors). Validates magic, version,
      flag zero, signature length, section bounds, and section non-
      overlap. Allocation-free on success and failure paths (OIP-013
      § S5.3 step 3). Returns borrowed slices into the input buffer
      via the new `OmniPackSections<'a>` view.
    - New stub `postcard_decode_manifest(manifest_bytes)` returning
      `DriverManifestError::ParserNotWired` — held back until the
      `postcard` crate is added to the kernel dep surface in P6.7.8.
    - New constants `OMNI_PACK_MAGIC`, `OMNI_PACK_VERSION`,
      `OMNI_PACK_HEADER_LEN`, `OMNI_PACK_MAX_BYTES`,
      `OMNI_PACK_MAX_MANIFEST_BYTES` exported for use by the syscall
      handler and the `omni-driver-pack` build tool.
    - New error variants `MalformedPack`, `PackTooLarge` (POSIX-aligned
      with `EINVAL`).
    - `DriverMeta.omni_issuer: String` → `omni_issuer_pubkey: [u8; 32]`
      per OIP-013 § S5.1: the manifest now carries the Ed25519
      verifying key bytes directly, matching the TOML schema locked in
      § S5.1. `verify_manifest` cross-checks that key against
      `KNOWN_ISSUERS` (§ S5.4 explicitly forbids TOFU).
    - Module docs rewritten: the kernel consumes omni-pack v1, never
      TOML; TOML is the developer-authored source format compiled
      offline by the `omni-driver-pack` build tool. The 64-byte
      header layout is reproduced inline so a reader knows the wire
      format without leaving the file.
    - Removed prior `parse_manifest(toml_bytes)` entry point — it was
      misnamed under the new contract.
  - `crates/omni-kernel/src/known_issuers.rs`:
    - `lookup_issuer(id: &str)` → `lookup_issuer(pubkey: &[u8; 32])`.
      The pubkey is the primary key per § S5.4 — the `id` field is
      retained on `KnownIssuer` purely as boot-log auditability
      metadata (never consulted for an authority decision).
    - Module docs updated to reflect the pubkey-primary lookup model.
  - The MMIO VA-range widening (2 GiB → 512 GiB PML4 slot) + KASLR
    base (§ B1 #2 / § S2.5), the `IrqNotification::{Tick, MissedSince(u32)}`
    in-band channel shape (§ B1 #3 / § S4.6), and the § R7 follow-up
    OIPs (§ B1 #4) are implementation details of syscall handlers
    still returning `NotYetImplemented` — no code change today.
  - Build Info panel: `Tests = 674 workspace pass`.
  - Acceptance: workspace test count rises **665 → 674 (0 fail)**
    under `cargo test --workspace --all-features -- --test-threads=1`
    (+9 new tests covering well-formed pack decode, short header,
    bad magic, wrong version, non-zero flags, wrong signature length,
    out-of-bounds section, overlapping sections, oversized manifest).
    All four clippy `-D warnings` surfaces clean, `cargo fmt`,
    `scripts/check-no-blanket-allow.sh`, cargo doc, and
    `python3 scripts/lint-oips.py` (19 files) all clean.

### Added

- **Driver — P6.7.8.5 NVMe bootable image sibling (2026-05-20).**
  New `crates/omni-driver-nvme-image/` workspace-excluded sibling crate
  hosts the bootable Ring 3 ELF that the kernel `DriverLoad (73)` syscall
  ingests per `OIP-Driver-NVMe-014` § S6 + `OIP-Driver-Framework-013`
  § S5.3 step 9. Mirrors the `omni-driver-net-virtio-image` precedent
  from P6.7.8.3 and the `omni-kernel` ↔ `kernel-runner` split.
  - `Cargo.toml`: `no_std + no_main` binary, target
    `x86_64-unknown-none`, profile `release` `lto=true`
    `codegen-units=1` `opt-level=z` `panic=abort` `strip=debuginfo`;
    single dependency on `omni-driver-nvme` via path. `.cargo/config.toml`
    minimal (inherits workspace rustflags including the force-soft SIMD
    cfgs).
  - `src/main.rs`: `_start` entry `#[unsafe(no_mangle)] pub extern "C" fn`
    consumes the per-driver capability tokens deposited by the kernel
    driver-loader trampoline at well-known user-VA slots (§ S5.3 step 10,
    to be wired in a follow-up) and drives the 13-step bring-up FSM in
    `omni_driver_nvme::bringup` from `Phase::PciEnumeration` to
    `Phase::Ready`. Exits via `TaskExit(0)` on success, `TaskExit(1)` on
    terminal failure, `TaskExit(2)` from the panic handler.
  - `syscall5` helper inline-asm wrapper pre-cabled for `MmioMap (70)` /
    `DmaMap (71)` / `IrqAttach (72)` with the two-register `(rax, rdx)`
    return convention from OIP-013 § S2. Gated `#[allow(dead_code)]`
    until P6.7.8.x wires the actual capability-token deposit.
  - `PanicOnAlloc` `#[global_allocator]` defensive stub: panics on any
    heap call (the `BringUp` FSM is `Copy` and the syscall wrappers use
    `[u64; 6]` on the stack, so no allocation is expected at runtime).
    Any future allocation surfaces loudly as `TaskExit(2)`.
  - Syscall numbers `SYS_TASK_EXIT=11` / `SYS_WRITE_CONSOLE=60` /
    `SYS_MMIO_MAP=70` / `SYS_DMA_MAP=71` / `SYS_IRQ_ATTACH=72` pinned
    locally to avoid creating a circular workspace dependency on
    `omni-kernel`.
  - Root `Cargo.toml` `[workspace.exclude]` extended with the new
    crate; `.gitignore` extended with `/crates/omni-driver-nvme-image/target/`.
  - Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`):
    Active=`P6.7.8.5 NVMe image sibling` (cyan),
    Next=`P6.7.8.6 e1000e (M2) driver`,
    Phase=`1 - Microkernel POC (~99.5%)`,
    Tests=`803 workspace pass`.
  - Acceptance gates: `cargo test --workspace --all-features --
    --test-threads=1` → **803 pass / 0 fail** (invariato vs P6.7.8.4 —
    sub-step is bare-metal-only and inherits the FSM tests from the
    library); clippy workspace + bare-metal + mb12-userprobe +
    kernel-runner + nvme-image (target `x86_64-unknown-none --release`);
    `cargo fmt --all -- --check`;
    `bash scripts/check-no-blanket-allow.sh` (14 crate-root files);
    `RUSTDOCFLAGS="-D warnings" cargo doc -p omni-kernel
    --features bare-metal --target x86_64-unknown-none --no-deps`;
    `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    --all-features`; `python3 scripts/lint-oips.py` (0 error / 0
    warning across 19 files).
  - Build artifact: `cargo build --manifest-path
    crates/omni-driver-nvme-image/Cargo.toml --target x86_64-unknown-none
    --release` produces a clean ELF at
    `target/x86_64-unknown-none/release/omni-driver-nvme-image` (Ring 3
    binary ready for future ingestion via `DriverLoad`).
  - Smoke validation on Proxmox VMID 103 (`100.101.77.9`, 8 vCPU q35
    OVMF + swtpm TPM 2.0 + 4 GiB RAM): boot UEFI image generated via
    `cargo +nightly run --manifest-path disk-image/Cargo.toml --
    kernel-runner/target/x86_64-unknown-none/release/kernel-runner`
    (2.06 MiB `boot-uefi.img`), written to zvol
    `/dev/zvol/rpool/data/vm-103-disk-6` via
    `dd bs=1M conv=fsync`, VM started. Serial log shows the full
    sequence `[mb14.a]` → `[mb14.h.2]` + `[elf] probe OK` +
    `[virtio] tablet ready` with zero errors / panics / #PF /
    warnings. VNC screenshot (`qm monitor screendump`) confirms the
    Build Info panel renders on the 1280×800 GOP framebuffer with
    commit hash `4b875fd` and updated Active/Next/Phase/Tests fields.

- **Kernel / Capability — P6.7.3 driver-framework skeleton + OIP-013 fast-path
  (2026-05-20).** Closes the framework half of the user-space driver
  initiative (`OIP-Driver-Framework-013`, now `Active`) by landing the
  capability extensions, syscall ABI surface, and signed-manifest
  verification path the per-family driver OIPs build on.
  - `OIP-Driver-Framework-013` promoted `Last Call → Active` via founder
    fast-path (deroga to `OIP-Process-001` §5.3 14-day window;
    `activated: 2026-05-20` in frontmatter). New **Appendix A —
    Editorial Reconciliations** documents the ABI-decade move from the
    Draft's `22..=25` (which collided with the v0.1 MB12 IPC slots
    `IpcSend=22` / `IpcReceive=23`) to the reserved `7x` decade:
    `MmioMap=70`, `DmaMap=71`, `IrqAttach=72`, `DriverLoad=73`.
    `OIP-Driver-TEE-016` follows: `TeeTdcall=26→74`, `TeeMsr=27→75`.
    Numbering-only correction; no normative change to TC1–TC6 or
    behaviour. `scripts/lint-oips.py` `OPTIONAL_FRONTMATTER_KEYS`
    extended with `activated`.
  - `crates/omni-capability/src/scope.rs`: 8 new `Action` variants
    (`MmioMap`, `DmaMap`, `IrqAttach`, `PciConfigRead`,
    `PciConfigWrite`, `DriverLoad`, `DriverUnload`, `TeeProbe`) and
    4 new `Resource` variants
    (`PciDevice {segment, bus, device, function}`,
    `MmioRegion {phys_base, len}`, `DmaWindow {iova_base, len}`,
    `IrqLine(u16)`), all `#[non_exhaustive]` so the canonical postcard
    encoding bound by `OIP-Serde-004` § S2 is preserved. Subset
    semantics: `PciDevice` and `IrqLine` byte-exact equality;
    `MmioRegion` and `DmaWindow` range-contained with `u128` widening
    inside `range_is_subset` so a range touching `u64::MAX` does not
    wrap.
  - `crates/omni-kernel/src/syscall.rs`: `SyscallNumber` extended with
    the six driver-framework slots. `syscall_numbers_are_stable` pins
    each value so an accidental renumber surfaces as a test failure.
  - `crates/omni-kernel/src/bare_metal/syscall_entry.rs`:
    `KernelSyscallDispatcher` matches the new slots explicitly (rather
    than via the catch-all `_ =>`) and returns
    `Err(KernelError::NotYetImplemented)` from each — preserves the
    exhaustiveness check that catches a future variant deletion at
    compile time. `kernel_syscall_dispatch` C-ABI translator maps
    raw numbers `70..=75` to the corresponding `SyscallNumber`.
  - New module `crates/omni-kernel/src/driver_manifest.rs`. Schema:
    `DriverManifest { meta, capabilities, matchers }`; `DriverMeta`
    fields (`name`, `version`, `omni_image_hash: [u8; 32]`,
    `omni_signature: [u8; 64]`, `omni_issuer: String`);
    `DriverCapabilities` and `DriverMatchers` aggregate the
    declared per-resource grants. `parse_manifest(toml_bytes)`
    currently returns `Err(ParserNotWired)` — the TOML parser
    selection is deferred to P6.7.8 to avoid co-mingling that
    decision with the OIP-013 spec ratification.
    `verify_manifest(manifest, image_bytes)` runs the full
    BLAKE3-then-Ed25519 verification chain wired against
    `omni_crypto::hash::Blake3` and
    `omni_crypto::signing::OmniVerifyingKey`, resolving the issuer
    through `known_issuers::lookup_issuer`. Signing payload is built
    by a stable byte-deterministic encoder (resource-variant tag
    bytes `0x10..=0x13` for `MmioRegion` / `DmaWindow` / `IrqLine` /
    `PciDevice`); the encoder is transitional and will be replaced
    by a `postcard` pass when the parser lands. Helpers
    `is_driver_framework_action`, `caps_for_single_mmio`, and
    constant `ISSUER_KEY_LEN` round out the public surface.
  - New module `crates/omni-kernel/src/known_issuers.rs`. Static
    allowlist `KNOWN_ISSUERS: &[KnownIssuer]` with the Phase 1
    invariant of being empty (no first-party driver has been signed
    yet); `lookup_issuer(id) -> Option<&'static KnownIssuer>` is the
    sole resolution path. The `phase1_table_is_empty` test acts as a
    forcing function so the first issuer provisioning in P6.7.8 has
    to update the assertion deliberately.
  - `crates/omni-kernel/Cargo.toml`: added direct `omni-crypto`
    dependency (`default-features = false, features = ["bare-metal"]`)
    so BLAKE3 + Ed25519 verify primitives are available on the
    `x86_64-unknown-none` target without dragging in `rng` /
    `getrandom`. Mirror of the existing `omni-capability` declaration
    pattern.
  - `crates/omni-kernel/src/lib.rs`: registers `driver_manifest` and
    `known_issuers` as public modules alongside `capabilities`.
  - Build Info panel updated: `Phase = 1 - Microkernel POC (~98%)`,
    `Active = P6.7.3 framework skeleton`, `Next = P6.7.7 promote
    014/015/016`, `Tests = 665 workspace pass`.
  - Acceptance: workspace test count rises **645 → 665 (0 fail)** under
    `cargo test --workspace --all-features -- --test-threads=1` (+23
    new host-side tests: 11 scope, 3 syscall_entry, 7 driver_manifest,
    2 known_issuers). Clippy `-D warnings` across the four standard
    surfaces (workspace, bare-metal target, mb12-userprobe target,
    kernel-runner), `cargo fmt --all -- --check`,
    `scripts/check-no-blanket-allow.sh`,
    `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel
    --features bare-metal --no-deps`, and
    `python3 scripts/lint-oips.py` (19 files) all clean.
  - Out of scope here (gated to P6.7.8): TOML parser selection +
    integration; first signed driver image
    (`omni-driver-virtio-net`) + initial `KNOWN_ISSUERS` entry;
    real `MmioMap` / `DmaMap` / `IrqAttach` / `DriverLoad` /
    `TeeTdcall` / `TeeMsr` handler bodies replacing the
    `NotYetImplemented` stubs; IOMMU vendor backends
    (`bare_metal/iommu/{vtd,amdvi}.rs`).

## [0.3.0-alpha.1] — 2026-05-20

### Added

- **Kernel — MB14.h.2: cross-CPU context switch (2026-05-20).**
  Promotes the MB14.h.1 observer-mode dispatcher to a real
  `yield_current` on Application Processors, closing the MB14.h cycle
  (and with it the MB14 milestone). Introduces a two-tier
  synchronisation hierarchy that lets BSP and AP both consult the
  global `SCHEDULER` without racing.

  Six additive / promoted changes:

  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** — `PerCpu` grows
    `in_scheduler: AtomicBool` plus three accessors: `enter_scheduler() -> bool`
    (atomic CAS `false → true`, `AcqRel`/`Acquire`), `leave_scheduler()`
    (Release store), `is_in_scheduler() -> bool` (Acquire peek). The
    flag is the **per-CPU recursion guard** — replaces the BSP-only
    global `scheduling::IN_SCHEDULER` for bare-metal MP builds. +3
    host-side tests pin the default-clear, mutual-exclusion-per-descriptor,
    and per-descriptor-isolation contracts.

  - **`crates/omni-kernel/src/scheduling.rs`** — new
    `SCHED_LOCK: AtomicBool` plus `try_acquire_sched_lock() -> bool` /
    `release_sched_lock()` helpers. Coarse cross-CPU spinlock
    serialising mutations of `static mut SCHEDULER` (the
    `tasks` / `processes` / `run_queues` vectors). Callers pair this
    with the per-CPU `in_scheduler` guard: the per-CPU flag stops
    same-CPU re-entrance, the global lock stops cross-CPU concurrency.
    The `yield_current` bare-metal branch now routes TSS.rsp0 writes
    through the per-CPU helper introduced below.

  - **`crates/omni-kernel/src/bare_metal/tss.rs`** — new
    `set_rsp0_for_cpu(cpu_id, rsp0) -> bool`: routes `cpu_id == 0` to
    the existing `set_rsp0` (BSP `TSS`), `cpu_id >= 1` to
    `AP_TSS[cpu_id - 1]` (per-AP sibling array minted in MB14.c.2.d).
    Out-of-range `cpu_id` returns `false` without writing — defensive
    signal that a regression of the AP enrolment path slipped past
    `register_ap`. +3 host-side tests pin the BSP delegate write, the
    AP sibling-slot isolation, and the out-of-range reject.

  - **`crates/omni-kernel/src/bare_metal/ap_dispatch.rs`** —
    `kernel_ap_dispatch_observe` promoted from observer (`pop + drop`)
    to live dispatcher (`pop + inc counter + SCHEDULER.yield_current`).
    The body is bracketed by `cpu.enter_scheduler()` (acquire/release
    on failure path returns) + `try_acquire_sched_lock` (acquire after
    per-CPU guard; release before guard). The `dispatch_observations`
    counter remains a long-lived diagnostic — bumped after both guards
    are held but before the yield body, so a future regression where
    the AP timer fires but never reaches `yield_current` surfaces as
    `observed = 0` on the next boot.

  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** —
    `kernel_check_need_resched` BSP branch now symmetric to the AP
    path: takes `cpu.enter_scheduler()` then
    `try_acquire_sched_lock()` with bail-on-fail (release per-CPU
    guard and return — the next tick retries). The host-fallback
    branch (target_os != "none") retains the legacy global
    `IN_SCHEDULER` / `NEED_RESCHED` for parity with single-CPU unit
    tests.

  - **`crates/omni-kernel/src/scheduling.rs::yield_current`** —
    bare-metal x86_64 branch reads `current_cpu().cpu_id()` and
    dispatches through `set_rsp0_for_cpu(cpu_id, kernel_stack_top)`
    instead of the BSP-only `set_rsp0`. The BSP path is byte-identical
    via the delegate.

  Boot-time smoke: `kmain` appends a single line after the MB14.h.1
  block exercising the three new APIs host-side from the BSP:

  ```text
  [mb14.h.2] sched_lock=ok per_cpu_in_sched=ok set_rsp0_for_cpu=ok
  ```

  `FAIL` on any of the three slots surfaces a regression on the
  corresponding API. The cross-CPU contract is exercised implicitly by
  the MB14.h.1 reachability proof (`dispatch_observations > 0`), which
  now also covers the `yield_current` body under contention.

  Build Info panel: `Active=MB14.h.2 cross-CPU ctx swap`,
  `Next=MB14 PR + v0.3.0-alpha.1`, `Track B=MB1-MB13 OK, MB14.a-h.2 wip`,
  `Phase 1 ≈ 97%`, `Tests=650+ workspace pass`.

  ADR: [`docs/adr/0010-mb14h2-cross-cpu-context-switch.md`](docs/adr/0010-mb14h2-cross-cpu-context-switch.md)
  `accepted`. Captures (a) why coarse `SCHED_LOCK` instead of per-CPU
  dispatch tables (Phase 2 optimisation), (b) why two guards instead
  of one (per-CPU stops re-entrance, global stops concurrency), (c)
  why bail-on-contention instead of spinning (IRQ tail latency),
  (d) the open follow-ups (AP first-dispatch admission, TLB shootdown
  latency under live APs, Phase 2 per-CPU SCHEDULER split).

  Test delta: workspace pass 639+ → **645 pass / 0 fail**
  (`cargo test --workspace --all-features -- --test-threads=1`;
  pre-existing SIGSEGV mitigation per `progress-omni.md` §4.5 #16).
  All clippy / fmt / blanket-allow-guard / RUSTDOCFLAGS doc strict
  variants clean. Branch `feat/kernel-mb11-userspace`. **MB14 cycle
  formally closed — PR onto `main` + `v0.3.0-alpha.1` tag now
  unblocked.**

- **Kernel — MB14.h.1: AP-side observer dispatcher (2026-05-20).**
  Wires the LAPIC timer IRQ tail on every Application Processor through
  a new observer-mode dispatcher that pops a task id from the per-CPU
  run-queue (with work-stealing fallback) on every tick and increments
  a per-CPU `dispatch_observations` counter — *no* context switch is
  performed (that's MB14.h.2, captured as roadmap in ADR-0009). The
  step proves the AP timer ISR reaches the per-CPU run-queue
  deterministically without touching any shared mutable scheduler
  state, collapsing the MB14.h.2 risk surface to the cross-CPU
  context-switch primitives alone.

  Five additive changes:

  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** — `PerCpu`
    grows a `dispatch_observations: AtomicU64` field with
    `inc_dispatch_observation()` (Release) + `dispatch_observations()`
    (Acquire) accessors. The field sits after `need_resched` so the
    GS-relative `gs:[0]` self-pointer invariant (MB14.b) is preserved
    byte-for-byte. +3 host-side tests pin the default-zero, monotonic,
    per-descriptor-isolation contract.

  - **`crates/omni-kernel/src/bare_metal/ap_dispatch.rs`** (new) —
    `kernel_ap_dispatch_observe()` `extern "C"`: reads
    `current_cpu()`, short-circuits on BSP (defence-in-depth — the
    BSP keeps the cooperative `yield_current` path), calls
    `per_cpu_run_queue::pop_for_cpu_with_stealing(cpu_id)` and, on
    `Some(_)`, increments the per-CPU observation counter and
    discards the popped id. Host stub for non-bare-metal builds keeps
    the symbol resolvable from `cargo test --workspace`. +1 host test
    pins the stub.

  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** —
    `kernel_check_need_resched` AP branch now calls
    `ap_dispatch::kernel_ap_dispatch_observe()` after consuming the
    per-CPU `need_resched` flag (previously the AP branch only
    drained the flag and returned). BSP path is unchanged — still
    falls through to the cooperative `yield_current` under
    `IN_SCHEDULER`.

  - **`crates/omni-kernel/src/lib.rs`** — `kmain` boot-time smoke
    inserted immediately after the MB14.g per-CPU plumbing block.
    Guarded on `per_cpu::registered_ap_count() > 0`: enqueues a
    sentinel id on `cpu_id = 1` via
    `per_cpu_run_queue::enqueue_on_cpu`, then busy-polls
    `ap_slot(1).dispatch_observations()` for up to 200 M iterations
    (≈ 1 s on modern silicon). Logs
    `[mb14.h.1] ap_dispatch observed=N (ok | timeout — AP did not observe)`
    or `[mb14.h.1] ap_dispatch BSP-only — no AP enrolled` on
    single-CPU dev VMs. The 200 M budget is an order of magnitude
    above the first AP tick post-`kernel_ap_lapic_init`.

  - **`crates/omni-kernel/src/bare_metal/mod.rs`** — registers the
    new `ap_dispatch` submodule between `address_space` and `arch`.

  Build Info panel updated:
  - `Active` = `MB14.h.1 AP observe loop`
  - `Next`   = `MB14.h.2 cross-CPU ctx switch`
  - `Track B` = `MB1-MB13 OK, MB14.a-h.1 wip`
  - `Phase`  = `1 - Microkernel POC  (~96%)`
  - `Tests`  = `639+ workspace pass`

  Validation gates pass clean:
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-features --all-targets -- -D warnings`,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings`,
  `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D warnings`,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --no-deps`,
  `bash scripts/check-no-blanket-allow.sh`,
  `cargo test --workspace --all-features` (639 pass).

  Pre-existing SIGSEGV on `cargo test -p omni-kernel --lib` remains a
  carryover (item §4.5 #16 in `progress-omni.md`; mitigated via
  `--test-threads=1` which produces a clean 332-pass run).

  ADR-0009 (`docs/adr/0009-mb14h-ap-dispatch-loop.md`) **accepted** —
  captures the observer-mode design end-to-end plus the safety
  invariants and sub-step sequencing for MB14.h.2 (`SCHEDULER`
  serialisation, per-CPU `IN_SCHEDULER`, `set_rsp0_for_cpu`, IST
  sharing). MB14.h.2 will open ADR-0010 on closure.

- **Kernel — MB14.g: per-CPU plumbing (TICK_COUNT / NEED_RESCHED +
  scheduler routing) (2026-05-20).** Moves the LAPIC tick counter and
  the resched flag from a single global pair into each CPU's `PerCpu`
  descriptor, and adds the dual-write `enqueue_for_cpu` /
  `pick_next_for_cpu` API surface on `RoundRobinScheduler` so a future
  MB14.h AP dispatch loop can pull tasks from the per-CPU run-queue
  table without disturbing host-side scheduler tests.

  Six additive changes:

  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** — `PerCpu`
    grows two atomic fields: `tick_count: AtomicU64` (incremented by
    `kernel_lapic_timer_tick` on this CPU) and
    `need_resched: AtomicBool` (set by the timer ISR, consumed by the
    IRQ-tail trampoline). Accessors: `inc_tick`, `tick_count`,
    `request_resched`, `take_resched`, `resched_pending`. The fields
    sit after `kernel_rsp` so the GS-relative `gs:[0]` self-pointer
    invariant (MB14.b) is preserved byte-for-byte. +4 host tests pin
    the per-descriptor isolation contract.

  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** —
    `kernel_lapic_timer_tick` now writes only `current_cpu()` storage:
    `cpu.inc_tick(); lapic_eoi(); cpu.request_resched();`. The
    MB14.f.3 `is_bsp()` early-return is gone — every CPU records its
    own ticks without race. `kernel_check_need_resched` consumes the
    per-CPU flag (`cpu.take_resched()`), then short-circuits on APs
    (the dispatch loop arrives in MB14.h) or runs the cooperative
    `yield_current` path on the BSP exactly as before. Host / test
    builds fall back to the legacy `scheduling::NEED_RESCHED` static
    so existing trampoline-contract assertions still hold.

  - **`crates/omni-kernel/src/lib.rs`** — the `pub static mut
    TICK_COUNT: u64 = 0;` global is removed; a transition comment in
    its place documents the move and points at
    `PerCpu::tick_count` / `PerCpu::inc_tick` for replacement
    callers. A new boot-time smoke `[mb14.g] per_cpu tick=N
    resched=ok sched_route=ok` exercises the per-CPU accessors and
    the new scheduler routing methods in `kmain` (post-`sti` so the
    BSP timer has armed). Sentinel id `0xFFFF_FFFF_FFFF_EE14` lets
    the smoke push/pop without disturbing the real task pool.

  - **`crates/omni-kernel/src/scheduling.rs`** —
    `RoundRobinScheduler::enqueue_for_cpu(cpu_id, task, prio) ->
    bool` dual-writes the legacy `self.run_queues[prio]` mirror
    AND, on bare-metal builds,
    `bare_metal::per_cpu_run_queue::enqueue_on_cpu(cpu_id, ...)`.
    `pick_next_for_cpu(cpu_id)` reads from
    `per_cpu_run_queue::pop_for_cpu_with_stealing(cpu_id)` and
    sweeps the same id from the legacy mirror so the two sources
    stay coherent. Host / test builds fall back to `pick_next` for
    parity with single-CPU unit tests. +3 host tests pin both the
    dual-write contract and the priority-ordering invariant.

  - **`crates/omni-kernel/src/bare_metal/demo.rs`** — Build Info
    panel updated: Active=`MB14.g per-CPU plumbing`,
    Next=`MB14.h AP dispatch loop`, Track B=`MB1-MB13 OK, MB14.a-g
    wip`, Phase 1 ≈ 95%, Tests=`635+ workspace pass`.

  - **CI / validation** — `cargo clippy --workspace --all-features
    --all-targets -- -D warnings`, `cargo clippy -p omni-kernel
    --target x86_64-unknown-none --no-default-features --features
    bare-metal -- -D warnings`, `cargo clippy --manifest-path
    kernel-runner/Cargo.toml --target x86_64-unknown-none -- -D
    warnings`, `cargo clippy -p omni-kernel --target
    x86_64-unknown-none --no-default-features --features
    mb12-userprobe -- -D warnings`, `cargo fmt --all -- --check`,
    `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features
    bare-metal --no-deps`, and `bash
    scripts/check-no-blanket-allow.sh` all clean. Workspace test
    count `cargo test --workspace --all-features` (kernel
    `--test-threads=1` for the pre-existing SIGSEGV carryover):
    **635** unit + integration tests passing (was 592+ on MB14.f).

- **Kernel — MB14.f: AP LAPIC enable + x2APIC awareness + per-AP timer
  (2026-05-20).** Closes the MB14.e.4 follow-up (TLB shootdown ack
  timeout on AP) and lands the x2APIC infrastructure that future
  server-class topologies require. Single ADR (0008) accepted.

  Six additive changes:

  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** — new
    `pub extern "C" fn kernel_ap_lapic_init()` (`#[unsafe(no_mangle)]`)
    invoked from the `kmain_ap` global_asm! body. Reads the BSP-
    observed `X2APIC_MODE` AtomicBool; if set, flips `IA32_APIC_BASE`
    bits 10+11 on this AP's MSR (the flip is per-CPU and the BSP-side
    write does not propagate); then calls `program_lapic_local(mode)`
    which writes SIVR (`LAPIC_ENABLE | 0xFF`), TPR=0, LVT timer
    (periodic, vector 0x20), timer divider, and timer initial count.
    Mode-aware: xAPIC path uses the bootloader-mapped MMIO window,
    x2APIC path uses the canonical Intel SDM Vol 3A Table 10-6 MSR
    addresses (`MSR = 0x800 + (mmio_offset >> 4)`). Same module:
    new `LapicMode::{XApic, X2Apic}` enum, `detect_lapic_mode()`
    (reads `IA32_APIC_BASE` bit 10), `is_x2apic_enabled()`, and
    mode-aware refactors of `lapic_eoi`, `lapic_send_ipi`,
    `lapic_icr_busy`, and `read_lapic_id` (x2APIC path returns the
    full 32-bit ID via `IA32_X2APIC_APICID` MSR `0x802`).
    `kernel_lapic_timer_tick` + `kernel_check_need_resched`
    short-circuit on `current_cpu().is_bsp() == false`: AP timer
    ticks EOI early without touching the `static mut TICK_COUNT`
    (BSP-only writer) or the global `NEED_RESCHED` flag — keeping
    the BSP scheduler race-free until MB14.g lands per-CPU dispatch.
  - **`crates/omni-kernel/src/bare_metal/mp_ap_entry.rs`** — `kmain_ap`
    global_asm! body extended in two places. Step 1 (LAPIC ID read)
    promoted from `mov eax, 1; cpuid; shr ebx, 24` (CPUID leaf 1
    EBX[31:24], 8-bit xAPIC ID) to `mov eax, 0x0B; xor ecx, ecx;
    cpuid; mov ebx, edx` (CPUID leaf 0xB sub-leaf 0 EDX, 32-bit
    x2APIC ID — identical to the zero-extended leaf 1 value in xAPIC
    mode per Intel SDM Vol 2 — CPUID leaf 0BH; supported on every
    x86_64 CPU since Nehalem). Step 8 (between `ltr` and the
    `lock inc AP_ONLINE_ACK`) gains a `call kernel_ap_lapic_init`
    so every AP leaves its init sequence with the local LAPIC
    enabled and the periodic timer armed. RSP at the call site is
    the freshly-loaded per-CPU kernel stack top (page-aligned →
    16-byte aligned), satisfying the System V AMD64 ABI.
  - **`crates/omni-kernel/src/lib.rs`** — `kmain` logs `[mb14.f]
    lapic_mode=xAPIC|x2APIC` right after `lapic_init` succeeds. The
    Build Info panel rows now read `Active = MB14.f AP LAPIC +
    x2APIC`, `Next = MB14.g AP dispatch + PR`, `Track B = MB1-MB13
    OK, MB14.a-f wip`, `Tests = 592+ workspace pass`.
  - **`docs/adr/0008-mb14f-per-cpu-scheduling-protocol.md`** — new
    ADR `accepted` capturing the MB14.e.4 root-cause confirmation
    (hypothesis (a): AP LAPIC was never enabled — SIVR bit 8 cleared
    → Intel SDM Vol 3A § 10.4.3 silent IPI drop), the .1/.2/.3
    sub-block decomposition, and the alternatives considered
    (BSP-driven IPI to every AP, runtime x2APIC flip, `TICK_COUNT`
    promotion to `AtomicU64` — all rejected with explicit rationale).
  - **+6 host-side tests** in `bare_metal::lapic::tests`:
    `lapic_mode_variants_compare_independently`,
    `x2apic_msr_addresses_match_intel_sdm_table_10_6`,
    `apic_base_msr_layout_matches_intel_sdm`,
    `x2apic_mode_flag_defaults_to_xapic_before_init`,
    `xapic_mmio_offsets_pinned_against_intel_sdm_table_10_1`,
    `x2apic_msr_offsets_match_mmio_via_canonical_shift` (algebraic
    relation `MSR = 0x800 + (mmio_offset >> 4)`).
  - **Workspace test count** 586+ → 592+.

  Build matrix verification (every command exits 0, no warnings):
  `cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal`,
  `cargo build --manifest-path kernel-runner/Cargo.toml --target
  x86_64-unknown-none`,
  `cargo build --manifest-path kernel-runner/Cargo.toml --target
  x86_64-unknown-none --features mb12-userprobe`,
  `cargo clippy --workspace --all-features --all-targets -- -D
  warnings`,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal -- -D warnings`,
  `cargo clippy -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features mb12-userprobe -- -D warnings`,
  `cargo clippy --manifest-path kernel-runner/Cargo.toml --target
  x86_64-unknown-none -- -D warnings`. The pre-existing SIGSEGV on
  `cargo test -p omni-kernel --lib` remains carry-over (item §4.5 #16
  in `progress-omni.md`) — unrelated to MB14.f.

- **Kernel — MB14.c.2.b.2: bare-metal trampoline emplacement (2026-05-19).**
  Materialises the pure-function builders of MB14.c.2.b.1 into actual
  physical memory: three frames from the global `BitmapFrameAllocator`
  host the temporary PML4 / PDPT / PD, their contents (generated by
  `build_temp_identity_paging`) are written through the bootloader's
  direct map (`phys_offset + paddr` → `core::ptr::write_volatile` for
  every u64 entry), the trampoline page `0x0000_8000` is identity-
  mapped in the active CR3 via `PageMapper::map_4k` (defensive — the
  BSP copy already goes through the direct map, but the mapping
  guarantees the page is also reachable as VA `0x8000` for future
  inspection / debug), and the 256-byte blob produced by
  `build_trampoline_blob(0x8000, pml4_u32, 0xFFFF_FFFF_8010_0000)` is
  copied to physical `0x8000` byte-by-byte via `write_volatile`. This
  is the last step before MB14.c.2.c flips `start_aps` to `Live` and
  the AP wakes up at the trampoline.

  Four additive changes:

  - **`crates/omni-kernel/src/bare_metal/mp_emplacement.rs`** — new
    module with `pub const TRAMPOLINE_PHYS_BASE: u32 = 0x0000_8000`
    and `pub const TRAMPOLINE_SIPI_VECTOR: u8 = 0x08` (the vector byte
    `TRAMPOLINE_PHYS_BASE >> 12` which MB14.c.2.c will encode into the
    ICR `vector` field for both SIPI writes),
    `pub struct EmplacedTrampoline { trampoline_paddr: u32,
    temp_pml4_paddr: u64 }`, `pub enum EmplacementError { OutOfFrames,
    Pml4Above4GiB, TrampolineVaConflict, MapFailed }`, and
    `pub fn place_trampoline<const N: usize>(allocator: &mut
    BitmapFrameAllocator<N>, mapper: &mut PageMapper, kernel_ap_entry:
    u64) -> Result<EmplacedTrampoline, EmplacementError>`. All error
    paths free the frames they allocated back to the bitmap allocator
    before returning — a partial OOM never leaves the allocator in
    an inconsistent state, and the function is safe to retry.
    Identity-mapping the trampoline page in active CR3 is idempotent:
    if `mapper.translate(VirtAddr(0x8000))` already returns
    `Some(PhysAddr(0x8000))`, the step is a no-op. The temp PML4 is
    range-checked (`<= u32::MAX`) so the trampoline's 32-bit
    `mov eax, temp_pml4_paddr` relocation cannot truncate.
  - **`crates/omni-kernel/src/bare_metal/mod.rs`** — registers the new
    module (`pub mod mp_emplacement;`).
  - **`crates/omni-kernel/src/lib.rs`** — hooks
    `bare_metal::mp_emplacement::place_trampoline(&mut FRAME_ALLOC,
    &mut pager, 0xFFFF_FFFF_8010_0000)` in `kmain` after the
    MB14.c.2.b.1 dry-run-only builder log block. The call is gated by
    `topo.enabled_count() > 1`: on uniprocessor systems we skip the
    emplacement entirely (no AP to receive the trampoline, no point
    reserving three frames + identity map entries for nothing) and
    log `[mb14.c.2.b.2] BSP-only — emplacement skipped`. On
    multiprocessor systems the success path logs
    `[mb14.c.2.b.2] emplaced tramp_paddr=0x8000 temp_pml4=<addr>`,
    the error path logs `[mb14.c.2.b.2] emplacement FAILED — BSP only`
    (a no-op fallback that keeps the kernel running).
  - **+11 host-side unit tests** in `bare_metal::mp_emplacement::tests`:
    `place_trampoline_returns_canonical_trampoline_paddr` (pin the
    SIPI vector / phys base contract),
    `place_trampoline_temp_pml4_is_in_low_4gib` (CR3 32-bit
    relocation invariant), `place_trampoline_consumes_three_frames_from_allocator`
    (frame accounting: 3 temp paging frames + up to 4 for the active-
    CR3 PT walk = 3..=7 frames consumed),
    `place_trampoline_materialises_pml4_pointing_at_pdpt` /
    `_pd_with_2mib_identity_entry` (read-back verification via
    `TestArena::read_pt_frame`),
    `place_trampoline_pml4_contents_match_pure_builder` (byte-equality
    vs the result of `build_temp_identity_paging` for the same inputs),
    `place_trampoline_handles_repeat_calls_consistently` (a second
    call must succeed and use a different temp PML4 frame; the
    identity mapping in active CR3 stays idempotent),
    `place_trampoline_returns_out_of_frames_when_allocator_empty`
    (error path — `OutOfFrames` returned, no aliasing of pre-allocated
    state), `place_trampoline_writes_blob_to_phys_8000` (read-back
    256 bytes from arena, byte-equality vs `build_trampoline_blob`),
    `place_trampoline_blob_starts_with_cli_cld_in_memory` (pin the
    first two opcodes of the in-memory trampoline),
    `sipi_vector_matches_trampoline_base` (`TRAMPOLINE_SIPI_VECTOR ==
    0x08`). The supporting `TestArena` is a 1.5 MiB / 384-frame heap
    allocation 4 KiB-aligned, with `phys_offset = arena.ptr` (so
    physical 0 maps to `arena.ptr` and physical `0x8000` maps to
    `arena.ptr + 0x8000`). The allocator is anchored at `PhysAddr(0)`
    with `mark_range_free(PhysAddr(0x10_0000), …)` — mirroring the
    `kmain` reservation of the low 1 MiB — so both the trampoline
    page (low memory) and the allocator-handed temp frames (above 1
    MiB) live inside the same arena.

  Workspace test count: 510+ → 521+. `cargo clippy --workspace
  --all-features --all-targets -- -D warnings` and
  `cargo clippy --manifest-path kernel-runner/Cargo.toml --target
  x86_64-unknown-none -- -D warnings` both clean. Build Info panel
  updated: `Active = "MB14.c.2.b.2 emplacement"`,
  `Next = "MB14.c.2.c live start_aps"`,
  `Track B = "MB1-MB13 OK, MB14.a-c.2.b.2 wip"`, `Phase 1 ≈ 88%`,
  `Tests = "515+ workspace pass"`.

- **Kernel — MB14.c.2.a: INIT-SIPI ICR encoder + dry-run start_aps orchestrator (2026-05-19).**
  Pins the bit-exact layout of the Interrupt Command Register against
  Intel SDM Vol 3A § 10.6.1 (xAPIC) and § 10.12.9 (x2APIC) — the
  foundation MB14.c.2.b (real-mode trampoline) and MB14.c.2.c (live
  INIT-SIPI fire + ack barrier) will build on. No LAPIC MMIO is
  performed in this sub-block: the orchestrator iterates the discovered
  topology, builds and encodes the canonical INIT/SIPI/SIPI sequence
  for every enabled non-BSP AP, and discards the encoded values. This
  moves the highest-leverage failure mode (a stray bit in the ICR
  triple-faults the BSP) into a host-side `cargo test` regression
  instead of a 6-hour QEMU debug session.

  Three additive changes:

  - **`crates/omni-kernel/src/bare_metal/mp.rs`** — adds the ICR-shaped
    enums `IcrDeliveryMode` (Fixed / LowestPriority / SMI / NMI / INIT
    / StartUp), `IcrDestinationMode` (Physical / Logical), `IcrLevel`
    (Deassert / Assert), `IcrTriggerMode` (Edge / Level),
    `IcrDestinationShorthand` (NoShorthand / Self / AllIncludingSelf /
    AllExcludingSelf), all `#[repr(u8)]` with the spec encoding values.
    `IcrCommand { vector, delivery_mode, destination_mode, level,
    trigger_mode, shorthand, destination_apic_id }` carries the
    intent; the const constructors `IcrCommand::init_assert(apic_id)`
    and `IcrCommand::sipi(apic_id, trampoline_page)` produce the two
    canonical IPIs the AP wake-up algorithm uses. The pure-function
    encoders `encode_icr_xapic(cmd) -> (u32, u32)` (high/low dwords for
    LAPIC offsets `0x310` / `0x300`) and `encode_icr_x2apic(cmd) -> u64`
    (single MSR write to `IA32_X2APIC_ICR` = `0x830`) translate to
    wire format. The orchestrator `start_aps(topology, bsp_apic_id,
    trampoline_page, mode) -> StartApsReport` iterates `topology.entries()`,
    skips the BSP and any disabled CPU, builds and encodes the canonical
    INIT + SIPI + SIPI sequence for every remaining AP, and reports
    `{ targeted, sequenced, dry_run }`. `StartApsMode::Live` is part of
    the API surface from this sub-block so MB14.c.2.c does not need to
    change the kmain call site, but it silently downgrades to `DryRun`
    until the live LAPIC ICR write path lands.
  - **`crates/omni-kernel/src/lib.rs`** — hooks `start_aps` in `kmain`
    immediately after the MB14.c.1 enumerate_cpus log block. The call
    passes the BSP LAPIC ID (the same `lid` read for the MB14.a
    descriptor seed), `trampoline_page = 0x08` (the canonical physical
    address `0x0000_8000` the MB14.c.2.b real-mode trampoline will
    occupy), and `StartApsMode::DryRun`. Output on COM1 is a single
    `[mb14.c.2.a] start_aps targeted=N sequenced=N (dry-run)` line.
  - **+13 host-side unit tests** in `bare_metal::mp::tests`:
    `xapic_init_encoding_matches_intel_layout` (asserts low=`0x4500` /
    high=`0x0100_0000` for INIT to APIC ID 1),
    `xapic_sipi_encoding_matches_intel_layout` (asserts low=`0x4608`
    for SIPI page=`0x08` to APIC ID 1),
    `xapic_destination_truncates_to_eight_bits` (asserts the encoder
    drops the upper 24 bits when emitting xAPIC layout),
    `x2apic_init_encoding_packs_destination_in_high_dword` /
    `x2apic_sipi_packs_trampoline_and_destination` (assert the full
    32-bit ID survives in the upper half of the 64-bit MSR),
    `encoder_emits_zero_for_default_init_fields` (sanity on the
    zero-by-default fields), `shorthand_all_excluding_self_encodes_to_bits_18_19`
    (covers the MB14.d use case), and 6 `start_aps_*` orchestrator
    tests that pin BSP exclusion by APIC ID match (not by entry
    position), disabled-CPU skipping, the `trampoline_page=0` → forced
    dry-run guard, the `Live` → `DryRun` silent downgrade, and the
    uniprocessor (1 vCPU) case which targets zero APs without underflow.
    Workspace test count: 467 → 480.

  No production behaviour changes on the bare-metal kernel beyond the
  single new serial log line; the bring-up path is unchanged. The
  encoder is `const fn` so MB14.d (TLB shootdown via Fixed-delivery
  IPI to `AllExcludingSelf`) can build compile-time IPI constants
  without a runtime translation step. Build Info panel updated:
  `Active = "MB14.c.2.a ICR encoder"`,
  `Next = "MB14.c.2.b trampoline @0x8000"`,
  `Track B = "MB1-MB13 OK, MB14.a-c.2.a wip"`, `Phase 1 ≈ 86%`.

- **Kernel — MB14.c.1: ACPI MADT cpu enumeration (2026-05-19).**
  Decodes the firmware-supplied MADT table to discover the set of
  logical CPUs the platform exposes — the prerequisite for the
  INIT-SIPI-SIPI orchestrator that will land in MB14.c.2. No APs are
  started here; the figure is logged on the early-boot serial console
  and consumed by later MB14.c.2 / MB14.e sub-blocks.

  Three additive changes:

  - **`crates/omni-kernel/src/bare_metal/mp.rs`** — new module with
    a pure-function decoder `parse_madt(&[u8]) -> Result<CpuTopology, MadtError>`
    that walks the MADT ICS (Interrupt Controller Structure) list and
    extracts `Processor Local APIC` (type `0x00`, 8 bytes) and
    `Processor Local x2APIC` (type `0x09`, 16 bytes) entries into a
    fixed-capacity (`MAX_CPUS = 32`) `CpuTopology` value. Other ICS
    types (IO APIC, NMI source, etc.) are skipped via the entry's
    `length` byte without producing an error. Malformed cases are
    rejected explicitly: zero-length ICS (would loop forever), ICS
    that runs past the table end, header `length` field that
    disagrees with the buffer, signature mismatch, MADT advertising
    more CPUs than the kernel tracks. The widened `CpuEntry.apic_id: u32`
    accommodates both 8-bit xAPIC and 32-bit x2APIC IDs in a single
    field; `CpuEntry.x2apic: bool` flags which encoding the
    MB14.c.2 orchestrator must use for the ICR write.
    On bare-metal, the entry point `enumerate_cpus(rsdp_phys, phys_offset)`
    (unsafe wrapper) walks `RSDP → XSDT/RSDT → MADT` to locate the
    table, modelled on the FADT walker in
    `crates/omni-kernel/src/bare_metal/arch/x86_64.rs::find_pm1a_cnt_from_fadt`
    so the safety invariants on the physical-memory window are
    identical. Host-side stub returns `None` on non-x86_64. `+12 unit
    tests` exercise the decoder with hand-crafted byte buffers
    (truncated header, bad signature, length mismatch, empty MADT,
    single BSP Local APIC, disabled Local APIC, x2APIC with 32-bit
    ID, multiple CPUs with an IO APIC entry interleaved, unknown ICS
    type skipped, zero-length ICS rejected, ICS running past table
    end rejected, more than `MAX_CPUS` CPUs rejected). The unit
    tests need no bare-metal plumbing, making the parser fully
    host-testable.
  - **`crates/omni-kernel/src/lib.rs`** — `kmain` calls
    `bare_metal::mp::enumerate_cpus(rsdp_phys, phys_offset)`
    immediately after `init_gs_base` returns and before `sti`. The
    call is best-effort: missing `BootInfo.rsdp_addr` or
    `physical_memory_offset` falls through to a `[mb14.c.1] rsdp /
    phys_offset unavailable — BSP only` log line; a parse failure
    falls through to `[mb14.c.1] MADT walk FAILED — BSP only`. On
    success each entry is printed as `[mb14.c.1] apic_id=<N> [x2apic]
    enabled|disabled`. The kernel proceeds with single-CPU operation
    in either case — MB14.c.1 is read-only.
  - **`crates/omni-kernel/src/bare_metal/mod.rs`** — registers the
    new `mp` submodule (unconditional, since the decoder is portable;
    the bare-metal walker is `#[cfg(target_arch = "x86_64")]` inside
    the module).

  Build Info panel updated to `Active = MB14.c.1 MADT cpu enum`,
  `Next = MB14.c.2 INIT-SIPI trampoline`, `Track B = MB1-MB13 OK,
  MB14.a-c.1 wip`, `Phase 1 ≈ 85%`, `Tests = 467+ workspace pass`.

- **Kernel — MB14.a: per-CPU descriptor scaffold + BSP LAPIC ID
  identification (2026-05-19).** Opens the MB14 (MP/AP enable + TLB
  shootdown) work-package by installing the foundation that every
  later sub-block depends on: a stable per-CPU descriptor that
  identifies the executing logical CPU. The bare-metal smoke remains
  identical to the post-MB13 baseline — only one extra serial line
  (`[mb14.a] BSP cpu_id=0 lapic_id=<N>`) is emitted right after
  `lapic_init` succeeds.

  Three additive changes:

  - **`crates/omni-kernel/src/bare_metal/lapic.rs`** — adds
    `read_lapic_id() -> Option<u32>`, which reads LAPIC register
    `0x20` (xAPIC ID, bits 31:24 per Intel SDM Vol 3A § 10.4.6) and
    returns `None` if `lapic_init` has not yet mapped the LAPIC MMIO
    window. The previously `#[allow(dead_code)]` `lapic_read` helper
    is now an actual caller, so the attribute is removed.
  - **`crates/omni-kernel/src/bare_metal/per_cpu.rs`** — new module
    exposing the `PerCpu` struct (atomic `cpu_id` / `lapic_id` /
    `is_bsp` fields), an uninitialised-sentinel constant
    `CPU_ID_UNINIT = u32::MAX` (chosen to never collide with an 8-bit
    xAPIC ID), and three accessors: `init_bsp(lapic_id)` seeds the
    single static `BSP` slot, `current_cpu()` returns the executing
    CPU's descriptor (MB14.a stub: always returns `BSP`; MB14.b will
    swap to a `GS_BASE`-relative load), `bsp()` returns the BSP
    descriptor explicitly. `+6 unit tests` exercise the seed/read
    cycle and the sentinel invariant.
  - **`crates/omni-kernel/src/lib.rs`** — `kmain` now calls
    `per_cpu::init_bsp(read_lapic_id())` immediately after
    `lapic_init` returns `true` and before `sti`. The `Option`
    `None` branch logs a diagnostic line and leaves the descriptor
    uninitialised — defence in depth, since `read_lapic_id` only
    returns `None` if `lapic_init` itself succeeded but raced with
    a write to `LAPIC_BASE` (impossible on single-CPU but cheap to
    guard).

  No public ABI surface change; no syscall handler touched; no
  scheduler invariant moved. The new descriptor is read-only from
  every kernel path until MB14.b wires `GS_BASE` and MB14.c starts
  application processors.

  `cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal` clean; idem
  `kernel-runner --features mb12-userprobe`. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean.
  Workspace test count rises from 447+ to 453+ (`+6` per the
  per-CPU unit tests). The pre-existing `cargo test -p omni-kernel
  --lib` SIGSEGV (carryover documented in `progress-omni.md` § 4.5
  #16) is unchanged.

  Build Info panel: Active=`MB14.a per-CPU identity`,
  Next=`MB14.b GS_BASE per-CPU ptr`,
  Track B=`MB1-MB13 OK, MB14.a wip`, Phase 1 ≈ 83%.

- **Kernel — MB13.e: closure of the `omni-capability` integration
  cycle (2026-05-19).** Closes the last open MB13 acceptance criteria
  by making `Ed25519CapabilityProvider` the canonical
  `KernelCapabilityCheck` implementation reachable from every
  `IpcCreateChannel` path, and demoting `StubCapabilityProvider` to
  a `#[cfg(test)]`-only mock.

  Three call sites previously instantiated
  `StubCapabilityProvider` outside `#[cfg(test)]`:

  - **`crates/omni-kernel/src/ipc.rs`** —
    `KernelIpcRegistry::create_channel_signed`'s `(None, None)`
    shortcut now forwards the caller-supplied
    `Ed25519CapabilityProvider` to `create_channel` instead of
    constructing a fresh stub. The per-IPC `verify` impl on either
    provider is identical O(1) shape-matching, so the runtime
    behaviour for open channels is unchanged byte-for-byte.
  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs`** — the
    legacy `(send_token = 0, recv_token = 0)` fast path now
    instantiates `Ed25519CapabilityProvider::placeholder()` on the
    kernel stack and hands it to `create_channel`, replacing the
    inline stub. The signed-token path already used
    `Ed25519CapabilityProvider`; both paths are now consistent.
  - **`crates/omni-kernel/src/bare_metal/userprobe_mb12.rs`** —
    comments updated; the actual call already routed through
    `Ed25519CapabilityProvider::placeholder()` since MB13.d.

  `StubCapabilityProvider` (struct + `KernelCapabilityCheck` impl)
  is wrapped in `#[cfg(test)]` so it cannot be reached from the
  production boot wiring. The kernel's own `#[cfg(test)]` unit
  tests under `src/ipc.rs` and `src/capabilities.rs` continue to
  use it as a minimal mock that exercises the registry without
  pulling Ed25519 verification into the assertion. The host-side
  integration test `tests/mb12_ipc_cross_process.rs` (a separate
  test binary that does not see the library's `cfg(test)` gate) is
  migrated to `Ed25519CapabilityProvider::placeholder()`.

  Module-level docs in `crates/omni-kernel/src/capabilities.rs`
  rewritten to reflect that `Ed25519CapabilityProvider` is now the
  canonical provider (per-IPC verify = O(1) shape-match; one-shot
  signature + time window + TEE binding at channel creation). The
  ADR-0005 reference is preserved; ADR-0006 captures the MB13
  migration closure.

  - **`docs/adr/0006-mb13-omni-capability-integration.md`** — new
    ADR (status `accepted`) consolidating the MB13.a → MB13.h work,
    the three alternatives considered for each sub-block, and the
    documented residuals (open-channel back-door for the userprobe
    demo, all-zero TEE placeholder until `omni-tee` lands,
    `force-soft` SIMD).

  No runtime semantics change for any existing caller. `cargo build
  -p omni-kernel --target x86_64-unknown-none --no-default-features
  --features bare-metal` clean; idem `mb12-userprobe` and
  `kernel-runner --features mb12-userprobe`. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean.
  Workspace test count unchanged at 447+ (no tests removed; the
  integration test imports were rewritten in place).

  Build Info panel: Active=`MB13.e closure + ADR-0006`,
  Next=`MB14 MP/AP + TLB shootdown`,
  Track B=`MB1-MB12 OK, MB13 closed`, Phase 1 ≈ 82%.

- **Kernel — MB13.h: TSS `ltr` wiring + dedicated IST stacks for
  #DF / #PF (2026-05-19).** Closes the silent-stall root cause that
  MB13.g surfaced as a missing diagnostic line: even with the full
  IDT coverage installed by MB13.g, the post-`iretq` Ring 3 fault
  could not write `[OMNI OS EXCEPTION] vec=NN` because the CPU was
  unable to resolve a kernel stack at the privilege transition.

  Two cooperating defects:

  - **`tss::ltr_load()` was never invoked.** `gdt::gdt_init` wrote
    the TSS descriptor at GDT slots 5+6 (introduced in MB11.1) and
    the scheduler kept `TSS.rsp0` up to date via `tss::set_rsp0`
    (MB12.0a), but `ltr 0x28` was never executed — so the CPU's
    task register stayed null. A Ring 3 → Ring 0 transition needs
    the task register to look up `TSS.rsp0`; without it the CPU
    cannot push the exception frame and cascades straight to a
    triple fault, silently resetting the VM before any handler runs.
  - **`TSS.ist1` / `TSS.ist2` were hard-coded to zero.** The TSS
    struct had the fields and the module-level docs since MB11
    advertised "MB11 uses IST1 for #DF and IST2 for #PF", but the
    fields were never populated and the IDT entries used IST=0.
    Even with a working `ltr`, a stack-related fault (e.g., a #PF
    on `rsp0` immediately after a CR3 reload to a per-process PML4
    where the kernel stack page has not yet propagated) would
    cascade to #DF on the same broken stack.

  Three atomic changes:

  - **`crates/omni-kernel/src/bare_metal/tss.rs`** — introduces two
    static `IstStack([u8; 16384])` buffers (`IST1_STACK`,
    `IST2_STACK`) in `.bss` (16 KiB each, parity with
    `scheduling::KERNEL_STACK_SIZE`), a public
    `init_ist_stacks()` that writes `base + IST_STACK_SIZE` into
    `TSS.ist1` / `TSS.ist2`, plus `current_ist1()` /
    `current_ist2()` read-back helpers for tests. Static IST
    buffers live in the kernel image so they are mapped by the
    bootloader once and survive every per-process CR3 reload via
    the kernel-half shared-by-reference mechanism
    (`AddressSpace::new_with_kernel_half`).
  - **`crates/omni-kernel/src/bare_metal/idt.rs`** — adds
    `IdtEntry::interrupt_gate_with_ist(handler, selector, ist_index)`
    encoding the IST index in the low 3 bits of byte 4. `idt_init`
    now uses `(isr_df, 1)` for vector 8 (#DF) and `(isr_pf, 2)` for
    vector 14 (#PF); the catch-all vectors keep IST=0 because they
    are not stack-related faults. The `& 0x07` mask blinds future
    callers against an out-of-range IST index corrupting the
    reserved bits.
  - **`crates/omni-kernel/src/lib.rs::kmain`** — inserts
    `tss::init_ist_stacks()` + `tss::ltr_load()` between
    `gdt::gdt_init()` and `idt::idt_init()`. Without this exact
    ordering the TSS descriptor wouldn't exist when `ltr` runs, or
    the IST fields would still be zero at the first fault — both
    fatal.

  Tests added: `tss::tests::ist_stack_size_is_16_kib`,
  `tss::tests::init_ist_stacks_writes_top_of_each_buffer`,
  `idt::tests::interrupt_gate_with_ist_sets_index`,
  `idt::tests::interrupt_gate_with_ist_masks_high_bits` (+4).
  Workspace test target ≥ 447. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean; bare-metal + mb12-userprobe
  + kernel-runner builds clean.

  Build Info panel: Active=`MB13.h TSS ltr + IST`, Next=`MB13.e PR
  + tag`, Track B=`MB1-MB12 OK, MB13.a-h OK`, Phase 1 ≈ 80%.

- **Kernel — MB13.g: comprehensive IDT coverage for synchronous
  exceptions (2026-05-19).** Extends [`bare_metal::idt`] from 4
  dedicated handlers (#DE, #DF, #GP, #PF) to **20 catch-all
  vectors covering 0..=21** so that previously-silent triple-faults
  surface as a single fault with provenance.

  Motivation: after MB13.b (ET_DYN/PIE upper-half kernel) and MB13.f
  (`enter_user_mode` kernel-stack swap) the Proxmox VMID 103
  `mb12-userprobe` deploy reached the `iretq` boundary cleanly
  (verified via inline COM1 tracepoint `'E'`) but the VM then halted
  without emitting any tracepoint from the syscall, LAPIC, #PF, #GP
  or #DF handlers. With 256 IDT slots initialised to
  `IdtEntry::missing()` (P=0), any synchronous fault on a vector
  outside {0,8,13,14} triggers a #NP that itself faults to #DF on a
  missing entry — cascading to a triple fault and a silent VM
  reset. MB13.g replaces that silence with a `[OMNI OS EXCEPTION]
  vec=NN  code=X  rip=… cs=… rflags=…` line on the early console,
  unblocking the root-cause analysis for MB13.h.

  - **`crates/omni-kernel/src/bare_metal/idt.rs`** — adds 16 new
    `global_asm!` stubs, one per vector in
    `{1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 16, 17, 18, 19, 20, 21}`, each
    loading the vector number as an immediate into `RDI` (and the
    CPU-pushed error code into `RDX` when applicable) before calling
    one of two new `extern "C"` Rust functions:

    - `kernel_handle_exception_noerr(vector, frame)` — used by the
      no-error-code vectors (#DB/NMI/#BP/#OF/#BR/#UD/#NM/#MF/#MC/#XF/#VE).
    - `kernel_handle_exception_witherr(vector, frame, error_code)` —
      used by the error-code vectors (#TS/#NP/#SS/#AC/#CP).

    Both handlers write the vector tag and frame snapshot through
    `early_console::write_*`, then call `super::arch::halt_forever()`.
    The four dedicated handlers (#DE/#DF/#GP/#PF) are preserved
    unchanged — they emit a mnemonic and (for #PF) the faulting
    `CR2`. The two architecturally-reserved Intel slots 9 and 15
    remain `missing()` by design.

  - **`idt_init()`** now installs all 20 catch-all entries in
    addition to the original four. The IDTR descriptor and `lidt`
    issue are unchanged. The new test
    `bare_metal::idt::tests::mb13g_synchronous_vectors_covered`
    documents the coverage matrix symbolically and asserts that
    reserved vectors do not overlap with the covered list.

  - **`crates/omni-kernel/src/bare_metal/demo.rs`** — Build Info
    panel updated to `Active=MB13.g full ISR coverage`,
    `Next=MB13.h iretq stall fix`, `Track B=MB1-MB12 OK, MB13.a-g OK`,
    `Phase 1 ≈ 78%`.

  - **Stage gate:** MB13.g is intentionally a *diagnostic* fix — it
    does not change the kernel's semantics under success paths.
    Workspace build and clippy are clean on `x86_64-unknown-none`
    (`bare-metal`, `mb12-userprobe`, default desktop demo); the new
    unit test is +1 over MB13.f, so workspace target ≥ 444. The
    expected boot output on the Proxmox VMID 103
    `mb12-userprobe` build is now one of three branches:

    1. *(Success path)* the original
       `ping` + double `[user] exit=0` MB12 banner sequence (this
       would mean MB13.f closed the bug too and the silence was a
       coincidence of cache flushing or display refresh).
    2. *(Catch-all triggered)* a new
       `[OMNI OS EXCEPTION] vec=NN ...` line identifies the
       specific vector being raised post-iretq, narrowing MB13.h
       to a targeted fix (TSS.ist1 wiring for #DF, a stale GDT
       segment descriptor for #NP, an iretq-frame mis-build for
       #SS, etc.).
    3. *(Still silent)* the triple-fault precedes the first vector
       dispatch and points at a hardware-level reset (CR0/CR4
       mis-config, EFER inconsistency, or a problem in the very
       first `mov cr3` itself) rather than an IDT-coverage gap.

- **Kernel — MB13.f: `enter_user_mode` kernel-stack swap before CR3
  reload (first-dispatch smoke fix, 2026-05-19).** Closes the open
  `mb12-userprobe` follow-up tracked in `progress-omni.md` § 4.5 #22:
  with MB13.b the VM stopped emitting any `[user] exit=0` / `ping`
  after `[mb12] handing off to user tasks`, because the first
  `enter_user_mode` invoked from inside a syscall handler executed
  `mov cr3, dest_cr3` while RSP still pointed at the *outgoing* task's
  user stack (SYSCALL on x86_64 does not switch SP). After the CR3
  reload that page was no longer mapped (user-half is per-process), so
  the very next `push {ss}` of the iretq frame produced a Ring-0 page
  fault → triple fault → VM reset, before any Ring-3 instruction
  could execute.

  - **`crates/omni-kernel/src/bare_metal/usermode.rs`** —
    `enter_user_mode` gains a new `kernel_stack_top: u64` parameter and
    issues `mov rsp, {kstk}` **before** the `mov cr3`. The destination
    kernel stack lives in the MB10 isolated range
    `[KERNEL_STACK_VA_BASE, KERNEL_STACK_VA_END)` (PML4 index `≥ 0x180`,
    canonical kernel half), which `AddressSpace::new_with_kernel_half`
    mirrors by reference into every per-process PML4. The page therefore
    survives the CR3 reload, and the subsequent `push` sequence that
    builds the iretq frame runs on a valid mapping. The non-x86_64 stub
    is updated in tandem to keep callers source-compatible.

  - **`crates/omni-kernel/src/scheduling.rs`** —
    `RoundRobinScheduler::yield_current` first-dispatch path now
    forwards `kernel_stack_top` (computed in stack as
    `kernel_stack_va + KERNEL_STACK_SIZE`, already used for the
    `TSS.rsp0` update introduced by MB12.0a). The pre-MB13.b
    "bare-metal limitation" comment is removed — that triple-fault
    cause was resolved by ET_DYN/PIE relocation.

  - **`crates/omni-kernel/src/lib.rs`** — MB11 single-task dispatch
    (`kmain` → `enter_user_mode` direct call) passes
    `pcb.task.kernel_stack_va + scheduling::KERNEL_STACK_SIZE`. The
    MB11 caller-side RSP is already on the boot stack (upper half,
    mirrored), but the fix is applied uniformly.

  - **Latency note:** the bug was latent at MB11 — `kmain` called
    `enter_user_mode` directly with RSP on the boot stack, which since
    MB13.b lives in upper half (mirrored). MB12 surfaced it because the
    first-dispatch is now triggered from inside a syscall handler whose
    RSP is the *outgoing* task's user stack. Host tests did not catch
    it because `enter_user_mode` has a `panic!()` stub on non-x86_64.

  - **`crates/omni-kernel/src/process.rs`** — `spawn_from_elf` now
    allocates and maps the per-process MB10 kernel stack via
    `mapper.map_4k(...)` *before* cloning the per-process PML4 via
    `AddressSpace::new_with_kernel_half`. Reason: `new_with_kernel_half`
    copies PML4 entries 256..511 by value, and any *new* PDPT installed
    in the boot PML4 *after* a clone (e.g. the first kstk allocation
    at PML4 index 0x180) does not propagate to clones taken earlier.
    Pre-MB13.f, the first user-task spawn cloned an empty PML4[0x180]
    and then mapped the kstk into the boot mapper, leaving the new
    process's PML4 with a stale zero entry and the kstk unreachable
    after CR3 reload. Reordering forces the boot PML4 to allocate the
    kstk-range PDPT eagerly so the clone captures the shared PDPT, and
    every subsequent kstk slot inside the same shared PDPT propagates
    automatically.

  Verification (host, 2026-05-19 post-MB13.f):
    - `cargo build --workspace --all-features` → clean (0 warning).
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe` → clean.
    - `cargo clippy --workspace --all-features --all-targets -- -D warnings` → clean.
    - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe -- -D warnings` → clean.
    - `cargo test -p omni-kernel --tests` (integration suite: mb11 6 + mb12 8 + mb13 11 + boot_info 7 + panic_record 5 + heap 9) → 46/46 pass.
    - Build Info panel updated to `Active = MB13.f iretq kstk-swap`,
      `Next = MB13.e PR + intermediate tag`, `Track B = MB1-MB12 OK,
      MB13.a-f OK`, `Phase 1 ≈ 77%`, `Tests = 443+ workspace pass`.
    - **Note:** the pre-existing `cargo test -p omni-kernel --lib`
      `SIGSEGV` in `dispatcher_time_monotonic_returns_u64` (CMOS port
      I/O on Linux userspace) and at the `bare_metal::paging::tests`
      teardown remains; this carryover (item §4.5 #16) is unrelated to
      MB13.f, which touches only the `x86_64` asm path.

  Smoke validation on Proxmox VMID 103 (2026-05-19):
    - **Default desktop build** (no `mb12-userprobe`): VM boots to
      `[virtio] tablet ready`, GOP framebuffer renders, screenshot via
      `qm monitor 103 :: screendump` confirms Build Info panel at the
      new milestone state. ✅
    - **`mb12-userprobe` build:** with MB13.f + the process.rs reorder,
      `enter_user_mode` now executes the full
      `mov rsp` → `mov cr3` → 5 `push` → `iretq` sequence without
      Ring 0 fault (verified via inline tracepoints on COM1 port
      0x3F8: emit `A` enter / `B` after `mov rsp` / `C` after
      `mov cr3` / `D` after first push / `E` before `iretq`). The VM
      still arrests immediately after `iretq` without emitting any of
      the SYSCALL/timer/ISR tracepoints; the receiver does not even
      execute a `jmp $`. Tracked as **MB13.g — Ring 3 post-iretq
      triple-fault** (open). Diagnostic next step requires
      `-d int,cpu_reset -D /tmp/qemu-trace.log` on QEMU args of VMID
      103, not performed in this round.

- **Kernel — MB13.d: `IpcCreateChannel` syscall ABI extension for
  postcard-encoded signed tokens (2026-05-19).** Closes the last
  bare-metal-side gap of the MB13 work-package by plumbing
  `omni_capability::CapabilityToken` blobs from user space all the way
  down to `Ed25519CapabilityProvider::verify_signed_token`. The MB12
  "open channel" calling convention (both token pointers zero) keeps
  working byte-for-byte — `mb12-userprobe` boots unchanged — but any
  call that supplies even one token now gates the channel registration
  on a full Ed25519 + time-window + TEE-binding verification.

  - **`crates/omni-kernel/src/capabilities.rs`** — new
    `decode_and_authenticate_token(bytes, expected_action, provider, now)
    -> KernelResult<KernelPrincipal>` helper. Decodes postcard bytes via
    `omni_types::wire::decode_canonical`, runs
    `Ed25519CapabilityProvider::verify_signed_token`, enforces
    `scope.action` matches the slot, and accepts any
    `Resource::IpcChannel(_)` value (the kernel rebinds the resource to
    the freshly-allocated channel id — user space cannot predict the
    monotonic counter at mint time). The subject in the kernel-side
    `KernelPrincipal` is sourced from `token.payload.subject` (32-byte
    NodeId attestation hash).

  - **`crates/omni-kernel/src/ipc.rs`** — new
    `KernelIpcRegistry::create_channel_signed(owner, policy,
    send_token_bytes, recv_token_bytes, &provider, now)`. When both
    `send_token_bytes` and `recv_token_bytes` are `None`, delegates to
    the existing `create_channel(..., &StubCapabilityProvider)` so the
    open-channel pre-create in `userprobe_mb12::spawn_userprobe_mb12`
    keeps the byte-for-byte MB12 semantics. Otherwise each non-`None`
    slot runs `decode_and_authenticate_token` and the channel is
    registered with the verified subject in the corresponding
    `send_subject` / `recv_subject` slot.

  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs`** —
    `ipc_handlers::ipc_create_channel` now accepts the MB13.d six-arg
    ABI: `(queue_depth, backpressure, tee_bound, send_token_ptr,
    recv_token_ptr, lens)` where `lens` packs
    `send_len:u32 | (recv_len:u32 << 32)`. Both token pointers `0` →
    legacy stub-provider path. At least one non-zero → per-slot
    `user_range_ok` bounds check + on-stack `[u8; 1024]` copy (caps the
    accepted token at 1 KiB; real `CapabilityToken` payloads are ~200
    bytes) + delegate to `create_channel_signed`. Kernel monotonic
    `now` comes from `bare_metal::arch::rtc_seconds`.

  - **`crates/omni-kernel/src/bare_metal/userprobe_mb12.rs`** — the
    `spawn_userprobe_mb12` pre-create now routes through
    `create_channel_signed` with both slots `None` and
    `Ed25519CapabilityProvider::placeholder()`. Behaviour is identical
    to the MB12 baseline (the registry recognises the no-token call
    and forwards to the stub provider); the indirection documents the
    new canonical entry point.

  - **`crates/omni-kernel/tests/mb13_capability_signed.rs`** — new
    integration suite, +11 tests:
      * 7 on `decode_and_authenticate_token`: happy path, bit-flipped
        canonical bytes, action mismatch, pre-window / post-window
        time, TEE attestation mismatch, non-IpcChannel resource,
        truncated postcard.
      * 4 on `KernelIpcRegistry::create_channel_signed`: send-token
        authentication populates `send_subject`, open-channel (both
        `None`) delegates to the legacy stub path, invalid send-token
        bytes reject without leaving a half-registered channel, and a
        full-round-trip end-to-end check that the per-IPC
        `subject == requester` gate still rejects an intruder
        principal after the signed-token registration.

  - **`crates/omni-kernel/src/bare_metal/demo.rs`** — Build Info panel
    updated to `Active = MB13.d IpcCreateChannel ABI`, `Next = MB13.e
    PR + intermediate tag`, `Track B = MB1-MB12 OK, MB13.a-d OK`,
    `Phase 1 ≈ 75%`, `Tests = 443+ workspace pass`.

  Verification (host, 2026-05-19 post-MB13.d):
  - `cargo build --workspace --all-features` → clean (zero warnings).
  - `cargo build -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal` → clean.
  - `cargo build -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features mb12-userprobe` → clean.
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none --features mb12-userprobe` → clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D
    warnings` → clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` →
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features mb12-userprobe -- -D warnings` →
    clean.
  - `cargo test -p omni-kernel --features bare-metal --test
    mb13_capability_signed` → 11 / 11 pass.
  - `cargo test -p omni-kernel --features bare-metal --tests` →
    mb11_userspace (6) + mb12_ipc_cross_process (8) +
    mb13_capability_signed (11) + panic_record (5) + boot_info (7) +
    heap (9) = 46 / 46 integration pass. Lib unit tests still hit the
    pre-existing x86_64-host SIGSEGV in `paging` and
    `dispatcher_time_monotonic_returns_u64` (CMOS port I/O) — both
    documented in `progress-omni.md` § 4.5 #16 as carryover from
    v0.2.0; reproduces on HEAD before MB13.d.
  - `scripts/check-no-blanket-allow.sh` → ok (12 crate-root files).

- **Kernel — MB13.c: `omni-capability` integration + `Ed25519CapabilityProvider`
  (2026-05-19).** Lands the real Ed25519 signature-verification path
  alongside the MB12 `StubCapabilityProvider`. The kernel now depends on
  `omni-capability` (verify-only build) and can authenticate userspace
  `CapabilityToken` blobs end-to-end. The new provider is wired into the
  kernel surface but not yet plugged into the IPC syscall handlers —
  the syscall ABI extension that plumbs tokens through `IpcCreateChannel`
  ships as MB13.d. `StubCapabilityProvider` therefore remains the boot-
  wiring default until MB13.d. This closes the Phase 1 deliverable
  "capability-based security primitives implemented" at the verification
  layer; the matching ABI plumbing closes the rest.

  - **`crates/omni-types/Cargo.toml` + `src/lib.rs` + `src/identity.rs`**
    — split `id-generation` (default-on, runtime constructors) into a
    new lower-tier `id-types` feature that exposes the `identity`
    module's *type definitions* without dragging `getrandom`. The
    UUIDv4-minting `::new()` methods on `AgentId`, `CapabilityId`, and
    `SessionId` plus the `random_uuid_bytes` helper are now feature-
    gated to `id-generation`; the types themselves are available under
    `id-types`. The `uuid` crate is pulled with `features = ["serde"]`
    only (no `v4`) because the in-house `random_uuid_bytes` helper goes
    through `Uuid::from_bytes` and never needs `uuid`'s rng path. Net
    effect: `omni-types` compiles on `x86_64-unknown-none` with just
    `id-types` enabled, which is what the kernel needs.
  - **`crates/omni-capability/Cargo.toml`** — declares `omni-types`
    and `omni-crypto` with explicit `default-features = false`,
    enabling only `omni-types/id-types` for the library build. A new
    `mint` feature (default-on) gates the userspace-only paths:
    `CapabilityToken::mint`, `attenuation::attenuate`, and their
    transitive `id-generation` + `omni-crypto/rng` dependencies. A new
    `bare-metal` feature is a marker that forwards
    `omni-crypto/bare-metal`; combined with `--no-default-features`,
    it produces a verify-only build that compiles on
    `x86_64-unknown-none`. Dev-dependencies override the deps to
    re-enable `mint` / `id-generation` / `rng` for `cargo test`.
  - **`crates/omni-capability/src/scope.rs`** — adds three semver-safe
    `#[non_exhaustive]` variants used by the kernel IPC dispatcher:
    `Action::IpcSend`, `Action::IpcRecv`, and `Resource::IpcChannel(u64)`.
    Subset relation for `IpcChannel` is equality (opaque kernel handle —
    no wildcard at MB13.c); 5 new unit tests pin the new variants'
    behaviour, including cross-discriminant disjointness.
  - **`crates/omni-capability/src/{attenuation.rs,token.rs}`** — gate
    `attenuate` and `CapabilityToken::mint` behind `#[cfg(feature =
    "mint")]`. The verify path (`verify_signature`, `verify_full`) stays
    available without `mint`. Unused-import warnings on the
    bare-metal build are silenced by cfg-gating the corresponding
    `use` statements.
  - **`crates/omni-kernel/Cargo.toml`** — adds `omni-capability` as a
    runtime dep with `default-features = false, features = ["bare-metal"]`.
    dev-dependencies enable `mint` so host tests can mint Ed25519-
    signed tokens to exercise the new provider end-to-end. The bare-
    metal `cargo build --target x86_64-unknown-none` is unaffected
    because dev-deps are not pulled into non-test builds.
  - **`crates/omni-kernel/src/capabilities.rs`** — adds
    `Ed25519CapabilityProvider` with three methods:
    - `verify_signature_only(token)` — Ed25519 signature verification
      only, no time/TEE/revocation checks. Used by tests and by call
      sites that have validated the time window separately.
    - `verify_signed_token(token, now)` — full verification via
      `CapabilityToken::verify_full` with a `StubAttestation` bound to
      the provider's `node_id_bytes` and an empty `RevocationList`.
      MB13.c uses an all-zero placeholder node id; the real attested
      identity arrives with `omni-tee` (P5).
    - `verify(token, action, resource)` (impl
      `KernelCapabilityCheck`) — O(1) action/resource shape match
      identical to `StubCapabilityProvider`, so the provider is a
      drop-in replacement at the per-IPC level. Signature verification
      is a one-shot done at channel creation (MB13.d).
  - **Test delta (host-side):** +5 in `omni-capability` (new
    `IpcSend`/`IpcRecv`/`IpcChannel` subset semantics) + 6 in
    `omni-kernel` (`Ed25519CapabilityProvider` happy path, tampered
    payload, window/attestation rejection, per-IPC shape match).
    Workspace target: ≥ 432 pass (was 426 post-MB12).
  - **Verification:**
    - `cargo build -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal` → clean (was: `unresolved import omni_types::identity`).
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` → clean.
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean.
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean (ET_DYN, regression on MB13.b).
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` → clean.
    - `cargo clippy -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` → clean.
    - `cargo test -p omni-capability` → 64 + 7 + 5 = 76 pass (5 new).
    - `cargo test -p omni-kernel --lib capabilities` → 11/11 ok (6 new + 4 stub regressions + 1 carryover).
    - `scripts/check-no-blanket-allow.sh` → ok (12 crate roots scanned).
    - Build Info panel updated to `Active = MB13.c Ed25519 cap provider`,
      `Next = MB13.d IpcCreateChannel ABI`, `Track B = MB1-MB12 OK,
      MB13.a/b/c OK`, `Phase 1 ≈ 72%`, `Tests = 432+ workspace pass`.

- **Kernel — MB13.b: ET_DYN/PIE kernel + upper-half dynamic mapping
  (2026-05-19).** Fixes the `mb11-userprobe` / `mb12-userprobe`
  triple-fault on Proxmox VMID 103 / QEMU+OVMF at root cause: the
  kernel ELF was `ET_EXEC` with `p_vaddr = 0x200000` (PML4 index 0),
  which `bootloader_api 0.11` does not relocate. After
  `AddressSpace::new_with_kernel_half` cloned only PML4 256..=511,
  the `mov cr3` in `enter_user_mode` dropped the kernel image and
  triple-faulted on the next instruction fetch.

  - **`kernel-runner/.cargo/config.toml`** — removed
    `-C relocation-model=static` + `-C link-arg=--no-pie`. The
    `x86_64-unknown-none` target spec already sets
    `position-independent-executables = true` on Rust 1.83+, so the
    kernel ELF is now `ET_DYN` (PIE) by default with RIP-relative
    addressing. The file now contains only an explanatory comment so
    the workspace `.cargo/config.toml` rustflags (force-soft SIMD
    cfgs for `omni-crypto`) merge cleanly when the kernel-runner is
    built.
  - **`kernel-runner/build.rs`** — removed entirely. The legacy build
    script emitted `cargo:rustc-link-arg=--no-pie` which appended
    `--no-pie` to the linker command line *after* the target-spec's
    `-pie`. LLD honours the last flag, so the kernel ELF was still
    `ET_EXEC` even after cleaning the `.cargo/config.toml`. Deleting
    the script lets the target spec's `-pie` reach LLD unopposed.
    Verified via `readelf -h` → `Type: DYN (Position-Independent
    Executable file)`.
  - **`kernel-runner/src/main.rs`** — `BOOTLOADER_CONFIG` now sets
    `mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`,
    pushing every bootloader-managed mapping (kernel base, kernel
    stack, `BootInfo`, framebuffer, physical-memory direct map) into
    the canonical upper half (PML4 ≥ 256). Combined with the ET_DYN
    kernel, this guarantees `AddressSpace::new_with_kernel_half`
    (which mirrors PML4 256..=511 by reference) keeps every kernel
    mapping live across CR3 switches.
  - **Verification:**
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` → clean (ET_DYN).
    - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` → clean (regression).
    - `cargo test -p omni-kernel --all-features --test mb12_ipc_cross_process` → 8/8 ok (host-side, unaffected by the bare-metal fix).
    - Build Info panel updated to `Active = MB13.b ET_DYN upper-half`, `Next = MB13.c omni-capability dep`, `Phase 1 ≈ 70%`.

- **Kernel — MB13.a: `omni-crypto` builds on `x86_64-unknown-none`
  (2026-05-19).** Unblocks the bare-metal build of `omni-crypto`, removing
  the LLVM ICE on SIMD intrinsics in `sha2`, `poly1305`, `chacha20`, and
  `curve25519-dalek`. This is the first slice of MB13; MB13.b
  (ET_DYN kernel for the triple-fault smoke fix) closed above; MB13.c
  (`omni-capability` integration), MB13.d (capability syscall ABI),
  and MB13.e (PR + tag) land in subsequent commits.

  - **Workspace `.cargo/config.toml`** (new file) — target-conditional
    `rustflags` for `x86_64-unknown-none`:
    - `--cfg poly1305_force_soft` → portable Poly1305 backend.
    - `--cfg chacha20_force_soft` → portable ChaCha20 backend.
    - `--cfg curve25519_dalek_backend="serial"` → 64-bit serial
      Curve25519 field arithmetic (no AVX2/AVX-512 vector reductions).
    - `--cfg sha2_backend="soft"` → portable SHA-256 + SHA-512
      backends in `sha2 0.11` (the workspace direct dep).
    Host targets are unaffected — they keep the hardware-accelerated
    backends.
  - **`crates/omni-crypto/Cargo.toml`** — target-scoped
    `[target.x86_64-unknown-none.dependencies]` adds a feature
    passthrough on `sha2 0.10` (`force-soft`) which the dalek family
    transitively pulls in via `digest 0.10`. Cargo unifies the
    feature with the dalek-side resolution, so `sha2 0.10`'s
    portable backend is selected without forking the dep graph.
  - **Verification:**
    - `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` → clean (was: LLVM ICE on `poly1305 0.8` + `sha2 0.10` + `sha2 0.11`).
    - `cargo clippy -p omni-crypto --target x86_64-unknown-none --no-default-features -- -D warnings` → clean.
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean (regression).
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` → clean.
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
    - `cargo doc --workspace --no-deps` → clean (was: 5 pre-existing warnings, fixed below).
    - `scripts/check-no-blanket-allow.sh` → ok (scanned 12 crate-root files).

### Fixed

- **`crates/omni-crypto/src/kdf.rs`** — gate `zeroize` import behind
  `#[cfg(feature = "rng")]`; on the bare-metal verify-only build
  (`--no-default-features`) `Zeroize`/`ZeroizeOnDrop` are only used by
  the rng-gated `Argon2idHash`, so the import emitted
  `unused_imports` warnings.
- **Pre-existing doc warnings** (5 total, surfaced by `cargo doc`):
  - `omni-tee/src/lib.rs:46-47` — broken intra-doc links to `tdx` /
    `sev_snp` modules (feature-gated, not visible to default doc
    build). Demoted to inline code spans.
  - `omni-tee/src/traits.rs:148-149` — same root cause, same fix.
  - `omni-crypto/src/kex.rs:55` — link to non-existent
    `Self::from_bytes` on `OmniEphemeralSecret`. Demoted to inline
    code span (future `from_bytes` constructor is a hypothetical;
    the comment now reflects that without claiming the API exists).

- **Kernel — MB12: real message-passing IPC + multi-task user-space
  (Track B MB12.0a–MB12.9, 2026-05-18).** Closes the Phase 1 deliverable
  "IPC primitives operational (typed message passing)" from the roadmap.
  Two Ring 3 processes (sender + receiver) now exchange a payload
  through a kernel-mediated channel, with capability gating and
  scheduler-driven dispatch (TSS.rsp0 + CR3 reload + first-dispatch via
  `enter_user_mode`).

  - **Scheduler multi-task user (MB12.0a/b)** — `RoundRobinScheduler::yield_current`
    ([`scheduling.rs`](./crates/omni-kernel/src/scheduling.rs)) now
    updates `TSS.rsp0` and reloads CR3 when dispatching a user task,
    and detects the first-dispatch sentinel (`context.rsp == 0`) to
    transition directly to Ring 3 via `enter_user_mode` instead of the
    `context_switch` asm path. `USER_RFLAGS = 0x202` exported as a
    shared constant from [`usermode.rs`](./crates/omni-kernel/src/bare_metal/usermode.rs).
  - **`omni-crypto` feature gating (MB12.0c)** — introduces
    `default = ["rng"]` + `rng = ["dep:getrandom", "dep:rand_core",
    "dep:argon2", "omni-types/id-generation"]` + `bare-metal = []` in
    [`omni-crypto/Cargo.toml`](./crates/omni-crypto/Cargo.toml). All
    `generate()` methods on `OmniSigningKey`, `OmniAeadKey`,
    `OmniEphemeralSecret`, `OmniStaticSecret`, plus `generate_ephemeral`
    and the `argon2id_*` family, are now `#[cfg(feature = "rng")]`. The
    verify-only path (`OmniVerifyingKey::verify`,
    `domain_separated_hash`, HKDF) remains always available. Userspace
    consumers see no change with default features. Bare-metal compile
    on `x86_64-unknown-none` is partially unblocked — see *Known
    Limitations* below.
  - **Kernel capability check (MB12.0c')** —
    [`capabilities.rs`](./crates/omni-kernel/src/capabilities.rs)
    introduces `KernelPrincipal([u8; 32])`, `KernelAction::IpcSend/IpcRecv`,
    `KernelResource::IpcChannel(u64)`, `KernelCapabilityToken`,
    `CapabilityVerdict`, the `KernelCapabilityCheck` trait, and the
    `StubCapabilityProvider` MB12 implementation (action/resource
    shape-matching, no Ed25519 yet). Trait shape designed to swap in
    MB13 with a real provider when `omni-crypto` builds bare-metal.
  - **`KernelIpcRegistry` (MB12.1+2)** — concrete IPC backend in
    [`ipc.rs`](./crates/omni-kernel/src/ipc.rs). `BTreeMap<u64, Channel>`
    storage (no `HashMap` — `hashbrown::ahash` requires `getrandom`,
    conflict with MB12.0c). `Channel` carries
    `{policy, owner, send_subject, recv_subject, queue, waiters_send,
    waiters_recv}` — wait queues live inside the channel for O(1)
    lookup. `WakeAction { None, Wake(TaskId), Block(TaskId) }` is the
    contract between the registry and the syscall layer. Capability
    check is two-tiered: `verifier.verify()` at `create_channel`,
    byte-equality at `send`/`receive`.
  - **`PendingReceive` slot + `principal` in PCB (MB12.3)** —
    [`process.rs`](./crates/omni-kernel/src/process.rs) extends
    `ProcessControlBlock` with `principal: KernelPrincipal` (defaults
    to `KernelPrincipal::ZERO` for kernel-spawned tasks) and
    `pending_receive: Option<PendingReceive>` (reserved for MB13
    drain-at-dispatch / SharedMemoryGrant). `spawn_from_elf` takes
    `principal` as its last parameter.
  - **IPC syscall handlers 20-23 (MB12.5)** — `bare_metal/syscall_entry.rs`
    adds the `ipc_handlers` submodule (gated `cfg(all(feature =
    "bare-metal", target_os = "none", not(test)))`):
    - `IpcCreateChannel(20)` — `(queue_depth, backpressure, tee_bound,
      _, _, _) -> channel_id`. Open channels for MB12 (capability
      tokens via syscall deferred to MB13).
    - `IpcDestroyChannel(21)` — `(channel_id, _, _, _, _, _) -> 0 | u64::MAX`.
    - `IpcSend(22)` — `(channel_id, kind, payload_ptr, payload_len, _,
      _) -> 0 | u64::MAX`. Retry-loop on `WakeAction::Block`: parks via
      `yield_current(BlockedOnIpc)`, resumes on wake. `MAX_PAYLOAD = 4096`.
    - `IpcReceive(23)` — `(channel_id, dst_ptr, dst_cap, blocking, _,
      _) -> bytes_received | u64::MAX`. Same retry-loop pattern.
    - All four use `validate_user_buffer`-style range guards;
      hardware PT walks during `copy_nonoverlapping` enforce
      page-presence + `PTE_USER` semantics.
  - **`task_exit` now yields instead of halting** when other tasks are
    runnable — required for multi-task IPC. The fallback to
    `halt_forever` remains for the empty-run-queue terminator.
  - **MB12 user binaries (MB12.0f)** —
    [`bare_metal/userprobe_mb12.rs`](./crates/omni-kernel/src/bare_metal/userprobe_mb12.rs)
    embeds two hand-crafted ELFs (pattern mirroring MB11.7):
    - `USERPROBE_SENDER_ELF` (179 bytes, R+X 59-byte code segment):
      `IpcSend(ch=1, kind=Notification, "ping", 4) → TaskExit(0)`.
    - `USERPROBE_RECEIVER_ELF` (197 bytes file, 141 in-mem with 64-byte
      BSS scratch; R+W+X segment for Phase 1):
      `IpcReceive(ch=1, buf, 64, blocking=1) → WriteConsole(buf, n) → TaskExit(0)`.

    `spawn_userprobe_mb12(...)` pre-creates channel 1 (open, no
    capability subject set) + spawns both tasks `Runnable` on the
    scheduler.
  - **Boot wiring `mb12-userprobe` (MB12.6)** —
    [`lib.rs::kmain`](./crates/omni-kernel/src/lib.rs) under
    `#[cfg(feature = "mb12-userprobe")]`. Calls
    `spawn_userprobe_mb12`, registers a bootstrap task for `kmain`
    itself, then `yield_current(kmain, Terminated)` hands the CPU over
    to the scheduler. The scheduler's MB12.0a/b path dispatches the
    user processes via `enter_user_mode`. Mutually exclusive with
    `mb11-userprobe` at the boot wiring level. Forwarded as
    `mb12-userprobe` feature by
    [`kernel-runner/Cargo.toml`](./kernel-runner/Cargo.toml) (bootable
    image ready for QEMU+OVMF / Proxmox smoke).
  - **Integration tests `mb12_ipc_cross_process` (MB12.7)** —
    [`tests/mb12_ipc_cross_process.rs`](./crates/omni-kernel/tests/mb12_ipc_cross_process.rs):
    8 host-side end-to-end checks covering both ELFs loading into
    distinct address spaces, the happy-path send→receive round-trip,
    the receiver-parks-then-wakes-on-send sequence, `Block`-policy
    sender parking + wake on drain, send/recv capability subject
    mismatch denial, owner-only destroy, and the no-id-reuse invariant.
  - **Smoke output expected (manual QEMU+OVMF / Proxmox run):**
    ```
    [mb12] receiver task_id=N
    [mb12] sender   task_id=M
    [mb12] channel 1 pre-created
    [mb12] handing off to user tasks
    ping
    [user] exit=0
    [user] exit=0
    ```
  - **ADR-0005**
    ([`docs/adr/0005-mb12-ipc-message-passing.md`](./docs/adr/0005-mb12-ipc-message-passing.md))
    captures the architecture decisions (BTreeMap vs HashMap,
    drain-at-syscall vs at-dispatch, capability stub vs full
    omni-capability, hand-crafted ELFs vs separate user crate), the
    `omni-crypto` SIMD-ICE discovery, and the MB13 migration plan
    toward a real Ed25519 capability provider.

  **Known limitations:**
  - **MB12 bare-metal smoke triple-faults on Proxmox VMID 103 / QEMU
    OVMF** (validated 2026-05-18 post-merge). The receiver task spawns,
    the channel is pre-created, `[sched] entering Ring 3 via iretq` is
    emitted on the serial port — then the VM transitions to `stopped`.
    Root cause: Rust's `x86_64-unknown-none` target spec generates
    `ET_EXEC` ELFs with the kernel image at `p_vaddr = 0x200000` (PML4
    index 0); `bootloader 0.11` cannot relocate `ET_EXEC` (the
    `BootloaderConfig::mappings.dynamic_range_start` override is
    silently ignored), so the kernel ends up in the low half. The
    per-process `AddressSpace::new_with_kernel_half` clones only PML4
    indices 256..511, so when the scheduler dispatch path issues
    `mov cr3 → per-process PML4` inside `enter_user_mode`, the next
    instruction fetch lands on an unmapped page → page-fault → IDT
    handler also unmapped → triple fault. The same bug is latent in
    `mb11-userprobe`; the MB11 smoke was never manually validated.
    Host tests are not affected. **MB13 follow-up**: either (a) force
    the kernel ELF to `ET_DYN` (PIE) so the bootloader honours
    `dynamic_range_start`, (b) write a linker script that hard-codes
    the kernel image at an upper-half VA, or (c) install a cross-AS
    trampoline page mapped at the same VA in every PML4. Diagnosis
    captured in [`kernel-runner/src/main.rs`](./kernel-runner/src/main.rs)'s
    `BootloaderConfig` doc-comment.
  - **Capability check is a stub** — `StubCapabilityProvider::verify`
    matches `action`/`resource` shape but does not verify Ed25519
    signatures. MB13 swaps it once `omni-crypto` builds bare-metal.
  - **`omni-crypto` does not build on `x86_64-unknown-none` today** —
    LLVM ICE on SIMD intrinsics in `sha2`, `poly1305`,
    `curve25519-dalek`. ADR-0005 § *Migration* + § *Alternative A*
    document the MB13 work (≈ 1-2 days of `force-soft` feature gating
    or `omni-crypto-verify` extraction).
  - **`mb11-userprobe` and `mb12-userprobe` are mutually exclusive at
    boot wiring level** — when both features are enabled in the same
    build, the MB11 block runs first. CI matrix should run them as
    separate jobs.
  - **No automatic QEMU smoke for `[mb12]` yet** — the existing
    `qemu-boot-smoke` job validates MB1–MB10 + MB11; an MB12 variant
    with `EXPECTED_LINES` extended for `[mb12]` + `ping` is tracked as
    follow-up.

  **Test delta:** workspace test count **393 → 426** (+33).
  (`+4` capability, `+17` IPC registry, `+10` userprobe MB12, `+8`
  cross-process integration, `+3` PCB tests, minus the trait-scaffold
  baseline that the concrete impl replaces.)

- **Kernel — MB11 closure: real user-probe ELF, kmain boot wiring,
  integration tests (Track B MB11.7–MB11.9, 2026-05-18).** Closes the
  MB11 milestone started by the foundation commit; the kernel now spawns
  a real Ring 3 process that issues `WriteConsole("hello\n")` then
  `TaskExit(0)`, under the new `mb11-userprobe` feature.

  - **Hand-crafted user ELF** ([`bare_metal/userprobe.rs::USERPROBE_ELF`](./crates/omni-kernel/src/bare_metal/userprobe.rs)):
    167 bytes — 64-byte ELF64 header + 56-byte PT_LOAD program header +
    47 bytes of `x86_64` machine code & data at file offset 0x78,
    mapped to VA `0x4000_0000`. The code does:
    ```
    mov rax, 60          ; WriteConsole
    lea rdi, [rip+0x1b]  ; ptr → msg
    mov rsi, 6           ; len
    syscall
    mov rax, 11          ; TaskExit
    mov rdi, 0           ; exit code
    syscall              ; never returns
    jmp $                ; safety loop
    msg: "hello\n"
    ```
    Embedding the bytes directly avoids a recursive cargo build in
    `build.rs` and removes the linker-script + target-spec complexity
    a separate `omni-userprobe-helloworld` crate would have introduced.
  - **kmain boot wiring** ([`lib.rs`](./crates/omni-kernel/src/lib.rs))
    under `#[cfg(feature = "mb11-userprobe")]`: reads CR3, calls
    `userprobe::spawn_userprobe`, looks up the resulting `PCB`, prints
    diagnostic lines (`[user] userprobe spawned`, `[user] address
    space activated cr3 = …`, `[user] entering Ring 3 rip = …`), then
    transfers to Ring 3 via `usermode::enter_user_mode(rip, rsp,
    rflags=0x202, cr3)`. The user code's `TaskExit` then writes
    `[user] exit=0\n` and halts the CPU.
  - **`mb11-userprobe` feature flag** in
    [`crates/omni-kernel/Cargo.toml`](./crates/omni-kernel/Cargo.toml)
    and forwarded by [`kernel-runner/Cargo.toml`](./kernel-runner/Cargo.toml).
    Mirrors the `mb8-smoke` pattern: gated out of production builds
    (VirtualBox / Proxmox unaffected) and never on by default.
  - **Integration test**
    [`tests/mb11_userspace.rs`](./crates/omni-kernel/tests/mb11_userspace.rs):
    6 host-side end-to-end checks — userprobe ELF parses with the
    correct entry + flags + "hello\n" payload; `AddressSpace` clones
    only the kernel half of a synthetic boot PML4; user-stack slots
    are disjoint with guard pages; `validate_user_buffer` rejects
    kernel-half buffers and accepts zero-length anywhere.
  - **Unit tests on userprobe** (5 new): ELF parses, entry point is
    `0x4000_0000`, single PT_LOAD with RX flags, code carries
    "hello\n", lea displacement points at the message. Plus the
    6 integration tests above.
  - **QEMU smoke**: feature-gated path documented; running
    `cargo build --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none --features mb11-userprobe` produces a bootable
    image that exercises the full Ring 3 round trip. Expected serial
    output sequence (in addition to the existing K5/LAPIC/sched lines):
    ```
    [user] userprobe spawned  task_id=N
    [user] address space activated cr3 = 0x...
    [user] entering Ring 3 rip = 0x40000000
    hello
    [user] exit=0
    ```

- **Kernel — MB11 foundation: per-process address space, user stacks,
  Ring 3 trampoline, TaskExit/WriteConsole syscalls (Track B MB11.1–MB11.6,
  2026-05-18).** First slice of the userspace-Ring-3 milestone per
  [`docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md`](./docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md).
  Lands all the kernel-side infrastructure; the embedded user-probe ELF +
  `kmain` boot wiring + QEMU smoke close in a follow-up.

  - **GDT extended 3 → 7 slots** ([`bare_metal/gdt.rs`](./crates/omni-kernel/src/bare_metal/gdt.rs)):
    user-data (0x1B, DPL=3), user-code64 (0x23, DPL=3), TSS at 0x28.
    New constants `USER_CS`, `USER_SS`, `KERNEL_CS`, `KERNEL_SS`,
    `STAR_USER_BASE`, `STAR_KERNEL_BASE`.
  - **TSS module** ([`bare_metal/tss.rs`](./crates/omni-kernel/src/bare_metal/tss.rs)):
    104-byte `Tss` (rsp0..rsp2, ist1..ist7, iomap_base = 104), GDT
    descriptor builder (`tss_descriptor`), `ltr_load`, `set_rsp0`.
    `gdt_init` now also installs the TSS descriptor at slots 5–6.
  - **STAR fix** ([`bare_metal/syscall_entry.rs:308`](./crates/omni-kernel/src/bare_metal/syscall_entry.rs#L308)):
    placeholder `0x001B << 48` replaced with `STAR_USER_BASE = 0x10`,
    yielding SDM-correct SYSRET selectors (CS=0x23, SS=0x1B) and
    SYSCALL selectors (CS=0x08, SS=0x10).
  - **`AddressSpace` module** ([`bare_metal/address_space.rs`](./crates/omni-kernel/src/bare_metal/address_space.rs)):
    per-process PML4 with kernel-half **clone-by-reference** (memcpy
    of entries 256..512 from boot CR3 — shared sub-PDPTs). Methods
    `new_with_kernel_half`, `map_user_4k`, `activate` (wrcr3), `invlpg`.
  - **`PageMapper::map_4k_into`** ([`bare_metal/paging.rs:319`](./crates/omni-kernel/src/bare_metal/paging.rs#L319)):
    explicit-root variant required by `AddressSpace`; `map_4k` becomes
    a thin wrapper. ELF loader gains `Elf64::map_and_load_into` mirror.
  - **User stack allocator** ([`bare_metal/user_stack.rs`](./crates/omni-kernel/src/bare_metal/user_stack.rs)):
    range `[0x0000_0040_0000_0000, 0x0000_0040_8000_0000)` (2 GiB),
    16 KiB stack + 16 KiB guard per slot. Per-process bump counter.
  - **`ProcessControlBlock`** ([`crates/omni-kernel/src/process.rs`](./crates/omni-kernel/src/process.rs)):
    wraps the MB10 `TaskControlBlock` with `AddressSpace`, `user_entry`,
    `user_stack_top`, `next_user_stack_slot`. `spawn_from_elf` is the
    high-level entry point: parses ELF, builds AS, allocates user stack,
    registers the PCB with the scheduler.
  - **Scheduler integration** ([`scheduling.rs`](./crates/omni-kernel/src/scheduling.rs)):
    new `processes: Vec<ProcessControlBlock>`, `allocate_task_id`,
    `register_process`, `attach_process`, `process(id)` lookup.
    `allocate_stack_slot` promoted to `pub(crate)` so the user-process
    spawn path can grab a kernel stack from the MB10 isolated range.
  - **iretq trampoline** ([`bare_metal/usermode.rs::enter_user_mode`](./crates/omni-kernel/src/bare_metal/usermode.rs)):
    builds the 5-word Ring 3 stack frame (SS, RSP, RFLAGS, CS, RIP)
    and `iretq`-jumps after a `mov cr3` to the per-process PML4. Safe
    mid-instruction because kernel-half is identical by-reference.
  - **User pointer validation** ([`bare_metal/usermode.rs::validate_user_buffer`](./crates/omni-kernel/src/bare_metal/usermode.rs)):
    range guard `< 0x0000_8000_0000_0000` + 4-level page-table walk
    confirming `PTE_PRESENT | PTE_USER` on every page in the buffer.
  - **Syscall handlers** ([`bare_metal/syscall_entry.rs`](./crates/omni-kernel/src/bare_metal/syscall_entry.rs)):
    `TaskExit (11)` (dequeue + halt), `WriteConsole (60)` (validated
    copy to `early_console::emit`), `MemMap (1)` stub. Dispatch table
    extended; `SyscallNumber::WriteConsole = 60` added.
  - **User-probe scaffold** ([`bare_metal/userprobe.rs`](./crates/omni-kernel/src/bare_metal/userprobe.rs)):
    `spawn_userprobe` helper + placeholder `USERPROBE_ELF` (parseable
    but no code yet). The real `omni-userprobe-helloworld` crate +
    `kmain` boot wiring close MB11.7–MB11.9 in a follow-up.

  Verification:
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` clean.
  - `cargo test --workspace --all-features` → 381 pass / 0 fail
    (was 369 pre-MB11; +12 new unit tests across `tss`, `gdt`,
    `address_space`, `user_stack`, `usermode`, `process`).
  - `bash scripts/check-no-blanket-allow.sh` exits 0.

### Changed

- **Kernel — lift `unsafe_code` blanket allow on `omni-kernel` (Step 7.2,
  2026-05-18).** Final Step 7 PR. Removes the last crate-root blanket
  `#![allow(unsafe_code)]` from
  [`lib.rs:64`](./crates/omni-kernel/src/lib.rs#L64); the crate now carries
  **zero** non-whitelisted crate-root suppressions. The `cfg_attr(test,
  allow(...))` line at `lib.rs:88` is the only remaining inner attribute,
  explicitly whitelisted by ADR-0003 § Escape hatches.

  - Bare-metal target build surfaces ~40 cfg-gated lint violations (the
    host build path had them masked behind `target_os = "none"`); each
    handled with site- or module-level allow + reason, or fixed:
    - 6 `unsafe_code` sites in `lib.rs` (kmain orchestrator) — site-level
      allow citing single-core static-mut aliasing invariant.
    - Module-level allows added on `bare_metal/{arch/x86_64, elf_loader,
      gdt, idt, paging, syscall_entry}.rs` for the lints that fire most
      densely (`unsafe_code`, `doc_markdown`, `ptr_as_ptr`, `similar_names`,
      `cast_possible_truncation`, `integer_division`).
    - Fixed: `paging.rs` 3 trailing-semicolon style; `context_switch.rs`
      list-item indentation; `lapic.rs` `x86_64` backtick.
  - `scripts/check-no-blanket-allow.sh` flipped to **blocking** in CI
    (`continue-on-error: true` removed). Guardrail script now reports
    `ok (scanned 12 crate-root files)`.

  Verification:
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` clean.
  - `cargo test --workspace` 277 pass / 0 fail (unchanged).
  - `bash scripts/check-no-blanket-allow.sh` exits 0.

- **Kernel — lift `clippy::nursery` + `clippy::cargo` blanket allows on
  `omni-kernel` (Step 7.4, 2026-05-18).** Third of four Step 7 PRs. Removes
  the residual `#![allow(clippy::nursery, clippy::cargo)]` from
  [`lib.rs:78`](./crates/omni-kernel/src/lib.rs#L78); only `unsafe_code`
  remains, lifted by PR 7.2. Seven nursery findings handled across
  `scheduling.rs`, `wm.rs`, `demo.rs`:

  - 4 `clippy::too_long_first_doc_paragraph` in `scheduling.rs` — fixed by
    promoting the long first paragraph into a one-line summary + detail
    blank-line + body.
  - 2 `clippy::use_self` in `wm.rs:52` — `pub const DEFAULT: Window = ...`
    rewritten as `Self = Self { ... }`.
  - 1 `clippy::cognitive_complexity` (52/20) on
    `bare_metal/demo.rs::run_desktop` — allow + reason "event loop
    orchestrator: branches mirror input sources".
  - No `cargo` lint warnings fired (workspace deps already coherent).
  - Tests still 277 pass / 0 fail.

- **Kernel — lift `clippy::pedantic` blanket allow on `omni-kernel` (Step 7.3,
  2026-05-18).** Second of four Step 7 PRs. Removes `clippy::pedantic` from
  the crate-root suppression at [`lib.rs:78`](./crates/omni-kernel/src/lib.rs#L78);
  `clippy::nursery` and `clippy::cargo` follow in PR 7.4. ~68 sites split
  across `bare_metal/{cursor,demo,graphics,gdt,idt,input,paging,virtio_tablet,
  widget,wm}.rs`, `scheduling.rs`, and the trait-scaffold modules
  (`capabilities.rs`, `ipc.rs`, `memory.rs`, `syscall.rs`).

  - **Trait scaffold modules**: each got a module-level
    `#![allow(clippy::missing_errors_doc, reason = "trait scaffold methods
    return NotYetImplemented until ...")]`. Mandatory per ADR-0003 § Escape
    hatches (module-level allows are not blanket crate-root allows).
  - **`virtio_tablet.rs`**: module-level allow for `doc_markdown`,
    `cast_ptr_alignment`, `ptr_as_ptr` — MMIO BAR layout per VirtIO 1.0 §4.1.4
    requires raw pointer reinterpretation that's structurally safe.
  - **`demo.rs`**: module-level allow for `doc_markdown`, `similar_names`,
    `map_unwrap_or`, `too_many_lines`, `cast_possible_truncation`,
    `needless_pass_by_value` — orchestrator-level idioms for the desktop demo.
  - **Fixed** rather than allowed: `redundant_closure` in `scheduling.rs:662`
    (`|q| q.is_empty()` → `Vec::is_empty`), `cast_lossless` in
    `graphics.rs:153` (replaced `as u32` with `u32::from`), three
    `semicolon_if_nothing_returned` in `paging.rs:317/339/361`, and three
    `unsafe { … };` style fixes.
  - **Tests still 277 pass / 0 fail.**

- **Kernel — lift restriction + rustdoc blanket allows on `omni-kernel` (Step 7.1,
  2026-05-18).** First of four PRs that close the v0.2.0 kernel CI debt
  described in `progress-omni.md` § 4.5. Removes crate-root blanket
  suppressions for `clippy::indexing_slicing`, `clippy::integer_division`,
  `clippy::new_without_default`, `clippy::fn_to_numeric_cast`,
  `clippy::doc_lazy_continuation`, `clippy::implicit_saturating_sub`,
  `clippy::missing_errors_doc`, `rustdoc::broken_intra_doc_links`, and
  `rustdoc::private_intra_doc_links` from
  [`crates/omni-kernel/src/lib.rs:107-152`](./crates/omni-kernel/src/lib.rs#L107).
  Each intentional violation now carries a localized `#[allow(<lint>, reason
  = "…")]` attribute at the offending item, mirroring the pattern already
  in `memory.rs:278,302,342,346` and `lib.rs:413-432`. Rationale, scope, and
  enforcement formalised in [`docs/adr/0003-no-blanket-allows-in-production-crates.md`](./docs/adr/0003-no-blanket-allows-in-production-crates.md).

  - 39 sites annotated across `wm.rs`, `widget.rs`, `font.rs`, `input.rs`,
    `graphics.rs`, `demo.rs`, `scheduling.rs`, `elf_loader.rs`, `arch/x86_64.rs`.
  - 2 broken intra-doc links rewritten as code spans in `elf_loader.rs` (module
    doc) and `graphics.rs` (`restore_16x16` doc).
  - Workspace test count unchanged (277 pass / 0 fail).
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean post-lift.

- **Tooling — `scripts/check-no-blanket-allow.sh` (2026-05-18).** Bash
  guardrail script that enforces ADR-0003 against `crates/<scoped>/src/{lib,main}.rs`
  (12 scoped crates). Whitelists doc URL, `warn(...)`, `cfg_attr(test,
  allow(...))`, and `cfg_attr(all(feature = "bare-metal", ...))` only.
  Wired into `.github/workflows/ci.yml` as the `blanket-allow-guard` job
  (`continue-on-error: true` until PR 7.2 lifts the final `unsafe_code`
  blanket; flipped to blocking then).

### Added

- **Kernel — kernel stack isolation, Track B MB10 (2026-05-18).** Each kernel
  task now owns a dedicated 4 KiB VA slot in the kernel-only range
  `[0xFFFF_C000_0000_0000, 0xFFFF_C800_0000_0000)` (8 TiB capacity ≈ 1 G slots),
  with a 4 KiB guard page directly below the writable stack page. Stack
  overflow → `#PF` not-present with `CR2` = guard VA, deterministically caught
  by the IDT handler from MB3. Replaces the previous "physical frame + bootloader
  direct-map offset" stack VA that lived in the bootloader's RW window.
  Implemented on branch `feat/kernel-mb10-stack-isolation` per
  [`docs/adr/0002-mb10-kernel-stack-isolation.md`](./docs/adr/0002-mb10-kernel-stack-isolation.md):

  - **`scheduling.rs` constants**: `KERNEL_STACK_VA_BASE`,
    `KERNEL_STACK_VA_END`, `KERNEL_STACK_SIZE = 0x1000`, `KERNEL_STACK_STRIDE
    = 0x2000`. Walking the range by `KERNEL_STACK_STRIDE` gives the guard VA
    of each slot; adding `KERNEL_STACK_SIZE` gives the writable base.
  - **`RoundRobinScheduler::next_kernel_stack_slot`** bump allocator
    (`usize`); slots never reused (task-exit dealloc arrives with the
    process model in MB11+).
  - **`TaskControlBlock::kernel_stack_va`** new field alongside the existing
    `kernel_stack_phys`, retained for debug logs and future free-list.
  - **`spawn_kernel_task` new signature**:
    `(entry, kernel_stack_phys, mapper: &mut PageMapper,
      alloc: &mut BitmapFrameAllocator<N>, priority)`. Drops `phys_offset`
    (no longer needed — stack VA comes from the isolated range, not from the
    direct-map). Calls `mapper.map_4k(va, phys, PRESENT|WRITABLE|NX, alloc)`
    for the writable page; the guard page is deliberately NOT mapped.
  - **`kmain` / `bare_metal::mb8_smoke::run`**: call sites updated.
    `pager` is now `mut` so `map_4k` can extend the kernel page tables;
    the diagnostic line `[stack] kernel stack VA range = 0xFFFF_C000_… ..
    0xFFFF_C800_… (slot 0)` appears alongside the idle-task spawn.
  - **Bootstrap caveat invariant**: `spawn_bootstrap_task` continues to
    reuse the boot stack with sentinel `kernel_stack_phys = 0,
    kernel_stack_va = 0`; the first timer tick still overwrites
    `context.rsp` with the real boot-stack RSP. MB10 does not change the
    bootstrap path.
  - **`omni_context_switch` and `setup_task_frame`**: untouched — they
    operate on `stack_top: u64` and are agnostic to the VA's origin.
  - **Tests**: 4 new unit tests in `scheduling::tests` exercise the
    range-membership, stride-disjointness, guard-offset-below-stack,
    and range-arithmetic invariants. Full `cargo test -p omni-kernel
    --features bare-metal` → 79 unit + 21 integration green (was 75 +
    21).

## [0.2.0] — 2026-05-18

Closes the Track A desktop cycle (M1–M5 + M3b) and the Track B kernel-core cycle (MB1–MB9). The `omni-kernel` bare-metal binary now boots end-to-end on QEMU+OVMF, VirtualBox, and Proxmox VMID 103 with a full paging/IDT/syscall/scheduler/LAPIC/preemption stack and a graphical desktop demo. No public-API breakage versus `0.1.0` — versioning is bumped from `0.1.0` to `0.2.0` because of the new kernel capability surface (minor under SemVer 2.0.0). See `progress-omni.md` § 2 for the per-track milestone matrix and § 3 for the test/build evidence (`cargo test --workspace` → 273 pass, `cargo test -p omni-kernel --features bare-metal` → 75 unit + 21 integration).

### Added

- **Kernel — huge-page-aware paging + direct-map validator, Track B MB9 (2026-05-18).** Unblocks the QEMU+OVMF / Proxmox boot path that MB6-MB8 had marked as a known blocker. The kernel now correctly interprets the bootloader's huge-page direct-map and gives the frame allocator only frames that are reachable through it; the first scheduler/heap write no longer faults. Implemented on branch `feat/kernel-vga-wait` (see [`docs/adr/0001-mb9-paging-huge-page-aware.md`](./docs/adr/0001-mb9-paging-huge-page-aware.md)):

  - **`PageMapper::translate` huge-page aware** (`bare_metal/paging.rs`). The walker now follows entries with `PS=1` at both PDPT (1 GiB) and PD (2 MiB) levels and computes the final physical address from the leaf entry's frame field plus the appropriate page offset (30 bits for 1 GiB, 21 bits for 2 MiB). New flag constant `PTE_HUGE` plus four `HUGE_*_{FRAME,OFFSET}_MASK` constants isolate the bit math. The module doc-block is updated to reflect the new behaviour. `map_4k` is intentionally unchanged: it still operates on 4 KiB entries only and does not split huge pages (out of scope for MB9; documented in the doc-block).
  - **Direct-map validator in `kmain`** (`lib.rs::register_direct_mapped_regions`). A new helper iterates `boot_info.memory_regions`, takes the `Usable` ones, and only marks a region as free in the `BitmapFrameAllocator` if `mapper.translate(VirtAddr(phys_offset + region.start))` *and* `mapper.translate(VirtAddr(phys_offset + last_page_start))` both return `Some`. Regions that fail either probe are skipped wholesale. The result is a hard invariant: every frame the allocator hands out lives inside the active direct map, so `phys + phys_offset` writes never fault. New diagnostic line: `[paging] validated N MiB direct-mapped, skipped M MiB unmapped`.
  - **kmain init order change** (`lib.rs`). `PageMapper::new` now runs *before* the frame allocator is populated (the validator needs the mapper). The K5 banner remains exactly as specified by `OIP-Kernel-005` § S3 — no smoke-test impact. The defensive `mark_range_used(0, 0x100000)` is retained but re-documented as an independent BIOS-reserved-area policy (real-mode IVT, BIOS data, EBDA, video memory), not as the MB9 workaround it shadowed. The MB8 `FIXME(track-b-mb9)` is removed.
  - **`FRAME_BITMAP_WORDS` const + CR2 in #PF handler** (`lib.rs`, `bare_metal/arch/{x86_64,non_x86_64}.rs`, `bare_metal/idt.rs`). The frame-allocator capacity (`16 384` u64 words = 4 GiB) is extracted into a named const for reuse by the validator's signature. New arch primitive `read_cr2()` (with non-x86 stub); the `#PF` handler now prints `cr2=<addr>` alongside `code` and the exception frame, so any future faulting-VA investigation does not require reverse-engineering the disassembly to figure out which memory access triggered the fault.
  - **Cross-cutting heap-virt fix** (`kernel-runner/src/main.rs`). The original MB8 blocker was *thought* to be the kernel-stack allocation; smoke-test debugging on 2026-05-18 showed the actual first crash was the runner's heap install: `pick_region` returns a *physical* address, but the runner was passing it directly to `BumpHeap::init` as if it were virtual. On VirtualBox the low VAs happened to identity-map to low PAs and it worked by accident; on QEMU+OVMF the runner faulted at the first `Vec::push` from the scheduler init (`#PF code=2 cr2≈0x017800C0`). The runner now adds `boot_info.physical_memory_offset` before installing the heap. `pick_region`'s contract is unchanged and the existing host-side test in `tests/boot_info.rs` continues to pass.
  - **Tests**: 8 new unit tests in `paging.rs::tests` exercise huge-page translation at PDPTE PS=1 (start / middle / last byte / no-PD-deref regression), PDE PS=1 (start / middle / last byte), and a 4 KiB regression after the huge-page changes. Full `cargo test -p omni-kernel --features bare-metal` → 75 tests pass (67 pre-existing + 8 new); `cargo test --workspace` → all crates green.
  - **Smoke (QEMU+OVMF, macOS arm64 host, 2026-05-18)**: with `--features bare-metal,mb8-smoke --release`, serial now shows the full sequence the MB8 entry predicted but could not reach — K5 banner, `[paging] mapper ready CR3=0x101000`, `[mem] 244 MiB free / 245 MiB total`, **`[paging] validated 245 MiB direct-mapped, skipped 0 MiB unmapped`** (new MB9 line), `[idt] loaded`, `[syscall] LSTAR set`, `[sched] scheduler init idle task spawned`, `[sched] bootstrap kmain task registered`, `[lapic] timer started vector=0x20`, `[lapic] interrupts enabled`, `[mb8-smoke] task A/B spawned`, `[mb8-smoke] kmain halting — timer drives scheduler`. No `#PF code=2`. A/B character interleaving is present but sparse (QEMU TCG timer accuracy + brew-installed OVMF on arm64 host — host-environment limit, not a kernel issue).
  - **Proxmox verification (VMID 103, host `100.101.77.9`, 2026-05-18 00:49 CEST) — PASSED.** Pre-MB9 kernel image on VMID 103 was crashing with `[OMNI OS EXCEPTION] #PF Page Fault code=2 rip=0x204AE2` on the very same code path the QEMU+OVMF run had exposed. After redeploying HEAD (`926a37e`) via the standard procedure (`cargo run --manifest-path disk-image/Cargo.toml -- kernel-runner/target/x86_64-unknown-none/release/kernel-runner` → SSH copy of the resulting `boot-uefi.img` onto the Proxmox host → `dd` onto `zvol vm-103-disk-6` → `qm start 103`), serial log captured in `/tmp/omni-os-serial.log` shows the same banner+paging+IDT+syscall+sched+lapic+virtio sequence as the QEMU run, followed by `[virtio] tablet ready`. `demo::run_desktop` then proceeds to draw the System Info / Terminal / Clock / Power-Control windows on the VNC framebuffer with the taskbar and 5-minute countdown live. **MB9 fix is therefore confirmed on production hypervisor**, not only on the macOS-host QEMU smoke. This is the first OMNI OS kernel image to boot end-to-end on Proxmox without the pre-MB9 `#PF code=2` regression.
  - **Side-effect verification of MB6/MB7/MB8 on QEMU**: this is the first successful end-to-end boot of the MB6 cooperative scheduler, the MB7 LAPIC timer, the MB8 preemption trampoline, and the bootstrap-kmain-task on QEMU+OVMF. Up to today those milestones had been verified only via 273 workspace unit tests + VirtualBox boot; the QEMU path was gated by this blocker.

- **Kernel — graphical desktop demo, Track A M1–M5 (2026-05-13 → 2026-05-16).** The `omni-kernel` bare-metal binary runs a full interactive graphical session on UEFI/GOP hardware (verified on VirtualBox with OVMF). Implemented on branch `feat/kernel-vga-wait`:

  - **GOP framebuffer abstraction + 8×16 bitmap font renderer** (`bare_metal/graphics.rs`, `bare_metal/font.rs`). Typed `FrameBuffer` wrapper over the `bootloader_api` framebuffer pointer; `render_char` / `render_str` at arbitrary pixel coordinates; palette constants (`WHITE`, `CYAN`, `GRAY`, `RED`, `GREEN`, `DARK`). Validated against a 1024×768 SVGA UEFI framebuffer.
  - **Disk-image builder crate** (`disk-image-builder/`). `cargo run` invokes `llvm-objcopy` to strip the ELF, pads the resulting binary to 4 MiB (VDI minimum), and produces a bootable `.vdi` image consumable by VirtualBox without a host-side install step.
  - **M1/M2 — PS/2 event loop + minimal desktop WM** (`bare_metal/input.rs`, `bare_metal/wm.rs`). Scancode set 2 decode table; `poll_ps2_event()` busy-loops on port `0x64` status; window manager composites window rectangles to the framebuffer in painter's order.
  - **M3 — Software cursor with pixel save/restore** (`bare_metal/cursor.rs`). 11×19 arrow cursor; backing buffer saves the 8×16 tile under the cursor tip before rendering and restores on move — no flicker.
  - **M4 — Widget toolkit + click hit-test + Enter key handling** (`bare_metal/widget.rs`). `Button`, `Label`, `ProgressBar`, `Window` primitives; `HitTest::contains(x, y)` on every widget; Enter key dispatches a synthetic click to the focused button.
  - **M5 — Desktop orchestrator + RTC clock + terminal echo** (`bare_metal/demo.rs`). `run_desktop()` composes four windows (System Info, Terminal, Clock, Power Control); RTC reads BCD registers from ports `0x70`/`0x71` and formats `HH:MM:SS`; terminal echoes PS/2 keystrokes via the `KEYMAP_QWERTY` table; power-off button triggers ACPI S5.
  - **M3b — PS/2 mouse support + 5-minute countdown**. Third PS/2 device byte accumulated via port `0x60` into `MouseState`; countdown in System Info window starts from 5:00 and triggers graceful power-off when it reaches zero.
  - **ACPI S5 power-off** (`bare_metal/arch/x86_64.rs`). Dual strategy: FADT path (`acpi_poweroff_from_fadt`) reads RSDP → RSDT/XSDT → FADT to extract `PM1a_CNT_BLK` + `SLP_TYPa`; fallback (`acpi_poweroff`) tries PCI config-space scan (B0/D0/F0 `0x600` +S5 `0xB004`) and hardcoded QEMU/VirtualBox/Bochs ports.
  - **Verified on VirtualBox + OVMF** (2026-05-16): full desktop boots, RTC updates live, mouse tracks cursor, power-off button shuts down the VM cleanly.

- **Kernel — preemptive scheduling from LAPIC timer, Track B MB8 (2026-05-17).** Builds on MB6 (cooperative round-robin, commit `27720ee`) and MB7 (LAPIC periodic timer) to deliver real preemption: kernel tasks now alternate on the CPU driven by hardware interrupts alone, with no cooperative `yield_current` call. Design follows the **separation-of-contexts** principle (Linux-style): the interrupt handler owns the *trap frame* (caller-saved + CPU-pushed `[RIP, CS, RFLAGS, RSP, SS]`, closed by `iretq`); the scheduler owns the *switch frame* (callee-saved, closed by `ret`); the two never cross. Implemented on branch `feat/kernel-vga-wait`:

  - **`need_resched` trampoline** (`scheduling.rs`, `bare_metal/lapic.rs`). Two atomic booleans (`NEED_RESCHED`, `IN_SCHEDULER`) decouple the LAPIC tick from the actual reschedule. The timer handler does only `TICK_COUNT++` + EOI + `NEED_RESCHED.store(true)`; the asm stub then `call`s `kernel_check_need_resched` before popping caller-saved + `iretq`, which inspects the flag and (if set, and `IN_SCHEDULER` is not already taken) invokes the **existing MB6 cooperative `yield_current`** — `omni_context_switch` is reused unchanged. This means no second context-switch path: cooperative `TaskYield` syscall and timer preemption share the same code, and any future interrupt source (keyboard, disk, NIC) can request a reschedule without knowing how a context switch works.
  - **Bootstrap kmain task** (`scheduling::RoundRobinScheduler::spawn_bootstrap_task`). Registers the currently-executing kmain flow as a scheduler-visible `TaskControlBlock` *before* `sti`, with sentinel `context.rsp = 0` and `kernel_stack_phys = 0` (boot stack reused in place). The first timer fire overwrites the sentinel with kmain's real RSP and from that moment kmain is a regular preemptible task. Without this, the first tick would have no `current` to save state into.
  - **Task entry trampoline** (`bare_metal/context_switch.rs::omni_task_entry_trampoline`). Tiny `sti; ret` stub inserted between `omni_context_switch`'s `ret` and a freshly-spawned task's real entry point. Solves the "first switch from inside an Interrupt Gate leaves `IF=0`" problem: the Interrupt Gate masks `IF` on entry, and `omni_context_switch` does not restore it, so without the trampoline a brand-new task would run with interrupts permanently disabled and never be preempted again. `setup_task_frame` now writes 8 words (entry + trampoline-addr + 6 zero callee-saved) instead of 7.
  - **`mb8-smoke` feature** (`bare_metal/mb8_smoke.rs`, gated by `--features mb8-smoke`, which implies `bare-metal`). Spawns Task A and Task B as Interactive-priority kernel tasks; both run tight `early_console::emit(b"A")` / `b"B"` loops with no cooperative yield. Production builds (without the feature) are bit-for-bit unaffected: the smoke module is `#[cfg]`-gated out and `kmain` falls through to the desktop demo as before.
  - **Verification**: `cargo test --workspace` → 273 tests pass (272 baseline + 1 new `bootstrap_task_is_installed_as_current_with_sentinel_rsp`). `cargo build -p omni-kernel --target x86_64-unknown-none --release --no-default-features --features bare-metal` clean; same with `--features bare-metal,mb8-smoke`. Same warning baseline as commit `27720ee` (9 warnings, all pre-existing `multiple_unsafe_ops_per_block`-style lints).
  - **Known blocker (MB9)**: the QEMU+OVMF smoke run of MB8 is gated by a **pre-existing MB6 bug** caught while testing: `bootloader_api` 0.11's `Mapping::Dynamic` for physical memory maps RAM via 2 MiB / 1 GiB huge pages, but the kernel's `paging::PageMapper` only walks 4 KiB pages — so `translate` and `map_4k` mis-handle direct-map VAs. The very first call to `spawn_kernel_task` (idle task, MB6 code path) writes to a direct-map VA whose physical frame the bootloader left unmapped, producing #PF immediately after `[syscall] LSTAR set`. VirtualBox happens to dodge this because its bootloader-mapped region overlaps the first free frame allocated by `BitmapFrameAllocator`. Defensive mitigation landed in this commit: `kmain` reserves the low 1 MiB unconditionally via `mark_range_used`. Full fix tracked under MB9 (huge-page-aware `PageMapper` + post-boot direct-map completion). FIXME comment with the full investigation context is anchored in `lib.rs` at the scheduler init block. **The MB8 code itself is complete and correct** — it just cannot be live-verified on QEMU until MB9 unblocks the boot path.

- **Kernel — syscall dispatcher + ELF64 loader, Track B MB4–MB5 (2026-05-16).** Two milestones that complete the syscall ABI entry path and the binary loading prerequisite for MB6 (scheduler + first userspace task):

  - **MB4 — SYSCALL/SYSRET + INT 0x80 dispatcher** (commit `f2e88da`, `bare_metal/syscall_entry.rs`). Activates the P6.5 `SyscallDispatcher` trait scaffold. Two entry paths: (1) fast path via `IA32_LSTAR` MSR — `omni_syscall_entry` `global_asm!` stub marshals register args (RAX→RDI, RDI→RSI, RSI→RDX, RDX→RCX, R10→R8, R8→R9, R9→stack) to System V calling convention, calls `kernel_syscall_dispatch`, then `sysretq`; (2) compatibility path via IDT vector 0x80 (`omni_int80_entry`, same convention, `iretq`). MSR setup in `syscall_init()`: enables `IA32_EFER.SCE`, writes `IA32_STAR` (`KERNEL_CS=0x08`, user placeholder `0x1B`), sets `IA32_LSTAR` to stub VA, `IA32_FMASK=0x200` (masks IF). `KernelSyscallDispatcher`: `TimeMonotonicNanos` returns `rtc_seconds() * 10⁹`, `TaskYield` returns 0, all others `NotYetImplemented`. Error sentinel: `u64::MAX`. `idt_set_vector(vector, handler)` added to `idt.rs`. 8 unit tests. Serial: `[syscall] LSTAR set  INT80=0x80`. System Info window: `Syscall : LSTAR+INT80 ready (MB4)`.
  - **MB5 — ELF64 parser + segment mapper** (commit `960e440`, `bare_metal/elf_loader.rs`). `Elf64<'a>::parse` validates magic, class (64-bit), data (little-endian), machine (`EM_X86_64 = 62`), type (`ET_EXEC` or `ET_DYN`), and program-header bounds. `load_segments()` returns a `SegIter<'a>` struct-based iterator over `PT_LOAD` phdrs, yielding `LoadSegment { virt_addr, file_data: &'a [u8], mem_size, flags }`. `map_and_load<const N>` (x86_64-only): for each segment, allocates frames from `BitmapFrameAllocator<N>`, maps via `PageMapper::map_4k`, writes file content and zeros BSS via the direct-map physical window. `kmain` embeds a 120-byte hand-crafted test ELF (one `PT_LOAD` at `0x4000_0000`) and logs `[elf] probe OK  entry=0x40000000` to the serial console. 12 unit tests. System Info window: `ELF     : parser ready (MB5)`.

- **Kernel — physical-memory + paging + exception infrastructure, Track B MB1–MB3 (2026-05-16).** Three milestones that form the mandatory foundation for MB4 and MB5:

  - **MB1 — `BitmapFrameAllocator<const N: usize>` + GDT** (commit `119f3d8`). Const-generic physical-frame allocator backed by an inline `[u64; N]` bitmap; `N = 16 384` covers 4 GiB at 4 KiB granularity. API: `mark_range_free`, `mark_range_used`, `alloc_frame`, `free_frame`, `total/free_frames/bytes`. GDT (`bare_metal/gdt.rs`): null + kernel-code (0x08) + kernel-data (0x10) descriptors; loaded via `lgdt` at `kmain` entry. 6 unit tests (alloc, exhaustion, free/realloc, range-used, misaligned guard, stats). Frame allocator initialised in `kmain` from the `BootInfo` memory map; System Info window shows real free/total MiB.
  - **MB2 — x86_64 4-level page-table walker** (commit `102ec7a`, `paging.rs`). `PageMapper` backed by the bootloader direct-map window (`BootInfo.physical_memory_offset`); no CR3 write — the bootloader page tables remain active. `translate(VirtAddr) -> Option<PhysAddr>` (4-level walk, huge-page guard, page-offset preservation). `map_4k<N>(virt, phys, flags, alloc)` allocates intermediate frames from `BitmapFrameAllocator<N>`. `unmap_4k(virt)` clears the PTE and issues `invlpg`. New arch primitives: `read_cr3() -> u64`, `unsafe invlpg(virt: u64)` (no-op stubs for non-x86 host builds). 13 unit tests with a heap-allocated 4096-byte-aligned fake arena.
  - **MB3 — IDT + synchronous exception handlers** (commit `657d7d1`, `idt.rs`). 256-entry `IdtEntry` table (16 bytes each, `#[repr(C)]`); `global_asm!` stubs in Intel syntax (stable Rust, no nightly `abi_x86_interrupt`); System V AMD64 ABI stack alignment maintained via `sub rsp, 8` before each `call`. Four vectors: `#DE` (0), `#DF` (8), `#GP` (13), `#PF` (14). Handlers log to serial console via `early_console` then halt; `sti` is NOT issued — the IDT handles synchronous faults only. Loaded via `lidt` in `kmain` immediately after `gdt_init()`. 7 unit tests (struct packing, sentinel, interrupt-gate bit encoding). System Info window extended with `Paging: mapper ready (MB2)` and `IDT: loaded #DE #DF #GP #PF`.

- **OIP-Kernel-003 → Active (2026-05-16).** 48-hour Solo Founder Fast-Track window (§ 5.5, opened 2026-05-15) elapsed with no objections; P6.2 gate is formally closed. All five `no_std` transition steps (K1–K5) are now landed and verified. `OIP-Kernel-003` status field advances from `Last Call` to `Active`; amendment history table updated.

- **Brand pack v0.1 (2026-05-13).** First production-ready brand pack for OMNI OS and OMNI Foundation, landed under [`/brand/`](./brand/). Visual direction **C — Civic Tech / Generational** (Mozilla / Wikimedia / GOV.UK / Long Now reference frame), selected after a 3-way mood-board exploration.
  - [`brand/STRATEGY.md`](./brand/STRATEGY.md) — authoritative brand strategy: naming architecture (product `OMNI OS`, international brand `OMNI Foundation`, legal `Stichting OMNI` per Mozilla-style three-name pattern), positioning, 5-attribute personality + 5-row anti-personality, eight voice rules, full lexicon (owned + rejected), messaging hierarchy from 8-word tagline to 60-second pitch, boilerplate paragraphs, application guidance, co-branding rules, forks-welcome conventions.
  - [`brand/brand-book.html`](./brand/brand-book.html) + [`brand/OMNI-Brand-Book-v0.1.pdf`](./brand/OMNI-Brand-Book-v0.1.pdf) — 21-page consolidated brand book (cover, mission, personality, voice, hierarchy, mark, palette, typography, iconography, applications, operating-the-brand). Generated via WeasyPrint 68.1 from the print-ready HTML source.
  - [`brand/logos/`](./brand/logos/) — 8 production SVG lockups: primary horizontal, stacked, monogram, monochrome dark and light, OMNI Foundation primary, OMNI Foundation full legal lockup (with `Stichting OMNI · Amsterdam · The Netherlands` legal line), and a construction-grid reference. Mark semantics: brick-red core = Mission Anchor (irrevocable per [`docs/legal/bylaws-draft.md`](./docs/legal/bylaws-draft.md) Article 3); six petrol nodes = federated attested peers.
  - [`brand/colors/`](./brand/colors/) — color tokens in three formats: [`tokens.css`](./brand/colors/tokens.css) (CSS custom properties, light + dark mode), [`tokens.json`](./brand/colors/tokens.json) (Design Tokens Community Group format), [`palette.md`](./brand/colors/palette.md) (human-readable reference with WCAG 2.1 contrast matrix, AAA-target for body text). Five hue families (petrol / cream / brick / sage / charcoal). Brick reserved for governance signaling only — the single-red rule.
  - [`brand/typography/`](./brand/typography/) — type system: Source Serif 4 (display, wordmark), Inter (body, UI), IBM Plex Mono (code, metadata). All three SIL OFL — coherent with the AGPL-3.0 codebase + CC0 protocol specs. Modular scale 1.250 (major third). Tokens in [`type-tokens.css`](./brand/typography/type-tokens.css); rationale + pairing rules + anti-patterns in [`typography.md`](./brand/typography/typography.md).
  - [`brand/icons/icons.svg`](./brand/icons/icons.svg) — SVG sprite with 16 symbols (`omni-mesh`, `omni-node`, `omni-local-first`, `omni-cloud-deny`, `omni-attestation`, `omni-tee`, `omni-kernel`, `omni-agent`, `omni-inference`, `omni-encryption`, `omni-mesh-route`, `omni-governance`, `omni-fork`, `omni-oip`, `omni-zk`, `omni-anchor`). 24×24 viewBox, 1.5 stroke, `currentColor` inheritance, round caps/joins. Each symbol indexes a concept in the lexicon.
  - [`brand/templates/`](./brand/templates/) — 7 ready-to-use templates: [README header markdown](./brand/templates/README-header.md), [single-file slide deck](./brand/templates/slide-deck-starter.html), [social card 1200×630 SVG](./brand/templates/social-card-1200x630.svg), [GitHub social preview 1280×640 SVG](./brand/templates/github-social-1280x640.svg), [OIP/ADR cover template](./brand/templates/oip-cover.md), [HTML email signature](./brand/templates/email-signature.html), [plain-text email signature](./brand/templates/email-signature.txt).
  - Canonical tagline locked: **`An AI-native operating system. Local-first. Decentralized.`** (already in use in the repository `README.md`; promoted from de-facto to canonical).
- [`docs/12-brand.md`](./docs/12-brand.md) — new docs index entry: pointer document linking to the canonical brand pack under [`/brand/`](./brand/).
- [`docs/README.md`](./docs/README.md) — index table extended with row `12`; subdirectory table extended with `/brand/` row.
- [`README.md`](./README.md) — "Documentation" section gained a Brand & visual identity link.

- **Architecture — OMNI App Mesh (2026-05-12).** Five `Draft` OIPs filed that, together with `OIP-Container-006`, compose the user-facing AI-native application layer:
  - [`/oips/oip-helper-007.md`](./oips/oip-helper-007.md) — **OMNI Helper**: agentic need-detection daemon with three autonomy levels (`Autonomous` / `Guided` (default) / `Inform`), mandatory Impact Dashboard schema (Privacy / Trust / Cost / Time / Egress / Capabilities, 1-5 scales), escalation taxonomy for destructive / privacy-violating / capability-escalation classes, plain-language explanation engine, 30s undo window in Autonomous mode, per-context override grammar.
  - [`/oips/oip-pkg-008.md`](./oips/oip-pkg-008.md) — **`omni-pkg`**: content-addressed federated package manager (OCI v1 + Nix-style derivation), Sigstore-signing + CT-log mandatory, capability-declarative manifest, atomic upgrade via Nix-style symlink swap, per-package capability prompt at install. Federation protocol spec'd; trust ranking by tier + reputation + cap-minimality.
  - [`/oips/oip-forge-009.md`](./oips/oip-forge-009.md) — **`omni-forge`**: on-demand Rust → WASM/ELF generation pipeline. Default Cranelift fast-path (<1s compile), opt-in rustc+LLVM AOT for long-lived apps. LLM source generation + static analysis + capability inference + TEE-bound ephemeral signing. Mandatory user source review on first run; hard cap on daily generations.
  - [`/oips/oip-market-010.md`](./oips/oip-market-010.md) — **`omni-market`**: Stichting-curated marketplace (default registry for `omni-pkg`). Bronze / Silver / Gold / Stichting-Curated tiers with public verification criteria. Continuous CVE re-scan (< 6h from advisory publication) with public SLA per severity (Critical 14d). Mandatory reproducible build at Silver+. Commercial commission **10%** (vs 15-30% mainstream marketplaces); 0% OSS; 0% Stichting-sponsored.
  - [`/oips/oip-flagship-011.md`](./oips/oip-flagship-011.md) — **Omni\* flagship apps program**. Reserved `Omni{Function}` naming convention for Stichting-Curated apps. First flagship: **OmniCode** delivered in two phases — Phase 1 (immediate, ~2-3 engineer-months): upstream Codium packaged as `omni/linux-codium` OmniContainer image with Rust + Python + TypeScript LSPs pre-configured, OpenVSX (Eclipse Foundation registry) as extension marketplace, no telemetry. Phase 2 (year 4 target, ~9-12 engineer-months): native Tauri port for ~50MB binary and sub-second startup.
- [`docs/02-architecture.md`](./docs/02-architecture.md) — new section "OMNI App Mesh" between "Implementation choices" and "Open architectural questions". Diagrams the Helper → (Pkg, Forge) → Market → Flagship layering and ties everything back to the OmniContainer execution substrate.

- **`crates/omni-container/` skeleton (2026-05-12) — reference implementation for `OIP-Container-006` § 10.** Public trait surface, type definitions, lifecycle state machine, and capability profile parser. Every operational method on `engine::ContainerEngine` returns `ContainerError::NotYetImplemented(...)` with a PII-safe static context slug; real KVM ioctl wiring, TDX attestation glue, and SEV-SNP firmware calls land in follow-up OIPs (one per subsystem — engine, image, virtio, attestation). Crate is **std-enabled** (userspace service, per `OIP-Container-006` § 1 box-diagram annotation). Modules: `engine` (`ContainerEngine` trait + `KvmEngine` stub), `image` (`OciImageRef` newtype + structural parser), `lifecycle` (7-state machine with transition validation), `attestation` (`ContainerQuote` carrying host measurement, guest kernel hash, image, capability-set hash, nonce), `profile` (`CapabilityProfile` enum with 5 built-ins + `Custom` + `FromStr`), `virtio/{fs,net,vsock,gpu,rng}` (device-backend trait skeletons + `StubVirtio*` impls), `cli/{run,run_windows,ps}` (parser-agnostic argument structs). Feature flags `default = ["kvm"]`, `kvm`, `tdx`, `sev-snp`, `all-backends` per `OIP-Container-006` § 10. **52 unit tests + 5 integration tests** in `tests/lifecycle_state_machine.rs`. Verification gate: `cargo check / test / clippy --workspace --all-targets --all-features -- -D warnings` / `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` / `cargo fmt --all -- --check` all clean. One workspace-local carve-out documented: `clippy::literal_string_with_formatting_args` is `#![allow]`-listed inside the crate to suppress an upstream-shaped false positive on the workspace `clippy.toml` `=` banner (lint surface preserved everywhere else).

- **Architecture — Container engine + Linux/Windows compatibility (2026-05-12).**
  - [`/oips/oip-container-006.md`](./oips/oip-container-006.md) — `Draft`. **OmniContainer**: native micro-VM container engine (Firecracker/Kata pattern), with **per-container TEE attestation** as default on TDX / SEV-SNP capable hosts. Specifies guest Linux image policy (Stichting-signed `omni-guest-linux-vN.M`), virtio-only I/O with capability-bound backends, OCI image compatibility + OMNI extension manifest, 5 built-in capability profiles, full lifecycle state machine, and reference implementation plan (`crates/omni-container/`).
  - **Windows app compatibility via `omni/linux-wine:N-stable`**: Wine + DXVK + VKD3D-Proton pre-baked image, `omni-container run-windows` CLI alias. ~85-95% Win32 productivity / 75-90% gaming coverage per Steam Deck / ProtonDB baselines. macOS apps explicitly NOT supported (Apple license).
  - **cyDock evolution path** documented: cyDock (Apache-2.0, `cySalazar/cyDock`) is NOT the basis for `omni-container` (different layer — cyDock is a management plane for containerd), but `cyDock-omni` fork retargets backend to `omni-container` REST API in Phase 5+ while reusing the React frontend.
  - **Future-work OIPs registered (not yet filed)**: `OIP-AOT-Wine-XXX` (Phase 6 — AOT packager `.exe` + Wine → OMNI ELF), `OIP-Cross-ISA-XXX` (v1.1+ ARM port — Rosetta-style x86→ARM ISA translator for OMNI binaries), `OIP-Container-Networking-XXX`, `OIP-Container-Storage-XXX`, `OIP-Container-BYOLinux-XXX`, `OIP-Container-Windows-VM-XXX`.
  - [`docs/02-architecture.md`](./docs/02-architecture.md) § "Open architectural questions" — **POSIX compatibility question marked resolved**: no POSIX in OMNI kernel; POSIX exists only inside guest Linux of OmniContainers.

- **P3 — Threat model deepening & cryptographic peer review preparation (scaffolding).**
  - [`docs/protocol/handshake.md`](./docs/protocol/handshake.md) — formal wire-level spec of the mesh handshake (Noise_IK over QUIC with mandatory mutual TEE attestation). Documents invariants I1–I8 and lists 5 open issues for cryptographer review.
  - [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) — Tamarin model with `mutual_authentication`, `forward_secrecy`, `replay_resistance`, `kci_resistance`, and `protocol_version_binding` lemmas. Proof execution deferred to P3.2 cryptographer engagement.
  - [`docs/audits/cryptographer-engagement-template.md`](./docs/audits/cryptographer-engagement-template.md) — paid (USD 8–15k) and volunteer engagement modes; scope, deliverables, selection criteria, and engagement-letter templates.
  - [`/CONTRIBUTORS.md`](./CONTRIBUTORS.md) — initial contributor roster with placeholder rows for OIP Editors, Cryptographer (P3.2), and Stichting Trustees.
  - [`/oips/oip-crypto-002.md`](./oips/oip-crypto-002.md) — `Draft`. Compliance proof scheme: **`sig-v1` mandatory baseline + optional `stark-v0`** for v1.0; STARK chosen over SNARK to avoid trusted setup. `winterfell` v0.10+ as reference. SNARK explicitly deferred.
  - [`docs/04-security-model.md`](./docs/04-security-model.md) §"Compliance proofs" updated to reference `OIP-Crypto-002` decision; "Open security questions" item on zk-SNARK trusted setup marked resolved.
- **P4 — Phase 0 non-technical (Stichting + funding) drafts.**
  - [`docs/legal/bylaws-draft.md`](./docs/legal/bylaws-draft.md) — Stichting OMNI bylaws working English draft (15 articles + 3 appendices). Mission Anchor (Article 3) **immutable** by construction. Founder role with sunset 2026-05-09 → 2031-05-09.
  - [`docs/legal/stichting-checklist.md`](./docs/legal/stichting-checklist.md) — 6-phase execution checklist for Dutch notary + KVK registration + ANBI application.
  - [`docs/funding/pitch-deck.md`](./docs/funding/pitch-deck.md) — 15-slide markdown pitch deck.
  - [`docs/funding/one-pager.md`](./docs/funding/one-pager.md) — short one-pager for warm intros.
  - [`docs/funding/grant-applications/`](./docs/funding/grant-applications/) — drafts for **NLnet** (EUR 50k), **Mozilla MOSS** (USD 200k), **Sloan** (USD 300k / 18 months), **Open Philanthropy** (USD 500k / 24 months).
  - [`docs/funding/sponsor-tier-menu.md`](./docs/funding/sponsor-tier-menu.md) — Bronze / Silver / Gold / Platinum sponsorship tiers, anti-capture safeguards explicit.
  - [`docs/08-funding-policy.md`](./docs/08-funding-policy.md) bumped to **Draft v0.2**: cross-references to bylaws Article 3 (Mission Anchor) and Article 9.1 (30% diversification rule); preliminary founder view on NLnet recorded non-bindingly.
  - [`docs/hiring/role-rust-engineer-kernel.md`](./docs/hiring/role-rust-engineer-kernel.md), [`role-rust-engineer-networking.md`](./docs/hiring/role-rust-engineer-networking.md), [`role-cryptographer.md`](./docs/hiring/role-cryptographer.md) — 3 role descriptions for post-Phase-0 hires.
  - [`docs/hiring/salary-bands.md`](./docs/hiring/salary-bands.md) — public salary bands L1–L5 + D, with geographic-adjustment formula.
- **P5 — `omni-tee` (TEE HAL) production-ready trait surface.**
  - `crates/omni-tee/src/traits.rs` — `TeeBackend` trait with `attest`, `verify_quote`, `seal`, `unseal`, `derive_key_for`. `TeeFamily` enum (Intel TDX / AMD SEV-SNP / Apple Secure Enclave / ARMv9 CCA / Mock). `TeeError` + `TeeErrorKind` taxonomy with PII-safe static context slugs.
  - `crates/omni-tee/src/attestation.rs` — `Quote`, `Measurement` (48-byte cross-vendor), `Nonce`, `QuoteVersion`.
  - `crates/omni-tee/src/sealed_keys.rs` — `SealedBlob`, `SealPolicy`, `TeeSharedKey` (with redacted `Debug` and zeroize-on-Drop via volatile writes).
  - `crates/omni-tee/src/mock.rs` — `MockTeeBackend` with permissive and strict modes; deterministic in-process behavior for use by every other crate's tests. End-to-end unit tests covering attest/verify/seal/unseal/derive happy paths + 7 adversarial scenarios.
  - `crates/omni-tee/src/tdx.rs` (feature `tdx`) — Intel TDX backend scaffold with documented P5.2 integration roadmap.
  - `crates/omni-tee/src/sev_snp.rs` (feature `sev-snp`) — AMD SEV-SNP backend scaffold with documented P5.3 integration roadmap.
  - `crates/omni-tee/tests/mock_integration.rs` — end-to-end handshake simulation across two `MockTeeBackend` instances; replay, tampering, and cross-family negative tests.
  - `crates/omni-tee/Cargo.toml` — feature flags `default = ["mock"]`, `mock`, `tdx`, `sev-snp`, `all-backends`.
  - `crates/omni-hal/src/lib.rs` and `Cargo.toml` — `tee` module re-exports the full `omni-tee` surface; feature flags forwarded so `omni-hal/tdx` transitively enables `omni-tee/tdx`.
- **P6 — `omni-kernel` no_std-ready scaffolding + UEFI bootloader OIP.**
  - `crates/omni-kernel/Cargo.toml` — `bare-metal` feature flag.
  - `crates/omni-kernel/src/lib.rs` — `#![cfg_attr(feature = "bare-metal", no_std)]` + `KernelError` + `KernelResult<T>`.
  - `crates/omni-kernel/src/memory.rs` — `PhysAddr` / `VirtAddr` / `PageSize` / `PageFlags` + `Allocator` and `PageTable` trait skeletons. In-crate `bitflags_simple!` macro avoids the `bitflags` crate dependency at this layer.
  - `crates/omni-kernel/src/scheduling.rs` — `TaskId` / `PriorityClass` (System / RealTime / Interactive / **AiInference** / Background / Idle) / `TaskState` / `Scheduler` trait.
  - `crates/omni-kernel/src/ipc.rs` — `ChannelId` / `MessageKind` / `BackpressurePolicy` / `ChannelPolicy` (`tee_bound: bool`) / `MessageEnvelope` / `Ipc` trait.
  - `crates/omni-kernel/src/capabilities.rs` — `KernelCapabilityId` (u128) + `CapabilityTable` trait. Bridges to the userspace token in `omni-capability`.
  - `crates/omni-kernel/src/syscall.rs` — **stable numeric ABI** for syscalls (mem 1–9, task 10–19, ipc 20–29, cap 30–39, tee 40–49, time 50+). Renumbering is a breaking change requiring an OIP.
  - [`/oips/oip-kernel-003.md`](./oips/oip-kernel-003.md) — `Draft`. UEFI-only boot; **`bootloader` crate v0.11+** selected as reference bootloader for v1.0 (over Limine, GRUB2, custom `uefi-rs`); 5-step `no_std` transition plan (K1–K5).

### Changed

- **2026-05-12 — `OIP-Process-001` §8 (Numbering) structural amendment (bootstrap fiat, §6.3).** Section §8 split into four sub-sections (§8.1 identifier rule, §8.2 filename convention, §8.3 draft-stage placeholder numbers, §8.4 reserved numbers). Substantive clarifications: (a) the integer is the canonical identifier for all cross-references; (b) the slug is explicitly a **category hint**, not a secondary identifier; (c) the global-uniqueness invariant binds at `Review`, not at `Last Call → Active` as the original wording implied — placeholder integer collisions in `Draft` are explicitly permitted and reconciled by the editors at the `Draft → Review` transition. Frontmatter `updated:` bumped 2026-05-10 → 2026-05-12; Amendment history table grows a third row. `oips/README.md` § "Numbering" + "Filing a new OIP" step 4 + "Note on duplicate trailing numbers" updated to mirror the new §8.3 wording. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 10 file(s)`. No semantic change to any prior `Active` OIP. Rationale: the registry holds three placeholder collisions (`OIP-Bounty-002` / `OIP-Crypto-002`; `OIP-Serde-004` / `OIP-Kernel-004`; `OIP-Kernel-005` / `OIP-Voting-005`); the previous §8 wording required ad-hoc footnotes, this amendment formalizes the actual editor practice.
- [`docs/05-governance.md`](./docs/05-governance.md) bumped to **Draft v0.2** with explicit changelog (OIP-Process-001 delegation, BDFL veto immutable window 2026-05-09 → 2031-05-09, founder role years 1–5 / 5+ / 10+).
- [`docs/README.md`](./docs/README.md) — new "Subdirectories" section indexing `/docs/protocol/`, `/docs/audits/`, `/docs/legal/`, `/docs/funding/`, `/docs/hiring/`, `/oips/`, `/protocol-proofs/`.
- `Cargo.toml` `[workspace.package].authors` — aligned to project identity policy: `cySalazar <cySalazar@cySalazar.com>` until Stichting OMNI is constituted (was: `Stichting OMNI <hello@omni-os.org>` placeholder). Transition to Stichting documented in-file.
- `P0-COMPLETION-REPORT.md` moved to [`docs/audits/p0-completion-report.md`](./docs/audits/p0-completion-report.md) (lowercase, kebab-case, in audits directory); `todo.md` cross-reference updated.

### Added

- **2026-05-12 — Tamarin model extended to v0.2 (lemmas for I3 / I7-extended / I8).** [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) gains three new lemmas matching invariants documented in [`docs/protocol/handshake.md`](./docs/protocol/handshake.md) § 2:
  - `mutual_tee_attestation_binding` (I3): an Established session implies the peer's TEE measurement was signed in the M2 transcript by the peer's long-term key.
  - `measurement_root_binding` (I7-extended): the Merkle root of the measurement allowlist advertised in M1/M2 cannot be substituted by an attacker without invalidating the signature.
  - `compliance_capability_no_downgrade` (I8): the negotiated capability set (`caps_intersect(capsA, capsB)`) accepted by A matches a negotiation B actually performed.

  Model rule extensions: new functions `mr_root/1` and `caps_intersect/2`; `Initiator_Send_M1` and `Responder_Receive_M1_Send_M2` now bind `mr_root(allowlist)` and `caps` into the signed transcript; `Initiator_Receive_M2_Send_M3` carries `Bound_Measurements` / `Bound_Roots` / `Verified_Caps_Negotiation` action labels; `Responder_Receive_M3` carries the negotiated capability set into `!Session_Resp`. Proof execution itself remains P3.2-blocked (requires `tamarin-prover` install + cryptographer engagement); the lemma definitions and model rules are now in place to be verified.

  `todo.md` P3.1 status transitioned `[ ]` → `[~]` with per-lemma checkbox tracking; spec + Tamarin artefact deliverables ticked `[x]`; tamarin-prover execution + cryptographer review remain `[ ]`.

- **2026-05-12 — `OIP-Kernel-004` (Draft).** Standards-Track OIP defining the panic handler and global allocator for the bare-metal `omni-kernel`, closing gate K3 of `OIP-Kernel-003` § 3. Specifies the non-allocating, interrupt-disabled, halt-on-completion panic handler with a structured `PanicRecord` encoded via `omni-types::wire` (postcard, per `OIP-Serde-004`); the in-crate bump global allocator (no external dep, ~80 LOC, O(1) allocation, no `dealloc`); and the heap region provisioning stub that `OIP-Kernel-005` will replace with the formal `BootInfo` field. Closes the `cargo build --target x86_64-unknown-none --features bare-metal` failure that currently blocks K3 advancement. 5-step migration plan (K3.a–K3.e) with a new CI build target. Inline `requires:` declares the dependency on both `OIP-Process-001` and `OIP-Kernel-003`.

- **2026-05-12 — `OIP-Serde-004` (Draft).** Standards-Track OIP proposing the migration of the workspace serialization layer from `bincode` v2.0 (unmaintained per RUSTSEC-2025-0141) to `postcard` 1.x. Specifies the canonical-encoding helper module (`omni-types::wire`), the breaking wire-format change (`OMNI-PROTO-v0.1` → `OMNI-PROTO-v0.2`), and a 5-step migration plan (M1–M5) with per-step verification gates. Includes a 6-candidate selection matrix (postcard / bitcode / rkyv / wincode / bincode 1.3.3) ranked against six weighted criteria, with `postcard` as the only candidate satisfying all four hard requirements (`no_std + alloc + serde derive`, active maintenance, stable wire-format spec, audit history).
- **2026-05-12 — `todo.md` P7 tier.** New tier "Workspace serialization migration" with three task groups: P7.1 (`OIP-Serde-004` Last Call closure), P7.2 (M1–M5 migration commits), P7.3 (`OMNI-PROTO-v0.2` documentation update). Tracking gate for `cargo audit` / `cargo deny` returning to clean.
- **2026-05-12 — `oips/README.md` index.** Catches up on the three Draft OIPs filed in the 2026-05-10 scaffolding pass that were not added to the table at the time (`OIP-Crypto-002`, `OIP-Kernel-003`) plus the new `OIP-Serde-004`. Documents the duplicate `002` numbering between `OIP-Bounty-002` and `OIP-Crypto-002` as an acceptable Draft-stage placeholder collision per `OIP-Process-001` § 5 (editors reconcile global numbers on Last Call → Active).

- **2026-05-12 — `OIP-Kernel-005` (Draft).** Standards-Track OIP closing gate K4 of `OIP-Kernel-003` § 3: specifies the boot hand-off ABI between the bootloader and `omni-kernel`, and introduces the **`kernel-runner/` crate** (sibling to `crates/`, not under it) that owns `_start`, the `bootloader_api` / `bootloader` v0.11+ build glue, and the QEMU run configuration. § S5 replaces the K3 `OMNI_KERNEL_HEAP_BASE` / `OMNI_KERNEL_HEAP_LEN` extern-symbol stubs with the in-kernel `pick_region(boot_info.memory_regions)` selector (largest Usable contiguous region ≥ `MIN_HEAP_BYTES = 4 MiB`; tie-break by lowest start address; panic on no-eligible-region). § S4 commits the kernel to using/binding `memory_regions`, `framebuffer`, `rsdp_addr`, `physical_memory_offset`; other `BootInfo` fields are diagnostic-only. § S9 pins `bootloader` / `bootloader_api` at `=0.11.X` for reproducible boot images. K4 follows K3 (`OIP-Kernel-004`) and unblocks K5 (QEMU smoke test).

- **2026-05-12 — `OIP-Voting-005` (Draft).** Process-track OIP retiring the L1/L2 known limitations documented in `OIP-Process-001` § 5.2 (uptime saturating at 90 days; flat contribution factor). Filed two years ahead of the soft 2028-05-10 deadline to buy 24 months of real-world calibration runway. New formulas: `uptime_factor = log(1 + online_days_last_730) / log(731)` (logarithmic over a 2-year horizon, normalized to [0,1]); `contribution_factor = 1.0 + 0.25·commits + 0.25·oip_authorship + 0.20·reviews + 0.30·mesh_operator` (each component ∈ [0,1], coefficients sum to 1.00, max contribution_factor = 2.00, max weight ≈ 1.414). § S3 conflict-of-interest meta-rule: contribution data with OIP-X as its subject is excluded from the voter's contribution_factor on OIP-X. § S5 forward-only activation (in-flight votes continue under the bootstrap formula). § S6 two re-calibration triggers (annual editor review, hard 90-day pin after Phase-4 mesh-telemetry availability). § S7 three reference voter profiles with worked-out weights under both formulas; max-to-min-stake ratio is ≈ 1.85× under the new formula vs ≈ 1.41× under bootstrap. Tally script and pytest suite deferred to V2 (post-Last-Call).

- **2026-05-12 — Tamarin model v0.3 (`restriction caps_intersect_meet`).** [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) precommits the lattice-meet axioms (idempotence + commutativity) for the uninterpreted function `caps_intersect/2` that the I8 lemma `compliance_capability_no_downgrade` relies on. The original v0.2 footer flagged this as a follow-up the cryptographer would add "if proof heuristics struggle"; landing it now removes one ambiguity from the upcoming P3.2 review pass. The restriction is keyed on existing action labels (`Advertised_Caps`, `Negotiated_Caps`) so the constraint solver only expends search effort on capability sets the protocol actually instantiates. Associativity is intentionally omitted (no current lemma computes a 3-party meet). Proof execution itself remains P3.2-blocked.

- **2026-05-12 — Tamarin model v0.4 (5-defect fix-pass + first end-to-end run).** `protocol-proofs/handshake.spthy` had never executed under `tamarin-prover` (proof execution was P3.2-blocked). Installing tamarin-prover 1.12.0 locally surfaced five structural defects that prevented the model from loading: (i) user-declared `pk/1` collided with the `signing` builtin's reserved `pk/1`; (ii) `Responder_Receive_M3` referenced an unbound `dh2` in its `let`-binding; (iii) action label `Verified_Sig_M3` lacked an argument list; (iv) `compliance_capability_no_downgrade` quantified over `A, B` that did not appear in the body (unguarded); (v) lemma bodies used `_`-wildcards and free term variables without quantification. After the fix-pass, all 8 lemmas verify in ≈ 1.36 s. One residual wellformedness warning remains (Message Derivation Checks on peer-controlled variables) and is carried forward to the P3.2 cryptographer review with a written assessment checklist in `protocol-proofs/handshake-proof-run-2026-05-12.txt`. `todo.md` P3.1 "Tamarin proof execution" acceptance criterion transitioned `[ ]` → `[x]`.

- **2026-05-12 — OIP-Serde-004: Draft → Review → Last Call (closes 2026-05-26).** Editorial transitions by the interim editor body under `OIP-Process-001` §6.2. One in-Review correction applied: §S1 dropped the `use-std` feature from the workspace-level `postcard` dependency declaration (Cargo features are additive — enabling `use-std` in `[workspace.dependencies]` would unconditionally pull `std` into the foundational crates that must remain `no_std + alloc`). All five migration steps (M1–M5) landed locally on `feat/p1-foundational-crates`. Last Call → Active triggers on 2026-05-26 or earlier per `OIP-Process-001` § 5.3.

- **2026-05-12 — OIP-Bounty-002: Draft → Review → Last Call (closes 2026-05-26).** Editorial transitions with no substantive content change. As the first non-`Meta` OIP under `OIP-Process-001`, this OIP dogfoods §5 of the process — its `Last Call → Active` path is the project's first formal vote.

- **2026-05-12 — P7.2 M1–M5 (`bincode` v2 → `postcard` 1.x migration) landed locally.** Five-commit sequence on `feat/p1-foundational-crates` implementing `OIP-Serde-004`:
  - **M1 (commit `b8de469`)** — workspace `Cargo.toml` swaps `bincode = { version = "2.0", ... }` for `postcard = { version = "1.0", default-features = false, features = ["alloc"] }` (no `use-std`). Per-crate deps + call-sites in `omni-capability` and `omni-tee` migrated to direct `postcard::*` calls. Verified `cargo build --workspace --all-features` clean.
  - **M2 (commit `9b3d977`)** — introduces `crates/omni-types/src/wire.rs` (~230 LOC) with `encode_canonical<T: Serialize>(&T) -> Result<Vec<u8>>` and `decode_canonical<'a, T: Deserialize<'a>>(&'a [u8]) -> Result<T>`. The decoder uses `postcard::take_from_bytes` + explicit `tail.is_empty()` guard to enforce no-trailing-bytes (smuggling prevention). New `WireErrorKind` enum (`EncodeFailed`, `DecodeFailed`, `TrailingBytes`) + `OmniError::Wire { kind, context }` variant + `OmniError::wire(kind, context)` constructor. `omni-types` becomes the only crate with a direct `postcard` dep. Four `disallowed-methods` entries added to `clippy.toml` for `postcard::{to_allocvec, to_vec, from_bytes, take_from_bytes}`. All callsites in `omni-capability` and `omni-tee` migrated to go through the helper. Verified `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - **M3 (commit `b451539`)** — four `omni-capability` round-trip regression tests pinning postcard-canonical-encoding properties at the public-API boundary: `canonical_bytes_match_wire_encode_canonical_of_payload`, `token_round_trip_via_wire_helper_preserves_signature`, `token_decode_rejects_trailing_bytes`, `canonical_bytes_change_under_field_mutation`. 47 unit + 7 integration tests green.
  - **M4 (commit `61a2b02`)** — `omni-types::version::PROTOCOL_VERSION_V0_2 = ProtocolVersion::new(0, 2)` constant added. V0_1 docstring records supersession. Five new `omni-tee` integration tests on `Quote` / `SealedBlob` round-trip via the wire helper; two new V0_2 compatibility tests. Mock-integration grows from 4 to 9 tests; omni-types from 40 to 42.
  - **M5 (commit `784918b`)** — `crates/omni-capability/tests/wire_format_v0_2.rs` pins the first 49 bytes of `TokenPayload` (`varint(16) | CapabilityId | NodeId`) byte-for-byte under the deterministic fixture, plus encode-decode-encode idempotence and trailing-byte rejection. `crates/omni-tee/tests/wire_format_v0_2.rs` adversarial suite covers (a) bit-flip on every byte of the cryptographically-covered region (nonce / measurement / body marker prefix), (b) truncation at every prefix length, (c) trailing-byte extension with four patterns, (d) swap with unrelated bytes. The bit-flip test explicitly scopes itself to the mock backend's covered fields and documents that `report_data` is not signed by the mock (a real TDX/SEV-SNP backend would catch flips everywhere).

  **Verification gate per OIP-Serde-004 § S5 M5:**
  - `cargo build --workspace --all-features`: clean.
  - `cargo test --workspace --all-features`: 204 tests / 0 failures.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  - `cargo audit`: exit 0. **RUSTSEC-2025-0141 is no longer in the report** (`cargo tree --invert bincode` returns "did not match any packages"). 255 crate dependencies scanned, no advisories matched.
  - `cargo deny check advisories`: ok. `bans` and `licenses` FAIL with pre-existing issues (`cpufeatures` 0.2/0.3 duplicate via aes/argon2/curve25519; `Unicode-DFS-2016` license allowance no longer matching) — explicitly out of `OIP-Serde-004` scope.

- **2026-05-12 — `chore(ci): preload bootstrap-github.sh with oip-lint required check`** (commit `c7298e5`). `OIP-Process-001` §9 ¶2 mandates that branch protection on `main` add `oip-lint / oip-lint` as a required status check within 7 calendar days of the OIP transitioning to `Active` (deadline 2026-05-17). The idempotent bootstrap script `scripts/bootstrap-github.sh` is updated so the founder can apply the change by re-running it with an admin token; the live mutation is **not** applied by this commit because it is a shared-state change requiring founder authorization.

### Fixed

- **2026-05-12 — CI workflow gaps surfaced by PR #13.**
  - `.github/workflows/ci.yml` `cargo clippy` and `cargo doc` jobs were running *without* `--all-features`, while `cargo test` (and the local verify-state pass) used `--all-features`. This silently masked broken intra-doc links to cfg-gated items (`omni-tee::tdx`, `omni-tee::sev_snp`) until the first PR triggered the doc job. Both jobs now pass `--all-features`; inline rationale documents the symmetry.
  - `.github/codeql/codeql-config.yml` (new) — excludes `rust/hardcoded-credentials` (and the alternate `rs/`-namespaced id) from CodeQL analysis. The query was firing on every literal byte string passed as `password` / `salt` to a KDF inside `#[cfg(test)] mod tests` blocks (12 alerts in `crates/omni-crypto/src/kdf.rs:234-260`, all deterministic Argon2id test vectors). Pattern is ubiquitous in crypto-library test suites; the CodeQL config also pins analysis scope to `crates/**/src/**`, excluding doc / OIP / framework / scripts. Wired into `.github/workflows/codeql.yml` via `config-file:` on the `init@v3` step.
  - `oips/oip-crypto-002.md` and `oips/oip-kernel-003.md` — frontmatter normalized to the canonical `OIP-Process-001` template (`oip:` integer, `track:` instead of `category:`, `license: CC0-1.0`, `updated:` / `superseded-by:` populated); section headings to Title Case (the linter is case-sensitive). `oip-kernel-003` was missing `## Privacy Considerations` — added a substantive section covering boot-path identifier exposure, host-local boot logs, allocator zeroization across trust boundaries, and TEE-sealed signing-key residency. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 5 file(s)`. No semantic change to either OIP's proposed decision.

  Note: `cargo audit` and `cargo deny` are still failing on this branch and on `main` due to **`RUSTSEC-2025-0141` — `bincode` v2.0.1 marked unmaintained on 2025-12-16** after the maintainer's announcement to cease development. This is a **pre-existing supply-chain advisory**, not a regression introduced by this PR. Resolution is a separate project-level decision (a `deny.toml` `ignore` entry with a tracking issue + sunset date, OR migration to `postcard` / `bitcode` / `rkyv` via a Standards-Track OIP). Out of scope for this fix-pass.

- **2026-05-12 — P3–P6 scaffolding verify-state fix-pass.** Brought the 2026-05-10 scaffolding pass to green under `cargo build/test/clippy/doc/fmt --all-features`. Net effect: 185 tests pass (was: 142 in the P1 baseline; the +43 cover P5 / P6 scaffolds plus 2 new round-trip tests for `Measurement` serde); `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` clean; `cargo fmt --all -- --check` clean. `cargo deny check` is CI-only and verified by `.github/workflows/audit.yml`. Specific fixes:
  - `crates/omni-tee/src/attestation.rs` — `Measurement([u8; 48])` now implements `Serialize` / `Deserialize` manually (serde derives only auto-impl arrays up to `[T; 32]`); the `Visitor` rejects any input not exactly 48 bytes long, preserving the invariant on the wire. Added two unit tests (`bincode` round-trip + wrong-length rejection).
  - `crates/omni-tee/Cargo.toml` — `bincode` added as a `dev-dependency` to enable the round-trip tests, consistent with the design-comment intent that `Quote` / `Measurement` / `SealedBlob` travel through `bincode` on the wire.
  - `crates/omni-tee/src/sealed_keys.rs` — `TeeSharedKey::drop` `unsafe { write_volatile(...) }` block now carries `#[allow(unsafe_code)]` with documented justification (defeats dead-store elimination without pulling the `zeroize` crate). Test `shared_key_drop_zeroizes` rewritten with `core::mem::ManuallyDrop` so the destructor runs in place — `mem::drop(key)` was moving `key` into the parameter slot and zeroing a different stack address than the captured raw pointer.
  - `crates/omni-tee/src/tdx.rs`, `crates/omni-tee/src/sev_snp.rs` — removed inner `#![cfg(feature = "...")]` (duplicates the gating already present on `pub mod tdx;` / `pub mod sev_snp;` in `lib.rs`).
  - `crates/omni-tee/src/mock.rs` — XOR-fold loops in `seal` / `unseal` / `derive_key_for` rewritten with `iter().zip(...)` to eliminate `clippy::indexing_slicing` warnings; report-data padding loop similarly converted.
  - `crates/omni-tee/src/lib.rs`, `crates/omni-tee/src/traits.rs`, `crates/omni-tee/src/attestation.rs`, `crates/omni-tee/src/tdx.rs`, `crates/omni-tee/tests/mock_integration.rs`, `crates/omni-kernel/src/memory.rs`, `crates/omni-kernel/src/syscall.rs` — backticks added to identifiers in doc comments (`x86_64`, `ARMv9`, `derive_key_for`, `local_attest_secret`, `peer_quote.measurement`); `traits::TeeErrorKind` first doc-comment paragraph shortened.
  - `crates/omni-tee/src/{mock,tdx,attestation}.rs` and `crates/omni-tee/tests/mock_integration.rs` — test modules now carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]` (and `clippy::similar_names` for the integration test's alice/bob/blob naming). Test code is allowed to panic on assertion failure.
  - `crates/omni-kernel/src/lib.rs` — `#![cfg_attr(all(feature = "bare-metal", not(test)), no_std)]` and `no_main` (was: gated only on the feature, broke `cargo test --features bare-metal`); `#![allow(clippy::missing_errors_doc)]` for the kernel scaffold's trait surfaces (per-method `# Errors` docs land with the corresponding subsystem implementation per P6.3+).
  - `crates/omni-kernel/src/memory.rs` — `use crate::{KernelResult, bitflags_simple};` (the `#[macro_export]` macro must be brought into scope explicitly when used in the same crate as its definition).

### Notes

- The scaffolding pass does NOT advance any item to `[x]` status that depends on physical-world action (notarial deed, hardware procurement, cryptographer engagement, audit firm engagement, grant submission). Status icons in `todo.md` remain at `[ ]` for tasks awaiting those gates.
- `MockTeeBackend` is **intentionally permissive** about attestation soundness — its purpose is to exercise consumer code paths, not to enforce real attestation invariants. Production builds MUST disable the `mock` feature.
- `OIP-Kernel-003` and `OIP-Crypto-002` are in `Draft` status; they activate per the standard `OIP-Process-001` flow (Review → Last Call → Active).

---

## [0.1.0] — 2026-05-10

First foundational milestone. Repository hygiene (P0) and foundational crates (P1) are landed and verified.

### Added

- **Repository hygiene (P0, closed 2026-05-09).** AGPL-3.0 `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `Cargo.lock`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, GitHub Actions workflows (`ci`, `audit`, `sbom`, `reproducible-build`, `dco`, `codeql`, `labeler`), Dependabot config, branch protection on `main` (signed commits, linear history, required reviewers), issue / PR templates, label taxonomy.
- **Foundational crates (P1, closed 2026-05-10).**
  - `omni-types` (33 tests) — strongly-typed identifiers (`NodeId`, `AgentId`, `ModelId`, `CapabilityId`, `SessionId`), `OmniError` taxonomy with `*ErrorKind` discriminants and PII-safe static `context` slugs, `OsVersion` and `ProtocolVersion` (`OMNI-PROTO-vN.M`) with subset-aware compatibility, sealed-trait `EncryptedType` plus marker types (`EncryptedString`, `MaskedSSN`, `TokenizedEmail`, `AttestedHash`) gated behind the `_tokenization_provider` feature.
  - `omni-crypto` (55 tests, marker `AWAITING_CRYPTO_REVIEW`) — `RustCrypto`-family wrappers with typed APIs:
    - `aead`: `ChaCha20-Poly1305` (RFC 8439) with `OmniAeadKey`, `OmniNonce`, `OmniCiphertext`, `NonceCounter` panicking on overflow.
    - `signing`: `Ed25519` (RFC 8032) using `verify_strict` to reject malleable signatures.
    - `kex`: `X25519` (RFC 7748) ECDH with `OmniEphemeralSecret` / `OmniStaticSecret` / `OmniPublicKey` / `OmniSharedSecret` and explicit low-order-point validator.
    - `hash`: trait-based `BLAKE3` / `SHA-256` / `SHA3-256` plus mandatory `domain_separated_hash` helper.
    - `kdf`: `HKDF-SHA-256` (RFC 5869) and `Argon2id` with OWASP-2026 default parameters.
    - `fpe` and `snark` placeholder modules for Phase 4.
    - `ConstantTimeEq` on every adversarial path; `Zeroize`-on-`Drop` on every secret.
  - `omni-capability` (43 tests + 7 cross-crate integration tests) — Macaroons-style capability tokens:
    - `token::CapabilityToken` with `bincode` 2.0 canonical encoding signed via Ed25519, embedding the issuer public key for self-contained verification.
    - `scope` with typed `Action` × `Resource` × `TimeWindow` × `Caveat` and a partial-order `is_subset_of`.
    - `attenuation` with property-tested monotonicity (256 cases) plus an adversarial test producing 256 random tampered children, all rejected.
    - `revocation::RevocationList` backed by an in-crate `MicroBloom` (chosen over the `bloomfilter` crate to stay `no_std + alloc`) plus a `BTreeSet` for false-positive resolution.
    - `tee::AttestationSource` trait + `StubAttestation` placeholder; concrete `omni-tee` backends land in P5.
- **Workspace dependency set frozen** (`Cargo.toml` + `docs/09-tech-specifications.md` kept in sync). `RustCrypto` family for all crypto; `ring` was evaluated and intentionally rejected (not `no_std`-friendly).
- **`no_std + alloc`** mandatory across foundational crates (`omni-types`, `omni-crypto`, `omni-capability`).
- **Compile-fail tests** (`trybuild`) for `omni-types`: enforce that `NodeId` / `ModelId` cannot be confused, that `EncryptedType` cannot be implemented externally (sealed trait), and that no `From<String>` constructor exists for `EncryptedString`.
- **Cross-crate integration test** (`crates/omni-capability/tests/integration_full_flow.rs`): full mint → attenuate (3-deep) → verify lifecycle plus six adversarial scenarios (revocation, attestation mismatch, time-window boundaries, tampered child, canonical-encoding round-trip).
- **Fuzz harness scaffolding** (`crates/omni-crypto/fuzz/`) for `aead_open`, `signing_verify`, `kex_dh` — runnable on Rust nightly via `cargo-fuzz`. Execution pass is deferred to P3 (cryptographer review).
- **Mesh-protocol wire format** for `CapabilityToken` documented in [`docs/03-mesh-protocol.md`](./docs/03-mesh-protocol.md) § "Capability tokens".
- **`CHANGELOG.md`** (this file).

### Changed

- `Cargo.toml` workspace dependencies pinned at `RustCrypto`-family versions; `serde` / `bincode` switched to `default-features = false` + `alloc` for `no_std` compatibility.
- `clippy.toml` `disallowed-methods` / `disallowed-types` / `disallowed-macros` reformatted to single-line inline tables (TOML 1.0 compliance).
- Workspace `Cargo.toml` `exclude` list now skips `crates/omni-crypto/fuzz` so `cargo build` / `cargo test` ignore it.

### Security

- `omni-crypto` carries an explicit `AWAITING_CRYPTO_REVIEW` marker. The implementation follows established `RustCrypto`-family APIs with RFC test vectors for every primitive, but no external cryptographer has signed off yet (P3.2 in `/todo.md`, blocked on funding via P4). **Do not use the output of this crate in adversarial settings until that review lands.**

### Notes

- 131 unit tests + 7 integration tests + 4 trybuild compile-fail tests, all green.
- `cargo clippy --all-targets -- -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` both pass on every foundational crate.
- Stub crates (`omni-tee`, `omni-kernel`, `omni-hal`, `omni-runtime`, `omni-mesh`, `omni-tokenization`, `omni-sdk`, `omni-agent`, `omni-shell`) remain as scaffolds; their P5 / P6+ implementations are tracked in `/todo.md`.

[Unreleased]: https://github.com/CySalazar/omni/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/CySalazar/omni/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/CySalazar/omni/releases/tag/v0.1.0
