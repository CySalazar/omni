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
    reason = "kernel-internal IPC methods; errors mapped to syscall ABI at the boundary"
)]
#![cfg_attr(
    all(feature = "bare-metal", target_arch = "x86_64"),
    allow(
        unsafe_code,
        reason = "IPC_REGISTRY static mut singleton + addr_of_mut accessor; SAFETY documented at the fn boundary"
    )
)]

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use crate::capabilities::{
    CapabilityVerdict, KernelAction, KernelCapabilityCheck, KernelCapabilityToken, KernelPrincipal,
    KernelResource,
};
use crate::{KernelError, KernelResult, scheduling::TaskId};

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
// Wake actions — IPC↔scheduler contract
// -----------------------------------------------------------------------------

/// What the scheduler should do after the IPC layer returns.
///
/// The IPC layer never calls into the scheduler directly. Instead, each
/// fallible operation returns a [`WakeAction`] that the *caller* (the
/// syscall handler) translates into a scheduler operation:
///
/// - [`WakeAction::None`] — nothing to do.
/// - [`WakeAction::Wake(t)`] — the syscall handler calls
///   `scheduler.enqueue(t, priority)` to re-enable a previously-blocked
///   task. Used by `send` when a `receive` waiter was parked, and by
///   `receive` when a `Block`-policy `send` was parked.
/// - [`WakeAction::Block(t)`] — the syscall handler calls
///   `scheduler.yield_current(t, BlockedOnIpc)` to park the calling
///   task. Used by `send` under `Block` backpressure on a full queue,
///   and by `receive` on an empty queue with `blocking = true`.
///
/// This decoupling keeps the registry testable in `cargo test`
/// (no scheduler global needed) and the syscall layer flexible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WakeAction {
    /// Nothing to do.
    None,
    /// Re-enable a task that was waiting on this channel.
    Wake(TaskId),
    /// Park the current task until a counterpart unblocks it.
    Block(TaskId),
}

// -----------------------------------------------------------------------------
// Channel — kernel-internal per-channel state
// -----------------------------------------------------------------------------

/// Per-channel state owned by the [`KernelIpcRegistry`].
///
/// Wait queues live *inside* the channel (not lazily in the scheduler)
/// for two reasons:
///
/// 1. O(1) lookup of "who is waiting on this channel?" at send/receive
///    time.
/// 2. Local cleanup on `destroy_channel`: if the channel goes away,
///    waiters are visible right here without a scheduler walk.
#[derive(Debug)]
pub struct Channel {
    /// Kernel-allocated identifier.
    pub id: ChannelId,
    /// Per-channel policy. Immutable post-creation.
    pub policy: ChannelPolicy,
    /// The task that created this channel; only they may destroy it.
    pub owner: TaskId,
    /// Principal authorised to call `IpcSend`. `None` means the channel
    /// has no send-side authentication (dev mode); the kernel accepts
    /// any sender.
    pub send_subject: Option<KernelPrincipal>,
    /// Principal authorised to call `IpcReceive`. `None` means the
    /// channel has no recv-side authentication.
    pub recv_subject: Option<KernelPrincipal>,
    /// Messages enqueued but not yet delivered. FIFO.
    pub queue: VecDeque<MessageEnvelope>,
    /// Tasks blocked on a full queue under `BackpressurePolicy::Block`.
    pub waiters_send: VecDeque<TaskId>,
    /// Tasks blocked on an empty queue with `blocking = true`.
    pub waiters_recv: VecDeque<TaskId>,
}

impl Channel {
    /// Construct an empty channel slot. Reserved for [`KernelIpcRegistry::create_channel`].
    fn new(
        id: ChannelId,
        policy: ChannelPolicy,
        owner: TaskId,
        send_subject: Option<KernelPrincipal>,
        recv_subject: Option<KernelPrincipal>,
    ) -> Self {
        Self {
            id,
            policy,
            owner,
            send_subject,
            recv_subject,
            queue: VecDeque::new(),
            waiters_send: VecDeque::new(),
            waiters_recv: VecDeque::new(),
        }
    }

    /// Current queue length.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.queue.len()
    }
}

// -----------------------------------------------------------------------------
// KernelIpcRegistry — the singleton IPC backend
// -----------------------------------------------------------------------------

/// Kernel-internal IPC registry. One instance per kernel; bare-metal
/// builds keep it inside a `static mut`.
///
/// Backing storage is a `BTreeMap` rather than a `HashMap`: `hashbrown`
/// (the workspace's `HashMap` source) seeds `ahash` from `getrandom`,
/// which is exactly the dependency `omni-crypto`'s `rng` feature was
/// gated to avoid in bare-metal builds. `BTreeMap` is `alloc`-only,
/// deterministic, and well-suited to the small number of channels Phase
/// 1 will create (tens at most).
#[derive(Debug)]
pub struct KernelIpcRegistry {
    channels: BTreeMap<u64, Channel>,
    next_id: u64,
}

impl KernelIpcRegistry {
    /// Construct an empty registry. `const fn` so a `static mut` slot
    /// can hold one without a lazy initializer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            channels: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Number of live channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Borrow a channel by id, if it exists.
    #[must_use]
    pub fn channel(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.get(&id.0)
    }

    /// Create a new channel.
    ///
    /// `send_token` and `recv_token` are optional. When present, the
    /// kernel verifies each token via the provided
    /// [`KernelCapabilityCheck`] and memorises the embedded
    /// `subject` for later send/recv comparison. When absent, the
    /// channel is unauthenticated on that direction (developer mode).
    ///
    /// # Errors
    ///
    /// - [`KernelError::InvalidArgument`] when `policy.queue_depth` is
    ///   zero (a zero-depth channel would deadlock under `Block` and is
    ///   never useful).
    /// - [`KernelError::CapabilityDenied`] when a token is presented but
    ///   the capability check rejects it.
    /// - [`KernelError::ResourceExhausted`] when the registry's monotonic
    ///   id counter would overflow (~2^64 channels — practically never).
    pub fn create_channel<C: KernelCapabilityCheck>(
        &mut self,
        owner: TaskId,
        policy: ChannelPolicy,
        send_token: Option<KernelCapabilityToken>,
        recv_token: Option<KernelCapabilityToken>,
        verifier: &C,
    ) -> KernelResult<ChannelId> {
        if policy.queue_depth == 0 {
            return Err(KernelError::InvalidArgument);
        }

        let id_u64 = self.next_id;
        let id = ChannelId(id_u64);
        let resource = KernelResource::IpcChannel(id_u64);

        let send_subject = if let Some(tok) = send_token {
            if verifier.verify(&tok, KernelAction::IpcSend, resource) != CapabilityVerdict::Authorised
            {
                return Err(KernelError::CapabilityDenied);
            }
            Some(tok.subject)
        } else {
            None
        };

        let recv_subject = if let Some(tok) = recv_token {
            if verifier.verify(&tok, KernelAction::IpcRecv, resource) != CapabilityVerdict::Authorised
            {
                return Err(KernelError::CapabilityDenied);
            }
            Some(tok.subject)
        } else {
            None
        };

        let channel = Channel::new(id, policy, owner, send_subject, recv_subject);
        self.channels.insert(id_u64, channel);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(KernelError::ResourceExhausted)?;
        Ok(id)
    }

    /// Destroy a channel. Only the channel's `owner` may do this.
    ///
    /// Pending messages are dropped. Any task currently blocked on this
    /// channel (`waiters_send`/`waiters_recv`) is left for the caller
    /// to wake — the syscall handler that issued `IpcDestroyChannel`
    /// inherits the responsibility (Phase 1 single-CPU: typically the
    /// destroyer is also the only task that could have been waiting on
    /// the channel; multi-task destroy semantics ship with MB13).
    ///
    /// # Errors
    ///
    /// - [`KernelError::InvalidArgument`] if no such channel exists.
    /// - [`KernelError::CapabilityDenied`] if `requester != channel.owner`.
    pub fn destroy_channel(
        &mut self,
        channel: ChannelId,
        requester: TaskId,
    ) -> KernelResult<()> {
        let entry = self
            .channels
            .get(&channel.0)
            .ok_or(KernelError::InvalidArgument)?;
        if entry.owner.0 != requester.0 {
            return Err(KernelError::CapabilityDenied);
        }
        self.channels.remove(&channel.0);
        Ok(())
    }

    /// Send a message on a channel.
    ///
    /// The kernel fills `envelope.sender` from `sender_task`; the
    /// caller-supplied value (if any) is overwritten. The `requester`
    /// argument is the principal claimed by the calling task — the
    /// registry compares it against `channel.send_subject` when one is
    /// set.
    ///
    /// # Errors
    ///
    /// - [`KernelError::InvalidArgument`] if no such channel.
    /// - [`KernelError::CapabilityDenied`] if `requester` does not match
    ///   the channel's send subject.
    /// - [`KernelError::ResourceExhausted`] under
    ///   `BackpressurePolicy::Drop` when the queue is full.
    ///
    /// On `BackpressurePolicy::Block` with a full queue, the call
    /// succeeds with [`WakeAction::Block(sender_task)`] — the syscall
    /// handler must park the sender. The envelope is **not** enqueued
    /// in this case; the handler must re-issue the send when the task
    /// wakes up.
    pub fn send(
        &mut self,
        mut envelope: MessageEnvelope,
        sender_task: TaskId,
        requester: KernelPrincipal,
    ) -> KernelResult<WakeAction> {
        let channel = self
            .channels
            .get_mut(&envelope.channel.0)
            .ok_or(KernelError::InvalidArgument)?;

        if let Some(allowed) = channel.send_subject {
            if allowed != requester {
                return Err(KernelError::CapabilityDenied);
            }
        }

        envelope.sender = sender_task;
        envelope.channel = channel.id;

        let full = channel.queue.len() >= channel.policy.queue_depth;
        if full {
            match channel.policy.backpressure {
                BackpressurePolicy::Drop => return Err(KernelError::ResourceExhausted),
                BackpressurePolicy::EvictOldest => {
                    let _ = channel.queue.pop_front();
                }
                BackpressurePolicy::Block => {
                    channel.waiters_send.push_back(sender_task);
                    return Ok(WakeAction::Block(sender_task));
                }
            }
        }

        channel.queue.push_back(envelope);

        Ok(channel
            .waiters_recv
            .pop_front()
            .map_or(WakeAction::None, WakeAction::Wake))
    }

    /// Dequeue a message from a channel.
    ///
    /// Returns:
    /// - `Ok((Some(env), wake))` — a message was dequeued. `wake` is
    ///   `Wake(t)` if a `Block`-policy sender was parked on this
    ///   channel and now has space to enqueue; otherwise `None`.
    /// - `Ok((None, wake))` — the queue was empty. If `blocking` is
    ///   true, `wake` is `Block(requester_task)` and the caller must
    ///   park the task. If `blocking` is false, `wake` is `None`.
    ///
    /// # Errors
    ///
    /// - [`KernelError::InvalidArgument`] if no such channel.
    /// - [`KernelError::CapabilityDenied`] if `requester` does not match
    ///   the channel's recv subject.
    pub fn receive(
        &mut self,
        channel_id: ChannelId,
        requester_task: TaskId,
        requester: KernelPrincipal,
        blocking: bool,
    ) -> KernelResult<(Option<MessageEnvelope>, WakeAction)> {
        let channel = self
            .channels
            .get_mut(&channel_id.0)
            .ok_or(KernelError::InvalidArgument)?;

        if let Some(allowed) = channel.recv_subject {
            if allowed != requester {
                return Err(KernelError::CapabilityDenied);
            }
        }

        if let Some(env) = channel.queue.pop_front() {
            let wake = channel
                .waiters_send
                .pop_front()
                .map_or(WakeAction::None, WakeAction::Wake);
            return Ok((Some(env), wake));
        }

        if blocking {
            channel.waiters_recv.push_back(requester_task);
            Ok((None, WakeAction::Block(requester_task)))
        } else {
            Ok((None, WakeAction::None))
        }
    }

    /// Queue depth for a channel.
    ///
    /// # Errors
    ///
    /// - [`KernelError::InvalidArgument`] if no such channel.
    pub fn queue_depth(&self, channel: ChannelId) -> KernelResult<usize> {
        self.channels
            .get(&channel.0)
            .map(Channel::depth)
            .ok_or(KernelError::InvalidArgument)
    }
}

impl Default for KernelIpcRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// Singleton accessor (bare-metal only)
// -----------------------------------------------------------------------------

/// Global IPC registry. Single instance per kernel.
///
/// Mirrors the `SCHEDULER` / `FRAME_ALLOC` singleton pattern from the
/// rest of the kernel: a `static mut` rather than a `Mutex<...>` because
/// Phase 1 is single-CPU and the SYSCALL entry path masks interrupts via
/// `IA32_FMASK = 0x200`. MP introduction (Phase 2) will replace this
/// with a `Mutex` or per-CPU array — tracked in ADR-0005.
#[cfg(all(feature = "bare-metal", target_arch = "x86_64"))]
#[unsafe(no_mangle)]
static mut IPC_REGISTRY: KernelIpcRegistry = KernelIpcRegistry::new();

/// Borrow the global IPC registry mutably.
///
/// # Safety
///
/// Caller must be in a context where no other reference to
/// `IPC_REGISTRY` is live. The SYSCALL path already provides this
/// guarantee (interrupts masked + single-CPU + no recursion).
#[cfg(all(feature = "bare-metal", target_arch = "x86_64"))]
#[allow(
    clippy::mut_from_ref,
    static_mut_refs,
    reason = "single-CPU kernel singleton; SAFETY documented at the call site"
)]
pub unsafe fn ipc_registry_mut() -> &'static mut KernelIpcRegistry {
    // SAFETY: caller invariant — see fn doc.
    unsafe {
        let p = core::ptr::addr_of_mut!(IPC_REGISTRY);
        &mut *p
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::StubCapabilityProvider;
    use alloc::vec;

    fn principal(b: u8) -> KernelPrincipal {
        KernelPrincipal::from_bytes([b; 32])
    }

    fn open_policy(depth: usize, bp: BackpressurePolicy) -> ChannelPolicy {
        ChannelPolicy {
            queue_depth: depth,
            backpressure: bp,
            tee_bound: false,
        }
    }

    fn make_envelope(channel: ChannelId, payload: &[u8]) -> MessageEnvelope {
        MessageEnvelope {
            sender: TaskId(0),
            channel,
            kind: MessageKind::Request,
            payload: payload.to_vec(),
        }
    }

    // ---- Shape sanity --------------------------------------------------------

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

    // ---- KernelIpcRegistry: create / destroy --------------------------------

    #[test]
    fn create_channel_returns_monotonic_ids() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let a = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Block), None, None, &stub)
            .unwrap();
        let b = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Block), None, None, &stub)
            .unwrap();
        assert_eq!(a, ChannelId(1));
        assert_eq!(b, ChannelId(2));
        assert_eq!(r.channel_count(), 2);
    }

    #[test]
    fn create_rejects_zero_depth() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let err = r
            .create_channel(TaskId(1), open_policy(0, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap_err();
        assert_eq!(err, KernelError::InvalidArgument);
    }

    #[test]
    fn destroy_requires_owner() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let id = r
            .create_channel(TaskId(10), open_policy(4, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        // Non-owner cannot destroy.
        assert_eq!(
            r.destroy_channel(id, TaskId(99)).unwrap_err(),
            KernelError::CapabilityDenied
        );
        // Owner can.
        r.destroy_channel(id, TaskId(10)).unwrap();
        assert_eq!(r.channel_count(), 0);
    }

    // ---- KernelIpcRegistry: send / receive round-trip ------------------------

    #[test]
    fn send_then_receive_round_trip() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        let env = make_envelope(ch, b"ping");
        let wake = r.send(env, TaskId(10), principal(0)).unwrap();
        assert_eq!(wake, WakeAction::None);

        let (got, wake) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert_eq!(wake, WakeAction::None);
        let env = got.expect("message delivered");
        assert_eq!(env.sender, TaskId(10));
        assert_eq!(env.channel, ch);
        assert_eq!(env.payload, b"ping");
    }

    #[test]
    fn kernel_overwrites_sender_field() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(2, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        // Userspace claims to be TaskId(999); kernel must overwrite to actual.
        let mut env = make_envelope(ch, b"x");
        env.sender = TaskId(999);
        r.send(env, TaskId(42), principal(0)).unwrap();
        let (got, _) = r.receive(ch, TaskId(1), principal(0), false).unwrap();
        assert_eq!(got.unwrap().sender, TaskId(42));
    }

    // ---- Backpressure --------------------------------------------------------

    #[test]
    fn drop_policy_returns_resource_exhausted_when_full() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(1, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        r.send(make_envelope(ch, b"first"), TaskId(10), principal(0))
            .unwrap();
        let err = r
            .send(make_envelope(ch, b"second"), TaskId(10), principal(0))
            .unwrap_err();
        assert_eq!(err, KernelError::ResourceExhausted);
        // The original message is still there.
        let (got, _) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert_eq!(got.unwrap().payload, b"first");
    }

    #[test]
    fn evict_oldest_replaces_head_when_full() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(
                TaskId(1),
                open_policy(2, BackpressurePolicy::EvictOldest),
                None,
                None,
                &stub,
            )
            .unwrap();
        r.send(make_envelope(ch, b"a"), TaskId(10), principal(0)).unwrap();
        r.send(make_envelope(ch, b"b"), TaskId(10), principal(0)).unwrap();
        // Queue now full → "a" evicted, queue becomes [b, c].
        r.send(make_envelope(ch, b"c"), TaskId(10), principal(0)).unwrap();
        let (got, _) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert_eq!(got.unwrap().payload, b"b");
        let (got, _) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert_eq!(got.unwrap().payload, b"c");
    }

    #[test]
    fn block_policy_signals_block_action_when_full() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(1, BackpressurePolicy::Block), None, None, &stub)
            .unwrap();
        r.send(make_envelope(ch, b"a"), TaskId(10), principal(0)).unwrap();
        let wake = r.send(make_envelope(ch, b"b"), TaskId(20), principal(0)).unwrap();
        assert_eq!(wake, WakeAction::Block(TaskId(20)));
        // Sender 20 must be parked in waiters_send.
        let ch_ref = r.channel(ch).unwrap();
        assert_eq!(ch_ref.waiters_send.front().copied(), Some(TaskId(20)));
    }

    // ---- Wakeup contracts ----------------------------------------------------

    #[test]
    fn receive_on_empty_blocks_when_requested() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        let (got, wake) = r.receive(ch, TaskId(11), principal(0), true).unwrap();
        assert!(got.is_none());
        assert_eq!(wake, WakeAction::Block(TaskId(11)));
        let ch_ref = r.channel(ch).unwrap();
        assert_eq!(ch_ref.waiters_recv.front().copied(), Some(TaskId(11)));
    }

    #[test]
    fn receive_on_empty_nonblocking_returns_none() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        let (got, wake) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert!(got.is_none());
        assert_eq!(wake, WakeAction::None);
    }

    #[test]
    fn send_wakes_pending_receiver() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(4, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        // Receiver parks first.
        let _ = r.receive(ch, TaskId(11), principal(0), true).unwrap();
        // Now sender arrives.
        let wake = r.send(make_envelope(ch, b"x"), TaskId(10), principal(0)).unwrap();
        assert_eq!(wake, WakeAction::Wake(TaskId(11)));
    }

    #[test]
    fn receive_wakes_pending_blocking_sender() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(1, BackpressurePolicy::Block), None, None, &stub)
            .unwrap();
        r.send(make_envelope(ch, b"first"), TaskId(10), principal(0))
            .unwrap();
        let _ = r.send(make_envelope(ch, b"second"), TaskId(20), principal(0)).unwrap();
        // The sender 20 is parked; pull "first" → wake 20.
        let (got, wake) = r.receive(ch, TaskId(11), principal(0), false).unwrap();
        assert!(got.is_some());
        assert_eq!(wake, WakeAction::Wake(TaskId(20)));
    }

    // ---- Capability gating ---------------------------------------------------

    #[test]
    fn send_subject_mismatch_denies() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let send_tok = KernelCapabilityToken {
            subject: principal(42),
            action: KernelAction::IpcSend,
            resource: KernelResource::IpcChannel(1),
        };
        let ch = r
            .create_channel(
                TaskId(1),
                open_policy(4, BackpressurePolicy::Drop),
                Some(send_tok),
                None,
                &stub,
            )
            .unwrap();
        // Sender with wrong principal is rejected.
        let err = r
            .send(make_envelope(ch, b"x"), TaskId(99), principal(7))
            .unwrap_err();
        assert_eq!(err, KernelError::CapabilityDenied);
        // Correct principal succeeds.
        r.send(make_envelope(ch, b"y"), TaskId(99), principal(42)).unwrap();
    }

    #[test]
    fn recv_subject_mismatch_denies() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let recv_tok = KernelCapabilityToken {
            subject: principal(7),
            action: KernelAction::IpcRecv,
            resource: KernelResource::IpcChannel(1),
        };
        let ch = r
            .create_channel(
                TaskId(1),
                open_policy(4, BackpressurePolicy::Drop),
                None,
                Some(recv_tok),
                &stub,
            )
            .unwrap();
        r.send(make_envelope(ch, b"x"), TaskId(99), principal(0)).unwrap();
        let err = r
            .receive(ch, TaskId(11), principal(99), false)
            .unwrap_err();
        assert_eq!(err, KernelError::CapabilityDenied);
        // Correct principal succeeds.
        let (got, _) = r.receive(ch, TaskId(11), principal(7), false).unwrap();
        assert!(got.is_some());
    }

    #[test]
    fn create_with_invalid_token_action_denies() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        // Caller presents an IpcRecv token in the send slot — stub rejects.
        let wrong_action = KernelCapabilityToken {
            subject: principal(1),
            action: KernelAction::IpcRecv,
            resource: KernelResource::IpcChannel(1),
        };
        let err = r
            .create_channel(
                TaskId(1),
                open_policy(4, BackpressurePolicy::Drop),
                Some(wrong_action),
                None,
                &stub,
            )
            .unwrap_err();
        assert_eq!(err, KernelError::CapabilityDenied);
    }

    // ---- queue_depth + missing channel --------------------------------------

    #[test]
    fn queue_depth_reflects_in_flight_messages() {
        let mut r = KernelIpcRegistry::new();
        let stub = StubCapabilityProvider;
        let ch = r
            .create_channel(TaskId(1), open_policy(8, BackpressurePolicy::Drop), None, None, &stub)
            .unwrap();
        assert_eq!(r.queue_depth(ch).unwrap(), 0);
        r.send(make_envelope(ch, b"a"), TaskId(10), principal(0)).unwrap();
        r.send(make_envelope(ch, b"b"), TaskId(10), principal(0)).unwrap();
        assert_eq!(r.queue_depth(ch).unwrap(), 2);
    }

    #[test]
    fn operations_on_missing_channel_return_invalid_argument() {
        let mut r = KernelIpcRegistry::new();
        let missing = ChannelId(9999);
        assert_eq!(
            r.queue_depth(missing).unwrap_err(),
            KernelError::InvalidArgument
        );
        assert_eq!(
            r.destroy_channel(missing, TaskId(1)).unwrap_err(),
            KernelError::InvalidArgument
        );
        assert_eq!(
            r.send(make_envelope(missing, b"x"), TaskId(1), principal(0))
                .unwrap_err(),
            KernelError::InvalidArgument
        );
        assert_eq!(
            r.receive(missing, TaskId(1), principal(0), false)
                .unwrap_err(),
            KernelError::InvalidArgument
        );
    }
}
