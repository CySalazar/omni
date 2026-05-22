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

use crate::admin::{ADMIN_SQE_BYTES, AdminSqe};
use crate::controller_regs::sq_tail_doorbell_offset;
use crate::ring::{RingError, SqRing};

// =============================================================================
// MmioBackend — abstract doorbell sink
// =============================================================================

/// Abstract MMIO sink for doorbell writes.
///
/// The live driver implements this with a `volatile_write` to the
/// controller's BAR0 page; host tests implement it with an in-memory
/// recorder for assertion. The trait is deliberately minimal — one
/// method, no read side, no error type — so a future
/// `volatile_read` for status-register polling lands as a separate
/// trait without breaking the doorbell-write surface.
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
    /// The [`SqRing`] is full and no more commands can be submitted
    /// until the controller drains a completion.
    Full,
    /// The doorbell offset overflowed `usize` per
    /// [`crate::controller_regs::sq_tail_doorbell_offset`]
    /// (unreachable in well-formed code; defensive sentinel).
    DoorbellOffsetOverflow,
}

impl From<RingError> for QueueError {
    fn from(err: RingError) -> Self {
        Self::Ring(err)
    }
}

// =============================================================================
// AdminQueuePair — SQ-only scaffold (pre.11)
// =============================================================================

/// Admin queue pair scaffold — Phase 1 submission half.
///
/// Owns the local [`SqRing`] and the controller-side doorbell-array
/// layout parameters (`doorbell_base` is the MMIO offset of the
/// doorbell array per NVMe 1.4 § 3.1.21 — typically
/// [`crate::controller_regs::DOORBELL_ARRAY_OFFSET`] = `0x1000`;
/// `dstrd` is the `CAP.DSTRD` field that scales the per-doorbell
/// stride).
///
/// The completion half (CQ ring + CQ head doorbell write) lands in
/// a sibling sub-slice; this scaffold is sufficient to exercise the
/// SQE-into-page write + tail-doorbell write path host-side.
#[derive(Debug)]
pub struct AdminQueuePair {
    sq: SqRing,
    dstrd: u8,
}

impl AdminQueuePair {
    /// Admin queue ID per NVMe 1.4 § 3.1.21 — always `0` (the admin
    /// queue is the controller's bootstrap queue and lives at the
    /// head of the doorbell array).
    pub const ADMIN_QID: u16 = 0;

    /// Construct an empty admin queue pair.
    ///
    /// # Errors
    ///
    /// - [`QueueError::Ring`] wrapping any [`RingError`] the
    ///   underlying [`SqRing::new`] surfaces (capacity zero or
    ///   beyond `u16::MAX`).
    pub fn new(sq_depth: u32, dstrd: u8) -> Result<Self, QueueError> {
        let sq = SqRing::new(sq_depth)?;
        Ok(Self { sq, dstrd })
    }

    /// Borrow the underlying [`SqRing`] for read-only introspection
    /// (used by host tests and the future drain-side wiring).
    #[must_use]
    pub const fn sq(&self) -> &SqRing {
        &self.sq
    }

    /// Returns the configured doorbell stride (`CAP.DSTRD`).
    #[must_use]
    pub const fn dstrd(&self) -> u8 {
        self.dstrd
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
        let doorbell = sq_tail_doorbell_offset(Self::ADMIN_QID, self.dstrd)
            .ok_or(QueueError::DoorbellOffsetOverflow)?;

        // Claim a slot through the ring. The slot index is bounded
        // by `capacity - 1`; the slot-byte-range bounds therefore
        // fit in the SQ page by the check above.
        let slot = self.sq.submit().ok_or(QueueError::Full)?;

        let start = (slot as usize) * ADMIN_SQE_BYTES;
        let end = start + ADMIN_SQE_BYTES;
        let dest = sq_page.get_mut(start..end).ok_or(QueueError::SqPageTooSmall)?;
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
    /// Called from the future CQE drain loop with the `sq_head`
    /// field parsed from a matching completion entry. Frees ring
    /// slots so subsequent [`Self::submit`] calls succeed.
    pub fn record_head_observed(&mut self, head: u16) {
        self.sq.update_head(head);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::encode_identify;
    use omni_types::nvme::IdentifyTarget;
    use alloc::vec;
    use alloc::vec::Vec;

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
        AdminQueuePair::new(sq_depth, dstrd).expect("ctor")
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
        let res = AdminQueuePair::new(0, 0);
        assert_eq!(res.err(), Some(QueueError::Ring(RingError::CapacityZero)));
    }

    #[test]
    fn admin_queue_new_propagates_ring_error_for_oversized_depth() {
        let res = AdminQueuePair::new(u32::from(u16::MAX) + 1, 0);
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
        let s1 = page
            .get(ADMIN_SQE_BYTES..2 * ADMIN_SQE_BYTES)
            .unwrap();
        assert_eq!(s0, sqe_a.as_bytes());
        assert_eq!(s1, sqe_b.as_bytes());
        assert_ne!(s0, s1, "two distinct SQEs must occupy distinct slots");
    }
}
