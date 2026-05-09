//! # `omni-hal`
//!
//! Hardware Abstraction Layer for OMNI OS.
//!
//! Defines vendor-neutral traits for the four hardware classes that OMNI OS
//! cares about: tensor accelerators (CPU/GPU/NPU), networking, storage, and
//! TEEs. Userspace services depend on traits in this crate; concrete
//! backends are loaded at runtime based on detected hardware.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Trait definitions land in Phase 1 with concrete
//! backends following in Phase 2 (tensor) and beyond.
//!
//! ## Design rationale
//!
//! - **Trait-based dispatch**: callers don't know whether inference runs
//!   on CPU AVX-512, NVIDIA CUDA, or an integrated NPU. The HAL hides it.
//! - **Runtime backend selection**: concrete backends (e.g., CUDA wrappers)
//!   are dynamically loaded. Missing hardware is detected gracefully.
//! - **Async by default**: I/O and inference workloads are async-first.
//! - **TEE HAL is mandatory**: a node without a working TEE HAL cannot
//!   participate in the mesh.
//!
//! ## Modules
//!
//! - [`tensor`] — Tensor HAL (compute dispatch).
//! - [`network`] — Network HAL (transport-agnostic).
//! - [`storage`] — Storage HAL (block + filesystem-friendly).
//! - [`tee`] — TEE HAL (re-exports + integration with `omni-tee`).

#![doc(html_root_url = "https://docs.omni-os.org/omni-hal")]
#![warn(missing_docs)]

/// Tensor HAL — uniform compute dispatch across CPU/GPU/NPU.
pub mod tensor {
    // TODO(phase-2): `TensorBackend` trait + dispatch logic.
}

/// Network HAL — transport-agnostic networking primitives.
pub mod network {
    // TODO(phase-1): `NetworkBackend` trait covering Ethernet/Wi-Fi.
}

/// Storage HAL — block storage abstractions.
pub mod storage {
    // TODO(phase-1): `BlockDevice` and friends (NVMe-first).
}

/// TEE HAL — re-exports and integration with `omni-tee`.
pub mod tee {
    // TODO(phase-1): trait re-exports from `omni-tee` for HAL consumers.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
