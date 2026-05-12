# Alfred P. Sloan Foundation — Grant Application Draft

**Target program:** *Better Software for Science* OR *Technology* program
(evaluated at submission time per Sloan's current priorities).
**Status:** Draft v0.1 — for submission upon Stichting OMNI constitution.
**Drafted by:** cySalazar
**Last updated:** 2026-05-10

---

## 1. Project title

**OMNI OS — An AI-native, privacy-first, decentralized operating system.**

## 2. Principal investigator

**cySalazar** — Lead Architect, founder.
Affiliation: Stichting OMNI (Netherlands, in formation; KVK target Q3 2026).

## 3. Amount requested and duration

**USD 300,000 over 18 months.**

## 4. Project summary (one paragraph)

OMNI OS is a from-scratch Rust microkernel-based operating system that
embeds AI as a first-class kernel primitive while enforcing privacy
cryptographically. Users can leverage 100B+ parameter models by combining
on-device compute, LAN-pooled compute across their own devices, and opt-in
P2P mesh with other OMNI OS instances. The system roots in hardware TEEs
(Intel TDX, AMD SEV-SNP), uses Mixture-of-Experts model architecture for
natural fragmentation, and is governed by a Dutch foundation with explicit
anti-capture mandate. Sloan support would fund the technical core of Phase
1 (microkernel + AI runtime) plus the first external security audit.

## 5. Statement of significance

Mainstream AI today is delivered through a small number of US-headquartered
cloud providers. Every user prompt, every uploaded document, every
behavioral signal flows to those providers' infrastructure. Privacy-
preserving alternatives are limited to small models that fit on single-user
hardware, producing materially degraded quality versus the cloud-hosted
state-of-the-art.

This concentration creates three structural risks:

1. **Privacy:** users surrender personal information they would not surrender
   to a human assistant. Existing OS-level abstractions provide no boundary.
2. **Single-points-of-failure:** outages, censorship, or policy changes by
   any cloud provider affect all of its users simultaneously.
3. **Geopolitical dependency:** EU users depend on non-EU providers for
   mainstream AI; non-EU users depend equally on a small set of providers
   in any case.

OMNI OS addresses the root cause by reorganizing the OS layer around the
opposite assumption: AI is a kernel primitive with cryptographic privacy
guarantees, and remote AI is opt-in by exception. Practical use of state-
of-the-art models is preserved via the P2P mesh, where MoE expert
distribution prevents any single node from seeing complete prompt context.

The project is significant because it is the only effort attempting this
synthesis at the OS layer with anti-capture governance. Decentralized
networking projects (Tor, I2P) and decentralized application platforms
(SSB, Matrix) intervene at higher layers; OMNI OS intervenes at the OS
substrate, which is where the assumption "AI runs elsewhere" is
structurally embedded.

## 6. Specific aims

The 18-month grant supports four specific aims:

**Aim 1 — Microkernel implementation (P6 in project todo).** Transition
`omni-kernel` from a Rust library scaffold to a bare-metal `no_std` binary
running on Intel TDX-capable hardware. Memory management, scheduling, IPC,
and capability-based syscall dispatch. UEFI bootloader integration.

**Aim 2 — TEE attestation stack (P5).** Implementation of `omni-tee` for
Intel TDX and AMD SEV-SNP, with vendor-neutral trait abstraction. Includes
quote generation, verification chain, sealed key provisioning.

**Aim 3 — Formal protocol verification (P3.1).** Tamarin Prover models for
the mesh handshake. External cryptographer peer review of `omni-crypto` and
the handshake spec, with findings remediated and a public report published.

**Aim 4 — Security audit at Phase 1 closure (P6.8).** Independent security
firm audits kernel + capability system. Report public per disclosure policy
in [`SECURITY.md`](../../../SECURITY.md). Findings remediated; v0.2 of
relevant documents updated.

## 7. Methodology

**Engineering practices:**
- All code in Rust (2024 edition); foundational crates `no_std + alloc`.
- Test discipline: unit tests + property tests (`proptest`) + integration
  tests + compile-fail tests (`trybuild`) + fuzz harnesses (`cargo-fuzz`).
- CI gates: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `cargo
  audit`, `cargo deny`, reproducible-build dual-runner.
- No unsafe code without explicit per-PR justification.

**Verification practices:**
- Protocol-level: Tamarin Prover for symbolic protocol analysis.
- Cryptography: RFC test vectors + `subtle::ConstantTimeEq` + `zeroize::Zeroize`
  on every secret.
- Capability system: property tests for monotonicity invariants.
- External: independent firm at Phase 1 closure; mandatory cryptographer
  peer review before mesh implementation begins.

**Governance practices:**
- All substantive technical decisions via OIP (OMNI Improvement Proposal)
  process. OIPs are public, archived, voted on by TEE-attested nodes with
  quadratic-vote weighting.
- BDFL veto window sunsets after 5 years (immutable).
- Stichting OMNI manages funding, partnerships, and legal response; cannot
  override protocol decisions.

## 8. Budget justification

| Item | Cost (USD) | Notes |
|---|---|---|
| 2 senior Rust engineers (kernel + networking) — 12 months FTE | 220,000 | Includes EU benefits, salary band per [`docs/hiring/salary-bands.md`](../../hiring/salary-bands.md). |
| 1 cryptographer engagement | 15,000 | 4–6 weeks per engagement template. |
| External security audit (Phase 1 closure) | 50,000 | Reputable firm; scope defined by P6.8. |
| TEE development hardware (Intel TDX + AMD SEV-SNP) | 10,000 | Required for actual implementation past P5.1 mock. |
| Travel + conferences | 3,000 | One conference per year, in-person meet for cryptographer engagement. |
| Foundation overhead (audit fee, insurance allocation) | 2,000 | Annual audit per bylaws Article 9.3. |
| **TOTAL** | **300,000** | |

## 9. Investigator credentials

cySalazar is the pseudonymous public-facing identity of the founder. Real-
identity disclosure to Sloan under NDA is acceptable; the project's identity
policy (per [`docs/05-governance.md`](../../05-governance.md) and the
project memory) maintains the pseudonym in the public repo and
documentation through the BDFL veto window. Documented prior work: *(applicant
to attach CV under NDA)*.

## 10. Institutional commitment

Stichting OMNI commits to:

- Open-source release of all grant outputs under AGPL-3.0 (code) and
  CC-BY-SA 4.0 (documentation).
- Annual audited financial report covering Sloan funds.
- Acknowledgement of Sloan support in all relevant public materials.
- No exclusive licensing of grant deliverables to any commercial entity.

## 11. Open science commitments

- Code: AGPL-3.0, public Git repository, all commits cryptographically signed.
- Specifications: CC-BY-SA 4.0, versioned in repo, immutable history.
- Audit reports: public release within 90 days of receipt, redacted as
  required by responsible disclosure.
- Tamarin proofs: source committed to `/protocol-proofs/`; reproducible by
  any reviewer with the public Tamarin Prover.
- Hardware test vectors: published where TEE vendor agreements permit.

## 12. Why Sloan

Sloan's track record of supporting **long-term, foundational, open-source
infrastructure** (Open Source Geospatial Foundation, AstroPy, Project Jupyter)
matches OMNI OS's profile. The Foundation seeks funders whose patience
matches a 25-year horizon and whose values include open-source as a public
good. Most for-profit AI investors do not meet this bar.

## 13. Risks and mitigations

| Risk | Mitigation |
|---|---|
| TEE vendor compromise reveals attestation chain flaw | TEE diversity: project supports multiple vendors and can deny-list compromised generations via OIP. |
| MoE distribution proves infeasible in real network conditions | Roadmap has explicit "stop condition" trigger; pipeline parallelism is the fallback for dense models on personal clusters. |
| External audit identifies Critical findings requiring redesign | Roadmap accommodates one full phase of delay; AGPL ensures community can fork and remediate if Foundation cannot. |
| Founder bus factor | 2-engineer hire post-Phase-0 partially mitigates; BDFL veto sunsets at year 5; all decisions logged. |

## 14. Outcomes and reporting

Mid-grant report (month 9): progress against Aims 1–4, financial status,
risk register update.

Final report (month 18): all aims delivered or explicitly waived with
rationale, financial accounting, public audit reports, OIPs ratified
during grant period.

Public outputs (in addition to code and docs): one technical write-up per
Aim (4 total), conference presentation at a venue Sloan would consider
appropriate (e.g., USENIX Security, RWC, OSDI).

---

## Appendix A — Letters of support

To be solicited from:

- Members of the RustCrypto maintainer team.
- A privacy-focused EU NGO (Bits of Freedom, Privacy International, etc.).
- One academic cryptographer (depends on cryptographer-engagement selection).

## Appendix B — Prior funding

None to date. Phase 0 outlay is founder-funded. The Foundation has no prior
grants from any funder at the time of this application.

## Appendix C — Conflict of interest

cySalazar has no financial holdings in Sloan-funded organizations, no
employment relationship with any Sloan grantee, and no board position
creating a material conflict with the OMNI OS mission. Updated annually
per bylaws Article 8.
