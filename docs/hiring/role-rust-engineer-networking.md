# Senior Rust Engineer — Networking, Mesh, and AI Runtime

**Status:** Job description draft (Status v0.1)
**Posting target:** post-Phase-0 closure (Q3-Q4 2026)
**Location:** EU / EEA remote, with quarterly on-site (Amsterdam) for
in-person sprints.
**Compensation:** see [`docs/hiring/salary-bands.md`](salary-bands.md).
**Type:** full-time employment with Stichting OMNI.

---

## About OMNI OS

(See [`role-rust-engineer-kernel.md`](role-rust-engineer-kernel.md) §
"About OMNI OS" for the project introduction.)

This role is the **counterpart** to the kernel engineer position: where they
own the bare-metal kernel, you own the userspace services that ride on it —
the mesh protocol implementation, the AI Runtime Service, and the
tokenization service.

## What you will do

Across Phases 1–4 of the roadmap:

- Implement `crates/omni-mesh`: discovery (Kademlia DHT), transport (QUIC +
  Noise_IK), peer attestation handshake, routing, compute credits, reputation,
  compliance proofs. Per [`docs/protocol/handshake.md`](../protocol/handshake.md).
- Implement `crates/omni-runtime`: AI Runtime Service. Model lifecycle,
  inference scheduling across accelerators, tier routing, model attestation.
- Implement `crates/omni-tokenization`: on-device NER classifier, per-user
  token vault inside TEE, configurable PII policies.
- Integrate with TEE attestation (P5) and the kernel's capability system
  (P1.3).
- Participate in the formal mesh handshake verification (Tamarin) and the
  cryptographer's peer review (P3).
- Implement the first Tier 2 (federated mesh) network test deployment.

## What we expect

**Required:**

- 5+ years of professional Rust.
- Strong networking background: QUIC, TLS 1.3, libp2p, or comparable
  experience. Comfortable reading RFCs as a primary reference.
- Strong cryptographic-protocol implementation discipline: not "use the
  default", but "I know what `verify_strict` does and why we use it".
- Familiarity with Noise Protocol Framework (Noise_XX, Noise_IK).
- Comfortable with async Rust (`tokio` ecosystem) and `async-trait`.
- Strong test discipline: unit + property + integration + fuzz, including
  adversarial test scenarios.
- English working language.
- Mission alignment with OMNI OS principles.

**Bonus:**

- Experience implementing a federated / P2P network from scratch (libp2p
  internals, custom DHT, BitTorrent extensions, etc.).
- AI / ML inference experience, especially with MoE architectures.
- Tensor library experience: `candle`, `tch`, custom ONNX runtimes, etc.
- Familiarity with zero-knowledge proofs (`winterfell`, `arkworks`) —
  needed for `stark-v0` compliance proof implementation per
  `OIP-Crypto-002`.

**Not required:**

- Kernel / embedded experience (that's the kernel engineer's domain).
- AI training / fine-tuning experience (out of scope for v1.0; deferred to v2).

## What we offer

Same package as the kernel engineer role: salary EUR 95,000–135,000, NL
employment benefits or EOR equivalent, public credit, no CLA. See
[`role-rust-engineer-kernel.md`](role-rust-engineer-kernel.md) § "What we
offer" for details.

## Why this role specifically

The mesh is the *visible face* of OMNI OS. It is also the largest single
implementation surface (six crates: `omni-mesh`, `omni-runtime`,
`omni-tokenization`, plus integrations into `omni-hal`, `omni-sdk`,
`omni-agent`). The engineer who takes this role will be responsible for the
single largest body of code in the project and will work directly with the
cryptographer (P3.2) on protocol soundness.

If you want to **be the implementer of the protocol that makes private
federated AI inference real at mainstream scale**, this is the role.

## How to apply

See [`role-rust-engineer-kernel.md`](role-rust-engineer-kernel.md) §
"How to apply" for the process. Submit code samples that demonstrate
networking and crypto-protocol work (e.g., libp2p contributions, custom
Noise implementations, QUIC protocol work, etc.).

## Diversity and conflict-of-interest

Identical to the kernel engineer role.
