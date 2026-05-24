//! # `omni-agent`
//!
//! Five-Agent framework for OMNI OS.
//!
//! Implements the five-agent architecture defined in OIP-Agent-Arch-022:
//!
//! | Agent | Role | Module |
//! |---|---|---|
//! | **Orchestrator** (`orch`) | Central coordinator | [`orchestrator`] |
//! | **Guidance** (`guid`) | User assistant / educator | [`guidance`] |
//! | **`SysAdmin`** (`sadm`) | Technical operator | [`sysadmin`] |
//! | **Security** (`secp`) | Guardian / performance optimizer | [`security`] |
//! | **Task** (`task`) | User-delegated productive work | [`task`] |
//!
//! The framework provides two system-wide operational modes:
//!
//! - **Standard Mode**: Security Agent is advisory; user has final say.
//! - **High-Risk Mode**: Security Agent has absolute veto over all actors.
//!
//! An **Emergency Recovery Mode** provides a time-bounded, physically-
//! authenticated escape hatch for High-Risk veto override.
//!
//! ## Shared infrastructure
//!
//! - [`agent`] — `Agent` trait and `AgentKind` enum.
//! - [`mode`] — `OperationalMode` and `ModeManager`.
//! - [`message`] — Inter-agent communication protocol.
//! - [`policy`] — Per-agent capability policies.
//! - [`context`] — Per-agent persistent context store.
//! - [`budget`] — Per-agent computational budget.
//! - [`sandbox`] — Sandboxed execution environment.
//! - [`privacy`] — Differential privacy budget accountant (ε-DP, sequential composition).

#![doc(html_root_url = "https://docs.omni-os.org/omni-agent")]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unnecessary_wraps,
        clippy::indexing_slicing,
    )
)]

// ── Core trait & taxonomy ──────────────────────────────────────────

/// `Agent` trait, `AgentKind` enum, lifecycle states.
pub mod agent;

/// System-wide operational modes (Standard / High-Risk / Emergency Recovery).
pub mod mode;

/// Inter-agent communication protocol.
pub mod message;

// ── Shared infrastructure ──────────────────────────────────────────

/// Per-agent capability policy declarations.
pub mod policy;

/// Per-agent persistent context store.
pub mod context;

/// Per-agent computational budget tracking.
pub mod budget;

/// Sandboxed execution environment.
pub mod sandbox;

/// Differential privacy budget accountant (ε-DP, sequential composition).
pub mod privacy;

// ── Agent implementations ──────────────────────────────────────────

/// Orchestrator Agent — central coordinator.
pub mod orchestrator;

/// Guidance Agent — user assistant / educator (incorporates OIP-007).
pub mod guidance;

/// System Administrator Agent — technical operator.
pub mod sysadmin;

/// Security & Performance Agent — guardian of the system.
pub mod security;

/// Task Agent — user-delegated productive work.
pub mod task;

// ── Re-exports ─────────────────────────────────────────────────────

pub use crate::agent::{Agent, AgentKind, AgentState};
pub use crate::guidance::GuidanceAgent;
pub use crate::mode::OperationalMode;
pub use crate::orchestrator::OrchestratorAgent;
pub use crate::security::SecurityAgent;
pub use crate::sysadmin::SysAdminAgent;
pub use crate::task::TaskAgent;

#[cfg(test)]
mod tests {
    use super::*;
    use omni_types::AgentId;

    #[test]
    fn all_agent_kinds_have_distinct_short_ids() {
        let ids: Vec<&str> = AgentKind::all().iter().map(|k| k.short_id()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }

    #[test]
    fn agents_can_be_constructed() {
        let orch = OrchestratorAgent::new(AgentId::from_bytes([0x01; 16]));
        let guid = GuidanceAgent::new(AgentId::from_bytes([0x02; 16]));
        let sadm = SysAdminAgent::new(AgentId::from_bytes([0x03; 16]));
        let secp = SecurityAgent::new(AgentId::from_bytes([0x04; 16]));
        let task = TaskAgent::new(AgentId::from_bytes([0x05; 16]));

        assert_eq!(orch.kind(), AgentKind::Orchestrator);
        assert_eq!(guid.kind(), AgentKind::Guidance);
        assert_eq!(sadm.kind(), AgentKind::SysAdmin);
        assert_eq!(secp.kind(), AgentKind::Security);
        assert_eq!(task.kind(), AgentKind::Task);
    }

    #[test]
    fn default_mode_is_standard() {
        assert_eq!(OperationalMode::default(), OperationalMode::Standard);
    }

    #[tokio::test]
    async fn full_lifecycle_all_agents() {
        let ids: Vec<AgentId> = (1..=5u8).map(|i| AgentId::from_bytes([i; 16])).collect();

        let mut agents: Vec<Box<dyn Agent>> = vec![
            Box::new(OrchestratorAgent::new(ids[0])),
            Box::new(GuidanceAgent::new(ids[1])),
            Box::new(SysAdminAgent::new(ids[2])),
            Box::new(SecurityAgent::new(ids[3])),
            Box::new(TaskAgent::new(ids[4])),
        ];

        for agent in &mut agents {
            assert_eq!(agent.state(), AgentState::Initializing);
            agent.spawn().await.unwrap();
            assert_eq!(agent.state(), AgentState::Running);
            agent.suspend().await.unwrap();
            assert_eq!(agent.state(), AgentState::Suspended);
            agent.resume().await.unwrap();
            assert_eq!(agent.state(), AgentState::Running);
            agent.shutdown().await.unwrap();
            assert_eq!(agent.state(), AgentState::Shutdown);
        }
    }
}
