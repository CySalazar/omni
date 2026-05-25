#!/usr/bin/env bash
# =============================================================================
# OMNI OS — build a UEFI-bootable hybrid ISO from the current commit
# =============================================================================
#
# Produces a single .iso file that boots directly into the OMNI OS graphical
# desktop on UEFI machines (QEMU, VirtualBox, VMware, physical hardware via
# USB stick). The ISO is a LIVE image — there is no installer (OMNI OS does
# not yet have a persistent filesystem layer).
#
# Output layout:
#   dist/iso/omni-os-<short_sha>.iso   ← unique per commit
#   dist/iso/omni-os-latest.iso        ← symlink to the most recent build
#
# Both files are gitignored by default (see .gitignore). The directory itself
# is tracked via dist/iso/.gitkeep so the path always exists in the working
# tree.
#
# Prerequisites:
#   - xorriso (apt: xorriso)
#   - rustup nightly toolchain (used by disk-image's bootloader 0.11 build.rs)
#
# Usage:
#   bash scripts/build-iso.sh              # build ISO for HEAD
#   bash scripts/build-iso.sh --skip-build # reuse existing boot-uefi.img
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_RUNNER_DIR="${REPO_ROOT}/kernel-runner"
DISK_IMAGE_DIR="${REPO_ROOT}/disk-image"
ISO_OUT_DIR="${REPO_ROOT}/dist/iso"
ISO_ROOT="${REPO_ROOT}/dist/.iso-root"
KERNEL_ELF="${KERNEL_RUNNER_DIR}/target/x86_64-unknown-none/release/kernel-runner"
UEFI_IMG="${KERNEL_RUNNER_DIR}/target/x86_64-unknown-none/release/boot-uefi.img"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo "  [iso] $*"; }
ok()   { echo "  [iso] ✓ $*"; }
fail() { echo "  [iso] ✗ ERROR: $*" >&2; exit 1; }

SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        -h|--help)
            sed -n '3,30p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        *) fail "unknown option: $arg" ;;
    esac
done

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------
command -v xorriso >/dev/null 2>&1 || fail "xorriso non trovato. Installa con: sudo apt install -y xorriso"
command -v cargo >/dev/null 2>&1 || fail "cargo non trovato nel PATH"

# Determine short SHA for filename uniqueness; fall back to timestamp.
if SHORT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null)"; then
    TAG="$SHORT_SHA"
    if ! git -C "$REPO_ROOT" diff --quiet HEAD 2>/dev/null; then
        TAG="${TAG}-dirty"
    fi
else
    TAG="$(date -u +%Y%m%dT%H%M%SZ)"
fi
ISO_OUT="${ISO_OUT_DIR}/omni-os-${TAG}.iso"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
if [[ "$SKIP_BUILD" -eq 0 ]]; then
    log "Building kernel-runner ELF (release)..."
    (cd "$KERNEL_RUNNER_DIR" && cargo build --target x86_64-unknown-none --release --quiet)
    [[ -f "$KERNEL_ELF" ]] || fail "kernel-runner ELF non trovato: $KERNEL_ELF"
    ok "kernel-runner ELF: $(du -h "$KERNEL_ELF" | cut -f1)"

    log "Building UEFI boot image (cargo +nightly)..."
    (cd "$DISK_IMAGE_DIR" && cargo +nightly run --release --quiet -- "$KERNEL_ELF" >/dev/null)
    [[ -f "$UEFI_IMG" ]] || fail "boot-uefi.img non trovato: $UEFI_IMG"
    ok "boot-uefi.img: $(du -h "$UEFI_IMG" | cut -f1)"
else
    [[ -f "$UEFI_IMG" ]] || fail "--skip-build: boot-uefi.img mancante. Esegui senza --skip-build."
    log "Reusing existing boot-uefi.img: $(du -h "$UEFI_IMG" | cut -f1)"
fi

# ---------------------------------------------------------------------------
# ISO wrap (xorriso, UEFI-only El Torito via appended GPT partition)
# ---------------------------------------------------------------------------
mkdir -p "$ISO_OUT_DIR" "$ISO_ROOT"

# Drop a README inside the ISO so it is not a completely empty data track —
# some tools warn on empty ISOs.
cat > "${ISO_ROOT}/README.txt" <<EOF
OMNI OS — live UEFI boot image
Commit: ${TAG}
Built:  $(date -u +"%Y-%m-%d %H:%M:%S UTC")

This ISO boots a live graphical desktop. There is no installer.
Boot only on UEFI firmware (QEMU/OVMF, VirtualBox EFI, VMware EFI, modern PCs).
EOF

log "Wrapping into hybrid UEFI ISO..."
xorriso -as mkisofs \
    -iso-level 3 \
    -V "OMNI_OS" \
    -append_partition 2 0xef "$UEFI_IMG" \
    -appended_part_as_gpt \
    -e --interval:appended_partition_2:all:: \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    -partition_offset 16 \
    -o "$ISO_OUT" \
    "$ISO_ROOT" \
    >/dev/null 2>&1

[[ -f "$ISO_OUT" ]] || fail "xorriso non ha prodotto l'ISO: $ISO_OUT"

# Refresh the "latest" symlink for convenience.
ln -sfn "$(basename "$ISO_OUT")" "${ISO_OUT_DIR}/omni-os-latest.iso"

ok "ISO: $ISO_OUT ($(du -h "$ISO_OUT" | cut -f1))"
ok "Latest: ${ISO_OUT_DIR}/omni-os-latest.iso -> $(readlink "${ISO_OUT_DIR}/omni-os-latest.iso")"
