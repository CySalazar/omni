# Commercial License — OMNI OS

> **Status:** PLACEHOLDER (non-binding) — this template will become a binding offer
> only after Stichting OMNI is legally constituted in the Netherlands (per
> [`/docs/05-governance.md`](docs/05-governance.md) Layer 3 and
> [`/docs/08-funding-policy.md`](docs/08-funding-policy.md)).
>
> Until then, all distribution of OMNI OS is governed exclusively by the
> [AGPL-3.0-only](LICENSE) license.

---

## 1. Why this file exists

OMNI OS is dual-licensed by design:

1. **Open-source license — AGPL-3.0-only** ([LICENSE](LICENSE)).
   This is the primary license. Any user, contributor, or downstream project
   may use, modify, and redistribute OMNI OS under the AGPL terms, including
   the network-use copyleft clause (Section 13 AGPL-3.0).

2. **Commercial license — this document.**
   For organizations that cannot or will not comply with the AGPL's strong
   copyleft and network-use clauses (typical reasons: proprietary integrations,
   closed-source SaaS deployments, regulated industries with conflicting
   compliance requirements), Stichting OMNI will offer commercial terms that
   waive the AGPL obligations in exchange for a fee.

The dual-license model exists to fund the project while keeping the public
codebase strictly free software. It does not — and will not — modify the
AGPL-licensed codebase or restrict community use.

## 2. Licensor (when constituted)

- **Legal entity:** Stichting OMNI (a Dutch foundation, not yet incorporated)
- **Jurisdiction:** Netherlands
- **Registration:** KVK number — `<TBD: pending notary registration>`
- **Authorized signatory:** the Stichting OMNI board (5 trustees), acting via
  majority resolution per the bylaws.

Until Stichting OMNI exists, **no party is authorized to grant a commercial
license on behalf of the project.** Inquiries received before incorporation
will be acknowledged but not contracted.

## 3. Scope of the commercial license

A commercial license, when offered, will grant the licensee the following
rights for the agreed term:

- Use, modify, and distribute OMNI OS in proprietary or closed-source products.
- Operate OMNI OS as a hosted service (SaaS) without the Section 13 AGPL
  source-disclosure obligation toward users of that hosted service.
- Embed OMNI OS components in proprietary firmware or appliances.
- Receive priority security advisories (synchronized with public disclosure
  per [`SECURITY.md`](SECURITY.md), not ahead of it).

It will **not** grant:

- Trademark rights to "OMNI OS" or any associated marks (governed separately).
- The right to sublicense the open-source codebase under non-AGPL terms.
- Any waiver of contributors' copyright in their contributions
  (contributions remain governed by the project's contribution policy and
  the contributor's chosen license at the time of contribution).
- Any indemnification beyond what Stichting OMNI's bylaws permit.

## 4. Pricing model (indicative, non-binding)

The Stichting OMNI board will publish a tiered pricing model based on
licensee size and use case. Indicative tiers (subject to ratification):

| Tier | Profile | Indicative annual fee |
|---|---|---|
| Startup | < 50 employees, < €5M ARR | TBD |
| SMB | 50–500 employees | TBD |
| Enterprise | > 500 employees, or revenue > €100M | TBD |
| Sovereign / regulated | governments, defense, regulated finance | **Excluded** per Funding Policy |

**Sovereign and regulated-finance use is explicitly excluded** from
commercial licensing per [`/docs/08-funding-policy.md`](docs/08-funding-policy.md).
This boundary is non-negotiable and will be entrenched in the Stichting bylaws.

## 5. Excluded use cases (categorical, non-monetary)

Even with a paid commercial license, the following uses are forbidden:

- Mass surveillance infrastructure, whether state-operated or private.
- Predictive policing, social-scoring systems, or behavioral prediction
  systems aimed at populations rather than consenting individuals.
- Autonomous weapons systems (AWS) as defined by the Campaign to Stop
  Killer Robots, regardless of national-security framing.
- Systems whose primary purpose is to circumvent end-to-end encryption or
  to subvert TEE attestation.

Violation of these clauses voids the commercial license retroactively.

## 6. How to inquire (placeholder)

Until Stichting OMNI is operational:

- **Contact:** `cySalazar@cySalazar.com` (project founder, acting in
  personal capacity — no binding offer can be made)
- **Subject line prefix:** `[OMNI OS — Commercial License Inquiry]`
- **Required information:**
  1. Legal entity name and country of registration.
  2. Intended use case (one paragraph).
  3. Approximate scale (employees, revenue, OMNI OS deployment count).
  4. Why AGPL-3.0 obligations are not workable for the use case.

Inquiries are logged and triaged in a **read-only ledger** that will be
transferred to Stichting OMNI on incorporation, ensuring no licensee is
disadvantaged by the timing of their inquiry.

## 7. Effective date

This document becomes a binding offer on the date Stichting OMNI is
registered with the Dutch Chamber of Commerce (KVK) and ratifies its
commercial-licensing policy by board resolution. Until that date, this
file is informational only.

## 8. Change control

Material changes to this document — pricing model, excluded use cases,
licensor identity — require:

- A board resolution by Stichting OMNI (post-incorporation), and
- A 30-day public comment window referenced by an OIP (per
  [`/docs/05-governance.md`](docs/05-governance.md) Layer 2).

Editorial changes (typos, broken links, format) may be made by the
maintainer team without an OIP, with a changelog appended below.

---

## Changelog

- 2026-05-09 — Initial placeholder drafted by the founder. Non-binding.

---

*This file is part of OMNI OS and is governed by the project's documentation
policy. It is **not** legal advice. If your organization requires a binding
commercial license, contact the founder using the details in Section 6.*
