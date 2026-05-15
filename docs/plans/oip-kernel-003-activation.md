# Piano: OIP-Kernel-003 Review → Last Call → Active

## Context

OIP-Kernel-003 governa la selezione del bootloader e la transizione `no_std` di `omni-kernel`.
Tutti i suoi gate K1–K5 sono ora soddisfatti:

| Gate | Status |
|---|---|
| K1 — `bare-metal` feature flag | ✅ merged (questo OIP) |
| K2 — `cfg_attr` in `lib.rs` | ✅ merged (questo OIP) |
| K3 — panic handler + allocator | ✅ OIP-Kernel-012 `Active` |
| K4 — `kernel-runner` crate | ✅ implementato (OIP-Kernel-005 in `Review`) |
| K5 — QEMU smoke test CI | ✅ PR #25 merged; CI run 25888095006 verde |

L'OIP è attualmente in `Review` (avanzato da `Draft` il 2026-05-15). Avanzarlo ad `Active`
chiude P6.2 nel `todo.md` e sblocca il progresso formale della governance kernel.

Il **Solo Founder Fast-Track (§5.5)** è applicabile: il founder è l'unico voter eligibile
(≥50% weight, nessun altro voter ≥10%), quindi la finestra Last Call è compressa a **48 ore**
invece di 14 giorni. Qualsiasi blocking objection fondata riporta alla finestra standard.

---

## Approccio: due PR sequenziali

### PR A — `Review → Last Call` (oggi, 2026-05-15)

**File da modificare:**

**1. `oips/oip-kernel-003.md`**
- Frontmatter: `status: Last Call`, `updated: 2026-05-15`
- Tabella K-gates (§3): aggiornare la colonna "Gate" per K3, K4, K5 con riferimenti ai PR/run
  - K3: `OIP-Kernel-012 Active (PR #21)`
  - K4: `OIP-Kernel-005 Review; kernel-runner operativo (PR #25)`
  - K5: `PR #25; CI run 25888095006 — 5/5 banner lines green`
- Reference Implementation: aggiornare "To land before activation" per riflettere ciò che è già atterrato
- Aggiungere sezione "## Amendment History" (o appendere a quella esistente) con riga:
  ```
  | 2026-05-15 | Review → Last Call | 48-hour Solo Founder Fast-Track §5.5;
    window opens 2026-05-15, closes 2026-05-17. |
  ```

**2. `oips/README.md`**
- Riga OIP-003: `Review` → `Last Call *(closes 2026-05-17)*`, data `2026-05-15`

---

### PR B — `Last Call → Active` (dopo 48 ore, ≥ 2026-05-17)

**File da modificare/creare:**

**1. `oips/oip-kernel-003.md`**
- Frontmatter: `status: Active`, `updated: 2026-05-17`
- Appendere a "Amendment History":
  ```
  | 2026-05-17 | Last Call → Active | 48h window elapsed; no blocking objections;
    founder ballot: 1/1 in favour. §5.5 fast-track exercised; re-ratification
    deadline: 90 calendar days after second voter crosses 10% weight floor. |
  ```

**2. `oips/README.md`**
- Riga OIP-003: `Last Call ...` → `Active`, data `2026-05-17`

**3. `docs/audits/solo-founder-fast-track-log.md`** *(file esistente — appendere)*
- Nuova entry con struttura identica a quella esistente:
  - OIP: OIP-Kernel-003
  - PR A (Review → Last Call): URL PR
  - PR B (Last Call → Active): URL PR
  - Window: 48h compressed, aperta 2026-05-15, chiusa 2026-05-17
  - Dominant voter: `cySalazar <cySalazar@cySalazar.com>` (100% sole eligible voter)
  - Blocking objections: none
  - Re-ratification deadline: 90 days from §5.5 deactivation trigger

**4. `docs/audits/qemu-boot-smoke-2026-05.md`** *(nuovo — dal template esistente)*
- Creare da `docs/audits/qemu-boot-smoke-template.md`
- Documentare: data run, CI run ID 25888095006, serial log bytes 257,
  5 banner lines verificate, artifact bootimage-kernel-runner (7-day retention)
- Richiesto esplicitamente dal gate K5 dell'OIP

**5. `todo.md`**
- P6.2: `[~]` → `[x]`
- Aggiornare testo di stato: OIP-Kernel-003 Active (data), PR B merged
- Aggiornare `Last updated:` nell'header

---

## File critici

| File | Azione | PR |
|---|---|---|
| `oips/oip-kernel-003.md` | Frontmatter + K-table + amendment history | A + B |
| `oips/README.md` | Riga OIP-003 | A + B |
| `docs/audits/solo-founder-fast-track-log.md` | Append nuova entry | B |
| `docs/audits/qemu-boot-smoke-2026-05.md` | Nuovo file da template | B |
| `todo.md` | P6.2 `[~]` → `[x]` | B |

Template da riusare: `docs/audits/qemu-boot-smoke-template.md`

---

## Verifica

1. `bash scripts/lint-oips.py` deve passare dopo entrambe le PR (CI check `oip-lint`)
2. `grep "^status:" oips/oip-kernel-003.md` → `status: Active` dopo PR B
3. `grep "P6.2" todo.md` → `[x]` dopo PR B
4. `docs/audits/solo-founder-fast-track-log.md` contiene l'entry OIP-Kernel-003

---

## Note

- OIP-Kernel-005 è ancora in `Review` (K4 gate formale); la sua implementazione è operativa
  ma l'OIP non è Active. Non blocca OIP-Kernel-003 poiché il kernel boots (K5 verde).
  OIP-Kernel-005 può essere avanzato in una sessione separata.
- Nessun `CHANGELOG.md` da aggiornare (le transizioni OIP non sono release di codice).
- Nessun commit deve contenere attributi AI (policy CLAUDE.md).
