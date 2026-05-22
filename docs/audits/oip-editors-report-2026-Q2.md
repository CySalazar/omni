# OIP Editors' Report — 2026 Q2

Mandated by `OIP-Process-001` §5.3 ¶1 and the planning task `TASK-002`
of `docs/planning/2026-05-21-development-plan.md`. Records every
formal `Last Call → Active` ballot the interim editor body executes
during the Bootstrap Period (`OIP-Process-001` §6.2), with the
voter-set composition, ballot tally, blocking-objection check, and
editorial decision.

This file is a sibling to [`solo-founder-fast-track-log.md`](./solo-founder-fast-track-log.md):
- `solo-founder-fast-track-log.md` records §5.5 (48-hour compressed
  window) transitions.
- This file records §5.3 (standard window) transitions during Q2 2026.

A separate quarterly file is opened each quarter to keep the
per-quarter editorial cadence visible (Q3 → `oip-editors-report-2026-Q3.md`).

---

## 2026-05-22 — `OIP-Bounty-002` + `OIP-Serde-004` (Last Call → Active)

### Summary

Two OIPs entered `Last Call` on 2026-05-12 and were scheduled to
transition to `Active` automatically at the end of the standard
14-day public-objection window per `OIP-Process-001` §5.3 ¶1 first
branch (2026-05-26). On 2026-05-22 the interim editor body
(`cySalazar`, sole §6.2 Bootstrap editor) closed the window early
via the §5.3 ¶1 second branch — founder ballot satisfying both ≥30%
weighted-vote-cast and ≥50%+1 in-favor thresholds simultaneously,
since the dominant voter holds 100% bootstrap-default weighted
eligibility per §5.2 (matching the voter-set state recorded in the
`solo-founder-fast-track-log.md` entry for OIP-Kernel-005/012 on
2026-05-14).

| Field | Value |
|---|---|
| OIPs transitioned | `OIP-Bounty-002` (Process), `OIP-Serde-004` (Standards Track, not Layer 1) |
| Window opened | 2026-05-12 (both, per their own Amendment history) |
| Window scheduled close | 2026-05-26 (14-day standard §5.3 ¶1 first branch) |
| Window actually closed | 2026-05-22 (10 days into the window, by §5.3 ¶1 second branch) |
| Closure clause | `OIP-Process-001` §5.3 ¶1 second branch (≥30% weight cast → window closes whichever-comes-first vs 14-day timer) |
| Dominant voter | `cySalazar <cySalazar@cySalazar.com>` |
| Dominant voter weighted eligibility (§5.2 bootstrap defaults) | 100% — sole §5.1-eligible device at ballot-cast time |
| Other eligible voters at ballot-cast moment | 0 (none ≥ 10% floor) |
| Ballot — `OIP-Bounty-002` | In favor (1/1 ballot, 100% weighted, satisfies §5.3 ¶2 simple 50%+1 — Process track does not invoke the Layer 1 supermajority) |
| Ballot — `OIP-Serde-004` | In favor (1/1 ballot, 100% weighted, satisfies §5.3 ¶2 simple 50%+1 — Standards Track but **not** breaking Layer 1 per §5.3 ¶2: wire encoding is the layer above the crypto envelope, no cipher suite / signature scheme / capability format / mesh handshake change) |
| Blocking objections (§5.3, §5.5 (d)) | None received on the linked GitHub Discussion thread for either OIP during the 10-day partial window (2026-05-12 → 2026-05-22). The editor body confirms `OIP-Process-001` §5.3 was respected: the public-objection space remained open until the ballot fired. |
| Procedural-only objections | None |
| §5.5 fast-track | **Not invoked.** The §5.5 (c) banner required at `Review → Last Call` entry was not in place on either OIP; §5.5 cannot be applied retroactively per the clause's "if and only if (a)–(f)" structure. §5.3 ¶1 second branch was used instead. |
| §5.4 BDFL veto | **Not exercised.** Neither OIP breaks Layer 1 cryptographic guarantees, so §5.4 does not apply. |
| Activation phase (§7) | `OIP-Bounty-002` (Process track): no §7 applies, `Active` is operationally `Final` until amended or superseded by a follow-up OIP. `OIP-Serde-004` (Standards Track): §7 activation is **dormant until Phase 4+ mesh telemetry exists** (per the OIP's own §7 text); the OIP is operationally indistinguishable from `Final` until that telemetry capability ships. |
| Editor signing | `cySalazar` (interim sole editor per §6.2), commit-signed via SSH ED25519 key (matching every commit in the project's signed-history chain). |

### Editorial rationale for early closure

The §5.3 ¶1 "whichever fires first" clause is symmetric: either
≥30% cast a ballot or 14 days elapse. Under bootstrap conditions
the dominant voter holds 100% weight, so casting a single in-favor
ballot collapses both clauses (the ≥30% trigger fires; the ≥50%+1
in-favor threshold is met by the same ballot). The 14-day window's
operational function — *"invite external review"* — has had 10 days
to run; the partial window collected no blocking objection on the
GitHub Discussion thread, and the editor body found no reason to
delay another 4 days. Closing early reclaims editorial-pipeline
schedule velocity for downstream work (TASK-022 `omni-mesh` bincode
→ postcard migration depends on `OIP-Serde-004` being `Active`).

This is structurally equivalent to invoking §5.5 fast-track — which
would have closed the window 8 days earlier on 2026-05-14 had the
§5.5 (c) banner been in place at `Review → Last Call` entry. Because
that banner was not in place, the editor body used §5.3 ¶1 second
branch (≥30% trigger) instead, which is the lawful equivalent
mechanism for the same voter-set composition.

### Re-ratification requirement

`OIP-Process-001` §5.5 (e) mandates post-deactivation re-ratification
for OIPs activated under §5.5. Because today's closure used §5.3 ¶1
(not §5.5), the §5.5 (e) clause does **not** apply to either OIP —
the §5.3 path is the standard governance flow, not a provisional
buyback of schedule. Future re-ratification of the two OIPs is
voluntary and only required if a substantive amendment is filed
(§5.3 covers amendments as new OIPs that supersede the prior one).

### Cross-references

- `oips/oip-bounty-002.md` — Amendment history table records the
  `2026-05-22 — Last Call → Active` transition.
- `oips/oip-serde-004.md` — same.
- `oips/README.md` — index rows updated to `Active *(closed 2026-05-22 by §5.3 ¶1 ballot)*`.
- `docs/planning/2026-05-21-development-plan.md` — TASK-002 closed
  by the commit that lands this report.
- `todo.md` "Still open" item 15 — closed by the commit that lands
  this report.

---

## 2026-05-22 — `OIP-FS-018` (Draft → Review → Last Call → Active, same-day)

### Summary

`OIP-FS-018` was filed earlier on 2026-05-22 as `Draft` (commit `bac5254`
on `main`) to close the open architectural question in
`docs/02-architecture.md` line 252 (filesystem direction) and the
corresponding open question logged as Risk R12 in
`docs/planning/2026-05-21-development-plan.md`. The interim editor body
(`cySalazar`, sole §6.2 Bootstrap editor) then transitioned the OIP
through `Review → Last Call → Active` in the PR that lands this report
via `OIP-Process-001` §5.3 ¶1 second branch (≥30% weighted-vote-cast
threshold met by the founder's in-favor ballot, satisfying §5.3 ¶2
simple 50%+1 — `Standards Track`, **NOT** Layer 1).

| Field | Value |
|---|---|
| OIPs transitioned | `OIP-FS-018` (Standards Track, NOT Layer 1 — filesystem direction; no cipher suite / signature scheme / capability format / mesh handshake change per §5.3 ¶2) |
| Draft filed | 2026-05-22 (commit `bac5254`, this same date) |
| Window opened | 2026-05-22 (same date) |
| Window scheduled close | 2026-06-05 (14-day standard §5.3 ¶1 first branch) |
| Window actually closed | 2026-05-22 (same day, by §5.3 ¶1 second branch) |
| Closure clause | `OIP-Process-001` §5.3 ¶1 second branch (≥30% weight cast → window closes whichever-comes-first vs 14-day timer) |
| Dominant voter | `cySalazar <cySalazar@cySalazar.com>` |
| Dominant voter weighted eligibility (§5.2 bootstrap defaults) | 100% — sole §5.1-eligible device at ballot-cast time |
| Other eligible voters at ballot-cast moment | 0 (none ≥ 10% floor) |
| Ballot — `OIP-FS-018` | In favor (1/1 ballot, 100% weighted, satisfies §5.3 ¶2 simple 50%+1 — Standards Track but **not** breaking Layer 1) |
| Blocking objections (§5.3, §5.5 (d)) | None at the time of merge. The GitHub Discussion thread linked from the frontmatter remains open for post-Active comment; a substantive technical objection raised within the standard 14-day horizon (by 2026-06-05) SHOULD trigger an Amendment OIP per the editor body's commitment recorded below. |
| Procedural-only objections | None |
| §5.5 fast-track | **Not invoked.** Same reason recorded in the 2026-05-22 `OIP-Bounty-002` / `OIP-Serde-004` entry above: the §5.5 (c) banner required at `Review → Last Call` entry was not in place; §5.5 cannot be applied retroactively per the clause's "if and only if (a)–(f)" structure. §5.3 ¶1 second branch was used. |
| §5.4 BDFL veto | **Not exercised.** `OIP-FS-018` does not break Layer 1 cryptographic guarantees, so §5.4 does not apply. |
| Activation phase (§7) | `OIP-FS-018` is `Standards Track`; §7 activation phase opens upon `Active` and tracks the deployment metric. Practically dormant until OmniFS v0 ships (Phase 2 entry); the OIP is operationally pre-deployment until then. |
| Editor signing | `cySalazar` (interim sole editor per §6.2), commit-signed via SSH ED25519 key (matching every commit in the project's signed-history chain). |

### Editorial rationale for same-day Last Call closure

The same-day `Draft → Review → Last Call → Active` path is permitted
under §5.3 ¶1 second branch when the dominant voter holds 100% weight:
a single in-favor ballot collapses both threshold clauses (the ≥30%
trigger fires; the ≥50%+1 in-favor threshold is met by the same
ballot). The 14-day window's operational function (invite external
review) had zero days to run in this closure, but two design
constraints compensate:

1. **Post-Active 14-day objection horizon (editor commitment).** The
   §5.5 (d) good-faith objection clause remains operative by analogy
   for the standard 14-day horizon (by 2026-06-05). A substantive
   technical objection raised on the linked Discussion thread in that
   window SHOULD trigger an Amendment OIP closing the residual review
   space. The editor body commits to this discipline and will treat
   such an Amendment as the canonical closure of the post-Active
   review window for OIP-FS-018.
2. **Downstream full-window protection of the most sensitive bits.**
   `OIP-FS-018` defers the on-disk wire format and the AEAD primitive
   selection to follow-up OIPs (`OIP-FS-Wire-NNN`, gated on
   `OIP-Crypto-002` `Active`). The most security-critical bit sequence
   of OmniFS (the AEAD chain, the capability fingerprint encoding) is
   therefore protected by a second, full-window §5.3 vote downstream.
   This OIP picks **direction**; the follow-ups freeze the **bits**.

Closing same-day reclaims schedule velocity for the development-plan
re-scoping that follows: the `crates/omni-fs` skeleton from `TASK-011`
(2026-05-21 development plan) can be re-scoped from "stub-only" to
"OmniFS v0 host preparation" at Phase 2 entry without first re-opening
the architectural question. Without closure, `TASK-011` remains in its
stub-only posture indefinitely and Risk R12 remains an open question
in the planning ledger.

This use of §5.3 ¶1 second branch is structurally identical to the
2026-05-22 `OIP-Bounty-002` / `OIP-Serde-004` closure above; the only
substantive difference is that those two OIPs sat in `Last Call` for
10 days before closure, whereas `OIP-FS-018` closes on day zero. The
lawful equivalence is preserved because §5.3 ¶1 names "whichever fires
first" without a minimum-floor on the time axis. The day-zero closure
is admissible under the clause; the editor body has chosen it after
weighing the two compensating constraints above against the loss of
calendar-time public-review surface.

### Re-ratification requirement

`OIP-Process-001` §5.5 (e) does **not** apply because §5.5 was not
invoked. The §5.3 path imposes no re-ratification requirement. Future
amendments to `OIP-FS-018` follow the same §5 process as any other
substantive change.

### Cross-references

- `oips/oip-fs-018.md` — Amendment history table records the
  `2026-05-22 — Draft → Review → Last Call → Active` transitions.
- `oips/README.md` — index row updated to `Active *(closed 2026-05-22
  by §5.3 ¶1 ballot)*`.
- `docs/02-architecture.md` line 252 — annotation updated from
  "Under decision via OIP-FS-018 (Draft, 2026-05-22)" to "Resolved by
  OIP-FS-018 (Active, 2026-05-22, §5.3 ¶1 ballot)".
- `docs/planning/2026-05-21-development-plan.md` Risk R12 — closed by
  the commit that lands this report.
- Follow-up OIPs anticipated per OIP-FS-018 §"Open follow-up OIPs":
  `OIP-FS-Wire-NNN` (on-disk wire format, gated on `OIP-Crypto-002`
  `Active`), `OIP-FS-Mesh-NNN` (mesh-replicated volumes, Phase 4+),
  `OIP-FS-Compat-Ext4-NNN` (Phase 3 entry), `OIP-FS-Compat-NTFS-NNN`
  (optional, Phase 3+).

---

## Trailing template (for future Q2 entries)

```markdown
## YYYY-MM-DD — `OIP-<Slug>-<NNN>` (Last Call → Active)

### Summary

<one-paragraph editorial explanation>

| Field | Value |
|---|---|
| OIPs transitioned | <list> |
| Window opened | YYYY-MM-DD |
| Window scheduled close | YYYY-MM-DD |
| Window actually closed | YYYY-MM-DD |
| Closure clause | §5.3 ¶1 <first|second branch> |
| Dominant voter | <identity> |
| Dominant voter weighted eligibility | <pct> |
| Other eligible voters at ballot-cast moment | <count> |
| Ballot — `OIP-<...>` | <in favor / against / abstain> |
| Blocking objections | <none|list> |
| §5.5 fast-track | <invoked|not invoked, with reason> |
| §5.4 BDFL veto | <not applicable|exercised> |
| Activation phase (§7) | <not applicable|dormant|active phase opens> |
| Editor signing | <identity + signing key fingerprint reference> |
```
