//! Inter-process communication primitives.
//!
//! ## Status
//!
//! P6.5/P6.6 scaffold. Message-passing IPC with typed envelopes and
//! capability-gated send/receive.
//!
//! ## Design rationale
//!
//! - **Typed messages.** The IPC layer carries opaque byte slices on the
//!   wire (the kernel is type-agnostic), but each message slot is
//!   tagged with a `MessageKind` discriminant for fast triage. The
//!   sender's userspace stub (in `omni-sdk`) handles serialization.
//! - **Capability-gated send.** A task may only send a message to a
//!   channel for which it presents a valid capability. The capability
//!   names the action (`SEND` / `RECEIVE`) and the target channel.
//! - **Bounded queues.** Each channel has a fixed-size queue. Sends to
//!   a full queue either block, fail, or evict the oldest message
//!   depending on the channel's policy. The policy is set at channel
//!   creation; it cannot be changed without destroying and recreating
//!   the channel.
//! - **TEE awareness.** A channel can be marked as TEE-bound: messages
//!   are encrypted with a key sealed to the recipient's TEE measurement.
//!   The kernel does not see the plaintext; it routes ciphertext.

#![allow(
    clippy::missing_errors_doc,
    reason = "trait scaffold methods return NotYetImplemented until MB12 activates IPC"
)]

use alloc::vec::Vec;

use crate::{KernelResult, scheduling::TaskId};

// -----------------------------------------------------------------------------
// Channel identifier
// -----------------------------------------------------------------------------

/// IPC channel identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelId(pub u64);

// -----------------------------------------------------------------------------
// Message kind
// -----------------------------------------------------------------------------

/// Discriminant for the kind of message.
///
/// Used for fast triage; deeper deserialization is the receiver's
/// responsibility. The set is intentionally small; adding a variant
/// requires an OIP.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKind {
    /// Generic request expecting a reply.
    Request = 1,
    /// Reply to a previous request.
    Reply = 2,
    /// Asynchronous notification (no reply expected).
    Notification = 3,
    /// Capability passing — the message carries a capability handle.
    CapabilityHandoff = 4,
    /// Shared-memory grant.
    SharedMemoryGrant = 5,
}

// -----------------------------------------------------------------------------
// Channel policy
// -----------------------------------------------------------------------------

/// What the channel does when its queue is full on a send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackpressurePolicy {
    /// The sender's send call blocks until queue space frees up.
    Block,
    /// The send call returns [`crate::KernelError::ResourceExhausted`].
    Drop,
    /// The oldest queued message is evicted to make room.
    EvictOldest,
}

/// Per-channel configuration. Set at channel creation; immutable
/// thereafter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPolicy {
    /// Maximum number of in-flight messages on this channel.
    pub queue_depth: usize,
    /// What to do on a full queue.
    pub backpressure: BackpressurePolicy,
    /// Whether the channel is TEE-bound (messages are sealed to the
    /// recipient's TEE).
    pub tee_bound: bool,
}

// -----------------------------------------------------------------------------
// Message envelope
// -----------------------------------------------------------------------------

/// Kernel-side message envelope.
///
/// The `payload` is opaque to the kernel; userspace is responsible for
/// serialization. The envelope is allocated in a kernel-private buffer
/// pool and copied out to the receiver's address space on `receive`.
///
/// **Why a copy** (versus shared memory): copy ensures the sender cannot
/// continue to modify the message after the kernel has accepted it,
/// which is necessary for the capability invariant. Shared-memory
/// regions are a separate mechanism (`MessageKind::SharedMemoryGrant`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEnvelope {
    /// The sender task (filled in by the kernel; the sender cannot
    /// forge this).
    pub sender: TaskId,
    /// The channel.
    pub channel: ChannelId,
    /// The message kind.
    pub kind: MessageKind,
    /// Opaque payload. Length-limited per channel policy.
    pub payload: Vec<u8>,
}

// -----------------------------------------------------------------------------
// IPC trait
// -----------------------------------------------------------------------------

/// The IPC subsystem trait.
///
/// One `dyn Ipc` instance lives in the kernel. Multiple impls are
/// possible (e.g., a fast in-process impl for unit tests, a real one
/// backed by per-CPU queues for production); the trait keeps consumers
/// agnostic.
pub trait Ipc {
    /// Creates a new channel with the given policy. Returns the
    /// freshly-allocated channel identifier.
    fn create_channel(&mut self, policy: ChannelPolicy) -> KernelResult<ChannelId>;

    /// Destroys a channel. Pending messages are dropped.
    fn destroy_channel(&mut self, channel: ChannelId) -> KernelResult<()>;

    /// Sends `envelope` on its embedded `channel`. The kernel validates
    /// the sender's capability and enforces the channel's policy.
    fn send(&mut self, envelope: MessageEnvelope) -> KernelResult<()>;

    /// Receives a message on `channel`. Returns `None` if the queue is
    /// empty; blocking is the caller's responsibility (the scheduler
    /// transitions the task to `BlockedOnIpc` when desired).
    fn receive(&mut self, channel: ChannelId) -> KernelResult<Option<MessageEnvelope>>;

    /// Returns the current queue depth on `channel`.
    fn queue_depth(&self, channel: ChannelId) -> KernelResult<usize>;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn message_kind_fits_in_one_byte() {
        assert_eq!(core::mem::size_of::<MessageKind>(), 1);
    }

    #[test]
    fn envelope_round_trip() {
        let e = MessageEnvelope {
            sender: TaskId(7),
            channel: ChannelId(42),
            kind: MessageKind::Request,
            payload: vec![1, 2, 3],
        };
        assert_eq!(e.sender, TaskId(7));
        assert_eq!(e.channel, ChannelId(42));
        assert_eq!(e.kind, MessageKind::Request);
        assert_eq!(e.payload, vec![1, 2, 3]);
    }

    #[test]
    fn channel_policy_carries_tee_bit() {
        let p = ChannelPolicy {
            queue_depth: 16,
            backpressure: BackpressurePolicy::Block,
            tee_bound: true,
        };
        assert!(p.tee_bound);
        assert_eq!(p.backpressure, BackpressurePolicy::Block);
    }
}
