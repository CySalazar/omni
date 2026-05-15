# QEMU boot smoke — 2026-05-15

K5 gate of `OIP-Kernel-003` § 3 — `kernel-runner` boots under QEMU+SeaBIOS
and emits the canonical banner sequence on the serial console.

## Environment

| Field | Value |
|---|---|
| **Date (UTC)** | `2026-05-15` |
| **CI run ID** | `25888095006` |
| **CI run URL** | `https://github.com/CySalazar/omni/actions/runs/25888095006` |
| **Branch** | `feat/oip-kernel-006-qemu-smoke` (PR #25) |
| **kernel-runner commit** | see CI run artifact |
| **Serial log size** | 257 bytes |
| **Artifact** | `bootimage-kernel-runner` (7-day retention) |

## Invocation

```bash
bash scripts/qemu-boot-smoke.sh --release
```

(equivalently: `qemu-boot-smoke` workflow job — see CI run URL above.)

## Result

**PASS** — all 5 banner lines present and in order on first CI run.

## Banner-sequence assertion

The smoke script asserts that the five lines below appear, in order,
on the serial console. All five verified green in CI run 25888095006.

- [x] `[OMNI OS] kernel-runner: entry_point reached.`
- [x] `[OMNI OS] early console (COM1) is live.`
- [x] `[OMNI OS] proceeding to heap init + kmain.`
- [x] `[OMNI OS] kmain entered.`
- [x] `[OMNI OS] halting (K4 scope ends here).`

Lines 1–3 are emitted by `kernel-runner/src/early_console.rs::announce_boot`;
lines 4–5 are emitted by `omni_kernel::kmain`.

## Memory map observed

| Field | Value |
|---|---|
| Region count | see CI run serial log |
| `MIN_HEAP_BYTES` met by `pick_region` | YES — no panic |

## Anomalies / follow-ups

| Severity | Description | Action |
|---|---|---|
| low | ET_DYN → ET_EXEC fix required `--no-pie` linker flag via `build.rs` | Resolved in PR #25 (commits `be6a3e8`, `e327a8c`) |

## Sign-off

| Role | Name | Date |
|---|---|---|
| Smoke-test runner | `cySalazar` | `2026-05-15` |
| Editor body (Seat 1) | `cySalazar` | `2026-05-15` |

---

*This file is part of the K5 audit trail mandated by `OIP-Kernel-003`
§ 3 (last row of the K1–K5 table). Gate closed by CI run 25888095006;
OIP-Kernel-003 subsequently advanced to `Active` via §5.5 Solo Founder
Fast-Track (PR #26 → this PR).*
