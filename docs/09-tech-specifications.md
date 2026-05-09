# Technical Specifications

**Status:** Draft v0.1

This document tracks exact versions of languages, libraries, and tooling used by OMNI OS. It is updated whenever dependencies change. Per project policy, code changes that affect dependencies MUST update this document in the same change set.

## Language

| | Version | Notes |
|---|---------|-------|
| Rust | 1.85+ | Edition 2024 |
| MSRV (Minimum Supported Rust Version) | 1.85 | |

Rust 2024 edition is selected for: improved async ergonomics, better lifetime inference, stabilized features needed for systems programming, improved const generics.

## Toolchain

| Tool | Version | Purpose |
|---|---|---|
| `rustc` | matches MSRV | Compiler |
| `cargo` | matches MSRV | Build system |
| `rust-analyzer` | latest stable | IDE language server |
| `clippy` | matches MSRV | Linter |
| `rustfmt` | matches MSRV | Formatter |
| `cargo-watch` | latest | Iterative builds during development |
| `cargo-audit` | latest | Dependency security audit |
| `cargo-deny` | latest | License and dependency policy enforcement |
| `cargo-nextest` | latest | Faster test runner |
| `cargo-llvm-cov` | latest | Code coverage |

## Core dependencies (planned for v1)

These are intended dependencies. Final selections will be confirmed during Phase 1 implementation; placeholders flagged with `(TBD)`.

### Cryptography

| Crate | Version (planned) | Purpose |
|---|---|---|
| `ring` | 0.17.x | Symmetric and asymmetric primitives baseline |
| `ed25519-dalek` | 2.x | Ed25519 signatures |
| `x25519-dalek` | 2.x | X25519 key exchange |
| `chacha20poly1305` | 0.10.x | AEAD |
| `sha2`, `sha3`, `blake3` | latest | Hashes |
| `argon2` | 0.5.x | Password hashing (where applicable) |
| `arkworks` (`ark-*`) | latest | zk-SNARK construction (TBD) |
| `pq-crystals` (Kyber + Dilithium) | TBD | Post-quantum hybrid (Phase 4+) |

Rationale: `ring` for battle-tested baseline; RustCrypto family for specific algorithms; arkworks for zk-SNARK research-grade implementations.

### Networking

| Crate | Version (planned) | Purpose |
|---|---|---|
| `quinn` | 0.11.x | QUIC implementation |
| `snow` | 0.9.x | Noise Protocol Framework |
| `hickory-dns` | latest | DNS (formerly trust-dns) |
| `libp2p` | 0.55.x | Reference for DHT and gossip (selective use) |
| `if-watch` | latest | Network interface monitoring |

### Async runtime

| Crate | Version (planned) | Purpose |
|---|---|---|
| `tokio` | 1.x | Async runtime |
| `async-trait` | 0.1.x | Async traits in stable Rust |
| `futures` | 0.3.x | Stream/future utilities |

### Serialization

| Crate | Version (planned) | Purpose |
|---|---|---|
| `serde` | 1.x | General serialization |
| `prost` | 0.13.x | Protocol Buffers |
| `capnp` | latest | Cap'n Proto (alternative for low-latency wire format) |
| `bincode` | 2.x | Internal binary serialization |

Final wire format (Cap'n Proto vs. Protocol Buffers vs. custom) deferred to OIP-001.

### AI / Tensors

| Crate | Version (planned) | Purpose |
|---|---|---|
| `candle` | latest | Rust-native tensor library (Hugging Face) |
| `tch` | latest | LibTorch bindings (alternative; benchmarked) |
| `safetensors` | latest | Model weight format |
| `tokenizers` | latest | HF tokenizers |

Final selection between `candle` and `tch` depends on benchmark + ecosystem maturity at v1 implementation start.

### Sandboxing

| Crate | Version (planned) | Purpose |
|---|---|---|
| `wasmtime` | latest | WASM runtime for agent sandboxing (TBD) |

Sandboxing approach (WASM vs. process isolation) is an open architectural question — see [04-security-model.md](./04-security-model.md).

### Testing

| Crate | Version (planned) | Purpose |
|---|---|---|
| `criterion` | 0.5.x | Benchmarking |
| `proptest` | 1.x | Property-based testing |
| `mockall` | 0.13.x | Mocking |
| `insta` | latest | Snapshot testing |
| `playwright`-like for E2E | TBD | End-to-end tests (per project policy requiring E2E) |

End-to-end testing harness will be specified in Phase 1.

### Observability

| Crate | Version (planned) | Purpose |
|---|---|---|
| `tracing` | 0.1.x | Structured logging |
| `tracing-subscriber` | latest | Log/trace output |
| `metrics` | latest | Application metrics |

### Error handling

| Crate | Version (planned) | Purpose |
|---|---|---|
| `thiserror` | 2.0.x | Library error types. v2 chosen to align with upstream ecosystem (quinn, tokio); eliminates duplicate-dependency warnings. |
| `anyhow` | 1.0.x | Application-level error handling. |

## TEE-specific dependencies

| Component | Source | Purpose |
|---|---|---|
| Intel TDX SDK | Intel | TDX attestation, sealing |
| AMD SEV-SNP attestation | AMD | SEV-SNP attestation |
| `tdx-quote-verification` (TBD) | TBD | TDX quote verification library |

These are typically C/C++ libraries with Rust FFI wrappers. Wrappers will be vendored or maintained as separate crates within the OMNI OS workspace.

## Build and CI

| Tool | Purpose |
|---|---|
| GitHub Actions | CI/CD pipelines |
| `cross` | Cross-compilation testing |
| Docker | Reproducible build environments |
| `cargo-bloat` | Binary size analysis |
| `bindgen` | C FFI bindings generation |

CI must run on every pull request:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo audit`
- `cargo deny check`
- Code coverage (target ≥ 80% for new code)
- Documentation build

## Versioning policy

OMNI OS follows Semantic Versioning 2.0.0:

- **MAJOR**: incompatible protocol or API changes.
- **MINOR**: additions in a backwards-compatible manner.
- **PATCH**: backwards-compatible bug fixes.

The mesh protocol has its own versioning negotiated at handshake; protocol versions may evolve independently of the OS version. Format: `OMNI-PROTO-vN.M`.

## Documentation tooling

| Tool | Purpose |
|---|---|
| `mdBook` | Long-form documentation site |
| `cargo doc` | API documentation |
| Mermaid | Diagrams in markdown |

## License compliance

`cargo-deny` policy enforces:

- Allowed licenses: MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0, AGPL-3.0 (project itself).
- Forbidden licenses: GPL-2/3 (incompatible with our AGPL-3.0+commercial dual-licensing), proprietary, unlicensed.
- All dependencies must have clear, machine-readable licenses.

## Update cadence

This document is updated:

- On any dependency addition or removal.
- On any version bump of MSRV or core dependencies.
- At each release.

Last review: May 2026 (initial draft).
