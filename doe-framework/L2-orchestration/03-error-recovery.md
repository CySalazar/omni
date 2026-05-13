# Error Recovery — Gestione Errori e Recovery a Cascata

## Metadata

- **ID:** DIR-007
- **Version:** 1.0
- **Last updated:** 2026-03-24
- **Depends on:** DIR-005 (Decision Engine)
- **Project type:** universale
- **Status:** active
- **Superseded by:** —

## Objective

Define a deterministic, cascading error classification and recovery system that ensures
the agent handles every failure mode with a predictable, documented protocol — eliminating
improvisation, preventing silent failures, and guaranteeing that the user is involved
exactly when needed.

## Pre-conditions

- The agent is executing a task within an active project (project-state.md exists or
  is being created).
- The Decision Engine (DIR-005) is the entry point that routes errors to this protocol.
- The Interaction Protocol (DIR-008) is available for user communication when required.

## Input

| Input | Type/Format | Source | Required |
|-------|-------------|--------|----------|
| Error or failure event | Exception, exit code, unexpected output, or logical inconsistency | Runtime, test execution, tool output, or agent reasoning | yes |
| Current task context | TASK-ID, module, step | project-state.md or in-memory state | yes |
| Project state | Structured Markdown | project-state.md | yes (if exists) |
| Error history for current task | List of previous errors on this task | In-memory session state | no |

---

## Error Classification — The E1-E5 Scale

Every error the agent encounters MUST be classified into one of five classes before
any recovery action is taken. The class determines the recovery protocol, the level
of user involvement, and the impact on the project state.

### Classification Matrix

| Class | Name | Severity | User Involvement | Examples |
|-------|------|----------|-----------------|----------|
| **E1** | Auto-recoverable | Low | None (inform later) | Linting error, formatting issue, missing import, typo in variable name, test assertion needing minor adjustment |
| **E2** | Recoverable with investigation | Medium | None (inform later, unless 3 attempts fail) | Test failure requiring code fix, dependency conflict resolvable by version pinning, configuration mismatch |
| **E3** | Recoverable with trade-off | High | Required (present options) | Dependency incompatibility requiring alternative library, performance issue requiring architectural compromise, API limitation requiring workaround |
| **E4** | Requires design change | Critical | Required (STOP) | Architectural assumption invalidated, fundamental requirement unachievable with current stack, security vulnerability in core dependency without patch |
| **E5** | Environment/infrastructure failure | Variable | Required if not self-healing | Database unreachable, API key expired, disk full, network timeout, CI/CD pipeline broken, tool not installed |

### Classification Decision Tree

When an error occurs, the agent follows this tree to classify it:

```
ERROR DETECTED
    │
    ├─[1] Is this a code style/formatting issue?
    │   └── YES → E1
    │
    ├─[2] Is this a compilation/syntax/type error?
    │   ├── Caused by a typo or missing import → E1
    │   └── Caused by a logic or design flaw → Go to [4]
    │
    ├─[3] Is this a test failure?
    │   ├── Unit test fails due to incorrect assertion → E1
    │   ├── Unit test fails due to a bug in the code under test → E2
    │   ├── Integration test fails due to service interaction issue → E2 or E3
    │   └── Test reveals the design cannot support the requirement → E4
    │
    ├─[4] Is this a dependency or compatibility issue?
    │   ├── Version conflict resolvable by pinning → E2
    │   ├── Requires switching to an alternative library → E3
    │   └── No viable alternative exists → E4
    │
    ├─[5] Is this a security issue?
    │   ├── Known CVE with available patch → E2
    │   ├── Known CVE without patch, workaround possible → E3
    │   └── Fundamental security flaw in design → E4
    │
    ├─[6] Is this an environment/infrastructure issue?
    │   └── YES → E5
    │
    └─[7] None of the above match clearly
        └── Default to E3 (conservative: involve the user)
```

**Rule:** When in doubt, classify higher (more severe). The cost of over-involving
the user is always lower than the cost of silently making a wrong decision.

---

## Recovery Protocols

### Protocol E1 — Auto-Recovery

**Trigger:** Error classified as E1.

**Agent behavior:** Fix the issue autonomously, immediately, without interrupting the user.

```
E1 ERROR
    │
    ├─[1] IDENTIFY the root cause
    │     (linting rule, missing import, typo, formatting, minor assertion)
    │
    ├─[2] FIX the issue
    │     Apply the correction directly in the code.
    │
    ├─[3] RE-RUN the verification step that failed
    │     (V1-V5 from Decision Engine verification node)
    │     ├── PASS → Continue. Log the fix for the next status report.
    │     └── FAIL → Reclassify. The error is probably not E1.
    │                Escalate to E2.
    │
    └─[4] LOG for informative reporting
          Add a one-line entry to the next status report (Interaction Protocol S3):
          "[E1] [description] → [fix applied]"
```

**Anti-loop rule:** If the same E1 error recurs after 2 fix attempts, reclassify as E2.

**State impact:** None. E1 errors do not modify project-state.md.

---

### Protocol E2 — Investigative Recovery

**Trigger:** Error classified as E2, or an E1 error that failed auto-recovery.

**Agent behavior:** Investigate the root cause, attempt a fix, and inform the user
in the next report. If 3 attempts fail, escalate.

```
E2 ERROR
    │
    ├─[1] ANALYZE the error
    │     Read the full error message, stack trace, and context.
    │     Identify the failing component, the expected behavior,
    │     and the actual behavior.
    │
    ├─[2] HYPOTHESIZE the root cause
    │     Formulate a specific hypothesis: "The error occurs because X,
    │     which causes Y instead of Z."
    │
    ├─[3] FIX based on the hypothesis
    │     Apply the correction.
    │
    ├─[4] VERIFY the fix
    │     Re-run the failing test or verification step.
    │     ├── PASS → Continue. Log for status report:
    │     │          "[E2] [description] → [root cause] → [fix applied]"
    │     │
    │     └── FAIL → INCREMENT attempt counter
    │               ├── Attempts < 3 → Go back to [2] with a NEW hypothesis.
    │               │                   Do NOT repeat the same fix.
    │               │
    │               └── Attempts >= 3 → ESCALATE
    │                   The agent STOPS and involves the user
    │                   (Interaction Protocol S4).
    │                   Present:
    │                   - What the error is
    │                   - What was tried (all 3 attempts)
    │                   - Why each attempt failed
    │                   - Suggested next steps or questions
    │
    └─[5] UPDATE state (if fix was non-trivial)
          If the fix changed the approach or introduced a workaround,
          add to project-state.md → "Vincoli Scoperti" or "Debito Tecnico Noto"
          as appropriate.
```

**Anti-loop rule:** Maximum 3 attempts. After 3 failed attempts, the error is
escalated to the user via Interaction Protocol S4, regardless of whether the agent
believes it can try again.

**State impact:** Non-trivial fixes may add entries to "Vincoli Scoperti" or
"Debito Tecnico Noto" in project-state.md.

---

### Protocol E3 — Trade-off Recovery

**Trigger:** Error classified as E3, or an E2 error that exhausted its attempts
and the investigation revealed a trade-off is needed.

**Agent behavior:** STOP. Present the situation and options to the user. Wait for
a decision before proceeding.

```
E3 ERROR
    │
    ├─[1] ANALYZE the error and its root cause
    │     Confirm that the issue genuinely requires a trade-off
    │     (not solvable with a simple fix).
    │
    ├─[2] IDENTIFY options
    │     Generate at least 2 viable options, each with:
    │     - Description of the approach
    │     - Pros (what it solves, benefits)
    │     - Cons (what it sacrifices, risks)
    │     - Impact on existing code (files affected, tests to rewrite)
    │     - Estimated effort (S/M/L)
    │
    ├─[3] FORMULATE a recommendation
    │     The agent MUST have a recommended option with a clear rationale.
    │     The rationale must reference the project's priorities:
    │     security > stability > performance.
    │
    ├─[4] PRESENT to the user (Interaction Protocol S4 + S8)
    │     Format:
    │     ┌─────────────────────────────────────────┐
    │     │ ## Trade-off Decision Required           │
    │     │                                         │
    │     │ **Error:** [description]                 │
    │     │ **Root Cause:** [analysis]               │
    │     │ **Impact:** [what is blocked]            │
    │     │                                         │
    │     │ **Option A:** [description]              │
    │     │ - Pro: [...]                            │
    │     │ - Con: [...]                            │
    │     │ - Effort: [S/M/L]                       │
    │     │                                         │
    │     │ **Option B:** [description]              │
    │     │ - Pro: [...]                            │
    │     │ - Con: [...]                            │
    │     │ - Effort: [S/M/L]                       │
    │     │                                         │
    │     │ **Recommendation:** Option [X] because   │
    │     │ [rationale]                              │
    │     └─────────────────────────────────────────┘
    │
    ├─[5] WAIT for user decision
    │     Do NOT proceed until the user responds.
    │     Do NOT assume a default.
    │
    └─[6] EXECUTE the chosen option
          - Implement the chosen approach
          - Document the decision in project-state.md → "Decisioni Architetturali"
          - If the trade-off introduces tech debt, add to "Debito Tecnico Noto"
          - If the trade-off reveals a constraint, add to "Vincoli Scoperti"
          - Update the task plan if affected (new tasks, changed estimates)
```

**State impact:** Always updates project-state.md. May generate an ADR if the
trade-off is architecturally significant.

---

### Protocol E4 — Design Change Recovery

**Trigger:** Error classified as E4 — a fundamental assumption has been invalidated.

**Agent behavior:** FULL STOP. This is a critical situation that may require
rethinking part of the architecture or the project approach.

```
E4 ERROR
    │
    ├─[1] STOP all implementation work
    │     Do not attempt any fix. The current approach is fundamentally flawed.
    │
    ├─[2] DOCUMENT the discovery
    │     Write a clear analysis:
    │     - What assumption was invalidated
    │     - What evidence proves it (error output, test results, documentation)
    │     - What parts of the project are affected
    │     - Whether any completed work can be salvaged
    │
    ├─[3] ASSESS the blast radius
    │     Identify:
    │     - Modules directly affected
    │     - Modules indirectly affected (through dependencies)
    │     - Tests that need rewriting
    │     - Documentation that needs updating
    │     - Decisions that need revisiting
    │
    ├─[4] GENERATE recovery options
    │     For each option:
    │     - Architectural changes required
    │     - Code to rewrite vs code to salvage
    │     - New dependencies or tools needed
    │     - Impact on timeline and scope
    │     - Risk assessment
    │     Include at minimum:
    │     - Option A: Full redesign of the affected component
    │     - Option B: Workaround with documented technical debt
    │     - Option C: Scope reduction (remove the requirement that caused the issue)
    │
    ├─[5] PRESENT to the user (Interaction Protocol S4 + S8 + S9)
    │     Format:
    │     ┌─────────────────────────────────────────┐
    │     │ ## Critical: Design Change Required      │
    │     │                                         │
    │     │ **Discovery:** [what was found]          │
    │     │ **Evidence:** [proof]                    │
    │     │ **Blast radius:** [affected modules]     │
    │     │ **Salvageable work:** [what can be kept] │
    │     │                                         │
    │     │ **Option A: Redesign** [details, effort] │
    │     │ **Option B: Workaround** [details, debt] │
    │     │ **Option C: Scope reduction** [details]  │
    │     │                                         │
    │     │ **Recommendation:** [with full rationale]│
    │     └─────────────────────────────────────────┘
    │
    ├─[6] WAIT for user decision
    │     This is a blocking wait. No work proceeds.
    │
    └─[7] EXECUTE the chosen recovery
          - Create an ADR documenting the design change (mandatory for E4)
          - Update the project specification if requirements changed
          - Update project-state.md:
            → "Decisioni Architetturali" (append)
            → "Vincoli Scoperti" (if applicable)
            → "Debito Tecnico Noto" (if workaround chosen)
            → "Piano dei Task" (rework affected tasks)
            → "Stato dei Moduli" (mark affected modules as needing rework)
          - Re-run Task Decomposition (DIR-006) for the affected scope
          - Inform the user of the updated plan and timeline
```

**State impact:** Major. Always updates project-state.md (multiple sections).
Always generates an ADR. May trigger re-decomposition of the task plan.

---

### Protocol E5 — Environment/Infrastructure Recovery

**Trigger:** Error classified as E5 — the environment or infrastructure is the problem,
not the code.

**Agent behavior:** Attempt self-healing. If unsuccessful, involve the user.

```
E5 ERROR
    │
    ├─[1] CLASSIFY the infrastructure issue
    │     │
    │     ├── Tool not installed or not found
    │     │   → Attempt to install it (if within project dependencies)
    │     │   → If system-level tool: STOP, ask user to install
    │     │
    │     ├── Network/connectivity issue
    │     │   → Retry with exponential backoff (max 3 attempts, 2s/4s/8s)
    │     │   → If persistent: STOP, inform user
    │     │
    │     ├── Authentication/authorization failure
    │     │   → STOP immediately. NEVER attempt to fix credentials.
    │     │   → Inform user: "The credential for [service] appears
    │     │     to be invalid or expired. Please check [env variable name]."
    │     │
    │     ├── Resource exhaustion (disk, memory, rate limit)
    │     │   ├── Rate limit → Wait for reset window, then retry
    │     │   ├── Disk/memory → STOP, inform user
    │     │   └── API quota → STOP, inform user with usage info
    │     │
    │     ├── Database unreachable
    │     │   → Verify connection string (format only, never log the value)
    │     │   → Retry connection (max 3 attempts)
    │     │   → If persistent: STOP, inform user
    │     │
    │     └── CI/CD pipeline failure (not caused by code)
    │         → Analyze pipeline logs
    │         → If configuration issue: propose fix
    │         → If infrastructure issue: STOP, inform user
    │
    ├─[2] SELF-HEALING successful?
    │     ├── YES → Log for status report:
    │     │          "[E5] [issue] → [self-healed: action taken]"
    │     │          Continue with the current task.
    │     │
    │     └── NO → STOP and inform the user (Interaction Protocol S4)
    │              Present:
    │              - What the infrastructure issue is
    │              - What was attempted
    │              - What the user needs to do
    │              - Whether work can continue on other tasks in the meantime
    │
    └─[3] STATE impact
          If the issue reveals a permanent constraint (e.g., rate limit
          lower than expected), add to project-state.md → "Vincoli Scoperti".
```

**Security rule:** The agent NEVER attempts to modify, regenerate, or guess
credentials. Authentication failures are always escalated to the user.

**State impact:** Only if a new constraint is discovered.

---

## Cascading Recovery

When a recovery protocol itself fails, errors escalate through the cascade:

```
E1 (auto-fix fails after 2 attempts)
    → Escalates to E2

E2 (investigation fails after 3 attempts)
    → Escalates to E3 (if trade-off is possible)
    → Or to E4 (if design change is needed)
    → Or STOP with user involvement (S4)

E3 (user chooses an option, but implementation reveals new issues)
    → Re-classify the new error independently
    → If it escalates to E4: inform user that the chosen option
      uncovered a deeper problem

E4 (recovery plan fails)
    → FULL STOP. Present updated analysis to user.
    → The agent does NOT attempt further recovery without explicit
      user direction.

E5 (self-healing fails)
    → STOP. User must resolve the infrastructure issue.
    → Agent can offer to work on tasks that do not depend on
      the failed infrastructure.
```

**Maximum cascade depth:** An error can escalate at most 2 levels (E1→E2→E3 or
E2→E3→E4). If recovery fails at 2 levels above the original classification,
the agent STOPS unconditionally and involves the user.

---

## Error Logging and Traceability

Every error, regardless of class, is logged in a structured format. This log exists
in the agent's session memory and is persisted to project-state.md at checkpoints
and session end.

### Error Log Entry Format

```markdown
### Error [N] — [YYYY-MM-DD HH:MM]

- **Class:** E[1-5]
- **Task:** TASK-[NNN] ([description])
- **Module:** [affected module]
- **Description:** [what happened]
- **Root cause:** [analysis]
- **Recovery:** [action taken]
- **Outcome:** Resolved | Escalated to E[X] | Pending user input
- **State changes:** [sections of project-state.md updated, or "none"]
- **Lesson learned:** [if applicable, what should be done differently next time]
```

### Aggregation in Status Reports

In checkpoint and session-end reports, errors are aggregated by class:

```markdown
**Errors this session:**
- E1: 3 (all auto-resolved)
- E2: 1 (resolved after 2 attempts — dependency version conflict)
- E3: 0
- E4: 0
- E5: 1 (network timeout — self-healed after retry)
```

---

## Integration with Other Framework Documents

This document is referenced by and interacts with:

| Document | Interaction |
|----------|-------------|
| [01-decision-engine.md](01-decision-engine.md) | Routes errors to this protocol at step [6]. The verification node (V1-V5) generates errors that enter this classification system. |
| [02-task-decomposition.md](02-task-decomposition.md) | E4 recovery may trigger re-decomposition of the task plan. |
| [04-interaction-protocol.md](04-interaction-protocol.md) | S3 (informative) for E1/E2 resolved errors. S4 (STOP) for E2 escalated, E3, E4, E5 unresolved. S8 for deviations caused by E3/E4 recovery. S9 for new risks discovered during error analysis. |
| [05-state-management.md](05-state-management.md) | E3/E4 errors always update project-state.md. E5 may add to "Vincoli Scoperti". Errors are persisted at checkpoints. |
| [03-directive-template.md](../L1-directives/03-directive-template.md) | The Error Handling section of every directive references error classes E1-E5 as defined here. |
| [02-testing-strategy.md](../L3-execution/02-testing-strategy.md) | Test failures are a primary source of E1-E4 errors. |
| [03-security-guidelines.md](../L3-execution/03-security-guidelines.md) | Security issues follow classification rules in this document. Credential failures are always E5 with mandatory user escalation. |

---

## Anti-Patterns to Avoid

| Anti-Pattern | Why It Is Wrong | What to Do Instead |
|-------------|----------------|-------------------|
| Retry the same fix repeatedly | Insanity: same input, expecting different output | Each attempt MUST use a different hypothesis and approach |
| Silently swallow an error | Hides problems that will resurface later, harder to diagnose | Every error is classified, handled, and logged |
| Classify everything as E1 to avoid user interaction | Under-classifies severity, leads to wrong decisions | Follow the classification tree honestly. When in doubt, classify higher |
| Skip user involvement for E3/E4 | The agent makes trade-off decisions without authority | E3 and E4 always require user input. No exceptions |
| Attempt to fix credentials or secrets | Security risk — the agent should never handle raw secrets | Always escalate credential issues to the user |
| Continue working after E4 without user approval | Building on a broken foundation | E4 is a FULL STOP. No code is written until the user approves a recovery plan |
| Blame the user or external factors | Unproductive and unhelpful | Focus on the problem, the options, and the recommendation |
| Forget to update project-state.md after recovery | The next session will lack context about what happened | Always persist error outcomes and state changes |

---

## Output

| Output | Format | Destination | Validation |
|--------|--------|-------------|------------|
| Error classification | E1-E5 class | In-memory + project-state.md (for E3+) | Class matches the classification tree criteria |
| Recovery action | Code fix, configuration change, or user communication | Source files, config files, or user message | The original error no longer occurs after recovery |
| Error log entry | Structured Markdown | Session memory → project-state.md at checkpoint | All fields are populated, outcome is recorded |
| ADR (for E4 only) | Markdown | docs/adr/ | Follows ADR template, documents the design change |
| Updated task plan (for E3/E4) | Updated project-state.md | project-state.md → "Piano dei Task" | Plan reflects the new reality post-recovery |

---

## Error Handling (Meta)

| Error | Probable Cause | Resolution | Class |
|-------|---------------|------------|-------|
| Agent cannot classify an error into E1-E5 | Unusual or unprecedented error type | Default to E3 (conservative). Document the case. Propose an update to this directive's classification tree. | E1 |
| Recovery protocol loops (same error recurs after fix) | Root cause not correctly identified | Stop the loop after the maximum attempts. Escalate one level. Present the full history to the user. | E2 |
| User does not respond to E3/E4 escalation | User is unavailable or the question is unclear | Wait. Do NOT assume a default. The agent can offer to work on unrelated tasks in the meantime. | E5 |
| project-state.md is not writable | File permission or lock issue | Log the error in session memory. Attempt to write at the next opportunity. Inform the user if persistent. | E5 |

---

## Lessons Learned

| Date | Discovery | Action Taken |
|------|-----------|--------------|
| — | — | — |

---

## Changelog

| Version | Date | Author | Change Description |
|---------|------|--------|-------------------|
| 1.0 | 2026-03-24 | agent | Initial creation with E1-E5 classification, recovery protocols, cascading logic, and framework integration |
