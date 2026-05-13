# Open Philanthropy — Grant Application Draft

**Target program:** *Potential Risks from Advanced AI* OR *Effective Altruism
Movement Building* — actual program TBD at submission time. Most natural fit
is the long-term AI safety / governance bucket.
**Status:** Draft v0.1 — for submission upon Stichting OMNI constitution.
**Drafted by:** cySalazar
**Last updated:** 2026-05-10

> **Note on positioning:** Open Philanthropy funds work that improves the
> *trajectory* of advanced AI. OMNI OS contributes by changing the default
> deployment substrate: privacy-preserving AI, anti-capture governance, and
> auditable cryptographic guarantees. The application emphasizes long-term
> structural impact, not short-term product metrics.

---

## 1. Project: OMNI OS

A from-scratch, AI-native, privacy-first, decentralized operating system.

## 2. Lead organization

Stichting OMNI (Netherlands, in formation).

## 3. Amount and duration

**USD 500,000 over 24 months.**

## 4. Theory of change

**Claim:** the operating-system layer is the most consequential intervention
point for shaping how mainstream AI is deployed over the next decade.

**Reasoning:**

- Higher-layer interventions (model selection, application design, browser
  extensions) are necessarily downstream of the OS-level assumption that
  "AI runs on someone else's computer."
- Lower-layer interventions (silicon, hypervisors) are too slow-moving and
  too consolidated for any non-incumbent to credibly influence.
- The OS layer is the highest leverage point where a small, mission-aligned
  team can credibly shift defaults — *if* the work is done with a
  generational horizon and structural anti-capture.

**OMNI OS's contribution to a better AI trajectory:**

1. **Reduces structural privacy risk** by making private AI the default for
   users on OMNI OS, rather than an exotic configuration requiring expert
   knowledge.
2. **Diversifies AI infrastructure** beyond a small set of hyperscalers,
   reducing systemic risk from any single provider's policy or outage.
3. **Establishes a precedent** for cryptographically-enforced, federated AI
   that other communities (governments, NGOs, academia) can adopt or fork.
4. **Demonstrates anti-capture governance** at the OS layer: a credible model
   for how foundational infrastructure can be operated as a public good.

## 5. Why this counterfactually matters

If OMNI OS does not exist:

- **Counterfactual A:** Big-tech-controlled "private AI" features become the
  default — privacy-preserving in name but with vendor lock-in and policy
  capture. Users who refuse them have no operating-system-level alternative.
- **Counterfactual B:** Privacy-preserving AI remains a niche of single-user
  setups (Llama.cpp on a laptop). Mainstream users continue using cloud AI
  by default because the OS layer makes anything else difficult.
- **Counterfactual C:** Governmental "sovereign AI" stacks emerge in Europe,
  India, etc., that solve some risks but introduce others (regulatory
  capture, lawful-intercept mandates).

OMNI OS is the only ongoing project the founder is aware of attempting
**operating-system-level intervention with anti-capture governance**.
Decentralized AI projects exist but at the model/training layer, not the
OS layer. Privacy-preserving OS efforts exist (Qubes OS, Tails) but do not
treat AI as a first-class kernel primitive.

## 6. What the grant funds

Open Philanthropy's USD 500,000 funds the gap between Phase 0 closure and
Phase 2 (local AI runtime, month 18–30). Specifically:

| Aim | Allocation | Outcome |
|---|---|---|
| **A1** — Microkernel implementation completion (P6) | USD 220,000 | Bare-metal Rust microkernel runs on Intel TDX hardware; capability-based syscall dispatch; reference shell. |
| **A2** — AI Runtime Service implementation (Phase 2) | USD 180,000 | Local-only inference operational; tensor HAL with CPU/GPU dispatch; encrypted-by-default data types in OS API. |
| **A3** — Tokenization service (Phase 2) | USD 50,000 | On-device NER classifier; per-user token vault; deterministic tokenization within session. |
| **A4** — Two external security audits (kernel + AI runtime) | USD 50,000 | Public audit reports; remediation budget. |

## 7. Counterfactual impact assessment

Without this grant, the project still proceeds — but slower:

- Phase 1 (kernel) stretches from 12 months to 18+ months.
- Phase 2 (AI Runtime) may be deferred to year 4 rather than year 3.
- External audits would be smaller-scope, reducing public credibility.

Open Philanthropy's funding shortens the timeline to v1.0 release by an
estimated **9–15 months** and substantially improves the audit quality.
Given the criticality of "early credible v1.0 release" for influencing the
trajectory of AI deployment, this acceleration has meaningful counterfactual
impact.

## 8. Long-term outlook

The Foundation's funding model (per [`docs/08-funding-policy.md`](../../08-funding-policy.md))
targets self-sustainability by year 5 via:

- Mesh fees (small percentage of compute credits, post-adoption).
- Commercial license fees (dual-licensing, AGPL + commercial via Stichting).
- Recurring corporate sponsorship from aligned vendors.

Open Philanthropy's grant funds the trajectory from Phase 0 closure (the
critical funding gap) through Phase 2 (proof of local AI Runtime). After
Phase 2, mainstream adoption becomes credible enough that diversified
funding streams open.

## 9. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Project becomes captured by funders | Low | Bylaws Mission Anchor (Article 3) requires unanimous board + 80% OIP supermajority + 6-month notice to modify; explicit Excluded Sources for governmental and government-aligned funding. |
| Project fails to ship v1.0 | Medium | Roadmap stop-conditions are public; Foundation publicly closes with reasoning rather than dying silently. AGPL ensures forkability. |
| External audit identifies non-remediable Critical issues | Low–Medium | Phase 1 audit pre-Phase-4 catches kernel issues early; Phase 4 audit pre-v1.0 catches mesh issues. Budget includes remediation allowance. |
| Wide adoption stalls; project remains niche | Medium | Niche but real adoption (10K nodes) is still meaningful for the OSS reference design and influence on competing efforts. |

## 10. Reporting commitments

- Quarterly progress updates (concise; 1 page).
- Annual financial accounting (audited per bylaws).
- All audit reports public within 90 days.
- All OIPs public from filing.
- Final report at month 24: aims delivered or waived with rationale,
  outputs link, counterfactual reflection.

## 11. About the founder

**cySalazar** — Lead Architect, founder.

The project is intentionally single-founder for Phase 0 to minimize
coordination overhead while design stabilizes. Two senior Rust engineers
and one cryptographer hire upon Phase 0 closure (funded by this grant
plus the MOSS + NLnet + Sloan applications).

The pseudonym is project policy until the BDFL veto sunsets at year 5;
real-identity disclosure to Open Philanthropy is acceptable under NDA.

## 12. Open Philanthropy specifics

If accepted, OMNI OS would:

- Acknowledge Open Philanthropy in repository README and annual transparency
  reports.
- Submit to Open Philanthropy's standard evaluation and reporting framework.
- Make all outputs public under AGPL-3.0 / CC-BY-SA 4.0.
- Comply with Open Philanthropy's conflict-of-interest and grant agreement
  templates without modification.

## 13. Why OMNI OS aligns with EA / x-risk concerns

The applicant does not claim that OMNI OS *prevents* AI x-risk directly. The
claim is more modest:

- **Diversification:** a federated, privacy-preserving OS substrate reduces
  the concentration of capability and data in a small number of providers.
  Diversification is a robust hedge regardless of one's view on
  capability-level x-risk.
- **Default-shaping:** the operating-system layer determines the default
  affordances available to applications. A privacy-preserving default
  affordance is structurally different from a privacy-preserving option a
  user must seek out.
- **Anti-capture governance precedent:** if OMNI OS succeeds, it provides a
  template for how foundational AI infrastructure can be governed as a
  public good. This template is portable to other layers (model weights,
  training compute, dataset stewardship).

This is a "shape the trajectory" argument, not a "solve alignment" argument.
The applicant is honest about this scope.

## Appendix — Links

- [`docs/01-vision.md`](../../01-vision.md)
- [`docs/06-roadmap.md`](../../06-roadmap.md)
- [`docs/05-governance.md`](../../05-governance.md)
- [`docs/legal/bylaws-draft.md`](../../legal/bylaws-draft.md)
- [`docs/funding/pitch-deck.md`](../pitch-deck.md)
- Repository: https://github.com/CySalazar/omni
