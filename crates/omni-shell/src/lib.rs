//! # `omni-shell`
//!
//! System shell for OMNI OS.
//!
//! v1 ships a traditional command-line shell sufficient for development
//! and basic system administration. The long-term vision is an
//! **intent-based shell**: natural language is the primary interface, with
//! the shell lowering user intent into a structured plan that the user
//! previews and approves before execution.
//!
//! ## Status
//!
//! Draft v0.1 — scaffold. The basic CLI lands in Phase 1 (sufficient for
//! kernel development); intent-based features arrive in Phase 4+.
//!
//! ## Design rationale
//!
//! - **Plan-then-execute**: AI-generated commands never auto-execute.
//!   The user always sees a plan and approves it.
//! - **Capability-aware**: the shell holds a capability for the user's
//!   session and forwards it to invoked commands.
//! - **Auditable**: every command, plan, and result are logged to the
//!   per-user audit log.
//!
//! ## Modules
//!
//! - [`cli`] — argument parsing and entry points.
//! - [`command`] — command dispatch.
//! - [`repl`] — read-eval-print loop.

#![doc(html_root_url = "https://docs.omni-os.org/omni-shell")]
#![warn(missing_docs)]

/// Argument parsing and entry points.
pub mod cli {
    // TODO(phase-1): basic CLI scaffold.
}

/// Command dispatch.
pub mod command {
    // TODO(phase-1): registry of built-in commands.
}

/// Read-eval-print loop.
pub mod repl {
    // TODO(phase-1): interactive REPL.
}

#[cfg(test)]
mod tests {
    /// Placeholder test asserting the crate compiles.
    #[test]
    fn placeholder() {}
}
