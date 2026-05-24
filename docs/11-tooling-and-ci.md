# 11 — Tooling and CI

> **Status:** Draft v0.1 — 2026-05-09
> **Scope:** the toolchain, formatting / linting / supply-chain configuration,
> and the CI/CD pipeline that enforce OMNI OS's build hygiene.
>
> This document is the human-readable counterpart to the configuration files
> committed at the repository root and under `.github/`. It explains the
> *why* — the configuration files are the *how*.

---

## 11.1 Toolchain

OMNI OS pins its Rust toolchain to a specific channel via
[`rust-toolchain.toml`](../rust-toolchain.toml). All contributors and CI use
the same channel, removing the "works on my machine" failure mode.

| Item             | Value                          | Source of truth                  |
|------------------|--------------------------------|----------------------------------|
| Rust channel     | stable (latest minor pinned)   | `rust-toolchain.toml`            |
| Rust edition     | 2024                           | `[workspace.package].edition`    |
| MSRV             | 1.85                           | `[workspace.package].rust-version` and `clippy.toml` |
| Cargo resolver   | 3                              | `[workspace].resolver`           |
| Initial target   | `x86_64-unknown-linux-gnu`     | `/docs/07-hardware-requirements.md` |

When the MSRV bumps it must be coordinated across `rust-toolchain.toml`,
`Cargo.toml`, and `clippy.toml`. CI does not enforce this consistency
directly today — adding that lint is on the future-work list.

## 11.2 Formatting policy — `rustfmt.toml`

Configuration lives in [`rustfmt.toml`](../rustfmt.toml). Highlights:

- `max_width = 100` — modern displays, reasonable diff hygiene.
- `imports_granularity = "Crate"` — collapsed `use crate::{a, b, c};`.
- `group_imports = "StdExternalCrate"` — std → external → internal.
- `use_field_init_shorthand = true` — `Foo { x }` over `Foo { x: x }`.

CI runs `cargo fmt --all -- --check` and fails on any drift. Contributors
who use a pre-commit hook (recommended in `CONTRIBUTING.md` § 7.4) catch
drift before push.

## 11.3 Linting policy — `clippy.toml` + workspace lints

Two layers of lints are in force:

1. **Lint groups** in `Cargo.toml` (`[workspace.lints.rust]` and
   `[workspace.lints.clippy]`) — enforced workspace-wide via
   `lints.workspace = true`.

2. **Option-level configuration** in [`clippy.toml`](../clippy.toml).
   This file holds:
   - `msrv = "1.85"` (gates lints suggesting features unavailable on MSRV).
   - `cognitive-complexity-threshold = 20` (tighter than default 25).
   - `disallowed-methods` — blocks `std::env::var`, `std::process::exit`,
     `std::time::SystemTime::now`, std `Mutex/RwLock`, with reasons.
   - `disallowed-macros` — blocks `println!`, `eprintln!`, `dbg!` in favor
     of the `tracing` crate.
   - `doc-valid-idents` — recognized acronyms (TEE, AGPL, AEAD, BLAKE3, ...).

Each disallowed entry includes a `reason = "..."` so PR reviewers don't
need to re-derive the rationale.

CI runs `cargo clippy --workspace --all-targets -- -D warnings` and fails
on any warning.

## 11.4 Supply-chain policy — `deny.toml`

Configuration: [`deny.toml`](../deny.toml). Run with:

```bash
cargo deny check                   # all four sections
cargo deny check advisories        # RustSec advisories only
cargo deny check licenses          # license allowlist enforcement
cargo deny check bans              # banned crates / wildcards
cargo deny check sources           # registry / git allowlist
```

### 11.4.1 Advisories

- Vulnerabilities → `deny`.
- Yanked crates → `deny`.
- Unmaintained → `warn` (surfaces tech debt without blocking).

Advisory ignores require a tracking issue and a sunset date.

### 11.4.2 Licenses

The inbound license allowlist is fixed and explicit:

```
Apache-2.0, Apache-2.0 WITH LLVM-exception,
MIT, BSD-2-Clause, BSD-3-Clause, BSL-1.0, ISC,
Unicode-DFS-2016, Unicode-3.0, Zlib, CC0-1.0, MPL-2.0
```

Anything outside this list is rejected. Adding to the list requires:
- Founder approval during the 5-year veto window, or
- Stichting OMNI board approval afterward.

### 11.4.3 Bans (refused crates)

| Banned crate     | Reason                                                                                       |
|------------------|----------------------------------------------------------------------------------------------|
| `openssl-sys`    | Force `rustls + ring`. OpenSSL has a poor supply-chain track record and adds a C-toolchain attack surface incompatible with the OMNI OS threat model. |
| `openssl`        | Same as above.                                                                                |
| `native-tls`     | Pulls platform-specific TLS stacks. We use `rustls` everywhere for deterministic, audited behavior. |
| `md5`, `sha1`    | Cryptographically weak. Exception path exists for non-security checksums via PR review.       |
| `rand`           | Use `rand_core` + an explicit auditable RNG (`OsRng`, `ChaCha20Rng`).                         |
| `time`           | Historic CVE lineage and ergonomic footguns. Use `chrono` with vetted features, or `jiff`.   |

### 11.4.4 Sources

- `unknown-registry = "deny"` — only `crates.io`.
- `unknown-git = "deny"` — git deps require explicit allowlist with pinned
  full SHA and a sunset date for migration to the registered version.

## 11.5 CI/CD pipeline

The pipeline lives under [`.github/workflows/`](../.github/workflows/).
Branch protection on `main` requires every workflow listed below to be
green before a PR can merge.

| Workflow                  | Trigger(s)                                        | Purpose                                                                  |
|---------------------------|---------------------------------------------------|---------------------------------------------------------------------------|
| `ci.yml`                  | push, PR                                          | `cargo fmt`, `cargo clippy`, `cargo test`, `cargo doc`, TBD-placeholder guard. |
| `audit.yml`               | push (Cargo.lock change), PR, daily 06:00 UTC, dispatch | `cargo audit` + `cargo deny check` (advisories, licenses, bans, sources). |
| `sbom.yml`                | tag push (`v*.*.*`), dispatch                     | CycloneDX SBOM + SLSA build provenance attestation.                       |
| `reproducible-build.yml`  | tag push                                          | Two parallel runners build the same release artifact; hashes must match.  |
| `dco.yml`                 | PR (opened, synchronize, reopened)                | Every commit must carry a `Signed-off-by:` trailer.                       |
| `codeql.yml`              | push, PR, weekly                                  | GitHub CodeQL static analysis (Rust support is beta).                     |
| `labeler.yml`             | PR opened/synchronized                            | Auto-applies `area:*` labels based on changed paths.                      |

### 11.5.1 Status check naming

Branch-protection rules in `scripts/bootstrap-github.sh` reference these
job names verbatim:

- `ci / cargo fmt`
- `ci / cargo clippy`
- `ci / cargo test (ubuntu-24.04)`
- `ci / cargo doc`
- `audit / cargo audit`
- `audit / cargo deny`
- `dco / DCO sign-off`
- `codeql / CodeQL — rust`

If a workflow's `name:` field is renamed, update the bootstrap script in
the same PR.

### 11.5.2 Performance budget

The CI workflow targets **< 10 minutes wall-clock** for a typical PR
(per `todo.md` P0.4 acceptance criterion). Caching via
`Swatinem/rust-cache@v2` is enabled on `clippy`, `test`, and `doc` jobs.
When budget is exceeded, the first lever is reducing the matrix or
splitting into a separate `nightly-deep-checks.yml` workflow.

### 11.5.3 SLSA / SBOM

The `sbom.yml` workflow targets **SLSA Level 3** maturity:

- Build runs on a hosted, ephemeral runner.
- Provenance is generated via `actions/attest-build-provenance@v2`
  (cosign + GitHub OIDC).
- SBOM is CycloneDX JSON, attached to the GitHub release.
- Reproducible-build verification runs in parallel — divergent hashes
  fail the release.

## 11.6 Branch protection and signed commits

Configured by [`scripts/bootstrap-github.sh`](../scripts/bootstrap-github.sh)
once the repo is on GitHub. Highlights:

- `main` is the default branch.
- Force-pushes disabled; deletion disabled.
- Linear history required (squash-and-merge only).
- Required PR reviews: **1** until a co-maintainer joins (Phase 1
  hiring), then **2**.
- `dismiss_stale_reviews = true`; `require_last_push_approval = true`.
- **Signed commits required** (SSH or GPG; SSH signing is recommended
  for ergonomics — see `scripts/bootstrap-local.sh`).
- All status checks listed in 11.5.1 must be green.

Tag protection: only signed tags matching `v*.*.*` are accepted, and
they must originate from `main`.

## 11.7 Dependabot

Configuration: [`.github/dependabot.yml`](../.github/dependabot.yml).

- **Cargo:** weekly Monday 06:00 Europe/Amsterdam.
  - Security updates grouped.
  - Patch updates auto-approve after CI green.
  - Minor / major updates require human review.
  - **Major bumps for cryptographic and networking crates are explicitly
    ignored** — these come through Dependabot security advisories
    instead, and we triage manually.
- **GitHub Actions:** weekly Monday, minor + patch grouped.

## 11.8 GitHub templates and label taxonomy

Issue / PR templates and the auto-labeler config are under `.github/`:

- [`ISSUE_TEMPLATE/config.yml`](../.github/ISSUE_TEMPLATE/config.yml) —
  blank issues disabled; redirects for security and CoC.
- [`ISSUE_TEMPLATE/bug_report.yml`](../.github/ISSUE_TEMPLATE/bug_report.yml)
- [`ISSUE_TEMPLATE/feature_request.yml`](../.github/ISSUE_TEMPLATE/feature_request.yml)
- [`ISSUE_TEMPLATE/security_advisory.yml`](../.github/ISSUE_TEMPLATE/security_advisory.yml)
  — *redirects* to `SECURITY.md`; not for new vulnerabilities.
- [`ISSUE_TEMPLATE/oip_proposal.yml`](../.github/ISSUE_TEMPLATE/oip_proposal.yml)
- [`PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md)
- [`labeler.yml`](../.github/labeler.yml) — path → label rules.

The label taxonomy is created by `scripts/bootstrap-github.sh`:

- `area:kernel | crypto | capability | tee | hal | runtime | mesh | tokenization | sdk | agent | shell | types | docs | ci | oip`
- `priority:P0 | P1 | P2 | P3`
- `kind:bug | feature | refactor | docs | security | chore`
- Special: `oip-required`, `breaking-change`, `good-first-issue`, `help-wanted`, `needs-triage`, `dependencies`, `do-not-use`.

## 11.9 Local development quick reference

```bash
# Format and lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace --all-features

# Documentation
cargo doc --workspace --no-deps

# Supply chain
cargo deny check                                # all four sections
cargo audit                                     # RustSec advisories
cargo install --locked cargo-audit cargo-deny   # one-time installs
```

A pre-commit hook template is provided in `CONTRIBUTING.md` § 7.4.

## 11.10 Cross-references

- [`/CONTRIBUTING.md`](../CONTRIBUTING.md) — the contribution flow.
- [`/SECURITY.md`](../SECURITY.md) — disclosure policy.
- [`/docs/04-security-model.md`](./04-security-model.md) § Supply-chain.
- [`/docs/06-roadmap.md`](./06-roadmap.md) — phase-by-phase scope.
- [`/docs/09-tech-specifications.md`](./09-tech-specifications.md) — exact dependency versions.
- [`/todo.md`](../todo.md) — implementation backlog (P0 closes this document).

## 11.11 Maintenance policy

This document is updated **in the same PR** as any change to:

- `rustfmt.toml`, `clippy.toml`, `deny.toml`, `Cargo.toml` `[workspace.lints]`.
- Any file under `.github/`.
- `scripts/bootstrap-github.sh` or `scripts/bootstrap-local.sh`.

Changelog tracking lives at the bottom of each configuration file. This
document carries a brief change history below.

## Change history

- 2026-05-09 — Initial draft. Created during P0 closure (`todo.md` P0).
