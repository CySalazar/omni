//! NVMe Admin queue live-submission scaffold.
//!
//! Composes [`crate::ring::SqRing`] + the doorbell offset helper from
//! [`crate::controller_regs::sq_tail_doorbell_offset`] through a
//! [`MmioBackend`] trait so the same code path drives both the live
//! `volatile_write` MMIO on bare-metal and a `MockMmioBackend` in
//! host tests. This is the seam between the pure-state ring math
//! (P6.7.10-pre.9) and the live admin queue's MMIO half.
//!
//! ## Why a trait-based seam
//!
//! The live driver writes the SQ tail doorbell as a 32-bit
//! `volatile_write` to a controller MMIO offset. That access is
//! unavailable in host tests (the offset has no backing memory) but
//! the ring-buffer arithmetic and the SQE serialization steps that
//! precede the doorbell write must be exercised host-side. The
//! [`MmioBackend`] trait lets the test harness substitute an in-memory
//! recorder for the volatile write while sharing every other line of
//! code with the live build.
//!
//! ## What this module does NOT do
//!
//! - It does not drain CQ completions. The live admin CQE drain
//!   reuses [`crate::ring::CqRing`] + a future
//!   [`MmioBackend`] CQ head-doorbell write; that arc lands in a
//!   sibling sub-slice.
//! - It does not allocate the SQ data page. The caller passes a
//!   mutable slice that the driver has already obtained from the
//!   kernel via `DmaMap`; the seam treats it as opaque storage.

use crate::admin::{ADMIN_CQE_BYTES, ADMIN_SQE_BYTES, AdminCqe, AdminCqeFields, AdminSqe};
use crate::controller_regs::{
    ACQ_OFFSET, AQA_OFFSET, ASQ_OFFSET, CC_AMS_SHIFT, CC_CSS_SHIFT, CC_EN_BIT, CC_IOCQES_SHIFT,
    CC_IOSQES_SHIFT, CC_MPS_SHIFT, CC_OFFSET, CSTS_CFS_BIT, CSTS_OFFSET, CSTS_RDY_BIT,
    cq_head_doorbell_offset, sq_tail_doorbell_offset,
};
use crate::ring::{CqRing, RingError, SqRing};

/// Shift of the `ACQS` field inside the `AQA` register (NVMe 1.4
/// § 3.1.8 — bits 27:16 hold `ACQS`, 0-based).
pub const AQA_ACQS_SHIFT: u32 = 16;

/// Mask for the 12-bit `ASQS` / `ACQS` fields inside `AQA`.
pub const AQA_QSIZE_MASK: u32 = 0xFFF;

/// Maximum admin queue depth per NVMe 1.4 § 3.1.8 (`AQA` reserves
/// 12 bits for each of `ASQS` and `ACQS`, so the cap is 4096 since
/// the field is 0-based).
pub const MAX_ADMIN_QUEUE_DEPTH: u32 = 4096;

// =============================================================================
// MmioBackend — abstract doorbell sink
// =============================================================================

/// Abstract MMIO sink for doorbell writes.
///
/// The live driver implements this with a `volatile_write` to the
/// controller's BAR0 page; host tests implement it with an in-memory
/// recorder for assertion. The trait is deliberately minimal — one
/// method, no read side, no error type — so the read-side
/// [`MmioReadBackend`] for status-register polling lands as a
/// separate trait without breaking the doorbell-write surface.
pub trait MmioBackend {
    /// Write a 32-bit doorbell value at the given byte offset
    /// inside the controller's MMIO region.
    ///
    /// The live impl performs a `volatile_write` (NVMe 1.4 § 3.1.10
    /// mandates 32-bit aligned doorbell writes); host impls store
    /// the `(offset, value)` pair for assertion. `offset` is the
    /// byte offset returned by
    /// [`crate::controller_regs::sq_tail_doorbell_offset`].
    fn write_doorbell(&mut self, offset: usize, value: u32);

    /// Write a 32-bit value at the given byte offset inside the
    /// controller's MMIO region — used by the CC/AQA/ASQ/ACQ
    /// register writes during bring-up (NVMe 1.4 § 3.1).
    ///
    /// Default implementation forwards to
    /// [`Self::write_doorbell`] since both paths are semantically
    /// identical at the bus level (32-bit `volatile_write`); the
    /// trait keeps the two methods separate so a future host-test
    /// recorder can distinguish doorbell traffic from register
    /// traffic without re-implementing the doorbell side.
    fn write_register(&mut self, offset: usize, value: u32) {
        self.write_doorbell(offset, value);
    }
}

/// Abstract MMIO source for status-register reads.
///
/// Separate from [`MmioBackend`] because the doorbell write path
/// and the status read path have independent lifetimes — the
/// bring-up FSM polls CSTS during initialization (when no doorbells
/// are written yet), and the live IO path writes doorbells without
/// re-reading any status. Splitting the traits avoids forcing
/// every doorbell-only impl to also implement a read method.
///
/// The live impl performs a `volatile_read` (NVMe 1.4 § 3.0
/// "Endianness" mandates 32-bit aligned, little-endian register
/// reads); host impls return a pre-canned sequence of values per
/// call site so the bring-up FSM tests can simulate
/// "controller not ready" → "controller ready" transitions
/// deterministically.
pub trait MmioReadBackend {
    /// Read a 32-bit register value at the given byte offset
    /// inside the controller's MMIO region.
    fn read_register(&mut self, offset: usize) -> u32;
}

// =============================================================================
// QueueError
// =============================================================================

/// Reason an [`AdminQueuePair`] helper could not complete.
///
/// Maps `RingError` plus the new queue-specific failure modes to a
/// single observable taxonomy at the seam boundary. The future live
/// driver translates each variant to the appropriate
/// `BlkResponse::InvalidArgument` / `BackpressureFull` surface; host
/// tests assert against the enum directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QueueError {
    /// The underlying [`SqRing`] rejected the depth at construction
    /// time. Carries the wrapped [`RingError`] for triage.
    Ring(RingError),
    /// The caller-supplied SQ data page is smaller than
    /// `capacity * ADMIN_SQE_BYTES`. The driver pre-allocates the
    /// SQ page through `DmaMap` per OIP-014 § S6 step 4; landing
    /// here indicates a buffer-shape regression upstream.
    SqPageTooSmall,
    /// The caller-supplied CQ data page is smaller than
    /// `cq_capacity * ADMIN_CQE_BYTES`. Symmetric to
    /// [`Self::SqPageTooSmall`] — buffer-shape regression upstream.
    CqPageTooSmall,
    /// The [`SqRing`] is full and no more commands can be submitted
    /// until the controller drains a completion.
    Full,
    /// The doorbell offset overflowed `usize` per
    /// [`crate::controller_regs::sq_tail_doorbell_offset`]
    /// (unreachable in well-formed code; defensive sentinel).
    DoorbellOffsetOverflow,
    /// [`wait_for_csts_rdy`] polled `CSTS` for `poll_limit`
    /// iterations without observing the [`CSTS_RDY_BIT`] set. The
    /// live driver translates this to OIP-014 § S6 step 6 / step 4
    /// timeout — the controller either failed to acknowledge the
    /// enable/disable transition or was never wired correctly.
    ControllerNotReady,
    /// [`program_admin_queue_bases`] received a `sq_depth` or
    /// `cq_depth` outside the legal `1..=`[`MAX_ADMIN_QUEUE_DEPTH`]
    /// range per NVMe 1.4 § 3.1.8. The bring-up FSM normally
    /// catches this upstream against
    /// [`crate::queue_config::is_valid_admin_depth`]; surfacing it
    /// here is defence-in-depth.
    AdminDepthOutOfRange,
    /// [`program_admin_queue_bases`] received an `asq_phys` or
    /// `acq_phys` that is not page-aligned. NVMe 1.4 § 3.1.9
    /// requires the admin queue base addresses to be 4 KiB-aligned
    /// (matching `CC.MPS`).
    QueueBaseMisaligned,
    /// An `AdminSession::run_identify_*` helper exhausted its
    /// `poll_limit` without observing the matching completion.
    /// The live driver translates this to a "controller did not
    /// respond to Identify within budget" diagnostic — usually a
    /// bring-up bug (queue not enabled) or hardware fault.
    IdentifyCompletionTimeout,
    /// `CSTS.CFS` is set (Controller Fatal Status, NVMe 1.4
    /// § 3.1.6). The bring-up FSM MUST translate this to
    /// `BringUpError::ControllerFatal` and abort. CFS is sticky —
    /// once set, the controller never clears it until a full
    /// reset cycle.
    ControllerFatal,
}

impl From<RingError> for QueueError {
    fn from(err: RingError) -> Self {
        Self::Ring(err)
    }
}

// =============================================================================
// AdminQueuePair — SQ-only scaffold (pre.11)
// =============================================================================

/// Admin queue pair scaffold — submission + completion halves.
///
/// Owns the local [`SqRing`] + [`CqRing`] and the controller-side
/// doorbell-array layout parameters (`doorbell_base` is the MMIO
/// offset of the doorbell array per NVMe 1.4 § 3.1.21 — typically
/// [`crate::controller_regs::DOORBELL_ARRAY_OFFSET`] = `0x1000`;
/// `dstrd` is the `CAP.DSTRD` field that scales the per-doorbell
/// stride).
///
/// The completion path automatically feeds the `sq_head` field from
/// each consumed [`AdminCqeFields`] back into the [`SqRing`] via
/// [`SqRing::update_head`] — this is the NVMe 1.4 § 4.6 contract
/// (the controller always reports its current SQ head every
/// completion so the driver knows when to consider the matching SQ
/// slot free for reuse).
#[derive(Debug)]
pub struct AdminQueuePair {
    sq: SqRing,
    cq: CqRing,
    dstrd: u8,
    qid: u16,
}

impl AdminQueuePair {
    /// Admin queue ID per NVMe 1.4 § 3.1.21 — always `0` (the admin
    /// queue is the controller's bootstrap queue and lives at the
    /// head of the doorbell array).
    pub const ADMIN_QID: u16 = 0;

    /// Construct an empty admin queue pair (qid = 0).
    ///
    /// # Errors
    ///
    /// - [`QueueError::Ring`] wrapping any [`RingError`] the
    ///   underlying [`SqRing::new`] / [`CqRing::new`] surfaces
    ///   (capacity zero or beyond `u16::MAX`).
    pub fn new(sq_depth: u32, cq_depth: u32, dstrd: u8) -> Result<Self, QueueError> {
        Self::new_for_qid(Self::ADMIN_QID, sq_depth, cq_depth, dstrd)
    }

    /// Construct an empty queue pair for an arbitrary queue id
    /// (Phase-1 IO queues use qid `1..=4` per OIP-NVMe-014 § R5).
    /// The doorbell offset calculation routes to the matching
    /// SQ-tail / CQ-head doorbells via
    /// [`crate::controller_regs::sq_tail_doorbell_offset`] /
    /// [`crate::controller_regs::cq_head_doorbell_offset`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::new`].
    pub fn new_for_qid(
        qid: u16,
        sq_depth: u32,
        cq_depth: u32,
        dstrd: u8,
    ) -> Result<Self, QueueError> {
        let sq = SqRing::new(sq_depth)?;
        let cq = CqRing::new(cq_depth)?;
        Ok(Self { sq, cq, dstrd, qid })
    }

    /// Borrow the underlying [`SqRing`] for read-only introspection
    /// (used by host tests and the future drain-side wiring).
    #[must_use]
    pub const fn sq(&self) -> &SqRing {
        &self.sq
    }

    /// Borrow the underlying [`CqRing`] for read-only introspection.
    #[must_use]
    pub const fn cq(&self) -> &CqRing {
        &self.cq
    }

    /// Returns the configured doorbell stride (`CAP.DSTRD`).
    #[must_use]
    pub const fn dstrd(&self) -> u8 {
        self.dstrd
    }

    /// Returns the queue id this queue pair binds to (admin = 0,
    /// IO queues = `1..=io_queue_count`).
    #[must_use]
    pub const fn qid(&self) -> u16 {
        self.qid
    }

    /// Submit one Admin SQE into the queue.
    ///
    /// Steps:
    /// 1. Claim the next SQ ring slot through [`SqRing::submit`]; if
    ///    the ring is full, return [`QueueError::Full`].
    /// 2. Copy the 64-byte SQE into `sq_page` at the claimed slot
    ///    (`offset = slot * ADMIN_SQE_BYTES`). The caller MUST have
    ///    supplied a slice ≥ `capacity * ADMIN_SQE_BYTES` bytes long;
    ///    smaller slices surface [`QueueError::SqPageTooSmall`] and
    ///    the slot claim is rolled back via a manual tail decrement
    ///    is NOT performed — the contract treats the SQ-page-too-small
    ///    branch as a programmer error caught upstream of the
    ///    ring-buffer state, so the implementation prefers loud
    ///    failure over silent rollback. The bring-up FSM validates
    ///    `sq_page.len() == capacity * ADMIN_SQE_BYTES` once at boot.
    /// 3. Compute the SQ tail doorbell offset via
    ///    [`sq_tail_doorbell_offset`]; on overflow return
    ///    [`QueueError::DoorbellOffsetOverflow`].
    /// 4. Call [`MmioBackend::write_doorbell`] with the new tail
    ///    value so the controller picks up the SQE.
    ///
    /// Returns the slot index the SQE was written into on success.
    ///
    /// # Errors
    ///
    /// - [`QueueError::Full`] when the ring has no free slot.
    /// - [`QueueError::SqPageTooSmall`] when `sq_page` is shorter
    ///   than the slot's byte range.
    /// - [`QueueError::DoorbellOffsetOverflow`] when the per-slot
    ///   stride arithmetic overflows `usize` (Phase 1 unreachable).
    pub fn submit<M: MmioBackend>(
        &mut self,
        sqe: &AdminSqe,
        mmio: &mut M,
        sq_page: &mut [u8],
    ) -> Result<u16, QueueError> {
        // SQ page bounds check before claiming a slot so a failure
        // here does not perturb the ring state.
        let needed = (self.sq.capacity() as usize)
            .checked_mul(ADMIN_SQE_BYTES)
            .ok_or(QueueError::SqPageTooSmall)?;
        if sq_page.len() < needed {
            return Err(QueueError::SqPageTooSmall);
        }

        // Compute the doorbell offset eagerly so a stride-arithmetic
        // overflow surfaces before the ring-state mutation.
        let doorbell = sq_tail_doorbell_offset(self.qid, self.dstrd)
            .ok_or(QueueError::DoorbellOffsetOverflow)?;

        // Claim a slot through the ring. The slot index is bounded
        // by `capacity - 1`; the slot-byte-range bounds therefore
        // fit in the SQ page by the check above.
        let slot = self.sq.submit().ok_or(QueueError::Full)?;

        let start = (slot as usize) * ADMIN_SQE_BYTES;
        let end = start + ADMIN_SQE_BYTES;
        let dest = sq_page
            .get_mut(start..end)
            .ok_or(QueueError::SqPageTooSmall)?;
        dest.copy_from_slice(sqe.as_bytes());

        // Ring the SQ tail doorbell with the new tail value (the
        // ring's `submit` already advanced the local tail; reading
        // it back gives the value the controller wants).
        let new_tail = u32::from(self.sq.tail());
        mmio.write_doorbell(doorbell, new_tail);

        Ok(slot)
    }

    /// Update the local view of the controller's SQ head pointer.
    ///
    /// Called from [`Self::drain_completion`] with the `sq_head`
    /// field parsed from a matching completion entry, or by the
    /// caller in advanced flows where the head is observed
    /// out-of-band. Frees ring slots so subsequent
    /// [`Self::submit`] calls succeed.
    pub fn record_head_observed(&mut self, head: u16) {
        self.sq.update_head(head);
    }

    /// Try to drain the next admin completion.
    ///
    /// Steps:
    /// 1. Read the 16-byte CQE at `cq_page[head * ADMIN_CQE_BYTES..]`
    ///    where `head` is the local [`CqRing`] head; on
    ///    out-of-bounds surface [`QueueError::CqPageTooSmall`].
    /// 2. Try to consume the slot via [`CqRing::try_take`]; if the
    ///    parsed phase tag does not match the locally-expected
    ///    value the slot belongs to a previous lap — return
    ///    `Ok(None)` without touching MMIO.
    /// 3. On consume success: (a) ring the CQ head doorbell with
    ///    the new `head` value so the controller knows the slot is
    ///    free; (b) feed `fields.sq_head` back into the local
    ///    [`SqRing`] via [`SqRing::update_head`] so the matching SQ
    ///    slot becomes available for future submits.
    ///
    /// Returns `Ok(Some(fields))` on a consumed completion,
    /// `Ok(None)` when the next slot still belongs to the previous
    /// lap.
    ///
    /// # Errors
    ///
    /// - [`QueueError::CqPageTooSmall`] when `cq_page` is shorter
    ///   than `cq_capacity * ADMIN_CQE_BYTES`.
    /// - [`QueueError::DoorbellOffsetOverflow`] when the per-slot
    ///   stride arithmetic overflows `usize` (Phase 1 unreachable).
    pub fn drain_completion<M: MmioBackend>(
        &mut self,
        mmio: &mut M,
        cq_page: &[u8],
    ) -> Result<Option<AdminCqeFields>, QueueError> {
        // CQ page bounds check before parsing.
        let needed = (self.cq.capacity() as usize)
            .checked_mul(ADMIN_CQE_BYTES)
            .ok_or(QueueError::CqPageTooSmall)?;
        if cq_page.len() < needed {
            return Err(QueueError::CqPageTooSmall);
        }

        // Compute the doorbell offset eagerly so a stride-arithmetic
        // overflow surfaces before any state mutation.
        let doorbell = cq_head_doorbell_offset(self.qid, self.dstrd)
            .ok_or(QueueError::DoorbellOffsetOverflow)?;

        // Locate the current slot. `cq.head() < capacity` by
        // construction; the bounds check above guarantees the
        // 16-byte slot range is in `cq_page`.
        let slot = self.cq.head() as usize;
        let start = slot * ADMIN_CQE_BYTES;
        let end = start + ADMIN_CQE_BYTES;
        let bytes = cq_page.get(start..end).ok_or(QueueError::CqPageTooSmall)?;
        let mut raw = [0u8; ADMIN_CQE_BYTES];
        raw.copy_from_slice(bytes);
        let cqe = AdminCqe::from_bytes(raw);

        // Try to consume the slot. The CqRing checks the phase tag
        // against the locally-expected value; mismatch means the
        // controller has not filled this slot yet on the current
        // lap.
        let Some(fields) = self.cq.try_take(&cqe) else {
            return Ok(None);
        };

        // Ring the CQ head doorbell with the new head value.
        let new_head = u32::from(self.cq.head());
        mmio.write_doorbell(doorbell, new_head);

        // Feed the controller's SQ head back into the local ring so
        // the matching SQ slot becomes free.
        self.sq.update_head(fields.sq_head);

        Ok(Some(fields))
    }
}

// =============================================================================
// wait_for_csts_rdy — CSTS.RDY polling helper
// =============================================================================

/// Poll the controller's `CSTS` register until `CSTS.RDY` is set, up
/// to `poll_limit` iterations.
///
/// Used by OIP-Driver-NVMe-014 § S6 step 6 (after writing `CC.EN = 1`
/// the driver must wait for the controller to acknowledge by setting
/// `CSTS.RDY = 1`) and step 4 (`CC.EN = 0` → `CSTS.RDY = 0`,
/// callers invert the success bit with [`CSTS_RDY_BIT`] then re-use
/// this loop with their own predicate — left for a future helper).
///
/// `poll_limit` is the maximum number of read iterations the helper
/// will attempt before surfacing [`QueueError::ControllerNotReady`].
/// The live driver typically sets this to `CAP.TO` (Time Out, in
/// 500 ms units) × spin delay; the host harness passes a smaller
/// number to keep tests fast.
///
/// # Errors
///
/// - [`QueueError::ControllerNotReady`] when the poll budget is
///   exhausted without observing `CSTS.RDY = 1`.
/// - [`QueueError::ControllerFatal`] if any poll iteration
///   observes `CSTS.CFS = 1` (Controller Fatal Status). The
///   poll loop aborts immediately — CFS is sticky per NVMe 1.4
///   § 3.1.6 so continuing to wait is pointless.
pub fn wait_for_csts_rdy<R: MmioReadBackend>(
    mmio: &mut R,
    poll_limit: u32,
) -> Result<(), QueueError> {
    for _ in 0..poll_limit {
        let csts = mmio.read_register(CSTS_OFFSET);
        if (csts & CSTS_CFS_BIT) != 0 {
            return Err(QueueError::ControllerFatal);
        }
        if (csts & CSTS_RDY_BIT) != 0 {
            return Ok(());
        }
    }
    Err(QueueError::ControllerNotReady)
}

/// Poll `CSTS` until `CSTS.RDY` clears, up to `poll_limit`
/// iterations.
///
/// Symmetric to [`wait_for_csts_rdy`]; used by OIP-014 § S6 step 4
/// where the driver writes `CC.EN = 0` to disable the controller
/// and must wait for `CSTS.RDY = 0` before reconfiguring the admin
/// queue base addresses.
///
/// # Errors
///
/// - [`QueueError::ControllerNotReady`] when the poll budget is
///   exhausted without observing `CSTS.RDY = 0`.
/// - [`QueueError::ControllerFatal`] if any poll iteration
///   observes `CSTS.CFS = 1`.
pub fn wait_for_csts_not_rdy<R: MmioReadBackend>(
    mmio: &mut R,
    poll_limit: u32,
) -> Result<(), QueueError> {
    for _ in 0..poll_limit {
        let csts = mmio.read_register(CSTS_OFFSET);
        if (csts & CSTS_CFS_BIT) != 0 {
            return Err(QueueError::ControllerFatal);
        }
        if (csts & CSTS_RDY_BIT) == 0 {
            return Ok(());
        }
    }
    Err(QueueError::ControllerNotReady)
}

/// Inspect `CSTS.CFS` once (no polling).
///
/// Returns `true` if Controller Fatal Status is set per NVMe 1.4
/// § 3.1.6. The bring-up FSM can call this between admin commands
/// to early-abort if the controller crashed asynchronously.
#[must_use]
pub fn check_controller_fatal<R: MmioReadBackend>(mmio: &mut R) -> bool {
    let csts = mmio.read_register(CSTS_OFFSET);
    (csts & CSTS_CFS_BIT) != 0
}

/// Disable the NVMe controller (OIP-014 § S6 step 4 / NVMe 1.4
/// § 3.1.5).
///
/// Sequence:
/// 1. Read `CC` to capture the current configuration so the
///    enable side can restore it without clobbering the
///    IOSQES/IOCQES fields the manifest pinned.
/// 2. Write `CC` with the EN bit cleared.
/// 3. Poll `CSTS.RDY` until it clears (via
///    [`wait_for_csts_not_rdy`]).
///
/// Returns the captured `CC` value so the enable-side helper can
/// re-OR the EN bit without re-reading the register (the value
/// may have changed mid-bring-up on hardware that supports
/// background self-test; capturing once is the safer pattern).
///
/// # Errors
///
/// - [`QueueError::ControllerNotReady`] when `CSTS.RDY` does not
///   clear within `poll_limit` iterations.
pub fn disable_controller<W: MmioBackend, R: MmioReadBackend>(
    mmio_w: &mut W,
    mmio_r: &mut R,
    poll_limit: u32,
) -> Result<u32, QueueError> {
    let cc_current = mmio_r.read_register(CC_OFFSET);
    let cc_disabled = cc_current & !CC_EN_BIT;
    mmio_w.write_register(CC_OFFSET, cc_disabled);
    wait_for_csts_not_rdy(mmio_r, poll_limit)?;
    Ok(cc_current)
}

/// Program the admin queue base-address registers (OIP-014 § S6
/// step 5 / NVMe 1.4 § 3.1.7-9).
///
/// Writes the three registers in the spec-mandated order:
/// 1. `AQA` (`0x24`): `ACQS` (bits 27:16, `cq_depth - 1`) packed
///    with `ASQS` (bits 11:0, `sq_depth - 1`). Both fields are
///    0-based per § 3.1.8.
/// 2. `ASQ` (`0x28..=0x2F`): 64-bit ASQ base address, split into a
///    pair of 32-bit writes (lower at `0x28`, upper at `0x2C`).
/// 3. `ACQ` (`0x30..=0x37`): 64-bit ACQ base address, same
///    32-bit-pair scheme.
///
/// The controller MUST be disabled before this helper runs (the
/// bring-up FSM calls [`disable_controller`] first); writing
/// `AQA`/`ASQ`/`ACQ` while `CC.EN = 1` has implementation-defined
/// effects per § 3.1.7.
///
/// # Errors
///
/// - [`QueueError::AdminDepthOutOfRange`] if `sq_depth` or
///   `cq_depth` is outside `1..=MAX_ADMIN_QUEUE_DEPTH`.
/// - [`QueueError::QueueBaseMisaligned`] if `asq_phys` or
///   `acq_phys` is not 4 KiB-aligned.
#[allow(
    clippy::similar_names,
    reason = "asq/acq pairs are intentional parallel names per NVMe 1.4 § 3.1.9"
)]
pub fn program_admin_queue_bases<W: MmioBackend>(
    mmio_w: &mut W,
    asq_phys: u64,
    acq_phys: u64,
    sq_depth: u32,
    cq_depth: u32,
) -> Result<(), QueueError> {
    // 4 KiB page-size constant (NVMe 1.4 § 3.1.9 alignment).
    const PAGE_SIZE: u64 = 4096;

    // Depth validation per NVMe 1.4 § 3.1.8.
    if !(1..=MAX_ADMIN_QUEUE_DEPTH).contains(&sq_depth)
        || !(1..=MAX_ADMIN_QUEUE_DEPTH).contains(&cq_depth)
    {
        return Err(QueueError::AdminDepthOutOfRange);
    }
    // Base-address alignment per NVMe 1.4 § 3.1.9.
    if (asq_phys & (PAGE_SIZE - 1)) != 0 || (acq_phys & (PAGE_SIZE - 1)) != 0 {
        return Err(QueueError::QueueBaseMisaligned);
    }

    // AQA = (cq_depth - 1) << 16 | (sq_depth - 1). Subtraction is
    // safe by the validation above.
    let asqs_minus_one: u32 = (sq_depth - 1) & AQA_QSIZE_MASK;
    let acqs_minus_one: u32 = (cq_depth - 1) & AQA_QSIZE_MASK;
    let aqa: u32 = asqs_minus_one | (acqs_minus_one << AQA_ACQS_SHIFT);
    mmio_w.write_register(AQA_OFFSET, aqa);

    // ASQ / ACQ split into 32-bit pairs (NVMe 1.4 § 3.1 register
    // layout is 32-bit aligned). Little-endian semantics: lower
    // dword at lower offset.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "splitting a u64 into the two 32-bit halves is intentional"
    )]
    let asq_lo: u32 = asq_phys as u32;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "upper 32 bits of asq_phys after right-shift"
    )]
    let asq_hi: u32 = (asq_phys >> 32) as u32;
    mmio_w.write_register(ASQ_OFFSET, asq_lo);
    mmio_w.write_register(ASQ_OFFSET + 4, asq_hi);

    #[allow(
        clippy::cast_possible_truncation,
        reason = "splitting a u64 into the two 32-bit halves is intentional"
    )]
    let acq_lo: u32 = acq_phys as u32;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "upper 32 bits of acq_phys after right-shift"
    )]
    let acq_hi: u32 = (acq_phys >> 32) as u32;
    mmio_w.write_register(ACQ_OFFSET, acq_lo);
    mmio_w.write_register(ACQ_OFFSET + 4, acq_hi);

    Ok(())
}

// =============================================================================
// CC field programmer (P6.7.10-pre.30)
// =============================================================================

/// `CC.MPS` Phase-1 value — `log2(4096) - 12 = 0` per NVMe 1.4
/// § 3.1.5. The Phase-1 driver pins host memory page size to 4 KiB
/// because the OMNI OS kernel allocates DMA arenas at 4 KiB
/// alignment.
pub const PHASE_1_MPS_LOG2: u32 = 0;

/// `CC.IOSQES` Phase-1 value — `log2(64) = 6` per NVMe 1.4 § 5.4
/// (Submission Queue Entry Size).
pub const PHASE_1_IOSQES_LOG2: u32 = 6;

/// `CC.IOCQES` Phase-1 value — `log2(16) = 4` per NVMe 1.4 § 5.5
/// (Completion Queue Entry Size).
pub const PHASE_1_IOCQES_LOG2: u32 = 4;

/// Maximum legal value for any 4-bit CC field (`IOSQES`,
/// `IOCQES`, `CSS`, `AMS`). NVMe 1.4 § 3.1.5 allocates 4 bits to
/// each of these encoded fields.
const CC_FIELD_4BIT_MAX: u32 = 0xF;

/// Maximum legal value for `CC.MPS` (4 bits per § 3.1.5).
const CC_FIELD_MPS_MAX: u32 = 0xF;

/// Program the canonical `CC` initialisation fields before
/// [`enable_controller`].
///
/// Writes `CC` (with `EN = 0`) packed with:
/// - `CC.MPS = mps_log2`
/// - `CC.IOSQES = iosqes_log2`
/// - `CC.IOCQES = iocqes_log2`
/// - `CC.CSS = 0` (NVM Command Set per OIP-014 § R3)
/// - `CC.AMS = 0` (Round Robin arbitration per § R4)
/// - `CC.EN = 0` (the helper assumes the controller is disabled;
///   call [`disable_controller`] first per OIP-014 § S6 step 4)
///
/// The bring-up FSM calls this between
/// [`program_admin_queue_bases`] and [`enable_controller`].
///
/// # Errors
///
/// - [`QueueError::AdminDepthOutOfRange`] is reused for the
///   "field out of range" condition since the failure modes are
///   identical (out-of-range bring-up parameter). All four field
///   values are bounded to 4 bits (`0..=0xF`); larger values
///   surface this variant.
#[allow(
    clippy::similar_names,
    reason = "iosqes/iocqes are spec-mandated NVMe field names (NVMe 1.4 § 3.1.5)"
)]
pub fn program_cc_fields<W: MmioBackend>(
    mmio_w: &mut W,
    mps_log2: u32,
    iosqes_log2: u32,
    iocqes_log2: u32,
) -> Result<(), QueueError> {
    if mps_log2 > CC_FIELD_MPS_MAX
        || iosqes_log2 > CC_FIELD_4BIT_MAX
        || iocqes_log2 > CC_FIELD_4BIT_MAX
    {
        return Err(QueueError::AdminDepthOutOfRange);
    }
    let cc: u32 = (mps_log2 << CC_MPS_SHIFT)
        | (iosqes_log2 << CC_IOSQES_SHIFT)
        | (iocqes_log2 << CC_IOCQES_SHIFT)
        | (0u32 << CC_CSS_SHIFT)
        | (0u32 << CC_AMS_SHIFT);
    mmio_w.write_register(CC_OFFSET, cc);
    Ok(())
}

/// Enable the NVMe controller (OIP-014 § S6 step 6 / NVMe 1.4
/// § 3.1.5).
///
/// Sequence:
/// 1. Read `CC` to capture the current configuration (the manifest
///    template programs IOSQES + IOCQES + MPS + CSS before this
///    helper runs).
/// 2. Write `CC` with the EN bit set.
/// 3. Poll `CSTS.RDY` until it sets (via [`wait_for_csts_rdy`]).
///
/// Returns the final `CC` value the controller is now running
/// with. The bring-up FSM uses the value to assert that the
/// controller did not silently clear any of the manifest-pinned
/// fields during the enable handshake.
///
/// # Errors
///
/// - [`QueueError::ControllerNotReady`] when `CSTS.RDY` does not
///   set within `poll_limit` iterations.
pub fn enable_controller<W: MmioBackend, R: MmioReadBackend>(
    mmio_w: &mut W,
    mmio_r: &mut R,
    poll_limit: u32,
) -> Result<u32, QueueError> {
    let cc_current = mmio_r.read_register(CC_OFFSET);
    let cc_enabled = cc_current | CC_EN_BIT;
    mmio_w.write_register(CC_OFFSET, cc_enabled);
    wait_for_csts_rdy(mmio_r, poll_limit)?;
    Ok(cc_enabled)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::encode_identify;
    use alloc::vec;
    use alloc::vec::Vec;
    use omni_types::nvme::IdentifyTarget;

    /// Test-only `MmioBackend` impl that records every doorbell
    /// write for assertion.
    #[derive(Debug, Default)]
    struct MockMmioBackend {
        writes: Vec<(usize, u32)>,
    }

    impl MmioBackend for MockMmioBackend {
        fn write_doorbell(&mut self, offset: usize, value: u32) {
            self.writes.push((offset, value));
        }
    }

    fn admin_pair_with(sq_depth: u32, dstrd: u8) -> AdminQueuePair {
        // CQ depth = SQ depth for the test fixtures — the live
        // driver typically sizes CQ ≥ SQ to absorb async completions
        // without blocking, but equal depth is sufficient for the
        // host harness.
        AdminQueuePair::new(sq_depth, sq_depth, dstrd).expect("ctor")
    }

    fn empty_sq_page(capacity: u32) -> Vec<u8> {
        vec![0u8; (capacity as usize) * ADMIN_SQE_BYTES]
    }

    // -------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------

    #[test]
    fn admin_qid_is_zero() {
        assert_eq!(AdminQueuePair::ADMIN_QID, 0);
    }

    #[test]
    fn admin_queue_new_propagates_ring_error_for_zero_depth() {
        let res = AdminQueuePair::new(0, 64, 0);
        assert_eq!(res.err(), Some(QueueError::Ring(RingError::CapacityZero)));
        // CQ-side error is symmetric.
        let res = AdminQueuePair::new(64, 0, 0);
        assert_eq!(res.err(), Some(QueueError::Ring(RingError::CapacityZero)));
    }

    #[test]
    fn admin_queue_new_propagates_ring_error_for_oversized_depth() {
        let res = AdminQueuePair::new(u32::from(u16::MAX) + 1, 64, 0);
        assert_eq!(
            res.err(),
            Some(QueueError::Ring(RingError::CapacityTooLarge))
        );
    }

    #[test]
    fn admin_queue_new_records_dstrd_and_sq() {
        let q = admin_pair_with(64, 2);
        assert_eq!(q.dstrd(), 2);
        assert_eq!(q.sq().capacity(), 64);
        assert!(q.sq().is_empty());
    }

    // -------------------------------------------------------------------
    // Submit happy path
    // -------------------------------------------------------------------

    #[test]
    fn submit_copies_sqe_bytes_into_slot_zero() {
        let mut q = admin_pair_with(8, 0);
        let mut page = empty_sq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 0xABCD);
        let slot = q.submit(&sqe, &mut mmio, &mut page).expect("submit ok");
        assert_eq!(slot, 0);

        // Bytes 0..=63 of the SQ page MUST match the encoded SQE.
        let written = page.get(0..ADMIN_SQE_BYTES).expect("slot 0 range");
        assert_eq!(written, sqe.as_bytes());
    }

    #[test]
    fn submit_rings_doorbell_with_new_tail_value() {
        let mut q = admin_pair_with(8, 0);
        let mut page = empty_sq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        q.submit(&sqe, &mut mmio, &mut page).expect("submit");

        // One doorbell write recorded. Offset = SQ-tail doorbell for
        // qid=0, dstrd=0 (stride = 4 bytes). Value = new tail = 1.
        assert_eq!(mmio.writes.len(), 1);
        let expected_offset = sq_tail_doorbell_offset(0, 0).expect("offset");
        let (off, val) = mmio.writes.first().copied().expect("write");
        assert_eq!(off, expected_offset);
        assert_eq!(val, 1);
    }

    #[test]
    fn submit_three_sqes_writes_three_slots_and_three_doorbells() {
        let mut q = admin_pair_with(8, 0);
        let mut page = empty_sq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        assert_eq!(q.submit(&sqe, &mut mmio, &mut page).unwrap(), 0);
        assert_eq!(q.submit(&sqe, &mut mmio, &mut page).unwrap(), 1);
        assert_eq!(q.submit(&sqe, &mut mmio, &mut page).unwrap(), 2);
        assert_eq!(mmio.writes.len(), 3);
        // Doorbell values monotonically increase as the local tail
        // advances.
        let vals: Vec<u32> = mmio.writes.iter().map(|&(_, v)| v).collect();
        assert_eq!(vals, vec![1, 2, 3]);
    }

    #[test]
    fn submit_with_nonzero_dstrd_stride_scales_doorbell_offset() {
        // dstrd = 2 ⇒ stride = 4 << 2 = 16 bytes per doorbell.
        // Admin SQ tail doorbell sits at DOORBELL_ARRAY_OFFSET + 0
        // (qid=0, index=0) regardless of stride; sanity-check
        // explicitly so a future regression flips here.
        let mut q = admin_pair_with(8, 2);
        let mut page = empty_sq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        let (off, _) = mmio.writes.first().copied().unwrap();
        let expected = sq_tail_doorbell_offset(0, 2).expect("offset");
        assert_eq!(off, expected);
    }

    // -------------------------------------------------------------------
    // Submit error paths
    // -------------------------------------------------------------------

    #[test]
    fn submit_rejects_undersized_sq_page() {
        let mut q = admin_pair_with(4, 0);
        // SQ page only large enough for 2 slots, but capacity = 4.
        let mut page = vec![0u8; 2 * ADMIN_SQE_BYTES];
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        let res = q.submit(&sqe, &mut mmio, &mut page);
        assert_eq!(res, Err(QueueError::SqPageTooSmall));
        // Ring state stays untouched on the early-bounds failure.
        assert!(q.sq().is_empty());
        assert!(mmio.writes.is_empty());
    }

    #[test]
    fn submit_full_ring_returns_full_error() {
        let mut q = admin_pair_with(4, 0);
        let mut page = empty_sq_page(4);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        // capacity=4 ⇒ usable=3. Fill the ring.
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        assert!(q.sq().is_full());

        // Fourth submit must refuse.
        let res = q.submit(&sqe, &mut mmio, &mut page);
        assert_eq!(res, Err(QueueError::Full));
        // No fourth doorbell write.
        assert_eq!(mmio.writes.len(), 3);
    }

    #[test]
    fn submit_resumes_after_record_head_observed() {
        let mut q = admin_pair_with(4, 0);
        let mut page = empty_sq_page(4);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        q.submit(&sqe, &mut mmio, &mut page).unwrap();
        // Controller consumed slot 0; head = 1.
        q.record_head_observed(1);
        // Fourth submit now lands at slot 3.
        let slot = q.submit(&sqe, &mut mmio, &mut page).unwrap();
        assert_eq!(slot, 3);
        assert_eq!(mmio.writes.len(), 4);
    }

    // -------------------------------------------------------------------
    // QueueError taxonomy
    // -------------------------------------------------------------------

    #[test]
    fn queue_error_from_ring_error() {
        let err: QueueError = RingError::CapacityZero.into();
        assert_eq!(err, QueueError::Ring(RingError::CapacityZero));
    }

    #[test]
    fn queue_error_variants_are_distinguishable() {
        let variants = [
            QueueError::Ring(RingError::CapacityZero),
            QueueError::SqPageTooSmall,
            QueueError::Full,
            QueueError::DoorbellOffsetOverflow,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Slot-to-page offset arithmetic pinning
    // -------------------------------------------------------------------

    #[test]
    fn submit_writes_distinct_slots_at_distinct_page_offsets() {
        // The driver MUST place slot N at byte offset
        // `N * ADMIN_SQE_BYTES`. A regression that aliased slots
        // would corrupt the queue silently — pin the invariant
        // explicitly.
        let mut q = admin_pair_with(8, 0);
        let mut page = empty_sq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe_a = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 0xAAAA);
        let sqe_b = encode_identify(IdentifyTarget::Controller, 0x2000, 0, 0xBBBB);
        q.submit(&sqe_a, &mut mmio, &mut page).unwrap();
        q.submit(&sqe_b, &mut mmio, &mut page).unwrap();
        // Slot 0 holds sqe_a, slot 1 holds sqe_b.
        let s0 = page.get(0..ADMIN_SQE_BYTES).unwrap();
        let s1 = page.get(ADMIN_SQE_BYTES..2 * ADMIN_SQE_BYTES).unwrap();
        assert_eq!(s0, sqe_a.as_bytes());
        assert_eq!(s1, sqe_b.as_bytes());
        assert_ne!(s0, s1, "two distinct SQEs must occupy distinct slots");
    }

    // -------------------------------------------------------------------
    // drain_completion (P6.7.10-pre.12)
    // -------------------------------------------------------------------

    /// Build a synthetic CQE byte slot with the supplied phase bit
    /// + CID + sq_head, all other fields zero.
    fn build_cqe(phase: bool, cid: u16, sq_head: u16) -> [u8; ADMIN_CQE_BYTES] {
        let mut raw = [0u8; ADMIN_CQE_BYTES];
        // CDW2: sq_head in bits 15:0, sq_id zero.
        let cdw2: u32 = u32::from(sq_head);
        // CDW3: CID in bits 15:0, status word in bits 31:16 (phase
        // bit at position 0 of the status word).
        let status: u32 = u32::from(phase);
        let cdw3: u32 = u32::from(cid) | (status << 16);
        let mut chunks = raw.chunks_exact_mut(4);
        let _ = chunks.next(); // CDW0
        let _ = chunks.next(); // CDW1
        chunks.next().unwrap().copy_from_slice(&cdw2.to_le_bytes());
        chunks.next().unwrap().copy_from_slice(&cdw3.to_le_bytes());
        raw
    }

    fn empty_cq_page(capacity: u32) -> Vec<u8> {
        vec![0u8; (capacity as usize) * ADMIN_CQE_BYTES]
    }

    fn write_cqe_to_page(page: &mut [u8], slot: usize, cqe: &[u8; ADMIN_CQE_BYTES]) {
        let start = slot * ADMIN_CQE_BYTES;
        let dest = page
            .get_mut(start..start + ADMIN_CQE_BYTES)
            .expect("slot in range");
        dest.copy_from_slice(cqe);
    }

    #[test]
    fn drain_completion_returns_none_when_phase_mismatch() {
        // Initial expected_phase = true. CQE with phase = 0 is
        // stale-from-previous-lap.
        let mut q = admin_pair_with(8, 0);
        let mut mmio = MockMmioBackend::default();
        let cq_page = empty_cq_page(8); // all zero → phase = 0
        let res = q.drain_completion(&mut mmio, &cq_page).unwrap();
        assert!(res.is_none());
        // No doorbell write on a no-op drain.
        assert!(mmio.writes.is_empty());
        // Ring state untouched.
        assert_eq!(q.cq().head(), 0);
        assert!(q.cq().expected_phase());
    }

    #[test]
    fn drain_completion_consumes_matching_phase_and_advances_head() {
        let mut q = admin_pair_with(8, 0);
        let mut mmio = MockMmioBackend::default();
        let mut cq_page = empty_cq_page(8);
        write_cqe_to_page(&mut cq_page, 0, &build_cqe(true, 0x42, 5));
        let res = q.drain_completion(&mut mmio, &cq_page).unwrap();
        let fields = res.expect("phase matched");
        assert_eq!(fields.cid, 0x42);
        assert_eq!(fields.sq_head, 5);
        // CqRing head advanced to 1.
        assert_eq!(q.cq().head(), 1);
        // No wrap yet — phase stays true.
        assert!(q.cq().expected_phase());
        // CQ head doorbell rung exactly once with new_head = 1.
        assert_eq!(mmio.writes.len(), 1);
        let expected_off = cq_head_doorbell_offset(0, 0).unwrap();
        let (off, val) = mmio.writes.first().copied().unwrap();
        assert_eq!(off, expected_off);
        assert_eq!(val, 1);
    }

    #[test]
    fn drain_completion_feeds_sq_head_back_into_sq_ring() {
        let mut q = admin_pair_with(8, 0);
        let mut sq_page = empty_sq_page(8);
        let mut cq_page = empty_cq_page(8);
        let mut mmio = MockMmioBackend::default();

        let sqe = encode_identify(IdentifyTarget::Controller, 0x1000, 0, 1);
        // Three submits → tail = 3, head_observed = 0.
        q.submit(&sqe, &mut mmio, &mut sq_page).unwrap();
        q.submit(&sqe, &mut mmio, &mut sq_page).unwrap();
        q.submit(&sqe, &mut mmio, &mut sq_page).unwrap();
        assert_eq!(q.sq().head_observed(), 0);

        // Controller emits a completion reporting sq_head = 2 (it
        // has consumed slots 0 and 1).
        write_cqe_to_page(&mut cq_page, 0, &build_cqe(true, 1, 2));
        q.drain_completion(&mut mmio, &cq_page).unwrap().unwrap();

        // SqRing head_observed updated to 2 → slots 0 and 1 are now
        // free.
        assert_eq!(q.sq().head_observed(), 2);
    }

    #[test]
    fn drain_completion_phase_flips_on_cq_ring_wrap() {
        // Capacity = 2 ⇒ wrap after the second consumed CQE.
        let mut q = admin_pair_with(2, 0);
        let mut mmio = MockMmioBackend::default();
        let mut cq_page = empty_cq_page(2);
        // Lap 0, slots 0 and 1, both with phase = true.
        write_cqe_to_page(&mut cq_page, 0, &build_cqe(true, 1, 1));
        write_cqe_to_page(&mut cq_page, 1, &build_cqe(true, 2, 2));
        q.drain_completion(&mut mmio, &cq_page).unwrap().unwrap();
        assert_eq!(q.cq().head(), 1);
        assert!(q.cq().expected_phase());
        q.drain_completion(&mut mmio, &cq_page).unwrap().unwrap();
        // Wrapped → head = 0, phase flipped.
        assert_eq!(q.cq().head(), 0);
        assert!(!q.cq().expected_phase());
        // Two doorbell writes, one per consumed CQE.
        assert_eq!(mmio.writes.len(), 2);
        let vals: Vec<u32> = mmio.writes.iter().map(|&(_, v)| v).collect();
        // First write head = 1; second write head wrapped to 0.
        assert_eq!(vals, vec![1, 0]);
    }

    #[test]
    fn drain_completion_rejects_undersized_cq_page() {
        let mut q = admin_pair_with(4, 0);
        let mut mmio = MockMmioBackend::default();
        // CQ page only large enough for 2 slots, capacity = 4.
        let page = vec![0u8; 2 * ADMIN_CQE_BYTES];
        let res = q.drain_completion(&mut mmio, &page);
        assert_eq!(res, Err(QueueError::CqPageTooSmall));
        // No doorbell write, no ring mutation.
        assert!(mmio.writes.is_empty());
        assert_eq!(q.cq().head(), 0);
    }

    #[test]
    fn drain_completion_uses_cq_head_doorbell_offset_not_sq() {
        let mut q = admin_pair_with(4, 0);
        let mut mmio = MockMmioBackend::default();
        let mut cq_page = empty_cq_page(4);
        write_cqe_to_page(&mut cq_page, 0, &build_cqe(true, 1, 0));
        q.drain_completion(&mut mmio, &cq_page).unwrap().unwrap();

        let sq_doorbell = sq_tail_doorbell_offset(0, 0).unwrap();
        let cq_doorbell = cq_head_doorbell_offset(0, 0).unwrap();
        // Tripwire: SQ and CQ doorbells live at distinct offsets;
        // the drain MUST ring the CQ side, NOT the SQ side.
        assert_ne!(sq_doorbell, cq_doorbell);
        let (off, _) = mmio.writes.first().copied().unwrap();
        assert_eq!(off, cq_doorbell);
        assert_ne!(off, sq_doorbell);
    }

    #[test]
    fn cq_page_too_small_in_queue_error_taxonomy() {
        // Pin the new variant against the prior set.
        assert_ne!(QueueError::SqPageTooSmall, QueueError::CqPageTooSmall);
        assert_ne!(QueueError::Full, QueueError::CqPageTooSmall);
    }

    // -------------------------------------------------------------------
    // wait_for_csts_rdy (P6.7.10-pre.14)
    // -------------------------------------------------------------------

    /// Test-only `MmioReadBackend` that returns a pre-canned
    /// sequence of `(offset, value)` pairs, one per `read_register`
    /// call. Once the sequence is exhausted, subsequent reads
    /// return `0` (matches NVMe's "register reads as zero before
    /// MMIO is mapped" semantic).
    #[derive(Debug, Default)]
    struct ScriptedMmioRead {
        script: Vec<(usize, u32)>,
        cursor: usize,
    }

    impl ScriptedMmioRead {
        fn push(&mut self, offset: usize, value: u32) {
            self.script.push((offset, value));
        }
    }

    impl MmioReadBackend for ScriptedMmioRead {
        fn read_register(&mut self, offset: usize) -> u32 {
            let next = self.script.get(self.cursor).copied();
            self.cursor += 1;
            match next {
                Some((expected_off, value)) => {
                    assert_eq!(
                        offset, expected_off,
                        "ScriptedMmioRead: expected offset {expected_off:#x}, got {offset:#x}"
                    );
                    value
                }
                None => 0,
            }
        }
    }

    #[test]
    fn wait_for_csts_rdy_returns_ok_on_first_iteration_when_ready() {
        let mut mmio = ScriptedMmioRead::default();
        // First read returns CSTS with RDY = 1.
        mmio.push(CSTS_OFFSET, CSTS_RDY_BIT);
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Ok(()));
        // Only one read consumed.
        assert_eq!(mmio.cursor, 1);
    }

    #[test]
    fn wait_for_csts_rdy_polls_until_rdy_set() {
        let mut mmio = ScriptedMmioRead::default();
        // Three "not ready" reads, then RDY = 1.
        for _ in 0..3 {
            mmio.push(CSTS_OFFSET, 0);
        }
        mmio.push(CSTS_OFFSET, CSTS_RDY_BIT);
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Ok(()));
        assert_eq!(mmio.cursor, 4);
    }

    #[test]
    fn wait_for_csts_rdy_surfaces_controller_not_ready_after_poll_limit() {
        let mut mmio = ScriptedMmioRead::default();
        // Every read returns 0 (not ready).
        for _ in 0..16 {
            mmio.push(CSTS_OFFSET, 0);
        }
        let res = wait_for_csts_rdy(&mut mmio, 4);
        assert_eq!(res, Err(QueueError::ControllerNotReady));
        // Exactly poll_limit reads consumed.
        assert_eq!(mmio.cursor, 4);
    }

    #[test]
    fn wait_for_csts_rdy_ignores_other_non_cfs_csts_bits() {
        let mut mmio = ScriptedMmioRead::default();
        // CSTS with RDY = 1 AND other non-CFS bits set
        // (e.g. SHST, NSSRO). The helper MUST recognise RDY
        // regardless of the rest. CFS (bit 1) MUST stay clear or
        // the helper short-circuits with ControllerFatal — that
        // path is covered by `wait_for_csts_rdy_aborts_on_cfs`.
        let csts: u32 = CSTS_RDY_BIT | 0xFFFF_FFFE;
        // Clear bit 1 (CFS) so the test exercises only "other
        // bits".
        let csts = csts & !CSTS_CFS_BIT;
        mmio.push(CSTS_OFFSET, csts);
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Ok(()));
    }

    #[test]
    fn wait_for_csts_rdy_zero_poll_limit_surfaces_immediately() {
        let mut mmio = ScriptedMmioRead::default();
        let res = wait_for_csts_rdy(&mut mmio, 0);
        assert_eq!(res, Err(QueueError::ControllerNotReady));
        assert_eq!(mmio.cursor, 0);
    }

    #[test]
    fn controller_not_ready_in_queue_error_taxonomy() {
        assert_ne!(QueueError::ControllerNotReady, QueueError::Full);
        assert_ne!(QueueError::ControllerNotReady, QueueError::SqPageTooSmall);
        assert_ne!(QueueError::ControllerNotReady, QueueError::CqPageTooSmall);
        assert_ne!(
            QueueError::ControllerNotReady,
            QueueError::DoorbellOffsetOverflow
        );
    }

    // -------------------------------------------------------------------
    // CC.EN sequencer (P6.7.10-pre.15)
    // -------------------------------------------------------------------

    #[test]
    fn write_register_default_impl_forwards_to_write_doorbell() {
        // Default-impl tripwire: MmioBackend::write_register MUST
        // route through write_doorbell so existing recorder impls
        // see register writes without overriding the method.
        let mut mmio = MockMmioBackend::default();
        mmio.write_register(CC_OFFSET, CC_EN_BIT);
        assert_eq!(mmio.writes.len(), 1);
        assert_eq!(
            mmio.writes.first().copied().unwrap(),
            (CC_OFFSET, CC_EN_BIT)
        );
    }

    #[test]
    fn wait_for_csts_not_rdy_returns_ok_when_cleared() {
        let mut mmio = ScriptedMmioRead::default();
        // First read returns CSTS with RDY = 0 → success.
        mmio.push(CSTS_OFFSET, 0);
        assert_eq!(wait_for_csts_not_rdy(&mut mmio, 16), Ok(()));
        assert_eq!(mmio.cursor, 1);
    }

    #[test]
    fn wait_for_csts_not_rdy_polls_until_cleared() {
        let mut mmio = ScriptedMmioRead::default();
        // Three "RDY = 1" reads, then RDY = 0.
        for _ in 0..3 {
            mmio.push(CSTS_OFFSET, CSTS_RDY_BIT);
        }
        mmio.push(CSTS_OFFSET, 0);
        assert_eq!(wait_for_csts_not_rdy(&mut mmio, 16), Ok(()));
        assert_eq!(mmio.cursor, 4);
    }

    #[test]
    fn wait_for_csts_not_rdy_exhausts_poll_limit() {
        let mut mmio = ScriptedMmioRead::default();
        for _ in 0..16 {
            mmio.push(CSTS_OFFSET, CSTS_RDY_BIT);
        }
        assert_eq!(
            wait_for_csts_not_rdy(&mut mmio, 4),
            Err(QueueError::ControllerNotReady)
        );
        assert_eq!(mmio.cursor, 4);
    }

    #[test]
    fn disable_controller_clears_cc_en_bit_and_polls_csts_clear() {
        // Initial CC has EN | IOSQES | IOCQES bits set.
        let cc_initial: u32 = CC_EN_BIT | (6 << 16) | (4 << 20);
        let mut reader = ScriptedMmioRead::default();
        // Read 1: CC (returned as captured state)
        reader.push(CC_OFFSET, cc_initial);
        // Reads 2..=N: CSTS poll loop — RDY = 1, then RDY = 0.
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        reader.push(CSTS_OFFSET, 0);
        let mut writer = MockMmioBackend::default();

        let captured = disable_controller(&mut writer, &mut reader, 16).unwrap();
        assert_eq!(captured, cc_initial);
        // Writer recorded exactly one CC write with EN cleared but
        // IOSQES/IOCQES preserved.
        assert_eq!(writer.writes.len(), 1);
        let (off, val) = writer.writes.first().copied().unwrap();
        assert_eq!(off, CC_OFFSET);
        assert_eq!(val, cc_initial & !CC_EN_BIT);
        assert_eq!(val & CC_EN_BIT, 0, "EN bit MUST be clear");
        // IOSQES/IOCQES survived the clear.
        assert_eq!((val >> 16) & 0xF, 6);
        assert_eq!((val >> 20) & 0xF, 4);
    }

    #[test]
    fn disable_controller_surfaces_timeout_on_unresponsive_csts() {
        let cc_initial: u32 = CC_EN_BIT;
        let mut reader = ScriptedMmioRead::default();
        reader.push(CC_OFFSET, cc_initial);
        // CSTS never clears.
        for _ in 0..32 {
            reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        }
        let mut writer = MockMmioBackend::default();
        let res = disable_controller(&mut writer, &mut reader, 4);
        assert_eq!(res, Err(QueueError::ControllerNotReady));
        // The CC write happened despite the timeout — the contract
        // is "issue the write then poll", not "poll first then
        // write".
        assert_eq!(writer.writes.len(), 1);
    }

    #[test]
    fn enable_controller_sets_cc_en_bit_and_polls_csts_set() {
        // Manifest has already programmed IOSQES + IOCQES; the
        // enable helper just ORs EN.
        let cc_pre_enable: u32 = (6 << 16) | (4 << 20);
        let mut reader = ScriptedMmioRead::default();
        reader.push(CC_OFFSET, cc_pre_enable);
        reader.push(CSTS_OFFSET, 0);
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        let mut writer = MockMmioBackend::default();

        let final_cc = enable_controller(&mut writer, &mut reader, 16).unwrap();
        assert_eq!(final_cc, cc_pre_enable | CC_EN_BIT);
        assert_eq!(writer.writes.len(), 1);
        let (off, val) = writer.writes.first().copied().unwrap();
        assert_eq!(off, CC_OFFSET);
        assert_eq!(val, cc_pre_enable | CC_EN_BIT);
        assert_eq!(val & CC_EN_BIT, CC_EN_BIT);
    }

    #[test]
    fn enable_controller_surfaces_timeout_on_unresponsive_csts() {
        let mut reader = ScriptedMmioRead::default();
        reader.push(CC_OFFSET, 0);
        for _ in 0..32 {
            reader.push(CSTS_OFFSET, 0); // RDY never sets
        }
        let mut writer = MockMmioBackend::default();
        let res = enable_controller(&mut writer, &mut reader, 4);
        assert_eq!(res, Err(QueueError::ControllerNotReady));
        assert_eq!(writer.writes.len(), 1);
    }

    // -------------------------------------------------------------------
    // program_admin_queue_bases (P6.7.10-pre.16)
    // -------------------------------------------------------------------

    #[test]
    fn aqa_qsize_mask_and_acqs_shift_match_spec() {
        assert_eq!(AQA_QSIZE_MASK, 0xFFF);
        assert_eq!(AQA_ACQS_SHIFT, 16);
        assert_eq!(MAX_ADMIN_QUEUE_DEPTH, 4096);
    }

    #[test]
    #[allow(clippy::similar_names, reason = "asq/acq pairs are intentional")]
    fn program_admin_queue_bases_writes_aqa_asq_acq_in_order() {
        let mut writer = MockMmioBackend::default();
        let asq_phys: u64 = 0x1000_0000;
        let acq_phys: u64 = 0x1000_4000;
        program_admin_queue_bases(&mut writer, asq_phys, acq_phys, 64, 128).unwrap();
        // Five writes: AQA (1) + ASQ_lo/hi (2) + ACQ_lo/hi (2).
        assert_eq!(writer.writes.len(), 5);
        let offsets: Vec<usize> = writer.writes.iter().map(|&(o, _)| o).collect();
        assert_eq!(
            offsets,
            vec![
                AQA_OFFSET,
                ASQ_OFFSET,
                ASQ_OFFSET + 4,
                ACQ_OFFSET,
                ACQ_OFFSET + 4
            ]
        );
    }

    #[test]
    fn program_admin_queue_bases_encodes_aqa_with_0_based_depths() {
        let mut writer = MockMmioBackend::default();
        program_admin_queue_bases(&mut writer, 0x1000, 0x2000, 64, 128).unwrap();
        // AQA = (128-1) << 16 | (64-1) = 0x7F << 16 | 0x3F = 0x007F_003F.
        let (off, val) = writer.writes.first().copied().unwrap();
        assert_eq!(off, AQA_OFFSET);
        assert_eq!(val, ((128 - 1) << 16) | (64 - 1));
        assert_eq!(val & AQA_QSIZE_MASK, 63);
        assert_eq!((val >> AQA_ACQS_SHIFT) & AQA_QSIZE_MASK, 127);
    }

    #[test]
    fn program_admin_queue_bases_writes_64_bit_asq_base() {
        let mut writer = MockMmioBackend::default();
        let asq_phys: u64 = 0xDEAD_BEEF_F000_0000;
        program_admin_queue_bases(&mut writer, asq_phys, 0x1000, 1, 1).unwrap();
        // writes[1] = ASQ lower 32 bits at ASQ_OFFSET.
        let (off_lo, val_lo) = writer.writes.get(1).copied().unwrap();
        assert_eq!(off_lo, ASQ_OFFSET);
        #[allow(clippy::cast_possible_truncation)]
        let expected_lo = asq_phys as u32;
        assert_eq!(val_lo, expected_lo);
        // writes[2] = ASQ upper 32 bits at ASQ_OFFSET + 4.
        let (off_hi, val_hi) = writer.writes.get(2).copied().unwrap();
        assert_eq!(off_hi, ASQ_OFFSET + 4);
        #[allow(clippy::cast_possible_truncation)]
        let expected_hi = (asq_phys >> 32) as u32;
        assert_eq!(val_hi, expected_hi);
    }

    #[test]
    fn program_admin_queue_bases_writes_64_bit_acq_base() {
        let mut writer = MockMmioBackend::default();
        let acq_phys: u64 = 0xCAFE_BABE_0000_F000;
        program_admin_queue_bases(&mut writer, 0x1000, acq_phys, 1, 1).unwrap();
        // writes[3] = ACQ lower 32 bits at ACQ_OFFSET.
        let (off_lo, val_lo) = writer.writes.get(3).copied().unwrap();
        assert_eq!(off_lo, ACQ_OFFSET);
        #[allow(clippy::cast_possible_truncation)]
        let expected_lo = acq_phys as u32;
        assert_eq!(val_lo, expected_lo);
        let (off_hi, val_hi) = writer.writes.get(4).copied().unwrap();
        assert_eq!(off_hi, ACQ_OFFSET + 4);
        #[allow(clippy::cast_possible_truncation)]
        let expected_hi = (acq_phys >> 32) as u32;
        assert_eq!(val_hi, expected_hi);
    }

    #[test]
    fn program_admin_queue_bases_rejects_zero_depth() {
        let mut writer = MockMmioBackend::default();
        let res = program_admin_queue_bases(&mut writer, 0x1000, 0x2000, 0, 64);
        assert_eq!(res, Err(QueueError::AdminDepthOutOfRange));
        // No writes on validation failure.
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_admin_queue_bases_rejects_oversized_depth() {
        let mut writer = MockMmioBackend::default();
        let res =
            program_admin_queue_bases(&mut writer, 0x1000, 0x2000, MAX_ADMIN_QUEUE_DEPTH + 1, 64);
        assert_eq!(res, Err(QueueError::AdminDepthOutOfRange));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_admin_queue_bases_rejects_misaligned_asq() {
        let mut writer = MockMmioBackend::default();
        let res = program_admin_queue_bases(&mut writer, 0x1001, 0x2000, 64, 64);
        assert_eq!(res, Err(QueueError::QueueBaseMisaligned));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_admin_queue_bases_rejects_misaligned_acq() {
        let mut writer = MockMmioBackend::default();
        let res = program_admin_queue_bases(&mut writer, 0x1000, 0x2008, 64, 64);
        assert_eq!(res, Err(QueueError::QueueBaseMisaligned));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_admin_queue_bases_accepts_max_depth() {
        let mut writer = MockMmioBackend::default();
        program_admin_queue_bases(
            &mut writer,
            0x1000,
            0x2000,
            MAX_ADMIN_QUEUE_DEPTH,
            MAX_ADMIN_QUEUE_DEPTH,
        )
        .unwrap();
        let (_, val) = writer.writes.first().copied().unwrap();
        // AQA encodes 4095 (= 0xFFF) in both halves at max depth.
        assert_eq!(val & AQA_QSIZE_MASK, 0xFFF);
        assert_eq!((val >> AQA_ACQS_SHIFT) & AQA_QSIZE_MASK, 0xFFF);
    }

    #[test]
    fn admin_depth_out_of_range_in_queue_error_taxonomy() {
        assert_ne!(
            QueueError::AdminDepthOutOfRange,
            QueueError::ControllerNotReady
        );
        assert_ne!(
            QueueError::AdminDepthOutOfRange,
            QueueError::QueueBaseMisaligned
        );
    }

    #[test]
    fn enable_disable_round_trip_preserves_cc_iosqes_iocqes() {
        // Simulate: bring-up disables controller, reprograms ASQ/ACQ
        // base, re-enables. The IOSQES/IOCQES bits MUST survive
        // the EN→0→EN transition.
        let cc_initial: u32 = CC_EN_BIT | (6 << 16) | (4 << 20);
        let mut reader = ScriptedMmioRead::default();
        // Disable
        reader.push(CC_OFFSET, cc_initial);
        reader.push(CSTS_OFFSET, 0);
        // Enable (writer's view of the disabled CC becomes the
        // reader's next CC read).
        let cc_post_disable: u32 = cc_initial & !CC_EN_BIT;
        reader.push(CC_OFFSET, cc_post_disable);
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        let mut writer = MockMmioBackend::default();

        disable_controller(&mut writer, &mut reader, 16).unwrap();
        let final_cc = enable_controller(&mut writer, &mut reader, 16).unwrap();

        // Final CC has EN set and the IOSQES/IOCQES bits the
        // initial CC carried.
        assert_eq!(final_cc & CC_EN_BIT, CC_EN_BIT);
        assert_eq!((final_cc >> 16) & 0xF, 6);
        assert_eq!((final_cc >> 20) & 0xF, 4);
        // Writer recorded two CC writes (disable + enable).
        assert_eq!(writer.writes.len(), 2);
    }

    // -------------------------------------------------------------------
    // program_cc_fields (P6.7.10-pre.30)
    // -------------------------------------------------------------------

    #[test]
    fn phase_1_cc_field_constants_match_nvme_spec() {
        // NVMe 1.4 § 3.1.5 — 4 KiB pages MPS = 0, SQE 64 bytes
        // IOSQES = 6, CQE 16 bytes IOCQES = 4.
        assert_eq!(PHASE_1_MPS_LOG2, 0);
        assert_eq!(PHASE_1_IOSQES_LOG2, 6);
        assert_eq!(PHASE_1_IOCQES_LOG2, 4);
    }

    #[test]
    fn program_cc_fields_writes_packed_register_value() {
        let mut writer = MockMmioBackend::default();
        program_cc_fields(
            &mut writer,
            PHASE_1_MPS_LOG2,
            PHASE_1_IOSQES_LOG2,
            PHASE_1_IOCQES_LOG2,
        )
        .unwrap();
        assert_eq!(writer.writes.len(), 1);
        let (off, val) = writer.writes.first().copied().unwrap();
        assert_eq!(off, CC_OFFSET);
        // Expected: MPS(0)<<7 | IOSQES(6)<<16 | IOCQES(4)<<20 = (6<<16) | (4<<20)
        let expected: u32 = (6u32 << 16) | (4u32 << 20);
        assert_eq!(val, expected);
        // CC.EN bit MUST be clear (helper assumes controller is
        // disabled).
        assert_eq!(val & CC_EN_BIT, 0);
        // CC.CSS bits 4..=6 must be zero (NVM command set).
        assert_eq!((val >> 4) & 0x7, 0);
        // CC.AMS bits 11..=13 must be zero (Round Robin).
        assert_eq!((val >> 11) & 0x7, 0);
    }

    #[test]
    fn program_cc_fields_with_nonzero_mps_packs_bits_seven_through_ten() {
        let mut writer = MockMmioBackend::default();
        // MPS = 5 → 4 KiB * 2^5 = 128 KiB pages (hypothetical
        // future host).
        program_cc_fields(&mut writer, 5, 6, 4).unwrap();
        let (_, val) = writer.writes.first().copied().unwrap();
        assert_eq!((val >> 7) & 0xF, 5);
    }

    #[test]
    fn program_cc_fields_rejects_mps_above_fifteen() {
        let mut writer = MockMmioBackend::default();
        let res = program_cc_fields(&mut writer, 16, 6, 4);
        assert_eq!(res, Err(QueueError::AdminDepthOutOfRange));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_cc_fields_rejects_iosqes_above_fifteen() {
        let mut writer = MockMmioBackend::default();
        let res = program_cc_fields(&mut writer, 0, 16, 4);
        assert_eq!(res, Err(QueueError::AdminDepthOutOfRange));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_cc_fields_rejects_iocqes_above_fifteen() {
        let mut writer = MockMmioBackend::default();
        let res = program_cc_fields(&mut writer, 0, 6, 16);
        assert_eq!(res, Err(QueueError::AdminDepthOutOfRange));
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn program_cc_fields_accepts_max_field_values() {
        let mut writer = MockMmioBackend::default();
        program_cc_fields(&mut writer, 0xF, 0xF, 0xF).unwrap();
        let (_, val) = writer.writes.first().copied().unwrap();
        assert_eq!((val >> 7) & 0xF, 0xF);
        assert_eq!((val >> 16) & 0xF, 0xF);
        assert_eq!((val >> 20) & 0xF, 0xF);
    }

    // -------------------------------------------------------------------
    // CSTS.CFS detection (P6.7.10-pre.31)
    // -------------------------------------------------------------------

    #[test]
    fn wait_for_csts_rdy_aborts_on_cfs() {
        let mut mmio = ScriptedMmioRead::default();
        // First read sees CFS set → helper returns ControllerFatal
        // immediately without consuming further polls.
        mmio.push(CSTS_OFFSET, CSTS_CFS_BIT);
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Err(QueueError::ControllerFatal));
        assert_eq!(mmio.cursor, 1);
    }

    #[test]
    fn wait_for_csts_rdy_aborts_on_cfs_even_with_rdy_set() {
        let mut mmio = ScriptedMmioRead::default();
        // CFS takes precedence over RDY: the controller may have
        // toggled RDY=1 during the crash latch, but CFS=1
        // overrides — the driver MUST treat this as fatal.
        mmio.push(CSTS_OFFSET, CSTS_CFS_BIT | CSTS_RDY_BIT);
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Err(QueueError::ControllerFatal));
    }

    #[test]
    fn wait_for_csts_rdy_aborts_on_cfs_mid_poll() {
        let mut mmio = ScriptedMmioRead::default();
        // Two not-ready iterations, then CFS sets in iteration 3.
        // The helper MUST surface ControllerFatal on iteration 3
        // without continuing to the would-be ready iteration 4.
        mmio.push(CSTS_OFFSET, 0);
        mmio.push(CSTS_OFFSET, 0);
        mmio.push(CSTS_OFFSET, CSTS_CFS_BIT);
        mmio.push(CSTS_OFFSET, CSTS_RDY_BIT); // would set RDY, but never reached
        let res = wait_for_csts_rdy(&mut mmio, 16);
        assert_eq!(res, Err(QueueError::ControllerFatal));
        assert_eq!(mmio.cursor, 3);
    }

    #[test]
    fn wait_for_csts_not_rdy_aborts_on_cfs() {
        let mut mmio = ScriptedMmioRead::default();
        mmio.push(CSTS_OFFSET, CSTS_CFS_BIT | CSTS_RDY_BIT);
        let res = wait_for_csts_not_rdy(&mut mmio, 16);
        assert_eq!(res, Err(QueueError::ControllerFatal));
    }

    #[test]
    fn check_controller_fatal_returns_true_when_cfs_set() {
        let mut mmio = ScriptedMmioRead::default();
        mmio.push(CSTS_OFFSET, CSTS_CFS_BIT);
        assert!(check_controller_fatal(&mut mmio));
    }

    #[test]
    fn check_controller_fatal_returns_false_when_cfs_clear() {
        let mut mmio = ScriptedMmioRead::default();
        // Other bits set but not CFS.
        mmio.push(CSTS_OFFSET, CSTS_RDY_BIT);
        assert!(!check_controller_fatal(&mut mmio));
    }

    #[test]
    fn check_controller_fatal_returns_false_on_zero_csts() {
        let mut mmio = ScriptedMmioRead::default();
        mmio.push(CSTS_OFFSET, 0);
        assert!(!check_controller_fatal(&mut mmio));
    }

    #[test]
    fn controller_fatal_distinct_from_controller_not_ready() {
        assert_ne!(QueueError::ControllerFatal, QueueError::ControllerNotReady);
        assert_ne!(QueueError::ControllerFatal, QueueError::Full);
    }

    #[test]
    fn program_cc_fields_then_enable_round_trip_preserves_initialisation() {
        // Simulate: disable_controller cleared CC, then
        // program_cc_fields wrote the canonical initialisation
        // fields, then enable_controller ORs EN. The final CC
        // MUST carry MPS + IOSQES + IOCQES from the
        // program_cc_fields step.
        let mut writer = MockMmioBackend::default();
        program_cc_fields(
            &mut writer,
            PHASE_1_MPS_LOG2,
            PHASE_1_IOSQES_LOG2,
            PHASE_1_IOCQES_LOG2,
        )
        .unwrap();
        let (_, cc_initialised) = writer.writes.first().copied().unwrap();
        // The reader returns the value the writer just wrote.
        let mut reader = ScriptedMmioRead::default();
        reader.push(CC_OFFSET, cc_initialised);
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        let final_cc = enable_controller(&mut writer, &mut reader, 16).unwrap();
        // EN bit set.
        assert_eq!(final_cc & CC_EN_BIT, CC_EN_BIT);
        // IOSQES preserved.
        assert_eq!((final_cc >> 16) & 0xF, PHASE_1_IOSQES_LOG2);
        // IOCQES preserved.
        assert_eq!((final_cc >> 20) & 0xF, PHASE_1_IOCQES_LOG2);
    }

    // -------------------------------------------------------------------
    // P6.7.10-pre.32 — image canonical bring-up sequence composition
    //
    // The three tests below verify the exact ordering the live
    // `omni-driver-nvme-image::_start` runs at steps 4.9..=4.13:
    //
    //   disable_controller
    //   → program_admin_queue_bases (AQA, ASQ_lo/hi, ACQ_lo/hi)
    //   → program_cc_fields         (CC initialisation)
    //   → enable_controller         (CC.EN | poll CSTS.RDY)
    //   → check_controller_fatal    (CSTS.CFS tripwire)
    //
    // The image binary itself has no test harness (it is `no_main +
    // no_std`), so the host-side test below exercises the same call
    // sequence against the existing MockMmioBackend +
    // ScriptedMmioRead pair to guarantee the composition is sound.
    // -------------------------------------------------------------------

    /// Image-side canonical bring-up sequence: every helper is called
    /// in the order `_start` calls it. The writer recorder MUST hold
    /// the writes in the exact order:
    ///
    /// 1. CC      (disable_controller clears EN)
    /// 2. AQA     (program_admin_queue_bases)
    /// 3. ASQ_lo
    /// 4. ASQ_hi
    /// 5. ACQ_lo
    /// 6. ACQ_hi
    /// 7. CC      (program_cc_fields writes initialisation fields)
    /// 8. CC      (enable_controller ORs the EN bit on)
    ///
    /// — and the final CC value MUST carry EN + the initialisation
    /// fields from the program_cc_fields step.
    #[test]
    fn image_canonical_bringup_writes_aqa_asq_acq_cc_in_image_order() {
        // ScriptedMmioRead supplies the reads the helpers do:
        // - disable_controller : 1 CC read + 1 CSTS read (RDY = 0)
        // - enable_controller  : 1 CC read + 1 CSTS read (RDY = 1)
        // - check_controller_fatal : 1 CSTS read (CFS = 0)
        let mut reader = ScriptedMmioRead::default();
        // disable_controller pre-read CC: initial value has EN set
        // so disable has visible work to do.
        let cc_initial: u32 = CC_EN_BIT;
        reader.push(CC_OFFSET, cc_initial);
        // disable_controller CSTS poll: RDY clears immediately.
        reader.push(CSTS_OFFSET, 0);
        // enable_controller pre-read CC: returns the value
        // program_cc_fields just wrote (the mock writer keeps no
        // state visible to the reader so we synthesise it here).
        let cc_initialised: u32 =
            (PHASE_1_IOSQES_LOG2 << CC_IOSQES_SHIFT) | (PHASE_1_IOCQES_LOG2 << CC_IOCQES_SHIFT);
        reader.push(CC_OFFSET, cc_initialised);
        // enable_controller CSTS poll: RDY sets immediately.
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        // check_controller_fatal: CSTS has RDY = 1, CFS = 0 → false.
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);

        let mut writer = MockMmioBackend::default();

        // Step 4.9 — disable.
        disable_controller(&mut writer, &mut reader, 16).expect("disable");
        // Step 4.10 — admin queue bases.
        program_admin_queue_bases(&mut writer, 0x0, 0x1000, 64, 64)
            .expect("program_admin_queue_bases");
        // Step 4.11 — CC fields.
        program_cc_fields(
            &mut writer,
            PHASE_1_MPS_LOG2,
            PHASE_1_IOSQES_LOG2,
            PHASE_1_IOCQES_LOG2,
        )
        .expect("program_cc_fields");
        // Step 4.12 — enable.
        let final_cc = enable_controller(&mut writer, &mut reader, 16).expect("enable");
        // Step 4.13 — fatal-status tripwire.
        let fatal = check_controller_fatal(&mut reader);

        // 8 writes total in the canonical order.
        assert_eq!(writer.writes.len(), 8, "expected 8 register writes");
        // 1. CC disable (EN cleared).
        assert_eq!(writer.writes.first().copied(), Some((CC_OFFSET, 0)));
        // 2. AQA = (64-1) | ((64-1) << 16).
        assert_eq!(
            writer.writes.get(1).copied(),
            Some((AQA_OFFSET, 63 | (63 << 16)))
        );
        // 3..=6. ASQ + ACQ split into 32-bit lo/hi pairs.
        assert_eq!(writer.writes.get(2).copied(), Some((ASQ_OFFSET, 0x0)));
        assert_eq!(writer.writes.get(3).copied(), Some((ASQ_OFFSET + 4, 0x0)));
        assert_eq!(writer.writes.get(4).copied(), Some((ACQ_OFFSET, 0x1000)));
        assert_eq!(writer.writes.get(5).copied(), Some((ACQ_OFFSET + 4, 0x0)));
        // 7. CC initialisation: EN = 0, IOSQES = 6, IOCQES = 4.
        let cc_init_expected: u32 =
            (PHASE_1_IOSQES_LOG2 << CC_IOSQES_SHIFT) | (PHASE_1_IOCQES_LOG2 << CC_IOCQES_SHIFT);
        let cc_init_write = writer.writes.get(6).copied().expect("CC init write");
        assert_eq!(cc_init_write, (CC_OFFSET, cc_init_expected));
        assert_eq!(cc_init_write.1 & CC_EN_BIT, 0);
        // 8. CC enable: EN | IOSQES | IOCQES.
        let cc_enable_write = writer.writes.get(7).copied().expect("CC enable write");
        assert_eq!(cc_enable_write.0, CC_OFFSET);
        assert_eq!(cc_enable_write.1, cc_init_expected | CC_EN_BIT);

        // enable_controller returned the final CC with EN set and
        // initialisation fields preserved.
        assert_eq!(final_cc, cc_init_expected | CC_EN_BIT);
        assert_eq!((final_cc >> CC_IOSQES_SHIFT) & 0xF, PHASE_1_IOSQES_LOG2);
        assert_eq!((final_cc >> CC_IOCQES_SHIFT) & 0xF, PHASE_1_IOCQES_LOG2);

        // Tripwire reports a healthy controller.
        assert!(!fatal, "CFS = 0 → check_controller_fatal must be false");
    }

    /// Image-side tripwire success path: when `CSTS.CFS = 1` after
    /// `enable_controller` returns, `check_controller_fatal` MUST
    /// return `true` so the image bails with
    /// `EXIT_NVME_CONTROLLER_FATAL` instead of advancing the FSM
    /// into admin commands that would hang.
    #[test]
    fn image_canonical_bringup_check_controller_fatal_returns_true_when_cfs_set_post_enable() {
        let mut reader = ScriptedMmioRead::default();
        // enable_controller: pre-read CC = 0, poll → RDY = 1.
        reader.push(CC_OFFSET, 0);
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT);
        // check_controller_fatal: CSTS has both RDY and CFS set.
        // enable_controller's RDY-only check accepts this, so the
        // tripwire is the ONLY layer that catches the fatal state.
        reader.push(CSTS_OFFSET, CSTS_RDY_BIT | CSTS_CFS_BIT);

        let mut writer = MockMmioBackend::default();
        enable_controller(&mut writer, &mut reader, 16).expect("enable succeeds on RDY = 1");
        assert!(
            check_controller_fatal(&mut reader),
            "CFS = 1 post-enable → tripwire MUST fire"
        );
    }

    /// Defensive composition test: `program_cc_fields` writes ONLY
    /// to `CC_OFFSET`. Verifying this prevents a future refactor
    /// from accidentally touching `AQA`/`ASQ`/`ACQ` (which would
    /// silently clobber `program_admin_queue_bases`'s writes that
    /// run immediately before).
    #[test]
    fn program_cc_fields_write_targets_cc_offset_only() {
        let mut writer = MockMmioBackend::default();
        program_cc_fields(
            &mut writer,
            PHASE_1_MPS_LOG2,
            PHASE_1_IOSQES_LOG2,
            PHASE_1_IOCQES_LOG2,
        )
        .expect("program_cc_fields");
        assert_eq!(writer.writes.len(), 1, "exactly one register write");
        let (off, _val) = writer.writes.first().copied().expect("first write");
        assert_eq!(
            off, CC_OFFSET,
            "program_cc_fields must target CC_OFFSET only"
        );
        // The write does not touch any admin-queue register window.
        for (off, _) in &writer.writes {
            assert_ne!(*off, AQA_OFFSET, "must not clobber AQA");
            assert_ne!(*off, ASQ_OFFSET, "must not clobber ASQ_lo");
            assert_ne!(*off, ASQ_OFFSET + 4, "must not clobber ASQ_hi");
            assert_ne!(*off, ACQ_OFFSET, "must not clobber ACQ_lo");
            assert_ne!(*off, ACQ_OFFSET + 4, "must not clobber ACQ_hi");
        }
    }
}
