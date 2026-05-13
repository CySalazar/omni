# OMNI OS — One Pager

> **AI-native, privacy-first, decentralized operating system.**
> Local-first by default. Privacy by cryptographic construction. P2P mesh as
> collective compute. Generational horizon.

---

## What it is

OMNI OS is a new operating system built around AI as a kernel primitive. It
replaces the assumption that AI runs on someone else's computer with the
opposite: AI runs locally, scaled across the user's own devices and an
opt-in P2P mesh of other OMNI OS instances. Privacy is not a feature, it is
a structural property enforced cryptographically.

## Why now

| 2023 | Intel TDX and AMD SEV-SNP ship in mainstream CPUs — TEE-attested computing is no longer exotic. |
| 2023 | Mixture-of-Experts models (Mixtral, DBRX) prove that fragmenting an LLM across many nodes can match 70B dense quality with <2B active parameters per token. |
| 2024 | The Rust ecosystem reaches `no_std` maturity across the cryptographic stack. |
| 2024 | Transparent zero-knowledge proofs (STARK) production-ready. |
| 2025 | Apple Private Cloud Compute proves attestation-bound TEE inference works at planet scale. |

The technological gates closed within the last 24 months. The window for a
mission-aligned, privacy-first AI operating system is now.

## Architecture in 30 seconds

- **Microkernel** in Rust, capability-based, message-passing IPC.
- **AI Runtime Service** as a kernel-level concept with explicit execution
  tiers (`local-only` / `personal cluster` / `federated mesh` / `cloud`).
- **Mesh** uses MoE-style expert distribution so that no single node sees
  full prompt context.
- **Five privacy primitives** enforced at OS API level: encrypted-by-default
  data types, on-device tokenization, format-preserving encryption,
  compliance proofs, TEE-only decryption envelopes.

## Governance (anti-capture by structure)

Three layers:

1. **Protocol** — cryptographic, self-enforcing at runtime.
2. **Specification** — federated via OIP process, anti-Sybil via TEE attestation, quadratic voting.
3. **Operational** — Stichting OMNI (Netherlands), 5 trustees, founder veto sunsets at year 5.

Excluded funding sources: governments and government-aligned funds. The
mission requires explicit independence from regulatory authority.

## What we are asking for

**EUR 350,000 for 6 months of Phase 0:**
- Founder + 2 senior Rust engineers + 1 cryptographer.
- Stichting OMNI setup, insurance, accountant.
- External security audit budget reservation.
- 6-month reserve fund.

This funds the gap between "design completed, scaffolding in place" (now) and
"Phase 1 kernel implementation begins" (month 6).

## Repository

https://github.com/CySalazar/omni — design phase, v0.1 spec complete, P0 + P1
+ P2 closed (repository hygiene, foundational crates `omni-types` /
`omni-crypto` / `omni-capability`, OIP process active).

## Contact

`cySalazar@cySalazar.com` (pseudonym during Phase 0; real-identity disclosure
under NDA).

Response time: 5 business days.

---

*OMNI OS is a long-term effort. Stability of design before speed of delivery.
The first commit is dated 2026-05-09. The roadmap is honest about what is
possible by year 4 and explicitly does not promise faster.*
