# Stichting OMNI — Bylaws (Draft)

**Status:** Draft v0.1 — for Dutch notary review and translation
**Language:** English working draft. Authoritative version will be the **Dutch
notarial deed (`statuten`)** drafted by the appointed notary in the Netherlands.
**Last updated:** 2026-05-10

> **Disclaimer:** this is a working draft prepared by the founder. It is not
> legal advice. The final bylaws are drafted by a Dutch notary (`notaris`) and
> filed with the Kamer van Koophandel (KVK). This document captures the
> founder's intent so the notary can convert it into compliant Dutch legal text.

## Article 1 — Identity and seat

**1.1.** The foundation is named **Stichting OMNI** (the "Foundation").

**1.2.** The Foundation has its statutory seat in **Amsterdam, Noord-Holland,
the Netherlands**. The board may relocate the seat within the Netherlands by
unanimous decision.

**1.3.** The Foundation is constituted under Dutch law as a *stichting* per
Book 2, Title 6 of the Burgerlijk Wetboek (Dutch Civil Code).

**1.4.** The Foundation's fiscal year is the calendar year (1 January – 31 December).

## Article 2 — Mission

**2.1.** The mission of the Foundation is to develop, maintain, and steward
**OMNI OS**, an AI-native, privacy-first, decentralized operating system, and
to do so in the public interest under the principles defined in the **Mission
Anchor (Article 3)**.

**2.2.** The Foundation pursues this mission by:

- (a) developing and publishing OMNI OS under the AGPL-3.0 license;
- (b) operating the OMNI Improvement Proposal (OIP) process as specified in
      `OIP-Process-001`;
- (c) operating seed nodes for the federated mesh until community-operated
      nodes can replace them;
- (d) curating a blessed model registry of audited AI models;
- (e) commissioning and publishing independent security audits;
- (f) educating the public on privacy-preserving computing;
- (g) all other activities consistent with the Mission Anchor.

**2.3.** The Foundation pursues **Algemeen Nut Beogende Instelling (ANBI)** status
under Dutch tax law and shall conduct all its activities consistent with ANBI
requirements.

## Article 3 — Mission Anchor (irrevocable)

**3.1.** The following principles are the **Mission Anchor** of the Foundation
and are **immutable** for the lifetime of the Foundation:

- **(a) Local-first computing.** Default behavior of OMNI OS is local
  computation; remote computation is opt-in, transparent, and never the
  default.
- **(b) Privacy by construction.** Cryptographic enforcement is preferred over
  policy enforcement. The Foundation MUST NOT publish or fund a version of
  OMNI OS that weakens this principle.
- **(c) Anti-capture.** The Foundation MUST NOT accept funding from, or enter
  contractual relationships with, entities listed as Excluded Sources in
  [`docs/08-funding-policy.md`](../08-funding-policy.md), as that policy stands
  at the moment the funding or contract is offered.
- **(d) Open source.** Source code MUST remain under AGPL-3.0 (or a strictly
  more permissive open-source license adopted by OIP). Closed forks operated
  by the Foundation itself are prohibited.

**3.2.** Modification of the Mission Anchor requires:

- Unanimous decision of the board;
- AND a public 6-month notice and consultation period;
- AND ratification by an OIP passing under the procedure defined in
  `OIP-Process-001` §5.5 ("Supermajority for Mission Anchor changes" — 80% of
  weighted vote, quorum 60%).

If any of these conditions fails, the Mission Anchor remains unchanged. If a
foreign judicial order would force the Foundation to violate the Mission Anchor,
the board MUST refuse, publicly explain the refusal, and consider relocation of
the statutory seat per Article 1.2.

## Article 4 — Board

**4.1.** The Foundation is governed by a board of **five (5) trustees**.

**4.2.** Each trustee serves a **three (3) year term** with possible renewal up
to a maximum cumulative tenure of nine (9) years.

**4.3.** **Initial board composition (years 1–5):**

- The founder, **cySalazar**, is a trustee by initial appointment.
- The remaining four trustees are appointed by the founder in consultation with
  the project's first significant funders, subject to the eligibility
  requirements of Article 4.5.

**4.4.** **Board composition years 5–10:**

Trustees are elected via the OIP process. Founder retains an advisory seat
(no vote) for years 5–10. After year 10, the founder seat is dissolved.

**4.5.** **Eligibility:**

- At least one trustee MUST be a resident of the Netherlands at all times.
- No trustee may be an officer or employee of an Excluded Source (per
  [`docs/08-funding-policy.md`](../08-funding-policy.md)).
- No trustee may simultaneously serve on the board of another organization
  that competes commercially with OMNI OS in a way that creates a material
  conflict of interest.

**4.6.** **Compensation:**

- Trustees receive a nominal stipend (EUR 300–600 per board meeting attended)
  plus reasonable expense reimbursement.
- No trustee may receive a salary or consulting fee from the Foundation
  beyond the stipend.
- All compensation is disclosed in the annual transparency report.

**4.7.** **Removal:**

A trustee may be removed by:

- Unanimous decision of the remaining trustees (cause not required);
- OR by an OIP under `OIP-Process-001` §5.6 (a special "removal of trustee"
  procedure requiring 66% supermajority and 30-day public discussion).

## Article 5 — Founder role (years 1–5)

**5.1.** For the period **2026-05-09 to 2031-05-09**, the founder
(cySalazar) holds the title of **Lead Architect**.

**5.2.** The Lead Architect has:

- A **soft veto** on Standards-Track OIPs that would break Layer 1 protocol
  guarantees. The veto can be invoked at most once per OIP and is logged in
  [`docs/audits/bdfl-veto-log.md`](../audits/bdfl-veto-log.md).
- Final say on technical direction for the reference implementation, subject
  to OIP for substantive matters.
- A board seat for years 1–5.

**5.3.** The Lead Architect **cannot**:

- Modify the Mission Anchor unilaterally.
- Block a `Process`, `Informational`, or `Meta` OIP.
- Block a `Meta` OIP that narrows the Lead Architect's own authority.
- Bind the Foundation contractually without board approval beyond a EUR 5,000
  threshold per item.

**5.4.** Beyond the sunset (2031-05-09 23:59 UTC), the Lead Architect role
dissolves; the founder retains a non-voting advisory seat on the board for
years 5–10.

## Article 6 — Director

**6.1.** The board appoints a **Director** (executive officer) for day-to-day
operations. The Director need not be a trustee.

**6.2.** The Director:

- Reports to the board.
- Manages staff, contractors, and budget execution.
- Represents the Foundation externally for routine matters.
- Cannot bind the Foundation beyond limits set by board resolution.

**6.3.** The Director may be the same person as the Lead Architect during
years 1–3 of the Foundation's existence. From year 4 onward, the roles must
be held by different individuals (separation of executive and technical
roles).

## Article 7 — Decision-making

**7.1.** Routine board decisions require a simple majority (3 of 5).

**7.2.** Decisions affecting:

- The Mission Anchor (Article 3): see Article 3.2.
- Bylaw amendments other than Mission Anchor: 4 of 5 trustees AND OIP
  ratification.
- Dissolution (Article 13): 5 of 5 trustees AND OIP ratification.
- Acceptance of funding from a Borderline Source (per
  [`docs/08-funding-policy.md`](../08-funding-policy.md)): 4 of 5 trustees,
  publicly logged with rationale.
- Veto override (if Lead Architect veto is overruled): 5 of 5 trustees plus
  OIP supermajority of 75%.

**7.3.** Board meetings are quarterly minimum, with extraordinary meetings
called by any trustee with 7-day notice.

## Article 8 — Conflicts of interest

**8.1.** Trustees, the Director, and senior staff annually disclose:

- All material employment, board, and consulting relationships.
- Financial interests in Excluded Sources, Borderline Sources, or aligned
  corporate sponsors.
- Spousal and family financial interests where relevant.

**8.2.** A trustee or officer MUST recuse from any decision in which they have
a material interest.

**8.3.** All recusals are recorded in the board minutes and published in the
quarterly board summary.

## Article 9 — Funding

**9.1.** Funding policy is defined in [`docs/08-funding-policy.md`](../08-funding-policy.md).
That policy is incorporated by reference into these bylaws. Changes to the
funding policy require OIP ratification.

**9.2.** The Foundation MUST publish an annual audited financial report per
Article 9.3.

**9.3.** An independent auditor of recognized standing audits the Foundation's
finances annually. The auditor is appointed by the board and rotated at
least every five years.

## Article 10 — Intellectual property

**10.1.** All copyright in OMNI OS code held by the Foundation is licensed
to the public under **AGPL-3.0-only**. The Foundation may grant commercial
licenses on a transparent fee basis per the Dual Licensing policy in
[`docs/08-funding-policy.md`](../08-funding-policy.md).

**10.2.** Contributor IP is governed by the Developer Certificate of Origin
(DCO); the Foundation does NOT require a Contributor License Agreement (CLA).
This is deliberate: the Foundation seeks to be a steward, not an owner, of
contributor IP.

**10.3.** Trademarks ("OMNI OS", the logo) are held by the Foundation under
a public trademark policy permitting fair use, forks, and derivative works
that maintain protocol compatibility.

## Article 11 — Data

**11.1.** The Foundation MUST NOT collect personal data on OMNI OS users
beyond what is strictly required for the operation of seed nodes and OIP
participation, and never in a form that violates GDPR or the principles in
[`docs/04-security-model.md`](../04-security-model.md).

**11.2.** Any personal data collected is governed by a published Privacy
Policy that meets or exceeds GDPR requirements.

## Article 12 — Books and records

**12.1.** The Foundation maintains records of:

- Board meetings (minutes published quarterly, redacted as needed).
- Financial transactions.
- OIP archive (published in `/oips/`).
- Audit reports (published in `/docs/audits/`).
- Compliance with this bylaws and Dutch law.

**12.2.** Records are retained for at least seven (7) years.

## Article 13 — Dissolution

**13.1.** Dissolution requires:

- 5 of 5 trustees voting in favor;
- AND OIP ratification under `OIP-Process-001` with 66% supermajority and
  60% quorum;
- AND no objection from the Foundation's auditor regarding solvency.

**13.2.** Upon dissolution, residual assets MUST be transferred to one or
more organizations whose missions are substantially compatible with the
Mission Anchor (Article 3.1). Trustees, officers, employees, and contractors
of the dissolving Foundation MUST NOT receive any portion of the residual
assets beyond unpaid compensation lawfully due.

**13.3.** Source code repositories MUST be transferred (or mirrored) to a
custodian acceptable to the OIP community before dissolution closes, with
a transition plan published 6 months in advance.

## Article 14 — Governing law and disputes

**14.1.** These bylaws are governed by Dutch law.

**14.2.** Disputes between trustees concerning the bylaws are resolved by
mediation in the first instance; if mediation fails, by the competent court
in Amsterdam.

## Article 15 — Effective date and amendments

**15.1.** These bylaws become effective on the date of notarial deed
execution by the appointed Dutch notary.

**15.2.** Amendments to bylaws follow Article 7.2.

---

## Appendix A — Cross-references with project documents

| Concept | Authoritative document |
|---|---|
| Funding policy | [`docs/08-funding-policy.md`](../08-funding-policy.md) |
| OIP process | [`/oips/oip-process-001.md`](../../oips/oip-process-001.md) |
| Three-layer governance | [`docs/05-governance.md`](../05-governance.md) |
| Security model | [`docs/04-security-model.md`](../04-security-model.md) |
| BDFL veto log | [`docs/audits/bdfl-veto-log.md`](../audits/bdfl-veto-log.md) (created on first veto) |

## Appendix B — Items for the notary

These items are intentionally left open for the Dutch notary to fill in
during the notarial deed:

- KVK number (assigned upon registration).
- Specific Dutch civil-code references for ANBI status.
- Statutory-deed language matching `Boek 2, Titel 6 BW`.
- Names and BSN / passport details of the initial five trustees.
- Address of the statutory seat in Amsterdam.

## Appendix C — Translation note

Once the Dutch notarial deed is filed, the Dutch text is authoritative. This
English document is maintained for international stakeholders. In case of
conflict, the Dutch text prevails.
