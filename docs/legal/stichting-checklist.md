# Stichting OMNI — Constitution Checklist

**Status:** Operational checklist (Draft v0.1)
**Owner:** cySalazar (Founder)
**Timeline target:** 8–12 weeks from start to first KVK registration

This is a step-by-step execution plan for constituting **Stichting OMNI** in
the Netherlands. Each item is sized to a single founder operating remotely;
on-the-ground steps are flagged 🇳🇱 and require either travel or a Dutch
representative.

---

## Phase A — Preparation (weeks 1–2)

- [ ] **A.1** Review [`docs/legal/bylaws-draft.md`](bylaws-draft.md) end-to-end. Confirm Mission Anchor wording matches the project's strategic intent.
- [ ] **A.2** Identify a target Dutch notary specializing in stichtingen for tech / nonprofit. Shortlist:
  - Houthoff (large, English-speaking, expensive).
  - De Brauw Blackstone Westbroek (large, expensive, used to international clients).
  - **NautaDutilh** or **Allen & Overy Amsterdam** (mid-large).
  - **Boutique notarissen** specialized in stichtingen — e.g., Klijn Notariaat, Spier & Hazenberg.

  Selection criteria: experience with ANBI applications, English working language, fixed-fee quote, can handle remote signing via DocuSign-equivalent and Apostille.

- [ ] **A.3** Request fixed-fee quotes from three notaries. Typical range: EUR 1,500–4,000 for the deed itself plus EUR 500–1,500 for ancillary services. Document the quotes.
- [ ] **A.4** Identify five initial trustees (founder + four). Required:
  - **At least one Dutch resident** (regulatory requirement per bylaws Article 4.5).
  - **Diversity of expertise**: at least one with NL legal / corporate experience, at least one with technical credibility relevant to OMNI's mission, at least one with funding / nonprofit governance experience.
  - **No conflicts** with Excluded Sources per [`docs/08-funding-policy.md`](../08-funding-policy.md).
- [ ] **A.5** Open an Excel / Notion tracker for trustee documents needed (passport scan, BSN if NL-resident, signed consent to serve, conflicts disclosure).
- [ ] **A.6** Prepare a board resolution template for the foundational meeting (held immediately after deed execution).

## Phase B — Notary engagement (weeks 2–4)

- [ ] **B.1** Sign engagement letter with chosen notary. Communicate the bylaws draft and project context.
- [ ] **B.2** 🇳🇱 The notary translates the bylaws into Dutch as the authoritative *statuten*. Review the back-translation; flag any drift from intent.
- [ ] **B.3** Compile trustee identity documents and consent forms.
- [ ] **B.4** Notary drafts the *akte van oprichting* (deed of incorporation).

## Phase C — Deed and registration (weeks 4–6)

- [ ] **C.1** 🇳🇱 Founder signs the deed in front of the notary. Options:
  - In-person in the Netherlands (preferred).
  - Remote via Dutch notary's e-signing platform (where supported).
  - Through Power of Attorney to a Dutch representative.
- [ ] **C.2** Notary files the deed with the **Kamer van Koophandel (KVK)**.
- [ ] **C.3** KVK assigns a registration number (Kamer van Koophandel-nummer). Typical turnaround: 1–5 business days.
- [ ] **C.4** Notary registers the Foundation with the **Belastingdienst** (Dutch tax authority). RSIN (Rechtspersonen en Samenwerkingsverbanden Informatienummer) is assigned.
- [ ] **C.5** First board meeting held within 30 days. Resolutions:
  - Confirm trustees and roles.
  - Appoint Director (initially may be founder per bylaws Article 6.3).
  - Appoint auditor (provisional; final by year-end).
  - Open Foundation bank account.
- [ ] **C.6** Open a Dutch bank account. Options:
  - **ABN AMRO**, **ING**, **Rabobank** — traditional, slow, paperwork-heavy.
  - **Bunq** or **Triodos** — faster, more nonprofit-friendly, may have stricter KYC.
  - **bunq** for operational accounts plus a traditional bank for deposit / reserve fund.

## Phase D — ANBI status (weeks 6–12)

- [ ] **D.1** File ANBI application with Belastingdienst.
  - Requirements: bylaws + board resolutions + at least one year of activity plan + annual budget + standard ANBI questionnaire.
- [ ] **D.2** Publish the **ANBI-mandatory data** on the Foundation's website:
  - Name, RSIN, contact details.
  - Mission and goals.
  - Names of trustees (or "the board" if anonymity is deemed necessary; consult notary).
  - Compensation policy.
  - Annual reports and activity reports (placeholder until first year completes).
  - Financial statements.
- [ ] **D.3** Await Belastingdienst decision. Typical turnaround: 8–16 weeks.

## Phase E — Operational setup (in parallel from week 4)

- [ ] **E.1** Configure GitHub organization `omni-os` (or `CySalazar` → transfer at the appropriate moment). Founder retains ownership during Phase 0; transfers to Stichting once it exists.
- [ ] **E.2** Procure essential infrastructure:
  - Email domain `omni-os.org` (or similar). Mailbox provisioning unblocks `conduct@omni-os.org`, `security@omni-os.org`, `hello@omni-os.org`.
  - Web presence: minimal site explaining mission, link to GitHub.
  - PGP / SSH signing key for the Foundation entity (separate from founder personal keys).
- [ ] **E.3** Engage:
  - Accountant familiar with stichtingen.
  - Optional: external bookkeeper (Foundation may run pro-bono for year 1).
- [ ] **E.4** Adopt baseline policies:
  - Privacy policy (GDPR-compliant).
  - Cookie / analytics policy (likely "we don't use analytics").
  - Data retention policy.
  - Procurement policy.
  - Travel & expenses policy.
- [ ] **E.5** Insurance:
  - **Bestuurdersaansprakelijkheidsverzekering** (Directors and Officers liability — D&O) — strongly recommended.
  - General liability — usually low-cost.

## Phase F — Closure (week 12)

- [ ] **F.1** Update [`SECURITY.md`](../../SECURITY.md) with the real PGP fingerprint.
- [ ] **F.2** Update [`Cargo.toml`](../../Cargo.toml) workspace authors: transition from `cySalazar` pseudonym to `Stichting OMNI <hello@omni-os.org>`.
- [ ] **F.3** Update [`CODE_OF_CONDUCT.md`](../../CODE_OF_CONDUCT.md) enforcement contact to the real `conduct@omni-os.org` mailbox.
- [ ] **F.4** Update [`docs/05-governance.md`](../05-governance.md) and [`docs/legal/bylaws-draft.md`](bylaws-draft.md) with the real KVK number, RSIN, and ANBI confirmation status.
- [ ] **F.5** Publish "Phase 0 closure announcement" — blog post or release note. Cite this checklist as the audit trail.

---

## Estimated cost (founder-funded, reimbursable by Foundation)

| Item | Cost (EUR) |
|---|---|
| Notary engagement | 1,500–4,000 |
| Bank account opening | 0–250 |
| Domain + minimal hosting (year 1) | 50–200 |
| D&O insurance (year 1) | 800–2,500 |
| Accountant (year 1) | 1,500–5,000 |
| Travel (if in-person signing) | 500–1,500 |
| **Total Phase 0 outlay** | **4,350–13,450** |

Funding for Phase 0 outlay comes from founder personal funds (reimbursed at
Stichting constitution) OR a bridge grant. See [`docs/funding/pitch-deck.md`](../funding/pitch-deck.md)
"Phase 0 ask".

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Cannot identify NL-resident trustee | Medium | High | Reach out to NL-based privacy / open-source community (Bits of Freedom, NLnet network) for introductions. |
| Notary fee budget overrun | Low | Medium | Fixed-fee engagement letter, three quotes obtained. |
| ANBI rejection | Low–Medium | High (reduces donor tax benefits) | Anticipate; the Foundation can operate without ANBI but with reduced donor benefits. Document the rejection rationale and address it on re-application. |
| Bank refuses to open account (de-risking) | Medium | High | Have second-choice bank on standby; Triodos is generally friendly to mission-aligned nonprofits. |
| Trustees withdraw before signing | Medium | High | Maintain a reserve list of three additional candidates. |
| Dutch tax classification disputes | Low | Medium | Engage Dutch accountant from week 4 onward to navigate; do not rely on notary alone. |

## Outputs at Phase 0 closure

By Phase F completion, the project has:

- A legally constituted Stichting OMNI with KVK number, RSIN, ANBI application
  filed (decision pending or granted).
- Five trustees including the founder, with documented eligibility.
- Bank account, mailboxes, domain.
- Updated repository identifiers (authors field, contact addresses).
- A first annual activity plan ready for the foundation's first fiscal year.

This unblocks Phase 1 (per [`docs/06-roadmap.md`](../06-roadmap.md)): hiring,
substantive funding, and externally-audited engineering work.
