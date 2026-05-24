# Commercial License — OMNI OS

> **Status:** PLACEHOLDER (non-binding) — this template will become a binding offer
> only after Stichting OMNI is legally constituted in the Netherlands (per
> [`/docs/05-governance.md`](docs/05-governance.md) Layer 3 and
> [`/docs/08-funding-policy.md`](docs/08-funding-policy.md)).
>
> Until then, all distribution of OMNI OS is governed exclusively by the
> [Apache-2.0](LICENSE) license.

---

## 1. Why this file exists

OMNI OS is licensed under the **Apache License, Version 2.0** ([LICENSE](LICENSE)).
This permissive license allows any user, contributor, or downstream project
to use, modify, and redistribute OMNI OS — including in proprietary products —
with no copyleft obligation.

This document describes the **commercial support and certification program**
that Stichting OMNI will offer. Unlike a dual-license model, the commercial
offering does not grant additional license rights (Apache-2.0 already grants
them all). Instead, it provides:

- Priority security advisories and SLA-backed incident response.
- Certified builds with reproducible attestation.
- Trademark licensing for "OMNI OS Certified" branding.
- Professional support and consulting.

The commercial program exists to fund the project sustainably while keeping
the codebase fully open under Apache-2.0.

## 2. Licensor (when constituted)

- **Legal entity:** Stichting OMNI (a Dutch foundation, not yet incorporated)
- **Jurisdiction:** Netherlands
- **Registration:** KVK number — `<TBD: pending notary registration>`
- **Authorized signatory:** the Stichting OMNI board (5 trustees), acting via
  majority resolution per the bylaws.

Until Stichting OMNI exists, **no party is authorized to grant a commercial
license on behalf of the project.** Inquiries received before incorporation
will be acknowledged but not contracted.

## 3. Scope of the commercial program

A commercial agreement, when offered, will grant the subscriber the following
for the agreed term:

- Priority security advisories (synchronized with public disclosure
  per [`SECURITY.md`](SECURITY.md), not ahead of it).
- SLA-backed incident response (severity-tiered, per the agreement).
- Access to certified, reproducibly-built OMNI OS images with TEE attestation.
- Right to use the "OMNI OS Certified" trademark on compliant deployments.
- Professional support and consulting from the core team.

It will **not** grant:

- Any license rights beyond what Apache-2.0 already provides (it provides all).
- Exclusive use of any OMNI OS component or API.
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
  4. What commercial support or certification needs the use case has.

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
