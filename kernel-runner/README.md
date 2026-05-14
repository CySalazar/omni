# kernel-runner

OMNI OS UEFI bootloader-runner harness. Bootable artifact specified by
[`OIP-Kernel-005`](../oips/oip-kernel-005.md) (K4 gate of
[`OIP-Kernel-003`](../oips/oip-kernel-003.md) § 3).

This crate is **workspace-excluded** — it builds only for
`x86_64-unknown-none` and consumes [`omni-kernel`](../crates/omni-kernel)
with the `bare-metal` feature. Developer machines without a cross-
toolchain skip it automatically when running `cargo build --workspace`.

## Build

```bash
# 1. Install the cross target once per machine.
rustup target add x86_64-unknown-none

# 2. Build the runner binary.
cargo build --manifest-path kernel-runner/Cargo.toml \
            --target x86_64-unknown-none --release

# Output: target/x86_64-unknown-none/release/kernel-runner
#         (an ELF executable; the bootloader stub embeds it.)
```

## Run under QEMU

Disk-image generation is **currently deferred to K5** (see
[`OIP-Kernel-005`](../oips/oip-kernel-005.md) § S7) because the
`bootloader` v0.11.x build-side crate requires `-Z bindeps`
(nightly-only as of Rust 1.85). The kernel-runner binary itself
builds fine on stable; producing a `.efi` / `.img` from it needs one
of the following paths:

### Option A — `bootimage` external tool (recommended)

```bash
# Install (one-time).
cargo install bootimage

# Build a bootable BIOS image (fast iteration; QEMU starts in ~1 s).
cd kernel-runner
cargo bootimage --release --target x86_64-unknown-none

# Run under QEMU.
qemu-system-x86_64 \
    -drive format=raw,file=target/x86_64-unknown-none/release/bootimage-kernel-runner.bin \
    -serial stdio \
    -no-reboot \
    -no-shutdown \
    -m 512M \
    -smp 1
```

You should see, on the serial console:

```
[OMNI OS] kernel-runner: entry_point reached.
[OMNI OS] early console (COM1) is live.
[OMNI OS] proceeding to heap init + kmain.
[OMNI OS] kmain entered.
[OMNI OS] kernel version: 0.1.0
[OMNI OS] memory regions: <N>
[OMNI OS] halting (K4 scope ends here).
```

That sequence is the K5 acceptance signature.

### Option B — UEFI image via nightly toolchain

If/when the workspace is willing to take a nightly cross-toolchain
dependency for the image build step:

```bash
rustup toolchain install nightly --target x86_64-unknown-none
RUSTUP_TOOLCHAIN=nightly cargo build \
    --manifest-path kernel-runner/Cargo.toml \
    --target x86_64-unknown-none --release \
    -Z bindeps  # required by bootloader 0.11.x build.rs
```

Then point QEMU at the produced `.img` with `-bios OVMF`. OVMF paths:

| Distribution | `OVMF_CODE` | `OVMF_VARS` |
|---|---|---|
| Ubuntu / Debian | `/usr/share/OVMF/OVMF_CODE.fd` | `/usr/share/OVMF/OVMF_VARS.fd` |
| Arch / Fedora | `/usr/share/edk2/ovmf/OVMF_CODE.fd` | `/usr/share/edk2/ovmf/OVMF_VARS.fd` |
| macOS (Homebrew) | `$(brew --prefix qemu)/share/qemu/edk2-x86_64-code.fd` | `$(brew --prefix qemu)/share/qemu/edk2-x86_64-vars.fd` |

## Layout

```
kernel-runner/
├── Cargo.toml       # bin crate, workspace-excluded
├── README.md        # this file
└── src/
    ├── main.rs           # bootloader_api::entry_point! → kernel_entry
    └── early_console.rs  # facade onto omni_kernel::bare_metal::early_console
```

`kernel_entry` (in `src/main.rs`) is the function the bootloader hands
control to. It:

1. Announces boot over COM1 (sanity-check that the early console works
   before the heap is up — proves the K3 panic-path machinery).
2. Calls [`omni_kernel::bare_metal::heap::pick_region`] to select the
   largest Usable contiguous region of ≥ 4 MiB from the boot memory
   map (tie-break: lowest start address).
3. Installs the chosen region into the global `BumpHeap` via the
   one-shot [`omni_kernel::bare_metal::heap::GLOBAL_HEAP.init`].
4. Hands control to [`omni_kernel::kmain`], which banners the kernel
   version + memory-region count and halts forever via `hlt`.

## What's deliberately NOT here

- **No image generation.** See § "Run under QEMU" above — the
  bootloader image-build needs nightly or an external tool. The K4
  scope is the kernel binary; K5 brings up the image flow.
- **No bootloader CLI orchestration.** Adding `cargo bootimage` to
  the workspace requires careful pinning + a CI job; tracked in K5.
- **No GDB stub, no panic-driver, no exception handlers beyond
  `cli + hlt`.** K3 / K4 ship the minimum that produces a recognizable
  boot signature. K6+ subsystems (IDT, GDT, syscall dispatch, frame
  allocator) land in their own OIPs.

## See also

- [`OIP-Kernel-003`](../oips/oip-kernel-003.md) — UEFI bootloader
  selection + `no_std` transition plan (parent OIP).
- [`OIP-Kernel-012`](../oips/oip-kernel-012.md) — K3: panic handler
  + bump allocator (renumbered from `OIP-Kernel-004`).
- [`OIP-Kernel-005`](../oips/oip-kernel-005.md) — K4: boot hand-off
  ABI + this `kernel-runner` crate.
