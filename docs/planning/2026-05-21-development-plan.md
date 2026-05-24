# OMNI OS Development Plan — 2026-05-21

> **Author:** `planner-v2` (Development Planner agent under team `dev-cycle`).
> **Target reviewer:** Matteo Sala / cySalazar (project lead).
> **Status:** **DRAFT — pending approval.** No implementer should start work until the lead
> approves this plan (or escalates a specific task to the OIP Review Team).
> **Scope:** development backlog for the next 4–8 implementation waves. Funding-dependent
> and hardware-dependent items are listed but explicitly marked.
>
> **Hard rules applied while drafting:**
> 1. No task is listed without unit tests (and E2E where behaviour is observable).
> 2. No new dependency is proposed without a version pin checked against
>    `docs/09-tech-specifications.md` and verified Apache-2.0-compatible against
>    `deny.toml`.
> 3. Security > Stability > Performance when two tasks compete for the same wave slot.
> 4. This file is the only artifact this planner pass writes. `todo.md` is **not** modified
>    — proposed changes to `todo.md` live in **TASK-014** below for the lead to apply.

---

## 1. Current state snapshot

### 1.1 Phase position (per `docs/06-roadmap.md`)

- **Phase 0 — Foundation:** ~75 % closed (governance ✅, foundational crates ✅, OIP
  process ✅; **Stichting registration P4.1, funding P4.2, cryptographer P3.2 still
  open** — funding-dependent).
- **Phase 1 — Microkernel POC:** **~99.9 % closed**. `v0.3.0-alpha.1` shipped 2026-05-20
  (PR #36 squash-merged on `main` as `85293b8` + signed tag + GitHub pre-release with
  attached CycloneDX SBOM). P6.7.8.9 (capability deposit trampoline) landed on `main`
  same day. Remaining Phase 1 deliverables:
    - **P6.7.8.10** — driver-shared SDK helper (token lookup at the well-known deposit
      VA). Not started.
    - **P6.7.9** — live driver bring-up + Proxmox smoke (virtio-net, NVMe, e1000e).
      Not started.
    - **P5.2 / P5.3** — Intel TDX + AMD SEV-SNP real TEE backends. **Funding-dep.**
    - **P6.8** — first external kernel + capability audit. **Funding-dep.**
- **Phases 2–7:** 0 %.

### 1.2 What is implemented (verified from `crates/` and recent commits)

| Area | State |
|---|---|
| `omni-types` | ✅ Implemented (`no_std + alloc`, RFC vectors, `wire::{encode_canonical, decode_canonical}` per OIP-Serde-004). |
| `omni-crypto` | ✅ Implemented (`no_std + alloc`, RustCrypto family). Carries `AWAITING_CRYPTO_REVIEW` marker pending P3.2. |
| `omni-capability` | ✅ Implemented + `Action::{MmioMap, DmaMap, IrqAttach, PciConfigRead, PciConfigWrite, DriverLoad, DriverUnload, TeeProbe}` + `Resource::{PciDevice, MmioRegion, DmaWindow, IrqLine}` per OIP-013 § S1. |
| `omni-tee` | 🟡 Stub `AttestationSource` trait + `MockTeeBackend` only; TDX/SEV-SNP backends TBD (P5.2/P5.3). |
| `omni-kernel` | ✅ MB1–MB14.h.2 closed. Bare-metal `no_std + no_main` on `x86_64-unknown-none`. Frame allocator, 4-level paging, IDT, SYSCALL/SYSRET + INT 0x80, ELF64 loader, scheduler, LAPIC preemption, per-process CR3 Ring 3, IPC + multi-task, kernel-stack isolation, MP boot (BSP+APs), TLB shootdown, per-CPU run queues, x2APIC, AP dispatch + cross-CPU context switch under `SCHED_LOCK`. Driver framework wiring: `MmioMap (70)`, `DmaMap (71)`, `IrqAttach (72)`, `DriverLoad (73)` syscall handlers + `entropy.rs` (ChaCha20 CSPRNG) + `driver_cap_issuer.rs` (DEV-ONLY static Ed25519 seed) + `cap_deposit.rs` (8-page 32 KiB RO window @ user-VA `0x0010_0000`) + `known_issuers.rs` (empty in Phase 1) + `driver_manifest.rs` (omni-pack v1 decoder + BLAKE3 + Ed25519 verify). Track A desktop demo (GOP, font, cursor, PS/2 + VirtIO tablet, widget toolkit, WM, RTC, ACPI S5, Build Info panel) all live. |
| `omni-driver-net-virtio` (lib + image sibling) | ✅ Scaffold + FSM (7-state bring-up) + bootable Ring 3 ELF. No live syscalls yet. |
| `omni-driver-nvme` (lib + image sibling) | ✅ Scaffold + FSM (13-state bring-up, PRP-only) + bootable Ring 3 ELF. No live syscalls yet. |
| `omni-driver-e1000e` (lib + image sibling) | ✅ Scaffold + FSM (13-state bring-up) + bootable Ring 3 ELF. No live syscalls yet. |
| `omni-hal` | 🟡 Trait surfaces stub only. |
| `omni-runtime`, `omni-mesh`, `omni-tokenization` | 🟡 Stubs. |
| `omni-sdk`, `omni-agent`, `omni-shell` | 🟡 Stubs. |
| `omni-container` | 🟡 Skeleton (OIP-Container-006 Draft, P8.2+ unblocked but unstarted). |
| CI | ✅ `ci.yml` runs fmt + clippy (`-D warnings`, `--all-features`) + test (`--test-threads=1` due to pre-existing SIGSEGV) + doc + bare-metal-build + kernel-runner-build + blanket-allow-guard + TBD-guard. `qemu-boot-smoke.yml`, `audit.yml`, `sbom.yml`, `codeql.yml`, `dco.yml`, `reproducible-build.yml`, `oip-lint.yml` all green. |

Workspace test count today: **885 pass / 0 fail** with `cargo test --workspace --all-features
-- --test-threads=1`.

### 1.3 What is documented but not implemented

1. **omni-driver-pack** (omni-pack v1 producer / OMNI Forge build tool referenced in
   OIP-013 § S5.5 and per-driver OIPs) — **no crate exists yet**. Driver manifests under
   `crates/omni-driver-{net-virtio,nvme,e1000e}/manifest.toml` are inert today: nothing
   converts them into the `omni-pack v1` blob the kernel `DriverLoad (73)` handler ingests.
2. **IOMMU vendor backends** (`iommu::vtd` for Intel VT-d / DMAR, `iommu::amdvi` for
   AMD-Vi / IVRS) referenced in OIP-013 § S3 — **`DmaMap` is currently Phase-1 "no-IOMMU
   passthrough"** (`iova == user_va`, strict-contiguity frame allocation). Real IOMMU
   programming deferred.
3. **`omni-driver-shared`** SDK crate (`caps::find_token(action_tag, resource_predicate)`)
   referenced by `docs/plans/p6-7-8-9-cap-deposit-trampoline.md` § D3 — **does not exist**.
   Drivers cannot consume the deposit window without it.
4. **Filesystem service** that consumes `omni.svc.blk.<diskN>` (OIP-014 § S6 + roadmap
   Phase 1 "boot from NVMe") — **no crate**.
5. **TDX / SEV-SNP real backends** (`omni-tee::tdx`, `omni-tee::sev_snp`) — module skeletons
   referenced in CI `--all-features` matrix but no real attestation path.
6. **AI Runtime Service / Tokenization / Mesh Protocol Service** — Phase 2+ scope, all
   crates are stubs.
7. **`docs/protocol/handshake.md` § 3.2** still negotiates `OMNI-PROTO-v0.1` while
   `omni_types::version::PROTOCOL_VERSION_V0_2` is the canonical constant since
   P7.2.M5 — see `progress-omni.md` § 4.2 #8.
8. **OIP-Bounty-002 + OIP-Serde-004 Last Call closure (window closes 2026-05-26)** —
   editorial PR pending per `todo.md` "Still open" item 15.
9. **Branch protection update** for `oip-lint` required check is overdue (deadline
   2026-05-17 — see `todo.md` "Still open" item 9). Founder admin token required.
10. **Stichting OMNI signing key** in `KNOWN_ISSUERS` table — table is empty at v0.3.0;
    no driver pack can be admitted by `DriverLoad` today because every issuer pubkey
    fails the § S5.4 lookup with `EACCES`.
11. **DRIVER_CAP_ISSUER_SEED** in `crates/omni-kernel/src/driver_cap_issuer.rs` is a
    DEV-ONLY fixed 32-byte literal. OIP for production key custody (TEE-derived sealing
    key) — **not yet drafted**.

### 1.4 Open doc↔code gaps

| Doc says | Code says | Gap |
|---|---|---|
| `docs/protocol/handshake.md` § 3.2 negotiates `OMNI-PROTO-v0.1` | `omni_types::version::PROTOCOL_VERSION_V0_2` is canonical | TASK-006 (P7.3 docs update). |
| `OIP-Process-001` §9 ¶2 mandates branch-protection adds `oip-lint / oip-lint` within 7 calendar days of `Active` | branch protection still has 8 required status checks, not 9 | TASK-009 (founder admin action). |
| OIP-013 § S5.4 mandates static `KNOWN_ISSUERS` baked at compile time | `crates/omni-kernel/src/known_issuers.rs` has `static KNOWN_ISSUERS: &[KnownIssuer] = &[];` | TASK-013 (after TASK-012 produces a production key custody OIP). |
| OIP-013 § S5.5 mandates omni-pack v1 blobs | no producer crate exists | TASK-011 (`omni-driver-pack`). |
| OIP-013 § S3.3 mandates VT-d / AMD-Vi domain-per-driver IOMMU programming | `dma_map_handlers::dma_map` uses passthrough (`iova == user_va`, strict-contiguous frame alloc) | TASK-018 (P6.7.9-pre IOMMU backends). |
| Roadmap Phase 1 lists "Drivers (in user space): NVMe, Ethernet/Wi-Fi, TEE" as a deliverable | three driver scaffolds exist + bootable image siblings, but **none has been ingested by `DriverLoad`**, no driver has ever issued a real `MmioMap` call against a real device | TASK-002 / TASK-003 / TASK-004 (P6.7.9 live bring-up). |
| `docs/06-roadmap.md` Phase 1 says "First external security audit of kernel + capability system" | not engaged | TASK-017 (P6.8, funding-dep). |
| `progress-omni.md` § 4.5 #16 records pre-existing `omni-kernel --lib` SIGSEGV; CI mitigated via `--test-threads=1` | bug still present in `bare_metal::paging::tests::TestArena` | TASK-007 (P10.3 SIGSEGV fix). |

**STOP-ASK candidates checked — none found contradictory.** The OIP-013 amendment
sequence (Appendix B + § R7) is consistent with the syscall renumbering in `oips/README.md`
and the dispatcher in `crates/omni-kernel/src/bare_metal/syscall_entry.rs`. The
`OIP-Process-001 §5.5` founder fast-path for OIP-014/015/016 (2026-05-20) is documented
in each OIP's Appendix A and in the index — internally consistent. The roadmap's "≈ 99.9 %
closed" claim in `docs/06-roadmap.md` line 29 is consistent with what `progress-omni.md`
records.

---

## 2. Task inventory

> **Priority key.** P0 = security-critical or unblocks the Phase-1 closure critical path;
> P1 = significant Phase-1 deliverable; P2 = follow-up; P3 = funding-dependent or
> Phase-2+ scope.
>
> **Effort key.** S = < 2 h ; M = 2–8 h ; L = 1–3 d ; XL = > 3 d.

---

### TASK-001 — Branch-protection: add `oip-lint / oip-lint` as required status check

- **Priority:** P0 (overdue governance commitment; cheapest action on the board).
- **Phase alignment:** Roadmap Phase 0 (foundation; carryover from `OIP-Process-001` §9 ¶2 obligation 2026-05-17).
- **Crate ownership:** N/A — repo configuration only (founder admin token required).
- **Dependencies:** none.
- **Effort:** S (< 30 min on founder's side; planner cannot perform the action).
- **Acceptance criteria:**
  - `gh api repos/CySalazar/omni/branches/main/protection` shows 9 required status checks
    (current 8 + `oip-lint / oip-lint`).
  - One subsequent PR run shows `oip-lint` listed in the merge-check matrix.
  - `docs/audits/p0-completion-report.md` row "Branch protection" updated to reflect the
    new count (a single 1-line doc patch, no test).
  - **No code changes**, hence no test additions required for this task; tests already
    cover the OIP lint itself (`scripts/lint-oips.py` + `oip-lint.yml`).
- **Security considerations:** brings the OIP review pipeline into the merge gate, closing
  the editorial-bypass loophole that existed since 2026-05-17. No new attack surface.
- **Subagent assignment:** **lead-only** (founder admin token). Planner can prepare the
  one-liner `docs/audits/p0-completion-report.md` patch on request.
- **References:** `todo.md` "Still open" item 9 (line ~1505); `OIP-Process-001 §9`; `.github/workflows/oip-lint.yml`.

---

### TASK-002 — Close `OIP-Bounty-002` + `OIP-Serde-004` `Last Call → Active` (window 2026-05-26)

- **Priority:** P0 (deadline-bound editorial action; affects ≥ 2 already-prepared OIPs).
- **Phase alignment:** Phase 0 governance.
- **Crate ownership:** N/A (frontmatter-only PR in `oips/`).
- **Dependencies:** none.
- **Effort:** S (~ 1 h editorial review + 2 frontmatter edits + 1 README index update).
- **Acceptance criteria:**
  - `oips/oip-bounty-002.md` frontmatter: `status: Last Call → Active`, `updated: 2026-05-26`.
  - `oips/oip-serde-004.md` frontmatter: same transition.
  - `oips/README.md` index rows updated.
  - One editorial-report row appended to `oip-editors-report-2026-Q2.md` (new file under
    `docs/audits/` if it does not yet exist) recording the tally / absence-of-objection.
  - `python3 scripts/lint-oips.py` clean.
  - `.github/workflows/oip-lint.yml` job green on the PR.
  - **No code changes**; no unit tests added. The `oip-lint` workflow itself is the test.
- **Security considerations:** brings the postcard wire-format and the bug-bounty policy
  to `Active` (load-bearing for the `cargo deny` advisory clean-state and for the
  responsible-disclosure pipeline). Low-risk; reversible by a follow-up PR.
- **Subagent assignment:** **lead-only** (editorial decision); planner can produce the PR
  draft on request.
- **References:** `todo.md` "Still open" item 15; `OIP-Process-001 §5.3`; `oips/oip-bounty-002.md`; `oips/oip-serde-004.md`.

---

### TASK-003 — `omni-driver-shared` SDK crate (P6.7.8.10)

- **Priority:** P0 (unblocks every live-bring-up task TASK-004/005/006; closes the
  Phase-1 driver-side helper API).
- **Phase alignment:** Phase 1, `docs/06-roadmap.md` line 37 (driver-shared SDK helper).
- **Crate ownership:** **new crate `crates/omni-driver-shared`** (`no_std + alloc`,
  workspace member, no syscalls, dep-free).
- **Dependencies:** none (P6.7.8.9 cap-deposit trampoline already on `main`).
- **Effort:** M (4–6 h: small crate, plenty of pure-function host tests).
- **Acceptance criteria:**
  - Code in `crates/omni-driver-shared/src/lib.rs` exposing:
    - `pub const DRIVER_CAP_DEPOSIT_VA: u64 = 0x0010_0000;`
    - `pub const DRIVER_CAP_DEPOSIT_LEN: usize = 0x8000; // 32 KiB`
    - `pub mod caps { pub fn find_token(action_tag: u32, resource_predicate: impl Fn(&[u8]) -> bool) -> Option<&'static [u8]> }`
    - `unsafe fn header() -> &'static OmniCapsHeader` (validates magic + version).
    - `OmniCapsError::{InvalidMagic, UnsupportedVersion, EntryCountExceeded, OutOfBoundsOffset}`.
  - Unit tests in `crates/omni-driver-shared/src/lib.rs` (`#[cfg(test)]` host-side):
    - `header_parser_rejects_bad_magic`
    - `header_parser_rejects_unsupported_version`
    - `find_token_locates_action_mmio_map`
    - `find_token_returns_none_for_unknown_action`
    - `find_token_rejects_oob_offset`
    - `find_token_rejects_oob_len`
    - Property-based test (`proptest`) that `find_token` is a pure function of the page
      bytes (idempotent over re-invocation).
    - Coverage target: **≥ 90 % line coverage** on `lib.rs` (the crate is tiny).
  - Workspace `Cargo.toml` `[workspace.members]` extended (16 members).
  - `scripts/check-no-blanket-allow.sh` `SCOPED_CRATES` extended (15 → 16).
  - **E2E test** in `crates/omni-driver-shared/tests/e2e_deposit_round_trip.rs`: stages
    a synthetic `OMNICAPS` page in user-allocated memory, runs `find_token` against it,
    asserts the returned slice round-trips through
    `omni_types::wire::decode_canonical::<CapabilityToken>` — exercises the wire-format
    contract end-to-end. Mark with `#[cfg(not(target_os = "none"))]` so bare-metal builds
    skip it.
  - Driver crates (`omni-driver-net-virtio`, `omni-driver-nvme`, `omni-driver-e1000e`)
    receive a follow-up patch (separate sub-task in the same PR) that pulls
    `omni-driver-shared = { path = "../omni-driver-shared" }` and replaces every
    hard-coded `0x0010_0000` constant with the shared symbol.
  - Documentation update in `crates/omni-driver-shared/README.md` (new) cross-referencing
    OIP-013 § S5.3 step 8 and `docs/plans/p6-7-8-9-cap-deposit-trampoline.md` § D3.
  - **Gates:**
    - `cargo fmt --all -- --check`
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings`
    - All three driver-image clippy invocations remain green.
    - `cargo test --workspace --all-features -- --test-threads=1` (target ≥ 893 pass; +8 new).
    - `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` green.
    - `cargo deny check` green.
  - Build Info panel (`crates/omni-kernel/src/bare_metal/demo.rs::render_buildinfo`):
    Active=`P6.7.8.10 driver-shared SDK`, Next=`P6.7.9 virtio-net live bring-up`.
- **Security considerations:** new code runs in **Ring 3 driver processes**, **never in
  the kernel**. `unsafe` is local to a single function reading the 8-page deposit window
  through a `&'static [u8]` constructed from the well-known VA + length. No new attack
  surface for the kernel; mitigations enforced kernel-side (RO mapping, NX bit, no
  WRITABLE flag). New crate adds no transitive dependencies.
- **Subagent assignment:** **`impl-driver-shared`** (new instance) + **`test-engineer`**
  for the proptest harness + e2e file. **`code-reviewer`** required at PR time.
- **References:** `docs/plans/p6-7-8-9-cap-deposit-trampoline.md` § D3, "Open follow-up";
  OIP-013 § S5.3 step 8; `crates/omni-kernel/src/cap_deposit.rs`.

---

### TASK-004 — virtio-net live bring-up + Proxmox VMID 103 smoke (P6.7.9.a)

- **Priority:** P0 (first live driver — exercises the entire `DriverLoad` →
  `MmioMap`/`DmaMap`/`IrqAttach` chain end-to-end on real silicon).
- **Phase alignment:** Phase 1 closure deliverable per `docs/06-roadmap.md` line 37.
- **Crate ownership:** `crates/omni-driver-net-virtio` (FSM wiring) +
  `crates/omni-driver-net-virtio-image` (Ring 3 ELF `_start`) +
  `crates/omni-kernel/src/bare_metal/demo.rs` (boot-time `DriverLoad` invocation in a new
  cargo feature `p6-7-9-virtio-smoke`) + `crates/omni-driver-shared` (TASK-003 helper).
- **Dependencies:** TASK-003 (`omni-driver-shared`), TASK-011 (omni-driver-pack tool),
  TASK-013 (Stichting OMNI signing key enrolled in `KNOWN_ISSUERS`).
- **Effort:** XL (3–5 d: includes IOMMU passthrough validation, MSI-X allocation,
  virtqueue programming, Proxmox VNC capture).
- **Acceptance criteria:**
  - Code: `crates/omni-driver-net-virtio-image/src/main.rs` `_start` issues the 7-state
    FSM with real `syscall5` invocations against `MmioMap (70)`, `DmaMap (71)`,
    `IrqAttach (72)`. The `syscall5` helper loses its `#[allow(dead_code)]` gate.
  - Code: `crates/omni-kernel/src/bare_metal/demo.rs` gains a feature
    `p6-7-9-virtio-smoke` that (a) emits a synthetic `omni-pack v1` blob from a
    `static` byte array embedded via `include_bytes!("../../target/x86_64-unknown-none/release/omni-driver-net-virtio.opack")`,
    (b) calls `driver_load_handlers::driver_load` directly with that blob, (c) logs the
    FSM transitions via `console::log_kernel`.
  - Unit tests in `crates/omni-driver-net-virtio/src/bringup.rs` for the new transition
    table: + 6 host-side tests (one per phase boundary).
  - **E2E test in `tests/e2e/p6_7_9_virtio_smoke.rs`** (new `tests/e2e/` directory at
    repo root, std-only, gated `#[cfg(target_os = "linux")]`): spawns QEMU with `-device
    virtio-net-pci` + `-serial stdio`, scrapes the serial log for the literal
    `[driver-virtio-net] DriverOk mac=` followed by 17 hex chars, asserts exit code 0.
  - **Hardware smoke documented** in `docs/audits/p6-7-9-virtio-smoke-report.md` (new):
    Proxmox VMID 103, serial transcript, VNC screenshot of Build Info panel showing
    `Active=P6.7.9 virtio-net live` cyan, ping test from a guest VM on the same vmbr0.
  - Build Info panel update: Active=`P6.7.9.a virtio-net live`, Next=`P6.7.9.b NVMe live`.
  - **Gates:** full cargo fmt / clippy / test / doc / deny matrix green (per CI workflow);
    QEMU smoke job in `qemu-boot-smoke.yml` extended with a feature-gated branch that
    exercises `p6-7-9-virtio-smoke` and asserts the new `EXPECTED_LINES`.
- **Security considerations:** **first real adversarial surface for the driver framework.**
  Threats introduced:
  - **C-3 (compromised driver) becomes empirically testable.** Mitigations: `MmioMap`
    bounds-check vs `Resource::MmioRegion`; `DmaMap` strict-contiguous frame alloc +
    domain-per-driver invariant; `IrqAttach` shared-line rejection + vector allocation
    in `[0x40, 0xFE]` only.
  - **Capability replay across cap-deposit page snapshots** — mitigated by 90-day
    `not_after` cap on minted tokens + per-process subject binding via `NodeId`.
  - **Side-channel via IRQ coalescing counter** — coalesce counter is per-channel
    atomic, no cross-channel inference path; documented threat in OIP-013 § R3.
  - **DMA escape** — Phase 1 passthrough `iova == user_va` means an attacker who pwns
    the driver could DMA-write any frame the bare-metal allocator hands it; **explicit
    pre-IOMMU caveat** documented in OIP-013 § S3.3 Appendix B amendment 1. TASK-018
    must land before any production deployment of a driver against an Internet-facing NIC.
- **Subagent assignment:** **`impl-driver-net-virtio`** + **`test-engineer`** (QEMU
  harness + Proxmox capture). **`code-reviewer`** mandatory.
- **References:** OIP-Driver-Net-015 § S4; OIP-013 § S2/S3/S4/S5; `progress-omni.md`
  § 5 (Proxmox VMID 103 procedure).

---

### TASK-005 — NVMe live bring-up + boot-from-NVMe smoke (P6.7.9.b)

- **Priority:** P0 (second live driver; unblocks any persistent-storage roadmap; required
  by Phase 2 AI Runtime which loads model weights from disk).
- **Phase alignment:** Phase 1 closure deliverable.
- **Crate ownership:** `crates/omni-driver-nvme` + `crates/omni-driver-nvme-image` +
  `crates/omni-kernel` (BLK channel scaffolding under
  `crates/omni-kernel/src/services/blk.rs` — new).
- **Dependencies:** TASK-003, TASK-011, TASK-013, TASK-004 (proves the
  `MmioMap`/`DmaMap`/`IrqAttach` chain on a simpler device first).
- **Effort:** XL (5–8 d: NVMe Identify, IO Queue Create, MSI-X N-vector allocation, PRP
  list materialisation, BLK channel ABI).
- **Acceptance criteria:**
  - Code: `crates/omni-driver-nvme-image/src/main.rs` runs the 13-step FSM with real
    syscall invocations through TASK-003 helpers.
  - Code: `crates/omni-kernel/src/services/blk.rs` (new module, `#[cfg(feature = "bare-metal")]`)
    declares the kernel-side BLK channel registry table; OIP-014 § S6 shape.
  - Unit tests in `omni-driver-nvme/src/bringup.rs`: +9 new host tests covering every
    new transition + `BlkChannelRegistrationFailed` error path.
  - **E2E test** in `tests/e2e/p6_7_9_nvme_smoke.rs`: spawns QEMU with `-device nvme,drive=nvme0,serial=cafe`,
    scrapes the serial log for `[driver-nvme] DriverOk capacity=`, asserts exit code 0
    after a single 512 B read at LBA 0.
  - **Hardware smoke documented** in `docs/audits/p6-7-9-nvme-smoke-report.md`:
    Proxmox VMID 103 attached NVMe (`-drive file=…,if=nvme`), serial transcript, VNC
    screenshot.
  - Build Info update: Active=`P6.7.9.b NVMe live`, Next=`P6.7.9.c e1000e live`.
  - **Gates:** full CI matrix green; `qemu-boot-smoke.yml` extended.
- **Security considerations:**
  - DMA windows for the NVMe data path are larger than virtio-net (whole 4 GiB IOVA arena
    per manifest default). Phase-1 passthrough means a compromised driver can
    overwrite/read any frame the bare-metal allocator handed it — same caveat as TASK-004
    but with much greater blast radius. **TASK-018 (VT-d/AMD-Vi) is now a hard
    prerequisite for production deployment.**
  - `FormatNVM` is capability-gated (separate bit, manifest must explicitly opt in via
    `format_nvm_enabled = true`). Default manifest disables it — mitigates accidental
    disk wipe by compromised driver.
  - Side-channel: the IO completion queue tail doorbell pattern leaks command timing;
    deferred to a follow-up OIP (Phase 2+).
- **Subagent assignment:** **`impl-driver-nvme`** + **`test-engineer`**. **`code-reviewer`** mandatory.
- **References:** OIP-Driver-NVMe-014 § S1–S6; `crates/omni-driver-nvme/src/bringup.rs`.

---

### TASK-006 — e1000e live bring-up smoke (P6.7.9.c)

- **Priority:** P1 (third live driver; first bare-metal-only — exercises code paths that
  virtio-net masks).
- **Phase alignment:** Phase 1 closure deliverable.
- **Crate ownership:** `crates/omni-driver-e1000e` + `crates/omni-driver-e1000e-image`.
- **Dependencies:** TASK-003, TASK-004 (proves the framework on virtio-net before
  exercising on real silicon-class device).
- **Effort:** L (2–3 d: the FSM is already 13 steps, mostly register pokes; MSI-X is
  the only new surface vs virtio-net + NVMe).
- **Acceptance criteria:**
  - Code: `crates/omni-driver-e1000e-image/src/main.rs` runs the 13-step FSM with real
    syscalls.
  - Unit tests: +6 in `bringup.rs` for transitions.
  - **E2E test in QEMU** with `-device e1000e` (QEMU supports e1000e since 2.7);
    `tests/e2e/p6_7_9_e1000e_smoke.rs`. Asserts MAC reading + 1-packet TX/RX round-trip.
  - Phase-1 hardware validation **deferred** — Proxmox VMID 103 does not have a passthrough
    e1000e; documented in `docs/audits/p6-7-9-e1000e-smoke-report.md` as "QEMU-only,
    bare-metal validation requires hardware (P5.2 funding)".
  - Build Info update: Active=`P6.7.9.c e1000e live`, Next=`Phase 2 entry OIP`.
- **Security considerations:** identical to TASK-004 (same DMA passthrough caveat).
  Distinct from virtio-net: e1000e exposes a full 128 KiB CSR window; `MmioRegion` cap
  must subset-bound the BAR exactly, no over-mapping.
- **Subagent assignment:** **`impl-driver-e1000e`** + **`test-engineer`**.
- **References:** OIP-Driver-Net-015 § S5; Intel 82574L datasheet § 5/§ 10.

---

### TASK-007 — `omni-driver-pack` user-space tool (omni-pack v1 producer)

- **Priority:** P0 (every TASK-004/005/006 needs an omni-pack blob to ingest; today nothing
  produces one).
- **Phase alignment:** Phase 1 (tooling for the driver framework deliverable).
- **Crate ownership:** **new crate `tools/omni-driver-pack`** (binary, std-enabled, lives
  under a new `tools/` directory at repo root to keep it outside `crates/` which is
  reserved for runtime members per OIP-Kernel-005 convention; **workspace-excluded**
  like `kernel-runner` to keep the bare-metal cross-build matrix clean).
- **Dependencies:** none (depends only on already-pinned workspace deps).
- **Effort:** M (4–8 h).
- **Acceptance criteria:**
  - Code in `tools/omni-driver-pack/src/main.rs` implementing CLI:
    ```
    omni-driver-pack \
      --manifest <path/to/manifest.toml> \
      --image <path/to/ring3.elf> \
      --signing-key <path/to/ed25519.seed> \
      --output <out.opack>
    ```
  - Steps:
    1. Parse TOML via `toml = "0.8"` (already vetted under Apache-2.0-compatible MIT,
       transitively pinned by `cargo-deny`; **needs verification** — see Security below).
    2. Compute BLAKE3 of the ELF via `blake3 = "1.5"` (workspace-pinned).
    3. Compose `DriverManifestV1` Rust struct.
    4. Encode via `omni_types::wire::encode_canonical::<DriverManifestV1>` (re-exported
       from the foundational crate; consumes postcard 1.0 from the workspace pin).
    5. Ed25519-sign via `ed25519-dalek` (workspace-pinned 2.1).
    6. Lay out the omni-pack v1 binary per OIP-013 § S5.5.
    7. Write to `<out.opack>`.
  - **Unit tests** in `tools/omni-driver-pack/tests/header_layout.rs`: verify byte-for-byte
    that the produced blob matches OIP-013 § S5.5 layout for a 3-MMIO + 2-IRQ manifest.
  - **E2E test** in `tools/omni-driver-pack/tests/e2e_round_trip.rs`: pack a known
    Ring 3 ELF, then decode the same blob with
    `omni_kernel::driver_manifest::decode_omni_pack` + `verify_manifest`, assert
    `Ok(_)`.
  - Documentation: `tools/omni-driver-pack/README.md` (new) with CLI usage + the same
    invocation embedded in `docs/protocol/driver-manifest-v1.toml` reference.
  - **Build Info / Tests counters** unaffected (workspace-excluded crate); CI extension
    in `ci.yml` adds a new job `omni-driver-pack-build` running `cargo build
    --manifest-path tools/omni-driver-pack/Cargo.toml`, `cargo test --manifest-path
    tools/omni-driver-pack/Cargo.toml`, and `cargo clippy --manifest-path
    tools/omni-driver-pack/Cargo.toml -- -D warnings`.
  - `cargo deny check` green over the new tool's lockfile slice.
- **Security considerations:**
  - **New dependency:** `toml = "0.8"` (latest stable line) — **NOT in `docs/09-tech-specifications.md`**.
    Compatibility check:
    - License: **MIT OR Apache-2.0** (per crates.io 2026-05 metadata) — **compatible**
      with `deny.toml` `allow` list.
    - Version: `0.8` is the current major; `0.9` exists but `clap`/`cargo` ecosystem still
      pins to `0.8`. **Proposed pin: `toml = "0.8.19"`.**
    - **Action:** add a row in `docs/09-tech-specifications.md` § "Serialization" before
      the implementer starts. **The lead MUST approve the dep addition** before the
      implementer writes code. If the lead prefers, a hand-rolled TOML subset parser is
      feasible (the manifest schema is < 30 lines, all scalar / vector); planner
      recommends `toml = "0.8.19"` for soundness — the manifest schema may grow.
  - **Existing dependencies reused:** `blake3 = "1.5"`, `ed25519-dalek = "2.1"`,
    `postcard = "1.0"`, `serde = "1.0"` — all already pinned in workspace `Cargo.toml`
    per `docs/09-tech-specifications.md` § Cryptography / Serialization. **No version
    drift.**
  - Signing key handling: tool reads from local file (mode 0400 enforced via `stat()`
    pre-check). Production key custody is **TASK-008**.
  - Tool runs entirely user-space; no privileged path. No kernel-side risk.
- **Subagent assignment:** **`impl-driver-pack`** (new instance, std-enabled tooling
  comfort zone) + **`test-engineer`**. **`code-reviewer`** mandatory.
- **References:** OIP-013 § S5.5; `crates/omni-driver-net-virtio/manifest.toml` as the
  first input.

---

### TASK-008 — OIP draft: production key custody + `DRIVER_CAP_ISSUER_SEED` replacement

- **Priority:** P0 (security — the DEV-ONLY static seed currently signs every cap-deposit
  token, so anyone who reads `crates/omni-kernel/src/driver_cap_issuer.rs` can forge a
  token that passes `verify_signed_token`; this is documented in the source as a known
  issue but must be closed before any non-developer driver lands).
- **Phase alignment:** Phase 1 closure (security hardening).
- **Crate ownership:** N/A — OIP drafting only at this stage. Implementation OIP
  follow-up assigned a separate TASK number after ratification.
- **Dependencies:** none.
- **Effort:** L (1–2 d for the OIP draft + sample API sketch).
- **Acceptance criteria:**
  - New OIP file `oips/oip-key-custody-017.md` (next free integer) in **Standards Track**
    status `Draft`, fields per `oip-template.md`.
  - Must specify:
    - Source of the kernel-issuer signing key (TDX-sealed under measurement, SEV-SNP
      Guest VEK derivation, fallback for dev/test).
    - Bootstrap procedure (how the kernel obtains the key on first boot vs. subsequent
      boots).
    - Rotation policy (90-day max key lifetime; OIP-amend to extend).
    - Custody during the time `omni-tee` real backends do not exist (Phase 1 caveat).
    - Migration plan from DEV-ONLY seed to TEE-derived key.
  - Reference Implementation N/A at filing.
  - **`python3 scripts/lint-oips.py`** green.
  - **`oip-lint.yml` workflow** green on the PR.
  - **Index `oips/README.md`** updated.
  - No code or test changes in this task. Tests already cover the OIP lint pipeline.
  - The lead must explicitly schedule the implementation OIP-follow-up TASK (separate)
    after `Draft → Review → Last Call → Active` transition.
- **Security considerations:** **the whole point of this task is to close a stated
  security gap.** No new attack surface introduced; only specifies a path to close one.
- **Subagent assignment:** **planner-v2** (this agent) can draft if the lead asks;
  otherwise **lead-only**.
- **References:** `crates/omni-kernel/src/driver_cap_issuer.rs` (current DEV-ONLY seed);
  `docs/plans/p6-7-8-9-cap-deposit-trampoline.md` § D2 (rationale + deferral note); OIP-013 § S5.4.

---

### TASK-009 — Enroll Stichting OMNI signing key in `KNOWN_ISSUERS` (post-TASK-008)

- **Priority:** P0 (blocks every `DriverLoad` since `KNOWN_ISSUERS` is empty today —
  `verify_manifest` returns `UnknownIssuer → EACCES` for every driver pack).
- **Phase alignment:** Phase 1 closure.
- **Crate ownership:** `crates/omni-kernel/src/known_issuers.rs`.
- **Dependencies:** TASK-008 (`Active` status not strictly required to populate a single
  bootstrap key; the lead may approve a fast-path enrolment under
  `OIP-Process-001 §5.5`).
- **Effort:** S (< 2 h).
- **Acceptance criteria:**
  - Source: `crates/omni-kernel/src/known_issuers.rs` `static KNOWN_ISSUERS: &[KnownIssuer]`
    is no longer empty: one entry for the Stichting OMNI signing key (or, during the
    pre-Stichting bootstrap, the founder's interim signing key documented in
    `docs/audits/founder-issuer-pubkey-2026-05.md` — **new file**).
  - The public key bytes (32 byte Ed25519) MUST be derived from a key generated on the
    founder's signing hardware (YubiKey 5C NFC preferred) and committed to repo in
    hex form alongside a checksum.
  - Unit tests in `known_issuers.rs`:
    - `bootstrap_key_present` (replaces `phase1_table_is_empty`).
    - `lookup_bootstrap_key_returns_some`.
    - `lookup_unknown_key_returns_none` (retained).
  - **Smoke**: the `tools/omni-driver-pack` invocation in TASK-007 with the bootstrap
    issuer's signing key produces a blob that `verify_manifest` accepts (already covered
    by TASK-007 e2e_round_trip, but here we update it to use the real key).
  - `docs/audits/founder-issuer-pubkey-2026-05.md` documents the key's provenance,
    storage location, rotation date (2026-08-19 = 90 d).
- **Security considerations:**
  - **Trust on First Use is explicitly forbidden** (OIP-013 § S5.4); this task is the
    first non-TOFU enrolment.
  - Founder key must be backed up offline (Shamir 2-of-3 across paper + USB + safe).
  - The 90-day rotation is enforced by the cap-deposit `not_after` field at minting
    time, not by the issuer table itself.
- **Subagent assignment:** **lead-only** (key generation must happen on founder's
  hardware) + **`impl-kernel-known-issuers`** for the code patch.
- **References:** OIP-013 § S5.4; TASK-008.

---

### TASK-010 — VT-d / AMD-Vi IOMMU backends (P6.7.9-pre)

- **Priority:** P0 (current `DmaMap` passthrough is a stated security caveat in OIP-013
  § S3.3 Appendix B amendment 1; production deployment requires IOMMU; **TASK-005 in
  particular relies on this for any realistic threat model**).
- **Phase alignment:** Phase 1 closure (driver framework deliverable, OIP-013 § S3).
- **Crate ownership:** `crates/omni-kernel/src/bare_metal/iommu/` (new module tree):
  `iommu/mod.rs`, `iommu/vtd.rs` (Intel VT-d / DMAR), `iommu/amdvi.rs` (AMD-Vi / IVRS).
- **Dependencies:** none on the kernel side (ACPI tables already discovered by the
  bootloader; DMAR/IVRS parsing is new).
- **Effort:** XL (5–10 d: ACPI table parsing + per-vendor register programming + per-
  device domain allocation + integration into `dma_map_handlers::dma_map`).
- **Acceptance criteria:**
  - Code: new `iommu` module with `pub trait IommuBackend { fn install_domain(...);
    fn map(iova, phys, len, flags); fn unmap(iova, len); fn flush(domain); }` + two
    concrete implementations.
  - `dma_map_handlers::dma_map` swapped from passthrough to `iommu::current().map(...)`.
  - Boot-time probe in `kmain` (post `set_phys_offset`): scans ACPI DMAR + IVRS, picks
    the first available backend, falls back to `IommuBackend::Passthrough` (existing
    behaviour) only under a feature `iommu-passthrough-dev-only` that emits a loud kernel
    log line.
  - **Unit tests** (`#[cfg(test)]` host side):
    - DMAR table parser (round-trip from a canonical Intel sample table — at least 4
      unit tests for malformed, truncated, and well-formed inputs).
    - IVRS table parser (same shape, 4 tests).
    - Domain allocator: round-trip + collision rejection + free + re-allocate (≥ 6 tests).
  - **Bare-metal smoke** in `kmain`: post-IOMMU init, emits a single line
    `[iommu] vendor=<intel|amd> domains=<N>` validated by `qemu-boot-smoke.yml`
    `EXPECTED_LINES` extension.
  - **E2E test** in `tests/e2e/p6_7_9_pre_iommu_smoke.rs`: QEMU `-machine q35,iommu=intel`
    (or `iommu=amd`) drives `EnumeratePci → MmioMap → DmaMap`. Asserts that a
    cross-domain DMA write attempt returns `EACCES` from the kernel.
  - **Hardware smoke** documented in `docs/audits/p6-7-9-pre-iommu-vtd.md` and
    `docs/audits/p6-7-9-pre-iommu-amdvi.md` on Proxmox VMID 103.
  - Build Info update: Active=`P6.7.9-pre IOMMU live`, Next=`P6.7.9.a virtio-net live`.
  - Gates: full CI matrix green; `qemu-boot-smoke.yml` extended.
- **Security considerations:**
  - **Closes the DMA-escape gap** that was the most consequential caveat in the live
    bring-up tasks.
  - **Introduces a new TCB surface** (the IOMMU drivers themselves). Mitigations: keep
    the implementation in-kernel (cannot move it to user-space without the IOMMU
    bootstrapping itself); minimise per-vendor LOC (target < 500 SLOC per backend); two
    independent code paths reduce the blast radius of any single-vendor flaw.
  - **No new dependency.** ACPI table parsing is hand-rolled (existing ACPI MADT walker
    in `crates/omni-kernel/src/bare_metal/mp.rs` is the reference; `acpi` crate v5.0 was
    considered and rejected because (a) it pulls `aml` for AML interpreter we don't
    need, (b) license is MIT OR Apache-2.0 — fine — but the dep weight is unjustified
    for just two tables).
- **Subagent assignment:** **`impl-kernel-iommu`** (new instance, deepest kernel work in
  the plan) + **`test-engineer`** + **`code-reviewer`** mandatory.
- **References:** OIP-013 § S3; Intel VT-d spec rev 4.1 § 8 (DMAR ACPI); AMD I/O
  Virtualization Technology spec rev 3.10 § 5 (IVRS).

---

### TASK-011 — `omni.svc.fs.<volN>` filesystem service skeleton (post-NVMe)

- **Priority:** P1 (immediate consumer of TASK-005's BLK channel; no roadmap deliverable
  in Phase 1 named it directly, but Phase 2 AI Runtime depends on persistent storage
  for model weights).
- **Phase alignment:** Phase 1 → Phase 2 bridge.
- **Crate ownership:** **new crate `crates/omni-fs`** (`no_std + alloc`, workspace member).
- **Dependencies:** TASK-005 (BLK channel exists).
- **Effort:** L (2–3 d) for the skeleton (no actual filesystem yet — just the IPC
  channel registration + `BlkRequest` enum consumer).
- **Acceptance criteria:**
  - Code: `crates/omni-fs/src/lib.rs` with `pub struct FsService { blk_channel: ChannelId }`
    + `register(disk_n)` + `handle_request(req)` returning a stub `FsResponse::NotImplemented`.
  - Unit tests: 5 covering registration, double-registration rejection, request routing.
  - E2E test deferred (no real filesystem until a Phase 2 OIP picks ZFS port vs. native
    OMNI FS — open question in `docs/02-architecture.md` line 251).
  - Build Info update: Active=`P6.7.9.d FS skeleton`, Next=`Phase 2 entry OIP`.
  - Gates: full CI matrix green.
- **Security considerations:** stub crate; no syscall surface yet. Real filesystem
  introduces capability questions tracked in a future OIP.
- **Subagent assignment:** **`impl-fs`** (new instance, light load — skeleton-only).
- **References:** OIP-014 § S6 BLK channel ABI; `docs/02-architecture.md` line 251.

---

### TASK-012 — Resolve pre-existing `omni-kernel --lib` SIGSEGV (P10.3)

- **Priority:** P1 (carryover blocker; CI mitigates via `--test-threads=1` but parallel
  test perf and local developer iteration are degraded; affects coverage tooling that
  cannot easily parallelise).
- **Phase alignment:** Phase 1 follow-up.
- **Crate ownership:** `crates/omni-kernel/src/bare_metal/paging.rs` (`TestArena` struct).
- **Dependencies:** none.
- **Effort:** L (1–3 d depending on whether `&'static mut [MaybeUninit<u8>]` swap works
  or full `Arc<Mutex<...>>` refactor is needed).
- **Acceptance criteria:**
  - `TestArena` rewritten to be **race-free under parallel `cargo test`**.
    Recommended approach (in order of preference):
    1. Move the arena to a `OnceLock<&'static mut [MaybeUninit<u8>; ARENA_BYTES]>`
       initialised on first access; tests that need a fresh arena use a per-test
       `Mutex<()>` to serialise.
    2. Per-test `Box<TestArena>` with an explicit lifetime tying any `*mut RawPageTable`
       to the box (proves at compile time that pointers cannot outlive the arena).
  - **Unit test additions:** 4 new tests under `#[cfg(test)]` that exercise concurrent
    arena access via `std::thread::scope` (each thread mutates a disjoint slice; the
    test asserts no other thread's slice is touched).
  - **CI workflow** `ci.yml` `cargo test` step **drops `--test-threads=1`** and runs
    `cargo test --workspace --all-features` with default parallelism.
  - Local `cargo test -p omni-kernel --lib` returns `signal: 0` (exit clean).
  - Documentation: `progress-omni.md` § 4.5 #16 marked CLOSED with the commit hash.
  - Gates: full CI matrix green, test time **must not regress more than 50 %** vs the
    `--test-threads=1` baseline (currently ~ 45 s for 885 tests).
- **Security considerations:** removing `--test-threads=1` mitigation surfaces any
  race-condition bug that the serialisation hid. The 4 new concurrency tests are the
  safety net.
- **Subagent assignment:** **`impl-kernel-paging`** (kernel-MM expertise) + **`test-engineer`**.
- **References:** `progress-omni.md` § 4.5 #16; `.github/workflows/ci.yml` lines 82–94
  (comment block).

---

### TASK-013 — CI smoke automation for `mb11-userprobe` + `mb12-userprobe` (P10.4)

- **Priority:** P2 (regression catch for kernel changes; the smoke was deferred when
  MB13.b unblocked the triple-fault but the CI extension was never landed).
- **Phase alignment:** Phase 1 closure cleanup.
- **Crate ownership:** `scripts/qemu-boot-smoke.sh` + `.github/workflows/qemu-boot-smoke.yml`.
- **Dependencies:** TASK-012 (CI must be clean before adding more matrix entries).
- **Effort:** M (4–6 h).
- **Acceptance criteria:**
  - `scripts/qemu-boot-smoke.sh` accepts `--feature mb11-userprobe` and
    `--feature mb12-userprobe`; builds `kernel-runner` with the feature; asserts
    `EXPECTED_LINES` extended to include `[user] hello`, `[user] exit=0`, and for MB12
    also `[mb12] channel 1 pre-created`, `ping`, two consecutive `[user] exit=0`.
  - `qemu-boot-smoke.yml` adds two new jobs `qemu-smoke-mb11`, `qemu-smoke-mb12`
    each pulling the appropriate feature.
  - Branch protection update (founder admin token) adds the two new jobs to the
    required-checks list — **TASK-001-equivalent admin action**.
  - Unit tests: none new. The QEMU jobs ARE the test.
  - Documentation: `docs/audits/qemu-boot-smoke-template.md` updated with the new
    EXPECTED_LINES contract.
- **Security considerations:** no new attack surface; reduces regression risk for
  kernel changes.
- **Subagent assignment:** **`impl-ci`** + lead admin action.
- **References:** `todo.md` § P10.4; `progress-omni.md` § 4.5 #19, #20.

---

### TASK-014 — Update `todo.md` to reflect this plan (LEAD-EXECUTED)

- **Priority:** P0 (governance hygiene; planner is explicitly forbidden from touching
  `todo.md` per the hard rules in the agent spec).
- **Phase alignment:** N/A.
- **Crate ownership:** N/A (`todo.md` only).
- **Dependencies:** lead approval of this plan.
- **Effort:** S (~ 1 h for the lead to apply).
- **Acceptance criteria (proposed patches to `todo.md`):**
  - In the `> **Status:**` opening block: add a new sentence "Active development plan:
    `docs/planning/2026-05-21-development-plan.md`."
  - Add a new sub-task under § P6.7 / `P6.7.8.10`:
    > `- [ ] **P6.7.8.10 — `omni-driver-shared` SDK crate** — see TASK-003 in
    > `docs/planning/2026-05-21-development-plan.md`.`
  - Add a new sub-task block § P6.7.9 listing `P6.7.9.a/b/c/d` mapping to TASK-004/005/006/011.
  - In § "Still open" item 9: append "Tracked in TASK-001 of 2026-05-21 plan."
  - In § "Still open" item 15: append "Tracked in TASK-002 of 2026-05-21 plan."
  - In § "Phase 1 closure roadmap — executive sequence (post v0.2.0)" table: add a
    cross-reference column to the 2026-05-21 plan TASK-NNN identifiers.
- **Security considerations:** none — doc-only edit.
- **Subagent assignment:** **lead-only**. Planner remains forbidden from editing `todo.md`.
- **References:** the agent spec hard rule "Do not modify any file other than the plan
  you produce. Specifically, do not touch `todo.md`."

---

### TASK-015 — `OMNI-PROTO-v0.2` documentation update (P7.3)

- **Priority:** P2 (low-effort, parallelisable; closes the longest-standing doc↔code
  gap; required for OIP-Serde-004 to formally walk towards `Final`).
- **Phase alignment:** Phase 1 cleanup; OIP-Serde-004 closure.
- **Crate ownership:** `docs/protocol/handshake.md` only; no code.
- **Dependencies:** TASK-002 (OIP-Serde-004 promoted to `Active`).
- **Effort:** S (1–2 h).
- **Acceptance criteria:**
  - `docs/protocol/handshake.md` § 3.2 updated: handshake negotiates
    `OMNI-PROTO-v0.2` (with `OMNI-PROTO-v0.1` listed as legacy + deprecation date).
  - `serde_format = "postcard-1.0"` discriminant documented per OIP-Serde-004 § S2.
  - `docs/03-mesh-protocol.md` cross-reference updated.
  - `docs/changelog.md` row added under "2026-05-21".
  - No code changes, hence no test additions; OIP-Serde-004 already has its round-trip
    regression tests in P7.2.M3/M4/M5.
- **Security considerations:** none introduced; removing the v0.1 legacy negotiation
  closes an attack vector for protocol downgrade (defence-in-depth).
- **Subagent assignment:** **`impl-docs`** (light load).
- **References:** `todo.md` § P7.3; `progress-omni.md` § 4.2 #8.

---

### TASK-022 — Align `omni-mesh` wire encoding from `bincode 2.0` to `postcard 1.0` per OIP-Serde-004

- **Priority:** P2 (no immediate security or stability impact; internally observable
  correctness gap between `omni-mesh` code and the canonical workspace wire format
  ratified by OIP-Serde-004; closure path for the residual `docs/03-mesh-protocol.md:197`
  reference to `bincode 2.0`).
- **Phase alignment:** Phase 2 — Mesh & Networking section of `docs/06-roadmap.md`
  (`omni-mesh` bring-up scope). Specifically the "Mesh Protocol Service" deliverable
  that today sits as a stub crate awaiting Phase 2.
- **Crate ownership:** `crates/omni-mesh` (encoding swap + tests). **Also touches the
  `omni_types::wire` surface** — `wire::encode_canonical` / `wire::decode_canonical`
  already exist and are workspace-canonical since P7.2.M2 (commit `9b3d977`, verified
  in `crates/omni-types/src/wire.rs` and enforced by `clippy.toml` `disallowed-methods`
  on raw `postcard::*` calls outside that helper). No wire-helper addition required;
  `omni-mesh` consumes the existing API. **If during implementation any
  mesh-specific helper is found to be missing from `omni_types::wire`, the task expands
  to also add that helper in `omni-types` — see Effort estimate below.**
- **Dependencies:** TASK-002 (OIP-Serde-004 promoted `Last Call → Active`).
- **Effort:**
  - **M (2–8 h)** for the encoding swap + tests, assuming `omni_types::wire::{encode_canonical, decode_canonical}` cover every required shape (high-confidence path).
  - **L (1–3 d)** if any helper is missing from `omni_types::wire` and must be added in
    this task (low-confidence fallback path).
- **Acceptance criteria:**
  - **Code:** every `bincode::{serialize, deserialize, serialize_into, deserialize_from}`
    call site in `crates/omni-mesh/` swapped to
    `omni_types::wire::{encode_canonical, decode_canonical}`. Any `bincode` dependency
    in `crates/omni-mesh/Cargo.toml` removed in the same PR.
  - **Doc patch in the SAME PR:** `docs/03-mesh-protocol.md:197` updated from
    `bincode 2.0` to `postcard-1.0`. Verify by `git grep -n 'bincode' crates/omni-mesh/ docs/03-mesh-protocol.md`
    returning empty.
  - **Marker hygiene:** any `// TODO(TASK-022): ...` placeholder markers in
    `crates/omni-mesh/` (placed by `impl-docs` as part of TASK-015 follow-through, or
    by any earlier preparatory pass) are removed in this PR. Verify by
    `git grep -n 'TODO(TASK-022)' crates/omni-mesh/` returning empty.
  - **Unit tests** added in `crates/omni-mesh/tests/wire_round_trip.rs` (new) covering,
    for each mesh message variant currently defined in `omni-mesh`:
    - Typical message payload — round-trip `encode_canonical → decode_canonical` must
      preserve byte-for-byte equality of the decoded struct.
    - Empty payload (where applicable) — round-trip must succeed and decode to a
      semantically empty value (not `Err`).
    - Maximum-size payload (per OIP-013 § S5.5 `OMNI_PACK_MAX_BYTES = 32 MiB` ceiling
      where mesh frames reuse the omni-pack envelope; otherwise per mesh-protocol-defined
      max) — round-trip must succeed without panic.
    - Negative tests for malformed inputs:
      - Truncated input → `Err` (no panic).
      - Trailing bytes after a valid encoding → `Err(WireError::TrailingBytes)`
        per OIP-Serde-004 § S2 canonical-encoding no-trailing-bytes invariant.
      - Random-byte fuzz input (≥ 1024 iterations via `proptest`) — must surface as
        `Err`, never `panic!`. Postcard is generally robust; the proptest harness is a
        sanity net.
    - Coverage target: **≥ 85 % line coverage** on the touched files of `omni-mesh`.
  - **E2E test deferred** until `omni-mesh` exposes a service surface (Phase 2). Until
    then the round-trip tests above are the observable-behaviour surrogate. Document
    this deferral inline in `crates/omni-mesh/tests/wire_round_trip.rs` header
    comment.
  - **CHANGELOG:** `docs/changelog.md` row added under the implementation date noting
    "closes the bincode→postcard wire-format gap flagged in TASK-015's CHANGELOG entry"
    (cross-reference to the prior CHANGELOG row produced by TASK-015).
  - **Gates:**
    - `cargo fmt --all -- --check`
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings` (the
      `disallowed-methods` lint in `clippy.toml` will fail loudly if any raw
      `postcard::*` or `bincode::*` call sneaks back in).
    - `cargo test --workspace --all-features -- --test-threads=1` (or default
      parallelism if TASK-012 has already removed the `--test-threads=1` gate).
    - `cargo deny check` green; verify no orphan `bincode` transitive remains via
      `cargo tree -i bincode` returning "package not found".
- **Security considerations:**
  - **Wire-format swap is a compatibility break.** No external consumers of `omni-mesh`
    exist today (the crate is a stub), so no external incident path. Document the
    break explicitly in the CHANGELOG row.
  - **Adversarial-input robustness:** `postcard` 1.0 is generally robust against
    malformed inputs (no allocation amplification, no recursion-depth blow-up
    by default for the types `omni-mesh` defines), but the proptest harness
    above is the formal validation.
  - **No new dependency.** `postcard 1.0` is already pinned at the workspace level
    (see `Cargo.toml` workspace.dependencies line 177) per `docs/09-tech-specifications.md`
    § Serialization. `omni-mesh` consumes it transitively via `omni-types::wire`.
  - **Spurious-panic surface:** any `.unwrap()` / `.expect()` on a wire-encoding/
    decoding boundary in `omni-mesh` must be replaced with explicit `?` propagation
    using `WireError`-aware error types. Clippy `unwrap_used`/`expect_used` already
    `warn` workspace-wide, so the PR diff surfaces any new offender.
- **Suggested subagent assignment:** **`impl-mesh`** (new instance, to be spawned
  when `omni-mesh` bring-up enters scope — Wave 4 or later; **not** Wave 1) +
  **`test-engineer`** for the proptest + round-trip harness. **`code-reviewer`** at
  PR time.
- **References:**
  - `oips/oip-serde-004.md` (`Active` post TASK-002) § S2 canonical encoding;
  - `docs/03-mesh-protocol.md:197` (current bincode reference, target of doc patch);
  - OIP-013 § S5.5 (omni-pack v1 envelope, the only place a mesh frame might be
    nested inside a driver-load context);
  - `crates/omni-types/src/wire.rs` (existing canonical helper module, verified
    present per P7.2.M2 closure).

---

### TASK-016 — Schedule + scope external cryptographer engagement (P3.2)

- **Priority:** P3 (funding-dependent — blocks on P4.2; the engagement template is
  already drafted in `docs/audits/cryptographer-engagement-template.md`).
- **Phase alignment:** Phase 1 cryptographic review (pre-audit).
- **Crate ownership:** N/A.
- **Dependencies:** P4.2 funding closed.
- **Effort:** XL (4–8 weeks calendar once funded).
- **Acceptance criteria:**
  - Engagement contract signed; `AWAITING_CRYPTO_REVIEW` marker dropped from
    `omni-crypto` post-review.
  - At least one written review document filed under `docs/audits/`.
  - Any cryptographer findings filed as issues; each fix landed as its own task.
- **Security considerations:** the whole point of this task.
- **Subagent assignment:** **lead-only**.
- **References:** `docs/audits/cryptographer-engagement-template.md`; `todo.md` § P3.2.

---

### TASK-017 — Intel TDX backend (P5.2)

- **Priority:** P3 (funding-dependent — needs TDX-capable hardware; cloud TDX via Azure
  Confidential VMs is the documented contingency in `progress-omni.md` § 7).
- **Phase alignment:** Phase 1 closure deliverable "Intel TDX / AMD SEV-SNP attestation".
- **Crate ownership:** `crates/omni-tee/src/tdx.rs` + new helpers under `crates/omni-tee/`.
- **Dependencies:** P4.2 funding; TDX-capable hardware or Azure Confidential VM access.
- **Effort:** XL (3–6 weeks calendar).
- **Acceptance criteria:**
  - Real `TdxBackend` implementing `AttestationSource`; produces DCAP-format quotes.
  - Unit tests via test vectors from Intel TDX SDK.
  - **E2E test** under `tests/e2e/p5_2_tdx_attest.rs` (gated `#[cfg(target_feature =
    "tdx")]` or feature `tdx-hw`) that actually requests a quote on TDX silicon.
  - Documentation: `docs/audits/p5-2-tdx-attest-report.md`.
- **Security considerations:** real attestation surface; pre-deployment cryptographer
  review required (intersection with TASK-016).
- **Subagent assignment:** **`impl-tee-tdx`** (new instance; deep TDX expertise needed).
- **References:** OIP-Driver-TEE-016 § S5; `todo.md` § P5.2.

---

### TASK-018 — AMD SEV-SNP backend (P5.3)

- **Priority:** P3 (funding-dependent; companion of TASK-017).
- **Phase alignment:** Phase 1 deliverable.
- **Crate ownership:** `crates/omni-tee/src/sev_snp.rs`.
- **Dependencies:** P4.2 funding; SEV-SNP hardware (Zen 4 EPYC or AMD Ryzen
  desktop class).
- **Effort:** XL (3–6 weeks).
- **Acceptance criteria:** mirror TASK-017 with SEV-SNP `SNP_GET_REPORT` syscall path.
- **Security considerations:** as TASK-017.
- **Subagent assignment:** **`impl-tee-sev-snp`** (new instance).
- **References:** OIP-Driver-TEE-016 § S6; `todo.md` § P5.3.

---

### TASK-019 — External kernel + capability audit (P6.8)

- **Priority:** P3 (funding-dependent; Phase 1 closure deliverable).
- **Phase alignment:** Phase 1 closure.
- **Crate ownership:** N/A.
- **Dependencies:** P4.2 funding; TASK-004 / 005 / 006 / 010 closed; TASK-016 closed.
- **Effort:** XL (4–8 weeks calendar).
- **Acceptance criteria:** signed audit report filed under `docs/audits/`; findings
  triaged as individual tasks.
- **Security considerations:** the whole point.
- **Subagent assignment:** **lead-only**.
- **References:** `todo.md` § P6.8.

---

### TASK-020 — `OIP-Crypto-002` (STARK vs SNARK) drafting / promotion (P3.3)

- **Priority:** P3 (Phase 2+ scope; the resolution clause in
  `docs/04-security-model.md` line 157 already names STARKs as the v1 choice via
  `oips/oip-crypto-002.md` Draft, but the OIP itself never moved past Draft).
- **Phase alignment:** Phase 2 entry blocker (compliance-proof scheme is on the Phase 4
  list but the OIP's `Draft → Active` should land in Phase 2 to give cryptographers time
  to review).
- **Crate ownership:** `oips/oip-crypto-002.md` only at this stage.
- **Dependencies:** TASK-016 (cryptographer review).
- **Effort:** L (1–2 weeks editorial; the technical material is already drafted).
- **Acceptance criteria:**
  - OIP transitions `Draft → Review → Last Call → Active`.
  - `oip-lint` green.
  - Implementation OIP-follow-up TASK assigned separately.
- **Security considerations:** picks the cryptographic compliance-proof scheme; load-
  bearing for v1.0.
- **Subagent assignment:** **lead-only** for editorial decisions; **`impl-docs-oip`**
  for the prose patches.
- **References:** `oips/oip-crypto-002.md` Draft; `todo.md` § P3.3.

---

### TASK-021 — `OIP-Container-006` (`OmniContainer`) promotion path

- **Priority:** P3 (Phase 2+ scope; userspace container engine is documented in
  `docs/02-architecture.md` § "OMNI App Mesh" but no crate beyond skeleton; closure of
  Phase 2 AI Runtime is the bottleneck, not container engine).
- **Phase alignment:** Phase 2+.
- **Crate ownership:** `oips/oip-container-006.md` + `crates/omni-container/`.
- **Dependencies:** Phase 1 closure complete; the funding/team decisions in `todo.md`
  § P8.2.
- **Effort:** XL (a quarter+; phased delivery per OIP-006 itself).
- **Acceptance criteria:** out of scope for this planning window; tracked here only to
  pin the dependency tree.
- **Security considerations:** future-tracked.
- **Subagent assignment:** TBD.
- **References:** `oips/oip-container-006.md`; `todo.md` § P8.

---

## 3. Suggested execution wave plan

The team available is documented as **3 parallel `rust-implementer` instances + 1
`test-engineer` + 1 `code-reviewer`.** Tasks below are sequenced so each wave fully
consumes that capacity (or stays under it when bottlenecked by founder-only / lead-only
actions).

### Wave 1 (no inter-task dependencies, security-and-governance first)

Goal: close every overdue editorial / governance hygiene action and produce the SDK
helper unblocking every driver bring-up downstream.

| Slot | Task | Owner | Wall-clock |
|---|---|---|---|
| Lead (founder) | TASK-001 (branch protection update) | lead | < 30 min |
| Lead (founder) | TASK-002 (close Bounty + Serde OIPs Last Call) | lead | ~ 1 h |
| Lead (founder) | TASK-008 (key-custody OIP drafting) | lead / planner | 1–2 d |
| `impl-driver-shared` | TASK-003 (omni-driver-shared SDK) | impl-1 | 4–6 h |
| `impl-driver-pack` | TASK-007 (omni-driver-pack tool) | impl-2 | 4–8 h |
| `impl-docs` | TASK-015 (OMNI-PROTO-v0.2 docs) | impl-3 | 1–2 h |
| `test-engineer` | shared support across TASK-003 + TASK-007 | tester | full slot |
| `code-reviewer` | reviews TASK-003 + TASK-007 + TASK-015 PRs at wave close | reviewer | full slot |

**End-of-wave gate:** `cargo test --workspace --all-features -- --test-threads=1` green
(≥ 893 pass); `cargo deny check` green over the new `toml` dep proposed by TASK-007.

### Wave 2 (depends on Wave 1)

Goal: enrol the bootstrap issuer key, deliver the IOMMU backends, and start the
SIGSEGV refactor in parallel.

| Slot | Task | Owner | Wall-clock |
|---|---|---|---|
| Lead + impl-1 | TASK-009 (Stichting / founder bootstrap key enrolment) | lead + impl-1 | 1–2 h after key gen |
| `impl-kernel-iommu` | TASK-010 (VT-d + AMD-Vi backends) | impl-1 | 5–10 d |
| `impl-kernel-paging` | TASK-012 (SIGSEGV fix) | impl-2 | 1–3 d |
| `impl-ci` | TASK-013 (mb11 + mb12 smoke automation) | impl-3 | 4–6 h |
| `test-engineer` | DMAR/IVRS parser corpus + concurrent arena tests | tester | full slot |
| `code-reviewer` | reviews TASK-010 + TASK-012 + TASK-013 PRs | reviewer | full slot |

**End-of-wave gate:** every driver-prerequisite is in place. `qemu-boot-smoke.yml` runs
without `--test-threads=1`; `[iommu] vendor=…` line present in QEMU q35 boot logs.

### Wave 3 (driver live bring-up — depends on Wave 2 IOMMU + bootstrap key)

Goal: take a real driver from `_start` to `DriverOk` end-to-end on Proxmox VMID 103.

| Slot | Task | Owner | Wall-clock |
|---|---|---|---|
| `impl-driver-net-virtio` | TASK-004 (virtio-net live) | impl-1 | 3–5 d |
| `impl-driver-nvme` | TASK-005 (NVMe live) | impl-2 | 5–8 d |
| `impl-driver-e1000e` | TASK-006 (e1000e QEMU + future bare-metal) | impl-3 | 2–3 d |
| `test-engineer` | QEMU E2E harness + Proxmox VNC captures | tester | full slot |
| `code-reviewer` | mandatory review of each live-driver PR | reviewer | full slot |

**End-of-wave gate:** Phase 1 "Drivers (in user space)" deliverable
**substantially complete** for the storage + network leg. Roadmap Phase 1 tracker
moves to **~ 99.99 %**. The TEE leg remains funding-dependent.

### Wave 4 (Phase 1 → Phase 2 bridge — depends on Wave 3)

Goal: skeleton the filesystem service, start `omni-mesh` wire-format alignment, and
schedule the Phase 2 entry OIP; pause for funding-dependent items.

| Slot | Task | Owner | Wall-clock |
|---|---|---|---|
| `impl-fs` | TASK-011 (omni-fs skeleton) | impl-1 | 2–3 d |
| `impl-mesh` | TASK-022 (omni-mesh bincode → postcard alignment) | impl-2 | 2–8 h (M) / 1–3 d (L if wire helper expansion) |
| `impl-docs-oip` | TASK-020 prose patches (OIP-Crypto-002 promotion) | impl-3 | parallel |
| Lead | TASK-020 editorial close (OIP-Crypto-002 promotion) | lead | 1–2 weeks editorial |
| Lead | TASK-016 / 017 / 018 / 019 (funding-dep) | lead | scheduled when funded |
| `test-engineer` | proptest harness for TASK-022 + TASK-011 unit tests | tester | full slot |
| `code-reviewer` | reviews TASK-011 + TASK-022 PRs | reviewer | full slot |

**End-of-wave gate:** the project sits at the Phase 1 → Phase 2 boundary, ready for a
formal "Phase 2 Entry" OIP draft. `omni-mesh` no longer references `bincode`; the
`docs/03-mesh-protocol.md:197` reference is closed.

### Waves 5+ — Funding-dependent

- TASK-016 (cryptographer review)
- TASK-017 (Intel TDX backend)
- TASK-018 (AMD SEV-SNP backend)
- TASK-019 (external audit)
- TASK-021 (OmniContainer KVM backend)

Sequencing inside this group depends on which funding source closes first and on the
hardware procurement schedule (`progress-omni.md` § 7 risk row "Hardware TEE acquisition").

---

## 4. Risks and unknowns

| # | Risk / unknown | Probability | Impact | Mitigation / open question |
|---|---|---|---|---|
| R1 | `DRIVER_CAP_ISSUER_SEED` is a **published 32-byte constant** in `crates/omni-kernel/src/driver_cap_issuer.rs`. Anyone who reads the repo can forge a token that passes `verify_signed_token` on a stock build. | **Certain** (it's in git history) | **High** during dev; **Critical** if a `v0.3.0-alpha.1` build is deployed beyond developer machines | TASK-008 (OIP) + TASK-009 (bootstrap key enrolment). **Open question for the lead:** are any non-developer machines running `v0.3.0-alpha.1` today? If yes, escalate immediately. |
| R2 | `dma_map_handlers::dma_map` is **passthrough** (`iova == user_va`). Any compromised driver can DMA-read or DMA-write **any** RAM frame the bare-metal allocator handed it. | **Certain** (documented in OIP-013 Appendix B amendment 1) | **Critical** in production; tolerable in dev | TASK-010 (VT-d / AMD-Vi backends) is the hard fix. **Open question for the lead:** is the lead willing to mark every `v0.3.x` build as "developer-only / no untrusted drivers" in the release notes until TASK-010 lands? |
| R3 | `omni-driver-pack` requires a `toml` parser. Hand-rolled vs. `toml = "0.8.19"` is a real choice with security implications. | Medium | Low (tooling) | TASK-007 acceptance criteria require the lead to approve the dep addition. **Open question:** dep or hand-roll? Planner recommends dep. |
| R4 | Proxmox VMID 103 lacks an e1000e passthrough; TASK-006 can only validate in QEMU. Production e1000e on real silicon may surface issues invisible to QEMU. | Medium | Medium | Document the QEMU-only validation in TASK-006 audit report; defer real-hardware smoke to Phase 5 / funding-dep. |
| R5 | The `omni-kernel --lib` SIGSEGV is **CI-mitigated, not fixed**. Any kernel-MM change risks re-tripping it locally. | Medium | Medium | TASK-012 fixes; until then, mandate `--test-threads=1` for any local `cargo test -p omni-kernel`. |
| R6 | The IOMMU vendor backends (TASK-010) introduce ~ 500–1000 SLOC of high-trust code into the kernel. Cryptographer review (TASK-016) does not cover IOMMU programming. | Medium | High | Engage an independent reviewer for TASK-010 PR specifically; flag it for the **OIP Review Team** at the lead's discretion. |
| R7 | The Stichting OMNI is not yet incorporated. TASK-009 ships a **founder-personal** Ed25519 key in `KNOWN_ISSUERS` until that closes. Rotation in 90 days is mandatory. | High | High | Explicit 2026-08-19 calendar reminder; TASK-008 OIP must specify the rotation procedure. |
| R8 | `cargo test (ubuntu-24.04) SIGSEGV` admin-bypass pattern recorded in `progress-omni.md` § 4.5 risks normalisation. Future PRs may be tempted to bypass other red checks. | Medium | High | TASK-012 removes the underlying need. Until then, **lead-only** allowance per PR, documented in audit log. |
| R9 | Funding pipeline (P4.2) has no committed closure date. Funding-dep tasks (TASK-016 / 017 / 018 / 019) cannot be scheduled. | High | Critical | Pre-position the work that does NOT require funding (Wave 1–4); document the funding gate explicitly in each task. |
| R10 | OIP-Driver-TEE-016 (`Active` since 2026-05-20) has no Reference Implementation and depends on TDX / SEV-SNP hardware. The TEE driver is the third Phase-1 deliverable and the only one that cannot be delivered without funding. | Certain | High | Documented in OIP-016 Appendix A as funding-dep; the Phase-1 closure tracker explicitly excludes the TEE leg from the "≈ 99.99 %" target. |
| R11 | The agent spec demands "every task includes unit tests; E2E where observable." Some governance tasks (TASK-001, TASK-002, TASK-008, TASK-014) have **no code path** and therefore no unit tests. | Certain | Low | Each affected task lists its **CI-level test** (oip-lint, branch-protection enforcement, doc lint) as the test surrogate. The lead may override if they prefer formal exemption notation. |
| R12 | ~~`docs/02-architecture.md` line 251 still lists "Filesystem: native OMNI FS vs. existing options (ZFS port, ext4 via compatibility)" as an open architectural question. Resolving it shapes TASK-011 substantially.~~ **Closed 2026-05-22 by [`OIP-FS-018`](../../oips/oip-fs-018.md) `Active` (§5.3 ¶1 ballot, recorded in [`docs/audits/oip-editors-report-2026-Q2.md`](../audits/oip-editors-report-2026-Q2.md)).** Direction: native `OmniFS` as the single canonical persistent FS (Rust, user-space, CoW, capability-bound, AEAD-integrity, per-volume confidentiality); foreign FS (ext4, NTFS) admitted only as read-only compat services behind a `READONLY_COMPAT_FS` capability, scheduled no earlier than Phase 3 (`OIP-FS-Compat-Ext4-NNN`, `OIP-FS-Compat-NTFS-NNN` follow-ups); ZFS port rejected for v0–v2 (revisitable in v3.x). Quantitative parameters frozen in OIP-FS-018 §S1.1. **TASK-011 remains stub-only**; re-scoping to "OmniFS v0 host preparation" will be filed as a new development-plan task at Phase 2 entry. | ~~Medium~~ Closed | ~~Medium~~ Closed | Closed (see Risk column). |
| R13 | `OIP-Process-001 §6.5` bootstrap deadlock clause was used to fast-path OIP-013 / 014 / 015 / 016. Every fast-path mandates post-Bootstrap re-ratification per §5.5.e. The re-ratification list is growing. | Certain | Low (technical); Medium (governance hygiene) | Track an OIP-Process-001 amendment in the post-Stichting transition (out of scope for this planning window; mention here for awareness). |
| R14 | `docs/03-mesh-protocol.md:197` still references `bincode 2.0` while `OIP-Serde-004` ratifies `postcard 1.0` as the canonical workspace wire format (TASK-015 closes the handshake.md leg of the same doc↔code lag). `omni-mesh` source is also still on `bincode`. Internally observable correctness gap only — no external consumers today, but it confuses any reader following the protocol doc into the crate. | Certain (verified via `git grep -n 'bincode' crates/omni-mesh/ docs/03-mesh-protocol.md`) | Low (technical) / Medium (doc hygiene) | TASK-022 is the planned closure path: encoding swap in `omni-mesh` + doc patch in the **same** PR. Effort gated on `omni-mesh` bring-up entering scope (Wave 4 or later). **Open question for the lead:** is there appetite to bring TASK-022 forward to Wave 2/3 as a "fix on sight" once OIP-Serde-004 lands `Active` via TASK-002, or hold it to Wave 4 with the rest of the mesh bring-up? |

---

## 5. Notes for the lead

1. **Approve / revise / escalate.** Per the planner spec, no implementer starts work
   until you approve this plan (or escalate specific tasks to the OIP Review Team).
2. **Hard blockers to surface immediately:**
   - **TASK-008 / 009** (issuer key custody + bootstrap key). The DEV-ONLY signing seed
     is currently a **published constant**. Before any non-developer build of
     `v0.3.0-alpha.1` ships, this must be addressed. The fast-fix is the bootstrap key
     in TASK-009 plus a release-notes caveat; the proper fix is TASK-008 → TEE-derived
     key.
   - **TASK-010** (IOMMU backends). The DMA-passthrough caveat is the most consequential
     unmitigated security gap in the Phase-1 closure deliverable. Recommended sequencing:
     **TASK-010 before TASK-005** (NVMe live), because NVMe has the largest DMA window
     of any first-party driver.
3. **Funding gate.** Waves 1–4 are deliverable without any funding action. Wave 5+ all
   require P4.2 closure.
4. **`todo.md` update (TASK-014).** Planner is forbidden from touching `todo.md`; only
   the lead can apply the proposed patches in TASK-014. Apply when convenient.
5. **OIP Review Team escalation candidates:** TASK-008 (key custody), TASK-010 (IOMMU
   kernel surface), TASK-020 (STARK vs SNARK). Planner recommends escalating at least
   TASK-008.

---

*End of plan.*
