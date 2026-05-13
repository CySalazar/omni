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
//! Draft v0.2 — the [`tee`] module now re-exports [`omni_tee`]'s trait
//! surface so consumers can write `use omni_hal::tee::TeeBackend;` and not
//! care that the underlying implementation lives in a sibling crate.
//! Tensor, network, and storage modules remain scaffolds; their P1+
//! implementations land per the roadmap.
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
//! - [`tee`] — TEE HAL (re-exports from [`omni_tee`]).

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

/// TEE HAL — re-exports and integration with [`omni_tee`].
///
/// Consumers that want the TEE trait surface should `use omni_hal::tee::*`
/// (or pull individual symbols). The point of this module is to give
/// every HAL consumer a single dependency (`omni-hal`) instead of two
/// (`omni-hal` plus `omni-tee`), which simplifies the build graph and
/// makes the workspace's HAL story coherent.
///
/// Future additions: a `select_tee_backend()` helper that detects the
/// available TEE family at runtime and returns a `Box<dyn TeeBackend>`.
/// That helper requires `std`; it will land behind a feature flag when
/// `omni-runtime` integrates it.
pub mod tee {
    // Re-export the full vendor-neutral surface.
    pub use omni_tee::{
        Measurement, Nonce, Quote, QuoteVersion, SealPolicy, SealedBlob, TeeBackend, TeeError,
        TeeErrorKind, TeeFamily, TeeSharedKey,
    };

    // Re-export concrete backends when their features are enabled. Each
    // re-export is gated on the same feature as the backend itself, so a
    // build that didn't enable `tdx` does not need to compile its
    // dependencies.
    #[cfg(feature = "mock")]
    pub use omni_tee::MockTeeBackend;

    #[cfg(feature = "tdx")]
    pub use omni_tee::tdx::TdxBackend;

    #[cfg(feature = "sev-snp")]
    pub use omni_tee::sev_snp::SevSnpBackend;
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
