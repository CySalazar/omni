# OMNI Improvement Proposals (OIPs)

> **Status:** Bootstrap (interim editor: founder; second editor seat vacant until Phase 1 hire — see `OIP-Process-001` §6).
> **Process spec:** [`oip-process-001.md`](./oip-process-001.md) (Active, ratified by BDFL fiat under Bootstrap clause; first formal vote deferred to the first non-Meta OIP).
> **Template:** [`oip-template.md`](./oip-template.md) — copy this as `oip-<slug>-<NNN>.md` for new proposals.

---

## What is an OIP?

An **OMNI Improvement Proposal (OIP)** is the canonical, archived design document for any change
to OMNI OS that is non-trivial — protocol changes, governance changes, breaking API changes,
new TEE backends, new cryptographic primitives, etc. The OIP process is OMNI OS's **Layer 2**
governance mechanism (community-federated specification), as defined in
[`docs/05-governance.md`](../docs/05-governance.md).

OIPs are modeled after Bitcoin BIPs, Ethereum EIPs, Python PEPs, and IETF RFCs, with adaptations
specific to OMNI OS (TEE-attested anti-Sybil voting, BDFL veto sunset, cryptographic activation
thresholds).

---

## When you must file an OIP

Per `CONTRIBUTING.md` §9 and `OIP-Process-001` §3 (*Trigger Conditions*):

- Any **protocol-level** change (wire format, cipher suite, capability format, mesh handshake).
- Any **breaking API change** in a public crate.
- Any **governance change** (process, voting, BDFL, editor body, Stichting bylaws aspects
  delegated to OIPs).
- Any **new TEE backend** addition (because it expands the trust base).
- Any **new cryptographic primitive** in `omni-crypto` not on the v0.1 RFC list.

When in doubt, file a **draft OIP** and let the editors classify it. Filing has zero cost; not
filing and discovering the change should have been an OIP costs a forced revert.

---

## When you do **not** need an OIP

- Bug fixes that preserve external behavior.
- Documentation typos / clarifications.
- Internal refactoring with no public-API surface change.
- Test additions.
- CI tweaks that do not change merge requirements.

These go through ordinary PR flow described in `CONTRIBUTING.md`.

---

## Numbering

Authoritative spec: `OIP-Process-001` §8 (Numbering). Quick reference:

| Aspect | Convention |
|---|---|
| **Filename** | `oip-<slug>-<NNN>.md` — kebab-case slug, 3-digit zero-padded number |
| **Number `NNN`** | **Globally unique and monotonically increasing** across the entire registry (not per-track). Authors pick the next free integer at filing; editors reconcile placeholder collisions at the `Draft → Review` transition (§8.3) |
| **Slug** | 1–3 kebab-case **category hint** (e.g. `process`, `bounty`, `kernel`, `serde`). **NOT a secondary identifier** — cross-references MUST use the integer (§8.1, §8.2) |
| **Reserved** | `0000` is reserved for the template (`oip-0000-template.md`) |

Examples:
- `oip-process-001.md` — OIP #1, slug `process` (this registry's first proposal).
- `oip-bounty-002.md` — OIP #2, slug `bounty` (Process-track bug-bounty program).
- `oip-container-006.md` — OIP #6, slug `container` (OmniContainer micro-VM engine).
- `oip-helper-007.md` — OIP #7, slug `helper` (`omni-helper` daemon: autonomy levels + Impact Dashboard).
- `oip-snark-stark-NNN.md` — OIP #*NNN* (TBD), slug `snark-stark` (hypothetical future, see `todo.md` P3.3 — number will be allocated at filing).

> **Compatibility note:** older `todo.md` entries reference identifiers like `OIP-Voting-002`,
> `OIP-Bounty-001`, `OIP-Crypto-002`. These are **placeholder names** from a pre-OIP-Process-001
> period; the actual numbers will be assigned globally when each OIP is filed, and the placeholders
> in `todo.md` will be reconciled at that time.

---

## Lifecycle

States, in order, with allowed transitions:

```
                    ┌──────────────────► Withdrawn (author abandons)
                    │
   Draft ──► Review ──► Last Call ──► Active ──► Final
                    │              │           │
                    └──► Rejected  └► Withdrawn└► Superseded
                                                 (by another OIP)
```

| State | Meaning |
|---|---|
| **Draft** | Author iterating; no editorial review yet |
| **Review** | Submitted to editors; community discussion open |
| **Last Call** | Editors propose merging; ≥14-day public objection window |
| **Active** | Merged into the registry; for `Standards Track` this enables the **activation phase** (≥75% nodes for ≥30 days) |
| **Final** | Activated and stable; the canonical reference for that decision |
| **Rejected** | Editors / vote concluded against; archived for the record |
| **Withdrawn** | Author or editors withdrew before Final; archived |
| **Superseded** | Replaced by a later OIP; older OIP retains historical authority |

Full state machine and transition rules: `OIP-Process-001` §4 (*Lifecycle*).

---

## Categories

| Category | Use for | Voting requirement |
|---|---|---|
| **Standards Track** | Wire formats, crypto primitives, capability formats, kernel interfaces, mesh protocol | Quadratic-vote majority + activation threshold |
| **Process** | OIP procedure changes, editor rotation, voting parameters, contribution flow | Quadratic-vote majority |
| **Informational** | Best practices, advisories, guidelines (non-binding) | Editor approval only |
| **Meta** | OIPs that govern the OIP process itself (`OIP-Process-001` is Meta) | Quadratic-vote majority + BDFL non-veto |

---

## Index of OIPs

| # | Track | Title | Status | Authors | Created |
|---|---|---|---|---|---|
| 0000 | Meta | Template (reserved) | — | — | 2026-05-10 |
| 001 | Meta | The OIP Process | Active *(Bootstrap)* | cySalazar | 2026-05-10 |
| 002 | Process | Bug Bounty Program for OMNI OS | Last Call *(closes 2026-05-26)* | cySalazar | 2026-05-10 |
| 002 | Standards Track | Compliance Proof Scheme — STARK over SNARK for v1 | Draft | cySalazar | 2026-05-10 |
| 003 | Standards Track | UEFI Bootloader Selection and Kernel `no_std` Transition Plan | Last Call *(closes 2026-05-17)* | cySalazar | 2026-05-15 |
| 004 | Standards Track | Migrate workspace serialization from bincode v2 (unmaintained) to postcard | Last Call *(closes 2026-05-26)* | cySalazar | 2026-05-12 |
| 005 | Standards Track | Boot hand-off ABI and kernel-runner crate (gate K4 of OIP-Kernel-003) | Review | cySalazar | 2026-05-12 |
| 005 | Process | Voting weight formula — non-saturating uptime, contribution signals, conflict-of-interest guards | Draft | cySalazar | 2026-05-12 |
| 006 | Standards Track | OmniContainer — native container engine with Linux/Windows compatibility | Draft | cySalazar | 2026-05-12 |
| 007 | Standards Track | OMNI Helper — Agentic Need-Detection, Autonomy Levels, and Impact Dashboard | Draft | cySalazar | 2026-05-12 |
| 008 | Standards Track | `omni-pkg` — Content-Addressed Federated Package Manager | Draft | cySalazar | 2026-05-12 |
| 009 | Standards Track | `omni-forge` — On-Demand Rust → WASM/ELF Generation Pipeline | Draft | cySalazar | 2026-05-12 |
| 010 | Standards Track | `omni-market` — Stichting-Curated Marketplace + Continuous CVE Re-Scan | Draft | cySalazar | 2026-05-12 |
| 011 | Standards Track | Omni\* Flagship Apps Program + OmniCode v1 (Phased Delivery) | Draft | cySalazar | 2026-05-12 |
| 012 | Standards Track | Kernel panic handler and global allocator (gate K3 of OIP-Kernel-003) | Review | cySalazar | 2026-05-12 |
| 013 | Standards Track | User-space driver framework — capabilities, MMIO, DMA/IOMMU, IRQ routing, manifest | Draft | cySalazar | 2026-05-20 |

> **Note on duplicate trailing numbers (history):** `OIP-Bounty-002` / `OIP-Crypto-002`, `OIP-Serde-004` / `OIP-Kernel-004` (was), and `OIP-Kernel-005` / `OIP-Voting-005` shared trailing numbers as placeholder collisions at `Draft` stage. Per `OIP-Process-001` §8.3, placeholder collisions in `Draft` are explicitly permitted and reconciled by the editors at the `Draft → Review` transition: the first of a colliding pair to reach `Review` retains its placeholder integer; the other is renumbered to the next free integer in the same PR that opens its own `Review` window. Current state: `OIP-Bounty-002` and `OIP-Serde-004` are canonical (Last Call closes 2026-05-26). `OIP-Kernel-005` reached `Review` first within its collision pair, retaining `005`; `OIP-Voting-005` (still `Draft`) will be renumbered when it reaches `Review`. `OIP-Kernel-004` was renumbered to **`OIP-Kernel-012`** at its `Draft → Review` transition (2026-05-14) since `OIP-Serde-004` was already canonical. `OIP-Crypto-002` (still `Draft`) will be renumbered when it reaches `Review`.

---

## Filing a new OIP

1. **Read** `OIP-Process-001` §3 (*Trigger Conditions*) to confirm an OIP is required.
2. **Open a discussion issue** using the
   [`oip_proposal.yml`](../.github/ISSUE_TEMPLATE/oip_proposal.yml) issue template. Editors will
   pre-validate scope.
3. **Branch** as `oip/<slug>` (per `CONTRIBUTING.md` §6).
4. **Copy** [`oip-template.md`](./oip-template.md) → `oip-<slug>-<NNN>.md`. Per
   `OIP-Process-001` §8.3, pick the next free integer at filing (or any free integer if
   filing in parallel with another `Draft`). Editors reconcile placeholder collisions at
   the `Draft → Review` transition — the first colliding OIP to reach `Review` retains its
   integer; the other is renumbered in the same PR that opens its `Review` window.
5. **Fill all required sections.** The lint at `scripts/lint-oips.py` will run in CI; fix any
   structural errors before requesting review.
6. **Open a PR** with a `Signed-off-by:` trailer (DCO) and Conventional Commit prefix
   `oip(<slug>): <title>`.
7. **Iterate** through `Draft → Review → Last Call`. The editors merge on positive Last Call
   outcome.

---

## Maintenance policy

- This file is **auto-validated** in CI (the OIP lint enforces that the index table mirrors the
  files on disk).
- A new OIP merge **must** include the corresponding row in the index table; the lint will fail
  otherwise.
- A status transition (e.g. `Active → Final`) is its own PR, with the rationale captured in the
  PR body.

---

## License

OIPs themselves are released into the public domain under
[CC0-1.0](https://creativecommons.org/publicdomain/zero/1.0/) (per `OIP-Process-001` §10) so they
can be quoted, mirrored, and cited freely. The codebase remains AGPL-3.0-only.
