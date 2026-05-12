//! Virtio device backends — the **only** host↔guest I/O path for an
//! `OmniContainer`.
//!
//! See `OIP-Container-006` § 3 ("virtio device backing and capability
//! binding"). Every virtio device exposed to the guest is backed by a
//! host-side OMNI userspace service that enforces capability scope on
//! each request. The guest sees a generic virtio device; the host
//! side translates each guest request to a capability check + an
//! OMNI primitive call.
//!
//! | Device | Host backing | Capability required |
//! |---|---|---|
//! | `virtio-fs`      | `omni-fs` | `fs:read:<path>` / `fs:write:<path>` |
//! | `virtio-net`     | OMNI network stack | `net:outbound:<host>:<port>` / `net:inbound:<port>` |
//! | `virtio-vsock`   | OMNI IPC bridge | `ipc:channel:<id>` |
//! | `virtio-gpu`     | OMNI tensor HAL | `gpu:shared` / `gpu:exclusive:<id>` |
//! | `virtio-rng`     | Kernel `getrandom` | (always granted) |
//!
//! v0.1 status: each submodule defines the host-side **trait** that
//! the concrete backend will implement. No real I/O happens yet; each
//! trait method returns `Err(ContainerError::NotYetImplemented(...))`
//! with a PII-safe static slug.

pub mod fs;
pub mod gpu;
pub mod net;
pub mod rng;
pub mod vsock;

pub use fs::VirtioFsBackend;
pub use gpu::VirtioGpuBackend;
pub use net::VirtioNetBackend;
pub use rng::VirtioRngBackend;
pub use vsock::VirtioVsockBackend;
