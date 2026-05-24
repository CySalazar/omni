//! # `omni-fs`
//!
//! User-space filesystem service skeleton for OMNI OS — **`OmniFS` v0**.
//!
//! Phase-1 scope: the service registers itself against one or more BLK
//! channels (published by storage drivers via the `omni.svc.blk.<diskN>`
//! channel name per OIP-Driver-NVMe-014 § S4), tracks volumes in a
//! [`VolumeRegistry`], manages BLK channel consumer state via
//! [`BlkChannelConsumer`], and stubs every incoming [`FsRequest`] with
//! [`FsResponse::NotImplemented`].
//!
//! The real `OmniFS` host implementation lands in Phase 2 per
//! [`OIP-FS-018`](../../oips/oip-fs-018.md).
//!
//! ## Architecture
//!
//! ```text
//!   FsService
//!     ├── VolumeRegistry          (slot name → channel_id map)
//!     └── dispatch: FsRequest → FsResponse (all NotImplemented in Phase 1)
//!
//!   BlkChannelConsumer            (per-volume BLK channel client)
//!     ├── channel_id: u64
//!     ├── next_request_id: u64    (monotonically increasing opaque ID)
//!     └── pending: BTreeMap<u64, BlkRequest>  (in-flight correlation)
//! ```
//!
//! The `BlkChannelConsumer` is deliberately decoupled from `FsService` so
//! that unit tests can construct and drive consumers independently of the
//! full service state machine.
//!
//! ## BLK channel constants consumed from `omni-types`
//!
//! - [`omni_types::blk::CHANNEL_NAME_PREFIX`] — `"omni.svc.blk."` prefix
//!   used when constructing channel names from disk-slot strings.
//! - [`omni_types::blk::BLOCK_SIZE_BYTES`] — 4 096 B block size asserted in
//!   alignment checks (Phase 2).
//! - [`omni_types::blk::MAX_BLOCK_COUNT_PER_REQUEST`] — upper bound on the
//!   block count per BLK request (Phase 2 range validation).
//!
//! ## Status
//!
//! `OmniFS` v0 — `TASK-011` deliverable per
//! `docs/planning/2026-05-21-development-plan.md` (Wave 4, Stream 1).

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc,
        clippy::tests_outside_test_module
    )
)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use omni_types::blk::{
    BLOCK_SIZE_BYTES, BlkRequest, BlkResponse, CHANNEL_NAME_PREFIX, MAX_BLOCK_COUNT_PER_REQUEST,
};
use omni_types::wire::{decode_canonical, encode_canonical};

// =============================================================================
// Error taxonomy
// =============================================================================

/// All error conditions the filesystem service can surface.
///
/// Variants are `#[non_exhaustive]` so callers are forced to provide a `_`
/// arm; new error categories can be added without breaking downstream
/// pattern-match sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsError {
    /// No volume with the given disk-slot name exists in the registry.
    VolumeNotFound,
    /// A volume with the given disk-slot name is already registered.
    VolumeAlreadyRegistered,
    /// The supplied channel ID is invalid (zero is the sentinel "no channel").
    InvalidChannelId,
    /// The disk-slot name string is empty, which is never a valid slot name.
    InvalidSlotName,
    /// The underlying BLK channel has been closed by the driver side.
    ChannelDisconnected,
    /// No in-flight request with the given correlation ID exists.
    CorrelationIdNotFound,
    /// The request did not receive a response within the allowed window.
    ///
    /// Phase-1 never triggers this variant because no real I/O is issued;
    /// it is defined here so Phase-2 can propagate timeouts without an
    /// error-taxonomy change.
    RequestTimeout,
    /// Wire encoding or decoding of a BLK message failed.
    WireError,
}

// =============================================================================
// FileMetadata
// =============================================================================

/// Metadata record returned by a successful [`FsRequest::Stat`] operation.
///
/// All timestamp fields carry seconds since the OMNI OS epoch (monotonic
/// clock provided by the kernel HAL; not Unix epoch). Phase-1 always
/// returns [`FsResponse::NotImplemented`] so these fields are never
/// populated in practice until Phase 2.
///
/// The struct derives `Serialize` / `Deserialize` because metadata records
/// cross the trust boundary between the filesystem service and its callers
/// via the canonical wire encoding ([`omni_types::wire::encode_canonical`] /
/// [`omni_types::wire::decode_canonical`]).
///
/// # Example
///
/// ```rust
/// use omni_fs::FileMetadata;
///
/// let meta = FileMetadata {
///     size: 4096,
///     block_count: 1,
///     created: 0,
///     modified: 0,
/// };
/// assert_eq!(meta.size, 4096);
/// assert_eq!(meta.block_count, 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File size in bytes.
    pub size: u64,
    /// Number of 4 KiB blocks occupied by the file on-disk.
    ///
    /// A freshly created zero-length file has `block_count == 0`. After
    /// the first byte is written, `block_count` becomes 1. The value
    /// satisfies `block_count == (size + BLOCK_SIZE_BYTES as u64 - 1)
    /// / BLOCK_SIZE_BYTES as u64` once Phase 2 allocates blocks.
    pub block_count: u64,
    /// Creation timestamp in seconds since the OMNI OS HAL epoch.
    pub created: u64,
    /// Last-modified timestamp in seconds since the OMNI OS HAL epoch.
    pub modified: u64,
}

// =============================================================================
// FsResponse
// =============================================================================

/// Response codes (and payloads) returned by [`FsService::handle_request`].
///
/// The enum is `#[non_exhaustive]` so new response variants can be added in
/// future phases without breaking existing `match` sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsResponse {
    /// The request completed successfully with no additional payload.
    Ok,
    /// The filesystem service has not yet implemented the requested
    /// operation. Every Phase-1 request returns this variant.
    NotImplemented,
    /// The underlying BLK channel returned an error.
    BlkError,
    /// The requested path does not exist on the filesystem.
    NotFound,
    /// A generic I/O error that does not map to a more specific variant.
    IoError,
    /// Successful response to a [`FsRequest::Stat`] operation, carrying the
    /// file's [`FileMetadata`].
    ///
    /// Phase-1 never emits this variant (all requests return
    /// [`FsResponse::NotImplemented`]); the variant is declared here so
    /// Phase-2 can return populated metadata without an API break.
    Stat(FileMetadata),
}

// =============================================================================
// FsRequest
// =============================================================================

/// Request variants the filesystem service accepts from callers.
///
/// The enum is `#[non_exhaustive]` to allow new operation types in future
/// phases without breaking existing `match` expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsRequest {
    /// Read `count` bytes starting at `offset` from the file at `path`.
    Read {
        /// File path (UTF-8, forward-slash separated, must begin with `/`).
        path: String,
        /// Byte offset within the file.
        offset: u64,
        /// Number of bytes to read.
        count: u32,
    },
    /// Write `data_len` bytes at `offset` to the file at `path`.
    ///
    /// The data payload itself is delivered via the BLK channel's DMA buffer
    /// (IOVA-mapped, per OIP-Driver-NVMe-014 § M4) rather than inline in
    /// this request struct. Phase-2 resolves the IOVA address from the
    /// caller's capability context.
    Write {
        /// File path.
        path: String,
        /// Byte offset within the file.
        offset: u64,
        /// Number of bytes to write.
        data_len: u32,
    },
    /// Flush pending writes for the file at `path`.
    ///
    /// Maps to [`BlkRequest::Flush`] at the BLK layer (Phase 2).
    Flush {
        /// File path.
        path: String,
    },
    /// Query metadata (size, timestamps, block count) for `path`.
    ///
    /// A successful Phase-2 response carries [`FsResponse::Stat`] with a
    /// populated [`FileMetadata`]. Phase-1 returns [`FsResponse::NotImplemented`].
    Stat {
        /// File path.
        path: String,
    },
}

// =============================================================================
// FsRegistrationError
// =============================================================================

/// Error returned by [`FsService::register`] (single-channel legacy API).
///
/// For the multi-volume API use [`FsError`] via [`FsService::register_volume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsRegistrationError {
    /// The service is already registered to a BLK channel via the legacy
    /// single-channel API.
    AlreadyRegistered,
    /// The supplied channel ID is invalid (zero is the sentinel "no channel").
    InvalidChannelId,
}

// =============================================================================
// VolumeRegistry
// =============================================================================

/// Registry that maps disk-slot names to BLK channel IDs.
///
/// A "disk slot" is the suffix portion of the BLK channel name after the
/// [`CHANNEL_NAME_PREFIX`] (`"omni.svc.blk."`), e.g., `"nvme0"`, `"sata1"`,
/// `"virtio2"`. The registry tracks which slot names have been registered so
/// the filesystem service can look up the channel ID for any slot.
///
/// # Example
///
/// ```rust
/// use omni_fs::VolumeRegistry;
///
/// let mut reg = VolumeRegistry::new();
/// reg.register("nvme0", 1).expect("first registration succeeds");
/// assert_eq!(reg.lookup("nvme0"), Some(1));
/// assert_eq!(reg.volume_count(), 1);
/// reg.unregister("nvme0").expect("unregistration succeeds");
/// assert_eq!(reg.volume_count(), 0);
/// ```
#[derive(Debug)]
pub struct VolumeRegistry {
    /// Map from slot name to BLK channel ID.
    ///
    /// `BTreeMap` is chosen over `HashMap` because (a) `BTreeMap` lives in
    /// `alloc` and is therefore available in `no_std + alloc` environments
    /// without any additional dependencies, and (b) deterministic iteration
    /// order simplifies debugging and snapshot tests.
    volumes: BTreeMap<String, u64>,
}

impl VolumeRegistry {
    /// Create an empty volume registry.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::VolumeRegistry;
    ///
    /// let reg = VolumeRegistry::new();
    /// assert_eq!(reg.volume_count(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            volumes: BTreeMap::new(),
        }
    }

    /// Register a disk slot, binding it to `channel_id`.
    ///
    /// # Errors
    ///
    /// - [`FsError::InvalidSlotName`] if `slot` is empty.
    /// - [`FsError::InvalidChannelId`] if `channel_id` is zero.
    /// - [`FsError::VolumeAlreadyRegistered`] if `slot` is already present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::{VolumeRegistry, FsError};
    ///
    /// let mut reg = VolumeRegistry::new();
    /// assert!(reg.register("nvme0", 1).is_ok());
    /// assert_eq!(reg.register("nvme0", 2), Err(FsError::VolumeAlreadyRegistered));
    /// assert_eq!(reg.register("", 3), Err(FsError::InvalidSlotName));
    /// assert_eq!(reg.register("nvme1", 0), Err(FsError::InvalidChannelId));
    /// ```
    pub fn register(&mut self, slot: &str, channel_id: u64) -> Result<(), FsError> {
        if slot.is_empty() {
            return Err(FsError::InvalidSlotName);
        }
        if channel_id == 0 {
            return Err(FsError::InvalidChannelId);
        }
        if self.volumes.contains_key(slot) {
            return Err(FsError::VolumeAlreadyRegistered);
        }
        self.volumes.insert(String::from(slot), channel_id);
        Ok(())
    }

    /// Unregister the disk slot, removing it from the registry.
    ///
    /// # Errors
    ///
    /// - [`FsError::VolumeNotFound`] if `slot` is not currently registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::{VolumeRegistry, FsError};
    ///
    /// let mut reg = VolumeRegistry::new();
    /// reg.register("nvme0", 1).unwrap();
    /// assert!(reg.unregister("nvme0").is_ok());
    /// assert_eq!(reg.unregister("nvme0"), Err(FsError::VolumeNotFound));
    /// ```
    pub fn unregister(&mut self, slot: &str) -> Result<(), FsError> {
        if self.volumes.remove(slot).is_none() {
            return Err(FsError::VolumeNotFound);
        }
        Ok(())
    }

    /// Look up the BLK channel ID for a registered disk slot.
    ///
    /// Returns `None` if the slot is not registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::VolumeRegistry;
    ///
    /// let mut reg = VolumeRegistry::new();
    /// reg.register("nvme0", 42).unwrap();
    /// assert_eq!(reg.lookup("nvme0"), Some(42));
    /// assert_eq!(reg.lookup("sata1"), None);
    /// ```
    #[must_use]
    pub fn lookup(&self, slot: &str) -> Option<u64> {
        self.volumes.get(slot).copied()
    }

    /// Return the number of currently registered volumes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::VolumeRegistry;
    ///
    /// let mut reg = VolumeRegistry::new();
    /// assert_eq!(reg.volume_count(), 0);
    /// reg.register("nvme0", 1).unwrap();
    /// assert_eq!(reg.volume_count(), 1);
    /// ```
    #[must_use]
    pub fn volume_count(&self) -> usize {
        self.volumes.len()
    }

    /// Build the full BLK channel name for a given disk slot.
    ///
    /// The channel name is [`CHANNEL_NAME_PREFIX`] concatenated with `slot`,
    /// e.g., `"omni.svc.blk.nvme0"`. This helper is a pure string operation;
    /// it does not validate whether the slot is registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::VolumeRegistry;
    ///
    /// assert_eq!(
    ///     VolumeRegistry::channel_name_for("nvme0"),
    ///     "omni.svc.blk.nvme0"
    /// );
    /// ```
    #[must_use]
    pub fn channel_name_for(slot: &str) -> String {
        let mut name = String::from(CHANNEL_NAME_PREFIX);
        name.push_str(slot);
        name
    }
}

impl Default for VolumeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// BlkChannelConsumer
// =============================================================================

/// Client-side handle for a single `omni.svc.blk.<diskN>` IPC channel.
///
/// The consumer is responsible for:
///
/// 1. Submitting [`BlkRequest`] values to the driver (Phase 2 actually sends
///    them over IPC; Phase 1 only stubs the queue bookkeeping).
/// 2. Tracking in-flight requests by opaque correlation ID so responses can
///    be matched back to their originating request.
/// 3. Correlating incoming [`BlkResponse`] values to the pending request and
///    returning the response to the caller.
///
/// Correlation IDs are monotonically increasing `u64` values minted by
/// [`BlkChannelConsumer::submit`]. They are opaque to the driver — the driver
/// echoes whatever ID the consumer sent, and the consumer uses it to locate
/// the pending entry in the `pending` map.
///
/// # Example
///
/// ```rust
/// use omni_fs::BlkChannelConsumer;
/// use omni_types::blk::{BlkRequest, BlkResponse};
///
/// let mut consumer = BlkChannelConsumer::new(7);
/// assert_eq!(consumer.channel_id(), 7);
/// assert_eq!(consumer.pending_count(), 0);
///
/// let req = BlkRequest::Flush;
/// let id = consumer.submit(req).expect("submit succeeds");
/// assert_eq!(consumer.pending_count(), 1);
///
/// let resp = consumer
///     .correlate(id, BlkResponse::Ok)
///     .expect("correlate succeeds");
/// assert_eq!(resp, BlkResponse::Ok);
/// assert_eq!(consumer.pending_count(), 0);
/// ```
#[derive(Debug)]
pub struct BlkChannelConsumer {
    /// The IPC channel ID this consumer is bound to.
    channel_id: u64,
    /// Monotonically increasing counter used to mint unique correlation IDs.
    ///
    /// Starting at 1 keeps 0 available as a "no pending request" sentinel
    /// in external protocols that may need one.
    next_request_id: u64,
    /// Map from correlation ID to in-flight [`BlkRequest`].
    ///
    /// On submit the request is inserted; on correlate it is removed and the
    /// response is returned to the caller. The consumer never holds a
    /// response in this map — it is returned immediately.
    pending: BTreeMap<u64, BlkRequest>,
}

impl BlkChannelConsumer {
    /// Create a new consumer bound to the given BLK channel ID.
    ///
    /// The channel ID MUST NOT be zero; callers should validate before
    /// constructing (e.g., via [`VolumeRegistry::lookup`]). An ID of zero
    /// indicates "not connected" and is the sentinel used throughout the
    /// codebase.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    ///
    /// let c = BlkChannelConsumer::new(3);
    /// assert_eq!(c.channel_id(), 3);
    /// assert_eq!(c.pending_count(), 0);
    /// ```
    #[must_use]
    pub fn new(channel_id: u64) -> Self {
        Self {
            channel_id,
            // Start at 1 so that 0 remains a "not a real ID" sentinel.
            next_request_id: 1,
            pending: BTreeMap::new(),
        }
    }

    /// Return the BLK channel ID this consumer is bound to.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    ///
    /// assert_eq!(BlkChannelConsumer::new(99).channel_id(), 99);
    /// ```
    #[must_use]
    pub fn channel_id(&self) -> u64 {
        self.channel_id
    }

    /// Return the number of in-flight requests awaiting a response.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    /// use omni_types::blk::BlkRequest;
    ///
    /// let mut c = BlkChannelConsumer::new(1);
    /// assert_eq!(c.pending_count(), 0);
    /// c.submit(BlkRequest::Flush).unwrap();
    /// assert_eq!(c.pending_count(), 1);
    /// ```
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Enqueue a [`BlkRequest`] and return its opaque correlation ID.
    ///
    /// Phase-1: this call inserts the request into the pending map and
    /// returns the ID. No actual IPC send occurs until Phase 2 wires up
    /// the channel transport.
    ///
    /// # Errors
    ///
    /// - [`FsError::ChannelDisconnected`] if `channel_id` is zero, indicating
    ///   the consumer was constructed with an invalid handle (defensive; callers
    ///   should avoid constructing consumers with `channel_id == 0`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    /// use omni_types::blk::BlkRequest;
    ///
    /// let mut c = BlkChannelConsumer::new(5);
    /// let id = c.submit(BlkRequest::Flush).expect("submit succeeds");
    /// assert!(id > 0);
    /// ```
    pub fn submit(&mut self, request: BlkRequest) -> Result<u64, FsError> {
        // A zero channel_id means the consumer is in a disconnected state.
        if self.channel_id == 0 {
            return Err(FsError::ChannelDisconnected);
        }
        let id = self.next_request_id;
        // Wrapping add keeps the counter moving without panicking if it
        // somehow reaches u64::MAX in very long-running sessions. In practice
        // 2^64 requests per channel session is unreachable.
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.pending.insert(id, request);
        Ok(id)
    }

    /// Match an incoming [`BlkResponse`] to a previously submitted request.
    ///
    /// The pending entry for `request_id` is removed from the in-flight map
    /// and the `response` is returned to the caller. The caller is responsible
    /// for interpreting the response in context of the original request.
    ///
    /// # Errors
    ///
    /// - [`FsError::CorrelationIdNotFound`] if no in-flight request with the
    ///   given ID exists (duplicate response, stale ID, etc.).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    /// use omni_types::blk::{BlkRequest, BlkResponse};
    ///
    /// let mut c = BlkChannelConsumer::new(1);
    /// let id = c.submit(BlkRequest::Flush).unwrap();
    /// let resp = c.correlate(id, BlkResponse::Ok).expect("correlate succeeds");
    /// assert_eq!(resp, BlkResponse::Ok);
    /// ```
    pub fn correlate(
        &mut self,
        request_id: u64,
        response: BlkResponse,
    ) -> Result<BlkResponse, FsError> {
        if self.pending.remove(&request_id).is_none() {
            return Err(FsError::CorrelationIdNotFound);
        }
        Ok(response)
    }

    /// Wire-encode the given [`BlkRequest`] into a freshly allocated buffer
    /// using the canonical encoding ([`encode_canonical`]).
    ///
    /// This is a convenience helper for Phase-2 IPC send paths. Phase-1 code
    /// does not call IPC so this method is tested directly via round-trip
    /// assertions.
    ///
    /// # Errors
    ///
    /// - [`FsError::WireError`] if the encoder fails (allocation failure or
    ///   internal serializer error; both indicate a bug).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    /// use omni_types::blk::BlkRequest;
    ///
    /// let c = BlkChannelConsumer::new(1);
    /// let bytes = c
    ///     .encode_request(&BlkRequest::Flush)
    ///     .expect("encoding never fails for Flush");
    /// assert!(!bytes.is_empty());
    /// ```
    #[allow(
        clippy::unused_self,
        reason = "Phase-2 will use self.channel_id for per-channel encode state (e.g. request framing)"
    )]
    pub fn encode_request(&self, request: &BlkRequest) -> Result<Vec<u8>, FsError> {
        encode_canonical(request).map_err(|_| FsError::WireError)
    }

    /// Wire-decode a [`BlkResponse`] from `bytes` using the canonical
    /// encoding ([`decode_canonical`]).
    ///
    /// This is a convenience helper for Phase-2 IPC receive paths.
    ///
    /// # Errors
    ///
    /// - [`FsError::WireError`] if the decoder fails (truncated input,
    ///   trailing bytes, unknown discriminant, etc.).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::BlkChannelConsumer;
    /// use omni_types::blk::BlkResponse;
    /// use omni_types::wire::encode_canonical;
    ///
    /// let encoded = encode_canonical(&BlkResponse::Ok).unwrap();
    /// let c = BlkChannelConsumer::new(1);
    /// let resp = c.decode_response(&encoded).expect("round-trip succeeds");
    /// assert_eq!(resp, BlkResponse::Ok);
    /// ```
    #[allow(
        clippy::unused_self,
        reason = "Phase-2 will use self.channel_id for per-channel decode state (e.g. response validation)"
    )]
    pub fn decode_response(&self, bytes: &[u8]) -> Result<BlkResponse, FsError> {
        decode_canonical(bytes).map_err(|_| FsError::WireError)
    }
}

// =============================================================================
// FsService
// =============================================================================

/// Phase-1 filesystem service skeleton.
///
/// Owns a [`VolumeRegistry`] and dispatches [`FsRequest`] variants through
/// the BLK channel layer. All dispatches return [`FsResponse::NotImplemented`]
/// in Phase 1; Phase 2 replaces the stubs with the native `OmniFS` host.
///
/// The legacy single-channel API ([`FsService::register`] /
/// [`FsService::channel_id`]) is preserved for backward compatibility with
/// existing tests and callers. New code should use the multi-volume API
/// ([`FsService::register_volume`] / [`FsService::unregister_volume`] /
/// [`FsService::lookup_volume`]) which delegates to the internal
/// [`VolumeRegistry`].
///
/// # Example
///
/// ```rust
/// extern crate alloc;
/// use alloc::string::String;
/// use omni_fs::{FsService, FsRequest, FsResponse};
///
/// let mut svc = FsService::new();
/// svc.register_volume("nvme0", 1).expect("register succeeds");
/// assert_eq!(svc.lookup_volume("nvme0"), Some(1));
///
/// let req = FsRequest::Stat { path: String::from("/boot/kernel") };
/// assert_eq!(svc.handle_request(&req), FsResponse::NotImplemented);
/// ```
#[derive(Debug)]
pub struct FsService {
    /// Legacy single-channel BLK channel ID (preserved for backward compat).
    blk_channel_id: Option<u64>,
    /// Multi-volume registry (the authoritative map for Phase-2+ code).
    registry: VolumeRegistry,
}

impl FsService {
    /// Create a new, unregistered filesystem service.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::FsService;
    ///
    /// let svc = FsService::new();
    /// assert_eq!(svc.channel_id(), None);
    /// assert_eq!(svc.lookup_volume("nvme0"), None);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            blk_channel_id: None,
            registry: VolumeRegistry::new(),
        }
    }

    // -------------------------------------------------------------------------
    // Legacy single-channel API (backward compat)
    // -------------------------------------------------------------------------

    /// Register the service against a single BLK channel ID.
    ///
    /// This is the legacy single-channel API. For multi-volume registration
    /// use [`FsService::register_volume`].
    ///
    /// # Errors
    ///
    /// - [`FsRegistrationError::AlreadyRegistered`] if the service already
    ///   has a channel set via this API.
    /// - [`FsRegistrationError::InvalidChannelId`] if `channel_id` is zero.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::{FsService, FsRegistrationError};
    ///
    /// let mut svc = FsService::new();
    /// assert!(svc.register(1).is_ok());
    /// assert_eq!(svc.register(2), Err(FsRegistrationError::AlreadyRegistered));
    /// ```
    pub fn register(&mut self, channel_id: u64) -> Result<(), FsRegistrationError> {
        if self.blk_channel_id.is_some() {
            return Err(FsRegistrationError::AlreadyRegistered);
        }
        if channel_id == 0 {
            return Err(FsRegistrationError::InvalidChannelId);
        }
        self.blk_channel_id = Some(channel_id);
        Ok(())
    }

    /// Return the BLK channel ID set via the legacy [`FsService::register`]
    /// API, or `None` if it has not been called.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::FsService;
    ///
    /// let mut svc = FsService::new();
    /// assert_eq!(svc.channel_id(), None);
    /// svc.register(7).unwrap();
    /// assert_eq!(svc.channel_id(), Some(7));
    /// ```
    #[must_use]
    pub const fn channel_id(&self) -> Option<u64> {
        self.blk_channel_id
    }

    // -------------------------------------------------------------------------
    // Multi-volume API
    // -------------------------------------------------------------------------

    /// Register a disk slot in the volume registry.
    ///
    /// Delegates to [`VolumeRegistry::register`].
    ///
    /// # Errors
    ///
    /// See [`FsError`] variants returned by [`VolumeRegistry::register`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::{FsService, FsError};
    ///
    /// let mut svc = FsService::new();
    /// assert!(svc.register_volume("nvme0", 1).is_ok());
    /// assert_eq!(svc.register_volume("nvme0", 2), Err(FsError::VolumeAlreadyRegistered));
    /// ```
    pub fn register_volume(&mut self, slot: &str, channel_id: u64) -> Result<(), FsError> {
        self.registry.register(slot, channel_id)
    }

    /// Unregister a disk slot from the volume registry.
    ///
    /// Delegates to [`VolumeRegistry::unregister`].
    ///
    /// # Errors
    ///
    /// See [`FsError`] variants returned by [`VolumeRegistry::unregister`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::{FsService, FsError};
    ///
    /// let mut svc = FsService::new();
    /// svc.register_volume("nvme0", 1).unwrap();
    /// assert!(svc.unregister_volume("nvme0").is_ok());
    /// assert_eq!(svc.unregister_volume("nvme0"), Err(FsError::VolumeNotFound));
    /// ```
    pub fn unregister_volume(&mut self, slot: &str) -> Result<(), FsError> {
        self.registry.unregister(slot)
    }

    /// Look up the BLK channel ID for a registered disk slot.
    ///
    /// Delegates to [`VolumeRegistry::lookup`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::FsService;
    ///
    /// let mut svc = FsService::new();
    /// svc.register_volume("sata0", 9).unwrap();
    /// assert_eq!(svc.lookup_volume("sata0"), Some(9));
    /// assert_eq!(svc.lookup_volume("nvme0"), None);
    /// ```
    #[must_use]
    pub fn lookup_volume(&self, slot: &str) -> Option<u64> {
        self.registry.lookup(slot)
    }

    // -------------------------------------------------------------------------
    // Request dispatch
    // -------------------------------------------------------------------------

    /// Handle an incoming filesystem request.
    ///
    /// Phase-1 implementation: the method documents the intended BLK mapping
    /// for each variant (Phase-2 reference) and returns
    /// [`FsResponse::NotImplemented`] for every request.
    ///
    /// Phase-2 will replace the stub arms with real BLK dispatch via
    /// [`BlkChannelConsumer`].
    ///
    /// # BLK mapping (Phase-2 reference)
    ///
    /// | `FsRequest` variant | `BlkRequest` mapping |
    /// |---------------------|----------------------|
    /// | `Read`              | `BlkRequest::Read { lba, count, buf_iova }` |
    /// | `Write`             | `BlkRequest::Write { lba, count, buf_iova }` |
    /// | `Flush`             | `BlkRequest::Flush` |
    /// | `Stat`              | Metadata lookup (no direct BLK mapping) |
    ///
    /// # Example
    ///
    /// ```rust
    /// extern crate alloc;
    /// use alloc::string::String;
    /// use omni_fs::{FsService, FsRequest, FsResponse};
    ///
    /// let svc = FsService::new();
    /// let req = FsRequest::Stat { path: String::from("/etc/config") };
    /// assert_eq!(svc.handle_request(&req), FsResponse::NotImplemented);
    /// ```
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "Phase-2 OmniFS implementation will use self for volume state and BLK dispatch"
    )]
    pub fn handle_request(&self, request: &FsRequest) -> FsResponse {
        // Phase-1: document the intended BLK mapping via comments, but return
        // NotImplemented for every variant. The `_request` binding prevents
        // the unused-variable lint without silently swallowing the value.
        //
        // Phase-2 dispatch plan per variant:
        //   FsRequest::Read  → BlkRequest::Read  via BlkChannelConsumer::submit
        //   FsRequest::Write → BlkRequest::Write via BlkChannelConsumer::submit
        //   FsRequest::Flush → BlkRequest::Flush via BlkChannelConsumer::submit
        //   FsRequest::Stat  → metadata read, returns FsResponse::Stat(meta)
        let _ = request;
        FsResponse::NotImplemented
    }
}

impl Default for FsService {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Compile-time BLK constant assertions
// =============================================================================

/// Compile-time checks that the BLK constants imported from `omni-types`
/// satisfy the invariants this crate was written against.
///
/// If a future OIP changes `BLOCK_SIZE_BYTES` or
/// `MAX_BLOCK_COUNT_PER_REQUEST`, this crate's build fails immediately,
/// forcing a deliberate review of the impact on `OmniFS` before the change
/// can land.
#[allow(dead_code)]
const _BLK_CONST_GUARD: () = {
    assert!(BLOCK_SIZE_BYTES == 4096, "OmniFS requires 4 KiB BLK blocks");
    assert!(
        MAX_BLOCK_COUNT_PER_REQUEST == 2048,
        "OmniFS requires MAX_BLOCK_COUNT_PER_REQUEST == 2048"
    );
};

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use omni_types::wire::encode_canonical;

    // -------------------------------------------------------------------------
    // FsService — legacy single-channel API
    // -------------------------------------------------------------------------

    #[test]
    fn new_service_has_no_channel() {
        let svc = FsService::new();
        assert_eq!(svc.channel_id(), None);
    }

    #[test]
    fn register_sets_channel_id() {
        let mut svc = FsService::new();
        svc.register(42).expect("first registration succeeds");
        assert_eq!(svc.channel_id(), Some(42));
    }

    #[test]
    fn double_registration_returns_already_registered() {
        let mut svc = FsService::new();
        svc.register(1).unwrap();
        assert_eq!(svc.register(2), Err(FsRegistrationError::AlreadyRegistered));
    }

    #[test]
    fn register_rejects_zero_channel_id() {
        let mut svc = FsService::new();
        assert_eq!(svc.register(0), Err(FsRegistrationError::InvalidChannelId));
    }

    #[test]
    fn handle_request_returns_not_implemented_for_all_variants() {
        let svc = FsService::new();

        let variants = [
            FsRequest::Stat {
                path: String::from("/test"),
            },
            FsRequest::Read {
                path: String::from("/data"),
                offset: 0,
                count: 512,
            },
            FsRequest::Write {
                path: String::from("/out"),
                offset: 4096,
                data_len: 4096,
            },
            FsRequest::Flush {
                path: String::from("/log"),
            },
        ];

        for req in &variants {
            assert_eq!(
                svc.handle_request(req),
                FsResponse::NotImplemented,
                "expected NotImplemented for {req:?}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // VolumeRegistry
    // -------------------------------------------------------------------------

    #[test]
    fn registry_empty_on_construction() {
        let reg = VolumeRegistry::new();
        assert_eq!(reg.volume_count(), 0);
        assert_eq!(reg.lookup("nvme0"), None);
    }

    #[test]
    fn registry_register_and_lookup() {
        let mut reg = VolumeRegistry::new();
        reg.register("nvme0", 1).expect("register nvme0");
        assert_eq!(reg.lookup("nvme0"), Some(1));
        assert_eq!(reg.volume_count(), 1);
    }

    #[test]
    fn registry_register_multiple_volumes() {
        let mut reg = VolumeRegistry::new();
        reg.register("nvme0", 10).unwrap();
        reg.register("sata1", 20).unwrap();
        reg.register("virtio2", 30).unwrap();
        assert_eq!(reg.volume_count(), 3);
        assert_eq!(reg.lookup("nvme0"), Some(10));
        assert_eq!(reg.lookup("sata1"), Some(20));
        assert_eq!(reg.lookup("virtio2"), Some(30));
    }

    #[test]
    fn registry_rejects_duplicate_slot() {
        let mut reg = VolumeRegistry::new();
        reg.register("nvme0", 1).unwrap();
        assert_eq!(
            reg.register("nvme0", 2),
            Err(FsError::VolumeAlreadyRegistered)
        );
    }

    #[test]
    fn registry_rejects_zero_channel_id() {
        let mut reg = VolumeRegistry::new();
        assert_eq!(reg.register("nvme0", 0), Err(FsError::InvalidChannelId));
    }

    #[test]
    fn registry_rejects_empty_slot_name() {
        let mut reg = VolumeRegistry::new();
        assert_eq!(reg.register("", 1), Err(FsError::InvalidSlotName));
    }

    #[test]
    fn registry_unregister_removes_entry() {
        let mut reg = VolumeRegistry::new();
        reg.register("nvme0", 1).unwrap();
        reg.unregister("nvme0").unwrap();
        assert_eq!(reg.volume_count(), 0);
        assert_eq!(reg.lookup("nvme0"), None);
    }

    #[test]
    fn registry_unregister_nonexistent_returns_not_found() {
        let mut reg = VolumeRegistry::new();
        assert_eq!(reg.unregister("nvme0"), Err(FsError::VolumeNotFound));
    }

    #[test]
    fn registry_channel_name_for_builds_correct_prefix() {
        assert_eq!(
            VolumeRegistry::channel_name_for("nvme0"),
            "omni.svc.blk.nvme0"
        );
        assert_eq!(
            VolumeRegistry::channel_name_for("virtio2"),
            "omni.svc.blk.virtio2"
        );
    }

    // -------------------------------------------------------------------------
    // FsService — multi-volume API
    // -------------------------------------------------------------------------

    #[test]
    fn service_register_volume_and_lookup() {
        let mut svc = FsService::new();
        svc.register_volume("nvme0", 5).expect("register");
        assert_eq!(svc.lookup_volume("nvme0"), Some(5));
    }

    #[test]
    fn service_unregister_volume() {
        let mut svc = FsService::new();
        svc.register_volume("sata0", 7).unwrap();
        svc.unregister_volume("sata0").unwrap();
        assert_eq!(svc.lookup_volume("sata0"), None);
    }

    #[test]
    fn service_register_volume_rejects_zero_id() {
        let mut svc = FsService::new();
        assert_eq!(
            svc.register_volume("nvme0", 0),
            Err(FsError::InvalidChannelId)
        );
    }

    // -------------------------------------------------------------------------
    // BlkChannelConsumer
    // -------------------------------------------------------------------------

    #[test]
    fn consumer_new_has_no_pending() {
        let c = BlkChannelConsumer::new(1);
        assert_eq!(c.channel_id(), 1);
        assert_eq!(c.pending_count(), 0);
    }

    #[test]
    fn consumer_submit_increments_pending() {
        let mut c = BlkChannelConsumer::new(1);
        c.submit(BlkRequest::Flush).unwrap();
        assert_eq!(c.pending_count(), 1);
        c.submit(BlkRequest::Flush).unwrap();
        assert_eq!(c.pending_count(), 2);
    }

    #[test]
    fn consumer_submit_returns_unique_ids() {
        let mut c = BlkChannelConsumer::new(1);
        let id1 = c.submit(BlkRequest::Flush).unwrap();
        let id2 = c.submit(BlkRequest::Flush).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn consumer_submit_returns_err_when_channel_id_is_zero() {
        // channel_id == 0 is the "disconnected" sentinel; submit must reject.
        let mut c = BlkChannelConsumer::new(0);
        assert_eq!(
            c.submit(BlkRequest::Flush),
            Err(FsError::ChannelDisconnected)
        );
    }

    #[test]
    fn consumer_correlate_removes_pending() {
        let mut c = BlkChannelConsumer::new(2);
        let id = c.submit(BlkRequest::Flush).unwrap();
        assert_eq!(c.pending_count(), 1);
        let resp = c.correlate(id, BlkResponse::Ok).unwrap();
        assert_eq!(resp, BlkResponse::Ok);
        assert_eq!(c.pending_count(), 0);
    }

    #[test]
    fn consumer_correlate_unknown_id_returns_err() {
        let mut c = BlkChannelConsumer::new(1);
        assert_eq!(
            c.correlate(999, BlkResponse::Ok),
            Err(FsError::CorrelationIdNotFound)
        );
    }

    #[test]
    fn consumer_correlate_stale_id_after_completion_returns_err() {
        let mut c = BlkChannelConsumer::new(1);
        let id = c.submit(BlkRequest::Flush).unwrap();
        c.correlate(id, BlkResponse::Ok).unwrap();
        // Second correlate for the same ID must fail.
        assert_eq!(
            c.correlate(id, BlkResponse::Ok),
            Err(FsError::CorrelationIdNotFound)
        );
    }

    #[test]
    fn consumer_encode_decode_request_round_trip() {
        let c = BlkChannelConsumer::new(1);
        let req = BlkRequest::Read {
            lba: 0xDEAD_BEEF,
            count: 4,
            buf_iova: 0x1000,
        };
        let bytes = c.encode_request(&req).expect("encode");
        let bytes2 = c.encode_request(&req).expect("encode again");
        // Encoding must be deterministic.
        assert_eq!(bytes, bytes2);
        // Verify response round-trip via the decode helper.
        let resp_bytes = encode_canonical(&BlkResponse::Ok).unwrap();
        let decoded = c.decode_response(&resp_bytes).expect("decode response");
        assert_eq!(decoded, BlkResponse::Ok);
    }

    #[test]
    fn consumer_decode_response_rejects_empty_input() {
        let c = BlkChannelConsumer::new(1);
        assert_eq!(c.decode_response(&[]), Err(FsError::WireError));
    }

    // -------------------------------------------------------------------------
    // FileMetadata wire round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn file_metadata_round_trip() {
        let meta = FileMetadata {
            size: 8192,
            block_count: 2,
            created: 1_716_000_000,
            modified: 1_716_001_000,
        };
        let bytes = encode_canonical(&meta).expect("encode");
        let decoded: FileMetadata = omni_types::wire::decode_canonical(&bytes).expect("decode");
        assert_eq!(decoded, meta);
    }

    #[test]
    fn file_metadata_encoding_is_deterministic() {
        let meta = FileMetadata {
            size: 0,
            block_count: 0,
            created: 0,
            modified: 0,
        };
        let a = encode_canonical(&meta).expect("encode-a");
        let b = encode_canonical(&meta).expect("encode-b");
        assert_eq!(a, b);
    }

    // -------------------------------------------------------------------------
    // FsResponse equality (including Stat variant)
    // -------------------------------------------------------------------------

    #[test]
    fn fs_response_stat_carries_metadata() {
        let meta = FileMetadata {
            size: 4096,
            block_count: 1,
            created: 0,
            modified: 0,
        };
        let resp = FsResponse::Stat(meta);
        assert_eq!(resp, FsResponse::Stat(meta));
        assert_ne!(resp, FsResponse::Ok);
    }
}
