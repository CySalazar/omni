# OMNI OS — Documentation Index

> Technical documentation for OMNI OS. All documents are `Draft v0.1` until the project's v0.1 specification is formally released.

**Current status:** Design phase, May 2026.

## Documents

| # | Document | Status | Purpose |
|---|---|---|---|
| 01 | [Vision and principles](./01-vision.md) | Draft v0.1 | Mission, target audience, core principles |
| 02 | [Architecture overview](./02-architecture.md) | Draft v0.1 | System layers, execution tiers, model architecture |
| 03 | [Mesh protocol](./03-mesh-protocol.md) | Draft v0.1 | P2P design, transport, privacy primitives |
| 04 | [Security model](./04-security-model.md) | Draft v0.1 | High-level threat model and layered defenses |
| 04a | [Formal threat model](./04a-threat-model.md) | Draft v0.1 | STRIDE/LINDDUN analysis, attack trees, risk matrix |
| 05 | [Governance](./05-governance.md) | Draft v0.1 | 3-layer model, OIP process, Foundation structure |
| 06 | [Roadmap](./06-roadmap.md) | Draft v0.1 | Phases, milestones, version scope |
| 07 | [Hardware requirements](./07-hardware-requirements.md) | Draft v0.1 | Baseline, supported and excluded platforms |
| 08 | [Funding policy](./08-funding-policy.md) | Draft v0.1 | Accepted and excluded funding sources |
| 09 | [Tech specifications](./09-tech-specifications.md) | Draft v0.1 | Languages, libraries, exact versions |
| 10 | [Glossary](./10-glossary.md) | Draft v0.1 | Terminology and acronyms |
| 11 | [Tooling & CI](./11-tooling-and-ci.md) | Draft v0.1 | Toolchain pinning, lints, CI/CD enforcement matrix |

## Suggested reading order

For new contributors:

1. Start with [Vision](./01-vision.md) for context and motivation.
2. Read [Architecture](./02-architecture.md) for the big picture.
3. Dive into [Security model](./04-security-model.md) and [Mesh protocol](./03-mesh-protocol.md) for technical depth.
4. Consult [Glossary](./10-glossary.md) for unfamiliar terms.
5. See [Governance](./05-governance.md) and [Funding policy](./08-funding-policy.md) for project organization.

For implementers:
- [Tech specifications](./09-tech-specifications.md) for exact dependency versions.
- [Hardware requirements](./07-hardware-requirements.md) for target platforms.
- [Roadmap](./06-roadmap.md) for phase-by-phase scope.

## Maintenance policy

- Documentation lives alongside code in the same repository.
- Code changes that affect architecture, protocol, or external behavior MUST be accompanied by documentation updates in the same change set.
- Documentation versioning follows the project's semantic versioning.
- All documents track their status (`Draft`, `Review`, `Final`, `Deprecated`).

## OIP process

Substantive changes to architecture, protocol, or governance require an OMNI Improvement Proposal (OIP). See [Governance](./05-governance.md) for the OIP workflow.
