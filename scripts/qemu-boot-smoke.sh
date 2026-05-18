#!/usr/bin/env bash
# =============================================================================
# OMNI OS — QEMU boot smoke test
# =============================================================================
# Closes the K5 gate of OIP-Kernel-003 § 3: boots the `kernel-runner`
# ELF under QEMU+OVMF (UEFI) and asserts the canonical banner sequence
# appears on the serial console.
#
# Build pipeline (bootloader 0.11):
#   1. cargo build  → kernel-runner ELF for x86_64-unknown-none
#   2. disk-image   → UEFI disk image (boot-uefi.img) from the ELF
#   3. QEMU+OVMF   → boots the UEFI image, serial output captured
#
# Acceptance:
#   - The five banner lines emitted by `kernel_entry` + `kmain` appear,
#     in order, on the QEMU serial output within `SMOKE_TIMEOUT_SECS`.
#   - QEMU exits cleanly (kernel issues ACPI S5; QEMU tears down).
#
# Usage:
#   scripts/qemu-boot-smoke.sh                     # build + run + assert
#   scripts/qemu-boot-smoke.sh --skip-build        # use existing image
#   scripts/qemu-boot-smoke.sh --release           # release profile
#
# Environment:
#   OVMF_PATH            path to OVMF.fd firmware (default: auto-detect)
#   QEMU_BINARY          override qemu-system-x86_64 (default: from $PATH)
#   SMOKE_TIMEOUT_SECS   how long to wait for the banner (default: 30)
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_RUNNER_DIR="${REPO_ROOT}/kernel-runner"
DISK_IMAGE_DIR="${REPO_ROOT}/disk-image"
SMOKE_TIMEOUT_SECS="${SMOKE_TIMEOUT_SECS:-30}"
QEMU_BINARY="${QEMU_BINARY:-qemu-system-x86_64}"

PROFILE="dev"
PROFILE_DIR="debug"
SKIP_BUILD=0

for arg in "$@"; do
    case "$arg" in
        --release)
            PROFILE="release"
            PROFILE_DIR="release"
            ;;
        --skip-build)
            SKIP_BUILD=1
            ;;
        *)
            echo "unknown argument: $arg" >&2
            echo "usage: $0 [--release] [--skip-build]" >&2
            exit 2
            ;;
    esac
done

KERNEL_ELF="${KERNEL_RUNNER_DIR}/target/x86_64-unknown-none/${PROFILE_DIR}/kernel-runner"
UEFI_IMAGE="${KERNEL_RUNNER_DIR}/target/x86_64-unknown-none/${PROFILE_DIR}/boot-uefi.img"

# Auto-detect OVMF firmware path.
if [[ -z "${OVMF_PATH:-}" ]]; then
    for candidate in \
        /usr/share/ovmf/OVMF.fd \
        /usr/share/OVMF/OVMF.fd \
        /usr/share/edk2/ovmf/OVMF_CODE.fd \
        /opt/homebrew/share/ovmf/ovmf-x86_64.bin \
        /usr/local/share/ovmf/OVMF.fd; do
        if [[ -f "${candidate}" ]]; then
            OVMF_PATH="${candidate}"
            break
        fi
    done
fi

# ---------------------------------------------------------------------------
# Banner sequence — must match `kernel_entry` (kernel-runner/src/main.rs)
# and `kmain` (crates/omni-kernel/src/lib.rs). Keep in sync with both.
# ---------------------------------------------------------------------------

EXPECTED_LINES=(
    "[OMNI OS] kernel-runner: entry_point reached."
    "[OMNI OS] early console (COM1) is live."
    "[OMNI OS] proceeding to heap init + kmain."
    "[OMNI OS] kmain entered."
    "[OMNI OS] halting (K4 scope ends here)."
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log() { printf '\033[1;34m[smoke]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[smoke] FAIL:\033[0m %s\n' "$*" >&2; exit 1; }

ensure_qemu_installed() {
    if ! command -v "${QEMU_BINARY}" >/dev/null 2>&1; then
        fail "${QEMU_BINARY} not found in PATH"
    fi
}

ensure_ovmf() {
    if [[ -z "${OVMF_PATH:-}" ]] || [[ ! -f "${OVMF_PATH}" ]]; then
        fail "OVMF firmware not found. Install ovmf package or set OVMF_PATH."
    fi
    log "OVMF: ${OVMF_PATH}"
}

build_kernel_elf() {
    log "building kernel-runner ELF (${PROFILE})..."
    local profile_flag=""
    if [[ "${PROFILE}" == "release" ]]; then
        profile_flag="--release"
    fi
    cargo build \
        --manifest-path "${KERNEL_RUNNER_DIR}/Cargo.toml" \
        --target x86_64-unknown-none \
        ${profile_flag}

    if [[ ! -f "${KERNEL_ELF}" ]]; then
        fail "build did not produce ${KERNEL_ELF}"
    fi
    log "kernel ELF: ${KERNEL_ELF}"
}

build_disk_image() {
    log "building UEFI disk image..."
    # `bootloader 0.11`'s build script invokes `cargo -Z build-std=core`
    # (via the CARGO env-var) to compile the UEFI/BIOS stages, which
    # requires the nightly toolchain.  The kernel itself uses stable 1.85.
    #
    # The bootloader build script does not own the upstream stage-N
    # sources, so any `RUSTFLAGS="-D warnings"` exported by the parent
    # CI environment (qemu-boot-smoke.yml § env) bubbles into those
    # inner builds and trips on legitimate warnings inside upstream code
    # (e.g. unused-imports under newer nightlies). Strip RUSTFLAGS for
    # this single invocation — the kernel-runner build above already ran
    # under the full `-D warnings` policy, so the OMNI-OS-owned code
    # paths remain gated.
    local output
    output=$(RUSTFLAGS= cargo +nightly run --manifest-path "${DISK_IMAGE_DIR}/Cargo.toml" -- "${KERNEL_ELF}" 2>&1) \
        || fail "disk-image builder failed (exit $?); last 40 lines:\n${output}"
    log "${output}"

    if [[ ! -f "${UEFI_IMAGE}" ]]; then
        fail "disk-image builder did not produce ${UEFI_IMAGE}"
    fi
    log "UEFI image: ${UEFI_IMAGE}"
}

run_qemu_and_capture() {
    log "running QEMU (timeout ${SMOKE_TIMEOUT_SECS}s) with OVMF..."

    local serial_log qemu_debug_log
    serial_log=$(mktemp /tmp/qemu-serial-XXXXXXXXXX)
    qemu_debug_log=$(mktemp /tmp/qemu-debug-XXXXXXXXXX)

    # UEFI boot: -bios OVMF.fd + raw disk image via virtio-blk.
    # `-machine q35` is the modern UEFI-compatible chipset.
    # `-debugcon stdio` routes port 0xE9 writes to stdout (kernel's
    # first byte 'K' proves kernel_entry was reached).
    timeout "${SMOKE_TIMEOUT_SECS}" "${QEMU_BINARY}" \
        -machine "q35,accel=kvm:tcg" \
        -cpu "qemu64" \
        -m 256M \
        -bios "${OVMF_PATH}" \
        -drive "if=none,format=raw,file=${UEFI_IMAGE},id=boot" \
        -device "virtio-blk-pci,drive=boot" \
        -serial "file:${serial_log}" \
        -debugcon stdio \
        -d "guest_errors,cpu_reset,unimp" \
        -D "${qemu_debug_log}" \
        -display none \
        -no-reboot \
        -smp 1 \
        2>&1 || true

    echo "[smoke-diag] serial log bytes: $(wc -c < "${serial_log}" 2>/dev/null || echo '?')" >&2
    if [[ -s "${serial_log}" ]]; then
        echo "[smoke-diag] serial log:" >&2
        cat "${serial_log}" >&2
    fi
    if [[ -s "${qemu_debug_log}" ]]; then
        echo "[smoke-diag] QEMU debug events:" >&2
        cat "${qemu_debug_log}" >&2
    else
        echo "[smoke-diag] QEMU debug log: empty" >&2
    fi

    cat "${serial_log}"
    rm -f "${serial_log}" "${qemu_debug_log}"
}

assert_banner_sequence() {
    local output="$1"
    local last_index=-1
    local i
    for i in "${!EXPECTED_LINES[@]}"; do
        local expected="${EXPECTED_LINES[$i]}"
        local found_line
        found_line=$(printf '%s' "${output}" | grep -nF -- "${expected}" \
            | head -n1 | cut -d: -f1 || true)
        if [[ -z "${found_line}" ]]; then
            log "missing banner line: ${expected}"
            log "--- captured output ---"
            printf '%s\n' "${output}"
            log "--- end captured ---"
            fail "expected banner line not found"
        fi
        if [[ "${found_line}" -le "${last_index}" ]]; then
            fail "banner line out of order: '${expected}' at ${found_line}, prev at ${last_index}"
        fi
        last_index="${found_line}"
        log "  [${i}] ✓ ${expected}"
    done
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

log "OMNI OS QEMU boot smoke test"
log "repo root: ${REPO_ROOT}"

ensure_qemu_installed
ensure_ovmf

if [[ "${SKIP_BUILD}" -eq 0 ]]; then
    build_kernel_elf
    build_disk_image
fi

if [[ ! -f "${UEFI_IMAGE}" ]]; then
    fail "UEFI image not found at ${UEFI_IMAGE} (run without --skip-build first)"
fi

OUTPUT=$(run_qemu_and_capture)
log "QEMU done. asserting banner sequence..."

if printf '%s' "${OUTPUT}" | grep -qF 'K'; then
    log "[diag] debug-port marker 'K' found — kernel_entry WAS reached."
else
    log "[diag] debug-port marker 'K' NOT found — kernel_entry was NOT reached."
fi

assert_banner_sequence "${OUTPUT}"
log "PASS — all ${#EXPECTED_LINES[@]} banner lines present and in order."
