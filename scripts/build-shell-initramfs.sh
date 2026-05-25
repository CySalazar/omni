#!/usr/bin/env bash
# =============================================================================
# OMNI OS — Build shell initramfs archive
# =============================================================================
# Builds the omni-shell-image ELF for x86_64-unknown-none, then packs it
# into the flat initramfs format expected by omni_kernel::initramfs.
#
# Output: crates/omni-kernel/src/embedded_initramfs.bin
#
# The kernel embeds this file via include_bytes! and loads it into the VFS
# at boot time. Running this script is a prerequisite for booting into the
# shell prompt.
#
# Usage:
#   scripts/build-shell-initramfs.sh              # release build (default)
#   scripts/build-shell-initramfs.sh --debug      # debug build
# =============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHELL_IMAGE_DIR="${REPO_ROOT}/crates/omni-shell-image"
OUTPUT="${REPO_ROOT}/crates/omni-kernel/src/embedded_initramfs.bin"

PROFILE="release"
if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
fi

# -------------------------------------------------------------------------
# Step 1: Build the shell ELF
# -------------------------------------------------------------------------
echo "[initramfs] Building omni-shell-image (${PROFILE})..."
if [[ "${PROFILE}" == "release" ]]; then
    cargo build \
        --manifest-path "${SHELL_IMAGE_DIR}/Cargo.toml" \
        --target x86_64-unknown-none \
        --release
else
    cargo build \
        --manifest-path "${SHELL_IMAGE_DIR}/Cargo.toml" \
        --target x86_64-unknown-none
fi

ELF_PATH="${SHELL_IMAGE_DIR}/target/x86_64-unknown-none/${PROFILE}/omni-shell-image"

if [[ ! -f "${ELF_PATH}" ]]; then
    echo "[initramfs] ERROR: ELF not found at ${ELF_PATH}" >&2
    exit 1
fi

ELF_SIZE=$(stat -c%s "${ELF_PATH}")
echo "[initramfs] Shell ELF: ${ELF_PATH} (${ELF_SIZE} bytes)"

# -------------------------------------------------------------------------
# Step 2: Pack into initramfs archive format
# -------------------------------------------------------------------------
# Format per entry: [name_len: u16 LE] [name] [elf_len: u32 LE] [elf]
# This is the exact wire format of omni_kernel::initramfs::build_archive.

ENTRY_NAME="omni-shell"
NAME_LEN=${#ENTRY_NAME}

echo "[initramfs] Packing '${ENTRY_NAME}' (${ELF_SIZE} bytes)..."

{
    # name_len as u16 LE
    printf "\\x$(printf '%02x' $((NAME_LEN & 0xFF)))\\x$(printf '%02x' $(((NAME_LEN >> 8) & 0xFF)))"
    # name bytes
    printf '%s' "${ENTRY_NAME}"
    # elf_len as u32 LE
    printf "\\x$(printf '%02x' $((ELF_SIZE & 0xFF)))\\x$(printf '%02x' $(((ELF_SIZE >> 8) & 0xFF)))\\x$(printf '%02x' $(((ELF_SIZE >> 16) & 0xFF)))\\x$(printf '%02x' $(((ELF_SIZE >> 24) & 0xFF)))"
    # elf bytes
    cat "${ELF_PATH}"
} > "${OUTPUT}"

OUTPUT_SIZE=$(stat -c%s "${OUTPUT}")
echo "[initramfs] Archive written: ${OUTPUT} (${OUTPUT_SIZE} bytes)"
echo "[initramfs] Done. Rebuild kernel-runner to pick up the embedded blob."
