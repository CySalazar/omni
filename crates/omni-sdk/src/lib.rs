//! # `omni-sdk`
//!
//! Application SDK for OMNI OS.
//!
//! High-level Rust API used by applications to invoke AI capabilities,
//! interact with agents, and handle encrypted data types. The SDK is the
//! primary integration surface for third-party developers.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Implementation arrives in Phase 2 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md).
//!
//! ## Design rationale
//!
//! - **Ergonomics matters**: the SDK is the surface where adoption-by-
//!   developers is won or lost. APIs are designed for the common case to
//!   be one line.
//! - **Capabilities are first-class**: every API takes a capability token.
//!   Applications cannot "forget" to authenticate; the type system requires
//!   it.
//! - **Encrypted types propagate**: an `EncryptedString` cannot be
//!   converted to a `String` outside a TEE; the SDK preserves this through
//!   its own API.
//! - **Async-first**: every I/O / inference API is async.
//!
//! ## Modules
//!
//! - [`prelude`] — convenience re-exports for `use omni_sdk::prelude::*;`.
//! - [`ai`] — AI invocation API.
//! - [`agent`] — agent framework integration.
//! - [`data`] — encrypted-data-type integration.

#![doc(html_root_url = "https://docs.omni-os.org/omni-sdk")]
#![warn(missing_docs)]

/// Convenience re-exports for `use omni_sdk::prelude::*;`.
pub mod prelude {
    // TODO(phase-2): top-level re-exports.
}

/// AI invocation API.
pub mod ai {
    // TODO(phase-2): high-level AI invocation surface.
}

/// Agent framework integration.
pub mod agent {
    // TODO(phase-2): integration with `omni-agent`.
}

/// Encrypted-data-type integration.
pub mod data {
    // TODO(phase-2): re-exports + helpers for encrypted types.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
