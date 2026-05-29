# Development Plan — 2026-05-29 — Phase 2 Sprint 11.a "Serving Foundations"

> Status: **Approved** (founder, 2026-05-29) — parallel implementation set.
> Methodology: D.O.E. (Directive → Orchestration → Execution).
> Frontier: HEAD `fe6a358` (Phase 2, AI Runtime, Sprint 10 closed).

## 1. Context

Sprint 10 closed speculative decoding, GQA+RoPE, and continuous batching in
`omni-runtime`. The Build Info panel advertises `Next = P2 Sprint 11: model
serving`. This plan formalizes the first slice of Sprint 11 ("Serving
Foundations") as a set of **four mutually-independent, parallelizable tasks**.

The development-planner identified six candidates (S11.A–F); the founder
approved the subset **A + B + C + D** (no ADR reversal, security-neutral or
security-positive, all Phase 2).

## 2. Anti-conflict strategy

Only `crates/omni-runtime/src/lib.rs` is shared. A *scaffolding commit* adds the
two module declarations (`pub mod serving;`, `pub mod audit;`) and the stub
files **before** parallel work starts. Consequently:

| Task | Files owned (sole writer) |
|------|---------------------------|
| S11.A | `crates/omni-runtime/src/serving.rs`, `crates/omni-runtime/tests/e2e_sprint11_serving.rs` |
| S11.B | `crates/omni-runtime/src/audit.rs` |
| S11.C | `crates/omni-runtime/tests/e2e_tokenization_inference.rs` |
| S11.D | `crates/omni-runtime/src/lib.rs` (`mod router` section only) |

No two tasks write the same file. Each implementer runs in an isolated git
worktree branched from the scaffolding commit and emits a patch; the
orchestrator applies the patches in order A → B → C → D.

## 3. Tasks

### TASK-S11.A — Inference Session Lifecycle + Request/Response API
- **Crate/module:** `omni-runtime::serving` (new `src/serving.rs`).
- **Deliverables:** `InferenceSession` (state machine Open/Active/Closing/Closed),
  `SessionManager` (`open_session` / `close_session` / `submit` / `stream_tokens`),
  wire types `InferenceRequest`/`InferenceResponse`/`StreamChunk` (postcard via
  `omni_types::wire`), integration with `batch::BatchScheduler`.
- **Acceptance:** ≥8 unit tests (lifecycle, unique session id, capability
  rejection, double-close failure, stream backpressure); E2E
  `tests/e2e_sprint11_serving.rs` (3 concurrent requests → schedule → stream →
  close, FIFO per equal priority); `cargo fmt`, `clippy -D warnings`,
  `cargo test -p omni-runtime`, `cargo doc -p omni-runtime` green.
- **Security:** capability check on every entry point; session id via CSPRNG; no
  raw PII in `InferenceRequest`.

### TASK-S11.B — Inference Audit Log
- **Crate/module:** `omni-runtime::audit` (new `src/audit.rs`).
- **Deliverables:** `AuditRecord { timestamp_ns, session_id, capability_id,
  model_id, tier, input_token_count, output_token_count, latency_us, status }`;
  `AuditLog` trait + `InMemoryAuditLog` (ring buffer cap `MAX_AUDIT_RECORDS =
  16384`, drop-oldest); postcard serialization; query by session/model.
- **Acceptance:** ≥6 unit tests + proptest round-trip + doc test; standard gates.
- **Security:** metadata only, no PII captured. Security-positive (replaces
  ephemeral `tracing` with durable structured records).

### TASK-S11.C — PII E2E integration test (tokenization ↔ runtime)
- **Crate/module:** `omni-runtime/tests/e2e_tokenization_inference.rs` (new, test-only).
- **Deliverables:** synthetic-PII corpus (zero real PII) → `TokenizationService`
  (GDPR / HIPAA / PCI-DSS presets) → pipeline request → assert no raw PII reaches
  the pipeline → detokenize response.
- **Acceptance:** ≥4 tests (one per preset + fail-closed redact-on-sight); explicit
  "no raw PII in pipeline request" assertion (regex); standard gates.
- **Security:** closes a test gap; regression guard against vault bypass.

### TASK-S11.D — Real `TierRouter` policy + observable decision
- **Crate/module:** `omni-runtime::router` (extend `mod router` in `lib.rs`).
- **Deliverables:** `RoutingPolicy { allow_tier_1, allow_tier_2,
  max_model_size_bytes, require_attestation }`; `TierDecision { tier, reason,
  decided_at_ns }`; `TierError`; new method `route_decision(&req, &policy) ->
  Result<TierDecision, TierError>` returning `TierUnavailable` for Tier 1/2 (Phase
  2 contract, OIP-Phase2-Entry-021 § S2.1). **The existing `route()` method and
  its tests MUST remain unchanged (backward-compatible, additive only).**
- **Acceptance:** ≥6 unit tests (default Tier 0; large model → `TierUnavailable`;
  attestation required → fail when none); existing router tests still green;
  standard gates.
- **Security:** makes the routing decision explicit and auditable.

## 4. Non-parallelizable (excluded from this set)
S11.E (batched matmul) — would reverse ADR-0022 (deferred to Phase 4); excluded.
S11.F (handshake doc) — deferred. `omni-mesh` (Phase 4), `omni-fs` persistence
(Phase 3), full "model serving" spec — out of scope for this slice.

## 5. Execution gates (per task, before patch emit)
`cargo fmt --check` · `cargo clippy -p omni-runtime --all-targets -D warnings` ·
`cargo test -p omni-runtime` (and `-p omni-tokenization` for C). The whole-workspace
`cargo test` is avoided on dev hosts due to the known `omni-kernel --lib` SIGSEGV
(host-only, tracked separately).

## 6. Post-integration (orchestrator)
1. Apply patches A → B → C → D.
2. Run workspace fmt + clippy + targeted tests; recompute test count.
3. Update Build Info panel (`bare_metal/demo.rs`): `Active = P2 Sprint 11.a serving`,
   `Next = P2 Sprint 11.b serving wiring`, refreshed test count.
4. Update `CHANGELOG.md`, `progress-omni.md`, `todo.md`.
5. Deploy smoke validation to Proxmox VMID 103 (100.101.77.9).
6. Commit with DCO sign-off, no AI attribution (per CLAUDE.md policy).
