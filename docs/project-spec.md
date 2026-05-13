# Project Specification (index)

> **The OMNI OS technical specification predates the D.O.E. Framework installation. It is composed of the numbered documents below; this file is an index pointing into them.**
>
> The D.O.E. Framework (`doe-framework/CLAUDE.md`) expects to find an approved technical specification at `docs/project-spec.md`. The OMNI OS spec is too large for a single document and is split into the numbered series at `/docs/01-*.md` … `/docs/11-*.md`. The D.O.E. Project Intake Protocol (`doe-framework/L1-directives/01-project-intake.md`) is therefore considered **already completed** for OMNI OS, with the output being the documents below.

## Approved technical specification — composite document map

| Section | Authoritative source | Purpose |
|---|---|---|
| Vision and principles | [`/docs/01-vision.md`](./01-vision.md) | Mission, target audience, core principles |
| Architecture | [`/docs/02-architecture.md`](./02-architecture.md) | System layers, execution tiers, model architecture |
| Mesh protocol | [`/docs/03-mesh-protocol.md`](./03-mesh-protocol.md) | P2P design, transport, privacy primitives |
| Security model (overview) | [`/docs/04-security-model.md`](./04-security-model.md) | Layered defenses + 5 privacy primitives |
| Threat model (formal) | [`/docs/04a-threat-model.md`](./04a-threat-model.md) | STRIDE / LINDDUN analysis, attack trees, risk matrix |
| Governance | [`/docs/05-governance.md`](./05-governance.md) | 3-layer model, OIP process, Stichting structure |
| Roadmap | [`/docs/06-roadmap.md`](./06-roadmap.md) | Phases, milestones, version scope |
| Hardware requirements | [`/docs/07-hardware-requirements.md`](./07-hardware-requirements.md) | TEE-attestable hardware baseline |
| Funding policy | [`/docs/08-funding-policy.md`](./08-funding-policy.md) | Accepted, borderline, and excluded sources |
| Tech specifications | [`/docs/09-tech-specifications.md`](./09-tech-specifications.md) | Languages, libraries, exact versions |
| Glossary | [`/docs/10-glossary.md`](./10-glossary.md) | Terminology and acronyms |
| Tooling & CI | [`/docs/11-tooling-and-ci.md`](./11-tooling-and-ci.md) | Toolchain pinning, lints, CI matrix |
| Formal protocol specs | [`/docs/protocol/`](./protocol/) | Wire-level handshake spec (P3.1) |
| Audit records | [`/docs/audits/`](./audits/) | Cryptographer engagement template, P0 closure report |
| Legal | [`/docs/legal/`](./legal/) | Stichting OMNI bylaws draft + checklist |
| Funding materials | [`/docs/funding/`](./funding/) | Pitch deck, one-pager, grant drafts, sponsor menu |
| Hiring | [`/docs/hiring/`](./hiring/) | Role descriptions + salary bands |
| Improvement Proposals | [`/oips/`](../oips/) | OIP-Process-001, OIP-Crypto-002, OIP-Bounty-002, OIP-Kernel-003 |
| Implementation TODO | [`/todo.md`](../todo.md) | Canonical task backlog with P0–P6 priority tiers |
| Formal proofs | [`/protocol-proofs/`](../protocol-proofs/) | Tamarin / ProVerif artifacts |

## D.O.E. Project Intake — disposition

Per the D.O.E. Framework, the Project Intake Protocol must be completed and the resulting spec approved before any code is written. For OMNI OS the intake was performed in May 2026 and the spec is the composite above. **Restarting the intake from scratch is not appropriate**; the agent should treat the existing documents as the approved spec and proceed to D.O.E. Step 2 (Decision Engine) for any new substantive work.

For amendments to the existing spec the **OIP process** (per [`/oips/oip-process-001.md`](../oips/oip-process-001.md)) is the canonical change channel — not direct edits to this index nor to the numbered documents without OIP backing.
