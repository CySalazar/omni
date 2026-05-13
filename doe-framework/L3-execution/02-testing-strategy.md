# Testing Strategy — Strategia di Testing Completa

## Metadata

- **ID:** DIR-011
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** DIR-010 (Code Standards)
- **Tipo di progetto:** universale

---

## Obiettivo

Definire una strategia di testing strutturata a 4 livelli che garantisca la correttezza,
l'affidabilita e la stabilita di ogni progetto prodotto dall'agent, indipendentemente
dal linguaggio, dal framework o dal tipo di applicazione.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio e il framework sono stati selezionati e documentati.
- Gli standard di codice (DIR-L3-001) sono stati consultati e configurati.
- L'agent ha consultato `docs/tech-specs.md` per conoscere le versioni e i vincoli del progetto.

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Linguaggio scelto | stringa | Il linguaggio di programmazione del progetto |
| Framework scelto | stringa | Il framework principale (se presente) |
| Framework di test scelto | stringa | Il framework di testing (se gia deciso dall'utente) |
| `docs/tech-specs.md` | file Markdown | Specifiche tecniche con versioni e vincoli di compatibilita |
| Specifica tecnica | file Markdown | Requisiti funzionali e non funzionali del progetto |
| Tipo di applicazione | stringa | WebApp, API, CLI, Bot, Pipeline, Libreria, ecc. |

---

## Procedura

### 1. Livello 1 — Unit Test

I test unitari verificano il comportamento corretto di singole funzioni, metodi o classi
in isolamento. Sono la base della piramide dei test e devono essere i piu numerosi,
i piu veloci e i piu deterministici.

#### 1.1 Regole fondamentali

- **Ogni funzione pubblica ha almeno un test.** Le funzioni private possono essere testate
  indirettamente attraverso le funzioni pubbliche che le usano, ma se contengono logica
  complessa devono avere test dedicati.
- **Copertura minima: 80% delle linee** (configurabile per progetto tramite `docs/tech-specs.md`).
  La copertura e un indicatore, non un obiettivo in se: il 100% non garantisce qualita,
  ma sotto l'80% indica probabili lacune.
- **Ogni test verifica UNA cosa.** Un test con piu asserzioni non correlate e un segnale
  che va scomposto in test separati.
- **I test sono indipendenti.** L'ordine di esecuzione non deve influenzare il risultato.
  Nessun test deve dipendere dallo stato lasciato da un altro test.
- **I test sono deterministici.** Lo stesso test eseguito N volte deve dare N volte
  lo stesso risultato. Evitare dipendenze da tempo, rete, file system o random non
  controllati (usare seed quando serve randomness).

#### 1.2 Naming convention per i test

Il nome di un test deve comunicare immediatamente cosa testa, in quale condizione
e quale risultato si aspetta. Il pattern raccomandato e:

```
test_<cosa_testa>_<condizione>_<risultato_atteso>
```

Esempi:

| Nome del test | Cosa comunica |
|---------------|---------------|
| `test_calculate_total_with_discount_returns_reduced_price` | Testa il calcolo totale con sconto applicato |
| `test_validate_email_with_empty_string_raises_validation_error` | Testa la validazione email con stringa vuota |
| `test_create_user_with_duplicate_email_returns_conflict` | Testa la creazione utente con email duplicata |
| `test_parse_config_with_missing_required_field_raises_config_error` | Testa il parsing config con campo mancante |

Per linguaggi che usano classi di test (C#, Java), raggruppare i test per classe/metodo testato:

```
class TestUserService:
    test_create_user_with_valid_data_returns_user
    test_create_user_with_duplicate_email_raises_conflict
    test_get_user_by_id_with_existing_id_returns_user
    test_get_user_by_id_with_nonexistent_id_raises_not_found
```

#### 1.3 Casi da testare obbligatoriamente

Per ogni funzione pubblica, l'agent deve considerare almeno questi scenari:

| Categoria | Descrizione | Esempio |
|-----------|-------------|---------|
| **Happy path** | Input valido, flusso normale | `calculate_total([item1, item2])` restituisce la somma corretta |
| **Input nullo/vuoto** | `null`, `None`, stringa vuota, lista vuota, `0` | `calculate_total([])` restituisce `0` o lancia eccezione appropriata |
| **Input ai limiti** | Valori minimi, massimi, limiti di tipo | `calculate_total` con `MAX_INT`, con 1 solo item, con 10000 item |
| **Input invalido** | Tipo sbagliato, formato errato, fuori range | `calculate_total("not_a_list")` lancia `TypeError` |
| **Casi limite del dominio** | Regole di business specifiche | Sconto > 100%, quantita negativa, data nel passato |
| **Errori attesi** | Eccezioni che la funzione deve lanciare | `get_user(nonexistent_id)` lancia `NotFoundError` |

#### 1.4 Pattern di struttura per un test (AAA)

Ogni test deve seguire il pattern **Arrange-Act-Assert** (AAA):

```python
def test_calculate_total_with_discount_returns_reduced_price():
    # Arrange — Prepara i dati e le dipendenze
    items = [
        OrderItem(name="Widget", price=100.00, quantity=2),
        OrderItem(name="Gadget", price=50.00, quantity=1),
    ]
    discount = Discount(percentage=10)

    # Act — Esegui l'azione da testare
    result = calculate_total(items, discount=discount)

    # Assert — Verifica il risultato
    assert result == 225.00  # (200 + 50) - 10%
```

```typescript
describe("calculateTotal", () => {
  it("should return reduced price when discount is applied", () => {
    // Arrange
    const items: OrderItem[] = [
      { name: "Widget", price: 100.0, quantity: 2 },
      { name: "Gadget", price: 50.0, quantity: 1 },
    ];
    const discount: Discount = { percentage: 10 };

    // Act
    const result = calculateTotal(items, discount);

    // Assert
    expect(result).toBe(225.0); // (200 + 50) - 10%
  });
});
```

```csharp
[Fact]
public void CalculateTotal_WithDiscount_ReturnsReducedPrice()
{
    // Arrange
    var items = new List<OrderItem>
    {
        new OrderItem("Widget", 100.00m, 2),
        new OrderItem("Gadget", 50.00m, 1)
    };
    var discount = new Discount(percentage: 10);

    // Act
    var result = _calculator.CalculateTotal(items, discount);

    // Assert
    Assert.Equal(225.00m, result); // (200 + 50) - 10%
}
```

#### 1.5 Framework di test raccomandati

| Linguaggio | Framework | Runner | Coverage | Note |
|------------|-----------|--------|----------|------|
| Python | `pytest` | built-in | `pytest-cov` (`coverage.py`) | Preferito per semplicita e plugin ecosystem |
| JavaScript/TypeScript | `vitest` o `jest` | built-in | built-in (v8/istanbul) | `vitest` preferito per progetti Vite; `jest` per legacy |
| C# | `xUnit` o `NUnit` | `dotnet test` | `coverlet` | `xUnit` preferito per progetti nuovi |
| Go | `testing` (stdlib) | `go test` | `go test -cover` | Nessun framework esterno necessario |
| Dart | `test` (package) | `dart test` | `coverage` (package) | Standard de facto |

**Regola:** Il framework di test deve essere configurato nel progetto e documentato
in `docs/tech-specs.md`. I test devono poter essere eseguiti con un singolo comando
dalla root del progetto (es. `pytest`, `npm test`, `dotnet test`, `go test ./...`).

---

### 2. Livello 2 — Integration Test

I test di integrazione verificano che **moduli diversi funzionino correttamente insieme**.
A differenza degli unit test, coinvolgono piu componenti reali e testano le interfacce
tra di essi.

#### 2.1 Cosa testano i test di integrazione

| Aspetto | Descrizione | Esempio |
|---------|-------------|---------|
| **Interazione tra moduli** | Due o piu componenti del sistema collaborano correttamente | Il servizio utenti chiama correttamente il repository e il servizio email |
| **Contratti API** | Le API rispettano lo schema dichiarato (request/response) | L'endpoint POST /users accetta il payload documentato e ritorna il formato atteso |
| **Schema database** | Le query funzionano contro lo schema reale | Le migrazioni creano le tabelle corrette; le query ORM producono SQL valido |
| **Flussi multi-step** | Sequenze di operazioni producono il risultato atteso | Registrazione → Login → Accesso risorsa protetta funziona end-to-end nel backend |
| **Configurazione** | L'applicazione si avvia correttamente con la configurazione reale | Il servizio si connette al database, carica le configurazioni, registra le route |

#### 2.2 Gestione delle dipendenze esterne

Le dipendenze esterne (API di terze parti, servizi cloud, sistemi di pagamento)
devono essere gestite con una strategia esplicita:

| Strategia | Quando usarla | Come |
|-----------|---------------|------|
| **Mock** | API esterne non controllabili, servizi a pagamento | Librerie di mocking (`unittest.mock`, `jest.mock`, `Moq`, `gomock`) |
| **Stub/Fake** | Database, cache, code di messaggi | Implementazioni in-memory (`SQLite in-memory`, `fake-redis`, `testcontainers`) |
| **Container** | Dipendenze che devono essere realistiche | Docker / Testcontainers per database, broker, ecc. |
| **Sandbox** | API esterne che forniscono ambiente di test | Usare l'ambiente sandbox del provider (Stripe test mode, ecc.) |

**Principio:** Mockare il meno possibile. Piu il test e vicino alla realta, piu e utile.
Usare mock solo quando la dipendenza esterna e lenta, costosa, non deterministica
o non disponibile nell'ambiente di test.

#### 2.3 Contract testing

Quando il progetto espone o consuma API, l'agent deve implementare **contract testing**
per verificare che i contratti (schema di request/response) siano rispettati.

Per API REST:
- Validare le risposte contro lo schema OpenAPI/JSON Schema
- Verificare che i codici di stato siano corretti per ogni scenario
- Testare la negoziazione dei content type
- Verificare header obbligatori (CORS, autenticazione, rate limiting)

Per API gRPC:
- Validare i messaggi contro le definizioni protobuf
- Testare la gestione degli errori gRPC (codici di stato)

Per eventi/messaggi:
- Validare i payload contro lo schema (Avro, JSON Schema)
- Testare la serializzazione/deserializzazione

Strumenti raccomandati per contract testing:

| Linguaggio | Strumento | Note |
|------------|-----------|------|
| Multilingua | `Pact` | Standard de facto per consumer-driven contract testing |
| Python | `schemathesis` | Generazione automatica di test da schema OpenAPI |
| TypeScript | `supertest` + `zod` | Test HTTP con validazione schema |
| C# | `PactNet` | Implementazione .NET di Pact |
| Go | `pact-go` | Implementazione Go di Pact |

#### 2.4 Struttura dei test di integrazione

I test di integrazione devono essere separati fisicamente dagli unit test:

```
tests/
├── unit/                   # Test unitari — veloci, senza dipendenze
│   ├── test_user_model.py
│   └── test_calculator.py
├── integration/            # Test di integrazione — piu lenti, con dipendenze
│   ├── test_user_api.py
│   ├── test_database.py
│   └── test_auth_flow.py
└── e2e/                    # Test end-to-end — i piu lenti, full stack
    ├── test_registration_flow.py
    └── test_checkout_flow.py
```

Questa separazione permette di eseguire i test in modo selettivo:
- `pytest tests/unit/` — solo unit test (veloce, in CI ad ogni commit)
- `pytest tests/integration/` — solo integration (in CI ad ogni PR)
- `pytest tests/e2e/` — solo E2E (in CI pre-deploy o manualmente)

#### 2.5 Setup e teardown

I test di integrazione spesso richiedono setup complesso (database, servizi).
L'agent deve implementare:

- **Fixture condivise** per risorse costose (connessione DB, container Docker)
- **Isolamento tra test** tramite transazioni rollback, database dedicati per test,
  o cleanup esplicito
- **Timeout** per evitare test che pendono indefinitamente
- **Skip condizionale** per test che richiedono risorse non disponibili in tutti
  gli ambienti (es. Docker non disponibile in CI leggera)

Pattern raccomandato per database:

```python
# Python con pytest — fixture con transazione rollback
import pytest
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

@pytest.fixture(scope="session")
def db_engine():
    """Create a test database engine (once per test session)."""
    engine = create_engine("sqlite:///:memory:")
    Base.metadata.create_all(engine)
    yield engine
    engine.dispose()

@pytest.fixture(scope="function")
def db_session(db_engine):
    """Create a new database session for each test (with rollback)."""
    connection = db_engine.connect()
    transaction = connection.begin()
    session = Session(bind=connection)

    yield session

    session.close()
    transaction.rollback()
    connection.close()
```

```csharp
// C# con xUnit — fixture con database in-memory
public class DatabaseFixture : IDisposable
{
    public AppDbContext Context { get; }

    public DatabaseFixture()
    {
        var options = new DbContextOptionsBuilder<AppDbContext>()
            .UseInMemoryDatabase(databaseName: Guid.NewGuid().ToString())
            .Options;

        Context = new AppDbContext(options);
        Context.Database.EnsureCreated();
    }

    public void Dispose()
    {
        Context.Dispose();
    }
}

public class UserServiceTests : IClassFixture<DatabaseFixture>
{
    private readonly AppDbContext _context;

    public UserServiceTests(DatabaseFixture fixture)
    {
        _context = fixture.Context;
    }
}
```

---

### 3. Livello 3 — End-to-End Test (E2E)

I test end-to-end simulano **flussi completi dal punto di vista dell'utente finale**.
Verificano che l'intero sistema funzioni correttamente quando tutti i componenti
sono collegati, includendo UI, API, database e servizi esterni.

#### 3.1 Quando sono necessari

I test E2E sono obbligatori quando il progetto ha:
- Un'interfaccia utente (web, mobile, desktop)
- Un flusso critico multi-step (registrazione, checkout, onboarding)
- Integrazioni con servizi esterni critici per il business

I test E2E sono opzionali (ma raccomandati) quando il progetto e:
- Una libreria senza UI
- Un servizio backend semplice (dove i test di integrazione coprono i flussi)
- Uno script o tool CLI semplice

#### 3.2 Strumenti per tipo di applicazione

| Tipo di applicazione | Strumento raccomandato | Alternative | Note |
|---------------------|----------------------|-------------|------|
| **WebApp (browser)** | `Playwright` | `Cypress`, `Selenium` | Playwright preferito per multi-browser e stabilita |
| **API REST/GraphQL** | `pytest` + `httpx` / `supertest` | `Postman/Newman`, `k6` | Suite di chiamate sequenziali che simulano un utente |
| **CLI** | `pytest` + `subprocess` / `oclif testing` | `bats` (Bash) | Test con input/output simulati da terminale |
| **Bot (Telegram, Discord, ecc.)** | Mock del client + test conversazionali | Framework-specific test utils | Simulare conversazioni con sequenze di messaggi |
| **Mobile** | `Maestro` | `Appium`, `Detox` (React Native) | Flussi utente su emulatore/dispositivo |
| **Desktop** | `Playwright` (Electron) | `PyAutoGUI`, `WinAppDriver` | Dipende dal framework desktop |

#### 3.3 Principi per test E2E efficaci

- **Testare i flussi critici, non ogni dettaglio.** I test E2E sono lenti e fragili.
  Concentrarsi sui percorsi utente piu importanti (happy path dei flussi critici).
- **Usare selettori stabili.** Per WebApp: preferire `data-testid` a selettori CSS
  o XPath fragili. Per API: usare URL stabili e payload documentati.
- **Gestire l'attesa in modo esplicito.** Mai usare `sleep()` fissi. Usare wait
  condizionali (es. `waitForSelector`, `waitForResponse`, polling con timeout).
- **Isolare i dati di test.** Ogni test E2E deve creare i propri dati (via API o
  seed) e pulirli dopo l'esecuzione. Non dipendere da dati pre-esistenti.
- **Gestire la flakiness.** I test E2E possono essere flaky (fallire in modo
  intermittente). Strategie di mitigazione:
  - Retry automatico (max 2 tentativi) per test isolati
  - Screenshot/video su fallimento per diagnosi
  - Quarantine: test sistematicamente flaky vengono spostati in una suite separata
    e debuggati fuori dal flusso CI principale

#### 3.4 Struttura di un test E2E (WebApp con Playwright)

```typescript
import { test, expect } from "@playwright/test";

test.describe("User Registration Flow", () => {
  test("should allow a new user to register, receive confirmation, and login", async ({
    page,
  }) => {
    // Step 1 — Navigate to registration page
    await page.goto("/register");
    await expect(page.getByRole("heading", { name: "Create Account" })).toBeVisible();

    // Step 2 — Fill in registration form
    await page.getByLabel("Email").fill("test-user@example.com");
    await page.getByLabel("Password").fill("SecureP@ss123!");
    await page.getByLabel("Confirm Password").fill("SecureP@ss123!");

    // Step 3 — Submit and verify confirmation
    await page.getByRole("button", { name: "Register" }).click();
    await expect(page.getByText("Registration successful")).toBeVisible();

    // Step 4 — Login with new credentials
    await page.goto("/login");
    await page.getByLabel("Email").fill("test-user@example.com");
    await page.getByLabel("Password").fill("SecureP@ss123!");
    await page.getByRole("button", { name: "Login" }).click();

    // Step 5 — Verify dashboard access
    await expect(page).toHaveURL(/\/dashboard/);
    await expect(page.getByText("Welcome")).toBeVisible();
  });
});
```

#### 3.5 Struttura di un test E2E (API)

```python
import pytest
import httpx

class TestUserRegistrationFlow:
    """End-to-end test for the complete user registration flow via API."""

    BASE_URL = "http://localhost:8000/api/v1"

    @pytest.fixture(autouse=True)
    def setup_client(self):
        """Create an HTTP client for the test session."""
        self.client = httpx.Client(base_url=self.BASE_URL, timeout=10.0)
        yield
        self.client.close()

    def test_complete_registration_login_and_profile_access(self):
        """A new user can register, login, and access their profile."""
        # Step 1 — Register a new user
        registration_payload = {
            "email": "e2e-test@example.com",
            "password": "SecureP@ss123!",
            "name": "E2E Test User",
        }
        response = self.client.post("/auth/register", json=registration_payload)
        assert response.status_code == 201
        user_data = response.json()
        assert user_data["email"] == registration_payload["email"]
        user_id = user_data["id"]

        # Step 2 — Login with the new credentials
        login_payload = {
            "email": "e2e-test@example.com",
            "password": "SecureP@ss123!",
        }
        response = self.client.post("/auth/login", json=login_payload)
        assert response.status_code == 200
        token = response.json()["access_token"]
        assert token is not None

        # Step 3 — Access profile with the token
        headers = {"Authorization": f"Bearer {token}"}
        response = self.client.get(f"/users/{user_id}", headers=headers)
        assert response.status_code == 200
        profile = response.json()
        assert profile["email"] == "e2e-test@example.com"
        assert profile["name"] == "E2E Test User"

        # Step 4 — Cleanup: delete the test user
        response = self.client.delete(f"/users/{user_id}", headers=headers)
        assert response.status_code == 204
```

#### 3.6 Struttura di un test E2E (CLI)

```python
import subprocess

class TestCLIFlow:
    """End-to-end test for the CLI application."""

    CLI_COMMAND = ["python", "-m", "myapp"]

    def test_init_and_run_workflow(self):
        """The CLI can initialize a project and run a workflow."""
        # Step 1 — Initialize project
        result = subprocess.run(
            [*self.CLI_COMMAND, "init", "--name", "test-project"],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0
        assert "Project 'test-project' initialized" in result.stdout

        # Step 2 — Run workflow
        result = subprocess.run(
            [*self.CLI_COMMAND, "run", "--project", "test-project"],
            capture_output=True,
            text=True,
            timeout=60,
        )
        assert result.returncode == 0
        assert "Workflow completed successfully" in result.stdout

        # Step 3 — Verify output
        result = subprocess.run(
            [*self.CLI_COMMAND, "status", "--project", "test-project"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        assert result.returncode == 0
        assert "completed" in result.stdout.lower()
```

---

### 4. Livello 4 — Smoke Test Post-Deploy

Gli smoke test sono test **minimali e veloci** che verificano che il sistema sia vivo
e funzionante dopo un deploy. Non testano la logica di business, ma solo che i componenti
critici siano raggiungibili e operativi.

#### 4.1 Cosa verificano

| Controllo | Descrizione | Esempio |
|-----------|-------------|---------|
| **Healthcheck** | L'applicazione risponde | `GET /health` ritorna 200 |
| **Connessione database** | Il database e raggiungibile e risponde | Query `SELECT 1` o equivalente |
| **Servizi esterni** | Le dipendenze critiche sono raggiungibili | Ping verso API esterne, broker di messaggi, cache |
| **Configurazione** | Le variabili d'ambiente critiche sono presenti | Verifica che tutti i secret necessari siano configurati |
| **Funzionalita core** | La funzionalita principale funziona a livello base | Login funziona, homepage carica, API principale risponde |

#### 4.2 Implementazione

L'agent deve implementare gli smoke test come:

1. **Endpoint di healthcheck** nell'applicazione stessa (per servizi web):

```python
# Python (FastAPI)
from fastapi import APIRouter, status
from fastapi.responses import JSONResponse

router = APIRouter(tags=["health"])

@router.get("/health", status_code=status.HTTP_200_OK)
async def health_check():
    """Basic health check — the service is running."""
    return {"status": "healthy", "version": settings.APP_VERSION}

@router.get("/health/ready", status_code=status.HTTP_200_OK)
async def readiness_check(db: AsyncSession = Depends(get_db)):
    """Readiness check — the service and its dependencies are ready."""
    checks = {}

    # Database check
    try:
        await db.execute(text("SELECT 1"))
        checks["database"] = "ok"
    except Exception as e:
        checks["database"] = f"error: {str(e)}"
        return JSONResponse(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            content={"status": "unhealthy", "checks": checks},
        )

    # Cache check (if applicable)
    try:
        await redis_client.ping()
        checks["cache"] = "ok"
    except Exception as e:
        checks["cache"] = f"error: {str(e)}"

    all_healthy = all(v == "ok" for v in checks.values())
    return {
        "status": "healthy" if all_healthy else "degraded",
        "checks": checks,
        "version": settings.APP_VERSION,
    }
```

```csharp
// C# (ASP.NET Core) — using built-in health checks
builder.Services.AddHealthChecks()
    .AddNpgSql(connectionString, name: "database")
    .AddRedis(redisConnectionString, name: "cache")
    .AddUrlGroup(new Uri("https://external-api.example.com/health"), name: "external-api");

app.MapHealthChecks("/health", new HealthCheckOptions
{
    ResponseWriter = UIResponseWriter.WriteHealthCheckUIResponse
});

app.MapHealthChecks("/health/ready", new HealthCheckOptions
{
    Predicate = _ => true,
    ResponseWriter = UIResponseWriter.WriteHealthCheckUIResponse
});
```

2. **Script di smoke test** eseguibile post-deploy:

```bash
#!/usr/bin/env bash
# =============================================================================
# File: scripts/smoke-test.sh
# Purpose: Post-deploy smoke test — verifies the system is alive and functional
# Author: Agent (D.O.E. Framework)
# =============================================================================

set -euo pipefail

BASE_URL="${1:?Usage: smoke-test.sh <base-url>}"
TIMEOUT=10
EXIT_CODE=0

echo "=== Smoke Test Suite ==="
echo "Target: ${BASE_URL}"
echo "========================"

# Test 1: Health endpoint
echo -n "[1/4] Health check... "
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT}" "${BASE_URL}/health")
if [ "${HTTP_CODE}" = "200" ]; then
    echo "PASS (${HTTP_CODE})"
else
    echo "FAIL (${HTTP_CODE})"
    EXIT_CODE=1
fi

# Test 2: Readiness endpoint
echo -n "[2/4] Readiness check... "
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT}" "${BASE_URL}/health/ready")
if [ "${HTTP_CODE}" = "200" ]; then
    echo "PASS (${HTTP_CODE})"
else
    echo "FAIL (${HTTP_CODE})"
    EXIT_CODE=1
fi

# Test 3: Core API responds
echo -n "[3/4] Core API responds... "
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT}" "${BASE_URL}/api/v1/status")
if [ "${HTTP_CODE}" = "200" ]; then
    echo "PASS (${HTTP_CODE})"
else
    echo "FAIL (${HTTP_CODE})"
    EXIT_CODE=1
fi

# Test 4: Static assets (if applicable)
echo -n "[4/4] Static assets... "
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT}" "${BASE_URL}/")
if [ "${HTTP_CODE}" = "200" ] || [ "${HTTP_CODE}" = "304" ]; then
    echo "PASS (${HTTP_CODE})"
else
    echo "FAIL (${HTTP_CODE})"
    EXIT_CODE=1
fi

echo "========================"
if [ "${EXIT_CODE}" = "0" ]; then
    echo "Result: ALL PASSED"
else
    echo "Result: SOME FAILED — check above"
fi

exit "${EXIT_CODE}"
```

#### 4.3 Integrazione con CI/CD

Gli smoke test devono essere eseguiti automaticamente dopo ogni deploy:

```yaml
# GitHub Actions — esempio di step post-deploy
- name: Run smoke tests
  run: |
    # Wait for the service to be ready (max 60 seconds)
    for i in $(seq 1 12); do
      if curl -s -o /dev/null -w "%{http_code}" "${{ env.DEPLOY_URL }}/health" | grep -q "200"; then
        echo "Service is ready"
        break
      fi
      echo "Waiting for service to be ready... (attempt ${i}/12)"
      sleep 5
    done
    # Run smoke tests
    bash scripts/smoke-test.sh "${{ env.DEPLOY_URL }}"
```

---

### 5. Nodo Deterministico di Verifica (Pattern Chiave)

Questo e il protocollo obbligatorio che l'agent esegue dopo ogni fase di scrittura codice.
E il meccanismo che garantisce che il codice prodotto sia sempre verificato prima di
essere considerato completo.

#### 5.1 Sequenza obbligatoria

```
CODICE SCRITTO
    │
    ├── Step 1: LINTING
    │   ├── Esegui il linter configurato per il linguaggio
    │   ├── Errore? → Correggi → Ri-esegui
    │   └── Passato? → Procedi
    │
    ├── Step 2: TYPE CHECKING (se applicabile)
    │   ├── Esegui il type checker (mypy, tsc, dotnet build)
    │   ├── Errore? → Correggi → Ri-esegui
    │   └── Passato? → Procedi
    │
    ├── Step 3: UNIT TEST
    │   ├── Esegui tutti gli unit test
    │   ├── Fallimento? → Analizza: bug nel codice o nel test?
    │   │   ├── Bug nel codice → Correggi il codice → Ri-esegui dal Step 1
    │   │   └── Bug nel test → Correggi il test → Ri-esegui dal Step 3
    │   └── Passato? → Procedi
    │
    ├── Step 4: INTEGRATION TEST (se applicabile)
    │   ├── Esegui i test di integrazione rilevanti
    │   ├── Fallimento? → Analizza la causa
    │   │   ├── Bug nel codice → Correggi → Ri-esegui dal Step 1
    │   │   ├── Bug nel test → Correggi il test → Ri-esegui dal Step 4
    │   │   └── Problema di configurazione → Correggi config → Ri-esegui dal Step 4
    │   └── Passato? → Procedi
    │
    └── Step 5: COVERAGE CHECK
        ├── Verifica che la copertura sia >= soglia configurata
        ├── Sotto soglia? → Scrivi test aggiuntivi → Ri-esegui dal Step 3
        └── Sopra soglia? → CODICE VERIFICATO ✓
```

#### 5.2 Comandi di verifica per linguaggio

| Linguaggio | Linting | Type Check | Unit Test | Coverage |
|------------|---------|------------|-----------|----------|
| Python | `ruff check .` | `mypy .` | `pytest tests/unit/` | `pytest --cov=src tests/unit/ --cov-fail-under=80` |
| TypeScript | `eslint .` | `tsc --noEmit` | `vitest run tests/unit/` | `vitest run --coverage` |
| C# | `dotnet format --verify-no-changes` | `dotnet build` (implicito) | `dotnet test --filter Category=Unit` | `dotnet test --collect:"XPlat Code Coverage"` |
| Go | `golangci-lint run` | (implicito nella compilazione) | `go test ./...` | `go test -coverprofile=cover.out ./...` |
| Dart | `dart analyze` | (implicito nell'analyzer) | `dart test test/unit/` | `dart test --coverage=coverage` |

#### 5.3 Regola anti-loop

Se lo stesso errore si presenta **3 volte consecutive** dopo tentativi di correzione:

1. L'agent si ferma
2. Documenta il problema: cosa ha provato, cosa non ha funzionato
3. Chiede aiuto all'utente (Interaction Protocol, livello Error Non-Recoverable)
4. Registra il problema in `project-state.md` nella sezione "Vincoli Scoperti"

Questo previene cicli infiniti di correzione e segnala tempestivamente problemi
che richiedono intervento umano o un cambio di approccio.

---

### 6. Test Data Management

#### 6.1 Principi

- **I dati di test sono codice.** Devono essere versionati, reviewati e manutenuti
  come il codice produttivo.
- **I dati di test sono isolati.** Ogni test crea i propri dati e li pulisce.
  Non dipendere da dati pre-esistenti nel database o nel file system.
- **I dati di test sono realistici ma sicuri.** Usare dati che assomiglino a quelli
  reali (struttura, formato, dimensioni) ma che non contengano dati sensibili reali.
  Mai usare dati di produzione nei test.

#### 6.2 Pattern raccomandati

| Pattern | Quando usarlo | Esempio |
|---------|---------------|---------|
| **Factory** | Creare oggetti di test con valori default ragionevoli | `UserFactory.create(email="test@example.com")` |
| **Fixture** | Dati condivisi tra piu test nella stessa suite | Fixture di pytest, setup di xUnit |
| **Builder** | Oggetti complessi con molte varianti | `OrderBuilder().withItems(3).withDiscount(10).build()` |
| **Seed** | Database pre-popolato per test di integrazione/E2E | Script SQL o migrazione dedicata per test |

Librerie raccomandate per factory/fixture:

| Linguaggio | Libreria | Note |
|------------|----------|------|
| Python | `factory_boy` | Factory per modelli ORM (SQLAlchemy, Django) |
| TypeScript | `fishery` | Factory leggera con TypeScript support |
| C# | `Bogus` | Generazione dati fake realistici |
| Go | `go-faker` o costruzione manuale | Go preferisce costruzione esplicita |

---

### 7. Configurazione del Progetto per il Testing

#### 7.1 File di configurazione

L'agent deve creare i file di configurazione necessari per il testing come parte
del setup del progetto. Questi file devono essere committati nel repository.

**Python (`pyproject.toml` — sezione pytest):**
```toml
[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
addopts = [
    "-v",
    "--strict-markers",
    "--tb=short",
]
markers = [
    "unit: Unit tests (fast, no external dependencies)",
    "integration: Integration tests (may require external services)",
    "e2e: End-to-end tests (full stack, slow)",
    "smoke: Smoke tests (post-deploy verification)",
]

[tool.coverage.run]
source = ["src"]
omit = ["tests/*", "*/migrations/*"]

[tool.coverage.report]
fail_under = 80
show_missing = true
exclude_lines = [
    "pragma: no cover",
    "if __name__ == .__main__.:",
    "if TYPE_CHECKING:",
]
```

**TypeScript (`vitest.config.ts`):**
```typescript
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["src/**/*.ts"],
      exclude: ["src/**/*.d.ts", "src/**/index.ts"],
      thresholds: {
        lines: 80,
        branches: 75,
        functions: 80,
        statements: 80,
      },
    },
    testTimeout: 10000,
    hookTimeout: 30000,
  },
});
```

**C# (`.runsettings` — configurazione coverage):**
```xml
<?xml version="1.0" encoding="utf-8"?>
<RunSettings>
  <DataCollectionRunSettings>
    <DataCollectors>
      <DataCollector friendlyName="XPlat Code Coverage">
        <Configuration>
          <Format>opencover,cobertura</Format>
          <Exclude>[*]*.Migrations.*,[*]*.Tests.*</Exclude>
          <ExcludeByAttribute>GeneratedCodeAttribute,ExcludeFromCodeCoverageAttribute</ExcludeByAttribute>
        </Configuration>
      </DataCollector>
    </DataCollectors>
  </DataCollectionRunSettings>
</RunSettings>
```

#### 7.2 Script di esecuzione test

L'agent deve configurare script/comandi nel package manager per semplificare
l'esecuzione dei test:

**Python (`Makefile` o `pyproject.toml` scripts):**
```makefile
.PHONY: test test-unit test-integration test-e2e test-coverage lint typecheck verify

test:              ## Run all tests
	pytest

test-unit:         ## Run unit tests only
	pytest tests/unit/ -m unit

test-integration:  ## Run integration tests only
	pytest tests/integration/ -m integration

test-e2e:          ## Run end-to-end tests only
	pytest tests/e2e/ -m e2e

test-coverage:     ## Run tests with coverage report
	pytest --cov=src --cov-report=html --cov-report=term --cov-fail-under=80

lint:              ## Run linter
	ruff check .

typecheck:         ## Run type checker
	mypy .

verify:            ## Full verification pipeline (lint + typecheck + test + coverage)
	$(MAKE) lint
	$(MAKE) typecheck
	$(MAKE) test-coverage
```

**TypeScript (`package.json` scripts):**
```json
{
  "scripts": {
    "test": "vitest run",
    "test:unit": "vitest run tests/unit/",
    "test:integration": "vitest run tests/integration/",
    "test:e2e": "playwright test",
    "test:coverage": "vitest run --coverage",
    "test:watch": "vitest watch",
    "lint": "eslint .",
    "typecheck": "tsc --noEmit",
    "verify": "npm run lint && npm run typecheck && npm run test:coverage"
  }
}
```

---

### 8. Regole Operative

Le seguenti regole sono vincolanti per l'agent durante tutto il ciclo di sviluppo:

1. **L'agent DEVE eseguire il Nodo Deterministico di Verifica (sezione 5) dopo ogni
   implementazione significativa.** Una implementazione significativa e: un nuovo modulo,
   una nuova feature, un bugfix, un refactoring. Non e necessario dopo modifiche
   cosmetiche (commenti, formattazione).

2. **I test devono passare PRIMA di dichiarare un task completato.** Un task con test
   che falliscono non e completato, indipendentemente dallo stato del codice produttivo.

3. **Se un test fallisce, l'agent lo corregge (codice o test) prima di procedere.**
   L'agent analizza se il fallimento indica un bug nel codice produttivo o un errore
   nel test. In entrambi i casi, la correzione e prioritaria rispetto a qualsiasi
   altro lavoro.

4. **I test sono codice di prima classe.** Devono rispettare gli stessi standard
   di qualita del codice produttivo (DIR-L3-001): naming, struttura, commenti,
   error handling. L'unica eccezione e che i test possono avere funzioni piu lunghe
   del soft limit se il flusso di test lo richiede (es. test E2E con molti step).

5. **I test sono task espliciti nel piano di decomposizione.** "Scrivere unit test
   per X" e un task separato nel piano (vedi Task Decomposition, Regola 4), non
   un'attivita implicita di un altro task.

6. **La copertura e un indicatore, non un obiettivo.** L'agent punta al 80% come
   baseline ma privilegia la qualita dei test (scenari significativi, casi limite)
   rispetto alla copertura numerica. Un progetto con 70% di copertura e test
   significativi e migliore di uno con 95% di copertura e test triviali.

7. **I test E2E coprono i flussi critici, non tutto.** L'agent identifica i 3-5 flussi
   utente piu critici e li copre con test E2E. La copertura capillare e delegata
   agli unit test e ai test di integrazione.

8. **Ogni bug risolto genera un test di regressione.** Quando l'agent corregge un bug,
   scrive un test che riproduce il bug e verifica la correzione. Questo previene
   la regressione.

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| Test unitari | File di test nel linguaggio del progetto | `tests/unit/` |
| Test di integrazione | File di test nel linguaggio del progetto | `tests/integration/` |
| Test end-to-end | File di test nel linguaggio del progetto | `tests/e2e/` |
| Script smoke test | Bash script o equivalente | `scripts/smoke-test.sh` |
| Configurazione test | File di configurazione del framework | Root del progetto |
| Configurazione coverage | File di configurazione | Root del progetto |
| Script di esecuzione | Makefile o package.json scripts | Root del progetto |
| Report di copertura | HTML e/o testo | `coverage/` (gitignored) |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| Framework di test non installato | Dipendenza mancante | Installare e aggiungere a `docs/tech-specs.md` |
| Test falliscono su CI ma passano in locale | Dipendenza ambientale (OS, timezone, path) | Verificare isolamento; usare container per CI; aggiungere seed per random |
| Coverage sotto soglia dopo implementazione | Test insufficienti o troppo superficiali | Aggiungere test per i casi mancanti (edge cases, error paths) |
| Test E2E flaky (fallimento intermittente) | Race condition, timing, dipendenza esterna | Aggiungere wait espliciti; isolare dati; implementare retry con limite |
| Test di integrazione lenti | Dipendenze esterne reali (DB reale, API esterna) | Usare testcontainers, mock, o database in-memory per test piu veloci |
| Conflitto tra test (test che si influenzano) | Stato condiviso non pulito tra test | Verificare isolamento: fixture con rollback, cleanup esplicito, database separati |
| Timeout nei test | Operazione bloccante o attesa infinita | Configurare timeout espliciti; verificare deadlock; aggiungere cancellation |
| Linter e test in conflitto | Configurazione linter troppo aggressiva per file di test | Configurare override del linter per la directory `tests/` (es. allow longer functions) |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
