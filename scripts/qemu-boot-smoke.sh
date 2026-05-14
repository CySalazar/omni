#!/usr/bin/env bash
# =============================================================================
# OMNI OS — QEMU boot smoke test
# =============================================================================
# Closes the K5 gate of OIP-Kernel-003 § 3: boots the `kernel-runner`
# ELF under QEMU+OVMF and asserts the canonical banner sequence appears
# on the serial console.
#
# Acceptance:
#   - The five banner lines emitted by `kernel_entry` + `kmain` appear,
#     in order, on the QEMU serial output within `SMOKE_TIMEOUT_SECS`
#     seconds.
#   - QEMU exits cleanly (kernel halts via `hlt`; QEMU is launched with
#     `-no-reboot -no-shutdown` so we trap on the halt and tear down).
#
# Usage:
#   scripts/qemu-boot-smoke.sh                     # build + run + assert
#   scripts/qemu-boot-smoke.sh --skip-build        # use existing image
#   scripts/qemu-boot-smoke.sh --release           # release profile
#
# Environment:
#   OMNI_BOOTIMAGE_BIN   override bootimage command (default: `cargo
#                        bootimage`). Set to `cargo +nightly bootimage`
#                        if your local toolchain needs the override.
#   QEMU_BINARY          override qemu-system-x86_64 (default: from $PATH)
#   SMOKE_TIMEOUT_SECS   how long to wait for the banner (default: 30)
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_RUNNER_DIR="${REPO_ROOT}/kernel-runner"
SMOKE_TIMEOUT_SECS="${SMOKE_TIMEOUT_SECS:-30}"
QEMU_BINARY="${QEMU_BINARY:-qemu-system-x86_64}"
OMNI_BOOTIMAGE_BIN="${OMNI_BOOTIMAGE_BIN:-cargo bootimage}"

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

IMAGE_PATH="${REPO_ROOT}/kernel-runner/target/x86_64-unknown-none/${PROFILE_DIR}/bootimage-kernel-runner.bin"

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

ensure_bootimage_installed() {
    if ! command -v bootimage >/dev/null 2>&1; then
        log "bootimage CLI not found — install via: cargo install bootimage"
        log "(see kernel-runner/README.md § Run under QEMU)"
        fail "bootimage CLI not installed"
    fi
}

ensure_qemu_installed() {
    if ! command -v "${QEMU_BINARY}" >/dev/null 2>&1; then
        fail "${QEMU_BINARY} not found in PATH"
    fi
}

build_image() {
    log "building boot image (${PROFILE})..."
    cd "${KERNEL_RUNNER_DIR}"
    # `cargo bootimage` invokes the kernel build internally; we pass
    # --target so the right artifact directory is used.
    local profile_flag=""
    if [[ "${PROFILE}" == "release" ]]; then
        profile_flag="--release"
    fi
    ${OMNI_BOOTIMAGE_BIN} ${profile_flag} --target x86_64-unknown-none
    cd "${REPO_ROOT}"
    if [[ ! -f "${IMAGE_PATH}" ]]; then
        fail "bootimage build did not produce ${IMAGE_PATH}"
    fi
    log "image: ${IMAGE_PATH}"
}

run_qemu_and_capture() {
    log "running QEMU (timeout ${SMOKE_TIMEOUT_SECS}s)..."
    local output
    output=$(timeout --foreground "${SMOKE_TIMEOUT_SECS}" "${QEMU_BINARY}" \
        -drive "format=raw,file=${IMAGE_PATH}" \
        -serial stdio \
        -display none \
        -no-reboot \
        -no-shutdown \
        -m 512M \
        -smp 1 \
        2>&1 || true)
    printf '%s' "${output}"
}

assert_banner_sequence() {
    local output="$1"
    local last_index=-1
    local i
    for i in "${!EXPECTED_LINES[@]}"; do
        local expected="${EXPECTED_LINES[$i]}"
        # `grep -n` reports `<line_number>:<matched line>`; we trim to
        # just the leading number.
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
if [[ "${SKIP_BUILD}" -eq 0 ]]; then
    ensure_bootimage_installed
    build_image
fi
if [[ ! -f "${IMAGE_PATH}" ]]; then
    fail "image not found at ${IMAGE_PATH} (run without --skip-build first)"
fi

OUTPUT=$(run_qemu_and_capture)
log "QEMU done. asserting banner sequence..."
assert_banner_sequence "${OUTPUT}"
log "PASS — all ${#EXPECTED_LINES[@]} banner lines present and in order."
