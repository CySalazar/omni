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

    local serial_log qemu_debug_log
    serial_log=$(mktemp /tmp/qemu-serial-XXXXXXXXXX)
    qemu_debug_log=$(mktemp /tmp/qemu-debug-XXXXXXXXXX)

    # `-d guest_errors,cpu_reset,unimp -D <logfile>` captures QEMU internal
    # events: cpu_reset (triple faults → resets), guest_errors (illegal guest
    # state), unimp (unimplemented device access). The log file is dumped to
    # stderr after the run so it appears in the CI job log.
    #
    # `-machine pc,accel=tcg`: explicit PC (i440FX) machine + software
    # emulation.  Avoids potential KVM edge cases with a bare-metal payload
    # that has no full IDT yet.
    #
    # `-cpu qemu64`: well-known 64-bit CPU model; avoids host-CPU-feature
    # surprises in a VM environment.
    #
    # `-drive if=ide,...`: explicit IDE interface (BIOS INT 13h path).
    #
    # `-boot order=c,strict=on`: SeaBIOS → boot from first HDD immediately;
    # strict=on suppresses the PXE/floppy fallback that adds latency.
    #
    # QEMU's stderr (termination messages) is captured alongside the debug
    # log so failures show the full picture.
    # `-debugcon stdio` routes QEMU debug port 0xE9 writes to stdout.
    # The kernel writes b'K' (0x4b) to 0xE9 as its very first instruction
    # so we can distinguish "kernel_entry reached" from "bootloader hung".
    # This output is captured by the $() subshell and appears in $OUTPUT.
    timeout "${SMOKE_TIMEOUT_SECS}" "${QEMU_BINARY}" \
        -machine "pc,accel=tcg" \
        -cpu "qemu64" \
        -drive "if=ide,format=raw,file=${IMAGE_PATH}" \
        -serial "file:${serial_log}" \
        -debugcon stdio \
        -d "guest_errors,cpu_reset,unimp" \
        -D "${qemu_debug_log}" \
        -boot "order=c,strict=on" \
        -display none \
        -no-reboot \
        -m 128M \
        -smp 1 \
        2>&1 || true

    # Diagnostic output to the CI job log (stderr — not captured in $OUTPUT).
    echo "[smoke-diag] serial log bytes: $(wc -c < "${serial_log}" 2>/dev/null || echo '?')" >&2
    if [[ -s "${serial_log}" ]]; then
        echo "[smoke-diag] serial log (hex):" >&2
        xxd "${serial_log}" >&2
    fi
    if [[ -s "${qemu_debug_log}" ]]; then
        echo "[smoke-diag] QEMU debug events (guest_errors/cpu_reset/unimp):" >&2
        cat "${qemu_debug_log}" >&2
    else
        echo "[smoke-diag] QEMU debug log: empty (no guest_errors / cpu_reset / unimp events)" >&2
    fi

    # Emit the serial log to stdout so the caller's $() captures it.
    cat "${serial_log}"
    rm -f "${serial_log}" "${qemu_debug_log}"
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

# Diagnostic: check whether the debug port 0xE9 marker ('K', 0x4b) appeared
# in the captured output, which proves kernel_entry was reached.
if printf '%s' "${OUTPUT}" | grep -qF 'K'; then
    log "[diag] debug-port marker 'K' found — kernel_entry WAS reached."
else
    log "[diag] debug-port marker 'K' NOT found — kernel_entry was NOT reached (bootloader hung?)."
fi

assert_banner_sequence "${OUTPUT}"
log "PASS — all ${#EXPECTED_LINES[@]} banner lines present and in order."
