# Salary Bands (Public)

**Status:** Draft v0.1 — for public posting after Stichting OMNI constitution
**Last updated:** 2026-05-10
**Approval:** Board, at first hiring round (Phase 0 closure)

This document is the **public salary band table** for OMNI OS hires. It is
maintained per Stichting OMNI bylaws Article 9.3 (transparency) and is
referenced from each job description.

## Principles

1. **Public bands.** Salaries are published. Negotiation happens *within*
   a band, not outside it.
2. **Market-competitive but not extractive.** The Foundation is not a
   vehicle for above-market compensation, but recognizes that
   below-market pay would harm both the team and the mission.
3. **No equity.** Stichting OMNI is a foundation. There is no equity to
   grant; compensation is salary + standard benefits only.
4. **Geographic adjustment.** Bands are quoted in EUR for EU-resident hires.
   Non-EU EOR-employed hires are adjusted to local cost-of-living per a
   transparent formula (this document, Appendix A).
5. **Annual review.** Bands are reviewed annually and adjusted for
   inflation + market drift.

## Bands

| Band | Role profile | EUR FTE annualized (EU baseline) |
|---|---|---|
| **L1 — Engineer** | 0–2 years experience; not currently hiring | 55,000–75,000 |
| **L2 — Engineer** | 3–5 years experience | 70,000–95,000 |
| **L3 — Senior Engineer** | 5–8 years experience; deep specialism in one area; ships independently | **95,000–135,000** |
| **L4 — Principal Engineer** | 8+ years; sets technical direction for a sub-system; deep specialism in two+ areas | 125,000–170,000 |
| **L5 — Cryptography / Specialist** | premium band for cryptography and similarly specialist roles | **110,000–160,000** |
| **D — Director** | Foundation executive; ≥10 years operational experience; nonprofit / open-source background | 110,000–140,000 |

The bands the Foundation is actively hiring against in Phase 1:

- **L3 — Senior Rust Engineer (Kernel)**: 95,000–135,000.
- **L3 — Senior Rust Engineer (Networking + AI Runtime)**: 95,000–135,000.
- **L5 — Cryptographer**: 110,000–160,000.

## What "EUR FTE annualized" means

- Gross annual salary before income tax.
- Excludes employer-side taxes and contributions (which are added on top
  per NL employment law or EOR jurisdiction).
- Excludes the standard NL "vakantiegeld" (holiday allowance, ~8%) — that
  is added separately per NL law.

## Geographic adjustment (Appendix A)

For non-EU EOR-employed hires, the EU baseline is adjusted per:

```
local_band = eu_baseline × min(1.0, max(0.6, COL_local / COL_EU_baseline))
```

Where `COL_local` is the cost-of-living index for the candidate's resident
city (Numbeo cost-of-living index or equivalent) and `COL_EU_baseline` is
Amsterdam's index.

This means:

- A hire resident in **Amsterdam** receives the EU baseline (1.0×).
- A hire resident in a **lower-cost EU city** (e.g., Lisbon, Sofia) receives
  the baseline (we don't adjust *down* within the EU — the band itself
  is wide enough).
- A hire resident in a **higher-cost city** (e.g., London, NYC, SF) receives
  the baseline (we don't adjust *up* either — the wide band accommodates
  this within seniority negotiation).
- A hire in a **substantially-lower-cost country** (e.g., Argentina,
  Indonesia, India) receives a downward adjustment, floor 0.6× baseline.

The downward adjustment is *not* exploitation; it ensures the Foundation
can hire globally without distorting local labor markets or being seen as
a vehicle for arbitrage hires. Candidates can always negotiate within the
band; the geographic adjustment sets the band, not the offer within it.

## Benefits (in addition to salary)

- **NL-resident hires**: standard NL benefits (`vakantiegeld`, pension, 25
  days vacation, sick leave per NL labor law, health insurance subsidy,
  commuter allowance if applicable).
- **Non-NL hires via EOR**: benefits per EOR's local equivalent, calibrated
  to be substantively comparable to NL benefits.
- **Equipment**: laptop + monitor + keyboard + chair allowance; one-time
  EUR 2,000 home-office setup budget.
- **Conference budget**: EUR 2,500 per year per L3+ hire.
- **Sabbatical**: after 5 years of continuous service, a 4-week paid
  sabbatical with reset of vacation accrual.
- **Parental leave**: at least the more generous of NL law or the EOR's
  local minimum.

## What we do NOT offer

- **Equity / stock options**: not available (Foundation structure).
- **Bonuses tied to financial performance**: not available. The Foundation
  is not a for-profit; there is no profit to bonus on.
- **Long-vesting retention awards**: not available.

If those structures matter to you, the Foundation is probably not the right
employer. We are honest about this upfront.

## Annual review process

The board reviews salary bands annually at the first board meeting of each
fiscal year. The review uses:

- Public Rust salary surveys (Rust Foundation, JetBrains, Stack Overflow).
- Nonprofit-specific salary benchmarks (NL: Open Cultuur Data, EU: OSS
  funder reports).
- Inflation adjustment (Dutch CBS index for EU baseline; local CPI for
  EOR-employed staff).

Adjustments take effect at the start of the following fiscal year. Existing
employees receive the higher of (current salary) and (revised band midpoint
for their level).

## Disclosure

Each employee's actual salary within their band is **not** publicly disclosed
by the Foundation. The Foundation's annual transparency report discloses
**aggregate compensation** by level and **board / Director compensation
individually**.

## Footnote

Public salary bands are unusual for software engineering roles. The
Foundation publishes them because:

1. **Transparency is a mission value.** We cannot ask users to trust us with
   their data while obscuring how their donations are spent.
2. **Anti-bias.** Publishing bands reduces the negotiation-skill premium that
   disproportionately benefits men, native English speakers, and
   socially-confident candidates.
3. **Mission filtering.** Candidates who feel the bands are too low
   self-select out before applying. This saves everyone's time.
