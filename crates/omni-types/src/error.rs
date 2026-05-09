//! Common error types used across the workspace.
//!
//! OMNI OS uses `thiserror` for library error types and `anyhow` for
//! application-level error handling. Each crate defines its own error
//! enum that maps cleanly to a top-level `OmniError` defined here.
//!
//! ## Design rationale
//!
//! - **Explicit error variants**: panics are reserved for invariant
//!   violations only. All recoverable conditions return `Result`.
//! - **No `unwrap_used` / `expect_used`** in production code paths; both
//!   are warned by workspace-level Clippy lints.
//! - **PII never appears in error messages**: error messages must not
//!   leak sensitive data. Errors carry opaque identifiers, not contents.

// TODO(phase-1): define the top-level OmniError taxonomy.
