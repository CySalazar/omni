# Changelog

All notable changes to OMNI OS are documented in this file.

The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

OMNI OS distinguishes two version streams:

- **OS version** (`MAJOR.MINOR.PATCH`) — the distribution release.
- **Mesh protocol version** (`OMNI-PROTO-vMAJOR.MINOR`) — negotiated at handshake. Decoupled from the OS version (see [`docs/09-tech-specifications.md`](./docs/09-tech-specifications.md) § "Versioning policy").

Each entry below tracks the OS version. Protocol-version changes get their own bullet inside the OS-version entry that introduces them.

---

## [Unreleased]

Items not yet associated with a numbered release.

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
