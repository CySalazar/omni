# OMNI OS — Funding Pitch Deck (Draft)

**Status:** Draft v0.1 — for refinement before first grant submission
**Audience:** mission-aligned nonprofits and corporate sponsors
**Format:** markdown source. Renderable to PDF / PPTX via Marp, Pandoc, or
similar. One slide per `---` section.

---

## Slide 1 — Title

# OMNI OS
### An AI-native, privacy-first, decentralized operating system.

cySalazar — Lead Architect
2026

---

## Slide 2 — The problem

**AI is reshaping computing. The current trajectory hands user data to a
handful of centralized providers.**

- Every modern AI assistant ships user prompts, files, and behavior to a
  remote provider.
- Privacy-preserving alternatives (local LLMs) are limited to <10B-parameter
  models or require expensive single-user hardware.
- There is no operating system today that treats AI as a system primitive
  with privacy as a structural property.

> **The next operating system will be AI-shaped. The question is who controls
> the data.**

---

## Slide 3 — The OMNI OS thesis

OMNI OS is built on four committed principles:

1. **Local-first.** Default behavior is local computation; remote is opt-in.
2. **Privacy by construction.** Cryptographic enforcement, not policy.
3. **Decentralization as a means.** Not as an end — to achieve privacy and
   anti-capture.
4. **Hardware-rooted security.** TEE attestation is mandatory for mesh participation.

A user can opt to use a 100B+ parameter model **without** any data leaving
their network of trusted devices — pooled compute across the user's own
devices and a P2P mesh of other OMNI OS instances.

---

## Slide 4 — Architecture in one slide

```
┌────────────────────────────────────────────────────┐
│ Applications / Agents (sandboxed)                  │
├────────────────────────────────────────────────────┤
│ SDK / Agent framework / Shell                      │
├────────────────────────────────────────────────────┤
│ AI Runtime │ Mesh │ Tokenization │ Filesystem │ ...│
├────────────────────────────────────────────────────┤
│ Rust microkernel — capabilities, IPC, scheduling   │
├────────────────────────────────────────────────────┤
│ Tensor HAL │ Network HAL │ Storage HAL │ TEE HAL   │
├────────────────────────────────────────────────────┤
│ Hardware: CPU + GPU/NPU + TEE + Secure storage     │
└────────────────────────────────────────────────────┘
```

**Execution tiers (selected per workload):**

`Local-only` → `Personal Cluster (LAN)` → `Federated Mesh (P2P)` → `Cloud (last resort, opt-in)`

---

## Slide 5 — Privacy guarantees

Five privacy primitives, enforced at protocol level:

1. **Encrypted-by-default data types** at OS API level (`EncryptedString`,
   `MaskedSSN`, `TokenizedEmail`, …).
2. **Tokenization service** runs inside the user's TEE — replaces PII with
   deterministic tokens before any inference call.
3. **Format-preserving encryption** for routing metadata.
4. **Compliance proofs** (signature + STARK) per mesh payload.
5. **TEE-only decryption envelopes** — session keys are sealed to a specific
   attested TEE measurement.

**Mathematical guarantee, not policy promise.** A malicious mesh node literally
cannot produce valid traffic that violates these properties.

---

## Slide 6 — Why this is technically feasible now (and was not 5 years ago)

| Year | Enabler |
|---|---|
| 2023 | Intel TDX and AMD SEV-SNP ship in mainstream server CPUs. |
| 2023 | Mixture-of-Experts (Mixtral, DBRX) shows that <2B active parameters/token can match 70B dense quality — making distributed inference practical. |
| 2024 | Rust's `no_std` ecosystem matures: full crypto stack works on bare metal. |
| 2024 | Transparent STARK provers (`winterfell`, `triton-vm`) reach production. |
| 2025 | Apple Silicon Private Cloud Compute proves attestation-bound TEE inference is operationally viable at scale. |

OMNI OS is the synthesis of these primitives into a single coherent OS, with
governance designed to prevent capture.

---

## Slide 7 — Roadmap (high-level)

| Phase | Timeline | Deliverable |
|---|---|---|
| **0** | months 0–6 | Foundation, funding, team, repository |
| **1** | months 6–18 | Microkernel proof-of-concept on TEE-enabled x86_64 |
| **2** | months 18–30 | Local-only AI inference (Tier 0) operational |
| **3** | months 30–36 | Personal Cluster (Tier 1) operational |
| **4** | months 36–48 | Federated Mesh v1.0 release (inference-only) |
| **5** | year 4 | Apple Silicon support; v1.1 |
| **6** | year 4–5+ | Federated training v2.0 |

Generational horizon: 25+ years. Stability of design before speed of delivery.

---

## Slide 8 — Governance: three-layer, anti-capture

| Layer | What | Authority |
|---|---|---|
| 1 — Protocol | Cryptographic rules (cipher suites, compliance proofs, privacy primitives) | Self-enforcing. No human authority can override at runtime. |
| 2 — Specification | Protocol evolution via OIPs | Community-federated. Anti-Sybil via TEE attestation. Quadratic voting. |
| 3 — Operational | Stichting OMNI (NL): codebase, seed nodes, partnerships, funding | Five trustees, three-year terms, BDFL veto sunsets after 5 years. |

**Stop conditions** documented publicly: if the Foundation is captured, the
protocol's cryptographic guarantees still hold; the codebase can be forked
and re-rooted under new stewardship.

---

## Slide 9 — Why a foundation, not a company

- **Anti-capture by structure.** A for-profit company would face fiduciary
  pressure to monetize user data — incompatible with the mission.
- **AGPL-3.0 default + commercial dual-licensing** lets the Foundation fund
  itself from organizations that need closed-source use, without selling out
  the public release.
- **ANBI-aligned (NL).** Tax-deductible donations once status is granted.
- **No exit event by design.** The Foundation cannot be sold.

Stichting OMNI is constituted in the **Netherlands** for strong privacy laws,
EU jurisdiction, and a mature stichting framework.

---

## Slide 10 — Funding ask

**Phase 0 ask (now): EUR 350,000 for 6 months runway.**

Allocation:

- Lead Architect (founder) — 4 months full-time runway: EUR 60,000.
- 2 senior Rust engineers — 4 months at EUR 120,000 annualized each: EUR 80,000.
- 1 cryptographer (peer review, P3.2) — engagement: EUR 15,000.
- Stichting setup, legal, insurance: EUR 15,000.
- Reserve fund (≥ 6 months operating): EUR 60,000.
- External security audit budget (Phase 1 closure): EUR 80,000.
- Infrastructure & travel: EUR 25,000.
- Contingency: EUR 15,000.

**Phase 1 ask (post month 6):** EUR 1.2M for months 6–18 (kernel
implementation + first security audit). Detail in a follow-up brief.

---

## Slide 11 — Aligned funders we are approaching

| Funder | Role | Status |
|---|---|---|
| **NLnet** | Phase 0 / Phase 1 grants | Application drafted; awaits Stichting constitution |
| **Mozilla MOSS** | Phase 1 grant | Drafted |
| **Sloan Foundation** (open-source) | Phase 1 grant | Drafted |
| **Open Philanthropy** (long-term safety) | Phase 1/2 grant | Drafted |
| **Aligned corporate sponsors** | Tiered sponsorship | Materials drafted (Proton, Mullvad, Element, System76, Framework, Purism) |
| **Community donations** | Ongoing | OpenCollective post-Stichting |

**Explicitly excluded:** governments and government-aligned funds (incompatible
with anti-regulatory-capture principle). See [`docs/08-funding-policy.md`](../08-funding-policy.md).

---

## Slide 12 — The founder

**cySalazar** — Lead Architect, founder.

- *(real-identity disclosure available on request under NDA; the public-facing
  identity is the project pseudonym per project identity policy)*
- Background: cybersecurity, AI, programming languages.
- Time commitment: full-time post-Phase-0 funding.
- Conflict-of-interest disclosure: no holdings in Excluded Sources; no board
  positions creating conflicts with the OMNI mission.

The project is intentionally single-founder for Phase 0. Three additional
hires close before Phase 1 starts (2 Rust engineers + 1 cryptographer).

---

## Slide 13 — What success looks like

- **v1.0 ships** (year 4) with inference-only operational on x86_64 TEE hardware.
- **10,000+ TEE-attested mesh nodes** within 12 months of v1.0 — proving
  decentralization at meaningful scale.
- **Independent security audit** at Phase 1 and Phase 4 closures, both publicly
  published.
- **Foundation financial independence** by year 5 (self-sustaining via
  mesh fees + commercial licensing).
- **25-year horizon:** OMNI OS as a credible, long-lived alternative for users
  who refuse to accept centralized AI as the default.

---

## Slide 14 — Risks (we are honest about them)

| Risk | Mitigation |
|---|---|
| TEE vendor compromise | TEE diversity across mesh (Intel + AMD + Apple); attestation deny-list per OIP. |
| Mainstream adoption slow | Project is funded for slow growth; success is measured in users, not unicorn valuation. |
| Funding shortfall | Reserve fund mandatory; explicit stop conditions per roadmap; AGPL ensures forkability. |
| Regulatory hostility (e.g., backdoor mandates) | Bylaws require public refusal + jurisdictional relocation if needed. |
| Founder bus-factor | Hiring plan close-of-Phase-0; founder veto sunsets at year 5; BDFL log makes decisions auditable. |
| MoE distribution proves infeasible | v0.1 design allows fallback to pipeline parallelism for dense models on personal cluster. |

---

## Slide 15 — Contact and next steps

**Repository:** https://github.com/CySalazar/omni
**License:** AGPL-3.0 (dual: commercial via Stichting OMNI)
**Docs:** in repo `/docs/`

**Next steps:**

1. Read [`docs/01-vision.md`](../01-vision.md) and [`docs/02-architecture.md`](../02-architecture.md).
2. Request a 30-min call to discuss alignment and funding fit.
3. Founder responds within 5 business days.

**Contact:** `cySalazar@cySalazar.com` (pseudonym; will be transitioned to
`hello@omni-os.org` once Stichting OMNI is constituted).
