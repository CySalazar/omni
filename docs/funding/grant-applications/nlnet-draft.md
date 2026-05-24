# NLnet Foundation — Grant Application Draft

**Target program:** NLnet "NGI Zero Core" (most likely fit) or "NGI Zero Commons Fund".
**Status:** Draft v0.1 — for submission upon Stichting OMNI constitution.
**Drafted by:** cySalazar
**Last updated:** 2026-05-10

> **Operational note:** NLnet falls in the "borderline" category of the OMNI OS
> funding policy because, while NLnet is a private nonprofit, it channels EU NGI
> (Next Generation Internet) funds upstream. The Foundation's board MUST
> evaluate each NLnet grant per-instance for operational independence per
> [`docs/08-funding-policy.md`](../../08-funding-policy.md) "Borderline sources".
> The view from the founder is that NLnet's track record of genuine operational
> independence (selecting projects, no political interference, no IP claims)
> meets the bar — but the board has final say.

---

## A. Project basics

**Project name:** OMNI OS

**Tagline:** An AI-native, privacy-first, decentralized operating system.

**Website:** https://github.com/CySalazar/omni
*(formal website pending Stichting OMNI domain setup)*

**Organization:** Stichting OMNI (in formation — Dutch foundation, target
KVK registration: 2026-Q3)

**Applicant:** cySalazar — Lead Architect, founder
**Applicant role:** project lead; primary author of all design documents
**Region:** Netherlands (Stichting seat once constituted) / European Union

**Amount requested:** EUR 50,000 (NLnet single-grant ceiling)
**Project duration:** 12 months
**Start date target:** Q3 2026 (post-Stichting constitution)

---

## B. Abstract (max 1200 chars)

OMNI OS is an operating system built around AI as a first-class kernel
primitive. Inference, model orchestration, and intelligent agents are built
into the kernel and runtime — not bolted on as cloud services. Privacy is
enforced cryptographically: PII never leaves the device in cleartext;
sensitive workloads run inside attested TEEs; the mesh enforces compliance
proofs per payload. A user can leverage 100B-parameter models without
surrendering data to centralized providers, by combining local compute,
LAN-pooled compute across the user's own devices, and opt-in P2P mesh
participation with other OMNI OS instances.

The project is in early Phase 0 (foundation). Design is complete, foundational
Rust crates (types, crypto, capabilities) are implemented, OIP governance
process is active. This grant funds the cryptographic peer review of the
foundational crates plus the formal mesh handshake specification — both
gating Phase 1 (microkernel implementation).

---

## C. Have you been involved in projects or organisations related to internet research?

Not yet at organisational level; this is the first foundation the applicant
is constituting. Personal background in cybersecurity, applied cryptography,
and systems programming. Substantial relevant prior work includes:
*(applicant to fill in: pen-test reports, talks, prior projects)*.

---

## D. The challenge

Modern AI is reshaping computing. Every consumer AI assistant — ChatGPT,
Claude, Gemini, Copilot — sends user prompts, files, and behavioural data
to remote servers operated by a handful of US-headquartered companies.
Privacy-preserving alternatives (on-device LLMs) are constrained to models
small enough to fit on individual hardware (<10B parameters), which produces
significantly degraded quality versus the cloud-hosted state-of-the-art.

The structural problem is not "people choose to give up privacy" — it is
that the operating system layer assumes data exfiltration is normal.
Application developers have no API to invoke a privacy-preserving AI service;
the OS does not enforce any boundary between trusted local computation and
untrusted remote computation.

OMNI OS aims to **rebuild the OS layer around the opposite assumption**:
remote AI is opt-in, transparent, and cryptographically constrained.

This matters for the Next Generation Internet because the next decade of
computing will be AI-shaped. If the OS layer continues to assume cloud
delegation, EU users will continue to depend on non-EU cloud providers for
basic AI functionality. A privacy-first, mesh-based alternative — built in
the EU, governed in the EU — is a strategic asset.

## E. Solution

OMNI OS solves the problem in four moves:

1. **AI as a kernel primitive.** The microkernel exposes `ai_invoke`,
   `ai_stream`, `ai_embed`, `ai_classify` as syscalls. Every call requires a
   capability token; the AI Runtime Service refuses calls without capabilities.
2. **Execution tiers.** The runtime evaluates each workload against four tiers
   (local → personal cluster → federated mesh → commercial cloud) and picks
   the most local that meets the workload's requirements. Sensitive data is
   never elevated beyond the user's policy.
3. **Five privacy primitives at the protocol level.** Encrypted-by-default
   types, on-device tokenization, format-preserving encryption for routing
   metadata, compliance proofs per payload, TEE-only decryption envelopes.
4. **P2P mesh as collective compute.** OMNI OS instances form an opt-in
   federated mesh. MoE expert distribution means no single node sees a full
   prompt; routing is anonymized via 3-hop onion routing for sensitive
   workloads.

This grant funds **two specific deliverables** that are critical-path:

- **D1 — Cryptographic peer review** of `omni-crypto` (3 crates: AEAD, signing,
  key exchange) and the mesh handshake specification. Engaged cryptographer
  produces a written review (public, redacted as needed) covering soundness of
  primitive choice, API surface, and protocol invariants (I1–I8 in the
  handshake spec).
- **D2 — Formal handshake verification** via Tamarin. The handshake spec
  includes a Tamarin model in `/protocol-proofs/handshake.spthy`; this
  deliverable runs the prover, proves the lemmas, and extends them to cover
  measurement-root binding and downgrade resistance.

Both deliverables are public outputs (CC-BY-SA), benefiting the broader open-
source ecosystem (Noise protocol, RustCrypto, Tamarin tutorials).

## F. Roadmap

| Month | Milestone |
|---|---|
| 1 | Engage cryptographer (RFP closes; selection by NLnet). |
| 2 | Kickoff; cryptographer reviews `omni-crypto` API. |
| 3 | First written review delivered; project disposes findings. |
| 4 | Cryptographer reviews mesh handshake spec. |
| 5 | Tamarin lemmas extended; proofs executed. |
| 6 | Second written review delivered. |
| 7–10 | Project remediates findings; public OIP `OIP-Crypto-004` published. |
| 11 | Re-run Tamarin proofs after remediations. |
| 12 | Final public report published; grant closure. |

## G. Open source and license

All deliverables are released under:

- **Code**: Apache-2.0 (project default).
- **Documentation and specifications**: CC-BY-SA 4.0.
- **Cryptographer's review**: CC-BY-SA 4.0 with reviewer's attribution.
- **Tamarin proof artifacts**: CC-BY-SA 4.0.

## H. Budget

| Category | Amount (EUR) |
|---|---|
| Cryptographer engagement (4–6 weeks) | 15,000 |
| Founder time (research, coordination, remediation; 20 hours/month × 12 months × EUR 80/h) | 19,200 |
| Tamarin tooling (hardware, cloud compute for long proof runs) | 1,500 |
| Travel (one in-person workshop with cryptographer) | 2,500 |
| Public report drafting and publication | 2,000 |
| Buffer (10%) | 4,020 |
| Stichting overhead (audit fee allocation) | 5,780 |
| **TOTAL** | **50,000** |

## I. Comparison with existing work

| Project | Domain | OMNI overlap |
|---|---|---|
| **Linux** | OS | OMNI is from-scratch, AI-native, microkernel; not a Linux fork. |
| **redox-os** | Rust microkernel | OMNI takes inspiration from redox's Rust microkernel discipline; OMNI adds AI runtime as kernel primitive. |
| **Tor / I2P** | Anonymous networking | OMNI's onion routing borrows from Tor; OMNI is the *operating system* layer, not just network anonymity. |
| **ZeroNet / SSB** | Decentralized application platform | OMNI is OS-level, lower in the stack. |
| **Apple Private Cloud Compute** | TEE-attested cloud inference | OMNI replicates the attestation-bound TEE inference pattern but in a P2P mesh rather than centrally-operated cloud. |
| **TrueAI / NousAI** | Decentralized AI training | OMNI focuses on inference for v1; training arrives in v2. |

OMNI's distinctive contribution is the **synthesis**: no other project
combines microkernel + AI runtime + TEE mesh + cryptographic compliance
proofs into a single operating system with anti-capture governance.

## J. Track record

- **Repository**: https://github.com/CySalazar/omni — public, Apache-2.0,
  branch-protected, all commits SSH-signed and GitHub-verified.
- **State at application time** (2026-05-10):
  - Repository hygiene complete (P0: 9/9).
  - Foundational crates implemented (P1: 3/3, 131 unit tests + 7 integration
    tests + 4 compile-fail tests, all green; `cargo clippy -D warnings` and
    `cargo doc -D warnings` clean).
  - OIP process operational (`OIP-Process-001` active under bootstrap fiat;
    `OIP-Bounty-002` filed as first non-Meta OIP for dogfood testing of the
    formal voting flow; `OIP-Crypto-002` filed as STARK-over-SNARK decision).
- **No external funding to date.** Phase 0 outlay is founder-funded.

## K. Risks

| Risk | Mitigation |
|---|---|
| Cryptographer finds Critical issues that require redesign | Project's roadmap explicitly accommodates this (P3 gates Phase 4 mesh implementation; reorientation cost is at most "one phase late", not "project ended"). |
| Tamarin proofs require auxiliary lemmas that take longer than budgeted | Buffer line in budget; if exhausted, the public report ships with a clearly-flagged "in progress" section and a public timeline for completion. |
| Stichting OMNI delayed | Grant payment can be held by NLnet until Stichting exists; founder-funded bridge maintains progress. |

## L. Additional info

- Code of conduct: Contributor Covenant v2.1, enforced via
  [`CODE_OF_CONDUCT.md`](../../../CODE_OF_CONDUCT.md).
- Security policy: published in [`SECURITY.md`](../../../SECURITY.md).
- Conflicts of interest disclosure: the founder has no holdings in NLnet,
  any NLnet sponsor, or any Excluded Source per the OMNI funding policy.

---

## Appendix — Mapping to NLnet evaluation criteria

| Criterion | How OMNI OS meets it |
|---|---|
| Free / open technology | Apache-2.0; OIP process; no CLA. |
| Public benefit | Privacy and digital autonomy for end users; structural anti-capture. |
| Technical merit | Microkernel + MoE-on-mesh + STARK compliance proofs — non-trivial synthesis. |
| Feasibility | Foundational crates already implemented; OIP infrastructure live; design phase complete. |
| Visibility | Public repo, public OIPs, public roadmap, public funding policy. |
| Maintenance plan | Stichting OMNI commits to long-term stewardship; Apache-2.0 forkability as insurance. |
| Diversity / inclusion | Code of Conduct in place; OIP process is permissionless. |
