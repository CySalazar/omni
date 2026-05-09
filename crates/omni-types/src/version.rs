//! Semantic version and protocol version helpers.
//!
//! OMNI OS distinguishes:
//!
//! - **OS version**: follows `SemVer` 2.0.0 (`0.1.0`, `1.0.0`, etc.).
//! - **Protocol version**: a separate identifier negotiated at handshake
//!   on the mesh. Format `OMNI-PROTO-vN.M`. Decoupled from OS version so
//!   protocol can evolve at a different cadence.
//!
//! See [`/docs/09-tech-specifications.md`](../../../../docs/09-tech-specifications.md)
//! § "Versioning policy".

// TODO(phase-1): define `OsVersion` and `ProtocolVersion` types with
// negotiation helpers.
