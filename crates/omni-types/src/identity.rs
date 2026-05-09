//! System-level identifiers.
//!
//! Defines the strongly-typed identifier newtypes used across OMNI OS to
//! prevent accidental conflation of different ID kinds (e.g., passing a
//! `ModelId` where a `NodeId` is expected).
//!
//! ## Planned types (Phase 1)
//!
//! - `NodeId` — derived from a node's TEE attestation report; deterministic
//!   and unforgeable.
//! - `AgentId` — local identifier for an agent within a node.
//! - `ModelId` — content-addressed identifier (hash of the signed manifest).
//! - `CapabilityId` — opaque identifier for a capability token.
//! - `SessionId` — short-lived identifier for an inference session.

// TODO(phase-1): define identifier newtypes with appropriate trait derives.
