---
oip: 11
title: Omni* Flagship Apps Program — OmniCode as Phase-1 Reference Editor
track: Standards Track
status: Draft
authors:
  - cySalazar <cySalazar@cySalazar.com>
created: 2026-05-12
updated: 2026-05-12
requires:
  - OIP-Process-001
  - OIP-Container-006
  - OIP-Pkg-008
  - OIP-Market-010
supersedes: ~
superseded-by: ~
discussion: https://github.com/CySalazar/omni/discussions (TBD link)
license: CC0-1.0
---

# OIP-Flagship-011 — Omni* Flagship Apps Program, OmniCode v1

## Abstract

This OIP commits Stichting OMNI to a **flagship-app program** that
develops and maintains a set of **Stichting-Curated** reference
applications. These apps:

- Use the `Omni{Function}` naming convention (e.g., `OmniCode`,
  `OmniMail`, `OmniNotes`).
- Are **Stichting-Curated** tier in `omni-market` (per OIP-Market-010).
- Serve as exemplars of capability-minimality + reproducible build +
  AGPL-3.0 + no-telemetry.
- Are AGPL-3.0 source open, with `omni-market` distribution.

The first flagship is **`OmniCode`**, a VSCode-experience editor with
Rust + Python pre-configured and OpenVSX extension marketplace integration.
Phased delivery:

- **Phase 1** (immediate, v1.x): Codium inside an OmniContainer (Electron
  in container), shipped as `OmniCode`. Working version available within
  weeks of OmniContainer GA.
- **Phase 2** (v1.x+, target year 4): Tauri-based native port of Codium
  UI to eliminate Electron overhead. Effort: 9-12 engineer-months.

## Motivation

A new OS without flagship apps cannot demonstrate "this is what good
software looks like on this platform". Apple has iWork; GNOME has Files
+ Maps + Photos; macOS has Calculator + Notes + Photos. OMNI needs an
equivalent set to:

1. **Demonstrate the platform** — capability-minimality, reproducible
   builds, AGPL alignment in practice.
2. **Onboard developers** with a familiar code editor (VSCode UX is
   the dominant developer mental model in 2026).
3. **Provide reference Stichting-Curated** entries in `omni-market`
   so other developers see what Gold-tier looks like.

VSCode itself is owned by Microsoft and includes proprietary telemetry.
**Codium** (`codium.io`) is the community-maintained, telemetry-free,
open-source rebrand of VSCode; it uses the **OpenVSX registry** (Eclipse
Foundation operated) as its extension marketplace, rather than the
Microsoft Marketplace.

## Specification

### 1. Naming convention

Stichting-Curated apps use the prefix **`Omni`** followed by a function
name in CamelCase. Examples (planned):

| Name | Function | Phase target |
|---|---|---|
| **OmniCode** | Code editor with Rust + Python + extension support | **Phase 1 immediate / Phase 2 native** |
| **OmniShell** | Already pre-scaffolded in `crates/omni-shell` | Phase 6 |
| **OmniMail** | Email + PGP + privacy-first | Phase 7+ |
| **OmniNotes** | Markdown notes + sync | Phase 7+ |
| **OmniDocs** | Document viewer/editor | Phase 7+ |
| **OmniPhotos** | Photo viewer + minimal edit | Phase 8+ |
| **OmniCalendar** + **OmniContacts** | PIM suite | Phase 8+ |

The `Omni` prefix is **reserved** for Stichting-Curated apps; community
apps may not use it. This is enforced at `omni-market` submission.

### 2. Curation criteria for any Omni* app

To qualify for the Omni* prefix and Stichting-Curated status, an app
must satisfy:

1. **License**: AGPL-3.0-only or compatible OSS (no permissive escape).
2. **Capability-minimality**: declared capability set is the
   minimum required, reviewed by the Foundation and demonstrated by
   reproducible build + binary analysis.
3. **Reproducible build**: bit-identical artifact from source on two
   independent machines.
4. **No telemetry**: zero network egress not strictly required for app
   function; any optional egress is explicitly opt-in.
5. **Stichting maintenance**: a Foundation-paid maintainer (or
   sponsored volunteer with maintenance SLA).
6. **Annual security review**: rotating across the flagship set;
   external audit at least every 24 months.
7. **OpenVSX-compatible extension system** (where applicable): extensions
   declare capabilities, OMNI helper presents them at install.

### 3. OmniCode v1 — phased delivery

#### 3.1. Phase 1 — Codium in OmniContainer (v1.x immediate)

The fastest path to a working OmniCode:

- Take upstream Codium (Electron + TypeScript).
- Package as an OmniContainer (Linux guest with `omni/linux-codium:N-stable`
  image).
- Pre-install Rust extensions (rust-analyzer LSP), Python (pyright LSP),
  TypeScript, Markdown.
- Pre-configure to use **OpenVSX** as the extension marketplace.
- Distribute via `omni-market` Stichting-Curated tier.

User experience: launch OmniCode → opens a fully featured VSCode-experience
editor; install extensions from OpenVSX; develop Rust/Python/TS code that
runs in OmniContainers or as `omni-forge` artifacts.

Engineering effort: **2-3 engineer-months** (mostly packaging + integration
testing). Available within weeks of OmniContainer GA (Phase 5).

Trade-off: ~300MB binary (Electron is heavy); ~5s cold start.

#### 3.2. Phase 2 — Native Tauri port (v1.x+ target year 4)

Once OmniContainer Phase-1 is stable and OmniForge can compile non-trivial
binaries, a Tauri-based native port becomes feasible:

- **Tauri 2.x** as the application shell (Rust core + WebView UI).
- **Codium UI ported** from Electron to Tauri (the VSCode UI itself is
  TypeScript/CSS/HTML and largely Tauri-compatible after porting node-
  APIs).
- **Result**: ~50MB binary, sub-second cold start, native OMNI capability
  binding throughout.

Engineering effort: **9-12 engineer-months** for a feature-parity port.
Significant: the VSCode core has many Node.js-API dependencies that need
Tauri equivalents. A subset of extensions that rely heavily on `nodeIntegration`
may not survive the port; those continue to work in the Phase-1 container path.

Both versions coexist: Phase-1 container for max compatibility, Phase-2 native
for performance.

### 4. Extension marketplace — OpenVSX

OmniCode uses **OpenVSX registry** (Eclipse Foundation, `open-vsx.org`)
for extensions. Rationale:

- License-clean: OpenVSX extensions are licensed for redistribution; the
  Microsoft Marketplace terms-of-service do not permit non-VSCode-product
  consumption.
- Community-governed: aligned with OMNI's open-mission values.
- Coverage: OpenVSX has ~80% of the Microsoft Marketplace extensions
  most commonly used by developers (LSPs, themes, snippets).

Extensions are subject to **the same `omni-market` Bronze/Silver/Gold tier
flow** as native packages, with one exception: OpenVSX-sourced extensions
inherit OpenVSX's signing and are admitted automatically as Bronze tier.
Promotion to Silver+ requires the same Stichting verification as native
apps.

### 5. Default pre-configuration

| Component | Status |
|---|---|
| rust-analyzer LSP | Default-on |
| pyright LSP | Default-on |
| TypeScript (tsserver) | Default-on (needed for OMNI scripting) |
| Markdown preview | Default-on |
| Tree-sitter highlighting | Default-on |
| `omni-forge` integration ("generate snippet from intent") | Default-on (privacy-budget gated) |
| `omni-market` extension installer (with capability prompt) | Default-on |
| OpenVSX as the extension registry | Default-on |

### 6. Reference implementation

OmniCode lives in a **separate repo** outside the main OMNI OS workspace:

```
omni-code/
├── README.md
├── LICENSE  (AGPL-3.0)
├── packaging/
│   └── omni-container/
│       └── linux-codium/    # Phase 1 container image build
└── native/   # Phase 2 Tauri port (initially empty)
```

Reasons for separate repo:

- Different release cadence (apps version independently from the OS).
- Independent CI / test infrastructure.
- Independent contributor pool.

The repo is created at the start of Phase 5 implementation work and
imported into `omni-market` Stichting-Curated tier at v1.0 release.

## Rationale

### Why phased Codium-in-container then Tauri port?

The Phase 1 container delivers a working, recognisable editor within
weeks. Users get value immediately. Phase 2 is a quality / performance
upgrade that's worth the wait but doesn't block adoption.

### Why Codium and not Zed?

The founder explicitly chose Codium (VSCode UX 1:1) to leverage the
massive existing VSCode developer mind-share and extension ecosystem.
Zed (the alternative considered) would have meant a different UX and
fewer extensions. The trade-off accepts the larger Electron footprint
for the dominant developer UX.

### Why a separate repo for OmniCode?

OmniCode is a downstream consumer of OMNI; conflating it with the OS
workspace would couple their release cycles. Keeping it separate
respects the layering: OS first, apps on top.

### Why the `Omni` prefix only for Stichting-Curated?

To make the badge meaningful. If anyone could ship "OmniCalculator", the
prefix loses its trust signal. Enforcement at marketplace submission
keeps the namespace clean.

## Backwards Compatibility

Not applicable.

## Test Cases

1. **OmniCode launch (Phase 1)**: `omni-pkg install omni-code`,
   launch, opens a VSCode-experience UI within 5s.
2. **OpenVSX extension install**: install `rust-analyzer` from
   OpenVSX through the OmniCode UI; capability prompt shown via
   `omni-helper`; install succeeds.
3. **No-telemetry verification**: tcpdump during 5-minute idle
   OmniCode session shows zero unexpected network egress.
4. **Reproducible build**: `omni-code` package built on two
   independent machines yields bit-identical hash.
5. **Capability-minimality**: OmniCode declares `fs:read:cwd`,
   `fs:write:cwd`, `net:outbound:openvsx.org:443`; binary analysis
   confirms no additional egress.

## Reference Implementation

To land before activation:
- Separate repo `omni-code/` initialized.
- Phase-1 Codium container image (`omni/linux-codium`) published to
  `omni-market`.
- OpenVSX integration validated.
- Phase-2 Tauri port spec'd in a follow-up OIP.

## Security Considerations

- **Codium upstream supply chain**: we depend on Codium upstream
  honesty + Microsoft's VSCode source. Mitigation: reproducible build
  from a pinned Codium source tag, Foundation re-builds independently.
- **Extension marketplace risk**: malicious OpenVSX extensions could
  exfiltrate code. Mitigation: capability-bound extension model;
  `omni-helper` shows extension capability set at install; Bronze tier
  default for fresh OpenVSX extensions until Silver promotion.
- **Electron CVE surface (Phase 1)**: Electron has a non-trivial CVE
  history. Mitigation: pin Codium to latest LTS Electron with
  Foundation-owned patching; container isolation makes most Electron
  CVEs irrelevant to host.

## Privacy Considerations

- OmniCode default settings: no telemetry, no auto-update via internet
  (only via `omni-pkg upgrade`), no usage analytics. All can be
  re-enabled per-feature if the user opts in.
- Extensions inherit capability scoping; an extension cannot exceed
  the OmniCode container's declared capability set.

## Future Work

- **OIP-Flagship-OmniCode-Tauri-XXX** (year 4): formal spec of the
  Tauri-port; activates Phase 2.
- **OIP-Flagship-OmniMail-XXX** (Phase 7+): first PIM-class flagship.
- **OIP-Flagship-OmniDocs-XXX** (Phase 7+): document editor (potentially
  forking LibreOffice or building from scratch on Tauri).

## Copyright

CC0 1.0 Universal.
