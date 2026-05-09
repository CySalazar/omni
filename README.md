# OMNI OS

> An AI-native operating system. Local-first, privacy-by-construction, decentralized by design.

**Status:** Design phase — `v0.1-draft` — May 2026

OMNI OS reimagines the operating system around AI as a first-class citizen. Inference, model orchestration, and intelligent agents are built into the kernel and runtime — not bolted on as cloud services. Privacy is enforced cryptographically, not by policy. The system can leverage other OMNI OS instances as a peer-to-peer compute mesh, scaling computational power collectively without depending on commercial AI providers.

---

## Vision

A globally adopted operating system that gives users the full power of modern AI without surrendering their data to centralized providers. Built for a generational lifetime (25+ years), targeting mainstream adoption.

## Core principles

1. **Local-first** — by default, nothing leaves the device.
2. **Privacy by construction** — the protocol enforces privacy cryptographically; trust is not granted, it is mathematically required.
3. **Decentralization as a means** — to achieve privacy and resist capture, not as an end in itself.
4. **Hardware-rooted security** — TEE attestation is mandatory for mesh participation.
5. **Open evolution** — protocol-compliant forks are first-class citizens.

## Documentation

All technical documentation lives in [`/docs`](./docs/README.md). Highlights:

- [Vision and principles](./docs/01-vision.md)
- [Architecture overview](./docs/02-architecture.md)
- [Mesh protocol](./docs/03-mesh-protocol.md)
- [Security model](./docs/04-security-model.md)
- [Governance](./docs/05-governance.md)
- [Roadmap](./docs/06-roadmap.md)
- [Hardware requirements](./docs/07-hardware-requirements.md)
- [Funding policy](./docs/08-funding-policy.md)
- [Tech specifications](./docs/09-tech-specifications.md)
- [Glossary](./docs/10-glossary.md)
- [Tooling & CI](./docs/11-tooling-and-ci.md)

## Project policies

- [Security policy & responsible disclosure](./SECURITY.md)
- [Contributing guide](./CONTRIBUTING.md) — DCO sign-off, Conventional Commits, PR workflow
- [Code of Conduct](./CODE_OF_CONDUCT.md) — Contributor Covenant v2.1
- [Commercial license terms](./COMMERCIAL-LICENSE.md) — placeholder pending Stichting OMNI

## Quick facts

| | |
|---|---|
| **Language** | Rust (2024 edition) |
| **Architecture** | Custom microkernel, written from scratch |
| **Initial hardware** | x86_64 with Intel TDX or AMD SEV-SNP |
| **Model architecture** | Mixture of Experts (MoE) |
| **License** | Dual: AGPL-3.0 + Commercial (via Stichting OMNI) |
| **Governance** | 3-layer: cryptographic protocol / federated specification / Stichting |

## Status

OMNI OS is currently in the **design phase**. There is no executable code yet. All documentation is `Draft v0.1` and subject to revision until the v0.1 specification is finalized.

The next phase (Phase 0) will establish the legal foundation, secure initial funding, and assemble the core team. See the [roadmap](./docs/06-roadmap.md).

## License

Source code is released under the [GNU Affero General Public License v3.0](./LICENSE) by default.

Commercial licensing is available through Stichting OMNI for use cases incompatible with AGPL obligations. See [`COMMERCIAL-LICENSE.md`](./COMMERCIAL-LICENSE.md) and [funding policy](./docs/08-funding-policy.md).

## Reporting security issues

**Do not open public issues for security vulnerabilities.** Follow the procedure in [`SECURITY.md`](./SECURITY.md) — encrypted reports to `security@omni-os.org` (PGP fingerprint published once Stichting OMNI is constituted; fall back to `cySalazar@cySalazar.com` until then).

## Contributing

Read [`CONTRIBUTING.md`](./CONTRIBUTING.md) before opening a PR. Substantive proposals (protocol changes, breaking APIs, new TEE backends, governance changes) follow the [OMNI Improvement Proposal (OIP)](./docs/05-governance.md) process — see also `OIP-Process-001` (in progress, see [`/todo.md`](./todo.md) P2.1).

Local development quick-start:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

CI enforces all of the above on every PR. See [Tooling & CI](./docs/11-tooling-and-ci.md) for the full enforcement matrix.

## Contact

- Project lead: cySalazar — `cySalazar@cySalazar.com`

---

*OMNI OS is a long-term effort. Stability of design comes before speed of delivery.*
