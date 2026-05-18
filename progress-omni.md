# OMNI OS — Progress Report

**Data snapshot:** 2026-05-18
**Branch corrente:** `feat/kernel-vga-wait`
**HEAD:** `a822cae` (docs: close MB1-MB9 cycle with Proxmox VMID 103 validation)
**Versione:** `0.1.0` rilasciata, lavoro post-release accumulato su `[Unreleased]`
**Fase di roadmap:** Phase 0 → ingresso Phase 1 (microkernel proof-of-concept)

---

## 1. Executive summary

OMNI OS ha chiuso il ciclo **Track A (desktop grafica)** M1-M5 e il blocco
**Track B (kernel core)** MB1-MB9. Il microkernel `omni-kernel` ora:

- Boota in UEFI con `bootloader_api` 0.11+ su QEMU+OVMF, VirtualBox e
  Proxmox VMID 103 (validato 2026-05-18 00:49 CEST).
- Possiede un page-table walker x86_64 huge-page aware (`PageMapper`),
  un frame allocator basato su bitmap (`BitmapFrameAllocator`), una IDT
  con handler per le eccezioni principali (#DE/#DF/#GP/#PF con dump CR2),
  uno scheduler round-robin cooperativo con context switch assembly stub,
  e un timer LAPIC che pilota la preemption.
- Espone un'ABI syscall via `SYSCALL`/`SYSRET` (MSR `IA32_LSTAR`/`STAR`/`FMASK`)
  con fallback `INT 0x80`, e un loader ELF64 in grado di mappare segmenti
  `PT_LOAD` su VA arbitrari attraverso `PageMapper::map_4k`.
- Disegna un desktop demo con GOP framebuffer, font 8×16, software cursor,
  widget toolkit (`Button`/`Label`/`ProgressBar`/`Window`), input PS/2
  (tastiera + mouse) e tablet VirtIO 1.0+ per il mouse assoluto su
  QEMU/Proxmox, RTC, ACPI S5.

Tutta la pipeline è verificata da **273 test workspace** verdi.

Il prossimo blocco di lavoro è la **MB10 — kernel stack isolation**
(separare gli stack del kernel dal direct-map del bootloader), prerequisito
indispensabile per **MB11 — primo task userspace in Ring 3**.

---

## 2. Stato per track

### 2.1 — Track A: Desktop grafico

**Status:** ✅ chiusa (M1-M5 + M3b).

| Milestone | Contenuto | Stato | Commit |
|---|---|---|---|
| pre-M1 | GOP framebuffer + bitmap font 8×16 | ✅ | `4ba81f1` |
| pre-M1 | Disk-image builder (UEFI+BIOS) | ✅ | `59d712a` |
| M1-M2 | PS/2 event loop + minimal WM | ✅ | `6088f18` |
| M3 | Software cursor pixel save/restore | ✅ | `55665a5` |
| M3b | PS/2 mouse + 5-min countdown | ✅ | `cb91b1c` |
| M4 | Widget toolkit + hit-test + Enter | ✅ | `cea404f` |
| M5 | Desktop orchestrator + RTC + terminal echo | ✅ | `c5014b9` |
| extra | VirtIO 1.0+ tablet (mouse assoluto) | ✅ | `10cb081` |

**Verifica:** VirtualBox + OVMF (2026-05-16); Proxmox VMID 103 (2026-05-18).

### 2.2 — Track B: Kernel core (`omni-kernel` bare-metal)

**Status:** MB1-MB9 ✅ chiuse. Prossimo blocco MB10+.

| Milestone | Contenuto | Stato | Commit |
|---|---|---|---|
| MB1 | `BitmapFrameAllocator<const N>` + GDT | ✅ | `119f3d8` |
| MB2 | `PageMapper` x86_64 walker + `map_4k`/`unmap_4k` | ✅ | `102ec7a` |
| MB3 | IDT + handler #DE/#DF/#GP/#PF | ✅ | `657d7d1` |
| MB4 | `SYSCALL`/`SYSRET` MSR setup + `INT 0x80` dispatch | ✅ | `f2e88da` |
| MB5 | ELF64 loader (parser + segment mapper) | ✅ | `960e440` |
| MB6 | Round-robin scheduler + `omni_context_switch` asm | ✅ | `27720ee` |
| MB7 | LAPIC xAPIC + PIC disable + `sti` + `TICK_COUNT` | ✅ | `27720ee` |
| MB8 | Preemption from LAPIC timer + `need_resched` | ✅ | `5d9989b` |
| MB9 | `PageMapper` huge-page aware + direct-map validator | ✅ | `926a37e` |
| **MB10** | **Kernel stack isolation (ADR-0001 Alt B)** | ⬜ | — |
| **MB11** | **Primo userspace process Ring 3 (ELF→exec)** | ⬜ | — |
| **MB12** | **IPC reale (queue + capability check)** | ⬜ | — |

**Verifica MB1-MB9:**
- `cargo test --workspace` → 273 pass / 0 fail
- `cargo test -p omni-kernel --features bare-metal` → 75 unit + 21 integration green
- Boot QEMU+OVMF (macOS arm64): banner K5 + paging validator + IDT + syscall +
  sched + lapic + mb8-smoke = OK, niente `#PF code=2`.
- Boot Proxmox VMID 103 (`100.101.77.9`): banner + paging + `[virtio] tablet ready`
  + desktop disegnato sul framebuffer VNC. ADR-0001 → `accepted` pieno.

### 2.3 — Governance & OIP

**OIP totali:** 14 (escluso template/sentinel).

| Tier | OIP | Status |
|---|---|---|
| Active | `OIP-Process-001` (filing process) | ✅ |
| Active | `OIP-Kernel-003` (UEFI + `bootloader` crate) | ✅ |
| Active | `OIP-Kernel-005` (boot ABI + `kernel-runner`) | ✅ |
| Active | `OIP-Kernel-012` (panic handler + bump alloc) | ✅ |
| Last Call | `OIP-Bounty-002` (closes 2026-05-26) | 🟡 |
| Last Call | `OIP-Serde-004` (`bincode` → `postcard`, closes 2026-05-26) | 🟡 |
| Draft | `OIP-Crypto-002` (algoritmi base) | 🔵 |
| Draft | `OIP-Voting-005` (uptime/contribution v2) | 🔵 |
| Draft | `OIP-Container-006` (OmniContainer + Wine) | 🔵 |
| Draft | `OIP-Helper-007` (autonomy levels) | 🔵 |
| Draft | `OIP-Pkg-008` (package manager federato) | 🔵 |
| Draft | `OIP-Forge-009` (Rust→WASM/ELF on-demand) | 🔵 |
| Draft | `OIP-Market-010` (Stichting marketplace) | 🔵 |
| Draft | `OIP-Flagship-011` (Omni\* prefix + OmniCode) | 🔵 |

Tutti i 4 gate K1-K5 di `OIP-Kernel-003` sono chiusi; il CI ha 3 nuovi
required check su `main` (`QEMU boot smoke`, `bare-metal build`,
`kernel-runner build`).

### 2.4 — Cross-cutting

| Area | Stato |
|---|---|
| `omni-types` (id, errori, versioning) | ✅ P1 chiuso |
| `omni-crypto` (AEAD, sign, KEX, hash, KDF) | ✅ P1; ⏳ `AWAITING_CRYPTO_REVIEW` |
| `omni-capability` (Macaroons + revocation) | ✅ P1 |
| `omni-tee` (TDX/SEV-SNP scaffold + Mock) | 🟡 scaffold, P5.2/5.3 in `[~]` |
| `omni-hal` | 🟡 stub |
| `omni-mesh` | 🟡 stub + handshake spec |
| `omni-runtime`/`omni-sdk`/`omni-agent`/`omni-shell` | 🔵 stub |
| `omni-container` | 🟡 skeleton + KVM TODO (P8) |
| `omni-tokenization` | 🔵 stub |
| `omni-kernel` | 🟢 MB1-MB9 |
| `kernel-runner` | 🟢 OIP-Kernel-005 Active |
| `disk-image-builder` | 🟢 UEFI/BIOS |
| Migrazione `bincode`→`postcard` (P7) | ✅ M1-M5 landed, ⬜ P7.3 docs |
| Tamarin v0.4 (mesh handshake) | ✅ 8 lemmas verified in ~1.36s |
| CI (GitHub Actions) | ✅ ci/audit/sbom/codeql/dco/qemu-smoke/bare-metal/kernel-runner |
| Cryptographer review | ⏳ bloccata da funding (P3.2/P4) |
| External kernel audit | ⏳ Phase 1 deliverable, fine 2030 |

---

## 3. Test e build evidence

```
cargo test --workspace                                  273 pass / 0 fail
cargo test -p omni-kernel --features bare-metal          75 unit + 21 integration
cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal,mb8-smoke  clean
cargo clippy --workspace --all-targets --all-features -- -D warnings  clean
cargo audit                                              clean (RUSTSEC-2025-0141 risolto)
cargo deny check advisories                              ok (bans/licenses pre-esistenti)
```

LOC sorgente Rust del workspace:

- `crates/`                          20.166 LOC
- `kernel-runner/`                      489 LOC
- `disk-image-builder/`                 102 LOC
- Totale produzione                 ~20.757 LOC

---

## 4. Cosa manca (gap analysis)

### 4.1 — Kernel (Track B)

1. **MB10 — Kernel stack isolation.** Gli stack dei task kernel sono
   attualmente costruiti su `phys + boot_info.physical_memory_offset`,
   cioè nel direct-map del bootloader. Funziona solo perché il direct-map
   copre la RAM Usable; non isola il kernel da scritture errate da parte
   di codice futuro (driver, IPC, userspace tramite syscall). Alt B di
   `ADR-0001` propone un range VA dedicato (`0xFFFF_C000_…`) con
   `map_4k` esplicito per ogni stack. Prerequisito per MB11.
2. **MB11 — Primo processo userspace Ring 3.** Si compone di:
   - User-page mapping separato (range basso, `0x0000_0040_…`).
   - User-stack allocation.
   - Costruzione del frame `iretq` per il primo salto in Ring 3
     (CS = user-code 0x1B, SS = user-data 0x23, RFLAGS con IF=1).
   - Probe ELF64 (l'embed di 120 byte di MB5 è già pronto) eseguito
     come processo, non come scaffold di test.
   - Syscall stub mancanti: `TaskExit`, `MemoryAlloc`, `WriteConsole`
     (almeno), per chiudere il loop minimal "hello-userspace".
3. **MB12 — IPC reale.** Lo skeleton `crates/omni-kernel/src/ipc.rs`
   (`ChannelId`, `MessageKind`, `BackpressurePolicy`, `MessageEnvelope`,
   trait `Ipc`) è in tree dal 2026-05-12. Mancano: queue concreta in
   kernel space, capability check tramite `omni-capability`, syscall
   `IpcSend`/`IpcRecv`, integration test cross-process.
4. **TLB shootdown multi-core.** Nessun MP/AP enable; LAPIC è già pronta
   ma il sistema gira su un solo core. Non bloccante per MB10-MB12 ma
   sarà necessario prima di P6.7 (driver).
5. **`map_4k` huge-page split.** Documentato come limite in MB9: oggi
   `map_4k` non splitta una 2 MiB/1 GiB PS=1 entry. Non bloccante finché
   il kernel non riscrive VA in range huge-page mappati dal bootloader,
   ma rischia di mordere quando il driver model entra in scena.
6. **Userspace driver model (P6.7).** NVMe + Ethernet/Wi-Fi + TEE in
   user space. Bloccato da MB10-MB12 + MP/AP enable.
7. **External kernel + capability audit (P6.8).** Deliverable di Phase 1,
   bloccato da P4 funding + P6.7 done.

### 4.2 — Crypto / Mesh

8. **`omni-crypto` cryptographer review (P3.2).** Marker
   `AWAITING_CRYPTO_REVIEW` ancora attivo. Tamarin v0.4 chiude P3.1
   spec/proof, ma il review review di un cryptographer esterno resta
   bloccato da P4 funding.
9. **`OMNI-PROTO-v0.2` documentation (P7.3).** `docs/protocol/handshake.md`
   § 3.2 negozia ancora `OMNI-PROTO-v0.1`. Il codice è già v0.2
   (`omni-types::version::PROTOCOL_VERSION_V0_2`). Edit-only, 1 PR.
10. **TEE backend reali (P5.2/P5.3).** TDX + SEV-SNP scaffold presenti
    ma `MockTeeBackend` è l'unico operativo. Richiede hardware
    (P4.1 funding).

### 4.3 — Stichting / Governance / Funding

11. **Stichting OMNI registration.** Bylaws + checklist drafted
    (P4.1); registrazione notarile pending.
12. **Funding round.** Pitch deck + 4 grant draft pronti
    (P4.2); submission pending.
13. **Hiring core team.** Job descriptions + salary band drafted
    (P4.4); engagement bloccato dal funding.

### 4.4 — Container & App Mesh

14. **`OIP-Container-006`** rimane Draft. La specifica c'è (KVM micro-VM +
    Wine + cyDock evoluzione), il reference impl `crates/omni-container/`
    è solo skeleton. P8.2-P8.7 bloccati.
15. **`OIP-Helper-007`/`-Pkg-008`/`-Forge-009`/`-Market-010`/`-Flagship-011`**
    tutte Draft, blocked-on OIP-Container e/o decisioni governance.

### 4.5 — CI / Misc

16. **Pre-MB9 KNOWN BLOCKER nelle entries CHANGELOG MB8.** La riga
    "Known blocker (MB9)" del 2026-05-17 è ora storica. Non bloccante
    ma andrà annotata come "resolved by MB9" in un prossimo passaggio
    di documentation hygiene.

---

## 5. Prossimi step (priorità ordinata)

### Step 1 (oggi/domani) — Merge `feat/kernel-vga-wait` → `main`

Il branch accumula ~30 commit (Track A + MB1-MB9 + fix vari +
documentation closure). Decisione operativa: PR unica monolitica.

**Azioni:**

1. `gh pr create --base main --head feat/kernel-vga-wait` con descrizione
   che riferisce tutti i commit + ADR-0001 + le righe `[Unreleased]` del
   `CHANGELOG.md`.
2. Verifica che i 3 required check di `main` passino: `QEMU boot smoke`,
   `bare-metal build`, `kernel-runner build`.
3. Squash merge (per convenzione del repo) → bump versione `[Unreleased]`
   → `[0.2.0]` su `CHANGELOG.md` (è una minor: nessun breaking API
   pubblica, solo nuove capability kernel).
4. Tag `v0.2.0`.

**Output atteso:** `main` allineata a HEAD; release v0.2.0 pubblicata su GitHub.

### Step 2 (questa settimana) — MB10: Kernel stack isolation

**Filosofia:** trasformare gli stack del kernel da "frame fisico + offset
direct-map" in "VA dedicata mappata esplicitamente con `map_4k`".

**Design (da scrivere come ADR-0002 prima del codice):**

- Range VA: `0xFFFF_C000_0000_0000`..`0xFFFF_C7FF_FFFF_FFFF` (1 TiB,
  half-canonical, riserva kernel-only).
- Layout per task: ogni stack è 4 KiB (size attuale) + 4 KiB guard page
  non mappata (catch overflow → #PF deterministico).
- `RoundRobinScheduler::spawn_kernel_task` non passa più
  `phys + phys_offset` come stack VA: alloca un frame con
  `BitmapFrameAllocator`, ne sceglie una VA libera nel range dedicato,
  fa `mapper.map_4k(stack_va, frame_phys, PTE_PRESENT|PTE_WRITABLE)` +
  lascia la pagina sopra unmapped come guard.
- `omni_context_switch` invariato (lavora su RSP, non gli interessa
  l'origine della VA).
- Test: spawn N task, scrivi pattern sentinella, verifica che ogni VA
  cada nel range dedicato; provoca volutamente uno stack-overflow → #PF
  su guard page con CR2 atteso.

**File toccati attesi:**

- `crates/omni-kernel/src/scheduling.rs` — `spawn_kernel_task` / TCB.
- `crates/omni-kernel/src/lib.rs` — bootstrap del range VA in `kmain`.
- `crates/omni-kernel/src/bare_metal/paging.rs` — eventualmente
  helper `map_kernel_stack(frame_phys, &mut alloc)`.
- `docs/adr/0002-mb10-kernel-stack-isolation.md` — nuovo.

**Verifica:** 273 → ~280 test; smoke QEMU+OVMF + Proxmox verde con la
nuova diagnostica `[stack] kernel stack VA range = …`.

### Step 3 (settimana 2) — MB11: Primo userspace process

Dipende da MB10. Combinare:

- `Elf64::parse` + `map_and_load` per il probe ELF (già esistente).
- User-page mapping: range basso (es. `0x0000_0040_0000_0000`).
- User-stack: stesso pattern di MB10 ma con `PTE_USER` set.
- Frame `iretq` con CS/SS user, `RFLAGS|=IF`.
- Syscall stub `TaskExit` (chiama `Scheduler::despawn_current`),
  `WriteConsole(ptr, len)` (chiama `early_console::emit`),
  `MemoryAlloc(size) -> *mut u8` (chiama allocator).
- Probe user-ELF: un binario ~256 byte che fa `WriteConsole("hello\n")`
  e `TaskExit(0)`.

**File toccati attesi:** `bare_metal/syscall_entry.rs` (handler reali),
`scheduling.rs` (process vs task distinction), `bare_metal/process.rs`
(nuovo modulo), `bare_metal/usermode.rs` (nuovo modulo trampoline).

**Verifica:** smoke run mostra `[user] hello\n  exit=0`.

### Step 4 (settimana 3) — MB12: IPC concreto

Dipende da MB11. Sblocca P6.6, e a cascata P6.7 (driver model).

### Step 5 (parallelo, low-effort) — P7.3 docs

Aggiornare `docs/protocol/handshake.md` § 3.2 a `OMNI-PROTO-v0.2`. Edit-only,
nessun codice; chiude P7 e libera un check verde su `oip-lint`.

### Step 6 (parallelo, governance) — OIP transitions

- `OIP-Bounty-002` e `OIP-Serde-004` Last Call → Active il 2026-05-26
  (entrambe richiedono PR docs + audit log entry per Solo Founder Fast-Track).
- `OIP-Crypto-002` Draft → Review (richiede bibliografia + test vectors).

### Step 7 (medio termine) — Container P8

Sbloccabile dopo MB12 + MP/AP enable + driver model. `OIP-Container-006`
Draft → Review.

---

## 6. Allineamento con la roadmap

| Roadmap | Stato attuale |
|---|---|
| **Phase 0 — Foundation (mesi 0-6)** | ~75% (governance ✅, foundational crates ✅, OIP process ✅, funding/legal in corso) |
| **Phase 1 — Microkernel POC (mesi 6-18)** | ~40% (boot ✅, paging ✅, scheduler ✅, syscall ✅, ELF loader ✅; mancano IPC, capability dispatch, driver model, audit) |
| **Phase 2 — AI Runtime + Tier 0** | 0% (bloccato da Phase 1) |
| **Phase 3-7** | 0% |

I deliverable Phase 1 della roadmap (`docs/06-roadmap.md` § "Phase 1"):

- ✅ "Microkernel boots on x86_64 hardware" (QEMU+OVMF + VirtualBox + Proxmox).
- ⚠️ "with Intel TDX or AMD SEV-SNP" — TDX/SEV-SNP scaffolding c'è (`omni-tee`),
  ma nessun real boot su hardware TEE-capable: pending Phase 1.5 + hardware.
- ⬜ "IPC primitives operational (typed message passing)" → MB12.
- ⚠️ "Capability-based security primitives implemented" — `omni-capability`
  c'è (43 unit + 7 integration test); manca l'integrazione con syscall
  dispatch (MB11+).
- ✅ "Memory management, scheduling, interrupt handling" (MB1-MB3 + MB6-MB9).
- ⬜ "Drivers (in user space): NVMe storage, Ethernet/Wi-Fi networking, TEE"
  → P6.7 (post MB12).
- ✅ "Boot loader (UEFI-based)" — `bootloader` 0.11+ + `kernel-runner`
  (OIP-Kernel-005 Active).
- ⚠️ "Minimal shell sufficient for development" — il desktop demo (Track A)
  ha un terminal echo ma non un REPL; bloccato da MB11 (serve una shell
  in user-space, non in kernel).
- ⬜ "No AI yet — focus on a solid kernel foundation" — rispettato.
- ⬜ "First external security audit of kernel + capability system" → P6.8,
  bloccato da P4 funding + P6.7 done.

**Conclusione:** la roadmap Phase 1 è on-track. Il prossimo collo di
bottiglia tecnico è MB10-MB12 (stack isolation → user-space → IPC); il
prossimo collo di bottiglia non-tecnico è il funding Phase 0.

---

## 7. Rischi & blocker

| Rischio | Probabilità | Impatto | Mitigation |
|---|---|---|---|
| `bootloader_api` 0.12 rompe il direct-map | media | alta | Pinning a `=0.11.X` in `kernel-runner/Cargo.toml` (OIP-Kernel-005 § S9). Validator MB9 segnala automaticamente "skipped M MiB" se l'invariante decade. |
| Stack overflow nel kernel passa inosservato | alta (oggi) | alta | MB10 introduce guard page non mappata → #PF deterministico con CR2 sul serial. |
| Cryptographer review non si chiude in tempo per Phase 2 | alta | alta | Tamarin v0.4 chiude la metà spec; cercare review pro-bono se P4 funding ritarda. |
| `OIP-Kernel-005` (kernel-runner) dipende da single contributor | alta | media | Documentazione esiste; pinning versione bootloader; CI smoke gate. |
| Hardware TEE acquisition (Intel TDX / AMD SEV-SNP) | alta | media | Cloud TEE è alternativa (Azure Confidential VMs); decision deferred a Phase 1 mid-point. |
| Proxmox manual deploy step non scalabile | media | bassa | Documentato in `reference-proxmox-deploy`; valutare automation script in Step 3-4. |

---

## 8. Riferimenti

- Roadmap: [`docs/06-roadmap.md`](docs/06-roadmap.md)
- ADR MB9: [`docs/adr/0001-mb9-paging-huge-page-aware.md`](docs/adr/0001-mb9-paging-huge-page-aware.md)
- Plan OIP-Kernel-003: [`docs/plans/oip-kernel-003-activation.md`](docs/plans/oip-kernel-003-activation.md)
- Changelog: [`CHANGELOG.md`](CHANGELOG.md)
- OIP index: [`oips/README.md`](oips/README.md)
- Todo dettagliato: [`todo.md`](todo.md)

---

*Report generato manualmente dallo stato del repository a `HEAD = a822cae` su `feat/kernel-vga-wait`. Aggiornare a ogni milestone closure.*
