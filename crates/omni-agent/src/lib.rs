//! # `omni-agent`
//!
//! Agent framework for OMNI OS.
//!
//! Implements the **Agent** OS primitive: an autonomous AI-driven entity
//! that has a declared policy, persistent context, capability tokens, and
//! a computational budget. Agents differ from processes by being
//! goal-directed rather than instruction-stream-directed.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. Implementation arrives in Phase 2 per
//! [`/docs/06-roadmap.md`](../../../docs/06-roadmap.md).
//!
//! ## Design rationale
//!
//! - **Capability-bound**: every agent action requires a valid capability.
//!   The agent cannot exceed its declared scope.
//! - **Bounded budget**: each agent has a budget (tokens, compute time,
//!   memory). Exceeding it terminates the agent gracefully.
//! - **Sandboxed execution**: agents run in restricted sandboxes (WASM
//!   for v1; possibly process-level isolation in alternative configs).
//! - **Persistent context, scoped**: agents may carry state across
//!   sessions, but state is bound to a specific user + scope and cannot
//!   be exfiltrated.
//!
//! ## Modules
//!
//! - [`agent`] — `Agent` trait + lifecycle management.
//! - [`policy`] — agent policy declaration.
//! - [`context`] — persistent context store.
//! - [`budget`] — computational budget tracking.
//! - [`sandbox`] — sandboxed execution (WASM-based v1).

#![doc(html_root_url = "https://docs.omni-os.org/omni-agent")]
#![warn(missing_docs)]

/// `Agent` trait + lifecycle management.
pub mod agent {
    // TODO(phase-2): `Agent` trait, lifecycle (spawn, suspend, resume, kill).
}

/// Agent policy declaration.
pub mod policy {
    // TODO(phase-2): policy DSL or structured types.
}

/// Persistent context store, scoped per agent + user.
pub mod context {
    // TODO(phase-2): TEE-bound context storage.
}

/// Computational budget tracking and enforcement.
pub mod budget {
    // TODO(phase-2): budget accounting (tokens, time, memory).
}

/// Sandboxed execution.
pub mod sandbox {
    // TODO(phase-2): WASM sandbox via wasmtime.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
