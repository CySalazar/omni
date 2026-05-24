# Governance

**Status:** Draft v0.2

> **Changelog**
> - **v0.2 (2026-05-10):** OIP process delegated to authoritative `OIP-Process-001`
>   (`Active`); BDFL veto window dates immutably anchored (2026-05-09 → 2031-05-09,
>   sunset 23:59 UTC); founder role (years 1–5 / 5+ / 10+) fully fleshed; veto-log
>   requirement formalized at `docs/audits/bdfl-veto-log.md`; previously open
>   questions on voting threshold and category lifecycle resolved by `OIP-Process-001`.
> - **v0.1 (initial):** three-layer model, anti-Sybil, forking policy, conflict
>   resolution, transparency commitments.

## Three-layer governance model

OMNI OS governance is structured in three layers, each with distinct authority, speed, and reversibility.

```
┌────────────────────────────────────────────────────────┐
│  LAYER 3 — Operational (Stichting OMNI, Netherlands)   │
│  Codebase, seed nodes, partnerships, legal, funding    │
│                          │                             │
│                          ▼                             │
│  LAYER 2 — Specification (community-federated, OIP)    │
│  Protocol evolution, blessed model registry, params    │
│                          │                             │
│                          ▼                             │
│  LAYER 1 — Protocol (cryptographic, immutable runtime) │
│  Crypto rules, compliance proofs, privacy primitives   │
└────────────────────────────────────────────────────────┘
        Authority decreases as you go up.
        Reversibility decreases as you go down.
```

### Layer 1 — Protocol (cryptographic enforcement)

Rules enforced by every conforming node, automatically. No human authority can override at runtime. The "operating constitution" of the mesh.

What lives here:

- Mandatory cryptographic primitives (cipher suites, hash functions, signature schemes)
- Required compliance proof formats
- Acceptable cipher suites (with sunset dates for deprecation)
- PII handling rules at protocol level (encrypted-by-default types, tokenization requirements)
- Privacy-preserving routing requirements (TEE-bound decryption, FPE for metadata)

Modification path: only via Layer 2 process, with high adoption thresholds (≥75% of active nodes for ≥30 days).

### Layer 2 — Specification (community-federated)

How the protocol evolves. Modeled after IETF RFCs, Bitcoin BIPs, and Ethereum EIPs.

#### OIP process

The procedural detail of the OIP process — categories, lifecycle, voting, eligibility, editor
body, BDFL veto, Bootstrap Period — is the subject of [`OIP-Process-001`](../oips/oip-process-001.md)
(`Active` since 2026-05-10 under the bootstrap fiat clause defined in that OIP §6.3). This
section provides a high-level overview; **`OIP-Process-001` is authoritative on every detail**
and supersedes any earlier sketch in this document.

High-level summary:

1. **Proposal**: anyone files an OIP on the public OIP repository ([`/oips/`](../oips/README.md)) using the canonical template.
2. **Discussion**: public discussion on GitHub Discussions and the linked PR (open, archived).
3. **Reference implementation**: required for `Standards Track` OIPs; not required for `Process`/`Informational`/`Meta`.
4. **Vote**: weighted by **proof-of-uptime + proof-of-contribution**, anti-Sybil via TEE attestation (1 unique device = 1 vote), quadratic voting to reduce concentration of power. Quorum 30% of eligible weighted vote OR 14-day window, whichever first; 50%+1 quadratic majority for approval; 66.7% supermajority for OIPs that break Layer 1 cryptographic guarantees. See `OIP-Process-001` §5 for the formula.
5. **Activation**: for `Standards Track`, the new behavior runs in parallel with the old; the OIP transitions from `Active` to `Final` when ≥75% of attested active nodes have run the implementation for ≥30 consecutive days, with no unresolved Critical-severity finding. Old behavior is deprecated when usage drops below a threshold.

OIP categories: **Standards Track** (protocol), **Process** (governance), **Informational** (guidelines), **Meta** (OIPs governing the OIP process itself).

OIP lifecycle: `Draft → Review → Last Call → Active → Final | Withdrawn | Superseded | Rejected`.

#### Founder role (years 1–5)

For the **5-year window from 2026-05-09 to 2031-05-09 (immutable sunset, 23:59 UTC)**, the
project founder (cySalazar) holds:

- **Lead Architect** title with technical leadership responsibility.
- **Soft veto** on `Standards Track` OIPs that break Layer 1 protocol guarantees: the founder
  can *block* a proposal but cannot *impose* one. The veto cannot be applied to `Process`,
  `Informational`, or `Meta` OIPs, and it cannot be applied to a `Meta` OIP that narrows the
  founder's own authority (asymmetric clause). The veto is therefore **structurally
  non-extensible** by founder action alone.

The 5-year anchor is **2026-05-09**, the date the public repository
`github.com/CySalazar/omni` opened with the founder identity GitHub-verified. This date is
recorded immutably in:

- This document (versioned under `main`, signed commits).
- [`OIP-Process-001` §5.4](../oips/oip-process-001.md) (also versioned, ratified under bootstrap fiat).
- The first commit on `main` (`61426d5`, 2026-05-09, signed) — providing on-chain (well, on-Git) verifiable provenance.

The veto sunsets at 2031-05-09 by both `OIP-Process-001` and Stichting bylaws (once the
Stichting is constituted per [`08-funding-policy.md`](08-funding-policy.md) and the roadmap's
Phase 0 closure). Each veto exercise MUST be logged in
[`docs/audits/bdfl-veto-log.md`](audits/bdfl-veto-log.md) with the OIP number, date, and
written rationale (or the file is created the first time a veto is exercised).

#### After year 5 (post-2031-05-09)

Founder retains an **advisory role** with no veto. All protocol decisions are made by the OIP
process described in `OIP-Process-001`.

#### After year 10 (post-2036-05-09)

Full transition to community-elected technical board. Trustees of Stichting OMNI are no longer
founder-appointed; they are elected via the OIP process.

### Layer 3 — Operational (legal entity)

A legal entity sustains operations: codebase maintenance, seed node operation (initially), partnerships, legal response, funding allocation.

**Entity:** **Stichting OMNI** (Foundation, Netherlands).

#### Structure

- Board of 5 trustees, 3-year rotating mandates.
- Founder (cySalazar) on board for years 1–5 by initial appointment.
- ≥1 trustee resident in the Netherlands (regulatory practical requirement).
- Director (executive) for day-to-day operations; reports to the board.

#### Functions

- Maintain reference implementation of OMNI OS (Rust codebase, builds, releases).
- Operate seed nodes for mesh discovery (years 1–5; gradually transferred to high-reputation community-operated nodes thereafter).
- Curate "blessed model registry" — officially recommended, signed, audited models.
- Negotiate hardware vendor partnerships for TEE support, drivers, certifications.
- Respond to legal requests (DMCA, GDPR data requests, subpoenas) per published policy.
- Allocate funding with transparent annual audited reports.
- Run external security audits and publish results.

#### What the Foundation explicitly does NOT do

- **Cannot read user data.** The Foundation has no privileged access to mesh traffic; cryptographic guarantees apply equally to it.
- **Cannot revoke compliant nodes.** Reputation is local; no central revocation list overrides cryptographic compliance.
- **Cannot impose protocol changes unilaterally.** All changes go through the OIP process.

This separation is the structural anti-capture guarantee.

## Anti-Sybil mechanisms

A federated voting system requires Sybil resistance. OMNI OS achieves this via:

- **TEE attestation as identity**: each unique TEE device produces one identity. Cloning attestation requires breaking the TEE vendor's attestation chain — economically infeasible.
- **Rate-limited new identities**: a platform fingerprint (TEE vendor + chip generation) sets per-fingerprint rate limits on new attestations, blocking datacenter clones.
- **Proof-of-uptime weighting**: voting weight grows with continuous network presence, capping the influence of recently-attested nodes.
- **Quadratic voting**: vote weight scales sublinearly with stake (here, contribution), reducing plutocracy risk.

## Forking policy

Forks are first-class citizens. A fork that:

- **Implements the same protocol** → is fully interoperable on the mesh. The Foundation does not litigate. Apache-2.0 obligations apply.
- **Modifies the protocol** → forms a separate mesh, not interoperable with the main one, but free to exist.

This policy is structural: any captured Foundation can be forked. The fork can re-join the same mesh on the same protocol terms. The Foundation has no power to prevent this.

## Conflict resolution

For technical disputes that cannot be resolved by OIP vote alone:

1. **Mediation**: a panel of three respected technical contributors mediates.
2. **Time-boxed working group**: contested topics are delegated to a small working group with a deadline.
3. **Soft fork**: if disagreement persists, the mesh may temporarily support both alternatives until adoption data settles the question.

For ethical or legal disputes:

1. The Foundation's board reviews per its bylaws and published values.
2. External legal counsel as needed.
3. Public statement of resolution and rationale.

## Transparency commitments

- **Annual audited financial report** published by the Foundation.
- **OIP archive** publicly accessible, including rejected and withdrawn proposals.
- **Security advisory disclosure** following coordinated-disclosure best practices.
- **Board meeting summaries** published quarterly (without sensitive details).

## Open governance questions

Resolved by [`OIP-Process-001`](../oips/oip-process-001.md) on 2026-05-10:

- ~~**Specific OIP voting threshold formulas**~~ — quadratic-vote weight formula bootstrapped in
  `OIP-Process-001` §5.2 with a tunable, deferred Process OIP for the production-grade formula.
  Quorum (§5.3) and supermajority for Layer 1 changes (§5.3) are fixed.
- ~~**BDFL veto window dates**~~ — start `2026-05-09`, sunset `2031-05-09` (immutable),
  documented in three independent places: this file, `OIP-Process-001` §5.4, and the first
  commit on `main` (`61426d5`).
- ~~**OIP categories and lifecycle**~~ — formalized in `OIP-Process-001` §1 and §4.

Still open, pending Foundation bylaws (see [`08-funding-policy.md`](08-funding-policy.md) and
roadmap Phase 0 closure for `P4.1`):

- **Founder succession plan if cySalazar steps down in years 1–5**: bylaws specify board elects
  an interim Lead Architect from active maintainers, confirmed by OIP. Specific procedure to
  be detailed in Foundation bylaws.
- **Trustee selection for years 4+**: process for transitioning from founder-appointed to
  community-elected trustees.
- **Legal jurisdiction handling**: when laws of NL conflict with mission (e.g., hypothetical
  EU mandate to insert backdoors), explicit Foundation policy of public refusal + relocation
  if necessary.
