# OMNI OS — Implementation TODO

> **Status:** Phase 0 (Foundation) — v0.1 design complete, P0 closure in progress (8/9 done, P0.5 + P0.7 pending user-side execution).
> **Last updated:** 2026-05-09 (post-P0 deliverables)
> **Owner:** Matteo Sala (`matteo.sala@samacyber.io`) — Lead Architect / BDFL (5y)
> **Priority order:** Security → Stability → Performance (per project policy).

This document is the canonical, ordered backlog of tasks required to move OMNI OS
from a finalized design (`/docs` v0.1) into an executable, auditable, contribution-ready
project. Tasks are grouped by priority tier (P0 highest). Each task is self-contained
enough that an external contributor could pick it up in isolation.

---

## Legend

| Symbol | Meaning |
|---|---|
| `[ ]` | Not started |
| `[~]` | In progress |
| `[x]` | Done |
| `[!]` | Blocked / awaiting decision |

| Priority | Meaning |
|---|---|
| **P0** | Repo hygiene & supply-chain hardening — must close before any code ships |
| **P1** | Foundational crates (`omni-types`, `omni-crypto`, `omni-capability`) |
| **P2** | OIP process and governance operationalization |
| **P3** | Threat model deepening + cryptographic peer review |
| **P4** | Phase 0 non-technical (Stichting, funding, legal) |
| **P5** | `omni-tee` + TEE HAL (root of trust) |
| **P6** | Kernel `no_std` transition + UEFI bootloader (Phase 1 core) |

## Dependency graph (one-line)

```
P0 ─────────────────────────────────────────────────────► (sblocca contributi esterni)
   └──► P1 ──► P5 ──► P6
P2 ──► (parallel to P1, gates community contributions)
P3 ──► (parallel to P1, gates mesh implementation in Phase 4)
P4 ──► (parallel everywhere, gates team hiring + Phase 1 start)
```

---

# P0 — Repository Hygiene & Supply-Chain Hardening

**Goal:** make the repository safe, reproducible, and ready to receive external contributions.
**Estimated total effort:** 20–30 hours, solo-founder.
**Blocker for:** any merge from external contributor; Phase 0 closure; any external audit.

---

## P0.1 — Add `LICENSE` file (AGPL-3.0)

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0 / Critical
- **Effort:** 30 min
- **Dependencies:** none
- **Rationale:** README and `Cargo.toml` declare AGPL-3.0 but the physical license file is missing. Without it, the repo's license claim is legally unenforceable and GitHub does not surface it correctly.

**Deliverables:**
- `/LICENSE` — verbatim AGPL-3.0 text from the FSF.
- `/COMMERCIAL-LICENSE.md` — placeholder template referencing Stichting OMNI as licensor (per `08-funding-policy.md` dual-license model).

**Acceptance criteria:**
- [ ] GitHub correctly identifies the repo as AGPL-3.0 in the sidebar.
- [ ] `cargo metadata` reports `license = "AGPL-3.0-only"` for every workspace member.
- [ ] `COMMERCIAL-LICENSE.md` includes contact email and a clear note that it is non-binding until Stichting OMNI is constituted.

---

## P0.2 — Add `SECURITY.md` (responsible disclosure policy)

- **Status:** `[x]` (closed 2026-05-09 — PGP fingerprint TBD until Stichting OMNI is constituted, fallback contact `matteo.sala@samacyber.io` documented)
- **Priority:** P0 / Critical
- **Effort:** 2 h
- **Dependencies:** none
- **Rationale:** OMNI OS is a security-sensitive project. Without a published disclosure policy, researchers will either ghost or publish 0-day. A formal policy is also a precondition for any external audit firm.

**Deliverables (`/SECURITY.md`):**
- Reporting channel (email + PGP public key).
- Scope: protocol vulnerabilities, implementation bugs, supply-chain issues. Out of scope: third-party deps with upstream advisories already published.
- SLA: triage within 72h, status update every 14 days, fix or public-disclosure plan within 90 days (configurable per severity).
- Severity classification (CVSSv4-aligned).
- Safe harbor clause for good-faith research.
- Hall of fame / bounty program: defer to OIP-Bounty-001 (P2 dependency).

**Acceptance criteria:**
- [ ] PGP key fingerprint published and verifiable on at least 2 keyservers.
- [ ] SLA wording reviewed against the upstream RustSec disclosure template.
- [ ] Linked from README.

---

## P0.3 — Add `CONTRIBUTING.md` and `CODE_OF_CONDUCT.md`

- **Status:** `[x]` (closed 2026-05-09 — `conduct@omni-os.org` is a placeholder until Stichting OMNI mailbox exists)
- **Priority:** P0
- **Effort:** 3 h
- **Dependencies:** none
- **Rationale:** required by GitHub community standards; signals project maturity to grant evaluators (NLnet, MOSS, Sloan).

**`CONTRIBUTING.md` must cover:**
- DCO (Developer Certificate of Origin) sign-off requirement.
- Required commit format (Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, etc.).
- Branch naming: `feat/<short-desc>`, `fix/<issue-id>`, `oip/<oip-number>`.
- PR workflow: draft → review → 2 approvals → merge.
- Local setup: `rustup`, `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`.
- How to file an OIP for substantive proposals (link to P2 deliverable).

**`CODE_OF_CONDUCT.md`:**
- Adopt **Contributor Covenant v2.1** verbatim.
- Define enforcement contact: `conduct@omni-os.org` (placeholder until Stichting exists).
- Specify escalation chain: maintainer → lead architect → Foundation board (post-Phase 0).

**Acceptance criteria:**
- [ ] DCO check enforced in CI (P0.6).
- [ ] CoC enforcement contact resolves to a real mailbox.

---

## P0.4 — CI/CD pipeline (GitHub Actions)

- **Status:** `[x]` (closed 2026-05-09 — 7 workflows landed: ci, audit, sbom, reproducible-build, dco, codeql, labeler)
- **Priority:** P0 / Critical
- **Effort:** 8–12 h
- **Dependencies:** P0.1 (license) for `cargo deny` license check.
- **Rationale:** without CI, every merge is a leap of faith. Deterministic builds are explicitly mentioned in `rust-toolchain.toml` rationale; CI is the only way to enforce that.

**Workflows to create under `.github/workflows/`:**

1. **`ci.yml`** — runs on every push and PR:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace --all-features`
   - `cargo doc --workspace --no-deps` (link check)
   - Build matrix: `x86_64-unknown-linux-gnu` (initial scope per `07-hardware-requirements.md`).

2. **`audit.yml`** — daily + on `Cargo.lock` change:
   - `cargo audit` (RustSec advisories)
   - `cargo deny check advisories|bans|licenses|sources`

3. **`sbom.yml`** — on every tagged release:
   - Generate CycloneDX SBOM via `cargo-cyclonedx`.
   - Attach to GitHub release.
   - Generate provenance attestation (SLSA Level 3 target).

4. **`reproducible-build.yml`** — on every release tag:
   - Two parallel runners on identical Ubuntu pinned image.
   - Build the same release artifact, compare hashes byte-for-byte.
   - Fail the release if hashes diverge.

5. **`dco.yml`** — DCO sign-off check via `dcoapp`.

6. **`codeql.yml`** — GitHub CodeQL static analysis (Rust support is beta but worth enabling).

**Acceptance criteria:**
- [ ] All workflows pass on a trivial commit on a fresh branch.
- [ ] Branch protection on `main` requires: `ci`, `audit`, `dco`, `codeql` to be green.
- [ ] Workflow run cost < 10 minutes for the typical PR.

---

## P0.5 — Commit `Cargo.lock`

- **Status:** `[~]` (Cargo.lock present in tree; `scripts/bootstrap-local.sh` ready — user must run from local terminal to finalize `git init` + first commit, sandbox cannot complete due to `.git/` unlink restriction)
- **Priority:** P0
- **Effort:** 5 min
- **Dependencies:** none
- **Rationale:** the `.gitignore` policy comment says `Cargo.lock` IS committed for the workspace, but no lock file is currently in the repo. Reproducible builds and `cargo audit` both rely on the lockfile.

**Acceptance criteria:**
- [ ] `Cargo.lock` present in the repo root.
- [ ] `cargo audit` runs cleanly against committed lockfile.

---

## P0.6 — Add `rustfmt.toml`, `clippy.toml`, `deny.toml`

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0
- **Effort:** 3 h
- **Dependencies:** none
- **Rationale:** the workspace `Cargo.toml` defines lints, but project-wide tool configuration is not pinned. Without `rustfmt.toml` and `clippy.toml`, reformatting drift will emerge across contributors.

**`rustfmt.toml`:**
```toml
edition = "2024"
max_width = 100
use_field_init_shorthand = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
reorder_imports = true
```

**`clippy.toml`:**
```toml
msrv = "1.85"
avoid-breaking-exported-api = false  # we are pre-1.0
disallowed-methods = [
  { path = "std::env::var", reason = "Use a config crate; env reads must be auditable." },
]
```

**`deny.toml` (cargo-deny config):**
- `[advisories]`: vulnerability = `deny`, unmaintained = `warn`, yanked = `deny`.
- `[licenses]`: allow = `["AGPL-3.0", "Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-DFS-2016"]`. Deny everything else (force conscious inclusion).
- `[bans]`: deny `openssl-sys`, `native-tls` (force `rustls`), deprecated crypto crates.
- `[sources]`: only `crates.io` allowed; no git deps without explicit allowlist.

**Acceptance criteria:**
- [ ] `cargo fmt` is a no-op on a fresh checkout.
- [ ] `cargo clippy` produces zero warnings on a fresh checkout.
- [ ] `cargo deny check` passes.

---

## P0.7 — Branch protection + signed commits

- **Status:** `[~]` (`scripts/bootstrap-github.sh` ready — runs once user has pushed to GitHub and authenticated `gh` CLI; SSH-signing setup also handled by `scripts/bootstrap-local.sh`)
- **Priority:** P0
- **Effort:** 1 h
- **Dependencies:** P0.4 (CI must exist before requiring it).
- **Rationale:** "trust is mathematically required" is a project tenet. Signed commits are the lowest-friction enforcement at the SCM layer.

**Configuration:**
- Branch `main`: require PR, require 2 approvals (drops to 1 until co-maintainer joins), require all CI checks green, require linear history, require signed commits, dismiss stale reviews on push.
- Tags: only mergeable from `main`, signed.
- Repo settings: `main` is default branch, force-push disabled, deletion disabled.

**Acceptance criteria:**
- [ ] An unsigned commit is rejected at push time.
- [ ] A PR cannot be merged with red CI.

---

## P0.8 — Issue / PR templates and label taxonomy

- **Status:** `[x]` (closed 2026-05-09 — labels created by `bootstrap-github.sh` after first push)
- **Priority:** P0
- **Effort:** 2 h
- **Dependencies:** none

**`.github/ISSUE_TEMPLATE/`:**
- `bug_report.yml` — structured form: affected crate, version, repro steps, expected/actual, logs.
- `feature_request.yml` — must include a note "if this is substantive, file an OIP first" (link to P2).
- `security_advisory.yml` — links to `SECURITY.md`, refuses public discussion.
- `oip_proposal.yml` — entry point for OIP drafts (P2 dependency).

**`.github/PULL_REQUEST_TEMPLATE.md`:**
- Conventional Commits checklist.
- DCO sign-off reminder.
- Breaking change disclosure.
- Documentation update confirmation (per project policy: docs and code stay in sync).
- Test coverage statement.

**Labels (auto-applied via `.github/labeler.yml`):**
- `area:kernel`, `area:crypto`, `area:capability`, `area:tee`, `area:hal`, `area:runtime`, `area:mesh`, `area:tokenization`, `area:sdk`, `area:agent`, `area:shell`, `area:docs`, `area:ci`.
- `priority:P0`–`P3`.
- `kind:bug`, `kind:feature`, `kind:refactor`, `kind:docs`, `kind:security`.
- `oip-required`, `breaking-change`, `good-first-issue`, `help-wanted`.

**Acceptance criteria:**
- [ ] New issue UI shows all four templates.
- [ ] Auto-labeler correctly applies `area:*` based on changed paths.

---

## P0.9 — Dependabot / Renovate configuration

- **Status:** `[x]` (closed 2026-05-09)
- **Priority:** P0
- **Effort:** 1 h
- **Dependencies:** P0.5

Add `.github/dependabot.yml`:
- Weekly checks for `cargo` ecosystem.
- Group security updates.
- Auto-approve patch updates after CI green.
- Major-version updates: PR only, no auto-merge (require human review for breaking deps).

**Acceptance criteria:**
- [ ] First Dependabot PR opens within 7 days of config merge.

---

# P1 — Foundational Crates Implementation

**Goal:** implement the bottom of the dependency stack so every other crate has solid, tested, audited foundations to build on.
**Estimated total effort:** 4–6 weeks solo, 2–3 weeks with cryptographer.
**Order is mandatory:** `omni-types` → `omni-crypto` → `omni-capability`.

---

## P1.1 — Implement `omni-types`

- **Status:** `[ ]`
- **Priority:** P1
- **Effort:** 1 week
- **Dependencies:** P0 (CI must exist to gate this work)
- **Rationale:** every other crate imports `omni-types`. Identifier confusion (passing `ModelId` where `NodeId` is expected) is a class of bug we eliminate at the type level.

### Sub-tasks

- [ ] **P1.1.a — `identity.rs`**
  - `NodeId([u8; 32])` — derived from TEE attestation report hash, content-addressed, deterministic.
  - `AgentId(Uuid)` — local to a node.
  - `ModelId([u8; 32])` — content-addressed hash of signed model manifest.
  - `CapabilityId([u8; 16])` — opaque, random (UUIDv7 for sortability).
  - `SessionId([u8; 16])` — short-lived, random.
  - All newtypes derive: `Debug`, `Clone`, `Copy` (where size allows), `Hash`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Serialize`, `Deserialize`. **No** `Display` for raw bytes — force callers to use a hex/base32 helper to prevent accidental logging of sensitive IDs.
  - Constructors: each ID type has a `from_*` constructor that documents the trust boundary (e.g., `NodeId::from_attestation_quote(quote: &Quote) -> Result<Self>`).

- [ ] **P1.1.b — `error.rs`**
  - Top-level `enum OmniError` with `thiserror::Error`.
  - Variants: `Crypto`, `Capability`, `Identity`, `Ipc`, `Mesh`, `Tee`, `Hal`, `Tokenization`, `Policy`, `Internal`.
  - Each variant nests a domain-specific error to allow precise pattern matching upstream.
  - `Result<T, OmniError>` type alias.
  - **Critical:** error messages must NEVER include sensitive data (tokens, key material, plaintext PII). Add a `#[deny]` lint or compile-time check where feasible.

- [ ] **P1.1.c — `version.rs`**
  - `ProtocolVersion { major: u16, minor: u16, patch: u16 }`.
  - Constants: `PROTOCOL_VERSION_V0_1`, `PROTOCOL_VERSION_V1_0`, etc.
  - `is_compatible_with(&self, other: &Self) -> bool` — major must match, minor must be `>=`.

- [ ] **P1.1.d — `encrypted.rs` (API surface only, no impl yet)**
  - Empty marker types: `EncryptedString`, `MaskedSSN`, `TokenizedEmail`, `AttestedHash`.
  - `pub trait EncryptedType: Sealed { ... }` with sealed trait pattern (cannot be implemented outside this crate).
  - **No** constructors exposed outside `omni-tokenization`. The only way to mint one is via the tokenization service running inside an attested TEE. Phase 2 work, but the API needs to exist now to prevent other crates from "cheating".

- [ ] **P1.1.e — Tests**
  - Unit tests per newtype: round-trip serde, equality, hash determinism, ordering.
  - Property tests with `proptest`: any random byte sequence parses to the same ID twice (determinism), distinct sequences produce distinct IDs (uniqueness within `2^256` collision space).
  - Compile-fail tests via `trybuild`: cannot construct `EncryptedString` from `String` outside the crate, cannot pass `ModelId` to a function expecting `NodeId`.

**Acceptance criteria:**
- [ ] `cargo test -p omni-types` passes.
- [ ] `cargo doc -p omni-types --no-deps` produces zero warnings.
- [ ] `cargo clippy -p omni-types -- -D warnings` clean.
- [ ] No `unsafe` code, no `unwrap`, no `expect` outside `#[cfg(test)]`.
- [ ] 100% public API documented.

---

## P1.2 — Implement `omni-crypto`

- **Status:** `[ ]`
- **Priority:** P1 / Critical
- **Effort:** 2–3 weeks (longer if cryptographer review is sequential)
- **Dependencies:** P1.1, P3 (peer review should run in parallel and gate the merge)
- **Rationale:** every security guarantee in OMNI OS reduces to correct use of these primitives. A single mistake here is project-ending.

### Sub-tasks

- [ ] **P1.2.a — `aead.rs`**
  - Wrap `chacha20poly1305::ChaCha20Poly1305`.
  - Public types: `OmniAeadKey([u8; 32])`, `OmniNonce([u8; 12])`, `OmniCiphertext(Vec<u8>)`.
  - API: `seal(&key, &nonce, &aad, &plaintext) -> Result<OmniCiphertext>`, `open(&key, &nonce, &aad, &ct) -> Result<Vec<u8>>`.
  - Nonces must be unique per (key, message). Provide a `NonceCounter` type that panics on overflow (defensive).
  - Zeroize key material on drop (`zeroize::Zeroize` derive).

- [ ] **P1.2.b — `signing.rs`**
  - Wrap `ed25519-dalek::SigningKey` / `VerifyingKey`.
  - `OmniSigningKey`, `OmniVerifyingKey`, `OmniSignature([u8; 64])`.
  - API: `sign(&sk, &msg) -> OmniSignature`, `verify(&vk, &msg, &sig) -> Result<()>`.
  - Constant-time signature verification (already in `dalek`).
  - Zeroize on drop.

- [ ] **P1.2.c — `kex.rs`**
  - Wrap `x25519-dalek` for ECDH.
  - `OmniEphemeralSecret`, `OmniPublicKey`, `OmniSharedSecret`.
  - API: `generate_ephemeral() -> (secret, pubkey)`, `diffie_hellman(secret, peer_pub) -> OmniSharedSecret`.
  - Phase 4: hybrid KEM with Kyber (placeholder module).

- [ ] **P1.2.d — `hash.rs`**
  - Trait `OmniHash` with three impls: SHA-256, SHA3-256, BLAKE3.
  - Default for protocol-level hashing: BLAKE3 (fastest, hardware-friendly, post-quantum resilient).
  - `domain_separated_hash(domain: &str, data: &[u8]) -> [u8; 32]` — every hash call must be domain-separated to prevent cross-protocol collisions.

- [ ] **P1.2.e — `kdf.rs`**
  - HKDF-SHA-256 for protocol session keys.
  - Argon2id for user secrets (memory-hard).
  - API: `hkdf_expand(prk, info, len)`, `argon2id_hash(password, salt) -> Result<Hash>`.

- [ ] **P1.2.f — `fpe.rs`** (Phase 4 placeholder)
  - Module exists with `unimplemented!()` and TODO. Do not ship FF1/FF3-1 until Phase 4 needs it.

- [ ] **P1.2.g — `snark.rs`** (Phase 4 placeholder)
  - Module exists. Selection between STARK / transparent SNARK deferred to OIP-Crypto-002 (P3).

- [ ] **P1.2.h — Tests**
  - **RFC test vectors** for every primitive (Ed25519 RFC 8032, X25519 RFC 7748, ChaCha20-Poly1305 RFC 8439, SHA-256 NIST FIPS 180-4, BLAKE3 reference vectors).
  - Property tests: sign/verify round-trip, encrypt/decrypt round-trip, hash determinism.
  - Negative tests: tampered ciphertext fails to decrypt, wrong key fails to verify.
  - **Fuzz target** (`cargo-fuzz`): every public API takes arbitrary input without panicking.
  - Constant-time verification: critical paths (signature verify, AEAD tag check) must use `subtle::ConstantTimeEq`. Add a CI lint that grep-flags `==` on byte arrays in this crate.

**Acceptance criteria:**
- [ ] All RFC vectors pass.
- [ ] No `unsafe` code.
- [ ] Cryptographer review (P3.2) signed off in writing.
- [ ] `cargo bench` baselines recorded for each primitive (perf regressions caught later).

---

## P1.3 — Implement `omni-capability`

- **Status:** `[ ]`
- **Priority:** P1 / Critical
- **Effort:** 2 weeks
- **Dependencies:** P1.1, P1.2
- **Rationale:** capabilities are the runtime enforcement of security policy. A bug here = privilege escalation.

### Sub-tasks

- [ ] **P1.3.a — `token.rs`**
  - `struct CapabilityToken { id: CapabilityId, subject: NodeId, action: Action, resource: Resource, not_before: u64, not_after: u64, caveats: Vec<Caveat>, signature: OmniSignature }`.
  - `Action` and `Resource` are typed enums, not strings.
  - Canonical serialization (deterministic byte order) for signing — use `bincode` with strict-mode config or a hand-rolled encoder. Document the wire format in `/docs/03-mesh-protocol.md`.

- [ ] **P1.3.b — `scope.rs`**
  - Grammar: `Action × Resource × TimeWindow × Caveats`.
  - `fn intersects(&self, other: &Scope) -> bool` — used to validate that a child scope is contained in the parent.

- [ ] **P1.3.c — `attenuation.rs`**
  - Macaroons-style: `parent.attenuate(caveat) -> child` where `child.scope ⊆ parent.scope` always.
  - Each caveat is a signed monotonic restriction (e.g., `not_after = parent.not_after - delta`, `resource = parent.resource ∩ {x}`).
  - **Property test (critical):** for any random parent + random caveat sequence, the derived child scope is always a subset of the parent. This is the security-critical invariant.

- [ ] **P1.3.d — `revocation.rs`**
  - In-memory revocation list (sled-backed in Phase 2).
  - Bloom filter for fast membership check; full list for false-positive resolution.
  - Short TTL (5–15 min) means lists stay small; rotate hourly.

- [ ] **P1.3.e — TEE binding** (placeholder traits, real impl in P5)
  - A capability is invalid unless the verifying TEE attestation matches the capability's `subject`.
  - Define the trait `TeeAttestation` here; impl moves to `omni-tee`.

- [ ] **P1.3.f — Tests**
  - Unit + property tests on attenuation monotony.
  - Test that signature verification rejects: tampered fields, expired tokens, mismatched subject, broader-than-parent caveats.
  - Compile-fail tests: cannot construct a `CapabilityToken` without going through the issuer API.

**Acceptance criteria:**
- [ ] All sub-tasks complete with tests.
- [ ] Adversarial test suite: 100 random tampered tokens, 100% rejected.
- [ ] Wire format documented and added to `/docs/03-mesh-protocol.md`.

---

# P2 — OIP Process and Governance Operationalization

**Goal:** make Layer 2 governance (federated specification) actually usable.
**Estimated effort:** 1 week.
**Blocker for:** community contributions to architecture / protocol; Phase 0 closure.

---

## P2.1 — Write OIP-Process-001 (the meta-OIP)

- **Status:** `[ ]`
- **Priority:** P2 / Critical
- **Effort:** 3 days
- **Dependencies:** P0.3 (CONTRIBUTING)
- **Rationale:** roadmap explicitly requires `OIP-Process-001` to close Phase 0. Without it, every architectural change is autocratic instead of federated.

**Deliverables (`/oips/oip-process-001.md`):**
- OIP types: `Process`, `Standards Track`, `Informational`, `Meta`.
- Lifecycle: `Draft` → `Active` → (`Final` | `Withdrawn` | `Superseded`).
- Required sections: Abstract, Motivation, Specification, Rationale, Backwards Compatibility, Test Cases, Reference Implementation, Security Considerations, Privacy Considerations, Copyright.
- Voting mechanism (initial, manual until tooling exists):
  - Eligibility: TEE-attested unique device (anti-Sybil).
  - Weighting: proof-of-uptime + proof-of-contribution. Concrete formula deferred to OIP-Voting-002.
  - Quorum: 30% of eligible voters or 14-day open window, whichever is reached first.
  - Approval threshold: quadratic-vote majority.
- BDFL veto window: 5 years from v0.1, sunset confirmed in writing in OIP itself.
- Editor role: 2 OIP editors per term, rotated annually.

**Acceptance criteria:**
- [ ] OIP-Process-001 itself passes its own process (dogfood test).
- [ ] Linked from README and `05-governance.md`.
- [ ] Issue template for new OIPs (P0.8) functional.

---

## P2.2 — Set up `/oips/` directory and template

- **Status:** `[ ]`
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1

**Deliverables:**
- `/oips/README.md` — index of all OIPs by number, status, title.
- `/oips/oip-template.md` — copy this for new proposals.
- `/oips/oip-0000-template.md` — same content with reserved number 0.

**Acceptance criteria:**
- [ ] `/oips/README.md` auto-renders a table of contents.
- [ ] CI lint that fails if an OIP file deviates from the template structure.

---

## P2.3 — Document the BDFL veto window in writing

- **Status:** `[ ]`
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1
- **Rationale:** the memory says "BDFL veto for first 5 years (sunset clause)". This must be in a versioned, immutable document so it can't be silently extended.

**Deliverables:**
- Section in `05-governance.md` cross-referencing OIP-Process-001 with explicit start date and sunset date.
- Public commitment in README that the veto cannot be extended without an OIP that itself cannot be vetoed.

---

# P3 — Threat Model Deepening + Cryptographic Peer Review

**Goal:** validate the protocol design before code commits to it.
**Estimated effort:** 2–3 weeks (parallel to P1).
**Blocker for:** `omni-mesh` implementation in Phase 4.

---

## P3.1 — Formal mesh handshake specification

- **Status:** `[ ]`
- **Priority:** P3
- **Effort:** 1–2 weeks
- **Dependencies:** existing `04-security-model.md`, `04a-threat-model.md`
- **Rationale:** changing protocol post-implementation is 10× the cost of changing it on paper.

**Deliverables:**
- `/docs/protocol/handshake.md` — pseudo-code or formal notation (TLA+ if available, otherwise Alloy or pseudo-code with explicit invariants).
- Protocol verification with **ProVerif** or **Tamarin** for symbolic analysis. Validate:
  - Mutual authentication
  - Forward secrecy
  - TEE attestation freshness
  - Resistance to KCI (Key Compromise Impersonation)
  - Resistance to UKS (Unknown Key-Share)
- Document each property as an invariant in the spec.

**Acceptance criteria:**
- [ ] Spec lives under `/docs/protocol/`.
- [ ] Tamarin/ProVerif proof artifacts checked into `/protocol-proofs/`.
- [ ] Review by external cryptographer (P3.2).

---

## P3.2 — External cryptographer engagement

- **Status:** `[!]` blocked on funding (P4)
- **Priority:** P3 / Critical
- **Effort:** 2–4 weeks (cryptographer's calendar)
- **Dependencies:** P4 (funding for paid review) OR community volunteer
- **Rationale:** the roadmap lists "1 cryptographer" in the core team for Phase 0. This sub-task formalizes the engagement.

**Deliverables:**
- Signed engagement letter (paid review or volunteer agreement).
- Written review of: `omni-crypto` API design, mesh handshake spec, capability attenuation invariants, compliance proof scheme.
- Public review document (or executive summary) published in `/docs/audits/`.

**Acceptance criteria:**
- [ ] Cryptographer's name and credentials disclosed in `CONTRIBUTORS.md`.
- [ ] All findings tracked as issues with `kind:security` label.

---

## P3.3 — Decide STARK vs SNARK for compliance proofs

- **Status:** `[ ]`
- **Priority:** P3
- **Effort:** 1 week (research) + 1 week (decision via OIP)
- **Dependencies:** P3.1, P3.2
- **Rationale:** memory note: "favor STARK or transparent constructions for v1". This must become an OIP and a documented decision before `omni-mesh` is built.

**Deliverables:**
- `/oips/oip-crypto-002.md` — proposal: STARK-based compliance proofs, candidate libraries (`winterfell`, `triton-vm`), benchmark results, trusted-setup avoidance rationale.
- Update `04-security-model.md` § "Compliance proofs" with the chosen approach.

**Acceptance criteria:**
- [ ] OIP merged.
- [ ] Benchmark report (proof size, prover time, verifier time) published.

---

# P4 — Phase 0 Non-Technical (Stichting + Funding)

**Goal:** legal + financial foundation for the project to exist as a multi-decade entity.
**Estimated effort:** 3–6 months calendar (slow burn, parallel to all other tracks).
**Blocker for:** hiring (Phase 1), Phase 0 closure.

---

## P4.1 — Constitute Stichting OMNI in the Netherlands

- **Status:** `[ ]`
- **Priority:** P4 / Critical
- **Effort:** 2 months calendar (notary + KVK registration)
- **Dependencies:** legal counsel
- **Rationale:** mandated by `05-governance.md` Layer 3.

**Deliverables:**
- Notarial deed (`stichtingsakte`).
- KVK registration.
- Bylaws (`statuten`) embodying:
  - 5 trustees (founder included).
  - BDFL veto sunset 5y / full transition 10y.
  - Mission anchor: privacy-first, local-first, anti-regulatory-capture (excluded funding sources).
  - Asset lock clause (assets cannot be redirected to non-aligned mission).
- ANBI status pursuit (Dutch tax-deductible charity).

**Acceptance criteria:**
- [ ] KVK number obtained.
- [ ] Bylaws published in `/docs/legal/bylaws.md` (also in Dutch original).
- [ ] First trustee appointment letter signed.

---

## P4.2 — Funding pipeline

- **Status:** `[ ]`
- **Priority:** P4 / Critical
- **Effort:** 3 months calendar
- **Dependencies:** P4.1 (most grants require legal entity)
- **Rationale:** target €350K for 6 months runway per roadmap.

**Sub-tasks:**

- [ ] **P4.2.a — Pitch deck and one-pager**
  - 12–15 slides: problem, vision, proof-of-progress (`/docs` v0.1), team, ask, 5y plan.
  - One-pager for warm intros.
  - Both files in `/docs/funding/` (private branch or out-of-repo).

- [ ] **P4.2.b — Grant applications**
  - **NLnet Foundation** — DECISION REQUIRED: accept or reject as funder (memory marks as borderline TBD). Recommendation: accept, since NLnet funds privacy-aligned projects and EU NGI channeling is *operational* not *regulatory*.
  - **Mozilla MOSS** — apply.
  - **Sloan Foundation** (open-source) — apply.
  - **Open Philanthropy** (long-term safety) — apply.

- [ ] **P4.2.c — Corporate sponsor outreach**
  - Aligned sponsors per memory: Proton, Tutanota, Mullvad, Element, System76, Framework, Purism.
  - One-page sponsorship tier menu (Bronze/Silver/Gold + crypto-aligned naming).
  - Boundary: no regulatory power, no controlling stake, no kill-switch over project direction.

- [ ] **P4.2.d — Community donations**
  - Set up Open Collective or similar (post-Stichting).
  - Transparent monthly accounting.

**Acceptance criteria:**
- [ ] €350K secured or 3 active term-sheets.
- [ ] Public funding ledger.

---

## P4.3 — Excluded funding sources documented and enforced

- **Status:** `[ ]`
- **Priority:** P4
- **Effort:** 1 day
- **Dependencies:** P4.1
- **Rationale:** memory explicitly excludes governments and government-aligned funds. This must be in bylaws and in a public funding policy so a future board cannot quietly accept.

**Deliverables:**
- `08-funding-policy.md` already covers this — review and harden the language.
- Add a clause to bylaws making excluded-source acceptance a supermajority decision (4/5 trustees), publicly logged.

---

## P4.4 — Recruit core team

- **Status:** `[!]` blocked on P4.2
- **Priority:** P4
- **Effort:** 2 months calendar
- **Dependencies:** P4.2 (funding)
- **Roles per roadmap:**
  - Lead Architect — founder (Matteo).
  - 2 senior Rust engineers (one with kernel/embedded, one with networking/distributed).
  - 1 cryptographer.
- Compensation transparency: salary bands published before hiring.

---

# P5 — `omni-tee` + TEE HAL

**Goal:** root of trust. Every security guarantee in OMNI OS reduces to TEE attestation working correctly.
**Estimated effort:** 2–3 weeks after P1.
**Blocker for:** capability validation in production, mesh handshake.

---

## P5.1 — Define `TeeBackend` trait in `omni-tee`

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 3 days
- **Dependencies:** P1.1, P1.2

**API surface:**
```rust
pub trait TeeBackend: Send + Sync {
    fn attest(&self, nonce: &[u8]) -> Result<Quote, OmniError>;
    fn verify_quote(&self, quote: &Quote, expected_measurement: &Measurement) -> Result<(), OmniError>;
    fn seal(&self, plaintext: &[u8], policy: &SealPolicy) -> Result<SealedBlob, OmniError>;
    fn unseal(&self, sealed: &SealedBlob) -> Result<Vec<u8>, OmniError>;
    fn derive_key_for(&self, peer_attestation: &Quote) -> Result<OmniSharedSecret, OmniError>;
}
```

---

## P5.2 — Implement Intel TDX backend

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1–2 weeks
- **Dependencies:** P5.1, hardware access (TDX-capable Intel CPU 4th gen Xeon scalable or later)
- **Rationale:** TDX is the chosen baseline x86_64 TEE.

**Sub-tasks:**
- Wrap `tdx-attestation` crate (or implement Quote v4 generation manually if needed).
- Integration test using Intel's public TDX simulator first; hardware test later.
- Document TCB recovery procedure (when Intel publishes a microcode update affecting attestation).

---

## P5.3 — Implement AMD SEV-SNP backend

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1–2 weeks
- **Dependencies:** P5.1, hardware access (AMD EPYC Milan or later)

---

## P5.4 — TEE HAL re-export in `omni-hal::tee`

- **Status:** `[ ]`
- **Priority:** P5
- **Effort:** 1 day
- **Dependencies:** P5.1

Re-export `TeeBackend` and provide a runtime selector (`select_tee_backend()`) that detects available hardware and returns the appropriate concrete impl.

---

# P6 — Kernel `no_std` Transition + UEFI Bootloader

**Goal:** transition `omni-kernel` from a stub library to a bare-metal microkernel that boots on x86_64.
**Estimated effort:** 6–18 months (Phase 1 of roadmap).
**Blocker for:** everything userspace.

This tier is intentionally low-detail in the TODO — it is the scope of an entire phase of the roadmap, with multiple OIPs governing its sub-decisions. The high-level breakdown:

- [ ] **P6.1 — Convert `omni-kernel` to `no_std` + `no_main`**
- [ ] **P6.2 — UEFI bootloader (decision: Limine vs Tock vs custom)**
- [ ] **P6.3 — Page table management, virtual memory subsystem**
- [ ] **P6.4 — Scheduler (thermal-aware, AI-workload-aware)**
- [ ] **P6.5 — Capability-based syscall dispatch**
- [ ] **P6.6 — Typed message-passing IPC**
- [ ] **P6.7 — Userspace driver model (NVMe, Ethernet/Wi-Fi, TEE)**
- [ ] **P6.8 — First external security audit of kernel + capability system (per roadmap Phase 1 deliverables)**

Each of P6.1–P6.8 will be expanded into its own task list when its corresponding OIP is filed.

---

# Open decisions awaiting Founder input

These are the four decisions blocked on the user (Matteo) that determine how subsequent execution proceeds:

1. **Engagement mode** — implementer (Claude writes code, founder reviews) vs technical co-architect (continuous spec refinement, code authored elsewhere).
2. **P0 vs P1 ordering** — close repo hygiene first, or accept "unprotected" repo and start `omni-types`?
3. **Phase 0 non-technical work (P4)** — already in flight outside this collaboration, or include here?
4. **OIP-Process-001 authorship** — Claude drafts based on memory, founder reviews, OR founder drafts and Claude reviews?

These decisions block start of execution but not strategic planning.

---

# Maintenance policy for this document

- This file is updated **after every completed task**.
- Status icons must reflect reality. Do not mark `[x]` until acceptance criteria are all green.
- Adding a new task requires it to slot into the existing tier structure or justify a new tier.
- Removing or downgrading a task requires either (a) the work is genuinely done, or (b) an OIP that supersedes the requirement.
- Cross-references between this document and `/docs/06-roadmap.md` must stay in sync; when in conflict, the roadmap is authoritative for *what*, this file is authoritative for *how*.
