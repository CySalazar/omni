//! # `omni-fs`
//!
//! Filesystem service skeleton for OMNI OS.
//!
//! Phase-1 scope: the service registers itself against a BLK channel
//! (published by the NVMe driver via `BlkRegister (76)` / `BlkLookup (78)`)
//! and stubs every incoming request with [`FsResponse::NotImplemented`].
//! The real `OmniFS` host lands in Phase 2 per
//! [`OIP-FS-018`](../../oips/oip-fs-018.md).
//!
//! ## Architecture
//!
//! The service runs as a separate user-space process that communicates
//! with the kernel BLK registry via IPC channels. It is the single
//! consumer of the `omni.svc.blk.<diskN>` channel a storage driver
//! (NVMe, virtio-blk, etc.) publishes.
//!
//! ## Status
//!
//! Skeleton v0.2 — `TASK-011` deliverable per
//! `docs/planning/2026-05-21-development-plan.md`.

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

use alloc::string::String;

/// Filesystem response codes returned by
/// [`FsService::handle_request`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsResponse {
    /// The request completed successfully.
    Ok,
    /// The filesystem service has not yet implemented the requested
    /// operation. Phase-1 returns this for every request.
    NotImplemented,
    /// The underlying BLK channel returned an error.
    BlkError,
    /// The requested path does not exist.
    NotFound,
    /// A generic I/O error that does not map to a more specific
    /// variant.
    IoError,
}

/// Filesystem request types the service accepts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsRequest {
    /// Read `count` bytes starting at `offset` from the file at `path`.
    Read {
        /// File path (UTF-8, forward-slash separated).
        path: String,
        /// Byte offset within the file.
        offset: u64,
        /// Number of bytes to read.
        count: u32,
    },
    /// Write `data_len` bytes at `offset` to the file at `path`.
    Write {
        /// File path.
        path: String,
        /// Byte offset within the file.
        offset: u64,
        /// Number of bytes to write (the data itself arrives via the
        /// BLK channel's DMA buffer, not inline in this request).
        data_len: u32,
    },
    /// Flush pending writes for the file at `path`.
    Flush {
        /// File path.
        path: String,
    },
    /// Query metadata (size, permissions, timestamps) for `path`.
    Stat {
        /// File path.
        path: String,
    },
}

/// Registration error returned by [`FsService::register`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FsRegistrationError {
    /// The service is already registered to a BLK channel.
    AlreadyRegistered,
    /// The supplied channel ID is invalid (zero or sentinel).
    InvalidChannelId,
}

/// Phase-1 filesystem service skeleton.
///
/// Registers against a single BLK channel and stubs every request
/// with [`FsResponse::NotImplemented`]. Phase-2 replaces the stub
/// implementation with the native `OmniFS` host.
#[derive(Debug)]
pub struct FsService {
    blk_channel_id: Option<u64>,
}

impl FsService {
    /// Create a new unregistered filesystem service.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            blk_channel_id: None,
        }
    }

    /// Register the service against the supplied BLK channel ID.
    ///
    /// # Errors
    ///
    /// - [`FsRegistrationError::AlreadyRegistered`] if the service
    ///   already has a channel.
    /// - [`FsRegistrationError::InvalidChannelId`] if `channel_id` is
    ///   zero.
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

    /// Returns the BLK channel ID the service is registered to, or
    /// `None` if [`register`](Self::register) has not been called.
    #[must_use]
    pub const fn channel_id(&self) -> Option<u64> {
        self.blk_channel_id
    }

    /// Handle an incoming filesystem request.
    ///
    /// Phase-1 returns [`FsResponse::NotImplemented`] for every
    /// request. Phase-2 dispatches to the `OmniFS` implementation.
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "Phase-2 OmniFS implementation will use self for volume state"
    )]
    pub fn handle_request(&self, _request: &FsRequest) -> FsResponse {
        FsResponse::NotImplemented
    }
}

impl Default for FsService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn handle_request_returns_not_implemented() {
        let svc = FsService::new();
        let req = FsRequest::Stat {
            path: String::from("/test"),
        };
        assert_eq!(svc.handle_request(&req), FsResponse::NotImplemented);
    }
}
