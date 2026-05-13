# Architecture Overview

**Status:** Draft v0.1.1 — foundational layer implemented (P1, 2026-05-10).

## Executive summary

OMNI OS is structured in concentric layers, from a custom Rust microkernel up to the application layer. AI is a first-class kernel concept, not a userspace addition. Computation can happen entirely on the local device, distributed across the user's own devices on a personal LAN cluster, federated across the global P2P mesh, or — as a last resort — sent to commercial cloud providers.

## Implementation status

| Layer | Crates | State (2026-05-10) |
|---|---|---|
| Foundational | `omni-types`, `omni-crypto`, `omni-capability` | **Implemented** (P1 closed). 131 unit tests + 7 integration tests + 4 trybuild compile-fail tests, all green. `no_std + alloc`. `omni-crypto` carries the `AWAITING_CRYPTO_REVIEW` marker pending P3.2. |
| TEE root of trust | `omni-tee` | Trait surface (`omni_capability::tee::AttestationSource`) declared in P1; concrete backends (Intel TDX, AMD SEV-SNP) land in P5. |
| Microkernel | `omni-kernel` | Stub. Bare-metal `no_std + no_main` transition is Phase 1 (P6). |
| Hardware Abstraction | `omni-hal` | Stub. P5–P6. |
| System services | `omni-runtime`, `omni-mesh`, `omni-tokenization` | Stubs. Phase 2+. The `omni-tokenization` crate is the only one authorised to enable `omni-types`'s `_tokenization_provider` feature flag (the construction gate for `EncryptedString` and friends). |
| User-facing | `omni-sdk`, `omni-agent`, `omni-shell` | Stubs. Phase 2+. |

See [`/todo.md`](../todo.md) for the active backlog and [`/CHANGELOG.md`](../CHANGELOG.md) for the per-release record.

## High-level system layers

```
┌─────────────────────────────────────────────────────────────────────┐
│                  Applications and Agents (userspace)                │
├─────────────────────────────────────────────────────────────────────┤
│   Application SDK   │   Agent Framework    │   System UI / Shell   │
├─────────────────────────────────────────────────────────────────────┤
│  AI Runtime  │  Mesh Protocol  │  Filesystem  │  Networking  │ ... │
│   Service    │     Service     │   Service    │    Service   │     │
├─────────────────────────────────────────────────────────────────────┤
│             Microkernel — Rust, message-passing IPC                 │
│   Memory mgmt │ Scheduling │ Capabilities │ IPC primitives          │
├─────────────────────────────────────────────────────────────────────┤
│   Tensor HAL  │   Network HAL   │   Storage HAL  │   TEE HAL        │
├─────────────────────────────────────────────────────────────────────┤
│   Hardware: CPU + NPU/GPU + TEE + Secure Storage + Network          │
└─────────────────────────────────────────────────────────────────────┘
```

### Microkernel (Rust)

OMNI OS is built on a microkernel architecture, written entirely in Rust (2024 edition). The kernel is responsible only for:

- Memory management (virtual memory, page tables, allocators)
- Process and thread scheduling
- Inter-process communication (typed message passing)
- Capability-based security primitives
- Hardware abstraction interfaces (HAL contracts)

Everything else — filesystems, drivers, networking stacks, AI runtime — runs as user-space services communicating via IPC. This minimizes the trusted computing base (TCB) and provides strong isolation between subsystems.

The microkernel choice is motivated by:

- **Security**: smaller TCB → smaller attack surface.
- **Stability**: faults in one service do not crash the kernel.
- **Modularity**: services can evolve and be replaced without kernel changes.
- **Verifiability**: a small kernel is amenable to formal methods over time.

### AI Runtime Service

A privileged user-space service that exposes AI as a system primitive. Responsibilities:

- Model lifecycle (load, unload, version, attest)
- Inference scheduling across available accelerators
- Capability validation for AI invocations
- Routing decisions across execution tiers
- Tokenization and encrypted-data-type support

System calls exposed to applications:

- `ai_invoke(model, prompt, capability) -> response`
- `ai_stream(model, prompt, capability) -> stream<token>`
- `ai_embed(model, text, capability) -> vector`
- `ai_classify(model, input, capability) -> label`
- `ai_transcribe(model, audio, capability) -> text`

All calls take a capability token; the AI Runtime Service refuses calls without valid capabilities.

### Mesh Protocol Service

Manages all peer-to-peer interactions: discovery, authentication, routing, compute credit accounting, compliance proof generation and verification. Detailed in [03-mesh-protocol.md](./03-mesh-protocol.md).

### Tensor HAL

Hardware Abstraction Layer for AI accelerators. Processes targeting AI workloads do not need to know whether inference runs on CPU AVX-512, integrated GPU, discrete GPU, or NPU. The HAL handles dispatch and resource allocation.

Supported backends (planned for v1):

- CPU (with AVX-512 / AVX2 fallback)
- NVIDIA CUDA (via wrapper, runtime-loaded)
- AMD ROCm (via wrapper, runtime-loaded)
- Apple Metal (v1.1+)
- Intel/AMD integrated GPU via Vulkan compute

### TEE HAL

Hardware Abstraction Layer for Trusted Execution Environments. Provides a uniform API for:

- Generating remote attestation reports
- Provisioning sealed keys
- Executing confidential workloads
- Sealed memory regions

Supported TEEs (v1):

- Intel TDX
- AMD SEV-SNP

Future (v1.1+): Apple Secure Enclave, ARMv9 CCA Realms.

## Execution tiers

OMNI OS evaluates each AI workload against four execution tiers and selects the most appropriate based on workload sensitivity, user policy, available resources, and latency requirements.

### Tier 0 — Local-only (default)

The workload runs entirely on the local device. No network involved. Used for:

- Lightweight assistants (autocomplete, classification, embedding)
- Sensitive data that must never leave the device
- Offline operation
- Real-time interactive workloads

**Constraints:** limited by local hardware capacity. Suitable for models up to ~8B parameters (quantized).

### Tier 1 — Personal Cluster

The user's own devices (laptop + desktop + tablet + phone) discover each other via mDNS on the local network and form a private cluster, encrypted with mTLS. Models are split across devices using pipeline parallelism.

**Constraints:** requires LAN. Latency between devices must be < 5ms. Suitable for models up to ~70B parameters using aggregated VRAM.

### Tier 2 — Federated Mesh (opt-in)

Opt-in P2P network of OMNI OS instances. Detailed in [03-mesh-protocol.md](./03-mesh-protocol.md). Uses MoE expert distribution: each expert (or expert group) is hosted on different nodes; only 2 of N experts are active per token.

**Constraints:** higher latency (≥30ms RTT typical). Best for asynchronous, long-form workloads. Suitable for models 100B+ parameters.

**Privacy:** all payloads are wrapped in TEE-only decryption envelopes; PII is tokenized; compliance proofs are mandatory. See [04-security-model.md](./04-security-model.md).

### Tier 3 — Commercial cloud (opt-in, last resort)

Used only when explicitly authorized by the user for a specific query, or when no other tier is feasible and the user has pre-approved cloud fallback. Always requires explicit consent. Privacy budget consumption is tracked.

## Model architecture: MoE-first

The reference public model for OMNI OS uses a Mixture of Experts (MoE) architecture:

- 16 to 32 experts per layer (final number set by reference model selection at v1 implementation)
- Top-2 expert selection per token (sparse activation)
- Expert weights distributable across mesh nodes
- Only 2 of N experts active per token → minimal cross-node traffic per inference step

This architecture is chosen because it natively supports fragmentation across the federated mesh. Pipeline parallelism remains usable for personal cluster scenarios, where latency is low and dense models can be efficiently split layer-wise.

Dense models (non-MoE) are supported as second-class citizens: they can run locally or in personal cluster, but are not first-class for federated mesh.

## Privacy primitives (architectural)

The architecture mandates that PII never travels in cleartext over the mesh. This is enforced at the protocol level by:

1. **Encrypted-by-default data types** at OS API level (`EncryptedString`, `MaskedSSN`, `TokenizedEmail`, etc.).
2. **Tokenization service** that replaces PII with deterministic tokens before any inference.
3. **Format-preserving encryption** (FF1, FF3-1) for routing metadata.
4. **Compliance proofs** (zk-SNARKs or signatures) attached to every mesh payload.
5. **TEE-only decryption envelope** — sensitive data is decryptable only inside attested enclaves.

Detailed in [04-security-model.md](./04-security-model.md).

## Capability-based security

Every system action requires a capability token: a cryptographically signed structure that names the action, the actor, the resource, and time bounds. Capabilities are issued by the kernel, stored in TPM/Secure Enclave, and verified at every boundary.

This replaces the traditional Unix permission model, which is insufficient for AI agents that may compose actions across many resources.

Capability properties:

- **Scoped**: name a specific action and resource.
- **Time-bounded**: short TTL (minutes), refreshed as needed.
- **Attenuable**: an agent can derive a more restricted child capability for a sub-agent (Macaroons-style).
- **Revocable**: short TTL + revocation list ensures fast revocation.

## Implementation choices (committed)

| Decision | Choice | Rationale |
|---|---|---|
| Language | Rust 2024 edition | Memory safety + performance + crypto ecosystem |
| Architecture | Custom microkernel | Minimal TCB, full control, generational stability |
| Initial hardware | x86_64 with TDX/SEV-SNP | TEE-attestable, mainstream developer hardware |
| Model architecture | MoE | Mesh-friendly fragmentation |
| License | AGPL-3.0 + commercial (dual) | Mission protection + funding flexibility |

See [09-tech-specifications.md](./09-tech-specifications.md) for exact versions.

## OMNI App Mesh — the user-facing AI-native layer

OMNI OS treats application discovery, installation, generation, and marketplace curation as **integrated OS primitives**, not as orthogonal apps. The five components are governed by five OIPs filed 2026-05-12:

```
┌────────────────────────────────────────────────────────────────────┐
│  OMNI Helper (OIP-Helper-007)                                       │
│  • detects need (file-failure / explicit-invoke / watch opt-in)     │
│  • 3 autonomy levels: Autonomous / Guided (default) / Inform        │
│  • mandatory Impact Dashboard (Privacy / Trust / Cost / Time)       │
│  • escalation taxonomy for destructive / privacy / cap-escalation   │
│  • 30s undo window in Autonomous mode                               │
└───────────────────────────────┬────────────────────────────────────┘
                                ▼
              ┌─────────────────┴─────────────────┐
              │                                   │
   ┌──────────▼──────────┐         ┌──────────────▼──────────────┐
   │ omni-pkg (008)      │         │ omni-forge (009)            │
   │ content-addressed   │         │ Rust → WASM/ELF on-demand   │
   │ federated package   │         │ generation pipeline; LLM    │
   │ manager, Sigstore   │         │ source gen + static analysis│
   │ + CT log mandatory; │         │ + capability inference +    │
   │ capability manifest │         │ TEE-bound ephemeral signing │
   │ atomic upgrade      │         │ + mandatory first-run review│
   └──────────┬──────────┘         └──────────────┬──────────────┘
              │                                   │
              ▼                                   ▼
   ┌──────────────────────────────────────────────────────────────┐
   │ omni-market (OIP-Market-010)                                  │
   │ Stichting-curated marketplace + community-federated optional  │
   │ Bronze / Silver / Gold / Stichting-Curated tiers              │
   │ continuous CVE scan with public SLA (Critical: 14d)           │
   │ 0% OSS / 10% commercial / 0% Stichting-sponsored commission   │
   └──────────────────────────┬────────────────────────────────────┘
                              ▼
   ┌──────────────────────────────────────────────────────────────┐
   │ Omni* flagship apps (OIP-Flagship-011)                        │
   │ OmniCode (Codium-in-container Phase 1, Tauri-native Phase 2)  │
   │ OmniShell · OmniMail · OmniNotes · OmniDocs · OmniPhotos …    │
   │ Stichting-Curated tier in omni-market; AGPL-3.0; no telemetry │
   └──────────────────────────────────────────────────────────────┘
```

The same `OmniContainer` engine (per [OIP-Container-006](../oips/oip-container-006.md))
runs Linux apps from omni-pkg, Windows apps via Wine-in-container, AOT-generated apps from omni-forge, and flagship apps. The Helper, Pkg, Forge, Market, and Flagship layers all converge on a single execution substrate.

This synthesis — agentic discovery + federated package manager + generation pipeline + Foundation-curated marketplace + flagship reference apps — has no equivalent in Windows / macOS / Linux today, and is the single most distinguishing feature of OMNI OS at the user-experience layer.

## Open architectural questions

These will be resolved during Phase 1 implementation, captured as OIPs:

- **IPC message format**: Cap'n Proto vs. custom binary format. Cap'n Proto is mature; custom can be more compact.
- **Driver model**: separate processes per driver (max isolation, higher overhead) vs. driver service composition.
- **Boot architecture**: UEFI-only vs. UEFI + legacy BIOS support. Likely UEFI-only given hardware baseline.
- **Filesystem**: native OMNI FS vs. existing options (ZFS port, ext4 via compatibility).
- ~~**POSIX compatibility layer**: yes/no/partial. Affects userspace porting effort vs. ideological purity.~~ **Resolved by [`OIP-Container-006`](../oips/oip-container-006.md) (2026-05-12):** no POSIX in the OMNI kernel; POSIX exists only inside guest Linux of OmniContainers (micro-VM container engine with capability-bound virtio I/O). Linux apps and Windows apps (via Wine-in-container) are first-class via this path.
