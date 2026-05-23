# OMNI OS — Multichannel Interaction and Linux/Windows Compatibility — Analysis

> **Author:** `cySalazar` (analysis pass on branch `claude/omni-os-features-analysis-Syk8p`).
> **Created:** 2026-05-22.
> **Status:** Informational analysis. Not normative. Companion to two new OIP drafts
> (`oips/oip-multimodal-ux-XXX.md`, `oips/oip-compat-status-XXX.md`).
> **Scope:** answer two questions raised against the codebase as of branch state
> 2026-05-22:
>
> 1. Is an **omni-channel** interaction surface planned (voice, webcam, chat, email,
>    Telegram, WhatsApp, …) in line with the project philosophy? If not, what would it
>    take to add it, and what are the trade-offs? Accessibility (A11y) is an explicit
>    goal of the question.
> 2. Does a Wine-like compatibility layer toward Linux **and** Windows make sense, and
>    is it worth it?
>
> This document does not propose code changes. It cites concrete file paths so a
> reviewer can verify each claim independently.

---

## 0. Executive summary (TL;DR)

- **Question 1 — Omni-channel UX is not coded, but is philosophically aligned.** The
  building blocks already exist on paper: capability tokens
  (`crates/omni-capability/src/lib.rs`), an intent-based shell with plan-then-execute
  (`crates/omni-shell/src/lib.rs`), a sandboxed agent runtime
  (`crates/omni-agent/src/lib.rs`), a PII tokenization vault
  (`crates/omni-tokenization/src/lib.rs`), and an AI runtime that already declares
  `ai_transcribe(model, audio, capability) -> text` as a first-class syscall
  (`docs/02-architecture.md:78`). What is missing is **HAL surface for audio/video**,
  **drivers** (no USB stack today; the HAL only defines `tensor`/`network`/`storage`/
  `tee`, `crates/omni-hal/src/lib.rs`), **on-device STT/TTS/vision models**, and
  **messaging-egress policy**. A new OIP can land all of this without touching the
  microkernel's invariants. See § 2 below and `oips/oip-multimodal-ux-XXX.md`.
- **Question 2 — Wine-like compatibility is already designed.** OIP-Container-006
  (`oips/oip-container-006.md` § 8) commits OMNI to micro-VM containers with a
  Stichting-signed guest Linux kernel and a maintained Wine image
  (`omni/linux-wine:N-stable`, currently 11-stable, with DXVK + VKD3D-Proton). The
  microkernel itself stays POSIX-free; Linux **and** Win32 semantics live inside the
  guest VM. There is nothing new to decide; the work is tracked in `todo.md` as
  P8.1–P8.6 (Wine specifically is P8.6, blocked on P8.3 guest-Linux pipeline). See
  § 3 below and `oips/oip-compat-status-XXX.md` for a public-facing rationale.

---

## 1. Method

Three Explore passes across the repository (vision/security, agent/shell/UX, container
/ABI/compat) plus a manual read of the canonical OIPs (`oip-container-006`,
`oip-helper-007`, `oip-flagship-011`), the architecture and threat-model documents
(`docs/02-architecture.md`, `docs/04-security-model.md`, `docs/04a-threat-model.md`),
the roadmap (`docs/06-roadmap.md`) and the in-flight development plan
(`docs/planning/2026-05-21-development-plan.md`). Keyword sweeps over the whole tree
for `audio|microphone|webcam|voice|speech|stt|tts|whisper|telegram|whatsapp|email|smtp|
imap|notification|accessibility|screen reader` to confirm which surfaces are
**absent** in code.

---

## 2. Question 1 — Omni-channel interaction (voice, webcam, chat, messaging)

### 2.1 Status by channel

| Channel | Present in code | Planned (docs/OIP) | Notes |
|---|---|---|---|
| **Text shell (CLI)** | scaffold | ✓ Phase 1 → intent-based Phase 4+ | `crates/omni-shell/src/lib.rs` (Draft v0.1, three stub modules `cli`/`command`/`repl`). |
| **Natural-language intent** | scaffold | ✓ OIP-Helper-007 | `omni-helper` daemon with three autonomy levels (`Autonomous`/`Guided`/`Inform`) and a mandatory **Impact Dashboard** on Privacy/Trust/Cost/Time. |
| **Speech-to-text (STT)** | ✗ | ✓ syscall declared | `ai_transcribe(model, audio, capability) -> text` listed at `docs/02-architecture.md:78`. No model integration, no microphone path. |
| **Text-to-speech (TTS)** | ✗ | ✗ | Not declared as a syscall; not in roadmap. |
| **Webcam / vision** | ✗ | ✗ | No V4L2-equivalent trait, no USB Video, no vision model pipeline. `ai_classify` exists for batch inputs but is not wired to live capture. |
| **Audio drivers** | ✗ | ✗ | HAL exposes only `tensor`/`network`/`storage`/`tee` (`crates/omni-hal/src/lib.rs`). |
| **USB stack** | ✗ | ✗ | No USB host controller driver; no USB Audio Class / USB Video Class. |
| **Bluetooth** | ✗ | ✗ | Not referenced. |
| **Email (SMTP/IMAP)** | ✗ | △ flagship `OmniMail` Phase 7+ | `oips/oip-flagship-011.md`. |
| **Telegram / WhatsApp / Matrix / SMS** | ✗ | ✗ | Zero matches across the tree. |
| **Notifications** | ✗ | ✗ | No notification bus defined. |
| **Accessibility (screen reader, captioning, keyboard navigation)** | ✗ | △ implicit | Threat model mentions adversarial multimodal inputs (`docs/04a-threat-model.md` A2 class); no A11y framework. |

### 2.2 Philosophical alignment

The proposed feature does **not** require softening any project pillar:

- **Local-first.** Tier 0 inference exists by design (`docs/02-architecture.md` § Execution tiers). On-device STT (Whisper-tiny class, ≈ 39 M params) and TTS (Piper class, ≈ 60 M params) run within Tier 0 budgets; no audio needs to leave the device.
- **Privacy-by-construction.** `omni-tokenization` already turns PII into deterministic tokens inside a per-user TEE-sealed vault before any egress; the same path applies to transcripts before they reach a third-party messaging provider.
- **Capability model.** Microphone, camera and outbound messaging map cleanly onto new `KernelAction`/`KernelResource` variants alongside the existing `IpcSend`/`IpcRecv` (`crates/omni-kernel/src/capabilities.rs:194-201`). The Macaroons-style attenuation, TTL, and revocation behaviour all apply unchanged.
- **Plan-then-execute.** `omni-helper`'s three autonomy levels and Impact Dashboard (OIP-Helper-007 §§ 2–3) are already designed to gate exactly the destructive / privacy-violating / capability-escalating actions that a voice front-end would request. Reusing them avoids inventing a second UX policy surface.
- **No client of closed services in the TCB.** Telegram / WhatsApp / proprietary SMTP gateways live in userspace, ideally inside an OmniContainer (OIP-Container-006), and **never** in the host kernel.

### 2.3 Mini-architecture (high level — design lives in `oips/oip-multimodal-ux-XXX.md`)

```
┌────────────────────────────────────────────────────────────────────────┐
│  User                                                                   │
│   • voice (mic) ── speak ── camera ── keyboard ── screen reader (TTS)  │
└──────────────┬─────────────────────────────────┬───────────────────────┘
               │                                 │
               ▼                                 ▼
       ┌───────────────────┐           ┌───────────────────┐
       │  AudioDevice HAL  │           │  VideoDevice HAL  │   ◄── new HAL traits
       │   (USB Audio,     │           │   (USB Video,     │
       │    on-SoC codec)  │           │    on-SoC ISP)    │
       └──────────┬────────┘           └──────────┬────────┘
                  │                               │
                  │  AudioCapture cap             │  VideoCapture cap         ◄── new caps
                  ▼                               ▼
       ┌───────────────────────────────────────────────────────┐
       │  AI Runtime Service (ai_listen / ai_speak / ai_see)   │   ◄── extends `ai_transcribe`
       │   • Tier 0 by default (Whisper-tiny, Piper, vision-q) │
       └──────────┬────────────────────────────────────────────┘
                  │  text + intent
                  ▼
       ┌───────────────────────────────────────────────────────┐
       │  omni-tokenization (PII → token, TEE-sealed vault)    │
       └──────────┬────────────────────────────────────────────┘
                  ▼
       ┌───────────────────────────────────────────────────────┐
       │  omni-agent planner  →  Impact Dashboard (OIP-007)    │
       │   • autonomy level (Autonomous/Guided/Inform)         │
       │   • escalation taxonomy (destructive / privacy / cap) │
       └──────────┬────────────────────────────────────────────┘
                  │  approved plan
                  ▼
       ┌───────────────────────────────────────────────────────┐
       │  omni-shell executor — capability-bound, audit-logged │
       └──────────┬───────────────────────┬────────────────────┘
                  ▼                       ▼
        run / install / search   MessagingEgress(channel) cap
        (existing omni-pkg,           ▼
         omni-forge per OIP-008/9)   ┌──────────────────────────────────┐
                                     │  userspace messaging adapters    │
                                     │   • SMTP/IMAP  (sandboxed)       │
                                     │   • Telegram bot API (Container) │
                                     │   • WhatsApp via supported API   │
                                     │   • Matrix (preferred default)   │
                                     │  tokenize-or-refuse on egress    │
                                     └──────────────────────────────────┘
```

Key invariants for this design:

1. **No proprietary messaging client lives in the host kernel or in the TCB.** All egress adapters are userspace and ideally run inside an OmniContainer micro-VM (OIP-006). The kernel only sees an opaque `MessagingEgress(Channel)` capability.
2. **All cross-device egress goes through `omni-tokenization` first.** Audio/video that has been transcribed locally is reduced to text + intent before any third-party adapter sees it; raw audio/video does not leave the device by default. The policy is `tokenize-or-refuse`.
3. **High-risk intents always go through Impact Dashboard.** Reuse, do not duplicate, OIP-Helper-007 escalation taxonomy. "Send a WhatsApp message on the user's behalf" is by definition in the **Destructive** class ("sending messages on the user's behalf to third parties", OIP-Helper-007 § 3 table). No silent `Autonomous` execution.
4. **A11y is a first-class application, not a bolt-on.** A native screen reader and live captioning are the canonical Tier-0 consumers of the very same `ai_speak`/`ai_listen` stack — they share the model, the capability, and the audit log. Zero additional kernel surface.
5. **Webcam-as-context is opt-in per session and rate-limited.** "OMNI can see me" requires an explicit, short-TTL `VideoCapture` capability that auto-revokes; long-running watch is escalated even in `Autonomous` mode (mirrors `watch-always-on` policy in OIP-Helper-007 § 1).

### 2.4 Pros

- **Accessibility "by design."** A native, on-device screen reader and captioning are the cheapest possible consequence of having a TTS/STT stack at all.
- **Zero compromise on local-first.** The full happy path (mic → STT → planner → executor → TTS) runs at Tier 0 with quantized models in the < 200 MB RAM envelope; no network needed.
- **Reuses existing primitives.** Capability tokens, tokenization, intent UX, audit log, sandbox: nothing new to invent at the policy layer.
- **Single mechanism handles "talk to the OS" *and* "ask the OS to send a message"** without expanding the kernel ABI: outbound messaging is just one more capability variant.
- **Strong differentiator vs. mainstream OSes** that bind voice assistants to a cloud backend (Cortana, Siri, Google Assistant). OMNI's pitch is "voice assistant that physically cannot phone home unless you say so."

### 2.5 Cons / risks

- **TCB growth from new hardware surface.** USB host controllers + USB Audio + USB Video classes are historically rich in CVEs. The HAL trait must be small; the driver crates must run in user-mode driver processes with per-domain IOMMU (already in flight at P6.7.9), not in kernel mode.
- **Adversarial multimodal inputs** (audio prompt injection, image prompt injection, voice deepfake spoofing) — already on the threat model radar (`docs/04a-threat-model.md` A2). Mitigations: input pre-processing (randomized smoothing), provenance binding (the planner must know "this intent came from voice channel X at time T"), and **multi-model agreement** for any action in the OIP-007 escalation taxonomy.
- **Egress to closed messaging providers** (WhatsApp, Telegram official servers) is by definition non-TEE-attested. The `tokenize-or-refuse` rule is the right policy, but it adds friction: the user will sometimes want to send raw text that contains their own real name. Need clear UX for this trade-off.
- **Power and RAM cost of multimodal models** vs. the `tokens-per-second-per-watt` first-order metric (`docs/01-vision.md`). Whisper-tiny + Piper + a small vision-classify model fit comfortably in commodity laptops; larger models start eroding the headline metric.
- **Engineering cost.** Audio HAL + USB Audio driver + STT/TTS model integration + capability extensions + intent pipeline ≈ a sub-phase comparable in scope to a fraction of P8 (container engine). Concretely 6–12 engineer-months for MVP; another 6–10 for video and a polished A11y story.
- **Wake-word / continuous listening is a privacy hazard** and is excluded from the MVP. Push-to-talk first; wake-word only as a future, opt-in OIP with mandatory `Guided`-or-stricter escalation.

### 2.6 Phased introduction (cheapest → completest)

1. **Phase 2.5 — audio MVP.**
   - HAL: `AudioDevice` trait (PCM capture + playback, sample rate, channels) in `omni-hal`.
   - Driver: USB Audio Class 1.0 in a user-mode driver process (reuse the per-domain IOMMU work landing at P6.7.9-pre.10).
   - Models: Whisper-tiny (STT) + Piper (TTS) executed inside `omni-agent` WASM sandbox or a runtime-loaded native process bound by `omni-agent` budgets.
   - Caps: `AudioCapture`, `AudioPlayback`, default deny, short TTL.
   - Syscalls: `ai_listen`, `ai_speak` added to the AI Runtime Service ABI alongside `ai_transcribe`.
2. **Phase 2.5 — intent pipeline.** Wire STT output through `omni-tokenization` → `omni-agent` planner → Impact Dashboard (reuse OIP-007 implementation; no new UX policy). End-to-end demo: "Open Firefox", "Install GIMP", "Search local notes for 'budget'", with `Guided`-mode approval.
3. **Phase 2.6 — A11y app.** Native screen reader + live captioning as the first flagship A11y consumer. Direct beneficiary, zero new surface.
4. **Phase 3 — video.** `VideoDevice` HAL trait (USB Video Class), `VideoCapture` capability, quantized vision-classify model, `ai_see` syscall. Short-TTL, per-session, never `watch-always-on` by default.
5. **Phase 3.5 — messaging.** Outbound only first: SMTP via a sandboxed adapter (or inside OmniContainer), then Matrix (open protocol, our preferred default), then Telegram bot API. WhatsApp via official Business API only, in OmniContainer, with mandatory tokenize-or-refuse. Inbound (incoming messages → OMNI notification → planner) lands in Phase 4 once OmniMail is in flight.
6. **Phase 4+ — wake-word, multi-user voiceprint, on-device translation.** Distinct future OIPs.

### 2.7 Timing — why now

The earliest sensible filing is **after** P6 closes (microkernel POC done) and **before**
OmniMail enters detailed design in Phase 7+: that way the multichannel UX OIP shapes
OmniMail's APIs rather than retro-fitting them. The development plan
(`docs/planning/2026-05-21-development-plan.md`) already focuses Phase 2 on the AI
Runtime Service — that is the natural attachment point for `ai_listen`/`ai_speak`/
`ai_see`.

---

## 3. Question 2 — Wine-like compatibility toward Linux and Windows

### 3.1 The answer is already in OIP-Container-006

`oips/oip-container-006.md` § 8 ("Wine integration for Windows applications")
specifies a Stichting-maintained image, `omni/linux-wine:N-stable` (currently
`omni/linux-wine:11-stable`), bundling Wine LTS + DXVK + VKD3D-Proton + a Wine-prefix
init script. User-facing surface:

```bash
omni-container run-windows photoshop.exe \
    --wine-prefix=/home/<user>/.wine/photoshop \
    --profile=windows-app
```

which expands to a normal `omni-container run` with the `omni/linux-wine:11-stable`
image, capability-bound virtio I/O, and (where available) TEE-attested confidential
VM via Intel TDX or AMD SEV-SNP (`oips/oip-container-006.md` §§ 1, 8).

### 3.2 Why **not** a kernel-resident shim (no personality, no binfmt, no namespace fallback)

OIP-Container-006 explicitly rejects three alternatives (§ 2):

| Approach | Drawback (OIP wording) |
|---|---|
| Full POSIX in kernel | Doubles kernel ABI; legacy semantics (`fork`/`setuid`/`/proc`) leak into the OMNI capability model. |
| Partial POSIX shim | Leaky abstraction (WSL1 was abandoned for this reason); coverage 60–80 %. |
| No POSIX at all | Ecosystem isolation (Plan 9 risk). |

The chosen path — **POSIX exists, but only inside the guest Linux of a micro-VM,
never in the OMNI kernel** — preserves the capability-pure microkernel **and**
ships ≥ 99 % Linux app compatibility (because the guest is real Linux, not a shim).

Windows compatibility piggy-backs on the same mechanism: Wine runs inside the guest
Linux, so adding Win32 coverage costs **one image**, not a second kernel ABI. There
is no Wine-equivalent code to write in the OMNI host kernel and no syscall
translation surface to maintain.

### 3.3 Is it worth it?

Yes, and the trade-offs are tilted in OMNI's favour:

- **Coverage.** Wine reports ~85–95 % productivity Win32 coverage and ~75–90 %
  gaming coverage via DXVK + VKD3D-Proton (Steam Deck / ProtonDB data,
  `oips/oip-container-006.md:328–333`). Combined with native Linux container
  coverage that is essentially "anything that runs on a recent kernel", this
  is sufficient for mainstream adoption.
- **Single mechanism.** One container engine, two base images (`omni/linux-base`
  and `omni/linux-wine`) cover both Linux and Windows ecosystems. No second
  product, no second code path.
- **TEE attestation for free.** Each container becomes a confidential VM on
  TDX/SEV-SNP capable hosts. Wine inherits this; the user gets attestation on a
  Photoshop session without Wine ever knowing.
- **Cost is already in the budget.** `todo.md` tracks the work as P8.1
  (engine skeleton) → P8.6 (Wine image), with P8.6 blocked on P8.3 (signed guest
  Linux pipeline). Estimated ≈ 20–30 engineer-months for production readiness;
  no new architecture decision is required.

Known ceilings (not a reason to reconsider):

- Kernel-mode Windows drivers cannot run under Wine. OIP-006 § 8 marks this for a
  future v2.x "user-licensed Windows in a container" path.
- Anti-cheat and some DRM rely on kernel-mode shims and will not work.
- Real-time low-latency video work (DAW-style) inside a micro-VM is harder than
  on native Linux; not a Wine issue, a virtualization issue.

### 3.4 What is still missing (already in the plan, just unbuilt)

- P8.3 — reproducible, Stichting-signed guest Linux image pipeline.
- P8.1 — KVM ioctl wiring + the three engine backends (`KvmEngine`, `TdxEngine`,
  `SevSnpEngine`) per `oips/oip-container-006.md:344–360`.
- P8.4 — virtio backend implementations (fs, net, vsock, gpu, rng).
- P8.6 — Wine image build (`omni/linux-wine:11-stable`) on top of P8.3.

No new OIP is required for question 2; a short Informational OIP
(`oips/oip-compat-status-XXX.md` in this branch) is proposed only to give the
question "why not a native Wine in OMNI?" a single canonical answer.

---

## 4. Alignment summary

| Proposed capability | Alignment with project pillars |
|---|---|
| `AudioDevice` / `VideoDevice` HAL traits + user-mode drivers | ✓ matches per-domain IOMMU model (P6.7.9), no kernel growth |
| `ai_listen` / `ai_speak` / `ai_see` syscalls | ✓ peers of declared `ai_transcribe`, capability-gated |
| Tier-0 Whisper-tiny + Piper + vision-classify-q | ✓ local-first, fits `tokens-per-second-per-watt` |
| Intent pipeline reusing OIP-Helper-007 | ✓ no second policy surface |
| `MessagingEgress(Channel)` capability + tokenize-or-refuse | ✓ matches encrypted-by-default and tokenization invariants |
| Userspace IM/email adapters in OmniContainer | ✓ matches OIP-006 anti-shim stance |
| A11y as canonical TTS/STT consumer | ✓ no extra surface |
| Wake-word / continuous listening | ⚠ MVP excludes it; future OIP only, with mandatory escalation |
| Telegram/WhatsApp **client** in TCB | ✗ explicitly disallowed |
| Wine via micro-VM container | ✓ already specified in OIP-006 |
| POSIX/Win32 shim in OMNI kernel | ✗ explicitly disallowed by OIP-006 § 2 |

---

## 5. References

- `docs/01-vision.md` — project pillars, anti-goals, `tokens-per-second-per-watt`.
- `docs/02-architecture.md` § "AI Runtime Service" (lines 62–80) — declared syscalls including `ai_transcribe`.
- `docs/02-architecture.md` § "Execution tiers" — Tier 0 local-first envelope.
- `docs/04-security-model.md`, `docs/04a-threat-model.md` — capability model, multimodal adversaries (A2).
- `docs/06-roadmap.md` — phase ordering.
- `docs/planning/2026-05-21-development-plan.md` — current implementation waves.
- `crates/omni-hal/src/lib.rs` — current HAL traits (`tensor`, `network`, `storage`, `tee`).
- `crates/omni-kernel/src/capabilities.rs` (around lines 194–201) — `KernelCapabilityCheck` trait, current `KernelAction` / `KernelResource` variants.
- `crates/omni-capability/src/lib.rs` — Macaroons-style capability tokens.
- `crates/omni-agent/src/lib.rs`, `crates/omni-shell/src/lib.rs` — agent and shell scaffolds.
- `crates/omni-runtime/src/lib.rs` — AI runtime scaffold.
- `crates/omni-tokenization/src/lib.rs` — PII tokenization vault.
- `oips/oip-helper-007.md` — autonomy levels and Impact Dashboard (reused).
- `oips/oip-container-006.md` § 8 — Wine integration.
- `oips/oip-flagship-011.md` — OmniMail / OmniCode flagship roadmap.
- `oips/oip-multimodal-ux-XXX.md` (this branch) — Standards Track companion proposal.
- `oips/oip-compat-status-XXX.md` (this branch) — Informational companion note.
