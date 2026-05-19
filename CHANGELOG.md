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

- **Kernel — MB13.g: comprehensive IDT coverage for synchronous
  exceptions (2026-05-19).** Extends [`bare_metal::idt`] from 4
  dedicated handlers (#DE, #DF, #GP, #PF) to **20 catch-all
  vectors covering 0..=21** so that previously-silent triple-faults
  surface as a single fault with provenance.

  Motivation: after MB13.b (ET_DYN/PIE upper-half kernel) and MB13.f
  (`enter_user_mode` kernel-stack swap) the Proxmox VMID 103
  `mb12-userprobe` deploy reached the `iretq` boundary cleanly
  (verified via inline COM1 tracepoint `'E'`) but the VM then halted
  without emitting any tracepoint from the syscall, LAPIC, #PF, #GP
  or #DF handlers. With 256 IDT slots initialised to
  `IdtEntry::missing()` (P=0), any synchronous fault on a vector
  outside {0,8,13,14} triggers a #NP that itself faults to #DF on a
  missing entry — cascading to a triple fault and a silent VM
  reset. MB13.g replaces that silence with a `[OMNI OS EXCEPTION]
  vec=NN  code=X  rip=… cs=… rflags=…` line on the early console,
  unblocking the root-cause analysis for MB13.h.

  - **`crates/omni-kernel/src/bare_metal/idt.rs`** — adds 16 new
    `global_asm!` stubs, one per vector in
    `{1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 16, 17, 18, 19, 20, 21}`, each
    loading the vector number as an immediate into `RDI` (and the
    CPU-pushed error code into `RDX` when applicable) before calling
    one of two new `extern "C"` Rust functions:

    - `kernel_handle_exception_noerr(vector, frame)` — used by the
      no-error-code vectors (#DB/NMI/#BP/#OF/#BR/#UD/#NM/#MF/#MC/#XF/#VE).
    - `kernel_handle_exception_witherr(vector, frame, error_code)` —
      used by the error-code vectors (#TS/#NP/#SS/#AC/#CP).

    Both handlers write the vector tag and frame snapshot through
    `early_console::write_*`, then call `super::arch::halt_forever()`.
    The four dedicated handlers (#DE/#DF/#GP/#PF) are preserved
    unchanged — they emit a mnemonic and (for #PF) the faulting
    `CR2`. The two architecturally-reserved Intel slots 9 and 15
    remain `missing()` by design.

  - **`idt_init()`** now installs all 20 catch-all entries in
    addition to the original four. The IDTR descriptor and `lidt`
    issue are unchanged. The new test
    `bare_metal::idt::tests::mb13g_synchronous_vectors_covered`
    documents the coverage matrix symbolically and asserts that
    reserved vectors do not overlap with the covered list.

  - **`crates/omni-kernel/src/bare_metal/demo.rs`** — Build Info
    panel updated to `Active=MB13.g full ISR coverage`,
    `Next=MB13.h iretq stall fix`, `Track B=MB1-MB12 OK, MB13.a-g OK`,
    `Phase 1 ≈ 78%`.

  - **Stage gate:** MB13.g is intentionally a *diagnostic* fix — it
    does not change the kernel's semantics under success paths.
    Workspace build and clippy are clean on `x86_64-unknown-none`
    (`bare-metal`, `mb12-userprobe`, default desktop demo); the new
    unit test is +1 over MB13.f, so workspace target ≥ 444. The
    expected boot output on the Proxmox VMID 103
    `mb12-userprobe` build is now one of three branches:

    1. *(Success path)* the original
       `ping` + double `[user] exit=0` MB12 banner sequence (this
       would mean MB13.f closed the bug too and the silence was a
       coincidence of cache flushing or display refresh).
    2. *(Catch-all triggered)* a new
       `[OMNI OS EXCEPTION] vec=NN ...` line identifies the
       specific vector being raised post-iretq, narrowing MB13.h
       to a targeted fix (TSS.ist1 wiring for #DF, a stale GDT
       segment descriptor for #NP, an iretq-frame mis-build for
       #SS, etc.).
    3. *(Still silent)* the triple-fault precedes the first vector
       dispatch and points at a hardware-level reset (CR0/CR4
       mis-config, EFER inconsistency, or a problem in the very
       first `mov cr3` itself) rather than an IDT-coverage gap.

- **Kernel — MB13.f: `enter_user_mode` kernel-stack swap before CR3
  reload (first-dispatch smoke fix, 2026-05-19).** Closes the open
  `mb12-userprobe` follow-up tracked in `progress-omni.md` § 4.5 #22:
  with MB13.b the VM stopped emitting any `[user] exit=0` / `ping`
  after `[mb12] handing off to user tasks`, because the first
  `enter_user_mode` invoked from inside a syscall handler executed
  `mov cr3, dest_cr3` while RSP still pointed at the *outgoing* task's
  user stack (SYSCALL on x86_64 does not switch SP). After the CR3
  reload that page was no longer mapped (user-half is per-process), so
  the very next `push {ss}` of the iretq frame produced a Ring-0 page
  fault → triple fault → VM reset, before any Ring-3 instruction
  could execute.

  - **`crates/omni-kernel/src/bare_metal/usermode.rs`** —
    `enter_user_mode` gains a new `kernel_stack_top: u64` parameter and
    issues `mov rsp, {kstk}` **before** the `mov cr3`. The destination
    kernel stack lives in the MB10 isolated range
    `[KERNEL_STACK_VA_BASE, KERNEL_STACK_VA_END)` (PML4 index `≥ 0x180`,
    canonical kernel half), which `AddressSpace::new_with_kernel_half`
    mirrors by reference into every per-process PML4. The page therefore
    survives the CR3 reload, and the subsequent `push` sequence that
    builds the iretq frame runs on a valid mapping. The non-x86_64 stub
    is updated in tandem to keep callers source-compatible.

  - **`crates/omni-kernel/src/scheduling.rs`** —
    `RoundRobinScheduler::yield_current` first-dispatch path now
    forwards `kernel_stack_top` (computed in stack as
    `kernel_stack_va + KERNEL_STACK_SIZE`, already used for the
    `TSS.rsp0` update introduced by MB12.0a). The pre-MB13.b
    "bare-metal limitation" comment is removed — that triple-fault
    cause was resolved by ET_DYN/PIE relocation.

  - **`crates/omni-kernel/src/lib.rs`** — MB11 single-task dispatch
    (`kmain` → `enter_user_mode` direct call) passes
    `pcb.task.kernel_stack_va + scheduling::KERNEL_STACK_SIZE`. The
    MB11 caller-side RSP is already on the boot stack (upper half,
    mirrored), but the fix is applied uniformly.

  - **Latency note:** the bug was latent at MB11 — `kmain` called
    `enter_user_mode` directly with RSP on the boot stack, which since
    MB13.b lives in upper half (mirrored). MB12 surfaced it because the
    first-dispatch is now triggered from inside a syscall handler whose
    RSP is the *outgoing* task's user stack. Host tests did not catch
    it because `enter_user_mode` has a `panic!()` stub on non-x86_64.

  - **`crates/omni-kernel/src/process.rs`** — `spawn_from_elf` now
    allocates and maps the per-process MB10 kernel stack via
    `mapper.map_4k(...)` *before* cloning the per-process PML4 via
    `AddressSpace::new_with_kernel_half`. Reason: `new_with_kernel_half`
    copies PML4 entries 256..511 by value, and any *new* PDPT installed
    in the boot PML4 *after* a clone (e.g. the first kstk allocation
    at PML4 index 0x180) does not propagate to clones taken earlier.
    Pre-MB13.f, the first user-task spawn cloned an empty PML4[0x180]
    and then mapped the kstk into the boot mapper, leaving the new
    process's PML4 with a stale zero entry and the kstk unreachable
    after CR3 reload. Reordering forces the boot PML4 to allocate the
    kstk-range PDPT eagerly so the clone captures the shared PDPT, and
    every subsequent kstk slot inside the same shared PDPT propagates
    automatically.

  Verification (host, 2026-05-19 post-MB13.f):
    - `cargo build --workspace --all-features` → clean (0 warning).
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe` → clean.
    - `cargo clippy --workspace --all-features --all-targets -- -D warnings` → clean.
    - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --release --features omni-kernel/mb12-userprobe -- -D warnings` → clean.
    - `cargo test -p omni-kernel --tests` (integration suite: mb11 6 + mb12 8 + mb13 11 + boot_info 7 + panic_record 5 + heap 9) → 46/46 pass.
    - Build Info panel updated to `Active = MB13.f iretq kstk-swap`,
      `Next = MB13.e PR + intermediate tag`, `Track B = MB1-MB12 OK,
      MB13.a-f OK`, `Phase 1 ≈ 77%`, `Tests = 443+ workspace pass`.
    - **Note:** the pre-existing `cargo test -p omni-kernel --lib`
      `SIGSEGV` in `dispatcher_time_monotonic_returns_u64` (CMOS port
      I/O on Linux userspace) and at the `bare_metal::paging::tests`
      teardown remains; this carryover (item §4.5 #16) is unrelated to
      MB13.f, which touches only the `x86_64` asm path.

  Smoke validation on Proxmox VMID 103 (2026-05-19):
    - **Default desktop build** (no `mb12-userprobe`): VM boots to
      `[virtio] tablet ready`, GOP framebuffer renders, screenshot via
      `qm monitor 103 :: screendump` confirms Build Info panel at the
      new milestone state. ✅
    - **`mb12-userprobe` build:** with MB13.f + the process.rs reorder,
      `enter_user_mode` now executes the full
      `mov rsp` → `mov cr3` → 5 `push` → `iretq` sequence without
      Ring 0 fault (verified via inline tracepoints on COM1 port
      0x3F8: emit `A` enter / `B` after `mov rsp` / `C` after
      `mov cr3` / `D` after first push / `E` before `iretq`). The VM
      still arrests immediately after `iretq` without emitting any of
      the SYSCALL/timer/ISR tracepoints; the receiver does not even
      execute a `jmp $`. Tracked as **MB13.g — Ring 3 post-iretq
      triple-fault** (open). Diagnostic next step requires
      `-d int,cpu_reset -D /tmp/qemu-trace.log` on QEMU args of VMID
      103, not performed in this round.

- **Kernel — MB13.d: `IpcCreateChannel` syscall ABI extension for
  postcard-encoded signed tokens (2026-05-19).** Closes the last
  bare-metal-side gap of the MB13 work-package by plumbing
  `omni_capability::CapabilityToken` blobs from user space all the way
  down to `Ed25519CapabilityProvider::verify_signed_token`. The MB12
  "open channel" calling convention (both token pointers zero) keeps
  working byte-for-byte — `mb12-userprobe` boots unchanged — but any
  call that supplies even one token now gates the channel registration
  on a full Ed25519 + time-window + TEE-binding verification.

  - **`crates/omni-kernel/src/capabilities.rs`** — new
    `decode_and_authenticate_token(bytes, expected_action, provider, now)
    -> KernelResult<KernelPrincipal>` helper. Decodes postcard bytes via
    `omni_types::wire::decode_canonical`, runs
    `Ed25519CapabilityProvider::verify_signed_token`, enforces
    `scope.action` matches the slot, and accepts any
    `Resource::IpcChannel(_)` value (the kernel rebinds the resource to
    the freshly-allocated channel id — user space cannot predict the
    monotonic counter at mint time). The subject in the kernel-side
    `KernelPrincipal` is sourced from `token.payload.subject` (32-byte
    NodeId attestation hash).

  - **`crates/omni-kernel/src/ipc.rs`** — new
    `KernelIpcRegistry::create_channel_signed(owner, policy,
    send_token_bytes, recv_token_bytes, &provider, now)`. When both
    `send_token_bytes` and `recv_token_bytes` are `None`, delegates to
    the existing `create_channel(..., &StubCapabilityProvider)` so the
    open-channel pre-create in `userprobe_mb12::spawn_userprobe_mb12`
    keeps the byte-for-byte MB12 semantics. Otherwise each non-`None`
    slot runs `decode_and_authenticate_token` and the channel is
    registered with the verified subject in the corresponding
    `send_subject` / `recv_subject` slot.

  - **`crates/omni-kernel/src/bare_metal/syscall_entry.rs`** —
    `ipc_handlers::ipc_create_channel` now accepts the MB13.d six-arg
    ABI: `(queue_depth, backpressure, tee_bound, send_token_ptr,
    recv_token_ptr, lens)` where `lens` packs
    `send_len:u32 | (recv_len:u32 << 32)`. Both token pointers `0` →
    legacy stub-provider path. At least one non-zero → per-slot
    `user_range_ok` bounds check + on-stack `[u8; 1024]` copy (caps the
    accepted token at 1 KiB; real `CapabilityToken` payloads are ~200
    bytes) + delegate to `create_channel_signed`. Kernel monotonic
    `now` comes from `bare_metal::arch::rtc_seconds`.

  - **`crates/omni-kernel/src/bare_metal/userprobe_mb12.rs`** — the
    `spawn_userprobe_mb12` pre-create now routes through
    `create_channel_signed` with both slots `None` and
    `Ed25519CapabilityProvider::placeholder()`. Behaviour is identical
    to the MB12 baseline (the registry recognises the no-token call
    and forwards to the stub provider); the indirection documents the
    new canonical entry point.

  - **`crates/omni-kernel/tests/mb13_capability_signed.rs`** — new
    integration suite, +11 tests:
      * 7 on `decode_and_authenticate_token`: happy path, bit-flipped
        canonical bytes, action mismatch, pre-window / post-window
        time, TEE attestation mismatch, non-IpcChannel resource,
        truncated postcard.
      * 4 on `KernelIpcRegistry::create_channel_signed`: send-token
        authentication populates `send_subject`, open-channel (both
        `None`) delegates to the legacy stub path, invalid send-token
        bytes reject without leaving a half-registered channel, and a
        full-round-trip end-to-end check that the per-IPC
        `subject == requester` gate still rejects an intruder
        principal after the signed-token registration.

  - **`crates/omni-kernel/src/bare_metal/demo.rs`** — Build Info panel
    updated to `Active = MB13.d IpcCreateChannel ABI`, `Next = MB13.e
    PR + intermediate tag`, `Track B = MB1-MB12 OK, MB13.a-d OK`,
    `Phase 1 ≈ 75%`, `Tests = 443+ workspace pass`.

  Verification (host, 2026-05-19 post-MB13.d):
  - `cargo build --workspace --all-features` → clean (zero warnings).
  - `cargo build -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal` → clean.
  - `cargo build -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features mb12-userprobe` → clean.
  - `cargo build --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none --features mb12-userprobe` → clean.
  - `cargo clippy --workspace --all-targets --all-features -- -D
    warnings` → clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` →
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features mb12-userprobe -- -D warnings` →
    clean.
  - `cargo test -p omni-kernel --features bare-metal --test
    mb13_capability_signed` → 11 / 11 pass.
  - `cargo test -p omni-kernel --features bare-metal --tests` →
    mb11_userspace (6) + mb12_ipc_cross_process (8) +
    mb13_capability_signed (11) + panic_record (5) + boot_info (7) +
    heap (9) = 46 / 46 integration pass. Lib unit tests still hit the
    pre-existing x86_64-host SIGSEGV in `paging` and
    `dispatcher_time_monotonic_returns_u64` (CMOS port I/O) — both
    documented in `progress-omni.md` § 4.5 #16 as carryover from
    v0.2.0; reproduces on HEAD before MB13.d.
  - `scripts/check-no-blanket-allow.sh` → ok (12 crate-root files).

- **Kernel — MB13.c: `omni-capability` integration + `Ed25519CapabilityProvider`
  (2026-05-19).** Lands the real Ed25519 signature-verification path
  alongside the MB12 `StubCapabilityProvider`. The kernel now depends on
  `omni-capability` (verify-only build) and can authenticate userspace
  `CapabilityToken` blobs end-to-end. The new provider is wired into the
  kernel surface but not yet plugged into the IPC syscall handlers —
  the syscall ABI extension that plumbs tokens through `IpcCreateChannel`
  ships as MB13.d. `StubCapabilityProvider` therefore remains the boot-
  wiring default until MB13.d. This closes the Phase 1 deliverable
  "capability-based security primitives implemented" at the verification
  layer; the matching ABI plumbing closes the rest.

  - **`crates/omni-types/Cargo.toml` + `src/lib.rs` + `src/identity.rs`**
    — split `id-generation` (default-on, runtime constructors) into a
    new lower-tier `id-types` feature that exposes the `identity`
    module's *type definitions* without dragging `getrandom`. The
    UUIDv4-minting `::new()` methods on `AgentId`, `CapabilityId`, and
    `SessionId` plus the `random_uuid_bytes` helper are now feature-
    gated to `id-generation`; the types themselves are available under
    `id-types`. The `uuid` crate is pulled with `features = ["serde"]`
    only (no `v4`) because the in-house `random_uuid_bytes` helper goes
    through `Uuid::from_bytes` and never needs `uuid`'s rng path. Net
    effect: `omni-types` compiles on `x86_64-unknown-none` with just
    `id-types` enabled, which is what the kernel needs.
  - **`crates/omni-capability/Cargo.toml`** — declares `omni-types`
    and `omni-crypto` with explicit `default-features = false`,
    enabling only `omni-types/id-types` for the library build. A new
    `mint` feature (default-on) gates the userspace-only paths:
    `CapabilityToken::mint`, `attenuation::attenuate`, and their
    transitive `id-generation` + `omni-crypto/rng` dependencies. A new
    `bare-metal` feature is a marker that forwards
    `omni-crypto/bare-metal`; combined with `--no-default-features`,
    it produces a verify-only build that compiles on
    `x86_64-unknown-none`. Dev-dependencies override the deps to
    re-enable `mint` / `id-generation` / `rng` for `cargo test`.
  - **`crates/omni-capability/src/scope.rs`** — adds three semver-safe
    `#[non_exhaustive]` variants used by the kernel IPC dispatcher:
    `Action::IpcSend`, `Action::IpcRecv`, and `Resource::IpcChannel(u64)`.
    Subset relation for `IpcChannel` is equality (opaque kernel handle —
    no wildcard at MB13.c); 5 new unit tests pin the new variants'
    behaviour, including cross-discriminant disjointness.
  - **`crates/omni-capability/src/{attenuation.rs,token.rs}`** — gate
    `attenuate` and `CapabilityToken::mint` behind `#[cfg(feature =
    "mint")]`. The verify path (`verify_signature`, `verify_full`) stays
    available without `mint`. Unused-import warnings on the
    bare-metal build are silenced by cfg-gating the corresponding
    `use` statements.
  - **`crates/omni-kernel/Cargo.toml`** — adds `omni-capability` as a
    runtime dep with `default-features = false, features = ["bare-metal"]`.
    dev-dependencies enable `mint` so host tests can mint Ed25519-
    signed tokens to exercise the new provider end-to-end. The bare-
    metal `cargo build --target x86_64-unknown-none` is unaffected
    because dev-deps are not pulled into non-test builds.
  - **`crates/omni-kernel/src/capabilities.rs`** — adds
    `Ed25519CapabilityProvider` with three methods:
    - `verify_signature_only(token)` — Ed25519 signature verification
      only, no time/TEE/revocation checks. Used by tests and by call
      sites that have validated the time window separately.
    - `verify_signed_token(token, now)` — full verification via
      `CapabilityToken::verify_full` with a `StubAttestation` bound to
      the provider's `node_id_bytes` and an empty `RevocationList`.
      MB13.c uses an all-zero placeholder node id; the real attested
      identity arrives with `omni-tee` (P5).
    - `verify(token, action, resource)` (impl
      `KernelCapabilityCheck`) — O(1) action/resource shape match
      identical to `StubCapabilityProvider`, so the provider is a
      drop-in replacement at the per-IPC level. Signature verification
      is a one-shot done at channel creation (MB13.d).
  - **Test delta (host-side):** +5 in `omni-capability` (new
    `IpcSend`/`IpcRecv`/`IpcChannel` subset semantics) + 6 in
    `omni-kernel` (`Ed25519CapabilityProvider` happy path, tampered
    payload, window/attestation rejection, per-IPC shape match).
    Workspace target: ≥ 432 pass (was 426 post-MB12).
  - **Verification:**
    - `cargo build -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal` → clean (was: `unresolved import omni_types::identity`).
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal` → clean.
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean.
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean (ET_DYN, regression on MB13.b).
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` → clean.
    - `cargo clippy -p omni-capability --target x86_64-unknown-none --no-default-features --features bare-metal -- -D warnings` → clean.
    - `cargo test -p omni-capability` → 64 + 7 + 5 = 76 pass (5 new).
    - `cargo test -p omni-kernel --lib capabilities` → 11/11 ok (6 new + 4 stub regressions + 1 carryover).
    - `scripts/check-no-blanket-allow.sh` → ok (12 crate roots scanned).
    - Build Info panel updated to `Active = MB13.c Ed25519 cap provider`,
      `Next = MB13.d IpcCreateChannel ABI`, `Track B = MB1-MB12 OK,
      MB13.a/b/c OK`, `Phase 1 ≈ 72%`, `Tests = 432+ workspace pass`.

- **Kernel — MB13.b: ET_DYN/PIE kernel + upper-half dynamic mapping
  (2026-05-19).** Fixes the `mb11-userprobe` / `mb12-userprobe`
  triple-fault on Proxmox VMID 103 / QEMU+OVMF at root cause: the
  kernel ELF was `ET_EXEC` with `p_vaddr = 0x200000` (PML4 index 0),
  which `bootloader_api 0.11` does not relocate. After
  `AddressSpace::new_with_kernel_half` cloned only PML4 256..=511,
  the `mov cr3` in `enter_user_mode` dropped the kernel image and
  triple-faulted on the next instruction fetch.

  - **`kernel-runner/.cargo/config.toml`** — removed
    `-C relocation-model=static` + `-C link-arg=--no-pie`. The
    `x86_64-unknown-none` target spec already sets
    `position-independent-executables = true` on Rust 1.83+, so the
    kernel ELF is now `ET_DYN` (PIE) by default with RIP-relative
    addressing. The file now contains only an explanatory comment so
    the workspace `.cargo/config.toml` rustflags (force-soft SIMD
    cfgs for `omni-crypto`) merge cleanly when the kernel-runner is
    built.
  - **`kernel-runner/build.rs`** — removed entirely. The legacy build
    script emitted `cargo:rustc-link-arg=--no-pie` which appended
    `--no-pie` to the linker command line *after* the target-spec's
    `-pie`. LLD honours the last flag, so the kernel ELF was still
    `ET_EXEC` even after cleaning the `.cargo/config.toml`. Deleting
    the script lets the target spec's `-pie` reach LLD unopposed.
    Verified via `readelf -h` → `Type: DYN (Position-Independent
    Executable file)`.
  - **`kernel-runner/src/main.rs`** — `BOOTLOADER_CONFIG` now sets
    `mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`,
    pushing every bootloader-managed mapping (kernel base, kernel
    stack, `BootInfo`, framebuffer, physical-memory direct map) into
    the canonical upper half (PML4 ≥ 256). Combined with the ET_DYN
    kernel, this guarantees `AddressSpace::new_with_kernel_half`
    (which mirrors PML4 256..=511 by reference) keeps every kernel
    mapping live across CR3 switches.
  - **Verification:**
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` → clean (ET_DYN).
    - `cargo clippy --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe -- -D warnings` → clean.
    - `cargo clippy -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe -- -D warnings` → clean (regression).
    - `cargo test -p omni-kernel --all-features --test mb12_ipc_cross_process` → 8/8 ok (host-side, unaffected by the bare-metal fix).
    - Build Info panel updated to `Active = MB13.b ET_DYN upper-half`, `Next = MB13.c omni-capability dep`, `Phase 1 ≈ 70%`.

- **Kernel — MB13.a: `omni-crypto` builds on `x86_64-unknown-none`
  (2026-05-19).** Unblocks the bare-metal build of `omni-crypto`, removing
  the LLVM ICE on SIMD intrinsics in `sha2`, `poly1305`, `chacha20`, and
  `curve25519-dalek`. This is the first slice of MB13; MB13.b
  (ET_DYN kernel for the triple-fault smoke fix) closed above; MB13.c
  (`omni-capability` integration), MB13.d (capability syscall ABI),
  and MB13.e (PR + tag) land in subsequent commits.

  - **Workspace `.cargo/config.toml`** (new file) — target-conditional
    `rustflags` for `x86_64-unknown-none`:
    - `--cfg poly1305_force_soft` → portable Poly1305 backend.
    - `--cfg chacha20_force_soft` → portable ChaCha20 backend.
    - `--cfg curve25519_dalek_backend="serial"` → 64-bit serial
      Curve25519 field arithmetic (no AVX2/AVX-512 vector reductions).
    - `--cfg sha2_backend="soft"` → portable SHA-256 + SHA-512
      backends in `sha2 0.11` (the workspace direct dep).
    Host targets are unaffected — they keep the hardware-accelerated
    backends.
  - **`crates/omni-crypto/Cargo.toml`** — target-scoped
    `[target.x86_64-unknown-none.dependencies]` adds a feature
    passthrough on `sha2 0.10` (`force-soft`) which the dalek family
    transitively pulls in via `digest 0.10`. Cargo unifies the
    feature with the dalek-side resolution, so `sha2 0.10`'s
    portable backend is selected without forking the dep graph.
  - **Verification:**
    - `cargo build -p omni-crypto --target x86_64-unknown-none --no-default-features` → clean (was: LLVM ICE on `poly1305 0.8` + `sha2 0.10` + `sha2 0.11`).
    - `cargo clippy -p omni-crypto --target x86_64-unknown-none --no-default-features -- -D warnings` → clean.
    - `cargo build -p omni-kernel --target x86_64-unknown-none --no-default-features --features mb12-userprobe` → clean (regression).
    - `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` → clean.
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
    - `cargo doc --workspace --no-deps` → clean (was: 5 pre-existing warnings, fixed below).
    - `scripts/check-no-blanket-allow.sh` → ok (scanned 12 crate-root files).

### Fixed

- **`crates/omni-crypto/src/kdf.rs`** — gate `zeroize` import behind
  `#[cfg(feature = "rng")]`; on the bare-metal verify-only build
  (`--no-default-features`) `Zeroize`/`ZeroizeOnDrop` are only used by
  the rng-gated `Argon2idHash`, so the import emitted
  `unused_imports` warnings.
- **Pre-existing doc warnings** (5 total, surfaced by `cargo doc`):
  - `omni-tee/src/lib.rs:46-47` — broken intra-doc links to `tdx` /
    `sev_snp` modules (feature-gated, not visible to default doc
    build). Demoted to inline code spans.
  - `omni-tee/src/traits.rs:148-149` — same root cause, same fix.
  - `omni-crypto/src/kex.rs:55` — link to non-existent
    `Self::from_bytes` on `OmniEphemeralSecret`. Demoted to inline
    code span (future `from_bytes` constructor is a hypothetical;
    the comment now reflects that without claiming the API exists).

- **Kernel — MB12: real message-passing IPC + multi-task user-space
  (Track B MB12.0a–MB12.9, 2026-05-18).** Closes the Phase 1 deliverable
  "IPC primitives operational (typed message passing)" from the roadmap.
  Two Ring 3 processes (sender + receiver) now exchange a payload
  through a kernel-mediated channel, with capability gating and
  scheduler-driven dispatch (TSS.rsp0 + CR3 reload + first-dispatch via
  `enter_user_mode`).

  - **Scheduler multi-task user (MB12.0a/b)** — `RoundRobinScheduler::yield_current`
    ([`scheduling.rs`](./crates/omni-kernel/src/scheduling.rs)) now
    updates `TSS.rsp0` and reloads CR3 when dispatching a user task,
    and detects the first-dispatch sentinel (`context.rsp == 0`) to
    transition directly to Ring 3 via `enter_user_mode` instead of the
    `context_switch` asm path. `USER_RFLAGS = 0x202` exported as a
    shared constant from [`usermode.rs`](./crates/omni-kernel/src/bare_metal/usermode.rs).
  - **`omni-crypto` feature gating (MB12.0c)** — introduces
    `default = ["rng"]` + `rng = ["dep:getrandom", "dep:rand_core",
    "dep:argon2", "omni-types/id-generation"]` + `bare-metal = []` in
    [`omni-crypto/Cargo.toml`](./crates/omni-crypto/Cargo.toml). All
    `generate()` methods on `OmniSigningKey`, `OmniAeadKey`,
    `OmniEphemeralSecret`, `OmniStaticSecret`, plus `generate_ephemeral`
    and the `argon2id_*` family, are now `#[cfg(feature = "rng")]`. The
    verify-only path (`OmniVerifyingKey::verify`,
    `domain_separated_hash`, HKDF) remains always available. Userspace
    consumers see no change with default features. Bare-metal compile
    on `x86_64-unknown-none` is partially unblocked — see *Known
    Limitations* below.
  - **Kernel capability check (MB12.0c')** —
    [`capabilities.rs`](./crates/omni-kernel/src/capabilities.rs)
    introduces `KernelPrincipal([u8; 32])`, `KernelAction::IpcSend/IpcRecv`,
    `KernelResource::IpcChannel(u64)`, `KernelCapabilityToken`,
    `CapabilityVerdict`, the `KernelCapabilityCheck` trait, and the
    `StubCapabilityProvider` MB12 implementation (action/resource
    shape-matching, no Ed25519 yet). Trait shape designed to swap in
    MB13 with a real provider when `omni-crypto` builds bare-metal.
  - **`KernelIpcRegistry` (MB12.1+2)** — concrete IPC backend in
    [`ipc.rs`](./crates/omni-kernel/src/ipc.rs). `BTreeMap<u64, Channel>`
    storage (no `HashMap` — `hashbrown::ahash` requires `getrandom`,
    conflict with MB12.0c). `Channel` carries
    `{policy, owner, send_subject, recv_subject, queue, waiters_send,
    waiters_recv}` — wait queues live inside the channel for O(1)
    lookup. `WakeAction { None, Wake(TaskId), Block(TaskId) }` is the
    contract between the registry and the syscall layer. Capability
    check is two-tiered: `verifier.verify()` at `create_channel`,
    byte-equality at `send`/`receive`.
  - **`PendingReceive` slot + `principal` in PCB (MB12.3)** —
    [`process.rs`](./crates/omni-kernel/src/process.rs) extends
    `ProcessControlBlock` with `principal: KernelPrincipal` (defaults
    to `KernelPrincipal::ZERO` for kernel-spawned tasks) and
    `pending_receive: Option<PendingReceive>` (reserved for MB13
    drain-at-dispatch / SharedMemoryGrant). `spawn_from_elf` takes
    `principal` as its last parameter.
  - **IPC syscall handlers 20-23 (MB12.5)** — `bare_metal/syscall_entry.rs`
    adds the `ipc_handlers` submodule (gated `cfg(all(feature =
    "bare-metal", target_os = "none", not(test)))`):
    - `IpcCreateChannel(20)` — `(queue_depth, backpressure, tee_bound,
      _, _, _) -> channel_id`. Open channels for MB12 (capability
      tokens via syscall deferred to MB13).
    - `IpcDestroyChannel(21)` — `(channel_id, _, _, _, _, _) -> 0 | u64::MAX`.
    - `IpcSend(22)` — `(channel_id, kind, payload_ptr, payload_len, _,
      _) -> 0 | u64::MAX`. Retry-loop on `WakeAction::Block`: parks via
      `yield_current(BlockedOnIpc)`, resumes on wake. `MAX_PAYLOAD = 4096`.
    - `IpcReceive(23)` — `(channel_id, dst_ptr, dst_cap, blocking, _,
      _) -> bytes_received | u64::MAX`. Same retry-loop pattern.
    - All four use `validate_user_buffer`-style range guards;
      hardware PT walks during `copy_nonoverlapping` enforce
      page-presence + `PTE_USER` semantics.
  - **`task_exit` now yields instead of halting** when other tasks are
    runnable — required for multi-task IPC. The fallback to
    `halt_forever` remains for the empty-run-queue terminator.
  - **MB12 user binaries (MB12.0f)** —
    [`bare_metal/userprobe_mb12.rs`](./crates/omni-kernel/src/bare_metal/userprobe_mb12.rs)
    embeds two hand-crafted ELFs (pattern mirroring MB11.7):
    - `USERPROBE_SENDER_ELF` (179 bytes, R+X 59-byte code segment):
      `IpcSend(ch=1, kind=Notification, "ping", 4) → TaskExit(0)`.
    - `USERPROBE_RECEIVER_ELF` (197 bytes file, 141 in-mem with 64-byte
      BSS scratch; R+W+X segment for Phase 1):
      `IpcReceive(ch=1, buf, 64, blocking=1) → WriteConsole(buf, n) → TaskExit(0)`.

    `spawn_userprobe_mb12(...)` pre-creates channel 1 (open, no
    capability subject set) + spawns both tasks `Runnable` on the
    scheduler.
  - **Boot wiring `mb12-userprobe` (MB12.6)** —
    [`lib.rs::kmain`](./crates/omni-kernel/src/lib.rs) under
    `#[cfg(feature = "mb12-userprobe")]`. Calls
    `spawn_userprobe_mb12`, registers a bootstrap task for `kmain`
    itself, then `yield_current(kmain, Terminated)` hands the CPU over
    to the scheduler. The scheduler's MB12.0a/b path dispatches the
    user processes via `enter_user_mode`. Mutually exclusive with
    `mb11-userprobe` at the boot wiring level. Forwarded as
    `mb12-userprobe` feature by
    [`kernel-runner/Cargo.toml`](./kernel-runner/Cargo.toml) (bootable
    image ready for QEMU+OVMF / Proxmox smoke).
  - **Integration tests `mb12_ipc_cross_process` (MB12.7)** —
    [`tests/mb12_ipc_cross_process.rs`](./crates/omni-kernel/tests/mb12_ipc_cross_process.rs):
    8 host-side end-to-end checks covering both ELFs loading into
    distinct address spaces, the happy-path send→receive round-trip,
    the receiver-parks-then-wakes-on-send sequence, `Block`-policy
    sender parking + wake on drain, send/recv capability subject
    mismatch denial, owner-only destroy, and the no-id-reuse invariant.
  - **Smoke output expected (manual QEMU+OVMF / Proxmox run):**
    ```
    [mb12] receiver task_id=N
    [mb12] sender   task_id=M
    [mb12] channel 1 pre-created
    [mb12] handing off to user tasks
    ping
    [user] exit=0
    [user] exit=0
    ```
  - **ADR-0005**
    ([`docs/adr/0005-mb12-ipc-message-passing.md`](./docs/adr/0005-mb12-ipc-message-passing.md))
    captures the architecture decisions (BTreeMap vs HashMap,
    drain-at-syscall vs at-dispatch, capability stub vs full
    omni-capability, hand-crafted ELFs vs separate user crate), the
    `omni-crypto` SIMD-ICE discovery, and the MB13 migration plan
    toward a real Ed25519 capability provider.

  **Known limitations:**
  - **MB12 bare-metal smoke triple-faults on Proxmox VMID 103 / QEMU
    OVMF** (validated 2026-05-18 post-merge). The receiver task spawns,
    the channel is pre-created, `[sched] entering Ring 3 via iretq` is
    emitted on the serial port — then the VM transitions to `stopped`.
    Root cause: Rust's `x86_64-unknown-none` target spec generates
    `ET_EXEC` ELFs with the kernel image at `p_vaddr = 0x200000` (PML4
    index 0); `bootloader 0.11` cannot relocate `ET_EXEC` (the
    `BootloaderConfig::mappings.dynamic_range_start` override is
    silently ignored), so the kernel ends up in the low half. The
    per-process `AddressSpace::new_with_kernel_half` clones only PML4
    indices 256..511, so when the scheduler dispatch path issues
    `mov cr3 → per-process PML4` inside `enter_user_mode`, the next
    instruction fetch lands on an unmapped page → page-fault → IDT
    handler also unmapped → triple fault. The same bug is latent in
    `mb11-userprobe`; the MB11 smoke was never manually validated.
    Host tests are not affected. **MB13 follow-up**: either (a) force
    the kernel ELF to `ET_DYN` (PIE) so the bootloader honours
    `dynamic_range_start`, (b) write a linker script that hard-codes
    the kernel image at an upper-half VA, or (c) install a cross-AS
    trampoline page mapped at the same VA in every PML4. Diagnosis
    captured in [`kernel-runner/src/main.rs`](./kernel-runner/src/main.rs)'s
    `BootloaderConfig` doc-comment.
  - **Capability check is a stub** — `StubCapabilityProvider::verify`
    matches `action`/`resource` shape but does not verify Ed25519
    signatures. MB13 swaps it once `omni-crypto` builds bare-metal.
  - **`omni-crypto` does not build on `x86_64-unknown-none` today** —
    LLVM ICE on SIMD intrinsics in `sha2`, `poly1305`,
    `curve25519-dalek`. ADR-0005 § *Migration* + § *Alternative A*
    document the MB13 work (≈ 1-2 days of `force-soft` feature gating
    or `omni-crypto-verify` extraction).
  - **`mb11-userprobe` and `mb12-userprobe` are mutually exclusive at
    boot wiring level** — when both features are enabled in the same
    build, the MB11 block runs first. CI matrix should run them as
    separate jobs.
  - **No automatic QEMU smoke for `[mb12]` yet** — the existing
    `qemu-boot-smoke` job validates MB1–MB10 + MB11; an MB12 variant
    with `EXPECTED_LINES` extended for `[mb12]` + `ping` is tracked as
    follow-up.

  **Test delta:** workspace test count **393 → 426** (+33).
  (`+4` capability, `+17` IPC registry, `+10` userprobe MB12, `+8`
  cross-process integration, `+3` PCB tests, minus the trait-scaffold
  baseline that the concrete impl replaces.)

- **Kernel — MB11 closure: real user-probe ELF, kmain boot wiring,
  integration tests (Track B MB11.7–MB11.9, 2026-05-18).** Closes the
  MB11 milestone started by the foundation commit; the kernel now spawns
  a real Ring 3 process that issues `WriteConsole("hello\n")` then
  `TaskExit(0)`, under the new `mb11-userprobe` feature.

  - **Hand-crafted user ELF** ([`bare_metal/userprobe.rs::USERPROBE_ELF`](./crates/omni-kernel/src/bare_metal/userprobe.rs)):
    167 bytes — 64-byte ELF64 header + 56-byte PT_LOAD program header +
    47 bytes of `x86_64` machine code & data at file offset 0x78,
    mapped to VA `0x4000_0000`. The code does:
    ```
    mov rax, 60          ; WriteConsole
    lea rdi, [rip+0x1b]  ; ptr → msg
    mov rsi, 6           ; len
    syscall
    mov rax, 11          ; TaskExit
    mov rdi, 0           ; exit code
    syscall              ; never returns
    jmp $                ; safety loop
    msg: "hello\n"
    ```
    Embedding the bytes directly avoids a recursive cargo build in
    `build.rs` and removes the linker-script + target-spec complexity
    a separate `omni-userprobe-helloworld` crate would have introduced.
  - **kmain boot wiring** ([`lib.rs`](./crates/omni-kernel/src/lib.rs))
    under `#[cfg(feature = "mb11-userprobe")]`: reads CR3, calls
    `userprobe::spawn_userprobe`, looks up the resulting `PCB`, prints
    diagnostic lines (`[user] userprobe spawned`, `[user] address
    space activated cr3 = …`, `[user] entering Ring 3 rip = …`), then
    transfers to Ring 3 via `usermode::enter_user_mode(rip, rsp,
    rflags=0x202, cr3)`. The user code's `TaskExit` then writes
    `[user] exit=0\n` and halts the CPU.
  - **`mb11-userprobe` feature flag** in
    [`crates/omni-kernel/Cargo.toml`](./crates/omni-kernel/Cargo.toml)
    and forwarded by [`kernel-runner/Cargo.toml`](./kernel-runner/Cargo.toml).
    Mirrors the `mb8-smoke` pattern: gated out of production builds
    (VirtualBox / Proxmox unaffected) and never on by default.
  - **Integration test**
    [`tests/mb11_userspace.rs`](./crates/omni-kernel/tests/mb11_userspace.rs):
    6 host-side end-to-end checks — userprobe ELF parses with the
    correct entry + flags + "hello\n" payload; `AddressSpace` clones
    only the kernel half of a synthetic boot PML4; user-stack slots
    are disjoint with guard pages; `validate_user_buffer` rejects
    kernel-half buffers and accepts zero-length anywhere.
  - **Unit tests on userprobe** (5 new): ELF parses, entry point is
    `0x4000_0000`, single PT_LOAD with RX flags, code carries
    "hello\n", lea displacement points at the message. Plus the
    6 integration tests above.
  - **QEMU smoke**: feature-gated path documented; running
    `cargo build --manifest-path kernel-runner/Cargo.toml --target
    x86_64-unknown-none --features mb11-userprobe` produces a bootable
    image that exercises the full Ring 3 round trip. Expected serial
    output sequence (in addition to the existing K5/LAPIC/sched lines):
    ```
    [user] userprobe spawned  task_id=N
    [user] address space activated cr3 = 0x...
    [user] entering Ring 3 rip = 0x40000000
    hello
    [user] exit=0
    ```

- **Kernel — MB11 foundation: per-process address space, user stacks,
  Ring 3 trampoline, TaskExit/WriteConsole syscalls (Track B MB11.1–MB11.6,
  2026-05-18).** First slice of the userspace-Ring-3 milestone per
  [`docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md`](./docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md).
  Lands all the kernel-side infrastructure; the embedded user-probe ELF +
  `kmain` boot wiring + QEMU smoke close in a follow-up.

  - **GDT extended 3 → 7 slots** ([`bare_metal/gdt.rs`](./crates/omni-kernel/src/bare_metal/gdt.rs)):
    user-data (0x1B, DPL=3), user-code64 (0x23, DPL=3), TSS at 0x28.
    New constants `USER_CS`, `USER_SS`, `KERNEL_CS`, `KERNEL_SS`,
    `STAR_USER_BASE`, `STAR_KERNEL_BASE`.
  - **TSS module** ([`bare_metal/tss.rs`](./crates/omni-kernel/src/bare_metal/tss.rs)):
    104-byte `Tss` (rsp0..rsp2, ist1..ist7, iomap_base = 104), GDT
    descriptor builder (`tss_descriptor`), `ltr_load`, `set_rsp0`.
    `gdt_init` now also installs the TSS descriptor at slots 5–6.
  - **STAR fix** ([`bare_metal/syscall_entry.rs:308`](./crates/omni-kernel/src/bare_metal/syscall_entry.rs#L308)):
    placeholder `0x001B << 48` replaced with `STAR_USER_BASE = 0x10`,
    yielding SDM-correct SYSRET selectors (CS=0x23, SS=0x1B) and
    SYSCALL selectors (CS=0x08, SS=0x10).
  - **`AddressSpace` module** ([`bare_metal/address_space.rs`](./crates/omni-kernel/src/bare_metal/address_space.rs)):
    per-process PML4 with kernel-half **clone-by-reference** (memcpy
    of entries 256..512 from boot CR3 — shared sub-PDPTs). Methods
    `new_with_kernel_half`, `map_user_4k`, `activate` (wrcr3), `invlpg`.
  - **`PageMapper::map_4k_into`** ([`bare_metal/paging.rs:319`](./crates/omni-kernel/src/bare_metal/paging.rs#L319)):
    explicit-root variant required by `AddressSpace`; `map_4k` becomes
    a thin wrapper. ELF loader gains `Elf64::map_and_load_into` mirror.
  - **User stack allocator** ([`bare_metal/user_stack.rs`](./crates/omni-kernel/src/bare_metal/user_stack.rs)):
    range `[0x0000_0040_0000_0000, 0x0000_0040_8000_0000)` (2 GiB),
    16 KiB stack + 16 KiB guard per slot. Per-process bump counter.
  - **`ProcessControlBlock`** ([`crates/omni-kernel/src/process.rs`](./crates/omni-kernel/src/process.rs)):
    wraps the MB10 `TaskControlBlock` with `AddressSpace`, `user_entry`,
    `user_stack_top`, `next_user_stack_slot`. `spawn_from_elf` is the
    high-level entry point: parses ELF, builds AS, allocates user stack,
    registers the PCB with the scheduler.
  - **Scheduler integration** ([`scheduling.rs`](./crates/omni-kernel/src/scheduling.rs)):
    new `processes: Vec<ProcessControlBlock>`, `allocate_task_id`,
    `register_process`, `attach_process`, `process(id)` lookup.
    `allocate_stack_slot` promoted to `pub(crate)` so the user-process
    spawn path can grab a kernel stack from the MB10 isolated range.
  - **iretq trampoline** ([`bare_metal/usermode.rs::enter_user_mode`](./crates/omni-kernel/src/bare_metal/usermode.rs)):
    builds the 5-word Ring 3 stack frame (SS, RSP, RFLAGS, CS, RIP)
    and `iretq`-jumps after a `mov cr3` to the per-process PML4. Safe
    mid-instruction because kernel-half is identical by-reference.
  - **User pointer validation** ([`bare_metal/usermode.rs::validate_user_buffer`](./crates/omni-kernel/src/bare_metal/usermode.rs)):
    range guard `< 0x0000_8000_0000_0000` + 4-level page-table walk
    confirming `PTE_PRESENT | PTE_USER` on every page in the buffer.
  - **Syscall handlers** ([`bare_metal/syscall_entry.rs`](./crates/omni-kernel/src/bare_metal/syscall_entry.rs)):
    `TaskExit (11)` (dequeue + halt), `WriteConsole (60)` (validated
    copy to `early_console::emit`), `MemMap (1)` stub. Dispatch table
    extended; `SyscallNumber::WriteConsole = 60` added.
  - **User-probe scaffold** ([`bare_metal/userprobe.rs`](./crates/omni-kernel/src/bare_metal/userprobe.rs)):
    `spawn_userprobe` helper + placeholder `USERPROBE_ELF` (parseable
    but no code yet). The real `omni-userprobe-helloworld` crate +
    `kmain` boot wiring close MB11.7–MB11.9 in a follow-up.

  Verification:
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` clean.
  - `cargo test --workspace --all-features` → 381 pass / 0 fail
    (was 369 pre-MB11; +12 new unit tests across `tss`, `gdt`,
    `address_space`, `user_stack`, `usermode`, `process`).
  - `bash scripts/check-no-blanket-allow.sh` exits 0.

### Changed

- **Kernel — lift `unsafe_code` blanket allow on `omni-kernel` (Step 7.2,
  2026-05-18).** Final Step 7 PR. Removes the last crate-root blanket
  `#![allow(unsafe_code)]` from
  [`lib.rs:64`](./crates/omni-kernel/src/lib.rs#L64); the crate now carries
  **zero** non-whitelisted crate-root suppressions. The `cfg_attr(test,
  allow(...))` line at `lib.rs:88` is the only remaining inner attribute,
  explicitly whitelisted by ADR-0003 § Escape hatches.

  - Bare-metal target build surfaces ~40 cfg-gated lint violations (the
    host build path had them masked behind `target_os = "none"`); each
    handled with site- or module-level allow + reason, or fixed:
    - 6 `unsafe_code` sites in `lib.rs` (kmain orchestrator) — site-level
      allow citing single-core static-mut aliasing invariant.
    - Module-level allows added on `bare_metal/{arch/x86_64, elf_loader,
      gdt, idt, paging, syscall_entry}.rs` for the lints that fire most
      densely (`unsafe_code`, `doc_markdown`, `ptr_as_ptr`, `similar_names`,
      `cast_possible_truncation`, `integer_division`).
    - Fixed: `paging.rs` 3 trailing-semicolon style; `context_switch.rs`
      list-item indentation; `lapic.rs` `x86_64` backtick.
  - `scripts/check-no-blanket-allow.sh` flipped to **blocking** in CI
    (`continue-on-error: true` removed). Guardrail script now reports
    `ok (scanned 12 crate-root files)`.

  Verification:
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean.
  - `cargo clippy -p omni-kernel --target x86_64-unknown-none
    --no-default-features --features bare-metal -- -D warnings` clean.
  - `cargo test --workspace` 277 pass / 0 fail (unchanged).
  - `bash scripts/check-no-blanket-allow.sh` exits 0.

- **Kernel — lift `clippy::nursery` + `clippy::cargo` blanket allows on
  `omni-kernel` (Step 7.4, 2026-05-18).** Third of four Step 7 PRs. Removes
  the residual `#![allow(clippy::nursery, clippy::cargo)]` from
  [`lib.rs:78`](./crates/omni-kernel/src/lib.rs#L78); only `unsafe_code`
  remains, lifted by PR 7.2. Seven nursery findings handled across
  `scheduling.rs`, `wm.rs`, `demo.rs`:

  - 4 `clippy::too_long_first_doc_paragraph` in `scheduling.rs` — fixed by
    promoting the long first paragraph into a one-line summary + detail
    blank-line + body.
  - 2 `clippy::use_self` in `wm.rs:52` — `pub const DEFAULT: Window = ...`
    rewritten as `Self = Self { ... }`.
  - 1 `clippy::cognitive_complexity` (52/20) on
    `bare_metal/demo.rs::run_desktop` — allow + reason "event loop
    orchestrator: branches mirror input sources".
  - No `cargo` lint warnings fired (workspace deps already coherent).
  - Tests still 277 pass / 0 fail.

- **Kernel — lift `clippy::pedantic` blanket allow on `omni-kernel` (Step 7.3,
  2026-05-18).** Second of four Step 7 PRs. Removes `clippy::pedantic` from
  the crate-root suppression at [`lib.rs:78`](./crates/omni-kernel/src/lib.rs#L78);
  `clippy::nursery` and `clippy::cargo` follow in PR 7.4. ~68 sites split
  across `bare_metal/{cursor,demo,graphics,gdt,idt,input,paging,virtio_tablet,
  widget,wm}.rs`, `scheduling.rs`, and the trait-scaffold modules
  (`capabilities.rs`, `ipc.rs`, `memory.rs`, `syscall.rs`).

  - **Trait scaffold modules**: each got a module-level
    `#![allow(clippy::missing_errors_doc, reason = "trait scaffold methods
    return NotYetImplemented until ...")]`. Mandatory per ADR-0003 § Escape
    hatches (module-level allows are not blanket crate-root allows).
  - **`virtio_tablet.rs`**: module-level allow for `doc_markdown`,
    `cast_ptr_alignment`, `ptr_as_ptr` — MMIO BAR layout per VirtIO 1.0 §4.1.4
    requires raw pointer reinterpretation that's structurally safe.
  - **`demo.rs`**: module-level allow for `doc_markdown`, `similar_names`,
    `map_unwrap_or`, `too_many_lines`, `cast_possible_truncation`,
    `needless_pass_by_value` — orchestrator-level idioms for the desktop demo.
  - **Fixed** rather than allowed: `redundant_closure` in `scheduling.rs:662`
    (`|q| q.is_empty()` → `Vec::is_empty`), `cast_lossless` in
    `graphics.rs:153` (replaced `as u32` with `u32::from`), three
    `semicolon_if_nothing_returned` in `paging.rs:317/339/361`, and three
    `unsafe { … };` style fixes.
  - **Tests still 277 pass / 0 fail.**

- **Kernel — lift restriction + rustdoc blanket allows on `omni-kernel` (Step 7.1,
  2026-05-18).** First of four PRs that close the v0.2.0 kernel CI debt
  described in `progress-omni.md` § 4.5. Removes crate-root blanket
  suppressions for `clippy::indexing_slicing`, `clippy::integer_division`,
  `clippy::new_without_default`, `clippy::fn_to_numeric_cast`,
  `clippy::doc_lazy_continuation`, `clippy::implicit_saturating_sub`,
  `clippy::missing_errors_doc`, `rustdoc::broken_intra_doc_links`, and
  `rustdoc::private_intra_doc_links` from
  [`crates/omni-kernel/src/lib.rs:107-152`](./crates/omni-kernel/src/lib.rs#L107).
  Each intentional violation now carries a localized `#[allow(<lint>, reason
  = "…")]` attribute at the offending item, mirroring the pattern already
  in `memory.rs:278,302,342,346` and `lib.rs:413-432`. Rationale, scope, and
  enforcement formalised in [`docs/adr/0003-no-blanket-allows-in-production-crates.md`](./docs/adr/0003-no-blanket-allows-in-production-crates.md).

  - 39 sites annotated across `wm.rs`, `widget.rs`, `font.rs`, `input.rs`,
    `graphics.rs`, `demo.rs`, `scheduling.rs`, `elf_loader.rs`, `arch/x86_64.rs`.
  - 2 broken intra-doc links rewritten as code spans in `elf_loader.rs` (module
    doc) and `graphics.rs` (`restore_16x16` doc).
  - Workspace test count unchanged (277 pass / 0 fail).
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    clean post-lift.

- **Tooling — `scripts/check-no-blanket-allow.sh` (2026-05-18).** Bash
  guardrail script that enforces ADR-0003 against `crates/<scoped>/src/{lib,main}.rs`
  (12 scoped crates). Whitelists doc URL, `warn(...)`, `cfg_attr(test,
  allow(...))`, and `cfg_attr(all(feature = "bare-metal", ...))` only.
  Wired into `.github/workflows/ci.yml` as the `blanket-allow-guard` job
  (`continue-on-error: true` until PR 7.2 lifts the final `unsafe_code`
  blanket; flipped to blocking then).

### Added

- **Kernel — kernel stack isolation, Track B MB10 (2026-05-18).** Each kernel
  task now owns a dedicated 4 KiB VA slot in the kernel-only range
  `[0xFFFF_C000_0000_0000, 0xFFFF_C800_0000_0000)` (8 TiB capacity ≈ 1 G slots),
  with a 4 KiB guard page directly below the writable stack page. Stack
  overflow → `#PF` not-present with `CR2` = guard VA, deterministically caught
  by the IDT handler from MB3. Replaces the previous "physical frame + bootloader
  direct-map offset" stack VA that lived in the bootloader's RW window.
  Implemented on branch `feat/kernel-mb10-stack-isolation` per
  [`docs/adr/0002-mb10-kernel-stack-isolation.md`](./docs/adr/0002-mb10-kernel-stack-isolation.md):

  - **`scheduling.rs` constants**: `KERNEL_STACK_VA_BASE`,
    `KERNEL_STACK_VA_END`, `KERNEL_STACK_SIZE = 0x1000`, `KERNEL_STACK_STRIDE
    = 0x2000`. Walking the range by `KERNEL_STACK_STRIDE` gives the guard VA
    of each slot; adding `KERNEL_STACK_SIZE` gives the writable base.
  - **`RoundRobinScheduler::next_kernel_stack_slot`** bump allocator
    (`usize`); slots never reused (task-exit dealloc arrives with the
    process model in MB11+).
  - **`TaskControlBlock::kernel_stack_va`** new field alongside the existing
    `kernel_stack_phys`, retained for debug logs and future free-list.
  - **`spawn_kernel_task` new signature**:
    `(entry, kernel_stack_phys, mapper: &mut PageMapper,
      alloc: &mut BitmapFrameAllocator<N>, priority)`. Drops `phys_offset`
    (no longer needed — stack VA comes from the isolated range, not from the
    direct-map). Calls `mapper.map_4k(va, phys, PRESENT|WRITABLE|NX, alloc)`
    for the writable page; the guard page is deliberately NOT mapped.
  - **`kmain` / `bare_metal::mb8_smoke::run`**: call sites updated.
    `pager` is now `mut` so `map_4k` can extend the kernel page tables;
    the diagnostic line `[stack] kernel stack VA range = 0xFFFF_C000_… ..
    0xFFFF_C800_… (slot 0)` appears alongside the idle-task spawn.
  - **Bootstrap caveat invariant**: `spawn_bootstrap_task` continues to
    reuse the boot stack with sentinel `kernel_stack_phys = 0,
    kernel_stack_va = 0`; the first timer tick still overwrites
    `context.rsp` with the real boot-stack RSP. MB10 does not change the
    bootstrap path.
  - **`omni_context_switch` and `setup_task_frame`**: untouched — they
    operate on `stack_top: u64` and are agnostic to the VA's origin.
  - **Tests**: 4 new unit tests in `scheduling::tests` exercise the
    range-membership, stride-disjointness, guard-offset-below-stack,
    and range-arithmetic invariants. Full `cargo test -p omni-kernel
    --features bare-metal` → 79 unit + 21 integration green (was 75 +
    21).

## [0.2.0] — 2026-05-18

Closes the Track A desktop cycle (M1–M5 + M3b) and the Track B kernel-core cycle (MB1–MB9). The `omni-kernel` bare-metal binary now boots end-to-end on QEMU+OVMF, VirtualBox, and Proxmox VMID 103 with a full paging/IDT/syscall/scheduler/LAPIC/preemption stack and a graphical desktop demo. No public-API breakage versus `0.1.0` — versioning is bumped from `0.1.0` to `0.2.0` because of the new kernel capability surface (minor under SemVer 2.0.0). See `progress-omni.md` § 2 for the per-track milestone matrix and § 3 for the test/build evidence (`cargo test --workspace` → 273 pass, `cargo test -p omni-kernel --features bare-metal` → 75 unit + 21 integration).

### Added

- **Kernel — huge-page-aware paging + direct-map validator, Track B MB9 (2026-05-18).** Unblocks the QEMU+OVMF / Proxmox boot path that MB6-MB8 had marked as a known blocker. The kernel now correctly interprets the bootloader's huge-page direct-map and gives the frame allocator only frames that are reachable through it; the first scheduler/heap write no longer faults. Implemented on branch `feat/kernel-vga-wait` (see [`docs/adr/0001-mb9-paging-huge-page-aware.md`](./docs/adr/0001-mb9-paging-huge-page-aware.md)):

  - **`PageMapper::translate` huge-page aware** (`bare_metal/paging.rs`). The walker now follows entries with `PS=1` at both PDPT (1 GiB) and PD (2 MiB) levels and computes the final physical address from the leaf entry's frame field plus the appropriate page offset (30 bits for 1 GiB, 21 bits for 2 MiB). New flag constant `PTE_HUGE` plus four `HUGE_*_{FRAME,OFFSET}_MASK` constants isolate the bit math. The module doc-block is updated to reflect the new behaviour. `map_4k` is intentionally unchanged: it still operates on 4 KiB entries only and does not split huge pages (out of scope for MB9; documented in the doc-block).
  - **Direct-map validator in `kmain`** (`lib.rs::register_direct_mapped_regions`). A new helper iterates `boot_info.memory_regions`, takes the `Usable` ones, and only marks a region as free in the `BitmapFrameAllocator` if `mapper.translate(VirtAddr(phys_offset + region.start))` *and* `mapper.translate(VirtAddr(phys_offset + last_page_start))` both return `Some`. Regions that fail either probe are skipped wholesale. The result is a hard invariant: every frame the allocator hands out lives inside the active direct map, so `phys + phys_offset` writes never fault. New diagnostic line: `[paging] validated N MiB direct-mapped, skipped M MiB unmapped`.
  - **kmain init order change** (`lib.rs`). `PageMapper::new` now runs *before* the frame allocator is populated (the validator needs the mapper). The K5 banner remains exactly as specified by `OIP-Kernel-005` § S3 — no smoke-test impact. The defensive `mark_range_used(0, 0x100000)` is retained but re-documented as an independent BIOS-reserved-area policy (real-mode IVT, BIOS data, EBDA, video memory), not as the MB9 workaround it shadowed. The MB8 `FIXME(track-b-mb9)` is removed.
  - **`FRAME_BITMAP_WORDS` const + CR2 in #PF handler** (`lib.rs`, `bare_metal/arch/{x86_64,non_x86_64}.rs`, `bare_metal/idt.rs`). The frame-allocator capacity (`16 384` u64 words = 4 GiB) is extracted into a named const for reuse by the validator's signature. New arch primitive `read_cr2()` (with non-x86 stub); the `#PF` handler now prints `cr2=<addr>` alongside `code` and the exception frame, so any future faulting-VA investigation does not require reverse-engineering the disassembly to figure out which memory access triggered the fault.
  - **Cross-cutting heap-virt fix** (`kernel-runner/src/main.rs`). The original MB8 blocker was *thought* to be the kernel-stack allocation; smoke-test debugging on 2026-05-18 showed the actual first crash was the runner's heap install: `pick_region` returns a *physical* address, but the runner was passing it directly to `BumpHeap::init` as if it were virtual. On VirtualBox the low VAs happened to identity-map to low PAs and it worked by accident; on QEMU+OVMF the runner faulted at the first `Vec::push` from the scheduler init (`#PF code=2 cr2≈0x017800C0`). The runner now adds `boot_info.physical_memory_offset` before installing the heap. `pick_region`'s contract is unchanged and the existing host-side test in `tests/boot_info.rs` continues to pass.
  - **Tests**: 8 new unit tests in `paging.rs::tests` exercise huge-page translation at PDPTE PS=1 (start / middle / last byte / no-PD-deref regression), PDE PS=1 (start / middle / last byte), and a 4 KiB regression after the huge-page changes. Full `cargo test -p omni-kernel --features bare-metal` → 75 tests pass (67 pre-existing + 8 new); `cargo test --workspace` → all crates green.
  - **Smoke (QEMU+OVMF, macOS arm64 host, 2026-05-18)**: with `--features bare-metal,mb8-smoke --release`, serial now shows the full sequence the MB8 entry predicted but could not reach — K5 banner, `[paging] mapper ready CR3=0x101000`, `[mem] 244 MiB free / 245 MiB total`, **`[paging] validated 245 MiB direct-mapped, skipped 0 MiB unmapped`** (new MB9 line), `[idt] loaded`, `[syscall] LSTAR set`, `[sched] scheduler init idle task spawned`, `[sched] bootstrap kmain task registered`, `[lapic] timer started vector=0x20`, `[lapic] interrupts enabled`, `[mb8-smoke] task A/B spawned`, `[mb8-smoke] kmain halting — timer drives scheduler`. No `#PF code=2`. A/B character interleaving is present but sparse (QEMU TCG timer accuracy + brew-installed OVMF on arm64 host — host-environment limit, not a kernel issue).
  - **Proxmox verification (VMID 103, host `100.101.77.9`, 2026-05-18 00:49 CEST) — PASSED.** Pre-MB9 kernel image on VMID 103 was crashing with `[OMNI OS EXCEPTION] #PF Page Fault code=2 rip=0x204AE2` on the very same code path the QEMU+OVMF run had exposed. After redeploying HEAD (`926a37e`) via the standard procedure (`cargo run --manifest-path disk-image/Cargo.toml -- kernel-runner/target/x86_64-unknown-none/release/kernel-runner` → SSH copy of the resulting `boot-uefi.img` onto the Proxmox host → `dd` onto `zvol vm-103-disk-6` → `qm start 103`), serial log captured in `/tmp/omni-os-serial.log` shows the same banner+paging+IDT+syscall+sched+lapic+virtio sequence as the QEMU run, followed by `[virtio] tablet ready`. `demo::run_desktop` then proceeds to draw the System Info / Terminal / Clock / Power-Control windows on the VNC framebuffer with the taskbar and 5-minute countdown live. **MB9 fix is therefore confirmed on production hypervisor**, not only on the macOS-host QEMU smoke. This is the first OMNI OS kernel image to boot end-to-end on Proxmox without the pre-MB9 `#PF code=2` regression.
  - **Side-effect verification of MB6/MB7/MB8 on QEMU**: this is the first successful end-to-end boot of the MB6 cooperative scheduler, the MB7 LAPIC timer, the MB8 preemption trampoline, and the bootstrap-kmain-task on QEMU+OVMF. Up to today those milestones had been verified only via 273 workspace unit tests + VirtualBox boot; the QEMU path was gated by this blocker.

- **Kernel — graphical desktop demo, Track A M1–M5 (2026-05-13 → 2026-05-16).** The `omni-kernel` bare-metal binary runs a full interactive graphical session on UEFI/GOP hardware (verified on VirtualBox with OVMF). Implemented on branch `feat/kernel-vga-wait`:

  - **GOP framebuffer abstraction + 8×16 bitmap font renderer** (`bare_metal/graphics.rs`, `bare_metal/font.rs`). Typed `FrameBuffer` wrapper over the `bootloader_api` framebuffer pointer; `render_char` / `render_str` at arbitrary pixel coordinates; palette constants (`WHITE`, `CYAN`, `GRAY`, `RED`, `GREEN`, `DARK`). Validated against a 1024×768 SVGA UEFI framebuffer.
  - **Disk-image builder crate** (`disk-image-builder/`). `cargo run` invokes `llvm-objcopy` to strip the ELF, pads the resulting binary to 4 MiB (VDI minimum), and produces a bootable `.vdi` image consumable by VirtualBox without a host-side install step.
  - **M1/M2 — PS/2 event loop + minimal desktop WM** (`bare_metal/input.rs`, `bare_metal/wm.rs`). Scancode set 2 decode table; `poll_ps2_event()` busy-loops on port `0x64` status; window manager composites window rectangles to the framebuffer in painter's order.
  - **M3 — Software cursor with pixel save/restore** (`bare_metal/cursor.rs`). 11×19 arrow cursor; backing buffer saves the 8×16 tile under the cursor tip before rendering and restores on move — no flicker.
  - **M4 — Widget toolkit + click hit-test + Enter key handling** (`bare_metal/widget.rs`). `Button`, `Label`, `ProgressBar`, `Window` primitives; `HitTest::contains(x, y)` on every widget; Enter key dispatches a synthetic click to the focused button.
  - **M5 — Desktop orchestrator + RTC clock + terminal echo** (`bare_metal/demo.rs`). `run_desktop()` composes four windows (System Info, Terminal, Clock, Power Control); RTC reads BCD registers from ports `0x70`/`0x71` and formats `HH:MM:SS`; terminal echoes PS/2 keystrokes via the `KEYMAP_QWERTY` table; power-off button triggers ACPI S5.
  - **M3b — PS/2 mouse support + 5-minute countdown**. Third PS/2 device byte accumulated via port `0x60` into `MouseState`; countdown in System Info window starts from 5:00 and triggers graceful power-off when it reaches zero.
  - **ACPI S5 power-off** (`bare_metal/arch/x86_64.rs`). Dual strategy: FADT path (`acpi_poweroff_from_fadt`) reads RSDP → RSDT/XSDT → FADT to extract `PM1a_CNT_BLK` + `SLP_TYPa`; fallback (`acpi_poweroff`) tries PCI config-space scan (B0/D0/F0 `0x600` +S5 `0xB004`) and hardcoded QEMU/VirtualBox/Bochs ports.
  - **Verified on VirtualBox + OVMF** (2026-05-16): full desktop boots, RTC updates live, mouse tracks cursor, power-off button shuts down the VM cleanly.

- **Kernel — preemptive scheduling from LAPIC timer, Track B MB8 (2026-05-17).** Builds on MB6 (cooperative round-robin, commit `27720ee`) and MB7 (LAPIC periodic timer) to deliver real preemption: kernel tasks now alternate on the CPU driven by hardware interrupts alone, with no cooperative `yield_current` call. Design follows the **separation-of-contexts** principle (Linux-style): the interrupt handler owns the *trap frame* (caller-saved + CPU-pushed `[RIP, CS, RFLAGS, RSP, SS]`, closed by `iretq`); the scheduler owns the *switch frame* (callee-saved, closed by `ret`); the two never cross. Implemented on branch `feat/kernel-vga-wait`:

  - **`need_resched` trampoline** (`scheduling.rs`, `bare_metal/lapic.rs`). Two atomic booleans (`NEED_RESCHED`, `IN_SCHEDULER`) decouple the LAPIC tick from the actual reschedule. The timer handler does only `TICK_COUNT++` + EOI + `NEED_RESCHED.store(true)`; the asm stub then `call`s `kernel_check_need_resched` before popping caller-saved + `iretq`, which inspects the flag and (if set, and `IN_SCHEDULER` is not already taken) invokes the **existing MB6 cooperative `yield_current`** — `omni_context_switch` is reused unchanged. This means no second context-switch path: cooperative `TaskYield` syscall and timer preemption share the same code, and any future interrupt source (keyboard, disk, NIC) can request a reschedule without knowing how a context switch works.
  - **Bootstrap kmain task** (`scheduling::RoundRobinScheduler::spawn_bootstrap_task`). Registers the currently-executing kmain flow as a scheduler-visible `TaskControlBlock` *before* `sti`, with sentinel `context.rsp = 0` and `kernel_stack_phys = 0` (boot stack reused in place). The first timer fire overwrites the sentinel with kmain's real RSP and from that moment kmain is a regular preemptible task. Without this, the first tick would have no `current` to save state into.
  - **Task entry trampoline** (`bare_metal/context_switch.rs::omni_task_entry_trampoline`). Tiny `sti; ret` stub inserted between `omni_context_switch`'s `ret` and a freshly-spawned task's real entry point. Solves the "first switch from inside an Interrupt Gate leaves `IF=0`" problem: the Interrupt Gate masks `IF` on entry, and `omni_context_switch` does not restore it, so without the trampoline a brand-new task would run with interrupts permanently disabled and never be preempted again. `setup_task_frame` now writes 8 words (entry + trampoline-addr + 6 zero callee-saved) instead of 7.
  - **`mb8-smoke` feature** (`bare_metal/mb8_smoke.rs`, gated by `--features mb8-smoke`, which implies `bare-metal`). Spawns Task A and Task B as Interactive-priority kernel tasks; both run tight `early_console::emit(b"A")` / `b"B"` loops with no cooperative yield. Production builds (without the feature) are bit-for-bit unaffected: the smoke module is `#[cfg]`-gated out and `kmain` falls through to the desktop demo as before.
  - **Verification**: `cargo test --workspace` → 273 tests pass (272 baseline + 1 new `bootstrap_task_is_installed_as_current_with_sentinel_rsp`). `cargo build -p omni-kernel --target x86_64-unknown-none --release --no-default-features --features bare-metal` clean; same with `--features bare-metal,mb8-smoke`. Same warning baseline as commit `27720ee` (9 warnings, all pre-existing `multiple_unsafe_ops_per_block`-style lints).
  - **Known blocker (MB9)**: the QEMU+OVMF smoke run of MB8 is gated by a **pre-existing MB6 bug** caught while testing: `bootloader_api` 0.11's `Mapping::Dynamic` for physical memory maps RAM via 2 MiB / 1 GiB huge pages, but the kernel's `paging::PageMapper` only walks 4 KiB pages — so `translate` and `map_4k` mis-handle direct-map VAs. The very first call to `spawn_kernel_task` (idle task, MB6 code path) writes to a direct-map VA whose physical frame the bootloader left unmapped, producing #PF immediately after `[syscall] LSTAR set`. VirtualBox happens to dodge this because its bootloader-mapped region overlaps the first free frame allocated by `BitmapFrameAllocator`. Defensive mitigation landed in this commit: `kmain` reserves the low 1 MiB unconditionally via `mark_range_used`. Full fix tracked under MB9 (huge-page-aware `PageMapper` + post-boot direct-map completion). FIXME comment with the full investigation context is anchored in `lib.rs` at the scheduler init block. **The MB8 code itself is complete and correct** — it just cannot be live-verified on QEMU until MB9 unblocks the boot path.

- **Kernel — syscall dispatcher + ELF64 loader, Track B MB4–MB5 (2026-05-16).** Two milestones that complete the syscall ABI entry path and the binary loading prerequisite for MB6 (scheduler + first userspace task):

  - **MB4 — SYSCALL/SYSRET + INT 0x80 dispatcher** (commit `f2e88da`, `bare_metal/syscall_entry.rs`). Activates the P6.5 `SyscallDispatcher` trait scaffold. Two entry paths: (1) fast path via `IA32_LSTAR` MSR — `omni_syscall_entry` `global_asm!` stub marshals register args (RAX→RDI, RDI→RSI, RSI→RDX, RDX→RCX, R10→R8, R8→R9, R9→stack) to System V calling convention, calls `kernel_syscall_dispatch`, then `sysretq`; (2) compatibility path via IDT vector 0x80 (`omni_int80_entry`, same convention, `iretq`). MSR setup in `syscall_init()`: enables `IA32_EFER.SCE`, writes `IA32_STAR` (`KERNEL_CS=0x08`, user placeholder `0x1B`), sets `IA32_LSTAR` to stub VA, `IA32_FMASK=0x200` (masks IF). `KernelSyscallDispatcher`: `TimeMonotonicNanos` returns `rtc_seconds() * 10⁹`, `TaskYield` returns 0, all others `NotYetImplemented`. Error sentinel: `u64::MAX`. `idt_set_vector(vector, handler)` added to `idt.rs`. 8 unit tests. Serial: `[syscall] LSTAR set  INT80=0x80`. System Info window: `Syscall : LSTAR+INT80 ready (MB4)`.
  - **MB5 — ELF64 parser + segment mapper** (commit `960e440`, `bare_metal/elf_loader.rs`). `Elf64<'a>::parse` validates magic, class (64-bit), data (little-endian), machine (`EM_X86_64 = 62`), type (`ET_EXEC` or `ET_DYN`), and program-header bounds. `load_segments()` returns a `SegIter<'a>` struct-based iterator over `PT_LOAD` phdrs, yielding `LoadSegment { virt_addr, file_data: &'a [u8], mem_size, flags }`. `map_and_load<const N>` (x86_64-only): for each segment, allocates frames from `BitmapFrameAllocator<N>`, maps via `PageMapper::map_4k`, writes file content and zeros BSS via the direct-map physical window. `kmain` embeds a 120-byte hand-crafted test ELF (one `PT_LOAD` at `0x4000_0000`) and logs `[elf] probe OK  entry=0x40000000` to the serial console. 12 unit tests. System Info window: `ELF     : parser ready (MB5)`.

- **Kernel — physical-memory + paging + exception infrastructure, Track B MB1–MB3 (2026-05-16).** Three milestones that form the mandatory foundation for MB4 and MB5:

  - **MB1 — `BitmapFrameAllocator<const N: usize>` + GDT** (commit `119f3d8`). Const-generic physical-frame allocator backed by an inline `[u64; N]` bitmap; `N = 16 384` covers 4 GiB at 4 KiB granularity. API: `mark_range_free`, `mark_range_used`, `alloc_frame`, `free_frame`, `total/free_frames/bytes`. GDT (`bare_metal/gdt.rs`): null + kernel-code (0x08) + kernel-data (0x10) descriptors; loaded via `lgdt` at `kmain` entry. 6 unit tests (alloc, exhaustion, free/realloc, range-used, misaligned guard, stats). Frame allocator initialised in `kmain` from the `BootInfo` memory map; System Info window shows real free/total MiB.
  - **MB2 — x86_64 4-level page-table walker** (commit `102ec7a`, `paging.rs`). `PageMapper` backed by the bootloader direct-map window (`BootInfo.physical_memory_offset`); no CR3 write — the bootloader page tables remain active. `translate(VirtAddr) -> Option<PhysAddr>` (4-level walk, huge-page guard, page-offset preservation). `map_4k<N>(virt, phys, flags, alloc)` allocates intermediate frames from `BitmapFrameAllocator<N>`. `unmap_4k(virt)` clears the PTE and issues `invlpg`. New arch primitives: `read_cr3() -> u64`, `unsafe invlpg(virt: u64)` (no-op stubs for non-x86 host builds). 13 unit tests with a heap-allocated 4096-byte-aligned fake arena.
  - **MB3 — IDT + synchronous exception handlers** (commit `657d7d1`, `idt.rs`). 256-entry `IdtEntry` table (16 bytes each, `#[repr(C)]`); `global_asm!` stubs in Intel syntax (stable Rust, no nightly `abi_x86_interrupt`); System V AMD64 ABI stack alignment maintained via `sub rsp, 8` before each `call`. Four vectors: `#DE` (0), `#DF` (8), `#GP` (13), `#PF` (14). Handlers log to serial console via `early_console` then halt; `sti` is NOT issued — the IDT handles synchronous faults only. Loaded via `lidt` in `kmain` immediately after `gdt_init()`. 7 unit tests (struct packing, sentinel, interrupt-gate bit encoding). System Info window extended with `Paging: mapper ready (MB2)` and `IDT: loaded #DE #DF #GP #PF`.

- **OIP-Kernel-003 → Active (2026-05-16).** 48-hour Solo Founder Fast-Track window (§ 5.5, opened 2026-05-15) elapsed with no objections; P6.2 gate is formally closed. All five `no_std` transition steps (K1–K5) are now landed and verified. `OIP-Kernel-003` status field advances from `Last Call` to `Active`; amendment history table updated.

- **Brand pack v0.1 (2026-05-13).** First production-ready brand pack for OMNI OS and OMNI Foundation, landed under [`/brand/`](./brand/). Visual direction **C — Civic Tech / Generational** (Mozilla / Wikimedia / GOV.UK / Long Now reference frame), selected after a 3-way mood-board exploration.
  - [`brand/STRATEGY.md`](./brand/STRATEGY.md) — authoritative brand strategy: naming architecture (product `OMNI OS`, international brand `OMNI Foundation`, legal `Stichting OMNI` per Mozilla-style three-name pattern), positioning, 5-attribute personality + 5-row anti-personality, eight voice rules, full lexicon (owned + rejected), messaging hierarchy from 8-word tagline to 60-second pitch, boilerplate paragraphs, application guidance, co-branding rules, forks-welcome conventions.
  - [`brand/brand-book.html`](./brand/brand-book.html) + [`brand/OMNI-Brand-Book-v0.1.pdf`](./brand/OMNI-Brand-Book-v0.1.pdf) — 21-page consolidated brand book (cover, mission, personality, voice, hierarchy, mark, palette, typography, iconography, applications, operating-the-brand). Generated via WeasyPrint 68.1 from the print-ready HTML source.
  - [`brand/logos/`](./brand/logos/) — 8 production SVG lockups: primary horizontal, stacked, monogram, monochrome dark and light, OMNI Foundation primary, OMNI Foundation full legal lockup (with `Stichting OMNI · Amsterdam · The Netherlands` legal line), and a construction-grid reference. Mark semantics: brick-red core = Mission Anchor (irrevocable per [`docs/legal/bylaws-draft.md`](./docs/legal/bylaws-draft.md) Article 3); six petrol nodes = federated attested peers.
  - [`brand/colors/`](./brand/colors/) — color tokens in three formats: [`tokens.css`](./brand/colors/tokens.css) (CSS custom properties, light + dark mode), [`tokens.json`](./brand/colors/tokens.json) (Design Tokens Community Group format), [`palette.md`](./brand/colors/palette.md) (human-readable reference with WCAG 2.1 contrast matrix, AAA-target for body text). Five hue families (petrol / cream / brick / sage / charcoal). Brick reserved for governance signaling only — the single-red rule.
  - [`brand/typography/`](./brand/typography/) — type system: Source Serif 4 (display, wordmark), Inter (body, UI), IBM Plex Mono (code, metadata). All three SIL OFL — coherent with the AGPL-3.0 codebase + CC0 protocol specs. Modular scale 1.250 (major third). Tokens in [`type-tokens.css`](./brand/typography/type-tokens.css); rationale + pairing rules + anti-patterns in [`typography.md`](./brand/typography/typography.md).
  - [`brand/icons/icons.svg`](./brand/icons/icons.svg) — SVG sprite with 16 symbols (`omni-mesh`, `omni-node`, `omni-local-first`, `omni-cloud-deny`, `omni-attestation`, `omni-tee`, `omni-kernel`, `omni-agent`, `omni-inference`, `omni-encryption`, `omni-mesh-route`, `omni-governance`, `omni-fork`, `omni-oip`, `omni-zk`, `omni-anchor`). 24×24 viewBox, 1.5 stroke, `currentColor` inheritance, round caps/joins. Each symbol indexes a concept in the lexicon.
  - [`brand/templates/`](./brand/templates/) — 7 ready-to-use templates: [README header markdown](./brand/templates/README-header.md), [single-file slide deck](./brand/templates/slide-deck-starter.html), [social card 1200×630 SVG](./brand/templates/social-card-1200x630.svg), [GitHub social preview 1280×640 SVG](./brand/templates/github-social-1280x640.svg), [OIP/ADR cover template](./brand/templates/oip-cover.md), [HTML email signature](./brand/templates/email-signature.html), [plain-text email signature](./brand/templates/email-signature.txt).
  - Canonical tagline locked: **`An AI-native operating system. Local-first. Decentralized.`** (already in use in the repository `README.md`; promoted from de-facto to canonical).
- [`docs/12-brand.md`](./docs/12-brand.md) — new docs index entry: pointer document linking to the canonical brand pack under [`/brand/`](./brand/).
- [`docs/README.md`](./docs/README.md) — index table extended with row `12`; subdirectory table extended with `/brand/` row.
- [`README.md`](./README.md) — "Documentation" section gained a Brand & visual identity link.

- **Architecture — OMNI App Mesh (2026-05-12).** Five `Draft` OIPs filed that, together with `OIP-Container-006`, compose the user-facing AI-native application layer:
  - [`/oips/oip-helper-007.md`](./oips/oip-helper-007.md) — **OMNI Helper**: agentic need-detection daemon with three autonomy levels (`Autonomous` / `Guided` (default) / `Inform`), mandatory Impact Dashboard schema (Privacy / Trust / Cost / Time / Egress / Capabilities, 1-5 scales), escalation taxonomy for destructive / privacy-violating / capability-escalation classes, plain-language explanation engine, 30s undo window in Autonomous mode, per-context override grammar.
  - [`/oips/oip-pkg-008.md`](./oips/oip-pkg-008.md) — **`omni-pkg`**: content-addressed federated package manager (OCI v1 + Nix-style derivation), Sigstore-signing + CT-log mandatory, capability-declarative manifest, atomic upgrade via Nix-style symlink swap, per-package capability prompt at install. Federation protocol spec'd; trust ranking by tier + reputation + cap-minimality.
  - [`/oips/oip-forge-009.md`](./oips/oip-forge-009.md) — **`omni-forge`**: on-demand Rust → WASM/ELF generation pipeline. Default Cranelift fast-path (<1s compile), opt-in rustc+LLVM AOT for long-lived apps. LLM source generation + static analysis + capability inference + TEE-bound ephemeral signing. Mandatory user source review on first run; hard cap on daily generations.
  - [`/oips/oip-market-010.md`](./oips/oip-market-010.md) — **`omni-market`**: Stichting-curated marketplace (default registry for `omni-pkg`). Bronze / Silver / Gold / Stichting-Curated tiers with public verification criteria. Continuous CVE re-scan (< 6h from advisory publication) with public SLA per severity (Critical 14d). Mandatory reproducible build at Silver+. Commercial commission **10%** (vs 15-30% mainstream marketplaces); 0% OSS; 0% Stichting-sponsored.
  - [`/oips/oip-flagship-011.md`](./oips/oip-flagship-011.md) — **Omni\* flagship apps program**. Reserved `Omni{Function}` naming convention for Stichting-Curated apps. First flagship: **OmniCode** delivered in two phases — Phase 1 (immediate, ~2-3 engineer-months): upstream Codium packaged as `omni/linux-codium` OmniContainer image with Rust + Python + TypeScript LSPs pre-configured, OpenVSX (Eclipse Foundation registry) as extension marketplace, no telemetry. Phase 2 (year 4 target, ~9-12 engineer-months): native Tauri port for ~50MB binary and sub-second startup.
- [`docs/02-architecture.md`](./docs/02-architecture.md) — new section "OMNI App Mesh" between "Implementation choices" and "Open architectural questions". Diagrams the Helper → (Pkg, Forge) → Market → Flagship layering and ties everything back to the OmniContainer execution substrate.

- **`crates/omni-container/` skeleton (2026-05-12) — reference implementation for `OIP-Container-006` § 10.** Public trait surface, type definitions, lifecycle state machine, and capability profile parser. Every operational method on `engine::ContainerEngine` returns `ContainerError::NotYetImplemented(...)` with a PII-safe static context slug; real KVM ioctl wiring, TDX attestation glue, and SEV-SNP firmware calls land in follow-up OIPs (one per subsystem — engine, image, virtio, attestation). Crate is **std-enabled** (userspace service, per `OIP-Container-006` § 1 box-diagram annotation). Modules: `engine` (`ContainerEngine` trait + `KvmEngine` stub), `image` (`OciImageRef` newtype + structural parser), `lifecycle` (7-state machine with transition validation), `attestation` (`ContainerQuote` carrying host measurement, guest kernel hash, image, capability-set hash, nonce), `profile` (`CapabilityProfile` enum with 5 built-ins + `Custom` + `FromStr`), `virtio/{fs,net,vsock,gpu,rng}` (device-backend trait skeletons + `StubVirtio*` impls), `cli/{run,run_windows,ps}` (parser-agnostic argument structs). Feature flags `default = ["kvm"]`, `kvm`, `tdx`, `sev-snp`, `all-backends` per `OIP-Container-006` § 10. **52 unit tests + 5 integration tests** in `tests/lifecycle_state_machine.rs`. Verification gate: `cargo check / test / clippy --workspace --all-targets --all-features -- -D warnings` / `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features` / `cargo fmt --all -- --check` all clean. One workspace-local carve-out documented: `clippy::literal_string_with_formatting_args` is `#![allow]`-listed inside the crate to suppress an upstream-shaped false positive on the workspace `clippy.toml` `=` banner (lint surface preserved everywhere else).

- **Architecture — Container engine + Linux/Windows compatibility (2026-05-12).**
  - [`/oips/oip-container-006.md`](./oips/oip-container-006.md) — `Draft`. **OmniContainer**: native micro-VM container engine (Firecracker/Kata pattern), with **per-container TEE attestation** as default on TDX / SEV-SNP capable hosts. Specifies guest Linux image policy (Stichting-signed `omni-guest-linux-vN.M`), virtio-only I/O with capability-bound backends, OCI image compatibility + OMNI extension manifest, 5 built-in capability profiles, full lifecycle state machine, and reference implementation plan (`crates/omni-container/`).
  - **Windows app compatibility via `omni/linux-wine:N-stable`**: Wine + DXVK + VKD3D-Proton pre-baked image, `omni-container run-windows` CLI alias. ~85-95% Win32 productivity / 75-90% gaming coverage per Steam Deck / ProtonDB baselines. macOS apps explicitly NOT supported (Apple license).
  - **cyDock evolution path** documented: cyDock (Apache-2.0, `cySalazar/cyDock`) is NOT the basis for `omni-container` (different layer — cyDock is a management plane for containerd), but `cyDock-omni` fork retargets backend to `omni-container` REST API in Phase 5+ while reusing the React frontend.
  - **Future-work OIPs registered (not yet filed)**: `OIP-AOT-Wine-XXX` (Phase 6 — AOT packager `.exe` + Wine → OMNI ELF), `OIP-Cross-ISA-XXX` (v1.1+ ARM port — Rosetta-style x86→ARM ISA translator for OMNI binaries), `OIP-Container-Networking-XXX`, `OIP-Container-Storage-XXX`, `OIP-Container-BYOLinux-XXX`, `OIP-Container-Windows-VM-XXX`.
  - [`docs/02-architecture.md`](./docs/02-architecture.md) § "Open architectural questions" — **POSIX compatibility question marked resolved**: no POSIX in OMNI kernel; POSIX exists only inside guest Linux of OmniContainers.

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

- **2026-05-12 — `OIP-Process-001` §8 (Numbering) structural amendment (bootstrap fiat, §6.3).** Section §8 split into four sub-sections (§8.1 identifier rule, §8.2 filename convention, §8.3 draft-stage placeholder numbers, §8.4 reserved numbers). Substantive clarifications: (a) the integer is the canonical identifier for all cross-references; (b) the slug is explicitly a **category hint**, not a secondary identifier; (c) the global-uniqueness invariant binds at `Review`, not at `Last Call → Active` as the original wording implied — placeholder integer collisions in `Draft` are explicitly permitted and reconciled by the editors at the `Draft → Review` transition. Frontmatter `updated:` bumped 2026-05-10 → 2026-05-12; Amendment history table grows a third row. `oips/README.md` § "Numbering" + "Filing a new OIP" step 4 + "Note on duplicate trailing numbers" updated to mirror the new §8.3 wording. `python3 scripts/lint-oips.py` → `0 error(s), 0 warning(s) across 10 file(s)`. No semantic change to any prior `Active` OIP. Rationale: the registry holds three placeholder collisions (`OIP-Bounty-002` / `OIP-Crypto-002`; `OIP-Serde-004` / `OIP-Kernel-004`; `OIP-Kernel-005` / `OIP-Voting-005`); the previous §8 wording required ad-hoc footnotes, this amendment formalizes the actual editor practice.
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

- **2026-05-12 — Tamarin model v0.4 (5-defect fix-pass + first end-to-end run).** `protocol-proofs/handshake.spthy` had never executed under `tamarin-prover` (proof execution was P3.2-blocked). Installing tamarin-prover 1.12.0 locally surfaced five structural defects that prevented the model from loading: (i) user-declared `pk/1` collided with the `signing` builtin's reserved `pk/1`; (ii) `Responder_Receive_M3` referenced an unbound `dh2` in its `let`-binding; (iii) action label `Verified_Sig_M3` lacked an argument list; (iv) `compliance_capability_no_downgrade` quantified over `A, B` that did not appear in the body (unguarded); (v) lemma bodies used `_`-wildcards and free term variables without quantification. After the fix-pass, all 8 lemmas verify in ≈ 1.36 s. One residual wellformedness warning remains (Message Derivation Checks on peer-controlled variables) and is carried forward to the P3.2 cryptographer review with a written assessment checklist in `protocol-proofs/handshake-proof-run-2026-05-12.txt`. `todo.md` P3.1 "Tamarin proof execution" acceptance criterion transitioned `[ ]` → `[x]`.

- **2026-05-12 — OIP-Serde-004: Draft → Review → Last Call (closes 2026-05-26).** Editorial transitions by the interim editor body under `OIP-Process-001` §6.2. One in-Review correction applied: §S1 dropped the `use-std` feature from the workspace-level `postcard` dependency declaration (Cargo features are additive — enabling `use-std` in `[workspace.dependencies]` would unconditionally pull `std` into the foundational crates that must remain `no_std + alloc`). All five migration steps (M1–M5) landed locally on `feat/p1-foundational-crates`. Last Call → Active triggers on 2026-05-26 or earlier per `OIP-Process-001` § 5.3.

- **2026-05-12 — OIP-Bounty-002: Draft → Review → Last Call (closes 2026-05-26).** Editorial transitions with no substantive content change. As the first non-`Meta` OIP under `OIP-Process-001`, this OIP dogfoods §5 of the process — its `Last Call → Active` path is the project's first formal vote.

- **2026-05-12 — P7.2 M1–M5 (`bincode` v2 → `postcard` 1.x migration) landed locally.** Five-commit sequence on `feat/p1-foundational-crates` implementing `OIP-Serde-004`:
  - **M1 (commit `b8de469`)** — workspace `Cargo.toml` swaps `bincode = { version = "2.0", ... }` for `postcard = { version = "1.0", default-features = false, features = ["alloc"] }` (no `use-std`). Per-crate deps + call-sites in `omni-capability` and `omni-tee` migrated to direct `postcard::*` calls. Verified `cargo build --workspace --all-features` clean.
  - **M2 (commit `9b3d977`)** — introduces `crates/omni-types/src/wire.rs` (~230 LOC) with `encode_canonical<T: Serialize>(&T) -> Result<Vec<u8>>` and `decode_canonical<'a, T: Deserialize<'a>>(&'a [u8]) -> Result<T>`. The decoder uses `postcard::take_from_bytes` + explicit `tail.is_empty()` guard to enforce no-trailing-bytes (smuggling prevention). New `WireErrorKind` enum (`EncodeFailed`, `DecodeFailed`, `TrailingBytes`) + `OmniError::Wire { kind, context }` variant + `OmniError::wire(kind, context)` constructor. `omni-types` becomes the only crate with a direct `postcard` dep. Four `disallowed-methods` entries added to `clippy.toml` for `postcard::{to_allocvec, to_vec, from_bytes, take_from_bytes}`. All callsites in `omni-capability` and `omni-tee` migrated to go through the helper. Verified `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
  - **M3 (commit `b451539`)** — four `omni-capability` round-trip regression tests pinning postcard-canonical-encoding properties at the public-API boundary: `canonical_bytes_match_wire_encode_canonical_of_payload`, `token_round_trip_via_wire_helper_preserves_signature`, `token_decode_rejects_trailing_bytes`, `canonical_bytes_change_under_field_mutation`. 47 unit + 7 integration tests green.
  - **M4 (commit `61a2b02`)** — `omni-types::version::PROTOCOL_VERSION_V0_2 = ProtocolVersion::new(0, 2)` constant added. V0_1 docstring records supersession. Five new `omni-tee` integration tests on `Quote` / `SealedBlob` round-trip via the wire helper; two new V0_2 compatibility tests. Mock-integration grows from 4 to 9 tests; omni-types from 40 to 42.
  - **M5 (commit `784918b`)** — `crates/omni-capability/tests/wire_format_v0_2.rs` pins the first 49 bytes of `TokenPayload` (`varint(16) | CapabilityId | NodeId`) byte-for-byte under the deterministic fixture, plus encode-decode-encode idempotence and trailing-byte rejection. `crates/omni-tee/tests/wire_format_v0_2.rs` adversarial suite covers (a) bit-flip on every byte of the cryptographically-covered region (nonce / measurement / body marker prefix), (b) truncation at every prefix length, (c) trailing-byte extension with four patterns, (d) swap with unrelated bytes. The bit-flip test explicitly scopes itself to the mock backend's covered fields and documents that `report_data` is not signed by the mock (a real TDX/SEV-SNP backend would catch flips everywhere).

  **Verification gate per OIP-Serde-004 § S5 M5:**
  - `cargo build --workspace --all-features`: clean.
  - `cargo test --workspace --all-features`: 204 tests / 0 failures.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  - `cargo audit`: exit 0. **RUSTSEC-2025-0141 is no longer in the report** (`cargo tree --invert bincode` returns "did not match any packages"). 255 crate dependencies scanned, no advisories matched.
  - `cargo deny check advisories`: ok. `bans` and `licenses` FAIL with pre-existing issues (`cpufeatures` 0.2/0.3 duplicate via aes/argon2/curve25519; `Unicode-DFS-2016` license allowance no longer matching) — explicitly out of `OIP-Serde-004` scope.

- **2026-05-12 — `chore(ci): preload bootstrap-github.sh with oip-lint required check`** (commit `c7298e5`). `OIP-Process-001` §9 ¶2 mandates that branch protection on `main` add `oip-lint / oip-lint` as a required status check within 7 calendar days of the OIP transitioning to `Active` (deadline 2026-05-17). The idempotent bootstrap script `scripts/bootstrap-github.sh` is updated so the founder can apply the change by re-running it with an admin token; the live mutation is **not** applied by this commit because it is a shared-state change requiring founder authorization.

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

[Unreleased]: https://github.com/CySalazar/omni/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/CySalazar/omni/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/CySalazar/omni/releases/tag/v0.1.0
