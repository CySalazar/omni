# Architecture Patterns — Pattern Architetturali per Tipo di Progetto

## Metadata

- **ID:** DIR-002
- **Version:** 1.0
- **Last updated:** 2026-03-24
- **Depends on:** DIR-001
- **Project type:** universale
- **Status:** active
- **Superseded by:** —

## Objective

Provide the agent with a structured decision framework for selecting the appropriate
architectural pattern based on project characteristics, and supply reference directory
structures, dependency strategies, deploy coordination approaches, and cross-component
testing guidelines for each pattern.

## Pre-conditions

- The Project Intake Protocol (DIR-001) has been completed: all core questions have
  answers and contextual questions have been gathered.
- The project type, scale, team size, and integration requirements are known.
- The agent has access to this file and to the project specification template
  (`templates/project-spec.md`).

## Input

| Input | Type/Format | Source | Required |
|-------|-------------|--------|----------|
| Project Intake answers | Structured data | Output of DIR-001 Fase 1 + Fase 2 | yes |
| Scale estimate (users, data, throughput) | Numeric ranges | DIR-001 question #5 | yes |
| Team size / ownership model | String | User input or inferred | no (defaults to single-team) |
| Existing infrastructure constraints | List of strings | DIR-001 question #4 | no |

---

## Procedure

### Step 1 — Classify the Project Against the Selection Matrix

- **Action:** Evaluate the project characteristics against the selection matrix
  defined in Section "Pattern Selection Matrix" below. Identify which pattern(s)
  match the project's profile.
- **Tool/Script:** Agent reasoning based on intake answers.
- **Input:** All project intake answers.
- **Output:** One or more candidate patterns with match scores.
- **Success criterion:** At least one pattern is identified as a strong match.
  If multiple patterns match equally, proceed to Step 2 with all candidates.

### Step 2 — Evaluate Trade-offs for Candidate Patterns

- **Action:** For each candidate pattern, evaluate the trade-offs listed in its
  dedicated section below. Consider the project's specific constraints (budget,
  timeline, team expertise, scale requirements).
- **Tool/Script:** Agent reasoning. If trade-offs are unclear, activate the
  Interaction Protocol (DIR-008) and ask the user.
- **Input:** Candidate patterns from Step 1 + project constraints.
- **Output:** A ranked list of patterns with justification.
- **Success criterion:** A single pattern is selected with a documented rationale
  that references specific project constraints.

### Step 3 — Apply the Selected Pattern

- **Action:** Use the reference directory structure, dependency strategy, deploy
  approach, and testing guidelines from the selected pattern's section below to
  populate the project specification (`templates/project-spec.md`, sections 2 and 7).
- **Tool/Script:** Agent writes into the project-spec.md template.
- **Input:** Selected pattern + project-specific details.
- **Output:** Populated architecture and directory structure sections in the project spec.
- **Success criterion:** The project spec's architecture section is complete, the
  directory structure is concrete (no placeholders), and the pattern selection is
  documented with rationale.

### Step 4 — Validate Pattern Fitness

- **Action:** Cross-check the selected pattern against the project's non-functional
  requirements (scale, security, deploy frequency, team structure). If any mismatch
  is found, document it as a risk in the project spec.
- **Tool/Script:** Agent reasoning + risk assessment.
- **Input:** Populated project spec + original intake answers.
- **Output:** Validation report (pass/fail with notes). Any mismatches added to
  the risk section of the project spec.
- **Success criterion:** All non-functional requirements are addressed by the pattern,
  or mismatches are explicitly documented as risks with mitigations.

---

## Pattern Selection Matrix

Use this matrix as the primary decision tool. Match project characteristics to patterns.

| Characteristic | Monolith | Monorepo Multi-Service | Microservices (Polyrepo) | Frontend + Backend Split |
|---------------|----------|----------------------|------------------------|------------------------|
| **Team size** | 1-5 developers | 3-15 developers | 5+ developers, multiple teams | 2+ developers (FE + BE) |
| **Project phase** | MVP, prototype, early stage | Growing product, shared code | Mature product, independent scaling | Any phase with distinct UI + API |
| **Scale requirements** | Low to medium | Medium to high | High, heterogeneous | Varies (decoupled scaling) |
| **Deploy independence** | Not needed | Partial (per-service) | Full independence required | Frontend and backend independent |
| **Code sharing needs** | N/A (single codebase) | High (shared libs) | Low (contracts only) | Low (API contract only) |
| **Technology homogeneity** | Single stack | Mostly uniform, some variation | Polyglot possible | Can differ (e.g., React + Go) |
| **Operational complexity** | Low | Medium | High | Medium |
| **Time to first deploy** | Fast | Medium | Slow | Medium |

### Quick Decision Guide

```
PROJECT CHARACTERISTICS
    |
    +-- Single team, MVP/prototype, tight deadline?
    |   YES --> MONOLITH
    |
    +-- Multiple related services sharing significant code?
    |   YES --> MONOREPO MULTI-SERVICE
    |
    +-- Independent teams, independent scaling, independent deploy?
    |   YES --> MICROSERVICES (POLYREPO)
    |
    +-- Distinct user-facing UI with a separate API backend?
    |   YES --> FRONTEND + BACKEND SPLIT
    |
    +-- None of the above clearly fits?
        --> Default to MONOLITH. It is always easier to extract services
            from a well-structured monolith than to merge poorly designed
            microservices. Document the decision in an ADR.
```

**Critical principle:** When in doubt, start with a monolith. A well-structured
monolith with clear module boundaries can be decomposed later. Premature distribution
adds complexity without proportional benefit.

---

## Pattern 1 — Monolith

### When to Use

- MVP, prototype, or early-stage product where speed of delivery is critical.
- Small team (1-5 developers) working on a single codebase.
- Low to medium scale requirements (< 10K concurrent users for web, < 1M records for data).
- No requirement for independent deployment of components.
- Budget or timeline constraints that preclude infrastructure complexity.

### When NOT to Use

- Different components need to scale independently (e.g., CPU-intensive processing
  vs. I/O-bound API serving).
- Multiple teams need to deploy independently without coordination.
- The project requires polyglot technology (different languages for different components).

### Trade-offs

| Advantage | Disadvantage |
|-----------|-------------|
| Simple to develop, test, and deploy | Scaling is all-or-nothing |
| Single codebase, easy to navigate | Large codebase can become unwieldy over time |
| No network latency between components | A bug in one module can crash the entire application |
| Straightforward debugging and profiling | Deploy requires full application restart |
| Easy local development setup | Technology lock-in to a single stack |

### Directory Structure — Layer-Based Organization

Use layer-based organization for projects where the domain is simple or the team
prefers traditional separation by technical concern.

```
project-root/
├── src/
│   ├── api/                    # HTTP handlers, routes, middleware
│   │   ├── routes/             # Route definitions grouped by domain
│   │   ├── middleware/         # Authentication, logging, CORS, rate limiting
│   │   └── validators/        # Request validation schemas
│   ├── services/               # Business logic layer
│   │   └── [domain-name]/     # One directory per business domain
│   ├── repositories/           # Data access layer
│   │   └── [domain-name]/     # One directory per data domain
│   ├── models/                 # Data models, entities, DTOs
│   ├── infrastructure/         # External service clients, messaging, cache
│   ├── config/                 # Configuration loading and validation
│   ├── common/                 # Shared utilities, constants, types
│   └── main.[ext]              # Application entry point
├── tests/
│   ├── unit/                   # Mirrors src/ structure
│   ├── integration/            # Tests for cross-layer interactions
│   └── e2e/                    # Full application flow tests
├── docs/
│   ├── architecture.md
│   ├── api.md
│   ├── deployment.md
│   └── tech-specs.md
├── .env.example
├── .gitignore
├── README.md
├── CHANGELOG.md
└── [package manager files]
```

### Directory Structure — Feature-Based Organization (Alternative)

Use feature-based organization for projects where each feature is self-contained
and the team prefers vertical slicing over horizontal layering.

```
project-root/
├── src/
│   ├── features/
│   │   ├── auth/               # Everything related to authentication
│   │   │   ├── auth.controller.[ext]
│   │   │   ├── auth.service.[ext]
│   │   │   ├── auth.repository.[ext]
│   │   │   ├── auth.models.[ext]
│   │   │   └── auth.validators.[ext]
│   │   ├── users/              # Everything related to user management
│   │   │   ├── users.controller.[ext]
│   │   │   ├── users.service.[ext]
│   │   │   ├── users.repository.[ext]
│   │   │   └── users.models.[ext]
│   │   └── [feature-name]/     # One directory per feature
│   ├── shared/                 # Cross-feature shared code
│   │   ├── middleware/
│   │   ├── utils/
│   │   └── types/
│   ├── config/
│   └── main.[ext]
├── tests/
│   ├── unit/
│   │   └── features/           # Mirrors src/features/ structure
│   ├── integration/
│   └── e2e/
├── docs/
├── .env.example
├── .gitignore
├── README.md
├── CHANGELOG.md
└── [package manager files]
```

**Selection guidance:** Layer-based is better for small teams and CRUD-heavy applications.
Feature-based is better when features are complex and relatively independent, or when
the team is preparing for a potential future split into services.

### Dependency Management

- Single package manager file at the root.
- All dependencies are project-wide.
- No special considerations beyond standard dependency management (see DIR-015).

### Deploy Strategy

- Single deployable artifact (container image, binary, or package).
- Single CI/CD pipeline.
- Rolling deploy or blue-green for zero-downtime updates.
- Database migrations run as part of the deploy process (before application starts).

### Testing Strategy

- **Unit tests:** Per-module, mocking cross-layer dependencies.
- **Integration tests:** Test service-repository interactions with real or in-memory
  database.
- **E2E tests:** Full HTTP request/response cycles against a running instance.
- **Smoke tests:** Health endpoint + basic CRUD operation post-deploy.

---

## Pattern 2 — Monorepo Multi-Service

### When to Use

- Multiple related services that share significant code (models, utilities, types).
- Single team or closely collaborating teams (3-15 developers).
- Need for atomic changes across services (one commit updates both a shared library
  and its consumers).
- Medium to high scale where some components need independent scaling.
- The services are under the same organizational ownership.

### When NOT to Use

- Services are owned by completely independent teams with different release cadences.
- Services are written in different languages with no shared code.
- The repository would exceed practical size limits for CI/CD (> 10 GB, > 100K files).
- Services need completely independent dependency trees with conflicting versions.

### Trade-offs

| Advantage | Disadvantage |
|-----------|-------------|
| Shared code without publishing packages | More complex CI/CD (need to detect what changed) |
| Atomic cross-service changes | Repository size grows over time |
| Consistent tooling and standards | Build times can increase without optimization |
| Easier code review across service boundaries | Requires workspace manager tooling |
| Single source of truth for contracts | Risk of tight coupling if boundaries are not enforced |

### Directory Structure

```
project-root/
├── services/
│   ├── api-gateway/            # Service 1: API gateway / BFF
│   │   ├── src/
│   │   ├── tests/
│   │   ├── Dockerfile
│   │   └── [service-specific config]
│   ├── user-service/           # Service 2: User management
│   │   ├── src/
│   │   ├── tests/
│   │   ├── Dockerfile
│   │   └── [service-specific config]
│   └── notification-service/   # Service 3: Notifications
│       ├── src/
│       ├── tests/
│       ├── Dockerfile
│       └── [service-specific config]
├── packages/
│   ├── shared-models/          # Shared data models and types
│   │   ├── src/
│   │   ├── tests/
│   │   └── [package config]
│   ├── shared-utils/           # Shared utility functions
│   │   ├── src/
│   │   ├── tests/
│   │   └── [package config]
│   └── api-contracts/          # Shared API contracts (OpenAPI, protobuf)
│       ├── schemas/
│       └── generated/
├── infrastructure/
│   ├── docker-compose.yml      # Local development orchestration
│   ├── docker-compose.test.yml # Testing orchestration
│   └── [IaC files]             # Terraform, Pulumi, etc.
├── scripts/
│   ├── setup.sh                # Initial project setup
│   ├── dev.sh                  # Start all services for development
│   └── test-all.sh             # Run all tests across the monorepo
├── docs/
│   ├── architecture.md         # System-wide architecture
│   ├── service-map.md          # Which service does what
│   ├── deployment.md
│   └── tech-specs.md
├── .env.example
├── .gitignore
├── README.md
├── CHANGELOG.md
└── [workspace config: package.json, pnpm-workspace.yaml, Cargo.toml, etc.]
```

### Workspace Managers by Language

| Language | Workspace Manager | Configuration |
|----------|------------------|---------------|
| JavaScript/TypeScript | npm workspaces, pnpm workspaces, Turborepo, Nx | `package.json` with `workspaces` field |
| Python | uv workspaces, pants, or manual with relative path deps | `pyproject.toml` per service |
| Go | Go workspace (`go.work`) | `go.work` at root |
| C# | .NET solution with multiple projects | `.sln` file at root |
| Rust | Cargo workspace | `Cargo.toml` with `[workspace]` |
| Dart | Melos | `melos.yaml` at root |

### Dependency Management

- **Shared packages** live in `packages/` and are referenced as workspace dependencies
  (never published to external registries unless explicitly needed).
- **Service-specific dependencies** are declared in each service's package config.
- **Version alignment:** Shared packages use a single version across all consumers.
  If a breaking change is needed in a shared package, all consumers must be updated
  in the same commit.
- **Dependency hoisting:** Use workspace manager features to hoist common dependencies
  and reduce duplication.
- The agent MUST update `docs/tech-specs.md` with the shared package versions and
  their consumer services.

### Deploy Strategy

- **Independent service deploy:** Each service has its own Dockerfile and CI/CD pipeline
  job.
- **Change detection:** The CI/CD pipeline MUST detect which services/packages changed
  and only build/test/deploy those. Common approaches:
  - Git diff-based: Compare changed files against the last successful build.
  - Workspace manager support: Turborepo `--filter`, Nx `affected`, pants `changed`.
- **Shared package change:** When a shared package changes, ALL services that depend
  on it MUST be rebuilt and retested.
- **Deploy order:** If service A depends on service B's API, deploy B first (or ensure
  backward-compatible changes).
- **Local development:** `docker-compose.yml` to start all services together with
  hot-reload.

### Testing Strategy

- **Unit tests:** Per-service and per-package, run independently.
- **Integration tests:** Per-service, testing interactions with mocked or containerized
  dependencies (database, message queue).
- **Cross-service integration tests:** Test service-to-service communication with
  all services running (typically using `docker-compose.test.yml`).
- **Contract tests:** Shared API contracts in `packages/api-contracts/` serve as the
  source of truth. Services validate against these contracts.
- **CI optimization:** Run only the tests for changed services/packages + their
  downstream dependents.

---

## Pattern 3 — Microservices (Polyrepo)

### When to Use

- Services are owned by independent teams with different release cadences.
- Services need completely independent technology stacks (polyglot).
- Independent scaling is critical (one service handles 100x the traffic of another).
- The organization has mature DevOps practices (CI/CD, monitoring, incident response).
- Each service has a well-defined bounded context (Domain-Driven Design).

### When NOT to Use

- Small team (< 5 developers) — the operational overhead is not justified.
- The service boundaries are unclear or frequently changing.
- Shared code needs outweigh the benefits of independence.
- The organization lacks infrastructure automation and observability.
- MVP or early-stage product where speed of iteration is the priority.

### Trade-offs

| Advantage | Disadvantage |
|-----------|-------------|
| Full independence: tech, deploy, scale | Distributed system complexity (network, latency, partial failures) |
| Teams can work without coordination | Contract management becomes critical |
| Individual services are smaller and simpler | Debugging across services is harder |
| Failure isolation (one service down, others survive) | Data consistency across services is challenging |
| Technology best fit per service | Operational overhead (monitoring, logging, tracing per service) |

### Directory Structure (Per Repository)

Each service lives in its own repository with this structure:

```
service-name/
├── src/
│   ├── api/                    # HTTP/gRPC handlers
│   ├── services/               # Business logic
│   ├── repositories/           # Data access
│   ├── models/                 # Domain models
│   ├── infrastructure/         # External clients
│   ├── config/                 # Configuration
│   ├── common/                 # Service-internal shared code
│   └── main.[ext]              # Entry point
├── tests/
│   ├── unit/
│   ├── integration/
│   └── contract/               # Consumer-driven contract tests
├── api/
│   └── openapi.yaml            # OR protobuf definitions — API contract
├── docs/
│   ├── architecture.md         # Service-specific architecture
│   ├── api.md                  # API documentation (may be auto-generated)
│   ├── deployment.md
│   ├── tech-specs.md
│   └── runbook.md              # Operational runbook for this service
├── infrastructure/
│   ├── Dockerfile
│   ├── [helm chart or k8s manifests]
│   └── [IaC for service-specific resources]
├── .env.example
├── .gitignore
├── README.md
├── CHANGELOG.md
└── [package manager files]
```

### Cross-Service Architecture Documentation

While each service has its own repo, there SHOULD be a central documentation
repository or wiki that maintains:

- **Service catalog:** List of all services with owner, purpose, API contract
  location, and status page.
- **System architecture diagram:** High-level view of how services interact.
- **Communication matrix:** Which service calls which, protocol, authentication.
- **Shared standards:** Common logging format, tracing headers, error response
  schema, authentication approach.

### Dependency Management

- **No shared code libraries** between services (if code must be shared, publish
  it as a versioned package to an internal registry).
- **API contracts as the only coupling:** Services communicate exclusively via
  well-defined API contracts (OpenAPI, protobuf, AsyncAPI for events).
- **Contract versioning:** Use semantic versioning for APIs. Breaking changes
  require a new major version. Old versions must be supported during a deprecation
  period.
- Each service manages its own dependency tree independently.
- The agent MUST ensure that each service's `docs/tech-specs.md` is self-contained.

### Communication Patterns

| Pattern | When to Use | Technology Examples |
|---------|------------|-------------------|
| **Synchronous (request/response)** | Real-time queries, CRUD operations | REST, gRPC, GraphQL |
| **Asynchronous (events)** | Decoupled workflows, eventual consistency | RabbitMQ, Kafka, NATS, Redis Streams, SQS |
| **Saga pattern** | Distributed transactions across services | Orchestration (central coordinator) or Choreography (event-driven) |
| **API Gateway** | Client-facing aggregation, auth, rate limiting | Kong, Traefik, AWS API Gateway, custom BFF |

### Deploy Strategy

- **Fully independent deploy:** Each service has its own CI/CD pipeline in its
  own repository.
- **Contract-first development:** API contracts are defined and agreed upon BEFORE
  implementation. Changes to contracts trigger contract tests in consumer services.
- **Backward compatibility:** New versions of a service MUST be backward compatible
  with existing consumers. Use API versioning for breaking changes.
- **Feature flags:** Use feature flags to deploy code that is not yet active,
  enabling trunk-based development.
- **Canary or rolling deploy:** Deploy to a subset of instances first, monitor,
  then roll out fully.

### Testing Strategy

- **Unit tests:** Standard per-service testing (see monolith pattern).
- **Integration tests:** Per-service, with external dependencies mocked or
  containerized.
- **Contract tests:** Consumer-driven contract testing (e.g., Pact). Each consumer
  service defines the contract it expects from a provider. The provider runs these
  contracts in its CI pipeline to ensure it does not break consumers.
- **End-to-end tests:** Minimal, focused on critical business flows. Run in a
  staging environment with all services deployed. These are expensive and should
  NOT be the primary quality gate.
- **Chaos testing (optional but recommended):** Test resilience by injecting
  failures (network partitions, slow responses, service outages).

---

## Pattern 4 — Frontend + Backend Split

### When to Use

- The project has a distinct user-facing interface (web, mobile) and a separate
  API backend.
- Frontend and backend teams want to develop and deploy independently.
- The API serves (or will serve) multiple clients (web, mobile, third-party).
- The frontend is a Single Page Application (SPA), Server-Side Rendered (SSR) app,
  or mobile application.

### When NOT to Use

- The project is a pure API with no user-facing interface.
- The project is a simple server-rendered application (e.g., traditional MVC) where
  splitting adds unnecessary complexity.
- The frontend is trivially simple and tightly coupled to the backend (e.g., admin
  dashboard for a single backend).

### Trade-offs

| Advantage | Disadvantage |
|-----------|-------------|
| Independent FE/BE development and deployment | API contract must be maintained and versioned |
| Frontend can be served from CDN (performance) | CORS configuration required |
| Backend can serve multiple clients | Authentication flow is more complex (token-based) |
| Technology freedom (React frontend + Go backend) | Two build pipelines to maintain |
| Clear separation of concerns | Local development requires running both services |

### Organization Options

This pattern can be organized as:

**Option A — Monorepo (recommended for small-medium teams):**

```
project-root/
├── frontend/
│   ├── src/
│   │   ├── components/         # UI components
│   │   ├── pages/              # Page-level components / routes
│   │   ├── services/           # API client layer
│   │   ├── hooks/              # Custom hooks (React) or composables (Vue)
│   │   ├── stores/             # State management
│   │   ├── types/              # TypeScript types/interfaces
│   │   ├── utils/              # Frontend utilities
│   │   └── assets/             # Static assets (images, fonts, icons)
│   ├── tests/
│   │   ├── unit/
│   │   ├── integration/
│   │   └── e2e/                # Browser-based E2E tests
│   ├── public/                 # Static files served as-is
│   └── [framework config: next.config.js, vite.config.ts, etc.]
├── backend/
│   ├── src/                    # Standard monolith structure (see Pattern 1)
│   │   ├── api/
│   │   ├── services/
│   │   ├── repositories/
│   │   ├── models/
│   │   ├── config/
│   │   └── main.[ext]
│   ├── tests/
│   │   ├── unit/
│   │   ├── integration/
│   │   └── e2e/
│   └── [backend config]
├── shared/
│   ├── api-types/              # Shared TypeScript types (if both are TS)
│   └── api-contract/           # OpenAPI spec — single source of truth
│       └── openapi.yaml
├── infrastructure/
│   ├── docker-compose.yml      # Local dev: frontend + backend + DB
│   └── [IaC files]
├── docs/
│   ├── architecture.md
│   ├── api.md                  # Generated from openapi.yaml
│   ├── deployment.md
│   └── tech-specs.md
├── .env.example
├── .gitignore
├── README.md
├── CHANGELOG.md
└── [workspace config]
```

**Option B — Polyrepo (for larger teams or independent deploy cycles):**

Two separate repositories following the per-repo structure defined in Pattern 3.
The API contract (OpenAPI spec) is either:
- In the backend repo (backend owns the contract), or
- In a dedicated shared repo (both FE and BE consume it).

**Selection guidance:** Use monorepo (Option A) unless you have a clear need for
fully independent deploy pipelines with different teams. Start with monorepo —
it is easier to split later than to merge.

### API Contract Management

The API contract is the critical coupling between frontend and backend. It MUST be:

1. **Defined in OpenAPI (Swagger) format** as the single source of truth.
2. **Versioned** using the same Conventional Commits approach as the codebase.
3. **Used for code generation:**
   - Backend: Generate server stubs or validation middleware from the spec.
   - Frontend: Generate typed API client from the spec (e.g., `openapi-typescript`,
     `openapi-generator`).
4. **Validated in CI:** Both frontend and backend pipelines validate their code
   against the contract.

### Cross-Cutting Concerns

| Concern | Implementation |
|---------|---------------|
| **Authentication** | JWT-based (recommended) or session-based with CSRF. The auth flow must be documented in `docs/architecture.md`. |
| **CORS** | Backend MUST explicitly configure allowed origins, methods, and headers. Never use `*` in production. |
| **Error responses** | Standardized error format across all API endpoints (e.g., `{ "error": { "code": "...", "message": "...", "details": [...] } }`). |
| **API versioning** | URL-based (`/api/v1/...`) recommended for simplicity. Header-based as alternative. |
| **Rate limiting** | Implemented at the API gateway or backend level. Frontend must handle 429 responses gracefully. |
| **Environment configuration** | Frontend: build-time env variables. Backend: runtime env variables. Document all variables in `.env.example`. |

### Dependency Management

- **Frontend and backend have independent dependency trees** (separate
  `package.json`, `pyproject.toml`, etc.).
- **Shared types/contracts:** If both are TypeScript, shared types can live in a
  workspace package. If different languages, the OpenAPI spec is the shared contract
  and types are generated per-language.
- The agent MUST maintain `docs/tech-specs.md` with both frontend and backend
  dependency information, clearly separated.

### Deploy Strategy

- **Frontend:** Build static assets → Deploy to CDN or static hosting (Vercel,
  Netlify, S3 + CloudFront, GitHub Pages).
- **Backend:** Standard server deploy (container, serverless, PaaS).
- **Deploy order:** Backend first (new API version must be available before the
  frontend that consumes it). The backend MUST maintain backward compatibility
  during the transition window.
- **Local development:** `docker-compose.yml` that starts both frontend (with
  hot-reload) and backend (with hot-reload) + any required infrastructure (DB, cache).

### Testing Strategy

- **Frontend unit tests:** Component tests (React Testing Library, Vue Test Utils),
  hook/composable tests, utility tests.
- **Frontend integration tests:** Page-level tests with mocked API responses.
- **Frontend E2E tests:** Browser-based tests (Playwright, Cypress) against a
  running backend (staging or local).
- **Backend tests:** Follow monolith testing strategy (Pattern 1).
- **Contract tests:** Validate frontend API client and backend API implementation
  against the shared OpenAPI spec.
- **Cross-stack E2E tests:** Full flow tests (browser → frontend → backend → DB)
  in a staging environment. Keep these minimal and focused on critical paths.

---

## Hybrid Patterns and Evolution

### Starting Monolith, Extracting Services

A common and recommended evolution path:

```
Phase 1: MONOLITH (all features in one codebase)
    |
    v  [Feature boundaries become clear, scaling needs diverge]
    |
Phase 2: MODULAR MONOLITH (feature-based organization with strict module boundaries)
    |
    v  [Specific modules need independent scaling or different technology]
    |
Phase 3: MONOLITH + EXTRACTED SERVICES (critical modules extracted, rest stays)
    |
    v  [Most modules are independent, shared code is minimal]
    |
Phase 4: MICROSERVICES (if the scale and organizational structure justify it)
```

**Rules for the agent when recommending evolution:**

1. Never recommend jumping from Phase 1 to Phase 4. Each phase must be justified
   by concrete needs (scaling bottleneck, team independence requirement, technology
   mismatch).
2. Document the current phase and the criteria that would trigger a move to the
   next phase in the project's `docs/architecture.md`.
3. When extracting a service, create an ADR documenting: what is being extracted,
   why, what the communication pattern will be, and what the rollback plan is.

### Backend for Frontend (BFF)

When the project has multiple client types (web, mobile, IoT) that need different
API shapes:

- Create a thin BFF layer per client type that aggregates and transforms calls to
  backend services.
- The BFF lives in the frontend repo (or alongside it in a monorepo).
- The BFF is owned by the frontend team.
- The backend services expose a generic, client-agnostic API.

---

## Output

| Output | Format | Destination | Validation |
|--------|--------|-------------|------------|
| Selected architecture pattern | Text with rationale | `project-spec.md` section 2.1 | Pattern matches project characteristics from intake |
| Reference directory structure | Text (file tree) | `project-spec.md` section 7 | All standard directories present, no placeholders |
| Dependency strategy notes | Text | `project-spec.md` section 5 + `docs/tech-specs.md` | Strategy is consistent with selected pattern |
| Deploy approach | Text | `project-spec.md` section 9 | Deploy strategy matches pattern guidelines |
| Testing approach | Text | `project-spec.md` section 8 | Testing levels match pattern guidelines |
| ADR (if pattern is non-obvious) | Markdown | `docs/adr/` | Follows ADR template from `templates/adr-template.md` |

---

## Error Handling

| Error | Probable Cause | Resolution | Class |
|-------|---------------|------------|-------|
| No pattern clearly matches the project | Unusual or hybrid project requirements | Combine elements from multiple patterns. Document the hybrid approach in an ADR. Ask user for confirmation. | E4 |
| Selected pattern conflicts with a project constraint | Constraint not fully considered during selection | Re-evaluate against the selection matrix. If no pattern fits, document the trade-off accepted and the mitigation plan. | E4 |
| Monorepo tooling not available for the chosen language | Language ecosystem limitation | Fall back to manual workspace management with scripts. Document the limitation in `docs/tech-specs.md`. | E2 |
| Team disagrees with recommended pattern | Different priorities or experience | Present the trade-off analysis. Respect the team's decision. Document in ADR as "team preference override" with the agent's recommendation for reference. | E1 |
| Project outgrows its initial pattern | Natural evolution, increased scale or team size | Follow the "Hybrid Patterns and Evolution" section. Plan the migration incrementally, never as a big-bang rewrite. | E4 |

---

## Lessons Learned

| Date | Discovery | Action Taken |
|------|-----------|--------------|
| — | — | — |

---

## Changelog

| Version | Date | Author | Change Description |
|---------|------|--------|-------------------|
| 1.0 | 2026-03-24 | agent | Initial creation with 4 core patterns, selection matrix, and evolution guidelines |
