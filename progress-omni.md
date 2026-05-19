# OMNI OS — Progress Report

**Data snapshot:** 2026-05-19 (post MB13.g — comprehensive IDT coverage for synchronous exception vectors 0..=21)
**Branch corrente:** `feat/kernel-mb11-userspace` (locale; in attesa di PR + merge in `main`)
**HEAD:** post-MB13.g — IDT cattura ora ogni vettore sincrono CPU 0..=21, niente più triple-fault silenti
**Versione:** `0.2.0` rilasciata 2026-05-18; lavoro post-release accumulato su `[Unreleased]` (MB10 + Step 7.1-7.4 + MB11.1-MB11.9 + MB12.0a-MB12.9 + **MB13.a + MB13.b + MB13.c + MB13.d + MB13.f + MB13.g**).
**Fase di roadmap:** Phase 0 → Phase 1 (microkernel proof-of-concept), ~78% Track B

---

## 1. Executive summary

OMNI OS ha chiuso il ciclo **Track A (desktop grafica)** M1-M5, il blocco
**Track B (kernel core)** MB1-MB9, ed è stata rilasciata la versione
**v0.2.0** (PR #29 squash-merged in `main` come commit `25790f0`, tag
`v0.2.0` su GitHub release).

Sopra v0.2.0 sono stati chiusi tre blocchi consecutivi sul branch
`feat/kernel-mb11-userspace` (locale, da PR):

1. **MB10 — kernel stack isolation** (PR #33 squash-merged come commit
   `8c1496a`, 2026-05-18). ADR-0002 `accepted`.
2. **Step 7 — lift dei blanket `#![allow]` su `omni-kernel`** (4 commit
   `770c7aa` → `1768966`, 2026-05-18). ADR-0003 `accepted` + guardrail CI
   `blanket-allow-guard` (`scripts/check-no-blanket-allow.sh`) ora bloccante.
3. **MB11 — primo processo userspace Ring 3 con per-process CR3**
   (2 commit `22289e1` + `c743173`, 2026-05-18). ADR-0004 `accepted`.
4. **MB12 — IPC concreto + multi-task user-space** (post-c743173,
   2026-05-18). ADR-0005 `accepted`. Sotto-blocchi MB12.0a-MB12.9.

Il microkernel `omni-kernel` ora:

- Boota in UEFI con `bootloader_api` 0.11+ su QEMU+OVMF, VirtualBox e
  Proxmox VMID 103 (validato 2026-05-18 00:49 CEST).
- Possiede un page-table walker x86_64 huge-page aware (`PageMapper`)
  con `map_4k_into(root, ...)` per address-space target espliciti,
  un frame allocator basato su bitmap (`BitmapFrameAllocator`), una IDT
  con handler per le eccezioni principali (#DE/#DF/#GP/#PF con dump CR2),
  uno scheduler round-robin cooperativo con context switch assembly stub,
  e un timer LAPIC che pilota la preemption.
- Espone un'ABI syscall via `SYSCALL`/`SYSRET` (MSR `IA32_LSTAR`/`STAR`/`FMASK`)
  con fallback `INT 0x80`. Lo `STAR[63:48]` è ora `0x10` (riconciliato in
  MB11.1 — il valore precedente `0x001B` era placeholder errato per la
  GDT a 3 slot e produceva selettori inesistenti). SYSRET produce
  CS=`0x23` (slot 4 ucode64) + SS=`0x1B` (slot 3 udata).
- Carica binari ELF64 nella propria AS via `Elf64::map_and_load_into`,
  oltre al map_and_load classico sull'AS attiva.
- **GDT estesa 3 → 7 slot** ([gdt.rs](crates/omni-kernel/src/bare_metal/gdt.rs)):
  null, kcode64 (0x08), kdata64 (0x10), udata (0x18 placeholder), ucode64
  (0x20), TSS (0x28 over slots 5+6).
- **TSS unica** ([tss.rs](crates/omni-kernel/src/bare_metal/tss.rs))
  installata via `ltr 0x28`. Holds `rsp0..rsp2`, `ist1..ist7`,
  `iomap_base=104`.
- **Alloca gli stack dei kernel-task in un range VA kernel-only dedicato
  (`0xFFFF_C000_0000_0000`..`0xFFFF_C800_0000_0000`, 8 TiB, ≈ 1 G slot)
  con guard page non mappata sotto ciascun stack utile** — stack overflow
  → `#PF` deterministico con `CR2` sulla guard (MB10).
- **`AddressSpace` per-process** ([address_space.rs](crates/omni-kernel/src/bare_metal/address_space.rs)):
  ogni processo possiede una PML4 dedicata; le entries 256–511
  (kernel half) sono clonate per riferimento dal CR3 di boot (MB11.2,
  ADR-0004 § 4).
- **User stack VA range** `0x0000_0040_0000_0000..0x0000_0040_8000_0000`
  (2 GiB, 16 KiB stack + 16 KiB guard per slot) — MB11.3.
- **`ProcessControlBlock` + `spawn_from_elf`** ([process.rs](crates/omni-kernel/src/process.rs))
  che orchestra AddressSpace + ELF map + user stack + kernel stack MB10
  + scheduler registration — MB11.4.
- **Trampolino `enter_user_mode` (iretq + CR3 reload)** ([usermode.rs](crates/omni-kernel/src/bare_metal/usermode.rs))
  + `validate_user_buffer` (range guard + 4-level PT walk con check
  `PTE_PRESENT | PTE_USER`) — MB11.5.
- **Syscall handler reali** ([syscall_entry.rs](crates/omni-kernel/src/bare_metal/syscall_entry.rs)):
  `TaskExit (11)` (dequeue + halt), `WriteConsole (60)` (validated copy
  to `early_console::emit`), `MemMap (1)` stub — MB11.6.
- **Hand-crafted ELF user-probe** (167 byte) embedded in
  [`bare_metal/userprobe.rs`](crates/omni-kernel/src/bare_metal/userprobe.rs):
  `syscall(WriteConsole, "hello\n") + syscall(TaskExit, 0)` —
  MB11.7. Gated dietro `mb11-userprobe` feature (mirror del pattern
  `mb8-smoke`).
- **`kmain` boot wiring** spawn dell'user-probe + jump in Ring 3 via
  `enter_user_mode` — MB11.8.
- Disegna un desktop demo con GOP framebuffer, font 8×16, software cursor,
  widget toolkit (`Button`/`Label`/`ProgressBar`/`Window`), input PS/2
  (tastiera + mouse) e tablet VirtIO 1.0+ per il mouse assoluto su
  QEMU/Proxmox, RTC, ACPI S5 (questo path solo senza `mb11-userprobe`).

Tutta la pipeline è verificata da **426 test workspace** verdi (era 393
post-MB11; +33 da MB12 fra capability stub, IPC registry, userprobe
MB12, integration cross-process, PCB extensions). Step 7 + MB12
mantengono il blanket allow guard verde: `omni-kernel/src/lib.rs` non
porta alcun `#![allow(<group>)]` non whitelisted; ogni nuovo `unsafe`
in `ipc.rs` è dichiarato a livello modulo con reason.

Il blocco **MB13.a (`force-soft` SIMD su `sha2`/`poly1305`/`chacha20`/
`curve25519-dalek` + `sha2_backend="soft"` per `sha2 0.11`)** è stato
chiuso il 2026-05-19. `cargo build -p omni-crypto --target
x86_64-unknown-none --no-default-features` ora compila clean (era LLVM
ICE su intrinsics SIMD). Soluzione: workspace `.cargo/config.toml` con
rustflags target-conditional + feature passthrough Cargo su `sha2 0.10`
(transitive via i dalek). Nessuna estrazione di `omni-crypto-verify`
necessaria (Alternativa A in ADR-0005 § Migration NON adottata).

Il blocco **MB13.b (Boot-path fix — ET_DYN/PIE kernel + upper-half
dynamic mapping)** è stato chiuso il 2026-05-19. Tre cambi atomici:
(i) `kernel-runner/.cargo/config.toml` non forza più ET_EXEC (`-C
relocation-model=static` + `-C link-arg=--no-pie` rimossi); (ii)
`kernel-runner/build.rs` rimosso interamente (emetteva
`cargo:rustc-link-arg=--no-pie` *dopo* i flag del target spec, e LLD
honora l'ultimo flag — un override silenzioso che teneva il kernel
ET_EXEC anche dopo la rimozione dei flag); (iii) `BOOTLOADER_CONFIG`
in `kernel-runner/src/main.rs` ora imposta
`mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`. Il
target spec `x86_64-unknown-none` ha `position-independent-executables
= true` per default su Rust 1.83+, quindi con i due flag legacy rimossi
l'output è ora un ELF `ET_DYN` con codice RIP-relative (verificato via
`readelf -h` → `Type: DYN (Position-Independent Executable file)`).
`BOOTLOADER_CONFIG` in `kernel-runner/src/main.rs` ora imposta
`mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000)`, così
`bootloader 0.11` rilocca il kernel image, lo stack, il `BootInfo`, il
framebuffer e il direct-map della RAM fisica tutti in upper half (PML4
indici ≥ 256). Quella metà è clonata per riferimento da
`AddressSpace::new_with_kernel_half`, quindi il `mov cr3` in
`enter_user_mode` non perde più l'istruzione successiva — root cause
del triple-fault `mb12-userprobe` su Proxmox VMID 103 risolto a livello
deterministico.

Il blocco **MB13.c (`omni-capability` integration + `Ed25519CapabilityProvider`)**
è stato chiuso il 2026-05-19. Quattro cambi atomici:

(i) **`omni-types` split feature `id-generation` → `id-types` + `id-generation`**:
`id-types` espone i tipi del modulo `identity` senza richiedere
`getrandom`; `id-generation` (default ON, superset) abilita anche i
costruttori `::new()` CSPRNG-backed. La dep `uuid` è ora dichiarata
direttamente in `omni-types` con `features = ["serde"]` (senza `v4`),
così la build bare-metal non trascina più la transitive su `rand`. La
helper `random_uuid_bytes` e i metodi `::new()` su `AgentId`,
`CapabilityId`, `SessionId` sono ora gated `#[cfg(feature = "id-generation")]`.

(ii) **`omni-capability` nuove feature `mint` + `bare-metal`**:
`default = ["mint"]` mantiene il comportamento userspace; `mint`
abilita `omni-types/id-generation` + `omni-crypto/rng` (richiesti da
`CapabilityToken::mint` e `attenuation::attenuate`); `bare-metal`
forwarda `omni-crypto/bare-metal` e — combinato con
`--no-default-features` — produce un build verify-only che compila su
`x86_64-unknown-none`. I path mint sono gated
`#[cfg(feature = "mint")]`; il path verify (`verify_signature`,
`verify_full`) resta sempre disponibile. Aggiunte tre varianti
semver-safe `#[non_exhaustive]`: `Action::IpcSend`, `Action::IpcRecv`,
`Resource::IpcChannel(u64)`. Subset relation per `IpcChannel` è
uguaglianza (handle opaco kernel).

(iii) **`omni-kernel` ora dipende da `omni-capability`** con
`default-features = false, features = ["bare-metal"]`. Le dev-deps
abilitano `mint` + `id-generation` + `rng` per i test host, senza
inquinare la build bare-metal (`cargo build --target x86_64-unknown-none`
non pull dev-deps).

(iv) **`Ed25519CapabilityProvider` in
`crates/omni-kernel/src/capabilities.rs`** con tre superfici:
`verify_signature_only(token)` (Ed25519 sig only),
`verify_signed_token(token, now)` (full verify: signature + time +
TEE binding via `StubAttestation` legato a `node_id_bytes` + empty
`RevocationList`), e `verify(token, action, resource)` (impl
`KernelCapabilityCheck` — O(1) shape match identico allo stub, così
il provider è drop-in replacement al livello per-IPC). Il provider è
disponibile nel kernel ma non ancora wired nei syscall IPC: il
plumbing dei token postcard via `IpcCreateChannel` ABI è MB13.d.
`StubCapabilityProvider` resta quindi il default del boot wiring fino
a MB13.d. **+11 test host-side** (5 in `omni-capability::scope` +
6 in `omni-kernel::capabilities`), workspace target ≥ 432 pass.

Il blocco **MB13.d (`IpcCreateChannel` syscall ABI extension)** è stato
chiuso il 2026-05-19. Tre cambi atomici:

(i) **`crates/omni-kernel/src/capabilities.rs`** — nuovo helper
`decode_and_authenticate_token(bytes, expected_action, provider, now)
-> KernelResult<KernelPrincipal>`. Decodifica i byte postcard via
`omni_types::wire::decode_canonical::<CapabilityToken>`, esegue
`Ed25519CapabilityProvider::verify_signed_token` (signature + time
window + TEE binding), valida che `scope.action` corrisponda allo
slot send/recv, accetta qualunque `Resource::IpcChannel(_)` (lo user
non può prevedere l'id monotonico kernel — il kernel rebinda la
risorsa al canale appena allocato). Il `KernelPrincipal` restituito è
il `payload.subject` (32 byte NodeId attestation hash). +7 test.

(ii) **`crates/omni-kernel/src/ipc.rs`** + **`bare_metal/syscall_entry.rs`** —
`KernelIpcRegistry::create_channel_signed(owner, policy,
send_token_bytes, recv_token_bytes, &provider, now)` espone la nuova
superfice; entrambi i token `None` → delegate al
`StubCapabilityProvider` esistente (legacy MB12 path, byte-per-byte
identico al pre-MB13). Almeno uno presente → decode + verify per slot,
canale registrato con `send_subject` / `recv_subject` valorizzati dal
subject del token. La syscall `IpcCreateChannel(20)` ora usa l'ABI a
6 argomenti: `(queue_depth, backpressure, tee_bound, send_ptr,
recv_ptr, lens)` con `lens = send_len:u32 | (recv_len:u32 << 32)`;
cap on-stack di 1 KiB per token (real token ≈ 200 byte). +4 test
integration `KernelIpcRegistry::create_channel_signed`.

(iii) **`crates/omni-kernel/src/bare_metal/userprobe_mb12.rs`** — il
pre-create del canale MB12 ora passa attraverso `create_channel_signed`
con entrambi gli slot `None` e `Ed25519CapabilityProvider::placeholder()`.
Il behaviour è identico (la registry riconosce no-token e delega al
stub provider); l'indirezione documenta che `Ed25519CapabilityProvider`
è ora il provider canonico del boot wiring. `mb12-userprobe` smoke
build verde a tutti i livelli di clippy.

`tests/mb13_capability_signed.rs` (+11 test, target workspace ≥ 443).
Build Info panel aggiornato a Active=`MB13.d IpcCreateChannel ABI`,
Next=`MB13.e PR + intermediate tag`, Phase 1 ≈ 75%.

Il blocco **MB13.f (`enter_user_mode` kernel-stack swap, first-dispatch
smoke fix)** è stato chiuso il 2026-05-19. La pipeline MB13.b aveva
risolto il triple-fault del primo `mov cr3` dentro `enter_user_mode`
spostando il kernel ELF in upper half (PIE/ET_DYN), ma il deploy
post-MB13.b su Proxmox VMID 103 ha rivelato un secondo bug latente:
la VM raggiungeva `[mb12] handing off to user tasks` poi si fermava
senza emettere alcun `[user] exit=0` né `ping`. Indagine: nel path
MB12 first-dispatch invocato da dentro un syscall handler (lo user
chiama `IpcReceive`, l'handler kernel-side esegue `park_until_woken`
→ `yield_current(BlockedOnIpc)` → `enter_user_mode(...)` per
schedulare il prossimo task), `SYSCALL` su x86_64 non commuta `SP`,
quindi il kernel girava sullo *user stack del task uscente*.
`enter_user_mode` eseguiva `mov cr3, dest_cr3` mentre `RSP` puntava
ancora a quello user stack — pagina lower-half non mappata nel nuovo
PML4 → il primo `push {ss}` dell'iretq frame produceva un page-fault
in Ring 0 → triple-fault → VM reset, prima ancora che il sender
potesse eseguire una qualsiasi istruzione Ring 3.

Fix: `crates/omni-kernel/src/bare_metal/usermode.rs` aggiunge un nuovo
parametro `kernel_stack_top: u64` a `enter_user_mode` e fa
`mov rsp, {kstk}` **prima** del `mov cr3`. Lo stack di destinazione
risiede nel range MB10 isolato `[KERNEL_STACK_VA_BASE,
KERNEL_STACK_VA_END)` (PML4 index ≥ `0x180`, kernel half), mirrored
per riferimento in ogni PML4 per-process via la kernel-half clone in
`AddressSpace::new_with_kernel_half`. La VA resta quindi mappata
dall'altra parte del CR3 reload e il successivo build dell'iretq
frame avviene su una pagina valida. Entrambi i call site sono stati
aggiornati: (i) `scheduling.rs::yield_current` first-dispatch (il
`kernel_stack_top = kernel_stack_va + KERNEL_STACK_SIZE` era già
calcolato per il TSS.rsp0 update di MB12.0a, ora viene anche passato
a `enter_user_mode`); (ii) `lib.rs` MB11 single-task dispatch
(`pcb.task.kernel_stack_va + scheduling::KERNEL_STACK_SIZE`). Lo stub
non-x86_64 di `enter_user_mode` è stato aggiornato in tandem.

Il bug era latente già a MB11 in teoria, ma non si manifestava perché
il caller di `enter_user_mode` nel path MB11 era `kmain` direttamente
(RSP su boot stack, che con MB13.b è in upper half mirrored). Solo
MB12 — dove il first-dispatch viene innescato da un syscall handler
che gira sullo user stack del task uscente — rendeva il path
patologico raggiungibile. La pre-esistenza non era visibile nei test
host: `enter_user_mode` ha uno stub `panic!()` su non-x86_64.

Build Info panel aggiornato a Active=`MB13.f iretq kstk-swap`,
Next=`MB13.e PR + intermediate tag`, Track B=`MB1-MB12 OK, MB13.a-f
OK`, Phase 1 ≈ 77%.

Il blocco **MB13.g (comprehensive IDT coverage)** è stato chiuso il
2026-05-19. La pipeline MB13.b/MB13.f aveva risolto i due bug strutturali
del path `enter_user_mode` ma il deploy MB13.f su Proxmox VMID 103 si
fermava ancora subito dopo `iretq` senza emettere alcun tracepoint —
indistinguibile da un triple-fault silente. Root cause del silenzio:
l'IDT installava solo 4 dei 22 vettori sincroni CPU (`#DE=0`, `#DF=8`,
`#GP=13`, `#PF=14`); qualunque altro fault (#SS/#NP/#TS/#UD/...) saltava
a `IdtEntry::missing()` (P=0) → #NP → IDT entry missing → #DF (handler
registrato, ma se il TSS.ist non è configurato e il page-fault avviene
durante il push del frame su uno stack ora unmapped, cascada a triple).

Fix: `crates/omni-kernel/src/bare_metal/idt.rs` aggiunge 16 stub
assembly + 2 handler Rust generici
(`kernel_handle_exception_noerr(vector, frame)` per i vettori
1/2/3/4/5/6/7/16/18/19/20 senza error code,
`kernel_handle_exception_witherr(vector, frame, code)` per i vettori
10/11/12/17/21 con error code). Ogni handler scrive
`[OMNI OS EXCEPTION] vec=NN  code=X  rip=… cs=… rflags=…` sul COM1 e
halt forever. `idt_init()` ora registra TUTTI i 20 vettori coperti
(i due gap 9 e 15 restano `missing()` perché architetturalmente
riservati Intel SDM Vol 3A §6.15). Aggiunto 1 unit test simbolico
`mb13g_synchronous_vectors_covered` che documenta la matrice di
copertura. Workspace test ≥ 444.

Build Info panel aggiornato a Active=`MB13.g full ISR coverage`,
Next=`MB13.h iretq stall fix`, Track B=`MB1-MB12 OK, MB13.a-g OK`,
Phase 1 ≈ 78%. La fix non risolve direttamente l'iretq stall
(richiede deploy + lettura del vec=NN log per identificare il vettore
colpevole — è una fix diagnostica), ma sblocca il root-cause analysis
per MB13.h: il prossimo boot su Proxmox VMID 103 stamperà *quale*
vettore sta cascading a triple-fault, anziché bloccarsi muto.

Il prossimo blocco di lavoro è **MB13.e — chiusura ciclo MB13**:
apertura della PR `feat/kernel-mb11-userspace` → `main`, conformance
CI, scelta del tag intermedio (`v0.2.1` patch o `v0.3.0-alpha.1` minor
— preferenza minor perché c'è una nuova ABI surface), aggiornamento
finale di `progress-omni.md` § 2 + § 4 + spostamento di MB13 da gap
analysis a Done.

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

**Status:** MB1-MB12 ✅ chiuse. Prossimo blocco MB13.

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
| MB10 | Kernel stack isolation + guard page (ADR-0002) | ✅ | `8c1496a` |
| MB11 | Primo userspace process Ring 3 + per-process CR3 (ADR-0004) | ✅ | `22289e1` + `c743173` |
| MB12 | IPC reale (queue + capability stub + multi-task user) (ADR-0005) | ✅ | post-`c743173` |
| MB13.a | `omni-crypto` bare-metal unblock (force-soft SIMD) | ✅ | `2398d5c` |
| MB13.b | Boot-path fix: ET_DYN/PIE kernel + upper-half dynamic mapping | ✅ | `d9a0692` |
| MB13.c | `omni-capability` integration + `Ed25519CapabilityProvider` | ✅ | post-`fd09d1d` |
| MB13.d | `IpcCreateChannel` syscall ABI extension (postcard-encoded signed tokens) | ✅ | `5cb09fa` |
| MB13.f | `enter_user_mode` kernel-stack swap (first-dispatch smoke fix) | ✅ | `f098192` |
| MB13.g | Comprehensive IDT coverage (16 catch-all vectors → no more silent triple-fault) | ✅ | (this commit) |
| **MB13** | **omni-capability integration (Ed25519 verify) — MB13.e PR open** | 🟡 | — |

**Verifica MB1-MB12:**
- `cargo test --workspace --all-features` → **426 pass / 0 fail** (era 393 post-MB11, +33 da MB12 — vedi CHANGELOG `[Unreleased] § Added` riga "Test delta")
- `cargo test -p omni-kernel --all-features` → ~133 unit (lib) + 4 integration
  suites (`mb11_userspace.rs` 6 + `mb12_ipc_cross_process.rs` 8 + `panic_record.rs` 5 + sanity)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean
  (era con blanket allow su omni-kernel pre-Step 7; ora completamente lifted)
- `cargo clippy -p omni-kernel --target x86_64-unknown-none --features bare-metal -- -D warnings` → clean
- `cargo clippy -p omni-kernel --target x86_64-unknown-none --features mb11-userprobe -- -D warnings` → clean (regression)
- `cargo clippy -p omni-kernel --target x86_64-unknown-none --features mb12-userprobe -- -D warnings` → clean
- `cargo build --manifest-path kernel-runner/Cargo.toml --target x86_64-unknown-none --features mb12-userprobe` → bootable image pronta
- `scripts/check-no-blanket-allow.sh` → exit 0 (`scanned 12 crate-root files`)
- Boot QEMU+OVMF (CI ubuntu-24.04, `bootloader_api` 0.11): banner K5 +
  paging validator + IDT + syscall + sched + lapic + `[stack] kernel
  stack VA range = 0xFFFF_C000_… (slot 0)` + mb8-smoke = OK.
- Boot Proxmox VMID 103 (`100.101.77.9`): banner + paging + `[virtio]
  tablet ready` + desktop disegnato sul framebuffer VNC.
- ADR-0001, ADR-0002, ADR-0003, ADR-0004, ADR-0005 → `accepted`.

**Smoke output `mb11-userprobe` (atteso, manual run via QEMU/Proxmox):**
```
[user] userprobe spawned  task_id=N
[user] address space activated cr3 = 0x...
[user] entering Ring 3 rip = 0x40000000
hello
[user] exit=0
```

**Smoke output `mb12-userprobe` (atteso, manual run via QEMU/Proxmox):**
```
[mb12] receiver task_id=N
[mb12] sender   task_id=M
[mb12] channel 1 pre-created
[mb12] handing off to user tasks
ping
[user] exit=0
[user] exit=0
```

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

Tutti i 4 gate K1-K5 di `OIP-Kernel-003` sono chiusi; il CI ha 11
required check su `main` (`cargo fmt`, `cargo clippy`, `cargo test
(ubuntu-24.04)`, `cargo doc`, `DCO sign-off`, `CodeQL — rust`, `cargo
audit`, `cargo deny`, `QEMU boot smoke`, `bare-metal build`,
`kernel-runner build`). `enforce_admins: False` → admin può bypassare.

### 2.4 — Cross-cutting

| Area | Stato |
|---|---|
| `omni-types` (id, errori, versioning) | ✅ P1 chiuso |
| `omni-crypto` (AEAD, sign, KEX, hash, KDF) | ✅ P1; ⏳ `AWAITING_CRYPTO_REVIEW`; ✅ feature `rng`/`bare-metal` introdotti MB12.0c (host-side gating; bare-metal compile bloccato da SIMD ICE → MB13) |
| `omni-capability` (Macaroons + revocation) | ✅ P1 |
| `omni-tee` (TDX/SEV-SNP scaffold + Mock) | 🟡 scaffold, P5.2/5.3 in `[~]` |
| `omni-hal` | 🟡 stub |
| `omni-mesh` | 🟡 stub + handshake spec |
| `omni-runtime`/`omni-sdk`/`omni-agent`/`omni-shell` | 🔵 stub |
| `omni-container` | 🟡 skeleton + KVM TODO (P8) |
| `omni-tokenization` | 🔵 stub |
| `omni-kernel` | 🟢 MB1-MB12 (Ring 3 + per-process CR3 + IPC concreto + multi-task user) |
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
cargo test --workspace --all-features                    426 pass / 0 fail  (era 393 post-MB11)
cargo test -p omni-kernel --all-features                 ~133 unit + 4 integration suites (mb11 + mb12 + panic + sanity)
cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal             clean
cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features mb11-userprobe         clean (regression MB11)
cargo build -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features mb12-userprobe         clean (MB12 boot wiring)
cargo build --manifest-path kernel-runner/Cargo.toml
  --target x86_64-unknown-none --features mb12-userprobe  clean (bootable image MB12)
cargo build -p omni-crypto                                clean (default-features = ["rng"])
cargo test  -p omni-crypto --no-default-features          40 pass (verify-only path)
cargo clippy --workspace --all-targets --all-features -- -D warnings  clean
cargo clippy -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features bare-metal -- -D warnings  clean
cargo clippy -p omni-kernel --target x86_64-unknown-none
  --no-default-features --features mb12-userprobe -- -D warnings  clean
scripts/check-no-blanket-allow.sh                        ok (scanned 12 crate-root files)
cargo audit                                              clean
cargo deny check advisories                              ok
```

**Known limitation (resolved by MB13.a, 2026-05-19):** `cargo build -p
omni-crypto --target x86_64-unknown-none --no-default-features` ora
compila clean. La soluzione (rustflags `--cfg poly1305_force_soft` +
`--cfg chacha20_force_soft` + `--cfg curve25519_dalek_backend="serial"`
+ `--cfg sha2_backend="soft"` nel workspace `.cargo/config.toml`,
combinata con un passthrough Cargo target-scoped su `sha2 0.10` per
i dalek transitive) è preferita all'estrazione di `omni-crypto-verify`
(Alternativa A in ADR-0005) per mantenere stabile l'API surface.

**CI status sui commit post-MB10 (`770c7aa`..`c743173`):** tutti i 11
required check (`cargo fmt`, `cargo clippy`, `cargo doc`, `DCO
sign-off`, `CodeQL — rust`, `cargo audit`, `cargo deny`, `QEMU boot
smoke`, `bare-metal build`, `kernel-runner build`, **nuovo
`blanket-allow-guard`**) attesi verdi alla prossima push. Solo `cargo
test (ubuntu-24.04)` può restare fail per il SIGSEGV preesistente
carryover da v0.2.0 — admin-bypass come per #29/#33 (v. § 4.5
"kernel CI debt").

LOC sorgente Rust del workspace:

- `crates/`                          ~24.300 LOC (post-MB12: +124 MB10
  + ~330 Step 7 reason clauses + ~1.400 MB11 nuovi moduli + ~1.600 MB12
  fra `ipc.rs` impl, `capabilities.rs` extension, `userprobe_mb12.rs`,
  `mb12_ipc_cross_process.rs`, syscall handlers, scheduler wiring)
- `kernel-runner/`                      ~496 LOC (+5 per feature `mb12-userprobe`)
- `disk-image-builder/`                 102 LOC
- `scripts/check-no-blanket-allow.sh`   ~140 LOC (Step 7)
- Totale produzione                 ~25.000 LOC

---

## 4. Cosa manca (gap analysis)

### 4.1 — Kernel (Track B)

1. ~~**MB12 — IPC reale.**~~ ✅ **CHIUSO** (post-`c743173`, ADR-0005).
   `KernelIpcRegistry` concreta in tree con backpressure 3-mode, 4
   syscall handler operativi, capability check via
   `StubCapabilityProvider`, integration test cross-process 8 verdi,
   smoke `mb12-userprobe` pronto. Follow-up reale (Ed25519 verify) →
   MB13.

1bis. **MB13 — `omni-capability` integration reale.** `StubCapabilityProvider`
   è il placeholder MB12; serve swap con un provider Ed25519 reale che
   chiami `omni_capability::CapabilityToken::verify_full`. **MB13.a
   chiuso 2026-05-19** (workspace `.cargo/config.toml` + passthrough
   `sha2 0.10` force-soft): `omni-crypto` ora compila clean su
   `x86_64-unknown-none`. Restano MB13.b (ET_DYN kernel — sblocca smoke
   triple-fault), MB13.c (`omni-capability` come dep di `omni-kernel`),
   MB13.d (`IpcCreateChannel` ABI esteso), MB13.e (PR + tag). Effort
   residuo: ~1-1.5 giornate. ADR-0005 § Migration documenta la sequenza.
2. **TLB shootdown multi-core.** Nessun MP/AP enable; LAPIC è già pronta
   ma il sistema gira su un solo core. Non bloccante per MB12 ma
   sarà necessario prima di P6.7 (driver). MB11 ha previsto questo
   limite: il kernel-half "by reference" di `AddressSpace` diventerà
   un costo cross-AS broadcast con MP. ADR-0004 § Alternative B
   documenta la mitigazione futura.
3. **`map_4k` huge-page split.** Documentato come limite in MB9: oggi
   `map_4k` non splitta una 2 MiB/1 GiB PS=1 entry. Non bloccante finché
   il kernel non riscrive VA in range huge-page mappati dal bootloader,
   ma rischia di mordere quando il driver model entra in scena.
4. **MB11 follow-up minori** (non bloccanti per MB12):
   - **Reale QEMU smoke run di `mb11-userprobe`.** Build verde ma serve
     boot manuale (QEMU+OVMF o Proxmox VMID 103) per registrare il
     log seriale completo `[user] hello / exit=0`. Smoke automatico
     CI deferred (richiede nuovo job che imposta `--features mb11-userprobe`).
   - **`omni-userprobe-helloworld` come crate separato.** MB11.7 ha
     embedded i 167 byte hand-crafted; un crate Rust no_std con
     linker script + `build.rs` ricorsivo produrrebbe lo stesso ELF
     in modo manutenibile. Tracciato in futuro PR.
   - **TSS.rsp0 dinamico sul context switch.** Al momento `tss::set_rsp0`
     esiste ma il context switch path non aggiorna il valore: l'unico
     processo userspace gira fino a `TaskExit`, quindi nessun
     interrupt da Ring 3 → Ring 0 stress test. MB12 (multi-task)
     richiederà l'aggiornamento.
   - **CR3 reload nello scheduler.** `AddressSpace::activate()` esiste
     e viene chiamato solo dal trampolino `enter_user_mode`. Multi-task
     scheduler dispatcher dovrà chiamarlo a ogni switch in user task.
5. **Userspace driver model (P6.7).** NVMe + Ethernet/Wi-Fi + TEE in
   user space. Bloccato da MB12 + MP/AP enable.
6. **External kernel + capability audit (P6.8).** Deliverable di Phase 1,
   bloccato da P4 funding + P6.7 done.

### 4.2 — Crypto / Mesh

7. **`omni-crypto` cryptographer review (P3.2).** Marker
   `AWAITING_CRYPTO_REVIEW` ancora attivo. Tamarin v0.4 chiude P3.1
   spec/proof, ma il review di un cryptographer esterno resta
   bloccato da P4 funding.
8. **`OMNI-PROTO-v0.2` documentation (P7.3).** `docs/protocol/handshake.md`
   § 3.2 negozia ancora `OMNI-PROTO-v0.1`. Il codice è già v0.2
   (`omni-types::version::PROTOCOL_VERSION_V0_2`). Edit-only, 1 PR.
9. **TEE backend reali (P5.2/P5.3).** TDX + SEV-SNP scaffold presenti
   ma `MockTeeBackend` è l'unico operativo. Richiede hardware
   (P4.1 funding).

### 4.3 — Stichting / Governance / Funding

10. **Stichting OMNI registration.** Bylaws + checklist drafted
    (P4.1); registrazione notarile pending.
11. **Funding round.** Pitch deck + 4 grant draft pronti
    (P4.2); submission pending.
12. **Hiring core team.** Job descriptions + salary band drafted
    (P4.4); engagement bloccato dal funding.

### 4.4 — Container & App Mesh

13. **`OIP-Container-006`** rimane Draft. La specifica c'è (KVM micro-VM +
    Wine + cyDock evoluzione), il reference impl `crates/omni-container/`
    è solo skeleton. P8.2-P8.7 bloccati.
14. **`OIP-Helper-007`/`-Pkg-008`/`-Forge-009`/`-Market-010`/`-Flagship-011`**
    tutte Draft, blocked-on OIP-Container e/o decisioni governance.

### 4.5 — Kernel CI debt (post-v0.2.0)

Accumulato durante le 7 iterazioni di CI conformance su PR #29.

15. ~~**Blanket allow su omni-kernel.**~~ ✅ **CHIUSO da Step 7.1-7.4**
    (commit `770c7aa`..`1768966`, 2026-05-18). Ogni blanket
    `#![allow(...)]` non-whitelisted rimosso da `omni-kernel/src/lib.rs`;
    ogni violazione intenzionale ora ha `#[allow(<lint>, reason = "...")]`
    localizzato (site- o module-level). ADR-0003 documenta la policy +
    `scripts/check-no-blanket-allow.sh` la enforce in CI come job
    `blanket-allow-guard` (bloccante).
16. **`cargo test (ubuntu-24.04)` SIGSEGV.** Il binary
    `omni_kernel-…` exit con `signal: 11` al teardown del test
    harness *dopo* che tutti i unit test riportano `ok`. Locale
    macOS arm64 1.85.1 passa. Probabile bug nel drop di
    `bare_metal::paging::tests::TestArena` (raw 256-KiB alloc + manual
    dealloc consumed via `*mut RawPageTable`). Fix: o
    `--test-threads=1` nel workflow, o rifattorizzare l'arena in
    `Arc<Mutex<...>>` o `&'static mut [MaybeUninit<u8>]`. **Ancora
    aperto post-MB11.**
17. **`RUSTFLAGS=` workaround in `qemu-boot-smoke.sh`.** Il job export
    `RUSTFLAGS="-D warnings"` propagava nelle build interne di
    `bootloader 0.11` rompendole. Lo script ora clear `RUSTFLAGS=`
    sulla riga `cargo +nightly run --manifest-path disk-image/`.
    Pattern da conservare per ogni futura `cargo +nightly ...`
    invocation che esegue build script di crate upstream.
18. **Pre-MB9 KNOWN BLOCKER nelle entries CHANGELOG MB8.** La riga
    "Known blocker (MB9)" del 2026-05-17 è ora storica. Non bloccante
    ma andrà annotata come "resolved by MB9" in un prossimo passaggio
    di documentation hygiene.
19. **QEMU smoke automatico per `mb11-userprobe`.** Il job `qemu-boot-smoke`
    valida la banner sequence MB1-MB10. Per asserire le righe
    `[user] hello` / `[user] exit=0` serve un nuovo job CI (o un flag
    `--features mb11-userprobe` su `scripts/qemu-boot-smoke.sh`) con
    set di `EXPECTED_LINES` esteso. Non bloccante per il merge MB11
    ma utile prima di v0.3.
20. **QEMU smoke automatico per `mb12-userprobe`.** Stesso pattern del
    punto 19: nuovo job CI (o flag) che asserisce
    `[mb12] channel 1 pre-created` + `ping` + due `[user] exit=0`
    consecutivi. Anche questo non bloccante per il merge MB12.
21. ~~**Real boot manuale di `mb12-userprobe` (triple-fault).**~~ ✅
    **CHIUSO da MB13.b (2026-05-19).** Root cause: `kernel-runner/.cargo/
    config.toml` forzava ET_EXEC con `-C relocation-model=static` +
    `-C link-arg=--no-pie`, e `kernel-runner/build.rs` aggiungeva
    `--no-pie` *dopo* i flag del target spec via
    `cargo:rustc-link-arg`. `bootloader_api 0.11` non riloccava ET_EXEC,
    quindi il kernel finiva in PML4[0] (`p_vaddr = 0x200000`).
    `AddressSpace::new_with_kernel_half` mirrora solo PML4 256..511,
    quindi il `mov cr3` in `enter_user_mode` perdeva l'istruzione
    successiva → triple fault. Fix: rimossi i flag ET_EXEC dal config
    + rimosso `kernel-runner/build.rs` (il target spec è già PIE su
    Rust 1.83+) + impostato `BOOTLOADER_CONFIG.mappings.
    dynamic_range_start = 0xFFFF_8000_0000_0000`, così `bootloader 0.11`
    rilocca il kernel image, lo stack, il `BootInfo`, il framebuffer e
    il direct-map RAM tutti in upper half. **Validazione smoke Proxmox
    VMID 103 (2026-05-19):** kernel ELF type `DYN (Position-Independent
    Executable file)` (verificato via `readelf -h`); bootloader log
    riporta `virtual_address_offset: 0xffff800000000000` ed `Entry
    point at: 0xffff800000003590`; il boot della build di default
    (desktop demo) raggiunge `[virtio] tablet ready` e renderizza il
    Build Info panel sul framebuffer (Active=`MB13.b ET_DYN upper-half`,
    Next=`MB13.c omni-capability dep`); il boot della build
    `mb12-userprobe` supera il punto di triple-fault precedente e
    raggiunge `[mb12] handing off to user tasks` (vedi nuovo finding § 22).

22. ⚠️ **`mb12-userprobe` user-side serial output missing — parzialmente
    chiuso da MB13.f + process.rs reorder (2026-05-19).** Il bug aveva
    due cause sovrapposte:

    **Causa 1 (chiusa):** `enter_user_mode` eseguiva `mov cr3, dest_cr3`
    mentre `RSP` puntava ancora allo stack del chiamante (lo *user
    stack del task uscente* per i first-dispatch invocati da un syscall
    handler, dato che `SYSCALL` su x86_64 non commuta `SP`). Dopo il
    `mov cr3` quella pagina non era più mappata nel nuovo PML4 e il
    primo `push {ss}` produceva un page-fault → triple-fault.
    *Fix MB13.f:* `enter_user_mode` aggiunge `kernel_stack_top: u64`
    e fa `mov rsp, {kstk}` *prima* del `mov cr3`. Validato via
    tracepoint inline (port 0x3F8) — il blocco asm completo
    (`mov rsp` → `mov cr3` → 5 `push` → `iretq`) ora esegue
    interamente senza fault Ring 0.

    **Causa 2 (chiusa):** `new_with_kernel_half` clona PML4 entries
    256..511 by *value*. Per i nuovi PDPT non ancora presenti nel
    boot PML4 al momento del clone, la condivisione "by reference"
    documentata in ADR-0004 non vale: ogni nuovo PDPT installato
    successivamente nel boot PML4 (es. la prima allocazione di una
    kstk MB10 al PML4 index 0x180) rimane invisibile alle PML4
    cloned precedentemente. Per la PRIMA spawn di processo user, il
    flusso originale (clone PML4 → map kstk via boot mapper) lasciava
    il PML4 cloned con `PML4[0x180] = 0` mentre boot PML4 aveva
    `PML4[0x180] = PDPT_X`. CR3 reload + accesso a kstk_top → #PF.
    *Fix reorder:* `process.rs::spawn_from_elf` ora alloca + mappa
    la kstk *prima* del clone PML4. Il clone successivo cattura il
    PDPT shared, e tutte le kstk successive (slot ≥ 1 within the
    same PDPT) propagano via shared PDPT/PD/PT senza ulteriori
    reorder.

    **Stato residuo (open follow-up MB13.g):** con entrambi i fix, il
    primo `enter_user_mode` raggiunge `iretq` sano (verificato via
    tracepoint 'E' sul COM1) ma la VM si arresta subito dopo senza
    emettere nessuno dei tracepoint installati per debug (`'S'` ad
    `omni_syscall_entry`, `'T'` al `lapic_timer_handler`, `'P'/'G'/'F'`
    agli ISR `#PF`/`#GP`/`#DF`). Il task Ring 3 non esegue nemmeno
    una `jmp $` (testato sostituendo il codice del receiver con
    `0xEB 0xFE`). Ipotesi rimaste: (a) un fault ad `iretq` di vettore
    non gestito (#SS, #NP, #TS, #UD), che cascata a #DF il cui IDT
    entry esiste ma il cui handler non viene raggiunto per qualche
    motivo (TSS.rsp0 o segment-state?); (b) un problema di TLB stale
    sul nuovo CR3 nonostante il reload; (c) un descriptor in GDT che
    diventa unreachable dopo lo swap CR3. Diagnostica più profonda
    richiede `-d int,cpu_reset -D <log>` sui flag QEMU di VMID 103
    (modifica `/etc/pve/qemu-server/103.conf`), non eseguita in
    questo round.

    Validazione del default desktop demo build su Proxmox VMID 103
    confermata 2026-05-19: kernel-runner senza `mb12-userprobe`
    boota fino a `[virtio] tablet ready`, renderizza il framebuffer
    (screenshot 1280×800 catturato via `qm monitor 103 :: screendump`
    contiene pixel data RGB non-zero), Build Info panel riflette
    `Active = MB13.f iretq kstk-swap`, `Track B = MB1-MB12 OK,
    MB13.a-f OK`, `Phase 1 ≈ 77%`.

---

## 5. Prossimi step (priorità ordinata)

### Step 1 ✅ DONE — Merge `feat/kernel-vga-wait` → `main` + release v0.2.0

PR #29 squash-merged in `main` (commit `25790f0`, 2026-05-18). Tag
`v0.2.0` su GitHub release.
[github.com/CySalazar/omni/releases/tag/v0.2.0](https://github.com/CySalazar/omni/releases/tag/v0.2.0).

7 push iterativi di CI conformance + 1 push squashato per DCO + admin
bypass sul `cargo test (ubuntu-24.04)` SIGSEGV. Backup history
pre-squash su `backup/feat-kernel-vga-wait-pre-signoff` (locale).

### Step 2 ✅ DONE — MB10: Kernel stack isolation

PR #33 squash-merged in `main` (commit `8c1496a`, 2026-05-18). ADR-0002
`accepted`. 277 workspace test + 79 unit + 21 integration verdi. QEMU
smoke verde con la nuova diagnostica `[stack] kernel stack VA range =
0xFFFF_C000_… .. 0xFFFF_C800_… (slot 0)`.

### Step 3 ✅ DONE — MB11: Primo userspace process Ring 3

Commit `22289e1` (MB11.1-MB11.6 — foundation) + `c743173`
(MB11.7-MB11.9 — userprobe ELF + boot wiring + integration tests),
sul branch locale `feat/kernel-mb11-userspace`. ADR-0004 `accepted`.

Tutto come pianificato:
- GDT 3 → 7 slot + TSS + STAR fix (`STAR[63:48]=0x10` riconciliato).
- `AddressSpace` con kernel-half clone-by-reference.
- User-stack VA `0x0000_0040_0000_0000` (16 KiB stack + 16 KiB guard).
- `ProcessControlBlock::spawn_from_elf` orchestratore completo.
- `enter_user_mode` (iretq trampoline) + `validate_user_buffer`.
- Syscall handler reali: `TaskExit (11)`, `WriteConsole (60)`,
  `MemMap (1)` stub.
- ELF user-probe 167 byte embedded + `kmain` boot wiring sotto feature
  `mb11-userprobe`.

393 test pass (era 277; +12 unit MB11 + 6 integration). Smoke QEMU/Proxmox
manuale ancora da eseguire (build verde; serial assertion deferred a
job CI dedicato).

### Step 4 ✅ DONE — MB12: IPC concreto + multi-task user-space

Sotto-blocchi MB12.0a → MB12.9 (2026-05-18). ADR-0005 `accepted`.

- **MB12.0a/b**: scheduler dispatch carica TSS.rsp0 + reloads CR3 per
  task user; first-dispatch detection via `context.rsp == 0` → entra
  Ring 3 con `enter_user_mode` invece di `context_switch` asm.
- **MB12.0c**: feature `rng` su `omni-crypto` (host-side gating
  completo; bare-metal compile bloccato da SIMD LLVM ICE su sha2 +
  poly1305 + curve25519-dalek → MB13).
- **MB12.0c'** (pivot): `KernelCapabilityCheck` trait +
  `StubCapabilityProvider` in `capabilities.rs`. Mirror tipi-tipo di
  `omni-capability::Action/Resource` ridotti a IPC.
- **MB12.1+2**: `KernelIpcRegistry` concreta (BTreeMap, NO HashMap) +
  `Channel` + `WakeAction { None | Wake | Block }`. Wait queues per
  canale + capability check 2-livelli.
- **MB12.3**: `principal: KernelPrincipal` + `pending_receive:
  Option<PendingReceive>` nel PCB.
- **MB12.4**: capability gate inline nel registry (no enforce_*
  separato per Phase 1).
- **MB12.5**: 4 syscall handler `IpcCreateChannel/Destroy/Send/Receive`
  in `bare_metal/syscall_entry.rs` con retry-loop pattern su
  `WakeAction::Block`. `task_exit` ora yields invece di halt
  quando ci sono altri runnable.
- **MB12.0f**: due hand-crafted ELFs (`USERPROBE_SENDER_ELF` 179 byte,
  `USERPROBE_RECEIVER_ELF` 197 byte file / 141 in-mem con BSS) in
  `bare_metal/userprobe_mb12.rs`.
- **MB12.6**: boot wiring `mb12-userprobe` feature in `kmain`; forwarded
  dal `kernel-runner`.
- **MB12.7**: `tests/mb12_ipc_cross_process.rs` (8 test host-side).
- **MB12.8**: `docs/adr/0005-mb12-ipc-message-passing.md`.

Output smoke MB12 atteso (manual QEMU+OVMF / Proxmox):
```
[mb12] receiver task_id=N
[mb12] sender   task_id=M
[mb12] channel 1 pre-created
[mb12] handing off to user tasks
ping
[user] exit=0
[user] exit=0
```

426 test pass (era 393 post-MB11).

### Step 5 (parallelo, low-effort) — P7.3 docs

Aggiornare `docs/protocol/handshake.md` § 3.2 a `OMNI-PROTO-v0.2`. Edit-only,
nessun codice; chiude P7 e libera un check verde su `oip-lint`.

### Step 6 (parallelo, governance) — OIP transitions

- `OIP-Bounty-002` e `OIP-Serde-004` Last Call → Active il 2026-05-26
  (entrambe richiedono PR docs + audit log entry per Solo Founder Fast-Track).
- `OIP-Crypto-002` Draft → Review (richiede bibliografia + test vectors).

### Step 7 ✅ DONE — Lift omni-kernel blanket allow

4 commit (`770c7aa` 7.1, `50eddf1` 7.3, `83ff1e8` 7.4, `1768966` 7.2),
2026-05-18. Sui branch locali `chore/kernel-lift-*`. ADR-0003 `accepted`.

- 7.1: lift restriction + rustdoc lints, ~40 siti localized + ADR-0003 +
  `scripts/check-no-blanket-allow.sh` + CI job `blanket-allow-guard`
  (warning).
- 7.3: lift `clippy::pedantic`, ~68 siti (mix fix/allow module-level).
- 7.4: lift `clippy::nursery` + `clippy::cargo`, 7 siti.
- 7.2: lift `unsafe_code` blanket, ~40 cfg-gated bare-metal siti + CI
  `blanket-allow-guard` flipped to blocking. Lands immediatamente
  prima del branch MB11 per minimizzare merge-conflict.

`omni-kernel/src/lib.rs` non porta più alcun blanket `#![allow]`; solo
il `cfg_attr(test, allow(...))` whitelisted da ADR-0003 § Escape
hatches resta come escape hatch ammesso. Guardrail CI bloccante.

### Step 8 (medio termine) — Container P8

Sbloccabile dopo MB12 + MP/AP enable + driver model. `OIP-Container-006`
Draft → Review.

### Step 9 (prossima settimana) — MB13: omni-capability integration reale

Sbloccato da MB12 ✅. Lavoro:

- **Force-soft SIMD**: aggiungere feature `force-soft` su `sha2` +
  `poly1305` + `curve25519-dalek` nel workspace per sbloccare
  `omni-crypto` su `x86_64-unknown-none`. Alternativa: estrarre un
  crate `omni-crypto-verify` con solo `OmniVerifyingKey::verify` +
  `domain_separated_hash` come API.
- **`omni-capability` come dep di `omni-kernel`** con
  `default-features = false` + propagation `bare-metal`.
- **`Action::IpcSend/IpcRecv` + `Resource::IpcChannel(u64)`** in
  `omni-capability::scope` (variants `#[non_exhaustive]` →
  semver-safe).
- **`Ed25519CapabilityProvider`** che chiama
  `CapabilityToken::verify_full`; sostituisce `StubCapabilityProvider`
  nel boot wiring. `KernelCapabilityCheck` ha già la shape compatibile
  (MB12.0c').
- **`IpcCreateChannel` syscall ABI esteso**: accetta due pointer
  postcard-encoded `(send_token_ptr, recv_token_ptr)` opzionali.
  Aggiornare i userprobe ELFs di test integration MB13 + un nuovo
  `tests/mb13_capability_signed.rs`.

Effort stimato: 1-2 giornate (gating SIMD + glue + nuovi test).

Inoltre **MB13 deve includere il fix per il triple-fault smoke MB12**
(vedi gap analysis § 21). Approccio raccomandato: forzare `ET_DYN`
sul kernel-runner ELF (`relocation-model=pic` + `-pie` + verificare che
`bootloader 0.11` applichi `dynamic_range_start = 0xFFFF_8000_0000_0000`).
Se non praticabile, fallback con linker script che imposta `p_vaddr`
del kernel ELF in upper half.

---

## 6. Allineamento con la roadmap

| Roadmap | Stato attuale |
|---|---|
| **Phase 0 — Foundation (mesi 0-6)** | ~75% (governance ✅, foundational crates ✅, OIP process ✅, funding/legal in corso) |
| **Phase 1 — Microkernel POC (mesi 6-18)** | ~72% (boot ✅, paging ✅, scheduler ✅, syscall ✅, ELF loader ✅, kernel-stack isolation ✅, userspace Ring 3 + per-process CR3 ✅, **IPC concreto + multi-task user ✅ MB12**, **bare-metal smoke unblocked ✅ MB13.b** (ET_DYN/PIE kernel, upper-half mapping), **Ed25519CapabilityProvider ✅ MB13.c** (verify-only + signature/time/TEE binding, drop-in compatibile con `KernelCapabilityCheck`); mancano syscall ABI extension (MB13.d), driver model (P6.7), audit (P6.8)) |
| **Phase 2 — AI Runtime + Tier 0** | 0% (bloccato da Phase 1) |
| **Phase 3-7** | 0% |

I deliverable Phase 1 della roadmap (`docs/06-roadmap.md` § "Phase 1"):

- ✅ "Microkernel boots on x86_64 hardware" (QEMU+OVMF + VirtualBox + Proxmox).
- ⚠️ "with Intel TDX or AMD SEV-SNP" — TDX/SEV-SNP scaffolding c'è (`omni-tee`),
  ma nessun real boot su hardware TEE-capable: pending Phase 1.5 + hardware.
- ✅ **"IPC primitives operational (typed message passing)"** — MB12:
  `KernelIpcRegistry` con `BackpressurePolicy::{Block,Drop,EvictOldest}`,
  4 syscall handler (`IpcCreateChannel/Destroy/Send/Receive`), wait
  queues per canale, cross-process integration test.
- ⚠️ "Capability-based security primitives implemented" — `omni-capability`
  c'è (43 unit + 7 integration test) ma non integrato nel kernel per
  via del blocker SIMD su `omni-crypto` bare-metal. MB12 ha consegnato
  uno `StubCapabilityProvider` interno (subject byte-compare + action
  shape-match, no Ed25519). MB13 swappa con il provider reale.
- ✅ "Memory management, scheduling, interrupt handling" (MB1-MB3 + MB6-MB10).
- ✅ **"Ring 3 userspace + per-process address space isolation"** (MB11).
- ⬜ "Drivers (in user space): NVMe storage, Ethernet/Wi-Fi networking, TEE"
  → P6.7 (sbloccato da MB12 ✅; richiede ancora MP/AP enable + capability
  Ed25519 reale MB13).
- ✅ "Boot loader (UEFI-based)" — `bootloader` 0.11+ + `kernel-runner`
  (OIP-Kernel-005 Active).
- ⚠️ "Minimal shell sufficient for development" — il desktop demo (Track A)
  ha un terminal echo ma non un REPL; userprobe MB11 dimostra Ring 3
  funzionante, una shell user-space proper è work post-MB12 (richiede IPC
  per pty + filesystem).
- ⬜ "No AI yet — focus on a solid kernel foundation" — rispettato.
- ⬜ "First external security audit of kernel + capability system" → P6.8,
  bloccato da P4 funding + P6.7 done.

**Conclusione:** la roadmap Phase 1 è on-track con un'accelerazione
significativa (MB10 + Step 7 + MB11 + **MB12** chiusi nella stessa
giornata; +41 test workspace; ADR-0005 `accepted`). Il prossimo collo
di bottiglia tecnico è **MB13** (`omni-capability` reale →
feature-gating SIMD su `sha2`/`poly1305`/`curve25519-dalek`); il
prossimo collo di bottiglia non-tecnico resta il funding Phase 0.

---

## 7. Rischi & blocker

| Rischio | Probabilità | Impatto | Mitigation |
|---|---|---|---|
| `bootloader_api` 0.12 rompe il direct-map | media | alta | Pinning a `=0.11.X` in `kernel-runner/Cargo.toml` (OIP-Kernel-005 § S9). Validator MB9 segnala automaticamente "skipped M MiB" se l'invariante decade. |
| ~~Stack overflow nel kernel passa inosservato~~ | ~~alta~~ → bassa | ~~alta~~ | ✅ MB10 chiuso: guard page → `#PF` deterministico con `CR2` sul serial. |
| ~~User process può corrompere kernel memory~~ | ~~alta~~ → bassa | ~~alta~~ | ✅ MB11 chiuso: per-process CR3 + `PTE_USER` hardware paging + `validate_user_buffer` su syscall. |
| ~~Blanket allow su omni-kernel maschera bug futuri di lint~~ | ~~media~~ → bassa | ~~media~~ | ✅ Step 7 chiuso: ADR-0003 + CI guardrail bloccante `blanket-allow-guard`. |
| Cryptographer review non si chiude in tempo per Phase 2 | alta | alta | Tamarin v0.4 chiude la metà spec; cercare review pro-bono se P4 funding ritarda. |
| `OIP-Kernel-005` (kernel-runner) dipende da single contributor | alta | media | Documentazione esiste; pinning versione bootloader; CI smoke gate. |
| Hardware TEE acquisition (Intel TDX / AMD SEV-SNP) | alta | media | Cloud TEE è alternativa (Azure Confidential VMs); decision deferred a Phase 1 mid-point. |
| Proxmox manual deploy step non scalabile | media | bassa | Documentato in `reference-proxmox-deploy`; valutare automation script in Step 3-4. |
| `cargo test (ubuntu-24.04)` SIGSEGV blocca i futuri PR sul required check | alta | media | Carryover preesistente; mergiato via admin bypass su PR #29 e #33. Fix: rifattorizzare `TestArena` di `paging.rs` o `--test-threads=1`. |
| **STAR/GDT/iretq selector aritmetica errata** | media | alta | MB11.1 ha riconciliato (`STAR[63:48]=0x10` → CS=0x23, SS=0x1B). Unit test `sysret_arithmetic_matches_intel_sdm` lo enforza. |
| **Kernel-half shared by reference vs MP** | media (Phase 2+) | media | ADR-0004 § Alt B documenta la strategia full-clone per Phase 2 quando l'enable MP/AP arriva. Non bloccante Phase 1. |
| **MB12 capability stub vs Ed25519 reale** | bassa (oggi) → media (MB13) | media | ADR-0005 § Migration: il trait `KernelCapabilityCheck` è swap-in compatibile con il futuro `Ed25519CapabilityProvider`. `StubCapabilityProvider::verify` autorizza qualunque token con action/resource shape match — sufficiente in dev mode, **non** in production. Blocker tracciato come MB13. |
| **`omni-crypto` SIMD LLVM ICE su `x86_64-unknown-none`** | alta | media | Scoperto in MB12.0c. Soluzione MB13: `force-soft` feature su `sha2`+`poly1305`+`curve25519-dalek` oppure crate `omni-crypto-verify` separato. ADR-0005 § Alternative A. |
| **BumpHeap no-free per canali IPC distrutti** | media | media | Documentato in ADR-0005 § Negative. Cap raccomandato `queue_depth ≤ 256` per canale. Slab/free-list allocator → OIP separato (Phase 2). |

---

## 8. Riferimenti

- Roadmap: [`docs/06-roadmap.md`](docs/06-roadmap.md)
- ADR MB9: [`docs/adr/0001-mb9-paging-huge-page-aware.md`](docs/adr/0001-mb9-paging-huge-page-aware.md)
- ADR MB10: [`docs/adr/0002-mb10-kernel-stack-isolation.md`](docs/adr/0002-mb10-kernel-stack-isolation.md)
- ADR Step 7 policy: [`docs/adr/0003-no-blanket-allows-in-production-crates.md`](docs/adr/0003-no-blanket-allows-in-production-crates.md)
- ADR MB11: [`docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md`](docs/adr/0004-mb11-userspace-ring3-per-process-cr3.md)
- ADR MB12: [`docs/adr/0005-mb12-ipc-message-passing.md`](docs/adr/0005-mb12-ipc-message-passing.md)
- Guardrail script: [`scripts/check-no-blanket-allow.sh`](scripts/check-no-blanket-allow.sh)
- Plan OIP-Kernel-003: [`docs/plans/oip-kernel-003-activation.md`](docs/plans/oip-kernel-003-activation.md)
- Changelog: [`CHANGELOG.md`](CHANGELOG.md)
- OIP index: [`oips/README.md`](oips/README.md)
- Todo dettagliato: [`todo.md`](todo.md)
- GitHub release v0.2.0: [github.com/CySalazar/omni/releases/tag/v0.2.0](https://github.com/CySalazar/omni/releases/tag/v0.2.0)

---

*Report aggiornato manualmente dallo stato del repository post-`c743173` sul branch locale `feat/kernel-mb11-userspace` (post v0.2.0 release, MB10 merge, Step 7.1-7.4 lift, MB11.1-MB11.9 closure, **MB12.0a-MB12.9 closure con ADR-0005**). Aggiornare a ogni milestone closure.*
