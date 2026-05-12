# CI/CD Setup — Pipeline e Version Control

## Metadata

- **ID:** DIR-014
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** DIR-010 (Code Standards), DIR-011 (Testing Strategy), DIR-012 (Security Guidelines), DIR-013 (Documentation Rules)
- **Tipo di progetto:** universale

---

## Obiettivo

Definire standard operativi per il version control (Git), le convenzioni di commit,
la gestione dei branch, la configurazione delle pipeline CI/CD e la generazione di
`.gitignore` specifici per progetto, garantendo che ogni progetto sia pronto per
l'integrazione e il deploy continuo fin dal primo commit.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio, il framework e il provider CI/CD sono stati selezionati e documentati.
- L'agent ha consultato `docs/tech-specs.md` per conoscere le versioni e i vincoli del progetto.
- Il repository Git e stato inizializzato (o sara inizializzato come primo step).

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Provider CI/CD | stringa | Il provider scelto (GitHub Actions, GitLab CI, Azure Pipelines, ecc.) |
| Linguaggio / framework | stringa | Stack tecnologico del progetto |
| Strategia di deploy | stringa | Ambiente target (staging, produzione, multi-ambiente) |
| `docs/tech-specs.md` | file Markdown | Specifiche tecniche con versioni e vincoli |
| Tipo di progetto | stringa | Monolite, monorepo, microservizi, libreria, CLI, ecc. |

---

## Procedura

### 1. Version Control — Git

#### 1.1 Inizializzazione del repository

Ogni progetto inizia con questa sequenza:

```
1. git init
2. Creare .gitignore (sezione 4 di questo documento)
3. Creare la struttura directory base
4. Primo commit: "chore: initialize project structure"
5. Creare il branch develop (se la strategia lo prevede)
6. Configurare il remote origin
```

**Regola:** Il primo commit deve contenere SOLO la struttura base, il `.gitignore`,
il `README.md` e i file di configurazione. Nessun codice applicativo nel primo commit.

#### 1.2 Strategia di branching

L'agent seleziona la strategia di branching in base alla complessita del progetto:

**Strategia semplificata (progetti piccoli, singolo sviluppatore/agent):**

```
main ─────────────────────────────────────────────
  │                                      ▲
  └── feature/auth ──── commits ─────────┘
  │                                      ▲
  └── fix/login-validation ── commits ───┘
```

- `main` — codice stabile, pronto per produzione
- `feature/<nome>` — per ogni nuova funzionalita
- `fix/<nome>` — per bugfix

**Strategia completa (progetti complessi, team, rilasci pianificati):**

```
main ────────────────────────────────────────────────────────────
  │                                          ▲              ▲
  └── develop ───────────────────────────────┤              │
        │                        ▲           │              │
        └── feature/auth ───────┘            │              │
        │                        ▲           │              │
        └── feature/payments ────┘           │              │
        │                                    │              │
        └── release/1.0.0 ──────────────────┘              │
                                                            │
  └── hotfix/critical-bug ──────────────────────────────────┘
```

- `main` — codice stabile, pronto per produzione. Solo merge da `release/*` o `hotfix/*`
- `develop` — integrazione delle feature. Codice testato ma non ancora rilasciato
- `feature/<nome>` — per ogni nuova funzionalita. Branch da `develop`, merge in `develop`
- `fix/<nome>` — per bugfix non urgenti. Branch da `develop`, merge in `develop`
- `release/<versione>` — preparazione rilascio. Branch da `develop`, merge in `main` e `develop`
- `hotfix/<nome>` — fix urgenti in produzione. Branch da `main`, merge in `main` e `develop`

**Criterio di selezione:**

| Condizione | Strategia consigliata |
|------------|----------------------|
| Progetto personale o MVP | Semplificata |
| Singolo agent, nessun rilascio formale | Semplificata |
| Team multiplo o rilasci pianificati | Completa |
| Progetto con ambienti staging/produzione separati | Completa |
| Libreria pubblica con versionamento semantico | Completa |

#### 1.3 Protezione dei branch

Per la strategia completa, configurare le protezioni del branch `main`:

- Richiede pull request per il merge (no push diretto)
- Richiede che la pipeline CI passi prima del merge
- Richiede review (se team > 1 persona)
- Non consentire force push
- Non consentire la cancellazione del branch

**Configurazione per GitHub (esempio):**

Il file `.github/branch-protection.md` documenta le regole applicate. La configurazione
effettiva viene fatta tramite le Settings del repository o tramite API/Terraform.

---

### 2. Convenzioni di Commit — Conventional Commits

L'agent segue rigorosamente lo standard [Conventional Commits](https://www.conventionalcommits.org/).

#### 2.1 Formato del messaggio di commit

```
<tipo>(<scope>): <descrizione>

[corpo opzionale]

[footer opzionale]
```

#### 2.2 Tipi consentiti

| Tipo | Quando usarlo | Esempio |
|------|---------------|---------|
| `feat` | Nuova funzionalita per l'utente finale | `feat(auth): add JWT token refresh` |
| `fix` | Correzione di un bug | `fix(api): handle null response from payment gateway` |
| `docs` | Modifiche alla documentazione | `docs(readme): update installation instructions` |
| `style` | Formattazione, punto e virgola, spazi (no cambi di logica) | `style(lint): apply prettier formatting` |
| `refactor` | Ristrutturazione codice senza cambiare comportamento | `refactor(auth): extract token validation to separate module` |
| `test` | Aggiunta o modifica di test | `test(api): add integration tests for /users endpoint` |
| `chore` | Manutenzione, configurazione, tooling | `chore(deps): update eslint to v9` |
| `ci` | Modifiche alla pipeline CI/CD | `ci(github): add caching for node_modules` |
| `perf` | Miglioramento di prestazioni | `perf(db): add index on users.email column` |
| `build` | Modifiche al sistema di build o dipendenze esterne | `build(docker): optimize multi-stage build` |
| `revert` | Revert di un commit precedente | `revert: feat(auth): add JWT token refresh` |

#### 2.3 Regole del messaggio

- La **descrizione** e in inglese, al presente imperativo ("add", non "added" o "adds").
- La descrizione inizia con lettera minuscola, non termina con punto.
- Lunghezza massima della prima riga: **72 caratteri**.
- Lo **scope** e opzionale ma raccomandato. Indica il modulo o l'area impattata.
- Il **corpo** (opzionale) spiega il "perche" della modifica, non il "cosa" (il diff mostra il cosa).
- Il **footer** contiene informazioni strutturate: `BREAKING CHANGE:`, `Closes #123`, `Refs #456`.

#### 2.4 Breaking changes

Le breaking changes DEVONO essere segnalate in due modi:

1. Con `!` dopo il tipo/scope: `feat(api)!: change authentication flow`
2. Con `BREAKING CHANGE:` nel footer del commit:

```
feat(api)!: change authentication flow

The authentication endpoint now requires an API key header
instead of query parameter authentication.

BREAKING CHANGE: The /auth endpoint no longer accepts the
`api_key` query parameter. Use the `X-API-Key` header instead.

Migration guide: docs/migration/v2-auth.md
```

#### 2.5 Commit atomici

Ogni commit deve rappresentare un **singolo cambiamento logico coerente**:

- Un commit = una modifica logica (non necessariamente un file)
- Se un cambiamento tocca 10 file ma e un singolo refactoring, e un commit
- Se un commit include una feature e un bugfix non correlato, dividerli in due commit
- Mai committare codice che non compila o non passa i test (il branch deve essere sempre "green")

**Anti-pattern da evitare:**

| Anti-pattern | Problema | Correzione |
|-------------|----------|------------|
| `fix: various fixes` | Non descrive cosa e stato corretto | Un commit per fix con descrizione specifica |
| `wip` / `temp` / `test commit` | Non informativo, inquina la history | Usare `git stash` o branch temporaneo |
| Commit con file non correlati | Rende difficile il revert | Separare in commit distinti |
| Mega-commit con centinaia di righe | Impossibile da revieware | Scomporre in commit incrementali |

---

### 3. Pipeline CI/CD

#### 3.1 Architettura della pipeline

Ogni pipeline segue una struttura a stadi sequenziali con job paralleli dove possibile:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            PIPELINE CI/CD                               │
│                                                                         │
│  ┌──────────┐   ┌──────────────┐   ┌───────────┐   ┌────────────────┐  │
│  │  STAGE 1  │   │   STAGE 2    │   │  STAGE 3  │   │    STAGE 4     │  │
│  │  Validate │──▶│    Test      │──▶│   Build   │──▶│    Deploy      │  │
│  └──────────┘   └──────────────┘   └───────────┘   └────────────────┘  │
│  ┌──────────┐   ┌──────────────┐   ┌───────────┐   ┌────────────────┐  │
│  │ • lint   │   │ • unit test  │   │ • build   │   │ • staging      │  │
│  │ • format │   │ • integration│   │ • package  │   │ • smoke test   │  │
│  │ • type   │   │ • security   │   │ • artifact │   │ • production   │  │
│  │   check  │   │   scan       │   │   upload   │   │   (manual)     │  │
│  └──────────┘   └──────────────┘   └───────────┘   └────────────────┘  │
│                                                                         │
│  ← Automatico →  ← Automatico →  ← Automatico →   ← Semi-automatico → │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 3.2 Stage 1 — Validate (Validazione statica)

Eseguito su **ogni push** e **ogni pull request**:

| Job | Tool | Obiettivo | Fail = blocco? |
|-----|------|-----------|----------------|
| Lint | Linter del linguaggio (ruff, eslint, ecc.) | Conformita allo stile | Si |
| Format check | Formatter del linguaggio | Formattazione consistente | Si |
| Type check | Type checker (mypy, tsc, ecc.) | Correttezza dei tipi | Si (se il linguaggio lo supporta) |
| Commit lint | `commitlint` o equivalente | Conformita Conventional Commits | Si (su PR) |

#### 3.3 Stage 2 — Test

Eseguito dopo il successo dello Stage 1:

| Job | Obiettivo | Fail = blocco? | Note |
|-----|-----------|----------------|------|
| Unit test | Verificare logica isolata | Si | Con report di copertura |
| Integration test | Verificare interazione tra moduli | Si | Puo richiedere servizi (DB, cache) |
| Security scan | Vulnerabilita nelle dipendenze | Si (severity >= HIGH) | `npm audit`, `pip-audit`, `dotnet list package --vulnerable` |

**Copertura minima:** La pipeline deve fallire se la copertura scende sotto la soglia
configurata nel progetto (default: 80% linee). La soglia e documentata in `docs/tech-specs.md`.

#### 3.4 Stage 3 — Build

Eseguito dopo il successo dello Stage 2:

| Job | Obiettivo | Output |
|-----|-----------|--------|
| Build | Compilare/bundlare l'applicazione | Artefatto deployabile |
| Package | Creare il pacchetto di distribuzione (Docker image, zip, npm package) | Immagine o pacchetto |
| Artifact upload | Caricare l'artefatto nel registry | Tag con versione e hash del commit |

**Regole di build:**

- La build deve essere **riproducibile**: stesso input = stesso output.
- La build non deve dipendere da stato locale della macchina (no path hardcoded).
- Le variabili d'ambiente di build sono documentate in `docs/deployment.md`.
- I Docker image usano **multi-stage build** per ridurre la dimensione finale.
- I Docker image non girano come root (user non-root nel Dockerfile).

#### 3.5 Stage 4 — Deploy

Configurazione dipendente dall'ambiente:

| Ambiente | Trigger | Approvazione | Smoke test |
|----------|---------|-------------|------------|
| Staging | Automatico su merge in `develop` (o `main` se strategia semplificata) | No | Si, automatico |
| Production | Manuale o su merge in `main` (strategia completa) | Si (almeno 1 approvatore) | Si, automatico |

**Smoke test post-deploy (obbligatori):**

```
SMOKE TEST CHECKLIST
    │
    ├── [ ] L'applicazione risponde (healthcheck endpoint)
    ├── [ ] Il database e raggiungibile
    ├── [ ] I servizi esterni critici sono raggiungibili
    ├── [ ] L'autenticazione funziona (se presente)
    ├── [ ] Un flusso utente critico funziona end-to-end
    └── [ ] I log sono visibili nel sistema di monitoring
```

**Rollback:** Ogni deploy deve avere una strategia di rollback documentata:

- Per container: rollback all'immagine precedente
- Per serverless: rollback alla versione precedente della funzione
- Per database migrations: script di rollback incluso nella migrazione
- Tempo massimo per decisione di rollback: **15 minuti** dopo il deploy

---

### 4. Gestione del .gitignore

#### 4.1 Principio

Il `.gitignore` e un file critico per la sicurezza e la pulizia del repository.
L'agent genera un `.gitignore` specifico per il tipo di progetto, partendo da un
template base e personalizzandolo.

#### 4.2 Regole universali (presenti in OGNI .gitignore)

```gitignore
# =============================================================================
# D.O.E. Framework — .gitignore
# Project type: <tipo-progetto>
# Generated: <YYYY-MM-DD>
# =============================================================================

# --- Secrets and credentials (NEVER commit) ---
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.pfx
credentials.json
service-account.json
**/secrets/

# --- Agent-specific files (NEVER commit) ---
CLAUDE.md
.cursorrules
.cursor/
.aider*
copilot-*

# --- OS-generated files ---
.DS_Store
.DS_Store?
._*
Thumbs.db
ehthumbs.db
Desktop.ini
$RECYCLE.BIN/

# --- IDE and editor files ---
.idea/
.vscode/
*.swp
*.swo
*~
*.sublime-project
*.sublime-workspace

# --- Temporary and generated files ---
tmp/
temp/
.tmp/
*.log
*.bak
*.orig
```

#### 4.3 Regole specifiche per linguaggio/framework

L'agent aggiunge le regole specifiche in base al tipo di progetto. Di seguito i
template per i linguaggi piu comuni. I template completi sono disponibili nella
directory `templates/.gitignore-templates/`.

**Python:**
```gitignore
# --- Python ---
__pycache__/
*.py[cod]
*$py.class
*.so
.Python
build/
develop-eggs/
dist/
downloads/
eggs/
.eggs/
lib/
lib64/
parts/
sdist/
var/
wheels/
*.egg-info/
.installed.cfg
*.egg
.venv/
venv/
ENV/
.pytest_cache/
.mypy_cache/
.ruff_cache/
htmlcov/
.coverage
.coverage.*
coverage.xml
*.cover
```

**JavaScript / TypeScript:**
```gitignore
# --- JavaScript / TypeScript ---
node_modules/
.npm
.yarn/
!.yarn/patches
!.yarn/plugins
!.yarn/releases
!.yarn/sdks
!.yarn/versions
dist/
build/
.next/
.nuxt/
.output/
.cache/
*.tsbuildinfo
.eslintcache
coverage/
```

**C# / .NET:**
```gitignore
# --- C# / .NET ---
[Dd]ebug/
[Rr]elease/
x64/
x86/
[Bb]in/
[Oo]bj/
[Ll]og/
[Ll]ogs/
*.user
*.suo
*.userosscache
*.sln.docstates
packages/
*.nupkg
project.lock.json
*.nuget.props
*.nuget.targets
.vs/
```

**Go:**
```gitignore
# --- Go ---
/vendor/
*.exe
*.exe~
*.dll
*.so
*.dylib
*.test
*.out
go.work
go.work.sum
```

**Dart / Flutter:**
```gitignore
# --- Dart / Flutter ---
.dart_tool/
.packages
build/
.flutter-plugins
.flutter-plugins-dependencies
*.iml
.metadata
pubspec.lock  # solo per librerie, NON per applicazioni
```

#### 4.4 Regole per infrastruttura e tooling

**Docker:**
```gitignore
# --- Docker ---
docker-compose.override.yml
.docker/
```

**Terraform:**
```gitignore
# --- Terraform ---
.terraform/
*.tfstate
*.tfstate.*
*.tfvars
!*.tfvars.example
crash.log
override.tf
override.tf.json
*_override.tf
*_override.tf.json
.terraformrc
terraform.rc
```

#### 4.5 Verifica del .gitignore

L'agent deve verificare il `.gitignore` prima di ogni commit:

- Controllare che `.env` e tutti i file di credenziali siano ignorati
- Controllare che `CLAUDE.md` (e equivalenti per altri agent) sia ignorato
- Controllare che i file generati dalla build siano ignorati
- Controllare che i file IDE-specifici siano ignorati
- Eseguire `git status` per verificare che nessun file sensibile sia tracciato

**Se un file sensibile e gia stato committato:**

```bash
# Rimuovere il file dal tracking senza cancellarlo dal filesystem
git rm --cached <file>
# Aggiungere al .gitignore
echo "<file>" >> .gitignore
# Committare la rimozione
git commit -m "chore(security): remove tracked sensitive file <file>"
```

**ATTENZIONE:** Se il file contiene segreti e e stato committato, i segreti sono
compromessi anche dopo la rimozione (restano nella history). Ruotare immediatamente
le credenziali e considerare `git filter-repo` per pulire la history se necessario.

---

### 5. Template di Pipeline per Provider

#### 5.1 GitHub Actions

Template base per un progetto generico:

```yaml
# .github/workflows/ci.yml
# =============================================================================
# CI Pipeline — D.O.E. Framework
# Project: <nome-progetto>
# Generated: <YYYY-MM-DD>
# =============================================================================

name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

# Cancel in-progress runs for the same branch/PR
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

env:
  # Define shared environment variables here
  NODE_VERSION: '20'      # Adjust per project
  PYTHON_VERSION: '3.12'  # Adjust per project

jobs:
  # ──────────────────────────────────────────────────
  # STAGE 1: Validate
  # ──────────────────────────────────────────────────
  validate:
    name: Lint & Type Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup runtime
        # Adjust: use setup-node, setup-python, setup-dotnet, etc.
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: 'npm'

      - name: Install dependencies
        run: npm ci

      - name: Lint
        run: npm run lint

      - name: Type check
        run: npm run typecheck

      - name: Format check
        run: npm run format:check

  # ──────────────────────────────────────────────────
  # STAGE 2: Test
  # ──────────────────────────────────────────────────
  test-unit:
    name: Unit Tests
    needs: validate
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup runtime
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: 'npm'

      - name: Install dependencies
        run: npm ci

      - name: Run unit tests
        run: npm run test:unit -- --coverage

      - name: Upload coverage report
        uses: actions/upload-artifact@v4
        with:
          name: coverage-report
          path: coverage/
          retention-days: 7

  test-integration:
    name: Integration Tests
    needs: validate
    runs-on: ubuntu-latest
    # Example: service containers for integration tests
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: test
          POSTGRES_PASSWORD: test
          POSTGRES_DB: testdb
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v4

      - name: Setup runtime
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: 'npm'

      - name: Install dependencies
        run: npm ci

      - name: Run integration tests
        env:
          DATABASE_URL: postgresql://test:test@localhost:5432/testdb
        run: npm run test:integration

  security-scan:
    name: Security Scan
    needs: validate
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup runtime
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: 'npm'

      - name: Install dependencies
        run: npm ci

      - name: Audit dependencies
        run: npm audit --audit-level=high
        continue-on-error: false

  # ──────────────────────────────────────────────────
  # STAGE 3: Build
  # ──────────────────────────────────────────────────
  build:
    name: Build
    needs: [test-unit, test-integration, security-scan]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup runtime
        uses: actions/setup-node@v4
        with:
          node-version: ${{ env.NODE_VERSION }}
          cache: 'npm'

      - name: Install dependencies
        run: npm ci

      - name: Build
        run: npm run build

      - name: Upload build artifact
        uses: actions/upload-artifact@v4
        with:
          name: build-output
          path: dist/
          retention-days: 7

  # ──────────────────────────────────────────────────
  # STAGE 4: Deploy (only on main branch)
  # ──────────────────────────────────────────────────
  deploy-staging:
    name: Deploy to Staging
    needs: build
    if: github.ref == 'refs/heads/develop' || (github.ref == 'refs/heads/main' && github.event_name == 'push')
    runs-on: ubuntu-latest
    environment: staging
    steps:
      - uses: actions/checkout@v4

      - name: Download build artifact
        uses: actions/download-artifact@v4
        with:
          name: build-output
          path: dist/

      - name: Deploy to staging
        run: |
          echo "Deploy to staging — replace with actual deploy command"
          # Example: aws s3 sync dist/ s3://staging-bucket/
          # Example: kubectl apply -f k8s/staging/

      - name: Run smoke tests
        run: |
          echo "Smoke tests — replace with actual smoke test command"
          # Example: npm run test:smoke -- --env=staging

  deploy-production:
    name: Deploy to Production
    needs: deploy-staging
    if: github.ref == 'refs/heads/main' && github.event_name == 'push'
    runs-on: ubuntu-latest
    environment:
      name: production
      # Requires manual approval configured in GitHub repo settings
    steps:
      - uses: actions/checkout@v4

      - name: Download build artifact
        uses: actions/download-artifact@v4
        with:
          name: build-output
          path: dist/

      - name: Deploy to production
        run: |
          echo "Deploy to production — replace with actual deploy command"

      - name: Run smoke tests
        run: |
          echo "Production smoke tests — replace with actual smoke test command"

      - name: Notify deployment
        if: success()
        run: |
          echo "Deployment successful — replace with actual notification"
          # Example: slack notification, email, webhook
```

#### 5.2 GitLab CI

Template base equivalente per GitLab:

```yaml
# .gitlab-ci.yml
# =============================================================================
# CI Pipeline — D.O.E. Framework
# Project: <nome-progetto>
# Generated: <YYYY-MM-DD>
# =============================================================================

stages:
  - validate
  - test
  - build
  - deploy

variables:
  NODE_VERSION: '20'

# Cache shared across jobs
cache:
  key: ${CI_COMMIT_REF_SLUG}
  paths:
    - node_modules/
  policy: pull

# ──────────────────────────────────────────────────
# STAGE 1: Validate
# ──────────────────────────────────────────────────
lint:
  stage: validate
  image: node:${NODE_VERSION}
  cache:
    key: ${CI_COMMIT_REF_SLUG}
    paths:
      - node_modules/
    policy: pull-push
  script:
    - npm ci
    - npm run lint
    - npm run typecheck
    - npm run format:check

# ──────────────────────────────────────────────────
# STAGE 2: Test
# ──────────────────────────────────────────────────
test-unit:
  stage: test
  image: node:${NODE_VERSION}
  script:
    - npm ci
    - npm run test:unit -- --coverage
  coverage: '/Statements\s*:\s*(\d+\.?\d*%)/'
  artifacts:
    reports:
      coverage_report:
        coverage_format: cobertura
        path: coverage/cobertura-coverage.xml
    expire_in: 7 days

test-integration:
  stage: test
  image: node:${NODE_VERSION}
  services:
    - postgres:16
  variables:
    POSTGRES_USER: test
    POSTGRES_PASSWORD: test
    POSTGRES_DB: testdb
    DATABASE_URL: postgresql://test:test@postgres:5432/testdb
  script:
    - npm ci
    - npm run test:integration

security-scan:
  stage: test
  image: node:${NODE_VERSION}
  script:
    - npm ci
    - npm audit --audit-level=high
  allow_failure: false

# ──────────────────────────────────────────────────
# STAGE 3: Build
# ──────────────────────────────────────────────────
build:
  stage: build
  image: node:${NODE_VERSION}
  script:
    - npm ci
    - npm run build
  artifacts:
    paths:
      - dist/
    expire_in: 7 days

# ──────────────────────────────────────────────────
# STAGE 4: Deploy
# ──────────────────────────────────────────────────
deploy-staging:
  stage: deploy
  image: node:${NODE_VERSION}
  environment:
    name: staging
    url: https://staging.example.com
  script:
    - echo "Deploy to staging"
  rules:
    - if: $CI_COMMIT_BRANCH == "develop"
    - if: $CI_COMMIT_BRANCH == "main"

deploy-production:
  stage: deploy
  image: node:${NODE_VERSION}
  environment:
    name: production
    url: https://example.com
  script:
    - echo "Deploy to production"
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
      when: manual
```

#### 5.3 Azure Pipelines

Template base per Azure DevOps:

```yaml
# azure-pipelines.yml
# =============================================================================
# CI Pipeline — D.O.E. Framework
# Project: <nome-progetto>
# Generated: <YYYY-MM-DD>
# =============================================================================

trigger:
  branches:
    include:
      - main
      - develop

pr:
  branches:
    include:
      - main
      - develop

pool:
  vmImage: 'ubuntu-latest'

variables:
  nodeVersion: '20'

stages:
  # ──────────────────────────────────────────────────
  # STAGE 1: Validate
  # ──────────────────────────────────────────────────
  - stage: Validate
    displayName: 'Validate'
    jobs:
      - job: LintAndTypeCheck
        displayName: 'Lint & Type Check'
        steps:
          - task: NodeTool@0
            inputs:
              versionSpec: $(nodeVersion)

          - script: npm ci
            displayName: 'Install dependencies'

          - script: npm run lint
            displayName: 'Lint'

          - script: npm run typecheck
            displayName: 'Type check'

          - script: npm run format:check
            displayName: 'Format check'

  # ──────────────────────────────────────────────────
  # STAGE 2: Test
  # ──────────────────────────────────────────────────
  - stage: Test
    displayName: 'Test'
    dependsOn: Validate
    jobs:
      - job: UnitTests
        displayName: 'Unit Tests'
        steps:
          - task: NodeTool@0
            inputs:
              versionSpec: $(nodeVersion)

          - script: npm ci
            displayName: 'Install dependencies'

          - script: npm run test:unit -- --coverage
            displayName: 'Run unit tests'

          - task: PublishCodeCoverageResults@2
            inputs:
              summaryFileLocation: coverage/cobertura-coverage.xml
            displayName: 'Publish coverage'

      - job: SecurityScan
        displayName: 'Security Scan'
        steps:
          - task: NodeTool@0
            inputs:
              versionSpec: $(nodeVersion)

          - script: npm ci
            displayName: 'Install dependencies'

          - script: npm audit --audit-level=high
            displayName: 'Audit dependencies'

  # ──────────────────────────────────────────────────
  # STAGE 3: Build
  # ──────────────────────────────────────────────────
  - stage: Build
    displayName: 'Build'
    dependsOn: Test
    jobs:
      - job: BuildApp
        displayName: 'Build Application'
        steps:
          - task: NodeTool@0
            inputs:
              versionSpec: $(nodeVersion)

          - script: npm ci
            displayName: 'Install dependencies'

          - script: npm run build
            displayName: 'Build'

          - publish: dist/
            artifact: build-output
            displayName: 'Publish artifact'

  # ──────────────────────────────────────────────────
  # STAGE 4: Deploy
  # ──────────────────────────────────────────────────
  - stage: DeployStaging
    displayName: 'Deploy to Staging'
    dependsOn: Build
    condition: and(succeeded(), eq(variables['Build.SourceBranch'], 'refs/heads/main'))
    jobs:
      - deployment: DeployStaging
        displayName: 'Deploy to Staging'
        environment: staging
        strategy:
          runOnce:
            deploy:
              steps:
                - script: echo "Deploy to staging"
                  displayName: 'Deploy'

  - stage: DeployProduction
    displayName: 'Deploy to Production'
    dependsOn: DeployStaging
    condition: and(succeeded(), eq(variables['Build.SourceBranch'], 'refs/heads/main'))
    jobs:
      - deployment: DeployProduction
        displayName: 'Deploy to Production'
        environment: production  # Configure approval in Azure DevOps
        strategy:
          runOnce:
            deploy:
              steps:
                - script: echo "Deploy to production"
                  displayName: 'Deploy'
```

---

### 6. Versionamento Semantico

#### 6.1 Principio

Ogni progetto che viene rilasciato (applicazione, libreria, API) segue il
[Semantic Versioning 2.0.0](https://semver.org/):

```
MAJOR.MINOR.PATCH

MAJOR — breaking changes (incompatibilita con versione precedente)
MINOR — nuove funzionalita retrocompatibili
PATCH — bugfix retrocompatibili
```

#### 6.2 Pre-release e build metadata

```
1.0.0-alpha.1    — versione alpha (instabile)
1.0.0-beta.1     — versione beta (feature-complete, possibili bug)
1.0.0-rc.1       — release candidate (pronta per produzione, test finali)
1.0.0+build.123  — metadata di build (non influenza la precedenza)
```

#### 6.3 Quando incrementare

| Tipo di modifica | Incremento | Esempio |
|-----------------|------------|---------|
| Bug fix, patch di sicurezza | PATCH | 1.2.3 → 1.2.4 |
| Nuova funzionalita senza breaking change | MINOR | 1.2.4 → 1.3.0 |
| Qualsiasi breaking change | MAJOR | 1.3.0 → 2.0.0 |
| Prima release stabile | — | 0.x.y → 1.0.0 |

**Regola per versioni 0.x.y:** Durante lo sviluppo iniziale (prima della 1.0.0),
qualsiasi modifica puo essere breaking. La 0.x.y e considerata instabile per definizione.

#### 6.4 Automazione del versionamento

L'agent configura (dove possibile) strumenti per automatizzare il versionamento
basandosi sui commit Conventional Commits:

| Strumento | Ecosistema | Funzionalita |
|-----------|------------|-------------|
| `semantic-release` | Node.js | Versioning + changelog + release automatici |
| `python-semantic-release` | Python | Equivalente per Python |
| `versionize` | .NET | Conventional Commits per .NET |
| `goreleaser` | Go | Build + release automatizzati |

---

### 7. Documentazione della Pipeline

L'agent DEVE documentare la pipeline in `docs/deployment.md` includendo:

```markdown
## Pipeline CI/CD

### Provider
<GitHub Actions | GitLab CI | Azure Pipelines | ...>

### Stadi
1. **Validate:** Lint, type check, format check
2. **Test:** Unit test, integration test, security scan
3. **Build:** Build applicazione, creazione artefatto
4. **Deploy:** Staging (automatico), Production (manuale)

### Segreti richiesti
| Nome | Descrizione | Dove configurarlo |
|------|-------------|------------------|
| `DATABASE_URL` | Connection string del database | Repository Secrets |
| `API_KEY` | Chiave API per servizio esterno | Repository Secrets |
| `DEPLOY_TOKEN` | Token per il deploy | Environment Secrets (production) |

### Ambienti
| Ambiente | URL | Branch trigger | Approvazione |
|----------|-----|---------------|-------------|
| Staging | https://staging.example.com | develop / main | No |
| Production | https://example.com | main | Si |

### Rollback
<Procedura di rollback specifica per il progetto>
```

---

### 8. Checklist CI/CD per Nuovo Progetto

Quando l'agent configura CI/CD per un nuovo progetto, verifica questa checklist:

```
CHECKLIST CI/CD SETUP
    │
    ├── Version Control
    │   ├── [ ] Repository Git inizializzato
    │   ├── [ ] .gitignore configurato (sezione 4)
    │   ├── [ ] CLAUDE.md nel .gitignore
    │   ├── [ ] .env e file di credenziali nel .gitignore
    │   ├── [ ] Strategia di branching scelta e documentata
    │   ├── [ ] Branch protection configurata (se strategia completa)
    │   └── [ ] Primo commit con struttura base
    │
    ├── Commit Conventions
    │   ├── [ ] Conventional Commits adottati
    │   ├── [ ] commitlint configurato (opzionale ma raccomandato)
    │   └── [ ] Husky o equivalente per pre-commit hooks (opzionale)
    │
    ├── Pipeline CI/CD
    │   ├── [ ] File di pipeline creato per il provider scelto
    │   ├── [ ] Stage Validate configurato (lint, typecheck, format)
    │   ├── [ ] Stage Test configurato (unit, integration, security scan)
    │   ├── [ ] Stage Build configurato
    │   ├── [ ] Stage Deploy configurato (staging + production)
    │   ├── [ ] Caching delle dipendenze configurato
    │   ├── [ ] Concurrency/cancellation configurata
    │   └── [ ] Smoke test post-deploy definiti
    │
    ├── Documentazione
    │   ├── [ ] docs/deployment.md creato e completo
    │   ├── [ ] Segreti necessari documentati
    │   ├── [ ] Procedura di rollback documentata
    │   └── [ ] Ambienti e URL documentati
    │
    └── Versionamento
        ├── [ ] Semantic Versioning adottato
        ├── [ ] CHANGELOG.md inizializzato
        └── [ ] Automazione del versionamento configurata (se applicabile)
```

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| File `.gitignore` | gitignore | Root del progetto |
| File di pipeline CI/CD | YAML | `.github/workflows/`, `.gitlab-ci.yml`, o `azure-pipelines.yml` |
| Documentazione deploy | Markdown | `docs/deployment.md` |
| Configurazione commit hooks | JSON/YAML | Root del progetto (`commitlint.config.js`, `.husky/`) |
| CHANGELOG.md iniziale | Markdown | Root del progetto |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| Pipeline fallisce allo stage lint | Codice non conforme agli standard | Eseguire il linter localmente, correggere, ri-committare |
| Pipeline fallisce allo stage test | Test rotti o copertura insufficiente | Eseguire i test localmente, correggere il codice o i test |
| Pipeline fallisce allo stage build | Dipendenze mancanti o incompatibili | Verificare `docs/tech-specs.md`, aggiornare le dipendenze |
| Pipeline fallisce allo stage deploy | Credenziali mancanti o scadute | Verificare i segreti nel repository settings, ruotare se necessario |
| File sensibile committato per errore | `.gitignore` incompleto o file aggiunto prima del `.gitignore` | Rimuovere con `git rm --cached`, aggiornare `.gitignore`, ruotare le credenziali |
| Conflitto di merge su branch protetto | Branch non aggiornato rispetto a `main`/`develop` | Rebase o merge del branch base, risolvere conflitti, ri-pushare |
| Smoke test fallisce dopo deploy | Configurazione ambiente errata | Verificare variabili d'ambiente, connessioni DB, servizi esterni. Rollback se necessario |
| Caching della pipeline non funziona | Chiave di cache errata o path mancante | Verificare la configurazione di cache nel file di pipeline |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
