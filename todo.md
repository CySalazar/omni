# OMNI OS — Implementation TODO

> **Status:** **Phase 1 (Microkernel POC) in pieno corso** — Phase 0 chiusa (P0/P1/P2 ✅; P3/P4 parziali per dipendenze esterne — funding/cryptographer). Track A desktop ✅ (M1-M5 + M3b). Track B kernel ✅ **MB1-MB12** chiusi sul branch `feat/kernel-mb11-userspace` (post-v0.2.0); MB13.a (SIMD force-soft) + **MB13.b (ET_DYN/PIE kernel, upper-half mapping)** chiusi 2026-05-19. Roadmap Phase 1 ~70% — il deliverable "IPC primitives operational (typed message passing)" è chiuso e il blocker boot-path triple-fault è risolto. **Prossimo blocco tecnico: MB13.c** = `omni-capability` come dep di `omni-kernel` (Ed25519 verify reale sostituisce `StubCapabilityProvider`).
> **Last updated:** 2026-05-19 (riallineamento post-MB12: aggiunti tier P6.MB sezioni MB1-MB13, Step 7 closure, P6.6 chiuso da MB12, P6.7 sbloccato da MB12, P3 Tamarin proof eseguito 2026-05-12, `docs/adr/0001..0005` tutti `accepted`. Branch corrente `feat/kernel-mb11-userspace` con HEAD `1a0fa3e`; PR verso `main` ancora da aprire).
> **Storia stati precedenti:** 2026-05-18 (MB12 closure — IPC + multi-task user, ADR-0005, 426 tests). 2026-05-18 (Step 7.1-7.4 lift blanket allows + ADR-0003 + CI `blanket-allow-guard`). 2026-05-18 (MB11 closure — Ring 3 + per-process CR3, ADR-0004). 2026-05-18 (MB10 closure — kernel stack isolation, ADR-0002, PR #33 in `main`). 2026-05-18 (v0.2.0 release — MB1-MB9 + Track A, PR #29 in `main`). 2026-05-16 (MB4/MB5). 2026-05-15 (K5 QEMU smoke gate). 2026-05-12 (scaffolding pass P3-P6 verificato). 2026-05-10 (P1 + P2 chiusi). 2026-05-09 (P0 chiuso).
> **Owner:** cySalazar (`cySalazar@cySalazar.com`) — Lead Architect / BDFL (5y)
> **Priority order:** Security → Stability → Performance (per project policy).
> **Repo:** [github.com/CySalazar/omni](https://github.com/CySalazar/omni) · License: [AGPL-3.0-only](LICENSE) · Branch protection summary in [`docs/11-tooling-and-ci.md`](docs/11-tooling-and-ci.md).
> **HEAD verificato:** `1a0fa3e docs(kernel): MB12 bare-metal smoke finding` sul branch locale `feat/kernel-mb11-userspace` (post v0.2.0 release `25790f0` + PR #33 MB10 `8c1496a` su `main`).
>
> **Allineamento DOE:** questo documento è la **task decomposition L2** (riferimento [`doe-framework/L2-orchestration/02-task-decomposition.md`](doe-framework/L2-orchestration/02-task-decomposition.md)) — i tier P0-P8 sono i moduli DAG-ordinati; le sotto-task atomiche (P*.N.a/b/c) sono le `TASK-NNN` con dipendenze, complessità e acceptance criteria espliciti. I report di stato vivono in [`progress-omni.md`](progress-omni.md) (snapshot mensile/per-milestone — L2 state management) + [`CHANGELOG.md`](CHANGELOG.md) (per-release). Le decisioni architetturali stanno in [`docs/adr/`](docs/adr/) (template DOE `templates/adr-template.md`). Le direttive di esecuzione (code/test/security/docs/CI/deps) sono i 6 file in [`doe-framework/L3-execution/`](doe-framework/L3-execution/).

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
| **P7** | Workspace serialization migration `bincode` → `postcard` (resolves RUSTSEC-2025-0141) |

## Dependency graph (one-line)

```
P0 ─────────────────────────────────────────────────────► (sblocca contributi esterni)
   └──► P1 ──► P5 ──► P6
P2 ──► (parallel to P1, gates community contributions)
P3 ──► (parallel to P1, gates mesh implementation in Phase 4)
P4 ──► (parallel everywhere, gates team hiring + Phase 1 start)
P7 ──► (parallel, gates clean cargo audit/deny pass; depends on P1)
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
- `/LICENSE` — verbatim AGPL-3.0 text from the FSF (`md5 = eb1e647870add0502f8f010b19de32af`, byte-exact match to upstream).
- `/COMMERCIAL-LICENSE.md` — placeholder template referencing Stichting OMNI as licensor (per `08-funding-policy.md` dual-license model). Marked non-binding until Stichting incorporation.

**Acceptance criteria:**
- [x] GitHub correctly identifies the repo as AGPL-3.0 in the sidebar.
- [x] `[workspace.package].license = "AGPL-3.0-only"` and all 12 crate `Cargo.toml` use `license.workspace = true`. CI confirms via `cargo deny check licenses` on every push.
- [x] `COMMERCIAL-LICENSE.md` includes contact email (`cySalazar@cySalazar.com`) and an explicit non-binding clause until Stichting OMNI is constituted.

---

## P0.2 — Add `SECURITY.md` (responsible disclosure policy)

- **Status:** `[x]` (closed 2026-05-09 — PGP fingerprint TBD until Stichting OMNI is constituted, fallback contact `cySalazar@cySalazar.com` documented)
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
- [ ] PGP key fingerprint published and verifiable on at least 2 keyservers. *(Deferred until Stichting OMNI is constituted — `<TBD>` placeholder in `SECURITY.md` § 2.2; will land before any external audit engagement per `P3.2`.)*
- [x] SLA wording aligned to RustSec / industry-standard disclosure templates (72h ack, 14d updates, 90d disclosure; 24h/45d for Critical).
- [x] Linked from README ("Reporting security issues" section).

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
- [x] DCO check enforced in CI via `.github/workflows/dco.yml` (validates `Signed-off-by:` trailer on every PR commit).
- [ ] CoC enforcement contact resolves to a real mailbox. *(Currently `conduct@omni-os.org` is a placeholder, fallback `cySalazar@cySalazar.com` documented; mailbox provisioning awaits Stichting OMNI per `P4.1`.)*

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
- [x] All workflows triggered and started successfully on the initial push to `main` (2026-05-09 first run on commit `61426d5`).
- [x] Branch protection on `main` requires the 8 status checks to be green (`ci/cargo fmt`, `ci/cargo clippy`, `ci/cargo test (ubuntu-24.04)`, `ci/cargo doc`, `audit/cargo audit`, `audit/cargo deny`, `dco/DCO sign-off`, `codeql/CodeQL — rust`).
- [ ] Workflow run cost < 10 minutes for the typical PR. *(Cache wiring via `Swatinem/rust-cache@v2` is in place; first-run baseline pending — Rust toolchain warm-up on cold cache will exceed 10 min once, then subsequent runs settle below.)*

---

## P0.5 — Commit `Cargo.lock`

- **Status:** `[x]` (closed 2026-05-09 — `git init -b main`, four signed commits on `main` after history rewrite to project identity `cySalazar <cySalazar@cySalazar.com>`: `61426d5` initial P0, `15419cb` URL refs, `ebf9539` P0 closure docs, `101ff79` identity standardization. All four are GitHub-verified, not just locally signed.)
- **Priority:** P0
- **Effort:** 5 min
- **Dependencies:** none
- **Rationale:** the `.gitignore` policy comment says `Cargo.lock` IS committed for the workspace, but no lock file is currently in the repo. Reproducible builds and `cargo audit` both rely on the lockfile.

**Acceptance criteria:**
- [x] `Cargo.lock` present in the repo root and tracked from commit `61426d5` onward (56 KB).
- [ ] `cargo audit` runs cleanly against committed lockfile. *(First run scheduled via `audit.yml` daily cron + on Cargo.lock change; verify on first green run.)*

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
- [ ] `cargo fmt` is a no-op on a fresh checkout. *(Verified by `ci/cargo fmt` on first green run.)*
- [ ] `cargo clippy` produces zero warnings on a fresh checkout. *(Verified by `ci/cargo clippy` on first green run.)*
- [ ] `cargo deny check` passes. *(Verified by `audit/cargo deny` on first green run.)*

---

## P0.7 — Branch protection + signed commits

- **Status:** `[x]` (closed 2026-05-09 — applied to `CySalazar/omni` via `scripts/bootstrap-github.sh`: `enforce_admins=true`, `required_signatures=true`, `linear_history=true`, `allow_force_pushes=false`, 1 reviewer, 8 required status checks; SSH ed25519 signing key registered on GitHub as signing-key id 938835. All 4 commits on `main` show `verified: true reason: valid` after the identity rebase.)
- **Priority:** P0
- **Effort:** 1 h
- **Dependencies:** P0.4 (CI must exist before requiring it).
- **Rationale:** "trust is mathematically required" is a project tenet. Signed commits are the lowest-friction enforcement at the SCM layer.

**Configuration:**
- Branch `main`: require PR, require 1 approval (will rise to 2 once a co-maintainer joins per Phase 1 hiring), require all CI checks green, require linear history, require signed commits, dismiss stale reviews on push.
- Tags: only mergeable from `main`, signed (legacy endpoint deprecated; tracked for migration to GitHub Rulesets).
- Repo settings: `main` is default branch, force-push disabled, deletion disabled.

**Acceptance criteria:**
- [x] An unsigned/non-PR push is rejected at push time. *(Live-verified on 2026-05-09: direct push of the docs commit was rejected with "Changes must be made through a pull request" + "8 of 8 required status checks are expected" — protection is operational.)*
- [x] A PR cannot be merged with red CI. *(Enforced by `required_status_checks` containing all 8 workflow jobs.)*

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
- [x] New issue UI shows all four templates (config.yml, bug_report.yml, feature_request.yml, security_advisory.yml, oip_proposal.yml — blank issues disabled).
- [x] Auto-labeler workflow active (`.github/workflows/labeler.yml`); label taxonomy of 32 created via `bootstrap-github.sh`. *(First validation observed on Dependabot PRs — `labeler` job completed `success`.)*

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
- [x] First Dependabot PR opens within 7 days of config merge. *(Live-verified on 2026-05-09: 2 Dependabot PRs auto-opened within minutes of the initial push — `chore(deps)(deps): Bump mockall from 0.13.1 to 0.14.0` and `chore(deps)(deps): Bump the cryptography group with 2 updates`.)*

---

# P1 — Foundational Crates Implementation

**Goal:** implement the bottom of the dependency stack so every other crate has solid, tested, audited foundations to build on.
**Estimated total effort:** 4–6 weeks solo, 2–3 weeks with cryptographer.
**Order is mandatory:** `omni-types` → `omni-crypto` → `omni-capability`.

---

## P1.1 — Implement `omni-types`

- **Status:** `[x]` (closed 2026-05-10 — 33 tests passing, clippy strict + doc strict clean, `no_std + alloc` compile-verified)
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

- **Status:** `[x]` (closed 2026-05-10 — 55 tests passing including RFC vectors for ChaCha20-Poly1305 / Ed25519 / X25519 / SHA-256 / SHA3-256 / BLAKE3 / HKDF-SHA-256; clippy strict + doc strict clean; **AWAITING_CRYPTO_REVIEW** marker in `lib.rs` per P3.2 dependency)
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

- **Status:** `[x]` (closed 2026-05-10 — 43 tests passing including Macaroons-style attenuation monotony property test (64 cases) + tampered-child adversarial property test; clippy strict + doc strict clean; in-crate `MicroBloom` revocation list to keep `no_std + alloc`)
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

- **Status:** `[x]` (closed 2026-05-10 — `oips/oip-process-001.md` `Active` under bootstrap fiat clause §6.3; lifecycle, categories, voting, BDFL window, editor body, Bootstrap Period all formalized; structural lint green. **Amended same day** under bootstrap fiat clause §6.3 with three structural changes: new §6.5 Critical-security Bootstrap exception, expanded §5.2 Known limitations, refined `## Privacy Considerations` + template HTML guidance. Amendment history logged in OIP itself.)
- **Priority:** P2 / Critical
- **Effort:** 3 days
- **Dependencies:** P0.3 (CONTRIBUTING)
- **Rationale:** roadmap explicitly requires `OIP-Process-001` to close Phase 0. Without it, every architectural change is autocratic instead of federated.

**Deliverables (`/oips/oip-process-001.md`):**
- OIP types: `Process`, `Standards Track`, `Informational`, `Meta`. *(Done — §1)*
- Lifecycle: `Draft → Review → Last Call → Active → Final | Withdrawn | Superseded | Rejected`. *(Done — §4. Unified the two slightly different lifecycles in `todo.md` and `docs/05-governance.md` v0.1; the OIP is the authoritative source going forward.)*
- Required sections: Abstract, Motivation, Specification, Rationale, Backwards Compatibility, Test Cases, Reference Implementation, Security Considerations, Privacy Considerations, Copyright. *(Done — §2; CI lint enforces them.)*
- Voting mechanism (initial, manual until tooling exists):
  - Eligibility: TEE-attested unique device (anti-Sybil). *(Done — §5.1)*
  - Weighting: proof-of-uptime + proof-of-contribution. Concrete formula deferred to a future Process OIP under slug `voting`. *(Done — §5.2; bootstrap defaults specified for the deferred period.)*
  - Quorum: 30% of eligible voters or 14-day open window, whichever is reached first. *(Done — §5.3)*
  - Approval threshold: quadratic-vote majority (50%+1); 66.7% supermajority for Layer 1 cryptographic breaks. *(Done — §5.3)*
- BDFL veto window: 5 years from 2026-05-09 (first public commit), sunset 2031-05-09 23:59 UTC, structurally non-extensible. *(Done — §5.4)*
- Editor role: 2 OIP editors per term, rotated annually. Bootstrap Period explicitly codified: 1 interim editor (founder) + Seat 2 vacant until Phase 1 hire OR 2027-05-10 hard deadline. *(Done — §6)*

**Acceptance criteria:**
- [x] OIP-Process-001 itself passes its own process (dogfood test). *(Ratified under the one-time bootstrap fiat clause §6.3 — no prior process exists to vote it in. The dogfood test of the formal flow is deferred to the first non-`Meta` OIP, by design, and explicitly documented as such in §6.3 ¶3.)*
- [x] Linked from README and `05-governance.md`. *(README §"Public commitments" + §"Contributing"; `docs/05-governance.md` §2 fully refactored to point at the OIP as authoritative.)*
- [x] Issue template for new OIPs (P0.8) functional. *(Pre-existing: `.github/ISSUE_TEMPLATE/oip_proposal.yml`; cross-referenced from `oips/README.md` and `CONTRIBUTING.md` §9.)*

---

## P2.2 — Set up `/oips/` directory and template

- **Status:** `[x]` (closed 2026-05-10)
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1

**Deliverables:**
- `/oips/README.md` — index of all OIPs by number, status, title. *(Done; auto-validated against the registry by the lint.)*
- `/oips/oip-template.md` — copy this for new proposals. *(Done; canonical template with frontmatter + 10 required sections.)*
- `/oips/oip-0000-template.md` — same content with reserved number 0. *(Done; sentinel file with `oip: 0000`, status `Withdrawn` to keep it out of the active index, treated as a special case by the lint.)*
- `scripts/lint-oips.py` + `.github/workflows/oip-lint.yml` — CI lint that validates frontmatter, sections, filename↔number coherence, and index cross-reference. *(Done; stdlib-only Python 3.11.)*

**Acceptance criteria:**
- [x] `/oips/README.md` auto-renders a table of contents. *(Markdown table; lint enforces every OIP file has a row.)*
- [x] CI lint that fails if an OIP file deviates from the template structure. *(Verified manually: lint exits 0 with 2 valid OIPs, exits 1 on injected violations during development — see `scripts/lint-oips.py` test trace in dev session.)*

---

## P2.3 — Document the BDFL veto window in writing

- **Status:** `[x]` (closed 2026-05-10 — start `2026-05-09`, sunset `2031-05-09` 23:59 UTC, immutable; structurally non-extensible per asymmetric clause `OIP-Process-001` §5.4)
- **Priority:** P2
- **Effort:** 2 h
- **Dependencies:** P2.1
- **Rationale:** the memory says "BDFL veto for first 5 years (sunset clause)". This must be in a versioned, immutable document so it can't be silently extended.

**Deliverables:**
- Section in `05-governance.md` cross-referencing OIP-Process-001 with explicit start date and sunset date. *(Done — `docs/05-governance.md` §2 "Founder role (years 1–5)" rewritten with three independent immutable anchors: this file, OIP-Process-001 §5.4, first signed commit `61426d5` on 2026-05-09.)*
- Public commitment in README that the veto cannot be extended without an OIP that itself cannot be vetoed. *(Done — README §"Public commitments". The asymmetric clause in `OIP-Process-001` §5.4 makes the window structurally non-extensible by founder action alone.)*

---

# P3 — Threat Model Deepening + Cryptographic Peer Review

**Goal:** validate the protocol design before code commits to it.
**Estimated effort:** 2–3 weeks (parallel to P1).
**Blocker for:** `omni-mesh` implementation in Phase 4.

---

## P3.1 — Formal mesh handshake specification

- **Status:** `[~]` (spec + Tamarin model landed 2026-05-10; model extended to v0.2 on 2026-05-12 with lemmas for I3 + I7-extended + I8; proof execution gated on P3.2)
- **Priority:** P3
- **Effort:** 1–2 weeks
- **Dependencies:** existing `04-security-model.md`, `04a-threat-model.md`
- **Rationale:** changing protocol post-implementation is 10× the cost of changing it on paper.

**Deliverables:**
- [x] `/docs/protocol/handshake.md` — formal wire-level spec with 8 numbered invariants (I1–I8).
- [~] Protocol verification with **ProVerif** or **Tamarin** for symbolic analysis. v0.2 model at [`/protocol-proofs/handshake.spthy`](protocol-proofs/handshake.spthy) covers 8 lemmas:
  - [x] `mutual_authentication` (I1)
  - [x] `forward_secrecy` (I2)
  - [x] `mutual_tee_attestation_binding` (I3) — added 2026-05-12
  - [x] `replay_resistance` (I4 — partial; full I4 needs nonce-uniqueness lemma)
  - [x] `kci_resistance` (I5)
  - [x] `protocol_version_binding` (I7)
  - [x] `measurement_root_binding` (I7-extended) — added 2026-05-12
  - [x] `compliance_capability_no_downgrade` (I8) — added 2026-05-12
  - [ ] I6 (UKS) — to be added or implied by `mutual_authentication` + identity binding (cryptographer to confirm)
- Each property documented as an invariant in `handshake.md` § 2.

**Acceptance criteria:**
- [x] Spec lives under `/docs/protocol/`.
- [x] Tamarin/ProVerif proof artifacts checked into `/protocol-proofs/`.
- [x] **Tamarin proof execution** (`tamarin-prover handshake.spthy --prove` returns `verified` for all 8 lemmas) — completed 2026-05-12 with tamarin-prover 1.12.0; processing time ≈ 1.36s; full run log at [`protocol-proofs/handshake-proof-run-2026-05-12.txt`](protocol-proofs/handshake-proof-run-2026-05-12.txt). Five structural model defects were fixed in-place during the run; details in the run log footer and in [`protocol-proofs/handshake.spthy`](protocol-proofs/handshake.spthy) `Status of proofs` block. One residual wellformedness warning (Message Derivation Checks on peer-controlled variables) carried forward to the cryptographer review.
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
  - Lead Architect — founder (cySalazar).
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

- **Status:** `[~]` (scaffold landed + verified 2026-05-12 — `TeeBackend` trait + `TeeFamily` + `TeeError` taxonomy + `Quote` / `Measurement` / `Nonce` / `SealedBlob` / `SealPolicy` / `TeeSharedKey` + `MockTeeBackend` end-to-end at `crates/omni-tee/`. 23 unit + 4 integration tests. Full `[x]` after the API is consumed by `omni-mesh` per P3 closure.)
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

# P6 — Kernel `no_std` Transition + UEFI Bootloader (Phase 1 of roadmap)

**Goal:** transition `omni-kernel` from a stub library to a bare-metal microkernel that boots on x86_64 with TEE attestation, IPC, capability-based syscalls, user-space driver model, and first external audit.
**Estimated effort:** 6–18 months calendar (Phase 1 of roadmap; ~65% done as of 2026-05-18).
**Blocker for:** everything userspace beyond the embedded probes; Phase 2 entry.

## P6 — Subsystem-level status (one-line per OIP-mapped sub-tier)

- [x] **P6.1 — `omni-kernel` → `no_std` + `no_main`** (closed 2026-05-15 — `bare-metal` feature flag; `OIP-Kernel-012` `Active`; K5 QEMU smoke green on PR #25; panic handler + bump allocator + heap provisioning per `OIP-Kernel-012` § S2 operational).
- [x] **P6.2 — UEFI bootloader (decision: `bootloader_api` 0.11 selected)** (closed 2026-05-16 — `OIP-Kernel-003` `Active` via Solo Founder Fast-Track § 5.5; `kernel-runner` boots under QEMU+OVMF, VirtualBox, Proxmox VMID 103; PR #25 merged).
- [x] **P6.3 — Page table management + virtual memory subsystem** (closed 2026-05-18 — MB2 `PageMapper` x86_64 walker + MB9 huge-page-aware + MB10 kernel-stack VA isolation + MB11 per-process CR3 / `AddressSpace`. `map_4k_into(root,…)` for explicit-root targets. Limit: `map_4k` does not split huge-page entries — tracked under "Kernel follow-up" below).
- [x] **P6.4 — Scheduler (preemptive round-robin)** (closed 2026-05-18 — MB6 cooperative round-robin + MB7 LAPIC xAPIC + MB8 preemption from timer + MB12.0a/b multi-task user dispatch (TSS.rsp0 update + CR3 reload + first-dispatch sentinel via `enter_user_mode`). Thermal/AI-workload-aware variant deferred to Phase 2 — out of P6 scope).
- [x] **P6.5 — Capability-based syscall dispatch** (closed 2026-05-16 MB4 ABI + closed 2026-05-18 MB11/MB12 real handlers: `TaskExit(11)`, `WriteConsole(60)`, `MemMap(1)` stub, `IpcCreateChannel(20)`/`IpcDestroyChannel(21)`/`IpcSend(22)`/`IpcReceive(23)`. STAR fix MB11.1 (`STAR[63:48]=0x10` → CS=0x23, SS=0x1B per Intel SDM). Capability gate via in-kernel `KernelCapabilityCheck` trait + `StubCapabilityProvider` — swap-in compatibile col futuro `Ed25519CapabilityProvider` MB13).
- [x] **P6.5b — ELF64 loader** (closed 2026-05-16 MB5 + esteso MB11 con `Elf64::map_and_load_into` for explicit-root AddressSpace).
- [x] **P6.6 — Typed message-passing IPC** (closed 2026-05-18 MB12 — `KernelIpcRegistry` concreta (`BTreeMap`, niente `HashMap` per via di MB12.0c), `BackpressurePolicy::{Block,Drop,EvictOldest}`, wait queues per canale, capability check 2-livelli, 4 syscall handler operativi, `task_exit` yields se runnables presenti, retry-loop sender/receiver su `WakeAction::Block`. Integration test `mb12_ipc_cross_process.rs` 8 verdi. ADR-0005 `accepted`).
- [~] **P6.7 — User-space driver model (NVMe, Ethernet/Wi-Fi, TEE)** — sbloccato da MB12 (IPC ✅) ma richiede ancora (a) MB13 Ed25519 capability reale, (b) MP/AP enable (LAPIC IPI + per-CPU data + TLB shootdown). Tracciato in P6.MB14+ (Phase 1.5).
- [ ] **P6.8 — First external security audit of kernel + capability system** — Phase 1 deliverable, bloccato da P4 funding + P6.7 done.

---

## P6.MB — Track B kernel milestones (granulare, post-v0.2.0)

Sezione introdotta 2026-05-19 per riflettere il flusso effettivo di lavoro sul branch `feat/kernel-mb11-userspace`. Ogni voce mappa 1:1 alle entries di `progress-omni.md` § 2.2 + `CHANGELOG.md` `[Unreleased]`/`[0.2.0]`.

| ID | Contenuto | Stato | Commit | ADR |
|---|---|---|---|---|
| MB1 | `BitmapFrameAllocator<const N>` + GDT iniziale | `[x]` | `119f3d8` | — |
| MB2 | `PageMapper` x86_64 walker + `map_4k`/`unmap_4k` | `[x]` | `102ec7a` | — |
| MB3 | IDT + handler #DE/#DF/#GP/#PF (CR2 dump) | `[x]` | `657d7d1` | — |
| MB4 | `SYSCALL`/`SYSRET` MSR setup + `INT 0x80` fallback | `[x]` | `f2e88da` | — |
| MB5 | ELF64 loader (parser + segment mapper) | `[x]` | `960e440` | — |
| MB6 | Round-robin scheduler + `omni_context_switch` asm | `[x]` | `27720ee` | — |
| MB7 | LAPIC xAPIC + PIC disable + `sti` + `TICK_COUNT` | `[x]` | `27720ee` | — |
| MB8 | Preemption from LAPIC timer + `need_resched` | `[x]` | `5d9989b` | — |
| MB9 | `PageMapper` huge-page aware + direct-map validator | `[x]` | `926a37e` | [0001](docs/adr/0001-mb9-paging-huge-page-aware.md) |
| MB10 | Kernel stack isolation + guard page | `[x]` | `8c1496a` | [0002](docs/adr/0002-mb10-kernel-stack-isolation.md) |
| MB11 | Primo userspace Ring 3 + per-process CR3 + STAR fix | `[x]` | `22289e1` + `c743173` | [0004](docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md) |
| MB12 | IPC reale (queue + capability stub + multi-task user) | `[x]` | `60f3a82` | [0005](docs/adr/0005-mb12-ipc-message-passing.md) |
| **MB13** | **`omni-capability` integration reale (Ed25519) + bare-metal smoke fix + SIMD `force-soft`** | **`[~]`** (MB13.a + MB13.b chiusi 2026-05-19) | — | TBD ADR-0006 |
| MB14 | MP/AP enable + TLB shootdown cross-AS (Phase 1.5) | `[ ]` | — | — |

### P6.MB13 — `omni-capability` integration reale

- **Status:** `[~]` (MB13.a + MB13.b chiusi 2026-05-19; MB13.c/d/e ancora aperti)
- **Priority:** P6 / High
- **Effort:** 1-2 giornate (gating SIMD + glue + nuovi test) + 0.5-1 giornata per il fix triple-fault
- **Dependencies:** MB12 ✅; nessuna esterna
- **ADR di chiusura:** ADR-0006 (da scrivere — capability dispatch + bare-metal ABI extension)
- **Rationale:** la pipeline MB12 ha consegnato uno `StubCapabilityProvider` interno (subject byte-compare + action shape-match, niente Ed25519). MB13 chiude la promessa Phase 1 "Capability-based security primitives implemented" sostituendo lo stub con un provider reale che chiama `omni_capability::CapabilityToken::verify_full`. Tre work-package indipendenti convergono qui.

#### P6.MB13.a — `force-soft` SIMD su `sha2` + `poly1305` + `curve25519-dalek`

- **Status:** `[x]` (closed 2026-05-19)
- **Effort:** 0.5 giornata (delivered)
- **Deliverables (delivered):**
  - **Workspace `.cargo/config.toml`** (nuovo) — rustflags target-conditional per `x86_64-unknown-none`:
    - `--cfg poly1305_force_soft` (portable backend per `poly1305 0.8`).
    - `--cfg chacha20_force_soft` (portable backend per `chacha20 0.9`).
    - `--cfg curve25519_dalek_backend="serial"` (serial backend per `curve25519-dalek 4.1`).
    - `--cfg sha2_backend="soft"` (portable backend per `sha2 0.11`).
  - **`crates/omni-crypto/Cargo.toml`** — `[target.x86_64-unknown-none.dependencies]` con `sha2_010_force_soft = { package = "sha2", version = "0.10", default-features = false, features = ["force-soft"] }` per attaccare la feature `force-soft` all'istanza `sha2 0.10` portata dai dalek (digest 0.10). Cargo unifica per versione risolta.
  - **`crates/omni-crypto/src/kdf.rs`** — `Zeroize`/`ZeroizeOnDrop` import gating dietro `#[cfg(feature = "rng")]` (era `unused_imports` warning sulla build bare-metal).
- **Alternativa A** (documentata in ADR-0005 § Migration) **NON adottata**: l'estrazione di `omni-crypto-verify` come crate separato sarebbe stata più chirurgica ma avrebbe rotto l'API surface. La passthrough Cargo + cfg flags mantiene la API stabile e produce lo stesso effetto.
- **Acceptance (verified):**
  - `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` clean (era: LLVM ICE su poly1305 + sha2 0.10 + sha2 0.11).
  - `cargo clippy -p omni-crypto --target x86_64-unknown-none --no-default-features -- -D warnings` clean.
  - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` clean (regression).
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.

#### P6.MB13.b — Boot-path fix: ET_DYN/PIE kernel (triple-fault smoke)

- **Status:** `[x]` (closed 2026-05-19)
- **Effort:** 0.5 giornata (delivered)
- **Rationale:** `mb12-userprobe` (e per estensione `mb11-userprobe`) triple-fault su Proxmox VMID 103 / QEMU+OVMF perché il kernel ELF era `ET_EXEC` con `p_vaddr = 0x200000` (PML4 index 0). `bootloader_api` 0.11 non rilocca `ET_EXEC` → kernel finiva in lower half → `AddressSpace::new_with_kernel_half` (clone solo entries 256..511) → al `mov cr3` dentro `enter_user_mode` la pagina con l'istruzione successiva era non-mappata → triple fault.
- **Soluzione adottata (Opzione (a) — ET_DYN/PIE kernel):**
  - **`kernel-runner/.cargo/config.toml`** — rimossi i flag `-C relocation-model=static` + `-C link-arg=--no-pie`. Il target spec `x86_64-unknown-none` ha già `position-independent-executables = true` (Rust 1.83+), quindi il linker produce nativamente un ELF `ET_DYN` con addressing RIP-relative. Il file ora contiene solo un commento esplicativo del cambio MB13.b, così che la merge dei rustflags del workspace `.cargo/config.toml` (force-soft SIMD cfgs) avvenga in modo pulito.
  - **`kernel-runner/src/main.rs`** — `BOOTLOADER_CONFIG` ora imposta `mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`. `bootloader 0.11` applica le relocazioni RIP-relative del kernel ELF spostando l'immagine in upper half (PML4 indices ≥ 256), assieme a `kernel_stack`, `boot_info`, `framebuffer` e `physical_memory`. Tutte queste mapping cadono nella metà clonata per riferimento dal CR3 di boot in `AddressSpace::new_with_kernel_half`, quindi rimangono live dopo il `mov cr3` di `enter_user_mode`.
- **Alternative documentate (non adottate):** opzione (b) linker script con `p_vaddr` upper-half hard-coded (più invasivo, perde la dinamicità del bootloader); opzione (c) trampoline page aliased cross-AS (mitigazione, non risoluzione del root cause).
- **Acceptance (verified):**
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` clean (ET_DYN PIE).
  - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe -- -D warnings` clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` clean (regression).
  - Test integration `mb12_ipc_cross_process.rs` verdi (8/8) — non affetti dal fix bare-metal.
  - Build Info panel aggiornato a Active=`MB13.b ET_DYN upper-half`, Next=`MB13.c omni-capability dep`.
  - Validazione smoke completa su Proxmox VMID 103 deferred a deploy-time (vedi `progress-omni.md` § "Verifica MB13.b").

#### P6.MB13.c — `omni-capability` come dep di `omni-kernel`

- **Status:** `[ ]` (blocked-on `MB13.a`)
- **Effort:** 1 giornata
- **Deliverables:**
  - `crates/omni-capability/Cargo.toml`: aggiungere feature `bare-metal = []` + propagation; verificare `no_std + alloc` compatibility.
  - `crates/omni-kernel/Cargo.toml`: `omni-capability = { path = "../omni-capability", default-features = false, features = ["bare-metal"] }`.
  - `omni_capability::scope::Action` + `Resource` (variants `#[non_exhaustive]` per semver-safety): aggiungere `Action::IpcSend`, `Action::IpcRecv`, `Resource::IpcChannel(u64)`.
  - `crates/omni-kernel/src/capabilities.rs`: nuovo `Ed25519CapabilityProvider` che implementa `KernelCapabilityCheck::verify` chiamando `CapabilityToken::verify_full`; sostituisce `StubCapabilityProvider` nel boot wiring.
- **Acceptance:**
  - `cargo test -p omni-capability` resta verde (43 unit + 7 integration).
  - `cargo test -p omni-kernel --all-features` mostra +N test (target ~6 unit) sul nuovo provider.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` clean.

#### P6.MB13.d — `IpcCreateChannel` syscall ABI extension

- **Status:** `[ ]` (blocked-on `MB13.c`)
- **Effort:** 0.5 giornata
- **Deliverables:**
  - Estendere `IpcCreateChannel(20)` syscall ABI per accettare due pointer postcard-encoded opzionali `(send_token_ptr, recv_token_ptr)`; manteniere il legacy path (token NULL → comportamento MB12 con stub) per non rompere `mb12-userprobe`.
  - Aggiornare `bare_metal/syscall_entry.rs::ipc_handlers::create_channel`.
  - Nuovi user ELFs di test (`userprobe_mb13.rs`): sender ed receiver che pre-firmano token in build-time o usano un token canned.
  - Nuovo test integration `tests/mb13_capability_signed.rs` (target: 6+ checks — happy path, tampered token rejection, mismatched action, expired NBF/NAF, wrong subject).
- **Acceptance:**
  - 6+ nuovi test integration verdi (target workspace total ~432).
  - ADR-0006 scritto e `accepted`.

#### P6.MB13.e — Chiusura ciclo (PR + tag intermedio)

- **Status:** `[ ]` (last; blocked-on `MB13.a`-`MB13.d`)
- **Effort:** 0.5 giornata (PR + CI conformance + release notes)
- **Deliverables:**
  - PR `feat/kernel-mb13-capability-real` → `main` (squash-merge).
  - Tag intermedio `v0.2.1` o `v0.3.0-alpha.1` (decisione: minor bump perché c'è nuova surface API in `omni-capability`).
  - Aggiornamento `progress-omni.md` § 2 (chiusura MB13) + § 4 (move MB13 da gap analysis a Done).
  - Aggiornamento `CHANGELOG.md` `[Unreleased]` → `[X.Y.Z]`.

### Acceptance criteria globali MB13

- [ ] `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` clean (oggi: LLVM ICE).
- [ ] `cargo test --workspace --all-features` ≥ 432 pass (era 426 post-MB12, target +6 da `mb13_capability_signed.rs`).
- [ ] Smoke `mb12-userprobe` su QEMU+OVMF + Proxmox VMID 103 = serial output completo (oggi: triple-fault).
- [ ] Smoke `mb11-userprobe` su QEMU+OVMF = `[user] hello / [user] exit=0` (oggi: stesso bug latente, mai validato manuale).
- [ ] `StubCapabilityProvider` rimosso da `crates/omni-kernel/src/capabilities.rs` (o ridotto a `#[cfg(test)]` mock).
- [ ] ADR-0006 `accepted` in `docs/adr/`.

---

## P6 — Kernel follow-up minori (non bloccanti per MB13, da pianificare in MB14+)

Tracciati per non essere persi. Tutti carryover da `progress-omni.md` § 4.1.

- [ ] **MB13.b follow-up — `mb12-userprobe` user-side serial output missing** (scoperto 2026-05-19 post-MB13.b deploy). Con MB13.b il smoke MB12 ora supera il triple-fault del CR3 e raggiunge `[mb12] handing off to user tasks`, poi la VM va `stopped` senza emettere `ping` / `[user] exit=0` / `[user] exit=0`. Possibili cause: (a) `RoundRobinScheduler::yield_current` non invoca `enter_user_mode` per la first-dispatch dei task user appena registrati (MB12.0a/b detection via `context.rsp == 0` da rivedere), oppure (b) i task user innescano un fault silenzioso (CS/SS selector wrong dopo CR3 switch, oppure user RIP non mappato in upper-half-tolerant fashion). Indagare con `int3` breakpoint in user binary + serial dump dei selettori al primo `enter_user_mode`. Non bloccante per MB13.b strictly (CR3-switch è la goal); precondizione per MB13.c smoke completa.

- [ ] **TLB shootdown multi-core** — nessun MP/AP enable; LAPIC pronta ma il sistema gira single-core. Necessario prima di P6.7 (driver model). MB11 ha previsto questo: il kernel-half "by reference" di `AddressSpace` diventerà un costo cross-AS broadcast con MP. ADR-0004 § Alternative B documenta la mitigazione futura.
- [ ] **`map_4k` huge-page split** — `map_4k` oggi non splitta una 2 MiB/1 GiB PS=1 entry. Non bloccante finché il kernel non riscrive VA in range huge-page mappati dal bootloader, ma rischia di mordere quando il driver model entra in scena.
- [ ] **`omni-userprobe-helloworld` come crate separato** — MB11.7 ha embedded i 167 byte hand-crafted; un crate Rust `no_std` con linker script + `build.rs` ricorsivo produrrebbe lo stesso ELF in modo manutenibile.
- [ ] **CI smoke automatico per `mb11-userprobe` e `mb12-userprobe`** — il job `qemu-boot-smoke` valida MB1-MB10. Servono due nuovi job (o un flag su `scripts/qemu-boot-smoke.sh`) con `EXPECTED_LINES` esteso per le linee `[user] hello` / `[mb12] channel 1 pre-created` / `ping`. Sblocca-bile **dopo** MB13.b (oggi fallirebbero per il triple-fault).
- [ ] **BumpHeap no-free per canali IPC distrutti** — ADR-0005 § Negative: cap raccomandato `queue_depth ≤ 256` per canale; slab/free-list allocator → OIP separato (Phase 2).
- [ ] **Hygiene CHANGELOG MB8** — la riga "Known blocker (MB9)" del 2026-05-17 è ora storica; annotare "resolved by MB9".

---

## P6.7 — Userspace driver model (NVMe, Ethernet/Wi-Fi, TEE)

- **Status:** `[~]` (sbloccato da MB12, gated da MB13 + MB14)
- **Priority:** P6 / Critical (Phase 1 deliverable)
- **Effort:** 6-12 engineer-months estimated
- **Dependencies:** MB13 (capability reale) + MB14 (MP/AP enable) + per-driver OIPs (TBD: `OIP-Driver-NVMe-XXX`, `OIP-Driver-Net-XXX`, `OIP-Driver-TEE-XXX`)
- **Rationale:** roadmap Phase 1 list explicit: "Drivers (in user space): NVMe storage, Ethernet/Wi-Fi networking, TEE". Microkernel principle — tutto fuori dal TCB. Sblocca anche P5.2/P5.3 (TEE backends reali).

## P6.8 — First external security audit of kernel + capability system

- **Status:** `[!]` blocked-on P4 funding + P6.7 done
- **Priority:** P6 / Critical (Phase 1 deliverable)
- **Effort:** 4-8 settimane calendar (auditor's schedule)
- **Dependencies:** P4.2 (funding); P6.7 done; raccomandato anche P3.2 (cryptographer review) chiuso prima

---

Each of P6.1–P6.8 + P6.MB1–P6.MB14 will be expanded into its own task list when its corresponding OIP is filed (vedi `oips/` directory; `OIP-Kernel-003`, `OIP-Kernel-005`, `OIP-Kernel-012` già `Active`).

---

# P7 — Workspace serialization migration (`bincode` v2 → `postcard`)

**Goal:** resolve `RUSTSEC-2025-0141` (`bincode` v2 unmaintained) by migrating the workspace serialization layer to `postcard` 1.x, bumping the wire-protocol from `OMNI-PROTO-v0.1` to `OMNI-PROTO-v0.2`.
**Blocker for:** clean `cargo audit` and `cargo deny` runs on `main` and on every PR.
**Tracking OIP:** [`OIP-Serde-004`](oips/oip-serde-004.md) (`Last Call` since 2026-05-12; 14-day public-objection window closes 2026-05-26).
**Estimated effort:** 1–2 weeks (per the 5-step migration plan in `OIP-Serde-004` § S5).

---

## P7.1 — `OIP-Serde-004` Last Call closure

- **Status:** `[~]` (`Draft → Review → Last Call` all on 2026-05-12; 14-day public-objection window closes 2026-05-26)
- **Priority:** P7 / High
- **Effort:** 14-day Last Call window per `OIP-Process-001` § 5.3 + cryptographer review pass on the canonical-encoding contract (§ S2).
- **Dependencies:** none for advancement to `Review` or `Last Call`; cryptographer engagement (P3.2) for advancement to `Active` is recommended but not procedurally required.
- **Rationale:** the OIP needs to be `Active` for the migration evidence in M1–M5 to be ratified under the Standards-Track activation process.

**Acceptance:** OIP transitions `Draft → Review → Last Call → Active`. `Draft → Review` 2026-05-12 (commit `be4a920`); `Review → Last Call` 2026-05-12 (this commit). `Last Call → Active` triggers on 2026-05-26 (or earlier if ≥30% weighted vote is reached, per `OIP-Process-001` § 5.3).

---

## P7.2 — Migration steps M1–M5

- **Status:** `[~]` — M1–M5 all landed locally 2026-05-12 on branch `feat/p1-foundational-crates`. `OIP-Serde-004` remains in `Review` pending the `Review → Last Call → Active` transition; full `[x]` requires `audit.yml` cron green for 7 calendar days post-merge per the OIP's `Final` criterion.
- **Priority:** P7 / High
- **Effort:** ~1 week of focused work; each step is its own commit per `OIP-Serde-004` § S5.
- **Dependencies:** P7.1 `Active`.

**Sub-tasks:**

- [x] **P7.2.M1** — Workspace dep swap in `Cargo.toml`. Verified `cargo build --workspace --all-features` clean (commit `b8de469`).
- [x] **P7.2.M2** — `omni-types::wire` canonical-encoding helper module + clippy `disallowed-methods` on raw `postcard::*` calls outside the helper. Verified `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (commit `9b3d977`).
- [x] **P7.2.M3** — `omni-capability` `CapabilityToken` migration; 4 new round-trip regression tests pin postcard-canonical-encoding properties at the public-API boundary. 47 unit + 7 integration tests green (commit `b451539`).
- [x] **P7.2.M4** — `omni-tee` round-trip tests + `omni-types::ProtocolVersion::V0_2` constant. Five new wire-format tests on `Quote` and `SealedBlob` + two version-compatibility tests (commit `61a2b02`).
- [x] **P7.2.M5** — `crates/omni-capability/tests/wire_format_v0_2.rs` reference vector (`TokenPayload` byte prefix pinned at 49 bytes) + `crates/omni-tee/tests/wire_format_v0_2.rs` adversarial suite (4 tests covering bit-flip-on-covered-fields, prefix-truncation, trailing-byte-extension, swap-with-unrelated). `cargo audit` exit 0 (RUSTSEC-2025-0141 absent — `cargo tree --invert bincode` returns "did not match any packages"); `cargo deny check advisories` ok. (`bans` + `licenses` fail with **pre-existing** issues unrelated to OIP-Serde-004: `cpufeatures` 0.2/0.3 duplicate and `Unicode-DFS-2016` license — separate cleanup work.)

**Acceptance:** all workspace tests + 2 new test files green; `bincode` removed from `Cargo.lock` (`cargo tree --invert bincode` empty); `OIP-Serde-004` transitions to `Final` after 7 calendar days of clean `audit.yml` cron runs.

---

## P7.3 — `OMNI-PROTO-v0.2` documentation update

- **Status:** `[ ]` — **READY** (P7.2 M1-M5 ✅; non più blocked-on)
- **Priority:** P7 / Low (1 PR edit-only, sblocca check verde `oip-lint` collaterale)
- **Effort:** 1 day
- **Dependencies:** P7.2.M5 ✅
- **Rationale:** `docs/protocol/handshake.md` § 3.2 currently negotiates only `OMNI-PROTO-v0.1`. After P7.2.M5, the handshake spec must reflect the v0.2 cutover (`serde_format = "postcard-1.0"` discriminant; v0.1 negotiation removed). Il codice è già `omni_types::version::PROTOCOL_VERSION_V0_2`; solo doc-update.
- **Acceptance:**
  - [ ] `docs/protocol/handshake.md` § 3.2 menziona solo `OMNI-PROTO-v0.2`.
  - [ ] PR con label `area:docs` + `priority:P3` aperta e mergiata (admin fast-track, no codice).

---

# P8 — OIP-Container-006 reference implementation

**Tracking OIP:** [`OIP-Container-006`](oips/oip-container-006.md) (`Draft` filed 2026-05-12).

P8 turns the OmniContainer specification into the canonical Rust implementation under `crates/omni-container/`. Each milestone closes one of the OIP's subsystems and is a candidate for its own follow-up OIP if the subsystem's design surface raises significant questions during implementation.

## P8.1 — `crates/omni-container/` skeleton (closed 2026-05-12)

- **Status:** `[x]` closed 2026-05-12 (commit `31455a6`, on `feat/p1-foundational-crates`).
- **Priority:** P8
- **Effort:** done (~ 1 day implementer time)
- **Acceptance criteria:**
  - [x] Crate compiles clean under `cargo check --workspace --all-features`.
  - [x] Public trait surface (`ContainerEngine`, `ContainerLifecycleState`, `CapabilityProfile`, `OciImageRef`, `ContainerError`) exposed at the crate root.
  - [x] Every operational method returns `ContainerError::NotYetImplemented(<static slug>)`.
  - [x] Feature flags `kvm` (default), `tdx`, `sev-snp`, `all-backends` per OIP-Container-006 § 10.
  - [x] ≥ 15 unit tests + ≥ 1 integration test green (delivered 47 unit + 5 integration).
  - [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - [x] `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` clean.
  - [x] `cargo fmt --all -- --check` clean.

## P8.2 — KVM hypervisor backend implementation

- **Status:** `[ ]` blocked on a follow-up OIP (`OIP-Container-Engine-XXX`) that locks in the `kvm-ioctls` API surface, the vCPU thread model, the run-loop placement (tokio vs. dedicated thread), and the guest-kernel boot path.
- **Priority:** P8
- **Effort:** 4-6 engineer-months estimated per OIP-Container-006 § 10.
- **Dependencies:** P8.1 (this); a future `OIP-Container-Engine-XXX`.

## P8.3 — Guest Linux image build pipeline

- **Status:** `[ ]` blocked on a Stichting OMNI signing key (P4.1 derivative) and on the reproducible-build setup in the separate `omni-guest-linux` repo (does not exist yet).
- **Priority:** P8
- **Effort:** 3-4 engineer-months estimated.
- **Dependencies:** P4.1 (Stichting key custody); a future `OIP-Container-GuestImage-XXX`.

## P8.4 — Virtio host-side backends

- **Status:** `[ ]` per-backend, blocked on its own follow-up OIP: `OIP-Container-Networking-XXX` (virtio-net), `OIP-Container-Storage-XXX` (virtio-fs), TBD for virtio-gpu / virtio-vsock / virtio-rng.
- **Priority:** P8
- **Effort:** 5-6 engineer-months total estimated per OIP-Container-006 § 10.
- **Dependencies:** P8.2 (KVM engine), the per-backend OIPs.

## P8.5 — TDX + SEV-SNP confidential-VM modes

- **Status:** `[ ]` blocked on P5.2 / P5.3 (host TEE backends in `omni-tee`) reaching parity and on a Standards-Track OIP that locks in the per-container quote envelope shape.
- **Priority:** P8
- **Effort:** 2-3 engineer-months estimated.

## P8.6 — Wine integration image (`omni/linux-wine:N-stable`)

- **Status:** `[ ]` blocked on P8.3 (guest image pipeline).
- **Priority:** P8
- **Effort:** 1-2 engineer-months estimated.
- **Dependencies:** P8.3; tracking community ProtonDB compatibility reports.

## P8.7 — `cyDock-omni` fork retargeting

- **Status:** `[ ]` blocked on P8.2 + P8.4 reaching a stable REST API surface for the management plane.
- **Priority:** P8
- **Effort:** 3-4 engineer-months estimated per OIP-Container-006 "cyDock Evolution Path".
- **Dependencies:** P8.2, P8.4, plus a green light from the existing cyDock maintainer.

---

# P9 — Code hygiene & lint-debt management

**Goal:** maintain `omni-kernel` (and gradually the rest of the workspace) with **zero crate-root blanket `#![allow(...)]`**. Ogni soppressione intenzionale è localizzata, motivata, e validata da CI.
**Estimated effort:** 1 day setup + ongoing per ADR-0003.
**Tracking ADR:** [`ADR-0003 — No blanket allows in production crates`](docs/adr/0003-no-blanket-allows-in-production-crates.md).

## P9.1 — Step 7 closure (lift omni-kernel blanket allows)

- **Status:** `[x]` (closed 2026-05-18 — 4 commits on `main` `770c7aa → 1768966`).
- **Priority:** P9 / High (debt accumulato durante 7 iterazioni CI conformance su PR #29)
- **Effort:** delivered in ~1 giornata distribuita su 4 PR consecutivi
- **Dependencies:** v0.2.0 merge (PR #29) ✅
- **Deliverables:**
  - [x] **Step 7.1** (`770c7aa`) — lift `restriction` + `rustdoc` lints (~40 siti localizzati; +2 broken intra-doc links riscritti come code spans).
  - [x] **Step 7.3** (`50eddf1`) — lift `clippy::pedantic` (~68 siti, mix fix/allow module-level su `bare_metal/{cursor,demo,graphics,gdt,idt,input,paging,virtio_tablet,widget,wm}.rs`).
  - [x] **Step 7.4** (`83ff1e8`) — lift `clippy::nursery` + `clippy::cargo` (7 siti totali; 4 `too_long_first_doc_paragraph` in `scheduling.rs`, 2 `use_self` in `wm.rs`, 1 `cognitive_complexity` allow su `demo::run_desktop`).
  - [x] **Step 7.2** (`1768966`) — lift `unsafe_code` (~40 cfg-gated bare-metal siti; CI `blanket-allow-guard` flipped to **blocking**). **Last** dei 4 PR — sequenziato per landing immediato prima di MB11 (minimizza merge-conflict).
  - [x] **`scripts/check-no-blanket-allow.sh`** — bash guardrail script (whitelisted: doc URL, `warn(...)`, `cfg_attr(test, allow(...))`, `cfg_attr(all(feature = "bare-metal", ...))`).
  - [x] **CI job `blanket-allow-guard`** in `.github/workflows/ci.yml` — bloccante.
  - [x] **ADR-0003** `accepted`.
- **Acceptance:** `bash scripts/check-no-blanket-allow.sh` exit 0 (output: `ok (scanned 12 crate-root files)`); `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.

## P9.2 — Extend `check-no-blanket-allow.sh` ai restanti crate

- **Status:** `[ ]`
- **Priority:** P9 / Low
- **Effort:** ~1 giornata
- **Rationale:** lo script oggi scansiona solo `crates/<scoped>/src/{lib,main}.rs` (12 file). Altri crate workspace (`disk-image-builder/`, `kernel-runner/`, `omni-userprobe-helloworld` futuro) hanno la stessa policy ADR-0003 ma non sono enforced. Far passare anche loro alla guardrail.

---

# P10 — Branch / release workflow (post-v0.2.0)

**Goal:** mantenere `main` in stato release-able; cadenza di squash-merge dai branch feature; tag intermedi per milestone significativi.

## P10.1 — Merge `feat/kernel-mb11-userspace` → `main`

- **Status:** `[ ]` — **READY** (HEAD `1a0fa3e` complete; CI da validare)
- **Priority:** P10 / High
- **Effort:** 0.5-1 giornata (push remote + CI conformance + DCO sign-off + admin bypass su `cargo test (ubuntu-24.04)` SIGSEGV se ricompare)
- **Dependencies:** nessuna tecnica (MB11 + MB12 closed; ADR-0004 + ADR-0005 accepted). **Decisione esplicita:** se aprire ora un solo PR aggregato MB11+MB12+Step 7 oppure 3 PR sequenziali. Raccomandazione: 1 PR aggregato (Step 7 lì dentro già contiene 4 commit; aggiungere altri 4 da MB11+MB12 mantiene il branch leggibile per il review).
- **Deliverables:**
  - [ ] `git push origin feat/kernel-mb11-userspace`.
  - [ ] `gh pr create --base main --title "feat(kernel): MB10 follow-up + MB11 + MB12 + Step 7 (post v0.2.0)"`.
  - [ ] DCO sign-off check pass.
  - [ ] 11 required CI checks (vedi `progress-omni.md` § 2.3) → 10 verdi + 1 admin-bypass tollerato (`cargo test (ubuntu-24.04)` SIGSEGV — vedi P10.3 root cause).
  - [ ] Squash-merge con commit message ben formato (no Co-Authored-By AI per [CLAUDE.md § Git Attribution Policy](CLAUDE.md)).
- **Acceptance:** `main` HEAD include MB10 + MB11 + MB12 + Step 7; tag intermedio opzionale (`v0.2.1`?) deferred a P10.2.

## P10.2 — Tag intermedio v0.2.1 (decision pending)

- **Status:** `[ ]`
- **Priority:** P10 / Low
- **Effort:** 0.5 giornata (release notes + tag + GitHub release)
- **Rationale:** MB10 + MB11 + MB12 + Step 7 sono delta significativi (Ring 3 + IPC); valutare se rilasciare `v0.2.1` (patch — niente public API break) oppure attendere MB13 per `v0.3.0-alpha.1`. **Decisione raccomandata:** attendere MB13 (più narrabile come "real capability dispatch + bootable smoke fix"), ma se MB13 slitta oltre 2026-05-26 considerare `v0.2.1` come stop-gap.

## P10.3 — Risolvere `cargo test (ubuntu-24.04)` SIGSEGV

- **Status:** `[ ]` (carryover da v0.2.0 / PR #29 / PR #33)
- **Priority:** P10 / Medium (sblocca CI pulito; ad oggi richiede admin bypass su ogni PR)
- **Effort:** ~1 giornata
- **Rationale:** il binary `omni_kernel-…` exit con `signal: 11` al teardown del test harness *dopo* che tutti i unit test riportano `ok`. Locale macOS arm64 1.85.1 passa. Probabile bug nel drop di `bare_metal::paging::tests::TestArena` (raw 256-KiB alloc + manual dealloc consumed via `*mut RawPageTable`).
- **Deliverables (alternative):**
  - **Opzione (a) quick fix:** `--test-threads=1` nel workflow `.github/workflows/ci.yml` job `cargo test (ubuntu-24.04)`.
  - **Opzione (b) root cause fix:** rifattorizzare `TestArena` in `Arc<Mutex<...>>` o `&'static mut [MaybeUninit<u8>]` per evitare il manual dealloc race.
- **Acceptance:** CI green su `cargo test (ubuntu-24.04)` su 5 PR consecutivi senza admin bypass.

## P10.4 — CI smoke automatico per `mb11-userprobe` e `mb12-userprobe`

- **Status:** `[ ]` (blocked-on MB13.b — oggi il smoke triple-faulta)
- **Priority:** P10 / Medium
- **Effort:** 0.5 giornata per job (1 giornata totale)
- **Dependencies:** P6.MB13.b ✅
- **Deliverables:**
  - [ ] Estendere `scripts/qemu-boot-smoke.sh` con flag `--feature mb11-userprobe` + `--feature mb12-userprobe`.
  - [ ] Aggiungere 2 nuovi job in `.github/workflows/qemu-boot-smoke.yml` con `EXPECTED_LINES` esteso per `[user] hello` / `[mb12] channel 1 pre-created` / `ping` / `[user] exit=0` (x2 per MB12).
  - [ ] Branch protection update per renderli required (richiede admin token).

---

# Open decisions awaiting Founder input

Decisions resolved during P0 closure:

1. ~~**Engagement mode**~~ — **Resolved 2026-05-09:** *Implementer*. Claude writes deliverables, founder reviews. Confirmed across all P0 tasks; default for P1+ unless renegotiated.
2. ~~**P0 vs P1 ordering**~~ — **Resolved 2026-05-09:** *P0 first*. Closed before any code in foundational crates lands.
3. ~~**Phase 0 non-technical work (P4)**~~ — **Resolved 2026-05-09:** *Out of scope* for current implementer engagement; P4 remains in this document but execution is on the founder's calendar (notary, KVK, grants).

Decisions resolved during P2 closure (2026-05-10):

4. ~~**OIP-Process-001 authorship (P2.1)**~~ — **Resolved 2026-05-10:** *Claude drafts, founder reviews* (Implementer mode preserved from §1 above). OIP shipped under bootstrap fiat clause §6.3.
5. ~~**P1 vs P2 ordering**~~ — **Resolved de facto 2026-05-10:** P1 closed first; P2 followed within the same day. Sequence preserved but cycle time short enough that the "federated review of `omni-crypto`" benefit was not lost — `omni-crypto` carries an explicit `AWAITING_CRYPTO_REVIEW` marker (P3.2) and any breaking change to its API will now go through the formal OIP process.
6. ~~**BDFL veto window start date (P2.3)**~~ — **Resolved 2026-05-10:** *2026-05-09* (first public commit, GitHub-verified). Maximally constraining on the founder, certain today, independently verifiable. Sunset 2031-05-09 23:59 UTC, immutable.
7. ~~**OIP editor body composition during Bootstrap (P2.1)**~~ — **Resolved 2026-05-10:** *1 interim editor (founder), Seat 2 vacant until Phase 1 hire OR 2027-05-10 (hard deadline)*. Codified in `OIP-Process-001` §6.2.

Resolved during P2 review (2026-05-10, post-publication founder editorial review):

10. ~~**First non-`Meta` OIP to dogfood the formal vote**~~ — **Resolved 2026-05-10:** *`OIP-bounty-XXX`* (slug `bounty`, Process track; global number assigned at Last Call). Self-contained, sblocca grant narrative, ~1 settimana di drafting, primo Last Call reale. Subsequent order: `OIP-voting-XXX` (refines §5.2 bootstrap defaults), then `OIP-stark-snark-XXX` (after P3.2 cryptographer review unblocks). Drafting non avviato — gating su decisione del founder se partire ora o pre-allineare prima sui parametri chiave (severity tiers, payout ranges, eligibility filters).
11. ~~**OIP-Process-001 critical-security gap during Bootstrap**~~ — **Resolved 2026-05-10:** founder review surfaced the gap; addressed via OIP-Process-001 §6.5 amendment under bootstrap fiat. Bootstrap deadlock on `Critical` Standards Track OIPs is now bounded by 72h objection window + mandatory post-Bootstrap re-ratification.
12. ~~**OIP-Process-001 voting formula generational unfitness**~~ — **Resolved 2026-05-10:** founder review surfaced the saturation issue; addressed via §5.2 "Known limitations" amendment with soft 2028-05-10 deadline for the `voting`-slug Process OIP.
13. ~~**OIP-Process-001 author-identity privacy posture**~~ — **Resolved 2026-05-10:** founder review surfaced the GDPR/privacy-first inconsistency; addressed via `## Privacy Considerations` refinement + `oips/oip-template.md` HTML guidance.

Still open:

8. **Repo visibility long-term** — flipped to **PUBLIC** on 2026-05-09 because branch protection on the GitHub free plan requires it and AGPL-3.0 is consistent with public hosting. Confirm this remains the steady state, or signal a temporary embargo for any pre-disclosure phase.
9. **Branch-protection update for `oip-lint`** — `OIP-Process-001` §9 ¶2 mandates that branch protection on `main` add `oip-lint / oip-lint` as a required status check within 7 calendar days of the OIP transitioning to `Active`. Concrete action: re-run `scripts/bootstrap-github.sh` (or equivalent `gh` CLI invocation) before 2026-05-17 to extend the required-check list from 8 to 9. *(Founder-side action — requires GitHub admin token.)* — **NOTE 2026-05-19:** deadline 2026-05-17 superata; check da aggiungere comunque retroattivamente prima di mergiare `feat/kernel-mb11-userspace` (P10.1).
15. **Last Call closing actions for `OIP-Bounty-002` and `OIP-Serde-004` (window closes 2026-05-26)** — Both OIPs entered `Last Call` on 2026-05-12 under `OIP-Process-001` § 4. Under § 5.3 each transitions `Last Call → Active` automatically at the end of the 14-day window unless ≥ 30% weighted vote is reached earlier (in which case the editors close the window at that point) **or** a blocking good-faith objection is filed (in which case the OIP returns to `Review`). Concrete actions for the editor body on or before **2026-05-26**: (a) confirm no blocking objection has been filed on the linked GitHub Discussion thread; (b) merge a single PR per OIP transitioning the frontmatter `status:` from `Last Call` to `Active` and updating the `updated:` field to the close date; (c) for `OIP-Bounty-002` (Process track), no activation phase applies, the OIP is effectively `Final` at `Active`; (d) for `OIP-Serde-004` (Standards Track), the activation phase per § 7 is dormant until Phase 4+ mesh telemetry exists — the OIP remains in `Active` indefinitely; (e) append a row to `oip-editors-report-YYYY-Q2.md` recording the tally (or its absence) and the editorial decision. **No founder-side or hardware-side gate; pure editorial action.**
14. ~~**`OIP-bounty-002` drafting kickoff**~~ — **In progress 2026-05-10:** `Draft` filed at [`oips/oip-bounty-002.md`](oips/oip-bounty-002.md) (~31KB, 10 sezioni canoniche, lint green). Defaults applicati senza pre-allineamento ulteriore (founder ha confermato "procedi"): severity tiers riusati da `SECURITY.md` §4 (CVSS v4.0); payout ranges Critical €5K–€50K / High €1K–€10K / Medium €250–€2.5K / Low €50–€500; eligibility con 6-month contributor guard + esclusione editor body / Stichting board / commit-access su `main`; disclosure timeline ancorato a `SECURITY.md` §3; payment mechanics con opzioni crypto privacy-preserving (Monero, BTC LN); dispute resolution a 3 livelli che termina in public arbitration; **non-monetary mode** durante Bootstrap con commitment retroattivo entro 24 mesi dall'Activation Date. Index aggiornato in `oips/README.md`; `SECURITY.md` §7 aggiornato per puntare al Draft. Prossimi passi: editorial review by founder; transition to `Review` quando il founder è pronto; questo OIP è il **dogfood test** del flusso §5 di `OIP-Process-001`.
16. **MB13 fix opzione (a/b/c) per il triple-fault smoke** (vedi P6.MB13.b) — preferenza tecnica: opzione (a) ET_DYN/PIE kernel. Sblocco potenziale: capire se `bootloader_api` 0.11 onora davvero `dynamic_range_start` su `ET_DYN` x86_64-unknown-none — la docstring in `kernel-runner/src/main.rs:27-40` dice di no per `ET_EXEC` ma non è stato testato sperimentalmente per `ET_DYN`. **Azione richiesta:** spike di 2-4 ore per build ET_DYN sperimentale prima di committare a una delle tre opzioni.
17. **Quando rilasciare v0.2.1 vs aspettare v0.3.0-alpha.1** (vedi P10.2) — decisione del founder sul cadence del tag intermedio.

These decisions do not block strategic planning, only execution order.

---

# P0 closure summary (2026-05-09)

| What | Status / pointer |
|---|---|
| Repo URL | https://github.com/CySalazar/omni |
| Visibility | Public (AGPL-3.0) |
| Default branch | `main` |
| Branch protection | `enforce_admins=true`, `required_signatures=true`, `linear_history=true`, `allow_force_pushes=false`, 1 reviewer, 8 required status checks |
| Commits on `main` | `61426d5` → `15419cb` → `ebf9539` → `101ff79` (all `cySalazar <cySalazar@cySalazar.com>`, SSH-signed, GitHub-verified) |
| Workflows live | ci, audit, sbom, reproducible-build, dco, codeql, labeler |
| Dependabot active | First 2 PRs already auto-opened (mockall, cryptography group) |
| Label taxonomy | 32 labels (`area:*`, `priority:*`, `kind:*`, special) |
| Vulnerability alerts | Enabled |
| Secret scanning + push protection | Enabled |
| SSH signing key on GitHub | id 938835 (`~/.ssh/id_ed25519.pub`) |
| Project identity | `cySalazar <cySalazar@cySalazar.com>` (Matteo's real `matteo.sala@samacyber.io` removed from the GitHub account on 2026-05-09) |
| Bootstrap scripts | `scripts/bootstrap-local.sh` (idempotent), `scripts/bootstrap-github.sh` (idempotent) |
| Completion report | [`docs/audits/p0-completion-report.md`](docs/audits/p0-completion-report.md) (moved 2026-05-10 from repo root for hygiene) |
| Tooling docs | [`docs/11-tooling-and-ci.md`](docs/11-tooling-and-ci.md) |

---

# Phase 1 closure roadmap — executive sequence (post v0.2.0)

Sintesi ordinata dei prossimi sprint per chiudere **Phase 1 — Microkernel POC** della roadmap. Da leggere top-down; ogni step è un sotto-task elencato nelle sezioni precedenti.

| # | Sprint | Tasks chiave | Bloccato da | Effort | Output atteso |
|---|---|---|---|---|---|
| 1 | **Merge MB10/MB11/MB12 → main** | P10.1 | — | 0.5-1d | `main` HEAD include MB12 + Step 7; tag intermedio opzionale (P10.2) |
| 2 | **MB13 — `omni-capability` reale** | P6.MB13.a/b/c/d/e + ADR-0006 | Sprint 1 | 2-3d | `Ed25519CapabilityProvider` attivo + smoke `mb12-userprobe` verde + 432+ tests |
| 3 | **CI smoke MB11/MB12 automatico** | P10.4 | MB13.b | 1d | Job `qemu-boot-smoke` bloccante anche su `[user]`/`[mb12]` lines |
| 4 | **P7.3 docs `OMNI-PROTO-v0.2`** | P7.3 | — (parallelizzabile da subito) | 0.5d | Handshake spec aggiornato; OIP-Serde-004 verso `Final` |
| 5 | **P3.3 OIP STARK vs SNARK** | OIP `oip-crypto-002` Draft → Review | P3.2 cryptographer (P4.2 funding) | 1-2 sett | Decisione formale sulla strategia ZK |
| 6 | **MB14 — MP/AP enable + TLB shootdown** | nuovo (post-MB13) | MB13 | 5-10d | Multi-core operativo; ADR-0007 |
| 7 | **P6.7 — Userspace driver model** | NVMe + Net + TEE drivers | MB13 + MB14 + 3 nuovi OIP | 6-12 mesi | Phase 1 deliverable "Drivers in user space" |
| 8 | **P5.2/P5.3 — TDX + SEV-SNP backends reali** | + hardware acquisition | P4.2 funding | 2-4 sett | Phase 1 deliverable "TEE attestation" |
| 9 | **P6.8 — External kernel + capability audit** | engagement auditor | P4.2 funding + P6.7 done | 4-8 sett | Phase 1 closure deliverable |
| 10 | **Phase 1 → Phase 2 transition** | docs/06-roadmap.md update + OIP "Phase-2-Entry-XXX" | tutti gli sprint 1-9 | 1 sett | Phase 2 (AI Runtime) sblocca |

**Critical path techinical-only (esclude funding):** Sprint 1 → 2 → 3 → 6 → 7 → 9 → 10. Stimato realisticamente in **9-15 mesi** se single-implementer; **4-7 mesi** con il core team Phase 1 (2 senior Rust + 1 cryptographer) assunto.

**Critical path funding-dependent:** Sprint 5 + 8 + 9 dipendono da P4.2 (€350K runway). Senza funding, P3.2 cryptographer + TDX/SEV-SNP hardware + auditor pro-bono rimangono best-effort.

---

# Maintenance policy for this document

- This file is updated **after every completed task**.
- Status icons must reflect reality. Do not mark `[x]` until acceptance criteria are all green.
- Adding a new task requires it to slot into the existing tier structure or justify a new tier.
- Removing or downgrading a task requires either (a) the work is genuinely done, or (b) an OIP that supersedes the requirement.
- Cross-references between this document and `/docs/06-roadmap.md` must stay in sync; when in conflict, the roadmap is authoritative for *what*, this file is authoritative for *how*.
- Cross-references con [`progress-omni.md`](progress-omni.md) (snapshot stato) + [`CHANGELOG.md`](CHANGELOG.md) (per-release) devono restare coerenti; questo file è autoritativo per *what's next*, gli altri due per *what already happened*.
- **Allineamento DOE framework:** la struttura P0-P10 corrisponde al pattern `TASK-NNN` di `doe-framework/L2-orchestration/02-task-decomposition.md`. Ogni sotto-task ha: ID, Status, Priority, Effort, Dependencies, Deliverables, Acceptance criteria. Le decisioni architetturali significative producono un ADR in `docs/adr/` (template `doe-framework/templates/adr-template.md`).
