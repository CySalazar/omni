# Directive Template — Schema Formale delle Direttive D.O.E.

## Metadata
- ID: DIR-003
- Version: 1.0
- Last updated: 2026-03-24
- Depends on: []
- Project type: universale

## Objective

Define the mandatory schema that every directive within the D.O.E. framework must follow, ensuring completeness, consistency, and traceability across all directives.

## Overview

Every directive in the D.O.E. framework is a **Standard Operating Procedure (SOP) in Markdown**. To guarantee that directives are complete, machine-parsable, and consistently structured, each one MUST conform to the schema defined in this document.

A directive that does not follow this schema is considered **non-compliant** and must be updated before it can be referenced by the orchestration layer or cataloged in `04-directive-catalog.md`.

---

## Directive Schema

Every directive file MUST contain the following sections, in this exact order. Sections marked as `[conditional]` may be omitted only when the condition is explicitly not applicable (the agent must document why it was omitted).

```markdown
# [DIRECTIVE-NAME]
<!-- Human-readable name in Title Case, concise and descriptive -->

## Metadata
- ID: DIR-XXX
- Version: X.Y
- Last updated: YYYY-MM-DD
- Depends on: [comma-separated list of DIR-IDs, or "none"]
- Project type: [universale | webapp | api | bot | pipeline | cli | mobile | devops | data | library]
- Status: [draft | active | deprecated | superseded]
- Superseded by: [DIR-ID, only if status is "superseded"]

## Objective
<!-- One clear sentence describing what this directive achieves.
     Must answer: "After executing this directive, what is true that wasn't true before?" -->

## Pre-conditions
<!-- What MUST be true BEFORE this directive can be executed.
     Each pre-condition is a verifiable statement.
     Format: bulleted list, each item is testable (yes/no). -->
- [Pre-condition 1]
- [Pre-condition 2]

## Input
<!-- Data, files, configurations, or context required to execute this directive.
     Each input specifies: name, type/format, source, and whether it is required or optional. -->

| Input | Type/Format | Source | Required |
|-------|-------------|--------|----------|
| [name] | [type] | [where it comes from] | yes/no |

## Procedure
<!-- Ordered steps the agent must follow.
     Each step MUST include:
     - A specific, imperative action
     - The tool/script to use (if applicable)
     - Expected input for that step
     - Expected output for that step
     - Success criterion (how to know the step succeeded)

     Steps are numbered sequentially. Sub-steps use lettered notation (a, b, c). -->

### Step 1 — [Step Title]
- **Action:** [What to do]
- **Tool/Script:** `[path/to/tool]` or "manual" or "agent decision"
- **Input:** [What this step receives]
- **Output:** [What this step produces]
- **Success criterion:** [How to verify this step completed correctly]

### Step 2 — [Step Title]
- **Action:** [What to do]
- **Tool/Script:** `[path/to/tool]`
- **Input:** [What this step receives]
- **Output:** [What this step produces]
- **Success criterion:** [How to verify this step completed correctly]

<!-- Repeat for each step -->

## Output
<!-- Deliverables produced by this directive.
     Each output specifies: name, format, destination path, and validation criteria. -->

| Output | Format | Destination | Validation |
|--------|--------|-------------|------------|
| [name] | [format] | [file path or location] | [how to verify it is correct] |

## Error Handling
<!-- Known errors that may occur during execution.
     Each entry specifies: error description, probable cause, resolution action,
     and the error class (E1-E5) as defined in L2-orchestration/03-error-recovery.md -->

| Error | Probable Cause | Resolution | Class |
|-------|---------------|------------|-------|
| [description] | [cause] | [action to take] | E1/E2/E3/E4/E5 |

## Lessons Learned
<!-- This section grows over time as the agent discovers new insights.
     Each entry includes: date, discovery, and action taken.
     The agent MUST append here — never delete existing entries. -->

| Date | Discovery | Action Taken |
|------|-----------|--------------|
| YYYY-MM-DD | [What was learned] | [How the directive or process was updated] |

## Changelog
<!-- Version history of this directive.
     Each entry includes: version, date, author, and description of change. -->

| Version | Date | Author | Change Description |
|---------|------|--------|-------------------|
| 1.0 | YYYY-MM-DD | [agent/user] | Initial creation |
```

---

## Schema Validation Rules

The agent MUST validate every directive against these rules before considering it compliant:

### R1 — Structural Completeness
Every mandatory section listed in the schema above MUST be present. Missing sections cause the directive to be flagged as **non-compliant**.

### R2 — Unique Identifier
- The `ID` field follows the pattern `DIR-XXX` where `XXX` is a zero-padded three-digit number (e.g., `DIR-001`, `DIR-042`).
- IDs are assigned sequentially and MUST be unique across the entire framework.
- The catalog (`04-directive-catalog.md`) is the authoritative source for ID assignment.

### R3 — Semantic Versioning
- Directives use **two-part versioning**: `MAJOR.MINOR`.
- **MAJOR** increments when the directive's procedure changes in a way that alters its output or behavior.
- **MINOR** increments for clarifications, added lessons learned, improved error handling, or editorial corrections.
- Version `1.0` is always the initial release.

### R4 — Dependency Integrity
- Every `DIR-ID` listed in `Depends on` MUST exist in the catalog with status `active`.
- Circular dependencies are forbidden. The agent must verify the dependency graph is a DAG (Directed Acyclic Graph).
- If a depended-upon directive is deprecated or superseded, the depending directive must be reviewed and updated.

### R5 — Status Lifecycle
Directives follow this lifecycle:

```
draft → active → deprecated
                    ↓
              superseded (by DIR-XXX)
```

- **draft**: Under development, not yet usable by the orchestration layer.
- **active**: Validated, approved, and available for execution.
- **deprecated**: No longer recommended. The agent should warn if it is about to use a deprecated directive.
- **superseded**: Replaced by a newer directive (the `Superseded by` field points to the replacement).

### R6 — Procedure Determinism
- Each step in the `Procedure` section must be **specific enough** that two different agents executing the same directive with the same input would produce the same output.
- Ambiguous language ("consider", "maybe", "if appropriate") is forbidden in procedure steps. Use conditional logic explicitly: "IF [condition] THEN [action] ELSE [alternative action]".

### R7 — Testable Pre-conditions
- Every pre-condition must be verifiable programmatically or by direct observation.
- Bad example: "The project should be well-structured"
- Good example: "The file `docs/tech-specs.md` exists and contains a `## Runtime` section"

### R8 — Measurable Success Criteria
- Every step's success criterion must be objectively verifiable.
- Bad example: "The code looks good"
- Good example: "All unit tests pass with exit code 0 and coverage >= 80%"

---

## Governance Rules

### Creating a New Directive
1. The agent (or user) identifies a need for a new directive.
2. A new `DIR-ID` is reserved in `04-directive-catalog.md` with status `draft`.
3. The directive is written following this schema.
4. The agent validates the directive against all rules (R1-R8).
5. If validation passes, status is changed to `active` and the catalog is updated.
6. If validation fails, the agent documents what is missing and keeps status as `draft`.

### Modifying an Existing Directive
1. The agent identifies the need for a change (from a lesson learned, an error, or a user request).
2. The agent documents the proposed change and its motivation.
3. For **MINOR** changes: the agent applies the change, increments the minor version, updates the changelog, and informs the user in the next status report.
4. For **MAJOR** changes: the agent MUST present the proposed change to the user and receive explicit approval before applying it.
5. The catalog entry is updated with the new version and date.

### Deprecating a Directive
1. The agent identifies that a directive is no longer applicable or has been replaced.
2. The directive's status is changed to `deprecated` (or `superseded` with a pointer to the replacement).
3. The catalog is updated.
4. Any directive that depends on the deprecated one is flagged for review.

### Cross-Referencing Directives
- Directives reference each other by `DIR-ID` only (never by filename, as filenames may change).
- Example: "Execute the procedure defined in `DIR-001` before proceeding."
- The agent resolves `DIR-ID` to filename via the catalog.

---

## Example — Minimal Compliant Directive

```markdown
# Database Migration Execution

## Metadata
- ID: DIR-050
- Version: 1.0
- Last updated: 2026-03-24
- Depends on: DIR-001
- Project type: universale
- Status: active
- Superseded by: —

## Objective
Execute pending database migrations safely, with rollback capability and data integrity verification.

## Pre-conditions
- The file `docs/tech-specs.md` exists and specifies the database engine and ORM.
- The migration tool (e.g., Alembic, Prisma Migrate, EF Core) is installed and configured.
- A backup of the current database state has been created or is not required (development environment).

## Input

| Input | Type/Format | Source | Required |
|-------|-------------|--------|----------|
| Migration files | SQL/ORM migration files | `migrations/` directory | yes |
| Database connection string | String (URI) | `.env` file (`DATABASE_URL`) | yes |
| Target environment | String (dev/staging/prod) | User input or CI variable | yes |

## Procedure

### Step 1 — Verify Pending Migrations
- **Action:** List all pending (unapplied) migrations.
- **Tool/Script:** ORM CLI (e.g., `alembic heads`, `prisma migrate status`)
- **Input:** Database connection string
- **Output:** List of pending migrations with descriptions
- **Success criterion:** Command exits with code 0 and outputs a list (may be empty)

### Step 2 — Apply Migrations
- **Action:** Apply all pending migrations sequentially.
- **Tool/Script:** ORM CLI (e.g., `alembic upgrade head`, `prisma migrate deploy`)
- **Input:** Migration files + database connection string
- **Output:** Migration execution log
- **Success criterion:** All migrations applied successfully, exit code 0, no error in log

### Step 3 — Verify Data Integrity
- **Action:** Run a post-migration verification query to confirm schema is as expected.
- **Tool/Script:** Custom verification script or ORM introspection
- **Input:** Expected schema definition from migration files
- **Output:** Schema comparison report
- **Success criterion:** Current schema matches expected schema with zero differences

## Output

| Output | Format | Destination | Validation |
|--------|--------|-------------|------------|
| Migration execution log | Text/log | stdout + `docs/deployment.md` (summary) | No errors in log |
| Updated database schema | Database state | Target database | Schema matches migration definitions |

## Error Handling

| Error | Probable Cause | Resolution | Class |
|-------|---------------|------------|-------|
| Migration fails mid-execution | Conflicting schema change or data constraint violation | Rollback to pre-migration state, review migration file | E3 |
| Connection refused | Wrong connection string or database is down | Verify `.env` DATABASE_URL, check database service status | E5 |
| Migration already applied | Migration state out of sync | Run migration status check, resolve with `--fake` if appropriate | E2 |

## Lessons Learned

| Date | Discovery | Action Taken |
|------|-----------|--------------|
| — | — | — |

## Changelog

| Version | Date | Author | Change Description |
|---------|------|--------|-------------------|
| 1.0 | 2026-03-24 | agent | Initial creation |
```

---

## Agent Behavioral Rules for Directive Management

1. **Before executing any directive**, the agent MUST verify it is compliant with this schema. If not, the agent flags it and requests an update before proceeding.
2. **After resolving any error**, the agent MUST add an entry to the `Lessons Learned` section of the relevant directive.
3. **When creating a new directive**, the agent MUST use this template as the starting point — never create a directive from scratch without this structure.
4. **The agent MUST NOT silently modify a directive's Procedure section** without logging the change in the Changelog and informing the user.
5. **The agent treats directives as contracts**: once a directive is `active`, its Procedure is a binding set of instructions. Deviations require explicit justification and user approval.
