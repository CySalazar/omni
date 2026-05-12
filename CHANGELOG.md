# Changelog

All notable changes to OMNI OS are documented in this file.

The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

OMNI OS distinguishes two version streams:

- **OS version** (`MAJOR.MINOR.PATCH`) — the distribution release.
- **Mesh protocol version** (`OMNI-PROTO-vMAJOR.MINOR`) — negotiated at handshake. Decoupled from the OS version (see [`docs/09-tech-specifications.md`](./docs/09-tech-specifications.md) § "Versioning policy").

Each entry below tracks the OS version. Protocol-version changes get their own bullet inside the OS-version entry that introduces them.

---

## [Unreleased]

### Added

- **P3 — Threat model deepening & cryptographic peer review preparation (scaffolding).**
  - [`docs/protocol/handshake.md`](./docs/protocol/handshake.md) — formal wire-level spec of the mesh handshake (Noise_IK over QUIC with mandatory mutual TEE attestation). Documents invariants I1–I8 and lists 5 open issues for cryptographer review.
  - [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) — Tamarin model with `mutual_authentication`, `forward_secrecy`, `replay_resistance`, `kci_resistance`, and `protocol_version_binding` lemmas. Proof execution deferred to P3.2 cryptographer engagement.
  - [`docs/audits/cryptographer-engagement-template.md`](./docs/audits/cryptographer-engagement-template.md) — paid (USD 8–15k) and volunteer engagement modes; scope, deliverables, selection criteria, and engagement-letter templates.
  - [`/CONTRIBUTORS.md`](./CONTRIBUTORS.md) — initial contributor roster with placeholder rows for OIP Editors, Cryptographer (P3.2), and Stichting Trustees.
  - [`/oips/oip-crypto-002.md`](./oips/oip-crypto-002.md) — `Draft`. Compliance proof scheme: **`sig-v1` mandatory baseline + optional `stark-v0`** for v1.0; STARK chosen over SNARK to avoid trusted setup. `winterfell` v0.10+ as reference. SNARK explicitly deferred.
  - [`docs/04-security-model.md`](./docs/04-security-model.md) §"Compliance proofs" updated to reference `OIP-Crypto-002` decision; "Open security questions" item on zk-SNARK trusted setup marked resolved.
- **P4 — Phase 0 non-technical (Stichting + funding) drafts.**
  - [`docs/legal/bylaws-draft.md`](./docs/legal/bylaws-draft.md) — Stichting OMNI bylaws working English draft (15 articles + 3 appendices). Mission Anchor (Article 3) **immutable** by construction. Founder role with sunset 2026-05-09 → 2031-05-09.
  - [`docs/legal/stichting-checklist.md`](./docs/legal/stichting-checklist.md) — 6-phase execution checklist for Dutch notary + KVK registration + ANBI application.
  - [`docs/funding/pitch-deck.md`](./docs/funding/pitch-deck.md) — 15-slide markdown pitch deck.
  - [`docs/funding/one-pager.md`](./docs/funding/one-pager.md) — short one-pager for warm intros.
  - [`docs/funding/grant-applications/`](./docs/funding/grant-applications/) — drafts for **NLnet** (EUR 50k), **Mozilla MOSS** (USD 200k), **Sloan** (USD 300k / 18 months), **Open Philanthropy** (USD 500k / 24 months).
  - [`docs/funding/sponsor-tier-menu.md`](./docs/funding/sponsor-tier-menu.md) — Bronze / Silver / Gold / Platinum sponsorship tiers, anti-capture safeguards explicit.
  - [`docs/08-funding-policy.md`](./docs/08-funding-policy.md) bumped to **Draft v0.2**: cross-references to bylaws Article 3 (Mission Anchor) and Article 9.1 (30% diversification rule); preliminary founder view on NLnet recorded non-bindingly.
  - [`docs/hiring/role-rust-engineer-kernel.md`](./docs/hiring/role-rust-engineer-kernel.md), [`role-rust-engineer-networking.md`](./docs/hiring/role-rust-engineer-networking.md), [`role-cryptographer.md`](./docs/hiring/role-cryptographer.md) — 3 role descriptions for post-Phase-0 hires.
  - [`docs/hiring/salary-bands.md`](./docs/hiring/salary-bands.md) — public salary bands L1–L5 + D, with geographic-adjustment formula.
- **P5 — `omni-tee` (TEE HAL) production-ready trait surface.**
  - `crates/omni-tee/src/traits.rs` — `TeeBackend` trait with `attest`, `verify_quote`, `seal`, `unseal`, `derive_key_for`. `TeeFamily` enum (Intel TDX / AMD SEV-SNP / Apple Secure Enclave / ARMv9 CCA / Mock). `TeeError` + `TeeErrorKind` taxonomy with PII-safe static context slugs.
  - `crates/omni-tee/src/attestation.rs` — `Quote`, `Measurement` (48-byte cross-vendor), `Nonce`, `QuoteVersion`.
  - `crates/omni-tee/src/sealed_keys.rs` — `SealedBlob`, `SealPolicy`, `TeeSharedKey` (with redacted `Debug` and zeroize-on-Drop via volatile writes).
  - `crates/omni-tee/src/mock.rs` — `MockTeeBackend` with permissive and strict modes; deterministic in-process behavior for use by every other crate's tests. End-to-end unit tests covering attest/verify/seal/unseal/derive happy paths + 7 adversarial scenarios.
  - `crates/omni-tee/src/tdx.rs` (feature `tdx`) — Intel TDX backend scaffold with documented P5.2 integration roadmap.
  - `crates/omni-tee/src/sev_snp.rs` (feature `sev-snp`) — AMD SEV-SNP backend scaffold with documented P5.3 integration roadmap.
  - `crates/omni-tee/tests/mock_integration.rs` — end-to-end handshake simulation across two `MockTeeBackend` instances; replay, tampering, and cross-family negative tests.
  - `crates/omni-tee/Cargo.toml` — feature flags `default = ["mock"]`, `mock`, `tdx`, `sev-snp`, `all-backends`.
  - `crates/omni-hal/src/lib.rs` and `Cargo.toml` — `tee` module re-exports the full `omni-tee` surface; feature flags forwarded so `omni-hal/tdx` transitively enables `omni-tee/tdx`.
- **P6 — `omni-kernel` no_std-ready scaffolding + UEFI bootloader OIP.**
  - `crates/omni-kernel/Cargo.toml` — `bare-metal` feature flag.
  - `crates/omni-kernel/src/lib.rs` — `#![cfg_attr(feature = "bare-metal", no_std)]` + `KernelError` + `KernelResult<T>`.
  - `crates/omni-kernel/src/memory.rs` — `PhysAddr` / `VirtAddr` / `PageSize` / `PageFlags` + `Allocator` and `PageTable` trait skeletons. In-crate `bitflags_simple!` macro avoids the `bitflags` crate dependency at this layer.
  - `crates/omni-kernel/src/scheduling.rs` — `TaskId` / `PriorityClass` (System / RealTime / Interactive / **AiInference** / Background / Idle) / `TaskState` / `Scheduler` trait.
  - `crates/omni-kernel/src/ipc.rs` — `ChannelId` / `MessageKind` / `BackpressurePolicy` / `ChannelPolicy` (`tee_bound: bool`) / `MessageEnvelope` / `Ipc` trait.
  - `crates/omni-kernel/src/capabilities.rs` — `KernelCapabilityId` (u128) + `CapabilityTable` trait. Bridges to the userspace token in `omni-capability`.
  - `crates/omni-kernel/src/syscall.rs` — **stable numeric ABI** for syscalls (mem 1–9, task 10–19, ipc 20–29, cap 30–39, tee 40–49, time 50+). Renumbering is a breaking change requiring an OIP.
  - [`/oips/oip-kernel-003.md`](./oips/oip-kernel-003.md) — `Draft`. UEFI-only boot; **`bootloader` crate v0.11+** selected as reference bootloader for v1.0 (over Limine, GRUB2, custom `uefi-rs`); 5-step `no_std` transition plan (K1–K5).

### Changed

- [`docs/05-governance.md`](./docs/05-governance.md) bumped to **Draft v0.2** with explicit changelog (OIP-Process-001 delegation, BDFL veto immutable window 2026-05-09 → 2031-05-09, founder role years 1–5 / 5+ / 10+).
- [`docs/README.md`](./docs/README.md) — new "Subdirectories" section indexing `/docs/protocol/`, `/docs/audits/`, `/docs/legal/`, `/docs/funding/`, `/docs/hiring/`, `/oips/`, `/protocol-proofs/`.
- `Cargo.toml` `[workspace.package].authors` — aligned to project identity policy: `cySalazar <cySalazar@cySalazar.com>` until Stichting OMNI is constituted (was: `Stichting OMNI <hello@omni-os.org>` placeholder). Transition to Stichting documented in-file.
- `P0-COMPLETION-REPORT.md` moved to [`docs/audits/p0-completion-report.md`](./docs/audits/p0-completion-report.md) (lowercase, kebab-case, in audits directory); `todo.md` cross-reference updated.

### Added

- **2026-05-12 — Tamarin model extended to v0.2 (lemmas for I3 / I7-extended / I8).** [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) gains three new lemmas matching invariants documented in [`docs/protocol/handshake.md`](./docs/protocol/handshake.md) § 2:
  - `mutual_tee_attestation_binding` (I3): an Established session implies the peer's TEE measurement was signed in the M2 transcript by the peer's long-term key.
  - `measurement_root_binding` (I7-extended): the Merkle root of the measurement allowlist advertised in M1/M2 cannot be substituted by an attacker without invalidating the signature.
  - `compliance_capability_no_downgrade` (I8): the negotiated capability set (`caps_intersect(capsA, capsB)`) accepted by A matches a negotiation B actually performed.

  Model rule extensions: new functions `mr_root/1` and `caps_intersect/2`; `Initiator_Send_M1` and `Responder_Receive_M1_Send_M2` now bind `mr_root(allowlist)` and `caps` into the signed transcript; `Initiator_Receive_M2_Send_M3` carries `Bound_Measurements` / `Bound_Roots` / `Verified_Caps_Negotiation` action labels; `Responder_Receive_M3` carries the negotiated capability set into `!Session_Resp`. Proof execution itself remains P3.2-blocked (requires `tamarin-prover` install + cryptographer engagement); the lemma definitions and model rules are now in place to be verified.

  `todo.md` P3.1 status transitioned `[ ]` → `[~]` with per-lemma checkbox tracking; spec + Tamarin artefact deliverables ticked `[x]`; tamarin-prover execution + cryptographer review remain `[ ]`.

- **2026-05-12 — `OIP-Kernel-004` (Draft).** Standards-Track OIP defining the panic handler and global allocator for the bare-metal `omni-kernel`, closing gate K3 of `OIP-Kernel-003` § 3. Specifies the non-allocating, interrupt-disabled, halt-on-completion panic handler with a structured `PanicRecord` encoded via `omni-types::wire` (postcard, per `OIP-Serde-004`); the in-crate bump global allocator (no external dep, ~80 LOC, O(1) allocation, no `dealloc`); and the heap region provisioning stub that `OIP-Kernel-005` will replace with the formal `BootInfo` field. Closes the `cargo build --target x86_64-unknown-none --features bare-metal` failure that currently blocks K3 advancement. 5-step migration plan (K3.a–K3.e) with a new CI build target. Inline `requires:` declares the dependency on both `OIP-Process-001` and `OIP-Kernel-003`.

- **2026-05-12 — `OIP-Serde-004` (Draft).** Standards-Track OIP proposing the migration of the workspace serialization layer from `bincode` v2.0 (unmaintained per RUSTSEC-2025-0141) to `postcard` 1.x. Specifies the canonical-encoding helper module (`omni-types::wire`), the breaking wire-format change (`OMNI-PROTO-v0.1` → `OMNI-PROTO-v0.2`), and a 5-step migration plan (M1–M5) with per-step verification gates. Includes a 6-candidate selection matrix (postcard / bitcode / rkyv / wincode / bincode 1.3.3) ranked against six weighted criteria, with `postcard` as the only candidate satisfying all four hard requirements (`no_std + alloc + serde derive`, active maintenance, stable wire-format spec, audit history).
- **2026-05-12 — `todo.md` P7 tier.** New tier "Workspace serialization migration" with three task groups: P7.1 (`OIP-Serde-004` Last Call closure), P7.2 (M1–M5 migration commits), P7.3 (`OMNI-PROTO-v0.2` documentation update). Tracking gate for `cargo audit` / `cargo deny` returning to clean.
- **2026-05-12 — `oips/README.md` index.** Catches up on the three Draft OIPs filed in the 2026-05-10 scaffolding pass that were not added to the table at the time (`OIP-Crypto-002`, `OIP-Kernel-003`) plus the new `OIP-Serde-004`. Documents the duplicate `002` numbering between `OIP-Bounty-002` and `OIP-Crypto-002` as an acceptable Draft-stage placeholder collision per `OIP-Process-001` § 5 (editors reconcile global numbers on Last Call → Active).

- **2026-05-12 — `OIP-Kernel-005` (Draft).** Standards-Track OIP closing gate K4 of `OIP-Kernel-003` § 3: specifies the boot hand-off ABI between the bootloader and `omni-kernel`, and introduces the **`kernel-runner/` crate** (sibling to `crates/`, not under it) that owns `_start`, the `bootloader_api` / `bootloader` v0.11+ build glue, and the QEMU run configuration. § S5 replaces the K3 `OMNI_KERNEL_HEAP_BASE` / `OMNI_KERNEL_HEAP_LEN` extern-symbol stubs with the in-kernel `pick_region(boot_info.memory_regions)` selector (largest Usable contiguous region ≥ `MIN_HEAP_BYTES = 4 MiB`; tie-break by lowest start address; panic on no-eligible-region). § S4 commits the kernel to using/binding `memory_regions`, `framebuffer`, `rsdp_addr`, `physical_memory_offset`; other `BootInfo` fields are diagnostic-only. § S9 pins `bootloader` / `bootloader_api` at `=0.11.X` for reproducible boot images. K4 follows K3 (`OIP-Kernel-004`) and unblocks K5 (QEMU smoke test).

- **2026-05-12 — `OIP-Voting-005` (Draft).** Process-track OIP retiring the L1/L2 known limitations documented in `OIP-Process-001` § 5.2 (uptime saturating at 90 days; flat contribution factor). Filed two years ahead of the soft 2028-05-10 deadline to buy 24 months of real-world calibration runway. New formulas: `uptime_factor = log(1 + online_days_last_730) / log(731)` (logarithmic over a 2-year horizon, normalized to [0,1]); `contribution_factor = 1.0 + 0.25·commits + 0.25·oip_authorship + 0.20·reviews + 0.30·mesh_operator` (each component ∈ [0,1], coefficients sum to 1.00, max contribution_factor = 2.00, max weight ≈ 1.414). § S3 conflict-of-interest meta-rule: contribution data with OIP-X as its subject is excluded from the voter's contribution_factor on OIP-X. § S5 forward-only activation (in-flight votes continue under the bootstrap formula). § S6 two re-calibration triggers (annual editor review, hard 90-day pin after Phase-4 mesh-telemetry availability). § S7 three reference voter profiles with worked-out weights under both formulas; max-to-min-stake ratio is ≈ 1.85× under the new formula vs ≈ 1.41× under bootstrap. Tally script and pytest suite deferred to V2 (post-Last-Call).

- **2026-05-12 — Tamarin model v0.3 (`restriction caps_intersect_meet`).** [`protocol-proofs/handshake.spthy`](./protocol-proofs/handshake.spthy) precommits the lattice-meet axioms (idempotence + commutativity) for the uninterpreted function `caps_intersect/2` that the I8 lemma `compliance_capability_no_downgrade` relies on. The original v0.2 footer flagged this as a follow-up the cryptographer would add "if proof heuristics struggle"; landing it now removes one ambiguity from the upcoming P3.2 review pass. The restriction is keyed on existing action labels (`Advertised_Caps`, `Negotiated_Caps`) so the constraint solver only expends search effort on capability sets the protocol actually instantiates. Associativity is intentionally omitted (no current lemma computes a 3-party meet). Proof execution itself remains P3.2-blocked.

### Fixed

- **2026-05-12 — CI workflow gaps surfaced by PR #13.**
  - `.github/workflows/ci.yml` `cargo clippy` and `cargo doc` jobs were running *without* `--all-features`, while `cargo test` (and the local verify-state pass) used `--all-features`. This silently masked broken intra-doc links to cfg-gated items (`omni-tee::tdx`, `omni-tee::sev_snp`) until the first PR triggered the doc job. Both jobs now pass `--all-features`; inline rationale documents the symmetry.
  - `.github/codeql/codeql-config.yml` (new) — excludes `rust/hardcoded-credentials` (and the alternate `rs/`-namespaced id) from CodeQL analysis. The query was firing on every literal byte string passed as `password` / `salt` to a KDF inside `#[cfg(test)] mod tests` blocks (12 alerts in `crates/omni-crypto/src/kdf.rs:234-260`, all deterministic Argon2id test vectors). Pattern is ubiquitous in crypto-library test suites; the CodeQL config also pins analysis scope to `crates/**/src/**`, excluding doc / OIP / framework / scripts. Wired into `.github/workflows/codeql.yml` via `config-file:` on the `init@v3` step.
  - `oips/oip-crypto-002.md` and `oips/oip-kernel-003.md` — frontmatter normalized to the canonical `OIP-Process-001` template (`oip:` integer, `track:` instead of `category:`, `license: CC0-1.0`, `updated:` / `superseded-by:` populated); section headings to Title Case (the linter is case-sensitive). `oip-kernel-003` was missing `## Privacy Considerations` — added a substantive section covering boot-path identifier exposure, host-local boot logs, allocator zeroization across trust boundaries, and TEE-sealed signing-key residency. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 5 file(s)`. No semantic change to either OIP's proposed decision.

  Note: `cargo audit` and `cargo deny` are still failing on this branch and on `main` due to **`RUSTSEC-2025-0141` — `bincode` v2.0.1 marked unmaintained on 2025-12-16** after the maintainer's announcement to cease development. This is a **pre-existing supply-chain advisory**, not a regression introduced by this PR. Resolution is a separate project-level decision (a `deny.toml` `ignore` entry with a tracking issue + sunset date, OR migration to `postcard` / `bitcode` / `rkyv` via a Standards-Track OIP). Out of scope for this fix-pass.

- **2026-05-12 — P3–P6 scaffolding verify-state fix-pass.** Brought the 2026-05-10 scaffolding pass to green under `cargo build/test/clippy/doc/fmt --all-features`. Net effect: 185 tests pass (was: 142 in the P1 baseline; the +43 cover P5 / P6 scaffolds plus 2 new round-trip tests for `Measurement` serde); `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` clean; `cargo fmt --all -- --check` clean. `cargo deny check` is CI-only and verified by `.github/workflows/audit.yml`. Specific fixes:
  - `crates/omni-tee/src/attestation.rs` — `Measurement([u8; 48])` now implements `Serialize` / `Deserialize` manually (serde derives only auto-impl arrays up to `[T; 32]`); the `Visitor` rejects any input not exactly 48 bytes long, preserving the invariant on the wire. Added two unit tests (`bincode` round-trip + wrong-length rejection).
  - `crates/omni-tee/Cargo.toml` — `bincode` added as a `dev-dependency` to enable the round-trip tests, consistent with the design-comment intent that `Quote` / `Measurement` / `SealedBlob` travel through `bincode` on the wire.
  - `crates/omni-tee/src/sealed_keys.rs` — `TeeSharedKey::drop` `unsafe { write_volatile(...) }` block now carries `#[allow(unsafe_code)]` with documented justification (defeats dead-store elimination without pulling the `zeroize` crate). Test `shared_key_drop_zeroizes` rewritten with `core::mem::ManuallyDrop` so the destructor runs in place — `mem::drop(key)` was moving `key` into the parameter slot and zeroing a different stack address than the captured raw pointer.
  - `crates/omni-tee/src/tdx.rs`, `crates/omni-tee/src/sev_snp.rs` — removed inner `#![cfg(feature = "...")]` (duplicates the gating already present on `pub mod tdx;` / `pub mod sev_snp;` in `lib.rs`).
  - `crates/omni-tee/src/mock.rs` — XOR-fold loops in `seal` / `unseal` / `derive_key_for` rewritten with `iter().zip(...)` to eliminate `clippy::indexing_slicing` warnings; report-data padding loop similarly converted.
  - `crates/omni-tee/src/lib.rs`, `crates/omni-tee/src/traits.rs`, `crates/omni-tee/src/attestation.rs`, `crates/omni-tee/src/tdx.rs`, `crates/omni-tee/tests/mock_integration.rs`, `crates/omni-kernel/src/memory.rs`, `crates/omni-kernel/src/syscall.rs` — backticks added to identifiers in doc comments (`x86_64`, `ARMv9`, `derive_key_for`, `local_attest_secret`, `peer_quote.measurement`); `traits::TeeErrorKind` first doc-comment paragraph shortened.
  - `crates/omni-tee/src/{mock,tdx,attestation}.rs` and `crates/omni-tee/tests/mock_integration.rs` — test modules now carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]` (and `clippy::similar_names` for the integration test's alice/bob/blob naming). Test code is allowed to panic on assertion failure.
  - `crates/omni-kernel/src/lib.rs` — `#![cfg_attr(all(feature = "bare-metal", not(test)), no_std)]` and `no_main` (was: gated only on the feature, broke `cargo test --features bare-metal`); `#![allow(clippy::missing_errors_doc)]` for the kernel scaffold's trait surfaces (per-method `# Errors` docs land with the corresponding subsystem implementation per P6.3+).
  - `crates/omni-kernel/src/memory.rs` — `use crate::{KernelResult, bitflags_simple};` (the `#[macro_export]` macro must be brought into scope explicitly when used in the same crate as its definition).

### Notes

- The scaffolding pass does NOT advance any item to `[x]` status that depends on physical-world action (notarial deed, hardware procurement, cryptographer engagement, audit firm engagement, grant submission). Status icons in `todo.md` remain at `[ ]` for tasks awaiting those gates.
- `MockTeeBackend` is **intentionally permissive** about attestation soundness — its purpose is to exercise consumer code paths, not to enforce real attestation invariants. Production builds MUST disable the `mock` feature.
- `OIP-Kernel-003` and `OIP-Crypto-002` are in `Draft` status; they activate per the standard `OIP-Process-001` flow (Review → Last Call → Active).

---

## [0.1.0] — 2026-05-10

First foundational milestone. Repository hygiene (P0) and foundational crates (P1) are landed and verified.

### Added

- **Repository hygiene (P0, closed 2026-05-09).** AGPL-3.0 `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `Cargo.lock`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, GitHub Actions workflows (`ci`, `audit`, `sbom`, `reproducible-build`, `dco`, `codeql`, `labeler`), Dependabot config, branch protection on `main` (signed commits, linear history, required reviewers), issue / PR templates, label taxonomy.
- **Foundational crates (P1, closed 2026-05-10).**
  - `omni-types` (33 tests) — strongly-typed identifiers (`NodeId`, `AgentId`, `ModelId`, `CapabilityId`, `SessionId`), `OmniError` taxonomy with `*ErrorKind` discriminants and PII-safe static `context` slugs, `OsVersion` and `ProtocolVersion` (`OMNI-PROTO-vN.M`) with subset-aware compatibility, sealed-trait `EncryptedType` plus marker types (`EncryptedString`, `MaskedSSN`, `TokenizedEmail`, `AttestedHash`) gated behind the `_tokenization_provider` feature.
  - `omni-crypto` (55 tests, marker `AWAITING_CRYPTO_REVIEW`) — `RustCrypto`-family wrappers with typed APIs:
    - `aead`: `ChaCha20-Poly1305` (RFC 8439) with `OmniAeadKey`, `OmniNonce`, `OmniCiphertext`, `NonceCounter` panicking on overflow.
    - `signing`: `Ed25519` (RFC 8032) using `verify_strict` to reject malleable signatures.
    - `kex`: `X25519` (RFC 7748) ECDH with `OmniEphemeralSecret` / `OmniStaticSecret` / `OmniPublicKey` / `OmniSharedSecret` and explicit low-order-point validator.
    - `hash`: trait-based `BLAKE3` / `SHA-256` / `SHA3-256` plus mandatory `domain_separated_hash` helper.
    - `kdf`: `HKDF-SHA-256` (RFC 5869) and `Argon2id` with OWASP-2026 default parameters.
    - `fpe` and `snark` placeholder modules for Phase 4.
    - `ConstantTimeEq` on every adversarial path; `Zeroize`-on-`Drop` on every secret.
  - `omni-capability` (43 tests + 7 cross-crate integration tests) — Macaroons-style capability tokens:
    - `token::CapabilityToken` with `bincode` 2.0 canonical encoding signed via Ed25519, embedding the issuer public key for self-contained verification.
    - `scope` with typed `Action` × `Resource` × `TimeWindow` × `Caveat` and a partial-order `is_subset_of`.
    - `attenuation` with property-tested monotonicity (256 cases) plus an adversarial test producing 256 random tampered children, all rejected.
    - `revocation::RevocationList` backed by an in-crate `MicroBloom` (chosen over the `bloomfilter` crate to stay `no_std + alloc`) plus a `BTreeSet` for false-positive resolution.
    - `tee::AttestationSource` trait + `StubAttestation` placeholder; concrete `omni-tee` backends land in P5.
- **Workspace dependency set frozen** (`Cargo.toml` + `docs/09-tech-specifications.md` kept in sync). `RustCrypto` family for all crypto; `ring` was evaluated and intentionally rejected (not `no_std`-friendly).
- **`no_std + alloc`** mandatory across foundational crates (`omni-types`, `omni-crypto`, `omni-capability`).
- **Compile-fail tests** (`trybuild`) for `omni-types`: enforce that `NodeId` / `ModelId` cannot be confused, that `EncryptedType` cannot be implemented externally (sealed trait), and that no `From<String>` constructor exists for `EncryptedString`.
- **Cross-crate integration test** (`crates/omni-capability/tests/integration_full_flow.rs`): full mint → attenuate (3-deep) → verify lifecycle plus six adversarial scenarios (revocation, attestation mismatch, time-window boundaries, tampered child, canonical-encoding round-trip).
- **Fuzz harness scaffolding** (`crates/omni-crypto/fuzz/`) for `aead_open`, `signing_verify`, `kex_dh` — runnable on Rust nightly via `cargo-fuzz`. Execution pass is deferred to P3 (cryptographer review).
- **Mesh-protocol wire format** for `CapabilityToken` documented in [`docs/03-mesh-protocol.md`](./docs/03-mesh-protocol.md) § "Capability tokens".
- **`CHANGELOG.md`** (this file).

### Changed

- `Cargo.toml` workspace dependencies pinned at `RustCrypto`-family versions; `serde` / `bincode` switched to `default-features = false` + `alloc` for `no_std` compatibility.
- `clippy.toml` `disallowed-methods` / `disallowed-types` / `disallowed-macros` reformatted to single-line inline tables (TOML 1.0 compliance).
- Workspace `Cargo.toml` `exclude` list now skips `crates/omni-crypto/fuzz` so `cargo build` / `cargo test` ignore it.

### Security

- `omni-crypto` carries an explicit `AWAITING_CRYPTO_REVIEW` marker. The implementation follows established `RustCrypto`-family APIs with RFC test vectors for every primitive, but no external cryptographer has signed off yet (P3.2 in `/todo.md`, blocked on funding via P4). **Do not use the output of this crate in adversarial settings until that review lands.**

### Notes

- 131 unit tests + 7 integration tests + 4 trybuild compile-fail tests, all green.
- `cargo clippy --all-targets -- -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` both pass on every foundational crate.
- Stub crates (`omni-tee`, `omni-kernel`, `omni-hal`, `omni-runtime`, `omni-mesh`, `omni-tokenization`, `omni-sdk`, `omni-agent`, `omni-shell`) remain as scaffolds; their P5 / P6+ implementations are tracked in `/todo.md`.

[Unreleased]: https://github.com/CySalazar/omni/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/CySalazar/omni/releases/tag/v0.1.0
