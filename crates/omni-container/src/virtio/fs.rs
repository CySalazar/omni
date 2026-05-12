//! `virtio-fs` host-side backend trait.
//!
//! See `OIP-Container-006` § 3. The backend implements two operations
//! that map to the guest's filesystem syscalls:
//!
//! - `open(path, flags)` — capability-checked against `fs:read:<path>`
//!   or `fs:write:<path>` (or both). Returns a host-side file handle
//!   that the guest sees as a virtio-fs FD.
//! - `close(handle)` — releases the host-side resource.
//!
//! Capability denial returns `Err(ContainerError::Capability(...))`
//! at the host side; the guest sees a virtio-fs `EACCES` response,
//! which its kernel surfaces to the user app as a regular POSIX
//! `EACCES`. This is the mechanism by which capabilities are enforced
//! **structurally** rather than retroactively.

use crate::{ContainerError, ContainerResult};

/// Opaque host-side file handle. The guest sees it as a virtio-fs FD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FsHandle(pub u64);

/// virtio-fs backend trait.
pub trait VirtioFsBackend: Send + Sync {
    /// Open a guest-supplied path with the requested access mode.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Capability`] if the container does
    /// not hold the corresponding `fs:read:<path>` or `fs:write:<path>`
    /// capability, [`ContainerError::Virtio`] for any host-side I/O
    /// failure, or [`ContainerError::NotYetImplemented`] in the
    /// v0.1 scaffold.
    fn open(&self, path: &str, write: bool) -> ContainerResult<FsHandle>;

    /// Close a host-side file handle.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Virtio`] for host-side errors or
    /// [`ContainerError::NotYetImplemented`] in the v0.1 scaffold.
    fn close(&self, handle: FsHandle) -> ContainerResult<()>;
}

/// v0.1 stub implementation. Every call returns
/// [`ContainerError::NotYetImplemented`].
#[derive(Debug, Default)]
pub struct StubVirtioFs;

impl VirtioFsBackend for StubVirtioFs {
    fn open(&self, _path: &str, _write: bool) -> ContainerResult<FsHandle> {
        Err(ContainerError::NotYetImplemented("virtio::fs::open"))
    }
    fn close(&self, _handle: FsHandle) -> ContainerResult<()> {
        Err(ContainerError::NotYetImplemented("virtio::fs::close"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stub_open_returns_not_yet_implemented() {
        let b = StubVirtioFs;
        let err = b.open("/tmp/x", false).expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::fs::open")
        ));
    }

    #[test]
    fn stub_close_returns_not_yet_implemented() {
        let b = StubVirtioFs;
        let err = b.close(FsHandle(0)).expect_err("stub");
        assert!(matches!(
            err,
            ContainerError::NotYetImplemented("virtio::fs::close")
        ));
    }
}
