# Directive Catalog — Registry of All D.O.E. Directives

## Metadata
- ID: DIR-004
- Version: 1.3
- Last updated: 2026-03-24
- Depends on: DIR-003
- Project type: universale
- Status: active
- Superseded by: —

## Objective

Maintain a single, authoritative index of all directives in the D.O.E. framework, enabling the agent to discover, resolve, and validate directives by their unique ID.

## Overview

This catalog is the **source of truth** for directive existence and identity. Every directive in the framework MUST have an entry here. The orchestration layer uses this catalog to:

1. **Resolve** a `DIR-ID` to its file path.
2. **Discover** which directives are available for a given project type.
3. **Validate** dependency integrity (no dangling references, no circular dependencies).
4. **Track** the lifecycle status of each directive.

---

## Catalog Registry

### L1 — Directives (Direttiva)

| DIR-ID  | Name                        | File                                      | Version | Status | Project Type | Depends On | Description                                              |
|---------|-----------------------------|-------------------------------------------|---------|--------|--------------|------------|----------------------------------------------------------|
| DIR-001 | Project Intake Protocol     | `L1-directives/01-project-intake.md`      | 1.0     | active | universale   | none       | Structured requirements gathering before any coding      |
| DIR-002 | Architecture Patterns       | `L1-directives/02-architecture-patterns.md`| 1.0    | active | universale   | DIR-001    | Architectural pattern selection based on project type     |
| DIR-003 | Directive Template          | `L1-directives/03-directive-template.md`  | 1.0     | active | universale   | none       | Mandatory schema and governance rules for all directives  |
| DIR-004 | Directive Catalog           | `L1-directives/04-directive-catalog.md`   | 1.0     | active | universale   | DIR-003    | This file — authoritative index of all directives         |

### L2 — Orchestration (Orchestrazione)

| DIR-ID  | Name                        | File                                       | Version | Status | Project Type | Depends On | Description                                              |
|---------|-----------------------------|-------------------------------------------|---------|--------|--------------|------------|----------------------------------------------------------|
| DIR-005 | Decision Engine             | `L2-orchestration/01-decision-engine.md`  | 1.0     | active | universale   | DIR-001    | Decision tree and principles for agent decision-making    |
| DIR-006 | Task Decomposition          | `L2-orchestration/02-task-decomposition.md`| 1.0    | active | universale   | DIR-005    | How to break complex projects into ordered tasks          |
| DIR-007 | Error Recovery              | `L2-orchestration/03-error-recovery.md`   | 1.0     | active | universale   | DIR-005    | Error classification (E1-E5) and cascading recovery       |
| DIR-008 | Interaction Protocol        | `L2-orchestration/04-interaction-protocol.md`| 1.0  | active | universale   | none       | When and how the agent interacts with the user            |
| DIR-009 | State Management            | `L2-orchestration/05-state-management.md` | 1.0     | active | universale   | DIR-005    | Project memory and persistent state across sessions       |

### L3 — Execution (Esecuzione)

| DIR-ID  | Name                        | File                                       | Version | Status | Project Type | Depends On | Description                                              |
|---------|-----------------------------|-------------------------------------------|---------|--------|--------------|------------|----------------------------------------------------------|
| DIR-010 | Code Standards              | `L3-execution/01-code-standards.md`       | 1.0     | active | universale   | none       | Naming conventions, structure, logging, error handling     |
| DIR-011 | Testing Strategy            | `L3-execution/02-testing-strategy.md`     | 1.0     | active | universale   | DIR-010    | 4-level testing strategy (unit, integration, E2E, smoke)  |
| DIR-012 | Security Guidelines         | `L3-execution/03-security-guidelines.md`  | 1.0     | active | universale   | DIR-010    | Security-by-default rules for credentials, input, deps    |
| DIR-013 | Documentation Rules         | `L3-execution/04-documentation-rules.md`  | 1.0     | active | universale   | DIR-010    | Documentation-as-code requirements and update rules       |
| DIR-014 | CI/CD Setup                 | `L3-execution/05-cicd-setup.md`           | 1.0     | active | universale   | DIR-010, DIR-011, DIR-012, DIR-013 | Pipeline structure, branching strategy, .gitignore   |
| DIR-015 | Dependency Management       | `L3-execution/06-dependency-management.md`| 1.0     | active | universale   | DIR-010, DIR-013 | Dependency selection checklist and compatibility tracking  |

---

## ID Allocation Rules

1. **Range allocation by layer:**
   - `DIR-001` to `DIR-099`: L1 — Directives
   - `DIR-100` to `DIR-199`: L2 — Orchestration (future expansion, currently using DIR-005 to DIR-009 for simplicity)
   - `DIR-200` to `DIR-299`: L3 — Execution (future expansion, currently using DIR-010 to DIR-015 for simplicity)
   - `DIR-300` to `DIR-499`: Templates and utilities
   - `DIR-500` to `DIR-999`: Project-specific directives (created per-project)

   > **Note:** The initial allocation (DIR-001 through DIR-015) uses sequential numbering for simplicity. As the framework grows, new directives SHOULD follow the range allocation above. Existing IDs are grandfathered and will not be renumbered.

2. **Next available ID:** DIR-016

3. **Retired IDs:** None. Retired IDs are never reassigned — they remain in the catalog with status `deprecated` or `superseded` for traceability.

---

## Dependency Graph

The following represents the dependency relationships between directives. The agent must verify this graph remains a DAG (no cycles) whenever a directive is created or modified.

```
DIR-003 (Directive Template)
  └── DIR-004 (Directive Catalog) [this file]

DIR-001 (Project Intake Protocol)
  ├── DIR-002 (Architecture Patterns)
  ├── DIR-005 (Decision Engine)
  │     ├── DIR-006 (Task Decomposition)
  │     ├── DIR-007 (Error Recovery)
  │     └── DIR-009 (State Management)
  └── (indirect: all execution directives benefit from intake)

DIR-008 (Interaction Protocol) [no dependencies — standalone]

DIR-010 (Code Standards)
  ├── DIR-011 (Testing Strategy)
  ├── DIR-012 (Security Guidelines)
  └── DIR-015 (Dependency Management)

DIR-011 (Testing Strategy) ──┐
DIR-012 (Security Guidelines)├── DIR-014 (CI/CD Setup)
                             ┘

DIR-010 (Code Standards)
  └── DIR-013 (Documentation Rules)

DIR-013 (Documentation Rules) ─┐
DIR-010 (Code Standards) ──────┤
                               └── DIR-015 (Dependency Management)
```

---

## Agent Behavioral Rules for Catalog Management

1. **Before creating a new directive**, the agent MUST check this catalog to:
   - Verify no existing directive already covers the same scope.
   - Reserve the next available ID.
   - Add a `draft` entry to the catalog.

2. **After completing a directive**, the agent MUST update this catalog entry with:
   - The correct version number.
   - Status changed from `draft` to `active`.
   - The accurate file path.

3. **When resolving a DIR-ID**, the agent looks up this catalog and uses the `File` column to locate the directive. If the file does not exist at the specified path, the agent flags an inconsistency.

4. **Periodic integrity check**: The agent SHOULD verify catalog integrity when starting a new project session:
   - Every file listed in the catalog exists.
   - Every directive file in the filesystem has a catalog entry.
   - No circular dependencies exist in the dependency graph.
   - No `active` directive depends on a `deprecated` or `draft` directive.

---

## Changelog

| Version | Date       | Author | Change Description                                   |
|---------|------------|--------|------------------------------------------------------|
| 1.0     | 2026-03-24 | agent  | Initial creation with all planned directives indexed |
| 1.1     | 2026-03-24 | agent  | DIR-010 (Code Standards) and DIR-011 (Testing Strategy) promoted to active v1.0 |
| 1.2     | 2026-03-24 | agent  | DIR-002 (Architecture Patterns) promoted to active v1.0 |
| 1.3     | 2026-03-24 | agent  | All directives promoted to active v1.0: DIR-001, DIR-005, DIR-006, DIR-007 (created), DIR-008, DIR-012, DIR-013, DIR-015. DIR-013 depends on DIR-010 (corrected from "none"). DIR-015 depends on DIR-010, DIR-013 (corrected). |
