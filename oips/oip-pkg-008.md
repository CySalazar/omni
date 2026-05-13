---
oip: 8
title: omni-pkg — Content-Addressed Federated Package Manager with Capability-Declarative Manifests
track: Standards Track
status: Draft
authors:
  - cySalazar <cySalazar@cySalazar.com>
created: 2026-05-12
updated: 2026-05-12
requires:
  - OIP-Process-001
  - OIP-Container-006
  - OIP-Crypto-002
supersedes: ~
superseded-by: ~
discussion: https://github.com/CySalazar/omni/discussions (TBD link)
license: CC0-1.0
---

# OIP-Pkg-008 — `omni-pkg`: Content-Addressed Federated Package Manager

## Abstract

This OIP commits OMNI OS to **`omni-pkg`**, the package manager OMNI OS
ships with v1.x. Key properties:

- **Content-addressed** (SHA-256 OCI digest model, similar to Docker).
- **Federated** registries with **`omni-market` as default blessed source**
  (per OIP-Market-010).
- **Sigstore-signed** mandatory + Certificate-Transparency-style log.
- **Capability-declarative manifest** per package: the manifest
  declares the capability set the app needs, the user approves it at
  install time, runtime cannot exceed it.
- **Atomic upgrade** via Nix-style symlink swap (no broken state mid-upgrade,
  rollback trivial).
- **OCI image compatibility** for container payloads (per OIP-Container-006).

## Motivation

`omni-pkg` is the substrate underneath `omni-helper` (OIP-Helper-007),
`omni-market` (OIP-Market-010), and ultimately every "install X" action
in OMNI. The design must satisfy:

- **Reproducibility**: same package digest → same install state.
- **Trust composability**: package signature + capability declaration +
  registry trust score compose into a single user-visible trust score.
- **Federation without monoculture**: Foundation runs the default
  registry but does NOT have exclusive distribution rights.
- **No ambient authority**: an installed package gets only the
  capabilities the user granted at install time.

Compared with existing systems:

| System | Content-addressed | Signing | Federated | Capability-declarative | Compatible |
|---|---|---|---|---|---|
| apt/dpkg | No (filesystem-based) | distro key | per-distro | No (DAC) | N/A |
| Nix | Yes (derivation hash) | Optional | channels | No | Compatible |
| OCI / Docker Hub | Yes (digest) | Optional | yes | No | Compatible |
| Flatpak | Yes (ostree) | Optional | flathub central | Partial (manifests) | Compatible |
| `omni-pkg` | **Yes** | **Mandatory Sigstore + CT log** | **Yes, blessed central** | **Yes, mandatory** | OCI v1 |

## Specification

### 1. Package format

A package is one of:

- An **OCI container image** (for desktop apps, services, AI workloads);
  produced via OmniContainer toolchain.
- A **Nix-style derivation** (for system components, libraries, build-time
  tooling); reproducible build mandatory.

Both formats carry a single mandatory **capability manifest** (OMNI
extension to OCI annotations):

```json
{
  "io.omni-os.capabilities-required": [
    "fs:read:/data",
    "fs:write:/data/output",
    "net:outbound:huggingface.co:443",
    "gpu:shared"
  ],
  "io.omni-os.tee-required": "tdx-or-sev-snp | none",
  "io.omni-os.guest-kernel-min-version": "v6.10-stable",
  "io.omni-os.signed-by": "ed25519:<fingerprint>",
  "io.omni-os.market-tier": "gold | silver | bronze | community"
}
```

### 2. Trust + signing

- Every package must be **Sigstore-signed** at publish time.
- Signature lands in a **Certificate Transparency-style append-only log**
  operated by Stichting OMNI (parallel infrastructure to model attestation
  from OIP-Crypto-002).
- Verifier checks signature + CT log inclusion before install. Missing
  inclusion = refuse.

### 3. Federated registry protocol

A registry is an HTTPS endpoint that speaks:

- `GET /v1/packages/<package-name>/<version>` → manifest (JSON).
- `GET /v1/packages/<package-name>/<version>/<digest>/blob` → content.
- `GET /v1/search?q=...&capabilities=...` → ranked search.
- `GET /v1/trust-attestations/<digest>` → Sigstore bundle + CT proof.

User configuration:

```
omni-pkg.registries = [
    "https://market.omni-os.org",     # default Stichting blessed
    "https://my-org.example/registry" # additional (opt-in)
]
omni-pkg.policy.trust-min-score = 3
omni-pkg.policy.allow-community-signed = true
omni-pkg.policy.prefer-tee-required = true
```

When `omni-helper` invokes `omni-pkg search`, all configured registries
are consulted in parallel. Results are merged and ranked by trust score
(market tier + community reputation + capability minimality).

### 4. Atomic upgrade + rollback

Packages install into content-addressed paths (Nix model):

```
/omni/pkgs/<sha256>/...   # immutable, content-addressed
/omni/profile/current/    # symlink → user's active set of packages
/omni/profile/generations/<N>/  # frozen historical generations
```

Upgrade = create new generation symlink, swap atomically. Rollback =
swap back. No broken state ever; concurrent installs cannot corrupt.

### 5. Capability prompt at install

User invokes `omni-pkg install <package>` (or via Helper). The CLI:

1. Fetches package + manifest.
2. Verifies signature + CT inclusion.
3. **Displays the Impact Dashboard** (Privacy/Trust/Cost/Time/Caps) per
   OIP-Helper-007.
4. **Lists capabilities requested**, in plain language.
5. User explicitly approves or denies.
6. On approval: capabilities minted, package activated, audit log entry.

The granted capability set is sealed under the user's TEE
(`omni-tee::SealPolicy`); the package's runtime cannot exceed it.

### 6. Reference implementation — `crates/omni-pkg/`

```
crates/omni-pkg/
├── Cargo.toml
├── src/
│   ├── lib.rs              # public surface
│   ├── registry/
│   │   ├── client.rs       # HTTPS+Sigstore client
│   │   ├── federation.rs   # parallel multi-registry search
│   │   └── policy.rs       # trust scoring
│   ├── manifest.rs         # OCI annotation parsing + capability decl
│   ├── verify.rs           # Sigstore + CT log + capability-minimality
│   ├── install.rs          # content-addressed install + symlink swap
│   ├── rollback.rs
│   └── cli/
│       ├── search.rs
│       ├── install.rs
│       ├── upgrade.rs
│       └── rollback.rs
└── tests/
    ├── atomic_upgrade.rs
    ├── signature_verification.rs
    ├── federated_search.rs
    └── capability_prompt.rs
```

Estimated effort: **9-12 engineer-months** for v0.1 (production-grade
federated registry + Sigstore + atomic install).

## Rationale

### Why content-addressed over filesystem-based (apt/dpkg)?

Reproducibility. Two users with the same package digest installed have
bit-identical state. Critical for the project's audit and attestation
story.

### Why federated rather than centralized?

Anti-capture. A centralized registry can be compelled by a hostile
jurisdiction; a federated topology with content-addressed signing
makes hostile compulsion nearly worthless (the content is mirrored,
the signature is in a public CT log).

### Why mandatory Sigstore over optional signing?

The 2024 SolarWinds-class attacks demonstrate that "optional" signing
becomes "rarely signed" at the long tail. Mandatory closes the loophole.

### Why OCI + Nix derivation, both?

OCI is the de-facto standard for container payloads — by adopting it we
make Docker Hub images runnable on OMNI directly. Nix derivations handle
system components (libraries, build-time tooling) where reproducible
build hashing matters more than image fetching.

## Backwards Compatibility

Not applicable.

## Test Cases

1. **OCI image install round-trip**: pull `alpine:latest`, install,
   atomic upgrade to `alpine:3.20`, rollback to `latest`, no broken
   state at any step.
2. **Signature verification negative**: tampered package fails
   signature verify; refuse to install.
3. **CT inclusion negative**: package with valid signature but no CT
   log entry; refuse to install.
4. **Federated search**: two registries configured, package present
   in both with different signatures; helper sees both, merges,
   ranks by trust.
5. **Capability minimality**: package declares `fs:write:/etc` but
   binary analysis shows only `fs:write:/data`; warning at install.
6. **Privacy budget gate**: install exceeds remaining budget;
   refused with clear error.
7. **Rollback to previous generation**: `omni-pkg rollback`, system
   returns to prior symlink target.

## Reference Implementation

To land before activation:
- `crates/omni-pkg/` skeleton with the structure in §6.
- Reference `market.omni-os.org` registry deployment (separate repo).
- CT log infrastructure (re-use the model attestation CT log from
  OIP-Crypto-002 — same Sigstore stack, separate domain namespace).
- Integration tests against a local mock registry.

## Security Considerations

- **CT log compromise**: if the Stichting CT log is compromised,
  signature trust degrades. Mitigation: external CT log witnesses
  (Sigstore community), cross-signed by independent witnesses.
- **Capability over-grant via install**: user reflexively approves
  too-broad capability sets. Mitigation: Helper-Guided default shows
  capability-minimality warnings; over-broad asks default to "deny".
- **Registry DoS**: malicious registry returns huge responses.
  Mitigation: response size caps, per-registry rate limit.

## Privacy Considerations

- **Registry queries leak package interest**: a query "search
  libreoffice" leaks intent. Mitigation: queries route through
  Tier 2 mesh (onion-routed) when privacy budget allows; Tier 0
  local-only for offline pre-cached packages.
- **Install metadata** in audit log is local-only and not exported.

## Future Work

- **OIP-Pkg-Mirror-XXX** (Phase 6 mid): IPFS-backed package mirroring
  for resilience.
- **OIP-Pkg-Bisect-XXX** (Phase 7): bisecting installs to find
  regression-introducing package version.

## Copyright

CC0 1.0 Universal.
