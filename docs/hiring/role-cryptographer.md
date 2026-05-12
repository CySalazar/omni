# Cryptographer / Crypto Engineer

**Status:** Job description draft (Status v0.1)
**Posting target:** post-Phase-0 closure (Q3-Q4 2026)
**Location:** EU / EEA preferred; remote globally negotiable. Quarterly
on-site (Amsterdam) for in-person collaboration.
**Compensation:** see [`docs/hiring/salary-bands.md`](salary-bands.md). Premium
band given specialist nature.
**Type:** full-time employment OR fractional consulting (≥ 40% FTE), per
candidate preference.

---

## About OMNI OS

(See [`role-rust-engineer-kernel.md`](role-rust-engineer-kernel.md) §
"About OMNI OS".)

OMNI OS is a cryptography-heavy project. The single most important
non-engineering hire is a cryptographer who can:

- Design or audit the foundational crypto primitives.
- Formally verify the mesh handshake (Tamarin, ProVerif, or comparable).
- Implement the STARK-based compliance proof scheme per
  [`OIP-Crypto-002`](../../oips/oip-crypto-002.md).
- Steward the protocol's evolution through the OIP process.

**The first cryptographer engagement is a short peer review** (P3.2,
4–6 weeks). This full-time role follows that engagement: the project
expects the peer reviewer to optionally convert into the full-time
cryptographer hire if mutual fit emerges.

## What you will do

- **Lead the cryptographic design** of `crates/omni-crypto` (already
  scaffolded; ready for production hardening). Audit and harden the
  AEAD, signing, KEX, hash, KDF wrappers. Specify cipher-suite agility.
- **Formally verify** the mesh handshake. Extend the existing Tamarin
  model to cover all I1–I8 invariants (per
  [`docs/protocol/handshake.md`](../protocol/handshake.md) § 7).
- **Implement the STARK-based compliance proof** (`stark-v0`) per
  [`OIP-Crypto-002`](../../oips/oip-crypto-002.md). Library selection
  (`winterfell` v0.10+ baseline). Benchmark prover and verifier overhead.
- **Design the post-quantum migration**. Hybrid Kyber + Dilithium (or ML-KEM
  + ML-DSA) baseline targeting 2030 per `09-tech-specifications.md`.
- **Author OIPs** for cryptographic decisions (`OIP-Crypto-*` series).
- **Liaise with external auditors** on the recurring security audits.
- **Educate the broader team** through code review, documentation, and
  internal seminars.

## What we expect

**Required:**

- A **PhD in cryptography** OR an equivalent track record:
  - Maintainer of a major cryptographic library (RustCrypto, libsodium,
    arkworks, Tink, etc.).
  - Published author at IACR venues (Crypto, Eurocrypt, Asiacrypt, RWC,
    CHES, USENIX Security, IEEE S&P).
  - Documented protocol-design work (Noise, WireGuard, Signal, Magic
    Wormhole, age, etc.).
- Hands-on cryptographic implementation skill in **at least one** of: Rust,
  Go, C, or formal-methods tools.
- Familiarity with Macaroons / capability-based cryptography.
- Experience with at least one of: Tamarin, ProVerif, EasyCrypt, Cryptol.
- Mission alignment with OMNI OS principles.

**Bonus:**

- Experience with zk-SNARK or STARK implementation (production, not academic).
- Experience with TEE attestation chains (Intel SGX/TDX, AMD SEV/SEV-SNP,
  ARM TrustZone/CCA).
- Public conference presentations at RWC or similar.
- Prior cryptographic peer review of an OSS project.

**Not required:**

- Operating-system or kernel expertise.
- AI / ML expertise.

## What we offer

- **Salary** at the premium band: EUR 110,000–160,000 FTE annualized,
  depending on seniority and location. Fractional consulting available at
  proportional rate.
- **Generous research time**: 20% of work time may be allocated to research
  that benefits the broader cryptographic community (publications, public
  reviews of unrelated OSS projects, etc.), with results published under
  CC-BY-SA 4.0.
- **Conference budget**: attendance and presentation budget for one major
  cryptography conference per year.
- **Tools and hardware**: any cryptographic-tooling subscription or
  TEE-capable hardware needed for the work.
- **NL employment benefits or EOR equivalent**, per kernel engineer role.
- **Public credit**: all substantive work attributed; co-authorship on
  OIPs for which you are a substantive author.

## How to apply

The hiring process for this role differs from the engineer roles because
the natural pipeline is **conversion from the P3.2 peer-review engagement**.
If you are interested:

1. **First option**: respond to the cryptographer engagement RFP
   (template: [`docs/audits/cryptographer-engagement-template.md`](../audits/cryptographer-engagement-template.md))
   for the 4–6 week peer-review window. Mutual fit determined during that
   engagement; conversion to FTE offered if both sides agree.
2. **Second option**: apply directly with the standard process described
   in [`role-rust-engineer-kernel.md`](role-rust-engineer-kernel.md) §
   "How to apply", noting your preference for direct FTE.

Submit, in addition to standard materials:

- A representative cryptographic publication, OSS contribution, or protocol
  review.
- A 1-page note on your view of the choice in
  [`OIP-Crypto-002`](../../oips/oip-crypto-002.md) (STARK over SNARK for v1).

## Diversity and conflict-of-interest

Identical to the engineer roles. Note that the project explicitly invites
applications from cryptographers underrepresented in the field; the
foundation will fund travel and reasonable accommodation.
