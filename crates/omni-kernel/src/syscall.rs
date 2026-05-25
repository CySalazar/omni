//! System call dispatch.
//!
//! ## Status
//!
//! P6.5 scaffold. The syscall *number* enumeration is locked in for
//! the v0.1 protocol surface; the actual dispatcher (which lives in
//! arch-specific entry code, e.g. `int 0x80` / `syscall` / `sysenter`
//! handlers on `x86_64`) is owned by the bootloader integration in P6.2.
//!
//! ## Design rationale
//!
//! - **Stable numeric ABI.** Syscall numbers are immutable after v1.0;
//!   adding a syscall is an OIP. This is the closest the kernel comes
//!   to a userspace ABI guarantee.
//! - **Capability-checked at the entry point.** Every syscall validates
//!   the caller's capability for the requested action before dispatching
//!   to the subsystem.
//! - **Small surface.** The v1 kernel exposes a deliberately small set
//!   of syscalls. Higher-level functionality (e.g. AI invocation) is
//!   provided by userspace services reached via IPC, not by direct
//!   syscall.

#![allow(
    clippy::missing_errors_doc,
    reason = "trait scaffold dispatch returns NotYetImplemented until MB11/MB12 wire handlers"
)]

use crate::KernelResult;

// -----------------------------------------------------------------------------
// Syscall numbers
// -----------------------------------------------------------------------------

/// Stable numeric identifiers for kernel syscalls.
///
/// **The numeric value is part of the userspace ABI.** Do NOT renumber
/// existing variants; only append new variants at the end. Removing a
/// variant requires an OIP and a multi-year deprecation window.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallNumber {
    // ----- Memory -----
    /// `mmap` equivalent: map an anonymous page region.
    MemMap = 1,
    /// Unmap a previously-mapped region.
    MemUnmap = 2,

    // ----- Scheduling / process -----
    /// Create a new task (process or thread).
    TaskCreate = 10,
    /// Terminate the calling task.
    TaskExit = 11,
    /// Yield the CPU voluntarily.
    TaskYield = 12,
    /// Sleep until a deadline.
    TaskSleep = 13,
    /// Spawn a new process with argv/envp and inherited file descriptors.
    /// ABI: `(elf_path_ptr, elf_path_len, argv_ptr, argv_count, envp_ptr, envp_count) -> child_pid`.
    ProcessSpawn = 14,
    /// Wait for a child process to exit.
    /// ABI: `(child_pid, flags, 0, 0, 0, 0) -> (rax=exit_code, rdx=child_pid)`.
    /// Pass `child_pid = 0` to wait for any child. Flags: bit 0 = WNOHANG.
    ProcessWait = 15,
    /// Get the calling process's current working directory.
    /// ABI: `(buf_ptr, buf_len, 0, 0, 0, 0) -> path_len`.
    GetCwd = 16,
    /// Set the calling process's current working directory.
    /// ABI: `(path_ptr, path_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    SetCwd = 17,

    // ----- IPC -----
    /// Create a new channel.
    IpcCreateChannel = 20,
    /// Destroy a channel.
    IpcDestroyChannel = 21,
    /// Send a message.
    IpcSend = 22,
    /// Receive a message.
    IpcReceive = 23,

    // ----- Capabilities -----
    /// Validate a capability.
    CapValidate = 30,
    /// Revoke a capability.
    CapRevoke = 31,
    /// Derive an attenuated capability (Macaroons-style).
    CapAttenuate = 32,

    // ----- TEE / Attestation -----
    /// Request a TEE attestation quote.
    TeeAttest = 40,
    /// Verify a peer's quote.
    TeeVerifyQuote = 41,
    /// Seal a blob under the current TEE measurement.
    TeeSeal = 42,
    /// Unseal a blob.
    TeeUnseal = 43,

    // ----- Time -----
    /// Get monotonic time (nanoseconds since boot).
    TimeMonotonicNanos = 50,

    // ----- I/O (MB11) -----
    /// Write a user-supplied byte slice to the kernel console. ABI:
    /// `(ptr: u64, len: u64) -> u64`. Returns `len` on success or
    /// `u64::MAX` on a validation failure.
    WriteConsole = 60,
    /// Read bytes from the console input buffer (keyboard / serial).
    /// ABI: `(buf_ptr, buf_len, 0, 0, 0, 0) -> bytes_read`.
    /// Line-buffered: blocks until `\n` or `buf_len` bytes available.
    ReadConsole = 61,
    /// Create an anonymous pipe.
    /// ABI: `(0, 0, 0, 0, 0, 0) -> (rax=read_fd, rdx=write_fd)`.
    PipeCreate = 62,
    /// Read from a file descriptor (console, pipe, or file).
    /// ABI: `(fd, buf_ptr, buf_len, 0, 0, 0) -> bytes_read`.
    FdRead = 63,
    /// Write to a file descriptor (console, pipe, or file).
    /// ABI: `(fd, buf_ptr, buf_len, 0, 0, 0) -> bytes_written`.
    FdWrite = 64,
    /// Close a file descriptor.
    /// ABI: `(fd, 0, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    FdClose = 65,
    /// Duplicate a file descriptor (lowest available number).
    /// ABI: `(fd, 0, 0, 0, 0, 0) -> new_fd`.
    FdDup = 66,
    /// Duplicate a file descriptor to a specific target number.
    /// ABI: `(old_fd, new_fd, 0, 0, 0, 0) -> new_fd`.
    FdDup2 = 67,
    /// Seek on a file descriptor.
    /// ABI: `(fd, offset_i64, whence, 0, 0, 0) -> new_offset`.
    /// Whence: 0 = SEEK_SET, 1 = SEEK_CUR, 2 = SEEK_END.
    FdSeek = 68,

    // ----- Driver framework (OIP-013, P6.7.3 skeleton) -----
    // Numeric decade `7x` reserved for the user-space driver framework.
    // See `OIP-Driver-Framework-013` Appendix A for the reconciliation
    // rationale (the original Draft proposed `22..=25` but those slots
    // are MB12-IPC-locked). Handlers are scaffolded to
    // `KernelError::NotYetImplemented` (ENOSYS-equivalent) until the
    // P6.7.8 first-party driver implementations land.
    //
    /// Map a PCI BAR MMIO region into the caller's address space.
    /// ABI: `(phys_base, len, flags, cap_ptr, cap_len) -> va_base`.
    /// See `OIP-Driver-Framework-013` § S2.
    MmioMap = 70,
    /// Install an IOMMU DMA window.
    /// ABI: `(iova_base, len, direction, cap_ptr, cap_len) -> 0`.
    /// See `OIP-Driver-Framework-013` § S3.
    DmaMap = 71,
    /// Attach an IRQ line to a per-driver IPC channel.
    /// ABI: `(irq_line, ipc_channel_id, cap_ptr, cap_len, 0) -> 0`.
    /// See `OIP-Driver-Framework-013` § S4.
    IrqAttach = 72,
    /// Load a signed driver image.
    /// ABI: `(manifest_ptr, manifest_len, image_ptr, image_len, 0) -> driver_pid`.
    /// See `OIP-Driver-Framework-013` § S5.
    DriverLoad = 73,
    /// Issue a kernel-mediated TDCALL on Intel TDX (Ring 0 only).
    /// ABI: `(leaf, r10, r11, r12, r13) -> rax_packed`.
    /// See `OIP-Driver-TEE-016` § S5.3 (editorially reconciled to 74).
    TeeTdcall = 74,
    /// Issue a kernel-mediated SEV-SNP MSR write (Ring 0 only).
    /// ABI: `(msr_index, value_lo, value_hi, payload_ptr, payload_len) -> 0`.
    /// See `OIP-Driver-TEE-016` § S6.3 (editorially reconciled to 75).
    TeeMsr = 75,

    // ----- BLK service-channel registry (OIP-Driver-NVMe-014 § S4) -----
    // Numeric range `76..=78` reserved for the kernel-mediated BLK
    // channel registry that backs the `omni.svc.blk.<diskN>` IPC
    // channel namespace. Producer drivers (NVMe today, future
    // SATA / virtio-blk) call `BlkRegister` after they create the
    // channel via `IpcCreateChannel`; the consumer filesystem
    // service calls `BlkLookup` to resolve `disk_slot → ChannelId`
    // without sniffing the IPC layer by string. See
    // `OIP-Driver-NVMe-014` § S4 + § S6 step 12.
    /// Record an `omni.svc.blk.<disk_slot>` channel in the kernel
    /// BLK registry. ABI:
    /// `(disk_slot_ptr, disk_slot_len, channel_id, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// The caller MUST already own the supplied `channel_id`; the
    /// kernel rejects the call with `EACCES` otherwise. Disk-slot
    /// validation matches [`crate::services::blk::BlkChannelRegistry::register`]
    /// (ASCII `[A-Za-z0-9_-]`, ≤ `MAX_DISK_SLOT_LEN` bytes).
    BlkRegister = 76,
    /// Remove an `omni.svc.blk.<disk_slot>` mapping the caller owns.
    /// ABI: `(disk_slot_ptr, disk_slot_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// Returns `EACCES` if the caller is not the recorded owner;
    /// task-exit clean-up is handled separately via
    /// [`crate::services::blk::BlkChannelRegistry::clear_for_owner`].
    BlkUnregister = 77,
    /// Resolve `omni.svc.blk.<disk_slot>` to its live channel id.
    /// ABI: `(disk_slot_ptr, disk_slot_len, 0, 0, 0, 0) -> (rax=channel_id, rdx=0)`
    /// on success; `(rax=0, rdx=ENOENT)` if the slot is not
    /// registered. Read-only — the channel id alone confers no
    /// IPC authority (`IpcSend` / `IpcRecv` still require the
    /// per-channel capability tokens minted at create time).
    BlkLookup = 78,

    // ----- Filesystem (shell terminal support) -----
    // Numeric range `90..=95` reserved for the in-kernel VFS syscalls
    // that back the shell's filesystem operations. Phase 1: dispatched
    // directly to `InMemoryVfs`. Phase 2: proxied via IPC to the
    // `omni-fs` userspace service.
    /// Open a file and return a file descriptor.
    /// ABI: `(path_ptr, path_len, flags, 0, 0, 0) -> fd`.
    /// Flags follow the `OpenFlags` bitfield (O_RDONLY, O_WRONLY, O_RDWR,
    /// O_CREAT, O_TRUNC, O_APPEND).
    FsOpen = 90,
    /// Stat a file or directory.
    /// ABI: `(path_ptr, path_len, stat_buf_ptr, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// Writes `FileStat` (inode: u64, size: u64, file_type: u8) to stat_buf_ptr.
    FsStat = 91,
    /// List the entries in a directory.
    /// ABI: `(path_ptr, path_len, buf_ptr, buf_len, 0, 0) -> entry_count`.
    /// Writes `\n`-separated entry names to buf_ptr.
    FsListDir = 92,
    /// Create an empty regular file.
    /// ABI: `(path_ptr, path_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    FsCreate = 93,
    /// Delete a file or empty directory.
    /// ABI: `(path_ptr, path_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    FsDelete = 94,
    /// Create a directory.
    /// ABI: `(path_ptr, path_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    FsMkdir = 95,
    /// List all running processes.
    /// ABI: `(buf_ptr, buf_len, 0, 0, 0, 0) -> entry_count`.
    ProcessList = 96,
    /// Terminate another process.
    /// ABI: `(target_pid, 0, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    ProcessKill = 97,

    // ----- AI Runtime (Phase 2 Sprint 2, OIP-Phase2-Entry-021 § AI Surface) -----
    // Numeric decade `8x` reserved for the AI syscall surface. These are
    // thin kernel entry points that validate the caller's capability and
    // forward the request via IPC to the `omni-runtime` service. The kernel
    // does not interpret inference payloads — it is a capability-checked
    // relay.
    /// Invoke a loaded model for synchronous inference.
    /// ABI: `(model_id_ptr, model_id_len, input_ptr, input_len, output_ptr, output_cap) -> output_len`.
    /// Capability-checked: caller must hold an `AiInvoke` capability for the target model.
    AiInvoke = 80,

    /// Start a streaming inference session.
    /// ABI: `(model_id_ptr, model_id_len, input_ptr, input_len, stream_channel_id, 0) -> session_id`.
    /// Returns a `session_id` that the caller uses to receive streamed tokens via IPC.
    AiStream = 81,

    /// Compute an embedding vector for the given input.
    /// ABI: `(model_id_ptr, model_id_len, input_ptr, input_len, output_ptr, output_cap) -> output_len`.
    AiEmbed = 82,

    /// Classify input into a set of categories.
    /// ABI: `(model_id_ptr, model_id_len, input_ptr, input_len, output_ptr, output_cap) -> output_len`.
    AiClassify = 83,

    /// Transcribe audio input to text.
    /// ABI: `(model_id_ptr, model_id_len, input_ptr, input_len, output_ptr, output_cap) -> output_len`.
    AiTranscribe = 84,

    // ----- NET service-channel registry (OIP-Driver-Net-015 § S2) -----
    // Numeric range `100..=113` reserved for the kernel-mediated NET
    // channel registry and the microkernel IPC proxy for socket
    // operations. NIC drivers call `NetRegister` after they create
    // both the command and event channels via `IpcCreateChannel`; the
    // network stack calls `NetLookup` to resolve `interface_name →
    // (ChannelId, EventChannelId)` without sniffing the IPC layer by
    // string. The socket syscalls (103–113) are thin capability-checked
    // relays that forward to the `omni-net` user-space network service.
    //
    // See `OIP-Driver-Net-015` § S2 for the full reconciliation.
    //
    /// Record an `omni.svc.net.<interface>` channel pair in the kernel
    /// NET registry. ABI:
    /// `(interface_name_ptr, name_len, channel_id, event_channel_id, mac_ptr, mac_len) -> (rax=0, rdx=errno)`.
    /// The caller MUST already own both supplied channel ids; the
    /// kernel rejects the call with `EACCES` otherwise.
    /// Interface-name validation matches
    /// [`crate::services::net::NetChannelRegistry::register`]
    /// (ASCII `[A-Za-z0-9_-]`, ≤ `MAX_INTERFACE_NAME_LEN` bytes).
    NetRegister = 100,
    /// Remove an `omni.svc.net.<interface>` mapping the caller owns.
    /// ABI: `(interface_name_ptr, name_len, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// Returns `EACCES` if the caller is not the recorded owner;
    /// task-exit clean-up is handled separately via
    /// [`crate::services::net::NetChannelRegistry::clear_for_owner`].
    NetUnregister = 101,
    /// Resolve `omni.svc.net.<interface>` to its live command channel id.
    /// ABI: `(interface_name_ptr, name_len, 0, 0, 0, 0) -> (rax=channel_id, rdx=0)`
    /// on success; `(rax=0, rdx=ENOENT)` if the interface is not
    /// registered. Read-only.
    NetLookup = 102,
    /// Create a new socket handle via the `omni-net` service.
    /// ABI: `(domain, type, 0, 0, 0, 0) -> socket_handle`.
    NetSocket = 103,
    /// Bind a socket handle to a local address.
    /// ABI: `(handle, addr_ptr, addr_len, 0, 0, 0) -> (rax=0, rdx=errno)`.
    NetBind = 104,
    /// Mark a bound socket as passive (listening).
    /// ABI: `(handle, backlog, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    NetListen = 105,
    /// Accept an incoming connection on a listening socket.
    /// ABI: `(handle, addr_buf_ptr, addr_buf_len, 0, 0, 0) -> new_handle`.
    NetAccept = 106,
    /// Initiate an outgoing connection.
    /// ABI: `(handle, addr_ptr, addr_len, 0, 0, 0) -> (rax=0, rdx=errno)`.
    NetConnect = 107,
    /// Send data on a connected socket.
    /// ABI: `(handle, buf_ptr, buf_len, 0, 0, 0) -> bytes_sent`.
    NetSend = 108,
    /// Receive data from a connected socket.
    /// ABI: `(handle, buf_ptr, buf_len, 0, 0, 0) -> bytes_received`.
    NetRecv = 109,
    /// Send data to an explicit destination address (connectionless).
    /// ABI: `(handle, buf_ptr, buf_len, addr_ptr, addr_len, 0) -> bytes_sent`.
    NetSendTo = 110,
    /// Receive data and record the sender's address (connectionless).
    /// ABI: `(handle, buf_ptr, buf_len, addr_buf_ptr, 0, 0) -> bytes_received`.
    NetRecvFrom = 111,
    /// Close a socket handle.
    /// ABI: `(handle, 0, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    NetClose = 112,
    /// Shut down part or all of a full-duplex connection.
    /// ABI: `(handle, how, 0, 0, 0, 0) -> (rax=0, rdx=errno)`.
    /// `how`: 0 = shut read, 1 = shut write, 2 = shut both.
    NetShutdown = 113,
}

// -----------------------------------------------------------------------------
// Two-register return value (OIP-013 § S2)
// -----------------------------------------------------------------------------

/// Two-register syscall return value.
///
/// The single-register dispatch path returns its value in `RAX`. Some
/// syscalls — initially `MmioMap` per `OIP-Driver-Framework-013` § S2 —
/// also report a POSIX-style error code in `RDX`. The `#[repr(C)]`
/// layout matches the System V AMD64 return convention for a struct
/// of two `INTEGER`-class fields: `rax = first u64`, `rdx = second
/// u64`. The kernel's `extern "C"` syscall dispatcher returns this
/// type by value; the assembly trampoline preserves RDX through to
/// the user-mode `sysretq` / `iretq` so user space observes the pair.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyscallReturn {
    /// Primary return value (`RAX`). Convention: non-zero on success
    /// for handlers that return a handle/VA/length; zero on hard
    /// errors when paired with a non-zero `rdx`.
    pub rax: u64,
    /// Secondary return value (`RDX`). `0` on success; one of the
    /// [`syscall_errno`] codes on error.
    pub rdx: u64,
}

impl SyscallReturn {
    /// Build a successful return with the supplied primary value and
    /// `rdx = 0` (no error).
    #[must_use]
    pub const fn ok(rax: u64) -> Self {
        Self { rax, rdx: 0 }
    }

    /// Build an error return with `rax = 0` and the supplied errno
    /// code in `rdx`.
    #[must_use]
    pub const fn err(errno: u64) -> Self {
        Self { rax: 0, rdx: errno }
    }
}

/// POSIX-aligned syscall errno codes used in the two-register return
/// path. Numbering follows Linux `errno-base.h` for the subset that
/// `OIP-Driver-Framework-013` § S2.3 references.
pub mod syscall_errno {
    /// No such entry — used by the BLK lookup syscall when the
    /// requested disk slot is not registered. POSIX `ENOENT = 2`.
    pub const ENOENT: u64 = 2;
    /// Permission denied — capability verification failed.
    pub const EACCES: u64 = 13;
    /// Bad address — user pointer or length is invalid.
    pub const EFAULT: u64 = 14;
    /// Invalid argument — alignment, range, or reserved bits.
    pub const EINVAL: u64 = 22;
    /// No space left — driver VA range exhausted, or BLK registry
    /// full (`MAX_BLK_CHANNELS`).
    pub const ENOSPC: u64 = 28;
    /// Function not implemented — feature requires runtime support
    /// that has not been initialised (e.g. PAT for WC mappings).
    pub const ENOSYS: u64 = 38;
    /// Object already exists — BLK registry already holds an entry
    /// for the requested disk slot. POSIX `EEXIST = 17`.
    pub const EEXIST: u64 = 17;
    /// Internal kernel invariant violation — surfaces
    /// [`crate::services::blk::BlkRegistryError::Internal`] at
    /// the BLK syscall boundary without aborting the kernel. POSIX
    /// `EIO = 5`.
    pub const EIO: u64 = 5;
    /// Bad file descriptor — `fd` is not open or is not valid.
    /// POSIX `EBADF = 9`.
    pub const EBADF: u64 = 9;
    /// No child processes — `ProcessWait` called but the caller has no
    /// children to wait for. POSIX `ECHILD = 10`.
    pub const ECHILD: u64 = 10;
    /// Broken pipe — write to a pipe whose read end has been closed.
    /// POSIX `EPIPE = 32`.
    pub const EPIPE: u64 = 32;
    /// Illegal seek — the fd does not support seeking (pipes, consoles).
    /// POSIX `ESPIPE = 29`.
    pub const ESPIPE: u64 = 29;
    /// No such process — target PID does not exist.
    /// POSIX `ESRCH = 3`.
    pub const ESRCH: u64 = 3;
    /// File or directory is not empty — `FsDelete` on a non-empty
    /// directory. POSIX `ENOTEMPTY = 39`.
    pub const ENOTEMPTY: u64 = 39;
    /// AI runtime service is not available — the omni-runtime IPC channel
    /// has not been registered. POSIX `EAGAIN = 11`.
    pub const EAGAIN: u64 = 11;
    /// Address already in use — the local address supplied to `NetBind`
    /// is already bound by another socket. POSIX `EADDRINUSE = 98`.
    pub const EADDRINUSE: u64 = 98;
    /// Connection refused — the remote host actively rejected the
    /// connection attempt (`NetConnect`). POSIX `ECONNREFUSED = 111`.
    pub const ECONNREFUSED: u64 = 111;
    /// Connection timed out — `NetConnect` or `NetRecv` did not
    /// complete within the allotted time. POSIX `ETIMEDOUT = 110`.
    pub const ETIMEDOUT: u64 = 110;
    /// Network unreachable — no route to the destination network.
    /// POSIX `ENETUNREACH = 101`.
    pub const ENETUNREACH: u64 = 101;
    /// Host unreachable — no route to the destination host.
    /// POSIX `EHOSTUNREACH = 113`.
    pub const EHOSTUNREACH: u64 = 113;
    /// Connection reset by peer. POSIX `ECONNRESET = 104`.
    pub const ECONNRESET: u64 = 104;
    /// Connection aborted by local policy or error.
    /// POSIX `ECONNABORTED = 103`.
    pub const ECONNABORTED: u64 = 103;
    /// Socket is not connected — `NetSend` / `NetRecv` on an
    /// unconnected socket. POSIX `ENOTCONN = 107`.
    pub const ENOTCONN: u64 = 107;
    /// Socket is already connected — `NetConnect` called on a socket
    /// that already has a peer. POSIX `EISCONN = 106`.
    pub const EISCONN: u64 = 106;
}

// -----------------------------------------------------------------------------
// Syscall dispatcher trait
// -----------------------------------------------------------------------------

/// Trait for the kernel syscall dispatcher.
///
/// The arch-specific entry code (`int 0x80` etc) translates the
/// arch-level register state into a call to `dispatch`; this trait
/// keeps the dispatch logic arch-neutral.
pub trait SyscallDispatcher {
    /// Dispatches a syscall by number with up to 6 generic register
    /// arguments (the `x86_64` ABI fits in 6 GPRs). Returns the syscall
    /// result code or [`crate::KernelError`].
    fn dispatch(&mut self, number: SyscallNumber, args: [u64; 6]) -> KernelResult<u64>;

    /// Dispatches a syscall and returns both `RAX` and `RDX`.
    ///
    /// Default implementation defers to [`Self::dispatch`] and wraps
    /// the result as [`SyscallReturn::ok`] on success or
    /// `SyscallReturn::err(syscall_errno::EINVAL)` on a `KernelError`.
    /// Handlers that need the richer two-register ABI (e.g. `MmioMap`)
    /// override this method to return the specific errno.
    fn dispatch_full(
        &mut self,
        number: SyscallNumber,
        args: [u64; 6],
    ) -> KernelResult<SyscallReturn> {
        self.dispatch(number, args).map(SyscallReturn::ok)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(
        clippy::cognitive_complexity,
        reason = "ABI stability test must enumerate every pinned syscall number in one place"
    )]
    fn syscall_numbers_are_stable() {
        // These constants form the userspace ABI. Any test failure
        // here is a deliberate ABI change and MUST go through OIP.
        assert_eq!(SyscallNumber::MemMap as u32, 1);
        assert_eq!(SyscallNumber::TaskCreate as u32, 10);
        assert_eq!(SyscallNumber::ProcessSpawn as u32, 14);
        assert_eq!(SyscallNumber::ProcessWait as u32, 15);
        assert_eq!(SyscallNumber::GetCwd as u32, 16);
        assert_eq!(SyscallNumber::SetCwd as u32, 17);
        assert_eq!(SyscallNumber::IpcSend as u32, 22);
        assert_eq!(SyscallNumber::CapValidate as u32, 30);
        assert_eq!(SyscallNumber::TeeAttest as u32, 40);
        assert_eq!(SyscallNumber::TimeMonotonicNanos as u32, 50);
        // Shell I/O + fd syscalls (terminal support).
        assert_eq!(SyscallNumber::ReadConsole as u32, 61);
        assert_eq!(SyscallNumber::PipeCreate as u32, 62);
        assert_eq!(SyscallNumber::FdRead as u32, 63);
        assert_eq!(SyscallNumber::FdWrite as u32, 64);
        assert_eq!(SyscallNumber::FdClose as u32, 65);
        assert_eq!(SyscallNumber::FdDup as u32, 66);
        assert_eq!(SyscallNumber::FdDup2 as u32, 67);
        assert_eq!(SyscallNumber::FdSeek as u32, 68);
        // Filesystem syscalls (shell terminal support).
        assert_eq!(SyscallNumber::FsOpen as u32, 90);
        assert_eq!(SyscallNumber::FsStat as u32, 91);
        assert_eq!(SyscallNumber::FsListDir as u32, 92);
        assert_eq!(SyscallNumber::FsCreate as u32, 93);
        assert_eq!(SyscallNumber::FsDelete as u32, 94);
        assert_eq!(SyscallNumber::FsMkdir as u32, 95);
        assert_eq!(SyscallNumber::ProcessList as u32, 96);
        assert_eq!(SyscallNumber::ProcessKill as u32, 97);
        // OIP-013 + OIP-016 driver-framework decade (P6.7.3 skeleton).
        // Pinning these here prevents an accidental renumber that would
        // silently break a driver manifest signed against the old number.
        assert_eq!(SyscallNumber::MmioMap as u32, 70);
        assert_eq!(SyscallNumber::DmaMap as u32, 71);
        assert_eq!(SyscallNumber::IrqAttach as u32, 72);
        assert_eq!(SyscallNumber::DriverLoad as u32, 73);
        assert_eq!(SyscallNumber::TeeTdcall as u32, 74);
        assert_eq!(SyscallNumber::TeeMsr as u32, 75);
        // OIP-Driver-NVMe-014 § S4 + § S6 step 12 BLK registry decade.
        // Pinning these numbers prevents an accidental renumber that
        // would silently break a future NVMe / SATA / virtio-blk
        // driver manifest signed against the old numbers.
        assert_eq!(SyscallNumber::BlkRegister as u32, 76);
        assert_eq!(SyscallNumber::BlkUnregister as u32, 77);
        assert_eq!(SyscallNumber::BlkLookup as u32, 78);
        // OIP-Phase2-Entry-021 AI syscall surface (P2 Sprint 2).
        assert_eq!(SyscallNumber::AiInvoke as u32, 80);
        assert_eq!(SyscallNumber::AiStream as u32, 81);
        assert_eq!(SyscallNumber::AiEmbed as u32, 82);
        assert_eq!(SyscallNumber::AiClassify as u32, 83);
        assert_eq!(SyscallNumber::AiTranscribe as u32, 84);
        // OIP-Driver-Net-015 § S2 NET registry + socket IPC proxy.
        // Pinning these prevents an accidental renumber that would
        // silently break a NIC driver manifest or the network stack
        // ABI signed against the old numbers.
        assert_eq!(SyscallNumber::NetRegister as u32, 100);
        assert_eq!(SyscallNumber::NetUnregister as u32, 101);
        assert_eq!(SyscallNumber::NetLookup as u32, 102);
        assert_eq!(SyscallNumber::NetSocket as u32, 103);
        assert_eq!(SyscallNumber::NetBind as u32, 104);
        assert_eq!(SyscallNumber::NetListen as u32, 105);
        assert_eq!(SyscallNumber::NetAccept as u32, 106);
        assert_eq!(SyscallNumber::NetConnect as u32, 107);
        assert_eq!(SyscallNumber::NetSend as u32, 108);
        assert_eq!(SyscallNumber::NetRecv as u32, 109);
        assert_eq!(SyscallNumber::NetSendTo as u32, 110);
        assert_eq!(SyscallNumber::NetRecvFrom as u32, 111);
        assert_eq!(SyscallNumber::NetClose as u32, 112);
        assert_eq!(SyscallNumber::NetShutdown as u32, 113);
    }

    #[test]
    fn net_syscall_numbers_are_stable() {
        // Dedicated tripwire for the NET syscall range (100–113).
        // This test is intentionally redundant with the slice in
        // `syscall_numbers_are_stable`; the duplication makes it
        // trivial to grep for NET-specific stability assertions.
        assert_eq!(SyscallNumber::NetRegister as u32, 100);
        assert_eq!(SyscallNumber::NetUnregister as u32, 101);
        assert_eq!(SyscallNumber::NetLookup as u32, 102);
        assert_eq!(SyscallNumber::NetSocket as u32, 103);
        assert_eq!(SyscallNumber::NetBind as u32, 104);
        assert_eq!(SyscallNumber::NetListen as u32, 105);
        assert_eq!(SyscallNumber::NetAccept as u32, 106);
        assert_eq!(SyscallNumber::NetConnect as u32, 107);
        assert_eq!(SyscallNumber::NetSend as u32, 108);
        assert_eq!(SyscallNumber::NetRecv as u32, 109);
        assert_eq!(SyscallNumber::NetSendTo as u32, 110);
        assert_eq!(SyscallNumber::NetRecvFrom as u32, 111);
        assert_eq!(SyscallNumber::NetClose as u32, 112);
        assert_eq!(SyscallNumber::NetShutdown as u32, 113);
    }

    #[test]
    fn syscall_number_fits_in_u32() {
        assert_eq!(core::mem::size_of::<SyscallNumber>(), 4);
    }

    // ---- Two-register return path (OIP-013 § S2) -------------------------

    #[test]
    fn syscall_return_ok_zero_errno() {
        let r = SyscallReturn::ok(0x4000_0000);
        assert_eq!(r.rax, 0x4000_0000);
        assert_eq!(r.rdx, 0);
    }

    #[test]
    fn syscall_return_err_zero_rax() {
        let r = SyscallReturn::err(syscall_errno::EACCES);
        assert_eq!(r.rax, 0);
        assert_eq!(r.rdx, 13);
    }

    #[test]
    fn syscall_return_is_two_u64_struct() {
        // Repr(C) on x86_64 places two u64 fields in (rax, rdx) at the
        // SysV ABI boundary. Pin the layout so a re-order would surface
        // as a failing test before the ABI breaks. Field-offset checks
        // are sufficient — the SysV "two INTEGER fields ≤ 16 bytes →
        // return in (rax, rdx)" rule is keyed on the in-memory layout.
        assert_eq!(core::mem::size_of::<SyscallReturn>(), 16);
        assert_eq!(core::mem::align_of::<SyscallReturn>(), 8);
        let r = SyscallReturn { rax: 1, rdx: 2 };
        assert_eq!(r.rax, 1);
        assert_eq!(r.rdx, 2);
        assert_eq!(core::mem::offset_of!(SyscallReturn, rax), 0);
        assert_eq!(core::mem::offset_of!(SyscallReturn, rdx), 8);
    }

    #[test]
    fn syscall_errno_codes_are_posix_aligned() {
        assert_eq!(syscall_errno::ENOENT, 2);
        assert_eq!(syscall_errno::EIO, 5);
        assert_eq!(syscall_errno::EACCES, 13);
        assert_eq!(syscall_errno::EFAULT, 14);
        assert_eq!(syscall_errno::EEXIST, 17);
        assert_eq!(syscall_errno::EINVAL, 22);
        assert_eq!(syscall_errno::ENOSPC, 28);
        assert_eq!(syscall_errno::ENOSYS, 38);
        // AI runtime errno — matches POSIX EAGAIN = 11.
        assert_eq!(syscall_errno::EAGAIN, 11);
    }

    #[test]
    fn ai_syscall_errno_eagain() {
        assert_eq!(syscall_errno::EAGAIN, 11);
    }

    #[test]
    fn net_syscall_errno_codes_are_posix_aligned() {
        // These values match the Linux `errno.h` numbers for the
        // NET socket error family. Any deviation from the POSIX table
        // must be accompanied by an OIP and a comment explaining why.
        assert_eq!(syscall_errno::EADDRINUSE, 98);
        assert_eq!(syscall_errno::ECONNREFUSED, 111);
        assert_eq!(syscall_errno::ETIMEDOUT, 110);
        assert_eq!(syscall_errno::ENETUNREACH, 101);
        assert_eq!(syscall_errno::EHOSTUNREACH, 113);
        assert_eq!(syscall_errno::ECONNRESET, 104);
        assert_eq!(syscall_errno::ECONNABORTED, 103);
        assert_eq!(syscall_errno::ENOTCONN, 107);
        assert_eq!(syscall_errno::EISCONN, 106);
    }
}
