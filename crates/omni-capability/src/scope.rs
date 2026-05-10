//! Typed scope vocabulary: action × resource × time × caveats.
//!
//! A [`Scope`] is the authority granted by a capability token. It is
//! deliberately **typed end-to-end** — no `String` stuffed with a
//! free-form action name, no `Vec<u8>` resource opaque blob — so that
//! the compiler catches whole categories of policy bugs (e.g.,
//! comparing a [`Resource::Network`] against a [`Resource::Filesystem`]
//! cannot accidentally succeed).
//!
//! # Invariants
//!
//! * Two scopes are **comparable** iff their [`Action`] discriminants
//!   match AND their [`Resource`] discriminants match.
//! * Subset (`a ⊆ b`) is a partial order: it requires both Actions to
//!   match exactly, the resource pattern of `a` to be at least as
//!   specific as `b`, the time window of `a` to be contained in `b`'s,
//!   and every caveat in `b` to be ALSO present in `a`.
//! * Time is in seconds since the Unix epoch (`u64`). The clock source
//!   is provided by the caller — see `omni-hal::clock` (P6).

use alloc::string::String;
use alloc::vec::Vec;

use omni_types::identity::{AgentId, ModelId, NodeId};
use serde::{Deserialize, Serialize};

// =============================================================================
// Action
// =============================================================================

/// The action a capability authorises.
///
/// `#[non_exhaustive]` so adding new action kinds is backwards-
/// compatible. Pattern-match sites in downstream code should always
/// include a `_ =>` arm.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Action {
    /// Read data from a resource.
    Read,
    /// Write or update data on a resource.
    Write,
    /// Append data to a resource (does not overwrite).
    Append,
    /// Execute a resource (binary, model, etc.).
    Execute,
    /// Delete a resource.
    Delete,
    /// Open an outgoing connection.
    Connect,
    /// Accept incoming connections.
    Listen,
    /// Run inference against a model.
    ModelInfer,
    /// Load a model into memory.
    ModelLoad,
    /// Spawn a child agent.
    AgentSpawn,
    /// Send a message to another agent.
    AgentSend,
}

// =============================================================================
// Resource
// =============================================================================

/// The target of an [`Action`].
///
/// Patterns match a single resource at a time; wildcard semantics live
/// inside each variant (e.g., a `Filesystem` path may end with `/*`).
/// We keep the variants small and well-typed; expanding to a richer
/// pattern grammar is a Phase 2+ decision behind an OIP.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Resource {
    /// Any resource (the implicit-deny ceiling). Use sparingly — every
    /// occurrence is an audit-flagged "broad capability".
    Any,
    /// A filesystem path. Wildcard suffix `/**` matches any descendant.
    Filesystem(String),
    /// A network endpoint, formatted as `host:port` (IPv4/v6 or DNS
    /// name). Port `0` matches any port; host `*` matches any host.
    Network(String),
    /// A specific model identified by content hash.
    Model(ModelId),
    /// A specific agent.
    Agent(AgentId),
    /// A specific node.
    Node(NodeId),
}

impl Resource {
    /// Returns `true` iff `self` is at least as specific as `other`.
    ///
    /// "More specific" means every concrete resource that satisfies
    /// `self` also satisfies `other`. Wildcards in `other` may bind a
    /// concrete value in `self` (so `Filesystem("/data/x")` is more
    /// specific than `Filesystem("/data/**")`), but not vice versa.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        match (self, other) {
            // `Any` is the universal set — anything is a subset of `Any`.
            // The `(Self::Any, _)` case where `_` is concrete is handled
            // by the catch-all wildcard arm at the bottom (returns false).
            (_, Self::Any) => true,
            (Self::Filesystem(a), Self::Filesystem(b)) => path_is_subset(a, b),
            (Self::Network(a), Self::Network(b)) => endpoint_is_subset(a, b),
            (Self::Model(a), Self::Model(b)) => a == b,
            (Self::Agent(a), Self::Agent(b)) => a == b,
            (Self::Node(a), Self::Node(b)) => a == b,
            // Cross-discriminant + (Any, concrete) → not a subset.
            _ => false,
        }
    }
}

// Filesystem path subset semantics:
// * Pattern ending in `/**` matches any path with that prefix.
// * Otherwise exact match.
//
// `a is_subset_of b` iff every concrete path matching `a` also matches `b`.
// Implementation kept minimal — the goal is to be obviously-correct, not
// fast. A future revision can add globbing.
//
// `option_if_let_else` is allowed because the nested `if let` reads
// closer to the spec ("if b ends with /**, then …") than an
// `Option::map_or_else` chain would.
#[allow(clippy::option_if_let_else)]
fn path_is_subset(a: &str, b: &str) -> bool {
    if let Some(b_prefix) = b.strip_suffix("/**") {
        // `b` matches any descendant of `b_prefix`. `a` is a subset iff
        // it's the same wildcard at a deeper level OR a concrete path
        // beneath `b_prefix`.
        if let Some(a_prefix) = a.strip_suffix("/**") {
            a_prefix == b_prefix || a_prefix.starts_with(&alloc::format!("{b_prefix}/"))
        } else {
            a == b_prefix || a.starts_with(&alloc::format!("{b_prefix}/"))
        }
    } else {
        // No wildcard in `b` -> exact match required.
        a == b
    }
}

// Network endpoint subset semantics:
// * `host:port` where host can be `*` and port can be `0` (= any).
// * `a is_subset_of b` iff host and port both match.
fn endpoint_is_subset(a: &str, b: &str) -> bool {
    let Some((a_host, a_port)) = a.rsplit_once(':') else {
        return false;
    };
    let Some((b_host, b_port)) = b.rsplit_once(':') else {
        return false;
    };
    let host_ok = b_host == "*" || a_host == b_host;
    let port_ok = b_port == "0" || a_port == b_port;
    host_ok && port_ok
}

// =============================================================================
// TimeWindow
// =============================================================================

/// A half-open `[not_before, not_after)` time window in seconds since
/// the Unix epoch.
///
/// Validity is checked against a caller-supplied "now" — this crate
/// intentionally does NOT call `SystemTime::now()` because it is
/// `no_std` and because the project policy mandates a monotonic
/// attestable clock.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Inclusive lower bound.
    pub not_before: u64,
    /// Exclusive upper bound.
    pub not_after: u64,
}

impl TimeWindow {
    /// Construct a window. `not_before` must be `<= not_after`; the
    /// constructor returns `None` otherwise.
    #[must_use]
    pub const fn new(not_before: u64, not_after: u64) -> Option<Self> {
        if not_before <= not_after {
            Some(Self {
                not_before,
                not_after,
            })
        } else {
            None
        }
    }

    /// Returns `true` iff `now` falls within `[not_before, not_after)`.
    #[must_use]
    pub const fn contains(&self, now: u64) -> bool {
        now >= self.not_before && now < self.not_after
    }

    /// Returns `true` iff this window is contained in `other`.
    #[must_use]
    pub const fn is_subset_of(&self, other: &Self) -> bool {
        self.not_before >= other.not_before && self.not_after <= other.not_after
    }

    /// Returns the duration of the window in seconds (`not_after -
    /// not_before`).
    #[must_use]
    pub const fn duration_secs(&self) -> u64 {
        self.not_after - self.not_before
    }
}

// =============================================================================
// Caveat
// =============================================================================

/// A monotonic restriction applied during attenuation.
///
/// Caveats can ONLY restrict — never broaden. Every caveat appended in
/// the attenuation chain is checked against the original parent scope
/// at verification time; if any caveat does not hold, the capability
/// is rejected.
///
/// The variants here cover the common cases. The escape hatch
/// [`Caveat::Custom`] carries an opaque tag; downstream verifiers map
/// the tag to a domain-specific predicate via
/// [`crate::attenuation::CaveatPredicate`].
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Caveat {
    /// Tighten the time window (`not_after = min(not_after, t)`).
    ExpiresAt(u64),
    /// Tighten the time window (`not_before = max(not_before, t)`).
    NotBefore(u64),
    /// Restrict to a specific node (caller MUST be running on `node`).
    BoundToNode(NodeId),
    /// Restrict to a specific session.
    BoundToSession([u8; 16]),
    /// Domain-specific tag. The tag MUST be ASCII; the verifier looks
    /// it up in its [`crate::attenuation::CaveatPredicate`] table.
    Custom {
        /// ASCII tag identifying the predicate.
        tag: String,
        /// Opaque payload bytes interpreted by the predicate.
        payload: Vec<u8>,
    },
}

// =============================================================================
// Scope
// =============================================================================

/// The authority a capability grants: an action over a resource within
/// a time window, restricted by zero or more caveats.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Scope {
    /// The authorised action.
    pub action: Action,
    /// The resource the action applies to.
    pub resource: Resource,
    /// The time window during which the capability is valid.
    pub window: TimeWindow,
    /// Restrictions applied along the attenuation chain. Order is
    /// preserved for canonical encoding determinism.
    pub caveats: Vec<Caveat>,
}

impl Scope {
    /// Returns `true` iff `self` is at least as restrictive as `other`.
    ///
    /// Concretely, every (action, resource, time, caveat) request that
    /// satisfies `self` also satisfies `other`. This is the invariant
    /// that [`crate::attenuation::attenuate`] preserves.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        // Action must match exactly — no widening (e.g., Read does not
        // imply Write, even though "less" would be a misleading word).
        if self.action != other.action {
            return false;
        }
        if !self.resource.is_subset_of(&other.resource) {
            return false;
        }
        if !self.window.is_subset_of(&other.window) {
            return false;
        }
        // Every caveat in the parent must also be in the child. The
        // child may add MORE caveats (further restrictions) — that's
        // permitted.
        other.caveats.iter().all(|c| self.caveats.contains(c))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn id32(b: u8) -> NodeId {
        NodeId::from_attestation_hash([b; 32])
    }

    // ---- TimeWindow ---------------------------------------------------------

    #[test]
    fn timewindow_contains() {
        let w = TimeWindow::new(100, 200).unwrap();
        assert!(!w.contains(99));
        assert!(w.contains(100));
        assert!(w.contains(199));
        assert!(!w.contains(200)); // exclusive upper
    }

    #[test]
    fn timewindow_subset() {
        let parent = TimeWindow::new(100, 200).unwrap();
        let child = TimeWindow::new(120, 180).unwrap();
        assert!(child.is_subset_of(&parent));
        assert!(!parent.is_subset_of(&child));
    }

    #[test]
    fn timewindow_rejects_inverted() {
        assert!(TimeWindow::new(200, 100).is_none());
    }

    #[test]
    fn timewindow_zero_duration_is_legal() {
        // Edge case: not_before == not_after means an empty (but valid)
        // window. `contains` returns false for every `now`.
        let w = TimeWindow::new(100, 100).unwrap();
        assert!(!w.contains(100));
        assert_eq!(w.duration_secs(), 0);
    }

    // ---- Resource subset ----------------------------------------------------

    #[test]
    fn resource_filesystem_exact_match() {
        let a = Resource::Filesystem("/data/x".to_string());
        let b = Resource::Filesystem("/data/x".to_string());
        assert!(a.is_subset_of(&b));
    }

    #[test]
    fn resource_filesystem_glob_subset() {
        let concrete = Resource::Filesystem("/data/x/y".to_string());
        let pattern = Resource::Filesystem("/data/**".to_string());
        assert!(concrete.is_subset_of(&pattern));
        assert!(!pattern.is_subset_of(&concrete));
    }

    #[test]
    fn resource_filesystem_glob_to_glob() {
        let inner = Resource::Filesystem("/data/x/**".to_string());
        let outer = Resource::Filesystem("/data/**".to_string());
        assert!(inner.is_subset_of(&outer));
        assert!(!outer.is_subset_of(&inner));
    }

    #[test]
    fn resource_filesystem_disjoint_paths() {
        let a = Resource::Filesystem("/data/x".to_string());
        let b = Resource::Filesystem("/etc/y".to_string());
        assert!(!a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));
    }

    #[test]
    fn resource_network_exact() {
        let a = Resource::Network("api.example.com:443".to_string());
        let b = Resource::Network("api.example.com:443".to_string());
        assert!(a.is_subset_of(&b));
    }

    #[test]
    fn resource_network_host_wildcard() {
        let a = Resource::Network("api.example.com:443".to_string());
        let b = Resource::Network("*:443".to_string());
        assert!(a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));
    }

    #[test]
    fn resource_network_port_wildcard() {
        let a = Resource::Network("api.example.com:443".to_string());
        let b = Resource::Network("api.example.com:0".to_string());
        assert!(a.is_subset_of(&b));
    }

    #[test]
    fn resource_any_is_supremum() {
        let concrete = Resource::Filesystem("/x".to_string());
        assert!(concrete.is_subset_of(&Resource::Any));
        assert!(!Resource::Any.is_subset_of(&concrete));
    }

    #[test]
    fn resource_cross_kind_never_subset() {
        let f = Resource::Filesystem("/x".to_string());
        let n = Resource::Network("h:1".to_string());
        assert!(!f.is_subset_of(&n));
        assert!(!n.is_subset_of(&f));
    }

    #[test]
    fn resource_node_match_by_id() {
        assert!(Resource::Node(id32(1)).is_subset_of(&Resource::Node(id32(1))));
        assert!(!Resource::Node(id32(1)).is_subset_of(&Resource::Node(id32(2))));
    }

    // ---- Scope subset -------------------------------------------------------

    #[test]
    fn scope_subset_same_action_and_window() {
        let parent = Scope {
            action: Action::Read,
            resource: Resource::Filesystem("/data/**".to_string()),
            window: TimeWindow::new(100, 200).unwrap(),
            caveats: vec![],
        };
        let child = Scope {
            action: Action::Read,
            resource: Resource::Filesystem("/data/x".to_string()),
            window: TimeWindow::new(120, 180).unwrap(),
            caveats: vec![],
        };
        assert!(child.is_subset_of(&parent));
        assert!(!parent.is_subset_of(&child));
    }

    #[test]
    fn scope_action_mismatch_never_subset() {
        let read = Scope {
            action: Action::Read,
            resource: Resource::Any,
            window: TimeWindow::new(0, u64::MAX).unwrap(),
            caveats: vec![],
        };
        let write = Scope {
            action: Action::Write,
            resource: Resource::Any,
            window: TimeWindow::new(0, u64::MAX).unwrap(),
            caveats: vec![],
        };
        assert!(!read.is_subset_of(&write));
        assert!(!write.is_subset_of(&read));
    }

    #[test]
    fn scope_child_must_carry_parent_caveats() {
        let parent_caveat = Caveat::BoundToNode(id32(1));
        let parent = Scope {
            action: Action::Read,
            resource: Resource::Any,
            window: TimeWindow::new(0, 100).unwrap(),
            caveats: vec![parent_caveat.clone()],
        };
        let child_missing = Scope {
            action: Action::Read,
            resource: Resource::Any,
            window: TimeWindow::new(0, 100).unwrap(),
            caveats: vec![],
        };
        assert!(!child_missing.is_subset_of(&parent));

        let child_ok = Scope {
            action: Action::Read,
            resource: Resource::Any,
            window: TimeWindow::new(0, 100).unwrap(),
            caveats: vec![parent_caveat],
        };
        assert!(child_ok.is_subset_of(&parent));
    }
}
