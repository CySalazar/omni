#!/usr/bin/env bash
# =============================================================================
# OMNI OS — VirtualBox launcher
# =============================================================================
# Builds the kernel-runner bootimage, converts it to a VDI disk, and
# starts (or creates) a VirtualBox VM that boots the image.
#
# Requirements:
#   - Rust toolchain with target x86_64-unknown-none (`rustup target add x86_64-unknown-none`)
#   - cargo bootimage  (`cargo install bootimage`)
#   - VirtualBox + VBoxManage in PATH
#
# Usage:
#   scripts/run-virtualbox.sh [--release] [--skip-build] [--headless]
#
# The VM is named "OMNI-OS-K4" and is re-created on first run.
# On subsequent runs the disk is replaced but the VM is reused.
#
# Serial output is saved to /tmp/omni-os-serial.log for inspection.
# =============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_RUNNER_DIR="${REPO_ROOT}/kernel-runner"

PROFILE="dev"
PROFILE_DIR="debug"
SKIP_BUILD=0
VM_TYPE="gui"

for arg in "$@"; do
    case "$arg" in
        --release)  PROFILE="release"; PROFILE_DIR="release" ;;
        --skip-build) SKIP_BUILD=1 ;;
        --headless) VM_TYPE="headless" ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

BOOTIMAGE="${KERNEL_RUNNER_DIR}/target/x86_64-unknown-none/${PROFILE_DIR}/bootimage-kernel-runner.bin"
VDI_PATH="/tmp/omni-os-k4.vdi"
SERIAL_LOG="/tmp/omni-os-serial.log"
VM_NAME="OMNI-OS-K4"

# ---------------------------------------------------------------------------
log() { echo "[omni-vbox] $*"; }
fail() { echo "[omni-vbox] ERROR: $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 1. Checks
# ---------------------------------------------------------------------------

command -v VBoxManage >/dev/null 2>&1 || fail "VBoxManage not found in PATH. Install VirtualBox."
command -v bootimage  >/dev/null 2>&1 || fail "bootimage not found. Run: cargo install bootimage"

# ---------------------------------------------------------------------------
# 2. Build
# ---------------------------------------------------------------------------

if [[ $SKIP_BUILD -eq 0 ]]; then
    log "Building kernel-runner (profile: ${PROFILE})..."
    (cd "${KERNEL_RUNNER_DIR}" && cargo bootimage --target x86_64-unknown-none \
        $([ "$PROFILE" = "release" ] && echo "--release" || true))
fi

[[ -f "$BOOTIMAGE" ]] || fail "Bootimage not found at ${BOOTIMAGE}. Build first."
log "Bootimage: ${BOOTIMAGE} ($(wc -c < "$BOOTIMAGE") bytes)"

# ---------------------------------------------------------------------------
# 3. Convert raw image → VDI
# ---------------------------------------------------------------------------

log "Converting to VDI → ${VDI_PATH}"
rm -f "${VDI_PATH}"
VBoxManage convertfromraw "${BOOTIMAGE}" "${VDI_PATH}" --format VDI

# ---------------------------------------------------------------------------
# 4. Create VM (idempotent: skip if already exists)
# ---------------------------------------------------------------------------

if ! VBoxManage showvminfo "${VM_NAME}" >/dev/null 2>&1; then
    log "Creating VM '${VM_NAME}'..."
    VBoxManage createvm --name "${VM_NAME}" --ostype "Linux_64" --register

    VBoxManage modifyvm "${VM_NAME}" \
        --memory 128 \
        --cpus 1 \
        --boot1 disk --boot2 none --boot3 none --boot4 none \
        --firmware bios \
        --nic1 none \
        --audio none \
        --usb off

    VBoxManage storagectl "${VM_NAME}" --name "IDE" --add ide --controller PIIX4
    VBoxManage storageattach "${VM_NAME}" --storagectl "IDE" \
        --port 0 --device 0 --type hdd --medium "${VDI_PATH}"
else
    log "VM '${VM_NAME}' already exists — replacing disk."
    # Detach old disk (ignore error if nothing was attached).
    VBoxManage storageattach "${VM_NAME}" --storagectl "IDE" \
        --port 0 --device 0 --type hdd --medium none 2>/dev/null || true
    # Unregister the old VDI from VirtualBox media registry.
    VBoxManage closemedium disk "${VDI_PATH}" --delete 2>/dev/null || true
    # Re-convert and re-attach.
    rm -f "${VDI_PATH}"
    VBoxManage convertfromraw "${BOOTIMAGE}" "${VDI_PATH}" --format VDI
    VBoxManage storageattach "${VM_NAME}" --storagectl "IDE" \
        --port 0 --device 0 --type hdd --medium "${VDI_PATH}"
fi

# ---------------------------------------------------------------------------
# 5. Serial port → file (COM1 @ 0x3F8 IRQ4)
# ---------------------------------------------------------------------------

VBoxManage modifyvm "${VM_NAME}" \
    --uart1 "0x3F8" "4" \
    --uartmode1 "file" "${SERIAL_LOG}"

log "Serial output will be saved to: ${SERIAL_LOG}"

# ---------------------------------------------------------------------------
# 6. Start VM
# ---------------------------------------------------------------------------

log "Starting VM '${VM_NAME}' (${VM_TYPE})..."
VBoxManage startvm "${VM_NAME}" --type "${VM_TYPE}"

echo ""
echo "===================================================================="
echo "  OMNI OS is booting in VirtualBox."
echo ""
echo "  The VM window shows the VGA banner for ~10 seconds, then halts."
echo "  Serial console output → ${SERIAL_LOG}"
echo ""
echo "  To read serial log after boot:"
echo "    cat ${SERIAL_LOG}"
echo ""
echo "  To delete the VM:"
echo "    VBoxManage unregistervm '${VM_NAME}' --delete"
echo "===================================================================="
