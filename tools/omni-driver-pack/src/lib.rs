//! `omni-driver-pack` — OMNI OS driver-pack v1 producer.
//!
//! This library crate exposes the core logic used by the `omni-driver-pack`
//! binary. Integration tests in `tests/` import from here, and downstream
//! build systems that want to produce `.opack` blobs programmatically can
//! use this crate directly.
//!
//! ## Usage (binary — see `--help` for full reference)
//!
//! ```text
//! omni-driver-pack \
//!   --manifest  path/to/driver.json \
//!   --image     path/to/ring3.elf \
//!   --signing-key path/to/ed25519.seed \
//!   --output    driver.opack
//! ```
//!
//! ## Library entry points
//!
//! - [`manifest::PackManifestJson`] — parse a JSON manifest.
//! - [`pack::build_opack`] — assemble a signed omni-pack v1 blob.
//! - [`keyfile::read_signing_seed`] — read a hex-encoded Ed25519 seed file.
//! - [`error::PackError`] — typed error enum with exit-code mapping.
//!
//! ## Wire format
//!
//! The tool produces blobs conforming to `OIP-Driver-Framework-013` § S5.5.
//! The kernel-side decoder lives in
//! [`omni_kernel::driver_manifest::decode_omni_pack`].

/// Typed error enum and exit-code mapping.
pub mod error;
/// Signing-key file reader and Unix permission checker.
pub mod keyfile;
/// JSON manifest schema and deserialization.
pub mod manifest;
/// omni-pack v1 binary blob builder.
pub mod pack;
