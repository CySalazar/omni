# Governance

**Status:** Draft v0.1

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

1. **Proposal**: anyone can publish an OMNI Improvement Proposal (OIP) on the public OIP repository.
2. **Discussion**: public discussion via mailing list / forum (open, archived).
3. **Reference implementation**: required for any non-trivial proposal.
4. **Vote**: weighted by **proof-of-uptime + proof-of-contribution**, anti-Sybil via TEE attestation (1 unique device = 1 vote), quadratic voting to reduce concentration of power.
5. **Activation**: the new behavior runs in parallel with the old; activation triggers when ≥75% of active nodes have run the implementation for 30 consecutive days. Old behavior is deprecated when usage drops below a threshold.

OIP categories:

- **Standards Track** — protocol changes
- **Process** — governance changes
- **Informational** — guidelines, best practices, advisories

OIP states: `Draft` → `Review` → `Last Call` → `Final` / `Rejected` / `Withdrawn`.

#### Founder role (years 1–5)

For the first 5 years, the project founder (Matteo Sala) holds:

- **Lead Architect** title with technical leadership responsibility.
- **Soft veto** on protocol breaking changes: the founder can *block* a proposal but cannot *impose* one.

The veto sunsets at year 5 by Stichting bylaws. This is a codified, non-discretionary expiry.

#### After year 5

Founder retains an advisory role (no veto). All protocol decisions are made by the OIP process.

#### After year 10

Full transition to community-elected technical board. Trustees of Stichting OMNI are no longer founder-appointed; they are elected via the OIP process.

### Layer 3 — Operational (legal entity)

A legal entity sustains operations: codebase maintenance, seed node operation (initially), partnerships, legal response, funding allocation.

**Entity:** **Stichting OMNI** (Foundation, Netherlands).

#### Structure

- Board of 5 trustees, 3-year rotating mandates.
- Founder (Matteo Sala) on board for years 1–5 by initial appointment.
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

- **Implements the same protocol** → is fully interoperable on the mesh. The Foundation does not litigate. AGPL-3.0 obligations apply (modifications must be published).
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

- **Founder succession plan if Matteo steps down in years 1–5**: bylaws specify board elects an interim Lead Architect from active maintainers, confirmed by OIP. Specific procedure to be detailed in Foundation bylaws.
- **Trustee selection for years 4+**: process for transitioning from founder-appointed to community-elected trustees.
- **Legal jurisdiction handling**: when laws of NL conflict with mission (e.g., hypothetical EU mandate to insert backdoors), explicit Foundation policy of public refusal + relocation if necessary.
- **Specific OIP voting threshold formulas**: quadratic voting parameters, quorum requirements, super-majority thresholds for protocol breaking changes.

These will be addressed in OIP-Process-001 and Foundation bylaws prior to v1 release.
