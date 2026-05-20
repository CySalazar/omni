# Piano: P6.7.8.9 — Capability Deposit Trampoline

## Context

P6.7.8.8 (commit `c10c92c`, 2026-05-20) ha chiuso il syscall handler
`DriverLoad (73)` end-to-end:

1. Decode + verify del `omni-pack v1`.
2. `KNOWN_ISSUERS` lookup + Ed25519 signature check del manifest.
3. BLAKE3 hash check sull'immagine ELF.
4. `spawn_from_elf` → driver process entra in scheduler con `principal = ZERO`.

Step 8 di **OIP-013 § S5.3** è esplicitamente deferito:

> For each requested capability, mint an attenuated child token bound to
> the new driver process's NodeId, scope from the manifest, lifetime = 90
> days. These tokens are pre-installed in the driver's initial capability
> namespace so it does not need to perform discovery.

**Conseguenza operativa.** Oggi un driver caricato raggiunge `_start` ma
le sue chiamate `MmioMap` / `DmaMap` / `IrqAttach` falliscono con
`EACCES` perché non possiede capability token firmati da presentare a
`Ed25519CapabilityProvider::verify_signed_token`.

P6.7.8.9 chiude il gap minando lato kernel i token necessari e mappando
una pagina read-only nello spazio del driver con il payload depositato.

---

## Decisioni di design (approvate dal founder 2026-05-20)

### D1. CSPRNG architecture (2-phase)

- **Phase 1 (early boot).** Modulo nuovo `omni-kernel::entropy` con:
  - `seed_from_hw_32() -> [u8; 32]` — raccoglie 32 byte da `RDRAND`
    (CPUID 1/ECX bit 30 gated, 10 retries per 64-bit chunk) XOR
    `RDTSC` (mixed con SplitMix64 finalizer come in `kaslr.rs`).
  - `KernelCsprng` wrapper su `ChaCha20Rng` da `rand_chacha 0.3` +
    `rand_core 0.6` (no default features, no `getrandom`).
  - `static KERNEL_CSPRNG: spin::Mutex<Option<KernelCsprng>>` —
    inizializzato lazy alla prima `with_csprng(|rng| …)` call, oppure
    eagerly da `kmain` dopo `set_phys_offset`.
- **Phase 2 (post-boot reseeding).** API pubbliche su `KernelCsprng`:
  - `pub fn add_entropy(&mut self, bytes: &[u8])` — XOR su `state[..32]`
    + `rng.reseed(state)`. Da chiamare dagli IRQ handler e dai
    network driver una volta il sistema è up.
  - `pub fn reseed(&mut self, new_seed: [u8; 32])` — replace totale del
    seed. Riservato a operazioni di re-key esplicite (es. wake-from-S3).
  - TODO + commento architetturale che documenta il path Phase 2 (non
    implementato in P6.7.8.9, ma API pronta).

**Decisione bypass `mint` feature.** Per evitare di trascinare
`omni-types/id-generation` (e quindi `getrandom`) sul kernel
bare-metal, il kernel costruisce direttamente:

```rust
let payload = TokenPayload {
    id: CapabilityId::from_bytes(kernel_csprng_16()),
    subject: NodeId::from_attestation_hash(provider.node_id_bytes),
    issuer: kernel_signing_key.verifying_key(),
    parent: None,
    scope,
};
let token = CapabilityToken::sign_payload(&kernel_signing_key, payload)?;
```

`CapabilityId::from_bytes` è `pub const fn` non-feature-gated;
`CapabilityToken::sign_payload` è unconditional (no `#[cfg(feature)]`).
Risultato: nessuna modifica a `omni-capability/Cargo.toml`; `force-soft`
su `sha2`/`poly1305`/`curve25519-dalek` resta intatto.

### D2. Kernel Ed25519 signing key

Statico baked-in:

```rust
// crates/omni-kernel/src/driver_cap_issuer.rs
pub const DRIVER_CAP_ISSUER_SEED: [u8; 32] = [
    /* 32 byte deterministic — vedi commento "DEV ONLY" sotto */
];

pub fn kernel_signing_key() -> OmniSigningKey {
    OmniSigningKey::from_bytes(DRIVER_CAP_ISSUER_SEED)
}
```

**DEV ONLY.** Il seed è hard-coded a un valore non-segreto per Phase 1
(stesso pattern del fixed all-zero `node_id_bytes` placeholder in
`Ed25519CapabilityProvider`). Sostituzione con TEE-derived sealing key
deferita a Phase 2 / P5.2 (TDX TDREPORT-based key derivation) +
OIP-follow-up per il key custody policy.

Il `KNOWN_ISSUERS` table per la verifica driver-manifest è **separato**
da questa kernel-signing key: gli issuer-driver firmano l'omni-pack
(verified al `DriverLoad`); il kernel-signing-key firma i capability
token che il kernel deposita nel driver process (verified al
`MmioMap`/`DmaMap`/`IrqAttach`).

### D3. Deposit ABI — well-known user-VA slot

**VA scelto:** `0x0000_0000_0010_0000` (1 MiB). **8 pagine consecutive
da 4 KiB = 32 KiB totali** read-only (`PTE_PRESENT | PTE_USER |
PTE_NO_EXEC`, no `PTE_WRITABLE`).

**Sizing.** 64 entries × ~154 byte/token postcard-encoded = ~9.8 KiB +
1 KiB header. 32 KiB lascia ~21 KiB di margine per token più grandi
del worst case (e.g. caveat lunghi).

Justification:
- Below `0x0000_0000_0040_0000` (ELF entry default per
  `bootloader_api 0.11` user range, vedi `kernel-runner/src/main.rs`).
- Non collidente con stack utente (`0x0000_0040_0000_0000`),
  MMIO PML4 slot (`0x0000_0080_0000_0000`), o ELF text.
- 1 MiB è un valore facilmente riconoscibile in dump esadecimali e
  resta lontano dal NULL-page guard convenzionale.

**Header binary format (4 KiB pagina, layout fisso):**

```
Offset  Size      Field
─────── ───────── ─────────────────────────────────────────────────
0x000   8 bytes   magic              = b"OMNICAPS"
0x008   4 bytes   version            = 1u32
0x00C   4 bytes   entry_count        N (0..=64)
0x010   N*16 byt  entries[N]:
                    u32 action_tag   (1=MmioMap, 2=DmaMap, 3=IrqAttach,
                                      4=PciConfigRead, 5=PciConfigWrite)
                    u32 resource_tag (1=MmioRegion, 2=DmaWindow,
                                      3=IrqLine, 4=PciDevice, 5=Any)
                    u32 token_offset (from page start; aligned to 8)
                    u32 token_len    (postcard-encoded bytes)
…       …         token_blobs[N]   postcard-canonical
                                    CapabilityToken bytes, packed
0xFFF   …         padding (zero)
```

**Why a flat indexed layout vs a Vec<u8>:** il driver scansiona
sequenzialmente le entry per shape-match (`Action::MmioMap` cerca la
prima entry con `action_tag == 1` che `is_subset_of` la sua richiesta).
Un layout JSON / postcard di livello superiore richiederebbe un
deserializer no_std nel driver runtime; il layout flat indexed è
parsable con `unsafe { *ptr_cast }` senza alcuna dipendenza.

**Compatibility footprint:** lo SDK driver (`omni-driver-net-virtio`,
`omni-driver-nvme`, `omni-driver-e1000e`) avrà un nuovo helper
`omni_driver_shared::caps::find_token(action_tag, resource_predicate)
-> Option<&[u8]>`. Phase 1 lo aggiungiamo come modulo no_std libero
dentro un nuovo crate `omni-driver-shared` (oppure embedded nei driver
crates esistenti — TBD se P6.7.8.9 lo emette).

### D4. Lifetime / scope-from-manifest mapping

Per ogni `Resource` in `DriverCapabilities`:
- `mmio_regions: Vec<Resource>` → un token per regione, `action =
  Action::MmioMap`.
- `dma_windows` → token per window, `action = Action::DmaMap`.
- `irq_lines` → token per IRQ, `action = Action::IrqAttach`.
- `pci_devices` → due token per device (`PciConfigRead` +
  `PciConfigWrite`).

**Time window:**
- `not_before = boot_seconds` (current `rtc_seconds()` at deposit time).
- `not_after = boot_seconds + 90 * 86_400` (90 giorni in secondi).

Cap massimo total entries: 64 (cover `OIP-Driver-Net-015` /
`OIP-Driver-NVMe-014` worst-case manifests + margine).

### D5. Error paths

`deposit_for_driver` ritorna `Result<(), DepositError>`:
- `TokenCountExceeded` (> 64 entries) → driver spawn aborts in
  `DriverLoad`, kernel logs + `ENOSPC` to user.
- `TokenEncodingFailed` → unrecoverable (canonical encode su una
  `CapabilityToken` valida non dovrebbe fallire); propagato come
  `EINVAL`.
- `MapFailed` (page-table allocation failure) → `ENOSPC`.
- `ScopeBytesOverflow` (token blob > 768 byte → impossibile fittare 64
  entry in 4 KiB) → `EINVAL`.

Tutti gli error path rollback completi: nessun token depositato, ELF
non rimosso ma TaskId rimosso dalla scheduler queue prima di Ring 3
entry (`scheduler.cancel_spawn` da implementare se non esiste, vedi
sub-task implementation).

### D6. Tests

Host-side (cargo test, `cfg(test)`):
- `kernel_csprng_seed_extracts_32_bytes` — mock RDRAND/RDTSC paths,
  verifica che `seed_from_hw_32()` ritorna 32 byte distinti su due
  call consecutive.
- `kernel_csprng_chacha20_round_trip` — seed → draw → re-seed con
  stesso seed → stesso output (determinismo).
- `kernel_csprng_add_entropy_changes_state` — `add_entropy(b"x")`
  cambia l'output successivo.
- `deposit_page_header_layout_matches_oips_013` — encode una
  `DriverCapabilities` con 3 MMIO + 1 DMA + 2 IRQ, verifica magic /
  version / count / offsets.
- `deposit_minted_token_verifies_against_provider` — token minato
  passa `Ed25519CapabilityProvider::verify_signed_token` con
  `node_id_bytes = [0u8; 32]` placeholder e `now ∈ [not_before,
  not_after)`.
- `deposit_token_count_exceeded_returns_error` — manifest con 65
  resource ritorna `TokenCountExceeded`.

Bare-metal (`#[cfg(all(feature = "bare-metal", target_os = "none"))]`):
- Stub in `bare_metal::driver_load_handlers::tests` per verificare che
  il path `DriverLoad → deposit_for_driver → return Ok` produce un
  `mmio_va_cursor = 0` (lazy KASLR ancora intatto, deposit non lo
  randomizza) + `pcb.cap_deposit_va == DRIVER_CAP_DEPOSIT_VA`.

### D7. Build Info

- `Active`: `P6.7.8.9 cap deposit trampoline` (cyan).
- `Next`: `P6.7.8.10 driver-shared SDK helper` (oppure direttamente
  `P6.7.9 driver bringup smoke` se SDK helper inline nei driver crate).
- `Phase`: `1 - Microkernel POC  (~99.9%)`.
- `Tests`: +~12 new tests, target `879 workspace pass` (era 867).

---

## File inventory

### Nuovi

- `crates/omni-kernel/src/entropy.rs` — Phase 1+2 CSPRNG.
- `crates/omni-kernel/src/driver_cap_issuer.rs` — kernel signing key.
- `crates/omni-kernel/src/cap_deposit.rs` — header layout, encoder,
  `deposit_for_driver(pcb, manifest, address_space) -> Result<()>`.

### Modificati

- `crates/omni-kernel/Cargo.toml` — `rand_core 0.6` + `rand_chacha 0.3`
  (`default-features = false`) come dependencies bare-metal.
- `crates/omni-kernel/src/lib.rs` — pub use dei nuovi moduli.
- `crates/omni-kernel/src/process.rs` — `ProcessControlBlock`
  estesa con `cap_deposit_va: Option<u64>` (None per processi senza
  deposit, e.g. mb11/mb12 userprobe).
- `crates/omni-kernel/src/bare_metal/syscall_entry.rs` —
  `driver_load_handlers::driver_load` chiama `deposit_for_driver`
  prima del `return SyscallReturn::ok(task_id.0)`; sub-CR3 switch al
  deposito è necessario perché il driver AS è già attivato in
  `spawn_from_elf`.
- `crates/omni-kernel/src/bare_metal/demo.rs` — Build Info Active /
  Next / Phase / Tests update.
- `crates/omni-kernel/src/bare_metal/mod.rs` — `DRIVER_CAP_DEPOSIT_VA`
  + `DRIVER_CAP_DEPOSIT_LEN` constants.
- `scripts/check-no-blanket-allow.sh` — no change (count resta 15).

### NON modificati (decisione bypass `mint`)

- `crates/omni-capability/Cargo.toml`
- `crates/omni-crypto/Cargo.toml`
- `crates/omni-types/Cargo.toml`

---

## Workflow di sviluppo

1. Implementare `entropy.rs` + test host-side (no behaviour change).
2. Implementare `driver_cap_issuer.rs` + test che round-trippa
   `OmniSigningKey::from_bytes / verifying_key` su seed costante.
3. Implementare `cap_deposit.rs` (encoder + mint + map) con host-side
   stub (`#[cfg(not(target_os = "none"))]` returns
   `Err(DepositError::HostStub)`).
4. Wirare `deposit_for_driver` nel `driver_load` handler.
5. Aggiornare Build Info.
6. Run completo gate set:
   - `cargo clippy --workspace --all-features --all-targets -- -D warnings`
   - `cargo clippy --target x86_64-unknown-none --features bare-metal -p omni-kernel -- -D warnings`
   - `cargo clippy -p kernel-runner --target x86_64-unknown-none -- -D warnings`
   - `cargo clippy -p omni-driver-net-virtio-image --target x86_64-unknown-none -- -D warnings`
   - `cargo clippy -p omni-driver-nvme-image --target x86_64-unknown-none -- -D warnings`
   - `cargo clippy -p omni-driver-e1000e-image --target x86_64-unknown-none -- -D warnings`
   - `cargo fmt --all -- --check`
   - `bash scripts/check-no-blanket-allow.sh`
   - `RUSTDOCFLAGS=-D warnings cargo doc -p omni-kernel --features bare-metal --target x86_64-unknown-none --no-deps`
   - `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features`
   - `python3 scripts/lint-oips.py`
   - `cargo test --workspace --all-features -- --test-threads=1`
7. Commit + push + Proxmox VMID 103 smoke (boot sequence
   `[mb14.a] → [mb14.h.2] → [virtio] tablet ready`, Build Info panel
   render correctly).

---

## Acceptance criteria

- [ ] 879+ workspace test pass (`--test-threads=1`).
- [ ] Zero clippy warning su tutti i target listati sopra.
- [ ] Build Info panel post-boot mostra `Active = P6.7.8.9 cap deposit
      trampoline`, `Phase = 1 - Microkernel POC (~99.9%)`,
      `Tests = 879 workspace pass`.
- [ ] Driver smoke (anche solo manuale per ora — nessun driver è ancora
      bring-up-capable, l'integrazione end-to-end con `MmioMap` reale è
      P6.7.9): un test mock host-side dimostra che un token minato +
      depositato passa la verifica di `verify_signed_token` lato kernel.
- [ ] Pre-existing `cargo test -p omni-kernel --lib` SIGSEGV resta
      carryover (mitigato da `--test-threads=1`).

---

## Open follow-up (out of scope P6.7.8.9)

- **P6.7.8.10** — SDK helper `omni_driver_shared::caps::find_token` +
  rifattorizzazione driver crates per consumarlo.
- **P5.2 follow-up** — sostituire il fixed `DRIVER_CAP_ISSUER_SEED`
  con una TEE-derived sealing key + OIP per il key custody policy.
- **Phase 2 CSPRNG reseed activation** — wirare `add_entropy` da IRQ
  handler e da network driver post-bringup.
