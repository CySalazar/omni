//! NVMe IO Queue Pair session — BLK ↔ NVMe glue.
//!
//! Composes [`crate::queue::AdminQueuePair`] (re-purposed for a
//! non-admin queue id via [`AdminQueuePair::new_for_qid`]) with the
//! BLK ↔ NVMe gateway from [`crate::blk_gateway`] so the
//! file-system service consumes a high-level
//! [`BlkRequest`] → [`BlkResponse`] surface without ever touching
//! the NVMe wire format directly.
//!
//! ## Phase-1 scope
//!
//! Phase-1 driver allocates exactly one IO queue pair (qid = 1)
//! per OIP-Driver-NVMe-014 § R2. Multi-queue support (qid `2..=4`
//! per § R5) lands as a future OIP without breaking the session
//! shape used today — the constructor accepts an arbitrary qid.
//!
//! ## What this module does NOT do
//!
//! - It does not validate `lba` against namespace size or `count`
//!   against [`omni_types::blk::MAX_BLOCK_COUNT_PER_REQUEST`].
//!   Those checks happen at the BLK channel boundary; the
//!   session trusts the caller.
//! - It does not own the PRP-list buffer for multi-page transfers
//!   (`block_count > 2` per
//!   [`crate::transfer_model::prp_layout`]). The caller composes
//!   the PRP layout via [`crate::transfer_model`] helpers and
//!   passes the resulting `prp1` / `prp2` pair to
//!   [`IoSession::submit_blk_request`].

use alloc::vec;
use alloc::vec::Vec;

use omni_types::blk::{BlkRequest, BlkResponse};

use crate::admin::{ADMIN_CQE_BYTES, ADMIN_SQE_BYTES};
use crate::blk_gateway::{cqe_to_blk_response, encode_blk_request};
use crate::queue::{AdminQueuePair, MmioBackend, QueueError};

/// Phase-1 IO queue identifier (single queue per OIP-014 § R2).
pub const PHASE_1_IO_QID: u16 = 1;

/// Per-session bookkeeping for the NVMe IO queue pair.
///
/// Mirrors the [`crate::admin_session::AdminSession`] pattern:
/// owns the SQ + CQ data pages (allocated as `Vec<u8>` for the
/// host harness; the live driver swaps them for `DmaMap`-backed
/// IOVA slices through a future trait) and a monotone CID counter
/// that wraps at `u16::MAX` skipping zero (reserved per
/// `omni_types::nvme::RESERVED_DRIVER_OPAQUE_ID`).
#[derive(Debug)]
pub struct IoSession {
    queue_pair: AdminQueuePair,
    sq_page: Vec<u8>,
    cq_page: Vec<u8>,
    next_cid: u16,
    nsid: u32,
}

impl IoSession {
    /// Construct a fresh IO session bound to the supplied qid +
    /// namespace.
    ///
    /// Phase-1 callers should pass `qid = PHASE_1_IO_QID` (1) and
    /// `nsid = 1` (the single active namespace).
    ///
    /// # Errors
    ///
    /// - [`QueueError::Ring`] wrapping any underlying
    ///   [`crate::ring::RingError`] (capacity zero or > `u16::MAX`).
    pub fn new(
        qid: u16,
        nsid: u32,
        sq_depth: u32,
        cq_depth: u32,
        dstrd: u8,
    ) -> Result<Self, QueueError> {
        let queue_pair = AdminQueuePair::new_for_qid(qid, sq_depth, cq_depth, dstrd)?;
        let sq_page = vec![0u8; (sq_depth as usize) * ADMIN_SQE_BYTES];
        let cq_page = vec![0u8; (cq_depth as usize) * ADMIN_CQE_BYTES];
        Ok(Self {
            queue_pair,
            sq_page,
            cq_page,
            next_cid: 1,
            nsid,
        })
    }

    /// Borrow the underlying queue pair for read-only
    /// introspection.
    #[must_use]
    pub const fn queue_pair(&self) -> &AdminQueuePair {
        &self.queue_pair
    }

    /// Borrow the SQ data page (read-only).
    #[must_use]
    pub fn sq_page(&self) -> &[u8] {
        &self.sq_page
    }

    /// Borrow the CQ data page mutably (host tests inject
    /// synthetic completions through this accessor).
    pub fn cq_page_mut(&mut self) -> &mut [u8] {
        &mut self.cq_page
    }

    /// Returns the namespace the session targets.
    #[must_use]
    pub const fn nsid(&self) -> u32 {
        self.nsid
    }

    /// Allocate the next CID and advance the monotone counter
    /// (wrapping at `u16::MAX`, skipping the reserved zero).
    fn allocate_cid(&mut self) -> u16 {
        let cid = self.next_cid;
        self.next_cid = self.next_cid.checked_add(1).unwrap_or(1);
        cid
    }

    /// Encode + submit a [`BlkRequest`] onto the IO submission
    /// queue. Returns the assigned CID for the matching
    /// completion.
    ///
    /// The caller derives `prp1` / `prp2` from the request's
    /// `buf_iova` via [`crate::transfer_model::prp_layout`] +
    /// [`crate::transfer_model::prp1_for`] +
    /// [`crate::transfer_model::prp2_for`]. The session does not
    /// own the PRP-list page (caller-supplied for
    /// `block_count > 2`).
    ///
    /// Returns `None` when [`encode_blk_request`] rejects the
    /// request (currently only `#[non_exhaustive]` future
    /// variants); the caller MUST then emit
    /// `BlkResponse::NotSupported` to the BLK channel client.
    ///
    /// # Errors
    ///
    /// Propagates any [`QueueError`] the underlying
    /// [`AdminQueuePair::submit`] surfaces (`Full`,
    /// `SqPageTooSmall`, `DoorbellOffsetOverflow`).
    pub fn submit_blk_request<M: MmioBackend>(
        &mut self,
        req: BlkRequest,
        prp1: u64,
        prp2: u64,
        mmio: &mut M,
    ) -> Result<Option<u16>, QueueError> {
        let cid = self.allocate_cid();
        let Some(sqe) = encode_blk_request(req, cid, self.nsid, prp1, prp2) else {
            return Ok(None);
        };
        self.queue_pair.submit(&sqe, mmio, &mut self.sq_page)?;
        Ok(Some(cid))
    }

    /// Drain the CQ for the matching `cid` and translate the
    /// completion to a [`BlkResponse`].
    ///
    /// Sibling completions for other in-flight CIDs are silently
    /// discarded. The poll loop terminates after `poll_limit`
    /// drained CQEs without a match — at which point the helper
    /// returns `Ok(None)`, mirroring
    /// [`crate::admin_session::AdminSession::poll_completion_for_cid`].
    ///
    /// # Errors
    ///
    /// Propagates any [`QueueError`] the underlying
    /// [`AdminQueuePair::drain_completion`] surfaces.
    pub fn poll_blk_response_for_cid<M: MmioBackend>(
        &mut self,
        cid: u16,
        poll_limit: u32,
        mmio: &mut M,
    ) -> Result<Option<BlkResponse>, QueueError> {
        for _ in 0..poll_limit {
            if let Some(fields) = self.queue_pair.drain_completion(mmio, &self.cq_page)? {
                if fields.cid == cid {
                    return Ok(Some(cqe_to_blk_response(&fields)));
                }
            }
        }
        Ok(None)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::AdminCqeFields;
    use crate::controller_regs::{cq_head_doorbell_offset, sq_tail_doorbell_offset};

    #[derive(Debug, Default)]
    struct BootstrapFake {
        writes: Vec<(usize, u32)>,
    }

    impl MmioBackend for BootstrapFake {
        fn write_doorbell(&mut self, offset: usize, value: u32) {
            self.writes.push((offset, value));
        }
    }

    /// Write a synthetic CQE at slot `slot` carrying the supplied
    /// CID + phase + status. The default status (0,0) means
    /// success.
    fn write_synthetic_cqe(
        page: &mut [u8],
        slot: usize,
        cid: u16,
        phase: bool,
        sct: u8,
        sc: u8,
    ) {
        let start = slot * ADMIN_CQE_BYTES;
        let end = start + ADMIN_CQE_BYTES;
        let dest = page.get_mut(start..end).expect("slot in range");
        for byte in dest.iter_mut() {
            *byte = 0;
        }
        // CDW3 = CID | status_word << 16 where status_word's bit 0
        // is the phase tag, bits 1..=8 are SC, bits 9..=11 are SCT.
        let status_word: u16 = u16::from(phase)
            | (u16::from(sc) << 1)
            | (u16::from(sct & 0b111) << 9);
        let cdw3: u32 = u32::from(cid) | (u32::from(status_word) << 16);
        let mut chunks = dest.chunks_exact_mut(4);
        let _ = chunks.next(); // CDW0
        let _ = chunks.next(); // CDW1
        let _ = chunks.next(); // CDW2
        chunks
            .next()
            .expect("cdw3")
            .copy_from_slice(&cdw3.to_le_bytes());
    }

    // -------------------------------------------------------------------
    // Construction & routing
    // -------------------------------------------------------------------

    #[test]
    fn phase_1_io_qid_constant_is_one() {
        // Tripwire per OIP-NVMe-014 § R2 (Phase-1 single IO queue
        // lives at qid = 1; qid = 0 is reserved for the admin
        // queue per § 3.1.21).
        assert_eq!(PHASE_1_IO_QID, 1);
    }

    #[test]
    fn io_session_new_records_qid_and_nsid() {
        let s = IoSession::new(PHASE_1_IO_QID, 1, 8, 8, 0).expect("ctor");
        assert_eq!(s.queue_pair().qid(), 1);
        assert_eq!(s.nsid(), 1);
    }

    #[test]
    fn io_session_doorbell_routes_to_io_qid() {
        // Submit one BlkRequest::Flush so the recorder captures
        // the SQ tail doorbell offset. The offset MUST match the
        // qid=1 SQ doorbell, NOT the qid=0 admin doorbell.
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 8, 8, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        s.submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio).unwrap();
        let (off, _) = mmio.writes.first().copied().expect("doorbell written");
        let io_sq_offset = sq_tail_doorbell_offset(PHASE_1_IO_QID, 0).unwrap();
        let admin_sq_offset = sq_tail_doorbell_offset(0, 0).unwrap();
        assert_eq!(off, io_sq_offset);
        assert_ne!(off, admin_sq_offset, "MUST route to IO doorbell, not admin");
    }

    // -------------------------------------------------------------------
    // submit_blk_request — full round-trip
    // -------------------------------------------------------------------

    #[test]
    fn submit_blk_request_returns_assigned_cid() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 8, 8, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let cid_opt = s
            .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
            .unwrap();
        assert_eq!(cid_opt, Some(1));
    }

    #[test]
    fn submit_blk_request_encodes_nsid_into_sqe() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 7, 8, 8, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        s.submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio).unwrap();
        // NSID at SQE bytes 4..=7 (little-endian).
        let sqe = s.sq_page().get(0..ADMIN_SQE_BYTES).unwrap();
        let mut tmp = [0u8; 4];
        tmp.copy_from_slice(sqe.get(4..8).unwrap());
        assert_eq!(u32::from_le_bytes(tmp), 7);
    }

    #[test]
    fn submit_blk_request_read_encodes_nvm_read_opcode() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 8, 8, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let req = BlkRequest::Read {
            lba: 0x100,
            count: 4,
            buf_iova: 0x1_0000,
        };
        s.submit_blk_request(req, 0x1_0000, 0, &mut mmio).unwrap();
        let sqe = s.sq_page().get(0..ADMIN_SQE_BYTES).unwrap();
        assert_eq!(
            sqe.first().copied().unwrap(),
            crate::io::OPC_NVM_READ
        );
    }

    // -------------------------------------------------------------------
    // poll_blk_response_for_cid — translation
    // -------------------------------------------------------------------

    #[test]
    fn poll_blk_response_returns_ok_on_successful_cqe() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let cid = s
            .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
            .unwrap()
            .unwrap();
        // Inject a successful CQE at slot 0 with phase=true.
        write_synthetic_cqe(s.cq_page_mut(), 0, cid, true, 0, 0);
        let mut nop = BootstrapFake::default();
        let resp = s
            .poll_blk_response_for_cid(cid, 16, &mut nop)
            .unwrap()
            .unwrap();
        assert_eq!(resp, BlkResponse::Ok);
        // The drain rang the IO CQ head doorbell (not the admin).
        let (off, _) = nop.writes.first().copied().unwrap();
        let io_cq_offset = cq_head_doorbell_offset(PHASE_1_IO_QID, 0).unwrap();
        assert_eq!(off, io_cq_offset);
    }

    #[test]
    fn poll_blk_response_returns_invalid_argument_on_sc_02() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let cid = s
            .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
            .unwrap()
            .unwrap();
        // SCT=0, SC=0x02 → Invalid Field in Command.
        write_synthetic_cqe(s.cq_page_mut(), 0, cid, true, 0, 0x02);
        let mut nop = BootstrapFake::default();
        let resp = s
            .poll_blk_response_for_cid(cid, 16, &mut nop)
            .unwrap()
            .unwrap();
        assert_eq!(resp, BlkResponse::InvalidArgument);
    }

    #[test]
    fn poll_blk_response_returns_out_of_range_on_sc_80() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let cid = s
            .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
            .unwrap()
            .unwrap();
        // SCT=0, SC=0x80 → LBA Out of Range.
        write_synthetic_cqe(s.cq_page_mut(), 0, cid, true, 0, 0x80);
        let mut nop = BootstrapFake::default();
        let resp = s
            .poll_blk_response_for_cid(cid, 16, &mut nop)
            .unwrap()
            .unwrap();
        assert_eq!(resp, BlkResponse::OutOfRange);
    }

    #[test]
    fn poll_blk_response_returns_device_error_on_other_status() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let cid = s
            .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
            .unwrap()
            .unwrap();
        // SCT=2 (Command-Specific), SC=0x82.
        write_synthetic_cqe(s.cq_page_mut(), 0, cid, true, 2, 0x82);
        let mut nop = BootstrapFake::default();
        let resp = s
            .poll_blk_response_for_cid(cid, 16, &mut nop)
            .unwrap()
            .unwrap();
        match resp {
            BlkResponse::DeviceError(status) => {
                // Packed: (SCT << 8) | SC = (2 << 8) | 0x82 = 0x0282.
                assert_eq!(status, 0x0282);
            }
            _ => panic!("expected DeviceError, got {resp:?}"),
        }
    }

    #[test]
    fn poll_blk_response_returns_none_when_no_matching_completion() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mmio = BootstrapFake::default();
        // No completion injected → drain returns None each
        // iteration → helper returns Ok(None) after exhausting
        // poll budget.
        let mut nop = BootstrapFake::default();
        let resp = s.poll_blk_response_for_cid(99, 4, &mut nop).unwrap();
        assert!(resp.is_none());
        let _ = mmio;
    }

    // -------------------------------------------------------------------
    // End-to-end: submit BLK Read → synthetic completion → poll
    // -------------------------------------------------------------------

    #[test]
    fn submit_then_poll_round_trips_blk_read_to_ok() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 4, 4, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        let req = BlkRequest::Read {
            lba: 0x100,
            count: 1,
            buf_iova: 0x1_0000,
        };
        let cid = s
            .submit_blk_request(req, 0x1_0000, 0, &mut mmio)
            .unwrap()
            .unwrap();
        // Synthetic successful CQE.
        write_synthetic_cqe(s.cq_page_mut(), 0, cid, true, 0, 0);
        let mut nop = BootstrapFake::default();
        let resp = s
            .poll_blk_response_for_cid(cid, 16, &mut nop)
            .unwrap()
            .unwrap();
        assert_eq!(resp, BlkResponse::Ok);
        // sq_head feedback advanced the IO SqRing's head_observed.
        // Synthetic CQE carried sq_head=0 (default), so it didn't
        // actually advance — but the drain still consumed the
        // slot and rang the CQ head doorbell.
        assert_eq!(s.queue_pair().sq().tail(), 1);
    }

    #[test]
    fn cid_allocator_starts_at_one_and_advances() {
        let mut s = IoSession::new(PHASE_1_IO_QID, 1, 8, 8, 0).expect("ctor");
        let mut mmio = BootstrapFake::default();
        for expected_cid in 1..=3 {
            let cid = s
                .submit_blk_request(BlkRequest::Flush, 0, 0, &mut mmio)
                .unwrap()
                .unwrap();
            assert_eq!(cid, expected_cid);
        }
    }

    #[test]
    fn ctor_propagates_ring_error_on_zero_depth() {
        let res = IoSession::new(PHASE_1_IO_QID, 1, 0, 8, 0);
        assert!(matches!(res, Err(QueueError::Ring(_))));
    }

    // Suppress dead-code lint for the AdminCqeFields helper if
    // the imports above don't get exercised; keeping the use
    // visible for future tests that compose this fixture with
    // the cqe_to_blk_response surface directly.
    #[allow(dead_code)]
    fn _unused_admin_cqe_fields_ref(_f: AdminCqeFields) {}
}
