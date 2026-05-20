# Technical Specifications

**Status:** Draft v0.1.1 (P1 implementation in progress)

This document tracks exact versions of languages, libraries, and tooling used by OMNI OS. It is updated whenever dependencies change. Per project policy, code changes that affect dependencies MUST update this document in the same change set.

**Last review:** May 2026 — workspace dependency set frozen for P1 (foundational crates implementation).

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

| Crate | Version (pinned) | Purpose | `no_std` |
|---|---|---|---|
| `ed25519-dalek` | 2.1.x | Ed25519 signatures (`verify_strict` to reject malleable forms) | yes |
| `x25519-dalek` | 2.0.x | X25519 ECDH key exchange | yes |
| `chacha20poly1305` | 0.10.x | AEAD (RFC 8439) | yes (`alloc`) |
| `sha2` | 0.10.x | SHA-256 / SHA-512 family | yes |
| `sha3` | 0.10.x | SHA3-256 / Keccak family | yes |
| `blake3` | 1.5.x | BLAKE3 (default protocol-level hash; fastest, hardware-friendly) | yes |
| `hkdf` | 0.12.x | HKDF-SHA-256 for protocol session keys | yes |
| `argon2` | 0.5.x | Argon2id for memory-hard user-secret hashing | yes (`alloc`) |
| `subtle` | 2.6.x | `ConstantTimeEq` for AEAD tag check / signature verify | yes |
| `zeroize` | 1.8.x | Wipe key material on `Drop` (derive macro) | yes (`alloc`) |
| `rand_core` | 0.6.x | CSPRNG abstraction shared by RustCrypto | yes |
| `getrandom` | 0.2.x | Platform CSPRNG bridge (Linux: `getrandom(2)`) | yes |
| `arkworks` (`ark-*`) | TBD | zk-SNARK construction (Phase 4) | partial |
| `pq-crystals` (Kyber + Dilithium) | TBD | Post-quantum hybrid (Phase 4+) | partial |

**Rationale (decision recorded 2026-05-10):** The crypto base is **RustCrypto family for everything**. `ring` was evaluated and explicitly rejected because (a) `ring` is not `no_std`-friendly which would block kernel-side use in P6, (b) maintaining two parallel crypto trust bases doubles the audit surface, (c) RustCrypto crates expose typed APIs (`Key`, `Nonce`, `Tag`) that map naturally to OMNI's strongly-typed wrappers, (d) `Zeroize` and `subtle` integrate natively with the rest of the family. The trade-off is that the audit history of any single RustCrypto primitive is shorter than `ring`'s; this is mitigated by the planned external cryptographer review (P3.2 in `/todo.md`).

### Identifiers and encoding

| Crate | Version (pinned) | Purpose | `no_std` |
|---|---|---|---|
| `uuid` | 1.11.x | UUIDv4 random identifiers (`AgentId`, `CapabilityId`, `SessionId`). v4 chosen over v7 because v7 requires `std::time::SystemTime`. To revisit when `omni-hal` exposes a `Clock` abstraction (P6). Entropy from `getrandom` at call site. | yes |
| `hex` | 0.4.x | Safe hex encoding for raw-byte IDs (we deliberately do NOT impl `Display` for byte arrays) | yes (`alloc`) |

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

| Crate | Version (pinned) | Purpose |
|---|---|---|
| `criterion` | 0.5.x | Benchmarking (Phase 1.5: perf baselines for crypto primitives) |
| `proptest` | 1.5.x | Property-based testing (attenuation monotony, ID determinism) |
| `mockall` | 0.13.x | Mocking (`TeeBackend`, `Clock`) |
| `trybuild` | 1.0.x | Compile-fail tests (proves type-system invariants — e.g. cannot pass `ModelId` where `NodeId` expected, cannot construct `EncryptedString` outside the tokenization service) |
| `insta` | latest | Snapshot testing (canonical wire format regression) |
| `playwright`-like for E2E | TBD | End-to-end tests (per project policy requiring E2E) |

End-to-end testing harness will be specified later in Phase 1.

### Observability

| Crate | Version (planned) | Purpose |
|---|---|---|
| `tracing` | 0.1.x | Structured logging |
| `tracing-subscriber` | latest | Log/trace output |
| `metrics` | latest | Application metrics |

### Error handling

| Crate | Version (pinned) | Purpose | `no_std` |
|---|---|---|---|
| `thiserror` | 2.0.x | Library error types. v2 chosen to align with upstream ecosystem (`quinn`, `tokio`); eliminates duplicate-dependency warnings. Used with `default-features = false` to enable `core::error::Error` (Rust 1.81+) and stay `no_std`-compatible. | yes |
| `anyhow` | 1.0.x | Application-level error handling. Std-only; not used in foundational crates. | no |

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

## `no_std` policy

OMNI OS targets a `no_std` future (P6 — kernel transition) where the foundational crates must compile without the standard library. The policy is enforced layer by layer:

| Crate | `no_std` status | Notes |
|---|---|---|
| `omni-types` | `#![no_std]` + `extern crate alloc` | Mandatory from P1.1. |
| `omni-crypto` | `#![no_std]` + `extern crate alloc` | Mandatory from P1.2. CSPRNG uses `getrandom` which auto-detects platform (Linux: `getrandom(2)`; falls back when in `no_std` host with appropriate feature). |
| `omni-capability` | `#![no_std]` + `extern crate alloc` | Mandatory from P1.3. Bloom filter implemented in-crate to avoid pulling a `std`-only dependency. |
| `omni-tee` | `#![no_std]` + `extern crate alloc` | Mandatory from P5. |
| `omni-kernel` | `#![no_std]` + `#![no_main]` | Bare metal. Active on `x86_64-unknown-none` since v0.2.0; kernel CSPRNG (`crates/omni-kernel/src/entropy.rs`) replaces `getrandom`-backed paths under the `bare-metal` feature so no platform CSPRNG dep is pulled (P6.7.8.9). |
| `omni-driver-net-virtio`, `omni-driver-nvme`, `omni-driver-e1000e` | `#![cfg_attr(not(test), no_std)]` + `extern crate alloc` | Workspace-member driver libs (P6.7.8.2/4/6). Host build keeps `std` only for the test harness. |
| `omni-driver-net-virtio-image`, `omni-driver-nvme-image`, `omni-driver-e1000e-image` | `#![no_std]` + `#![no_main]` | Workspace-excluded bootable Ring 3 ELF siblings (P6.7.8.3/5/7). Built on `x86_64-unknown-none`; defensive `PanicOnAlloc` global allocator. |
| `omni-hal` | `#![no_std]` | HAL trait surface only. |
| Service crates (`omni-runtime`, `omni-mesh`, `omni-tokenization`) | `std` allowed | Userspace daemons. |
| User-facing crates (`omni-sdk`, `omni-agent`, `omni-shell`) | `std` | Userspace. |

Why this matters: every `no_std` violation in a foundational crate is a refactor hazard for P6. We pay the discipline cost up front rather than amortize it across a kernel rewrite.

## Update cadence

This document is updated:

- On any dependency addition or removal.
- On any version bump of MSRV or core dependencies.
- At each release.

Last review: 2026-05-20 (post `v0.3.0-alpha.1` + P6.7.8.9 closure: kernel adds `rand_core 0.6` + `rand_chacha 0.3` + `spin 0.9` direct deps — all `default-features = false`, no `getrandom` on the bare-metal build — and three driver crates land as workspace members alongside their bootable image siblings).
