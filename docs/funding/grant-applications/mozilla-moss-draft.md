# Mozilla Open Source Support (MOSS) — Grant Application Draft

**Target track:** *Foundational Technology Track* (most likely fit). The
*Mission Partners* and *Secure Open Source* tracks are evaluated as
alternates depending on Mozilla's current program structure at submission time.
**Status:** Draft v0.1 — for submission upon Stichting OMNI constitution.
**Drafted by:** cySalazar
**Last updated:** 2026-05-10

---

## 1. Project name and URL

**OMNI OS** — https://github.com/CySalazar/omni

## 2. Brief description

OMNI OS is a new operating system that treats AI as a first-class kernel
primitive while enforcing privacy cryptographically at the protocol level.
Users can leverage 100B+ parameter models via a federated P2P mesh of other
OMNI OS instances **without** sending personally identifying information to
any third party. The system is rooted in hardware TEEs (Intel TDX, AMD SEV-SNP),
employs Mixture-of-Experts model architecture for natural fragmentation
across mesh nodes, and is governed by a Dutch foundation with an explicit
anti-capture mandate.

## 3. The team

**Lead Architect:** cySalazar (pseudonymous; real identity available under
NDA). Background in cybersecurity, AI security, systems programming.
**Time commitment:** full-time post-Phase-0 funding.

**Hiring plan:** 2 senior Rust engineers (kernel/embedded + networking) and 1
cryptographer to be hired upon Phase 0 closure. Job descriptions are public
under [`docs/hiring/`](../../hiring/).

**Foundation:** Stichting OMNI, Netherlands (in formation; KVK target Q3 2026).

## 4. What the grant funds

**Mozilla MOSS — Foundational Technology Track grant: USD 200,000.**

Allocation:

| Item | Amount (USD) | Outcome |
|---|---|---|
| 2 senior Rust engineers — 6 months FTE | 120,000 | Implementation of P5 (`omni-tee` Intel TDX + AMD SEV-SNP backends) and start of P6 (`omni-kernel` no_std transition + UEFI bootloader integration). |
| 1 cryptographer — 4-week engagement | 15,000 | Peer review of `omni-crypto` and mesh handshake (P3.2). |
| TEE-capable development hardware | 12,000 | 4 Intel TDX-capable workstations + 2 AMD SEV-SNP servers. Required for actual TDX/SEV-SNP backend implementation (cannot be mocked through P5 closure). |
| External security audit — pre-v1.0 milestone | 40,000 | Independent firm reviews kernel + capability system at Phase 1 closure (per roadmap P6.8). |
| Public report + community outreach | 8,000 | Two technical write-ups for the community; conference presentation budget. |
| Foundation overhead (audit, insurance allocation) | 5,000 | Allocated per Stichting bylaws Article 9.3. |

## 5. Why this matters for Mozilla / the open web

Mozilla's "internet health" framing identifies decentralization, privacy,
and digital agency as load-bearing values. The dominant AI providers today
violate all three: centralized infrastructure, default data collection,
zero user agency over the inference path.

OMNI OS rebuilds the OS substrate around the opposite assumptions and is the
only project that takes the *operating-system layer* seriously as the
intervention point. Existing decentralized projects target the network
(Tor, I2P), the application platform (SSB, Matrix, ActivityPub), or the
model itself (open weight releases). None of those replace the OS-level
assumption that data exfiltration is normal.

A successful OMNI OS materially strengthens the open web by:

1. **Reducing dependency** on hyperscaler-hosted AI for mainstream use cases.
2. **Demonstrating** that privacy-preserving AI is operationally viable at
   model scales mainstream users actually need.
3. **Providing an OSS reference design** that other communities can adopt or
   fork (Apache-2.0; no CLA; protocol forks are first-class).

## 6. Open source and license

- Code: **Apache-2.0**, with dual commercial licensing administered by
  Stichting OMNI for use cases that require commercial support.
- Documentation: **CC-BY-SA 4.0**.
- No CLA. Contributors retain copyright; DCO sign-off is required.

The dual-licensing income flows back to the Foundation budget and is
publicly disclosed in the annual transparency report.

## 7. Why now

Three technological enablers have closed within the last 24 months:

1. **TEE-attested computing in mainstream hardware**: Intel TDX (4th-gen Xeon
   Scalable, 2023) and AMD SEV-SNP (Milan, 2022) are now widely deployable.
   This is the load-bearing primitive for the mesh's privacy guarantees.
2. **MoE production deployment**: Mixtral (2023) and DBRX (2024) demonstrate
   that <2B active parameters per token can match dense 70B quality. This
   makes distributed inference on heterogeneous hardware practical.
3. **Rust `no_std` crypto stack**: RustCrypto family reached production
   quality across AEAD, signatures, KEX, and hashes (2023-2024). A from-
   scratch Rust microkernel with a full crypto stack is now feasible
   without C dependencies or proprietary libraries.

A privacy-first AI operating system was simply not engineering-feasible
before this hardware-software stack matured. The window is now.

## 8. Milestones (12-month horizon for the grant)

| Month | Milestone |
|---|---|
| 1 | Grant funds disbursed; engineers and cryptographer onboarded. |
| 2 | `omni-tee` TeeBackend trait and MockBackend implementation (P5.1). |
| 4 | Intel TDX backend implementation (P5.2) — first real attestation flow. |
| 6 | AMD SEV-SNP backend implementation (P5.3) — multi-vendor support. |
| 7 | Cryptographer review of `omni-crypto` complete (P3.2). |
| 9 | `omni-kernel` no_std transition; UEFI bootloader integration starts. |
| 10 | First public security audit (kernel + capability system) commences. |
| 12 | Audit report public; grant closure with all P5 deliverables landed,
        P6 underway. |

## 9. Sustainability beyond the grant

The Foundation pursues a multi-revenue funding model documented in
[`docs/08-funding-policy.md`](../../08-funding-policy.md):

- Mission-aligned nonprofit grants (this MOSS grant; NLnet; Sloan; OpenPhil).
- Aligned corporate sponsorship (Proton, Mullvad, System76, etc.).
- Dual commercial licensing fees.
- Community donations (post-Stichting ANBI status).
- Self-sustaining mesh fee (post-adoption).

No single funder exceeds 30% of operating budget after year 2 (bylaws-
mandated diversification).

## 10. Risks and contingencies

The roadmap publishes explicit **stop conditions** (per
[`docs/06-roadmap.md`](../../06-roadmap.md) § "Stop conditions"):

- If funding for ≥6 months runway cannot be secured despite documented effort.
- If a TEE security breach renders attestation untrustworthy across all
  vendors.
- If a core architectural assumption is fundamentally broken.

In any such event, the Foundation publishes a transparent status report and
decides within 90 days whether to pivot, hibernate, or wind down. Apache-2.0
guarantees forkability if the Foundation itself fails.

This stance — explicit, public, time-bound stop conditions — is intentional.
Long-lived open-source projects too often die in silence. OMNI OS commits to
public closure with reasoning if it must.

## 11. Contact

- **Project lead:** cySalazar (`cySalazar@cySalazar.com`).
- **Foundation contact** (post-constitution): `hello@omni-os.org`.
- **Security disclosure**: `security@omni-os.org` (mailbox provisioning
  pending Stichting; current fallback: `cySalazar@cySalazar.com`).

## Appendix — Documents linked from this application

- [`README.md`](../../../README.md)
- [`docs/01-vision.md`](../../01-vision.md)
- [`docs/02-architecture.md`](../../02-architecture.md)
- [`docs/04-security-model.md`](../../04-security-model.md)
- [`docs/05-governance.md`](../../05-governance.md)
- [`docs/06-roadmap.md`](../../06-roadmap.md)
- [`docs/08-funding-policy.md`](../../08-funding-policy.md)
- [`/oips/oip-process-001.md`](../../../oips/oip-process-001.md)
- [`/oips/oip-crypto-002.md`](../../../oips/oip-crypto-002.md)
