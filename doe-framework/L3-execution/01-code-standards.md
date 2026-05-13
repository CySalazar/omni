# Code Standards — Standard Qualitativi del Codice

## Metadata

- **ID:** DIR-010
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** nessuna (documento fondativo del Livello 3)
- **Tipo di progetto:** universale

---

## Obiettivo

Definire standard di qualita misurabili e verificabili per tutto il codice prodotto dall'agent,
garantendo leggibilita, manutenibilita, sicurezza e affidabilita indipendentemente dal linguaggio
o dal framework utilizzato.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio e il framework sono stati selezionati e documentati.
- L'agent ha consultato `docs/tech-specs.md` per conoscere le versioni e i vincoli del progetto.

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Linguaggio scelto | stringa | Il linguaggio di programmazione del progetto |
| Framework scelto | stringa | Il framework principale (se presente) |
| `docs/tech-specs.md` | file Markdown | Specifiche tecniche con versioni e vincoli di compatibilita |
| Convenzioni del team | testo (opzionale) | Eventuali convenzioni gia in uso dall'utente |

---

## Procedura

### 1. Naming Conventions

Le convenzioni di naming sono universali nei principi ma si adattano al linguaggio scelto.
L'agent deve seguire le convenzioni idiomatiche del linguaggio.

#### 1.1 Principi universali

- I nomi devono essere **descrittivi e non ambigui**. Evitare abbreviazioni a meno che
  non siano universalmente riconosciute nel dominio (es. `id`, `url`, `http`, `db`).
- La lunghezza del nome deve essere proporzionale allo scope: variabili di loop possono
  essere brevi (`i`, `j`), variabili di modulo devono essere esplicite.
- Non usare nomi generici come `data`, `info`, `temp`, `result` senza qualificazione
  (preferire `userData`, `connectionInfo`, `tempFilePath`, `validationResult`).
- I boolean devono esprimere una condizione: `isActive`, `hasPermission`, `canDelete`,
  `shouldRetry`.

#### 1.2 Convenzioni per linguaggio

| Elemento | Python | JavaScript/TypeScript | C# | Go | Dart |
|----------|--------|----------------------|-----|-----|------|
| File | `snake_case.py` | `kebab-case.ts` | `PascalCase.cs` | `snake_case.go` | `snake_case.dart` |
| Classe | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` |
| Funzione/Metodo | `snake_case` | `camelCase` | `PascalCase` | `PascalCase` (exported), `camelCase` (unexported) | `camelCase` |
| Variabile | `snake_case` | `camelCase` | `camelCase` | `camelCase` | `camelCase` |
| Costante | `UPPER_SNAKE_CASE` | `UPPER_SNAKE_CASE` | `PascalCase` | `PascalCase` (exported), `camelCase` (unexported) | `lowerCamelCase` o `UPPER_SNAKE_CASE` |
| Interfaccia | `PascalCase` (no prefix) | `PascalCase` (no `I` prefix) | `IPascalCase` | `PascalCase` | — (mixin/abstract) |
| Enum | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` |
| Pacchetto/Modulo | `snake_case` | `kebab-case` | `PascalCase` (namespace) | `lowercase` | `snake_case` |

#### 1.3 Standard di stile per linguaggio

L'agent DEVE configurare e rispettare i linter e i formatter standard del linguaggio:

| Linguaggio | Linter | Formatter | Style Guide di riferimento |
|------------|--------|-----------|---------------------------|
| Python | `ruff` (o `flake8` + `pylint`) | `ruff format` (o `black`) | PEP 8 |
| JavaScript/TypeScript | `eslint` | `prettier` | Airbnb o Standard |
| C# | `dotnet format` (Roslyn analyzers) | `dotnet format` | .NET Coding Conventions |
| Go | `golangci-lint` | `gofmt` (obbligatorio) | Effective Go |
| Dart | `dart analyze` | `dart format` | Effective Dart |

**Regola:** Il linter e il formatter devono essere configurati nel progetto (file di configurazione
committato) e devono essere eseguiti PRIMA di ogni commit. L'agent non deve mai disabilitare
regole del linter senza documentare il motivo in un commento inline.

---

### 2. Struttura del Codice

#### 2.1 Header di file

Ogni file sorgente deve iniziare con un header comment che fornisce contesto immediato:

```
// =============================================================================
// File: <nome-file>
// Purpose: <scopo del file in una riga>
// Author: Agent (D.O.E. Framework)
// Created: <YYYY-MM-DD>
// Dependencies: <dipendenze principali, se rilevanti>
// =============================================================================
```

Per linguaggi con docstring (Python), usare il formato nativo:

```python
"""
<nome-modulo> — <scopo del modulo in una riga>

Author: Agent (D.O.E. Framework)
Created: <YYYY-MM-DD>
Dependencies: <dipendenze principali>
"""
```

**Nota:** L'header e obbligatorio per file che contengono logica. File di configurazione,
file generati automaticamente e file triviali (es. `__init__.py` vuoti) sono esenti.

#### 2.2 Limiti di dimensione (soft limits)

| Elemento | Limite raccomandato | Azione se superato |
|----------|--------------------|--------------------|
| Funzione/metodo | 50 righe | Valutare scomposizione in sotto-funzioni |
| Classe | 300 righe | Valutare separazione responsabilita (SRP) |
| File | 500 righe | Valutare suddivisione in moduli |
| Parametri di funzione | 5 parametri | Raggruppare in oggetto/dataclass/struct |
| Livelli di indentazione | 4 livelli | Refactoring: early return, extract method |

Questi sono soft limits: possono essere superati se c'e una ragione valida, ma l'agent
deve documentare il motivo con un commento se il superamento e significativo (>50%).

#### 2.3 Principi di struttura

- **Single Responsibility Principle (SRP):** Ogni funzione fa una cosa. Ogni classe ha
  una responsabilita. Ogni modulo ha un tema coerente.
- **DRY (Don't Repeat Yourself):** Il codice duplicato deve essere estratto in funzioni
  condivise. Se lo stesso blocco appare 3+ volte, e un candidato per l'estrazione.
- **KISS (Keep It Simple, Stupid):** Preferire soluzioni semplici e leggibili a soluzioni
  "intelligenti" ma opache. Il codice viene letto molte piu volte di quante viene scritto.
- **Fail Fast:** Validare gli input all'inizio della funzione e uscire subito in caso di
  errore, evitando nidificazione profonda.
- **Separation of Concerns:** Separare logica di business, accesso ai dati, presentazione
  e infrastruttura in layer distinti.

---

### 3. Commenti e Documentazione Inline

#### 3.1 Filosofia dei commenti

I commenti spiegano il **perche**, non il **cosa**. Il codice deve essere auto-esplicativo
per il "cosa" (grazie a naming descrittivo e struttura chiara). I commenti esistono per
catturare contesto che il codice da solo non puo esprimere.

#### 3.2 Quando commentare

| Situazione | Azione |
|------------|--------|
| Logica di business non ovvia | Commentare il ragionamento |
| Workaround per bug noto | Commentare con link al bug/issue |
| Scelta non intuitiva tra alternative | Commentare perche questa e stata preferita |
| Regex o query complesse | Commentare cosa matchano/cercano |
| Costanti "magiche" | Commentare l'origine del valore |
| Performance optimization | Commentare cosa e perche si ottimizza |

#### 3.3 Quando NON commentare

- Codice ovvio: `i += 1  // incrementa i` — NO
- Codice che dovrebbe essere riscritto piu chiaramente invece che commentato
- Codice commentato (dead code) — eliminarlo, non commentarlo. Git lo conserva

#### 3.4 Marker standardizzati

L'agent usa marker standard per segnalare lavoro incompleto o problematico:

| Marker | Significato | Formato |
|--------|-------------|---------|
| `TODO` | Lavoro da completare | `// TODO(scope): descrizione — ref: #issue` |
| `FIXME` | Bug noto da correggere | `// FIXME(scope): descrizione — ref: #issue` |
| `HACK` | Workaround temporaneo | `// HACK: motivo — rimuovere quando: condizione` |
| `NOTE` | Informazione contestuale importante | `// NOTE: spiegazione` |
| `PERF` | Opportunita di ottimizzazione | `// PERF: descrizione miglioramento possibile` |
| `SECURITY` | Punto critico per la sicurezza | `// SECURITY: cosa e stato considerato e perche` |

**Regola:** Ogni `TODO` e `FIXME` deve avere un contesto sufficiente per essere azionabile
anche da chi non ha scritto il codice. Se esiste un issue tracker, includere il riferimento.

#### 3.5 Documentazione di funzioni e metodi pubblici

Ogni funzione/metodo pubblico deve avere documentazione con:
- Descrizione breve dello scopo
- Parametri con tipo e descrizione
- Valore di ritorno con tipo e descrizione
- Eccezioni/errori possibili
- Esempio d'uso (per API pubbliche o funzioni complesse)

Formato per linguaggio:

**Python (docstring Google style):**
```python
def calculate_risk_score(transactions: list[Transaction], threshold: float = 0.8) -> RiskResult:
    """Calculate the aggregate risk score for a set of transactions.

    Analyzes each transaction against fraud detection heuristics and
    produces a weighted risk score. Scores above threshold trigger alerts.

    Args:
        transactions: List of Transaction objects to analyze.
            Must contain at least one transaction.
        threshold: Risk threshold for alert triggering (0.0-1.0).
            Defaults to 0.8.

    Returns:
        RiskResult containing the aggregate score, individual scores,
        and a list of flagged transactions.

    Raises:
        ValueError: If transactions list is empty.
        ConnectionError: If fraud detection service is unreachable.

    Example:
        >>> txns = [Transaction(amount=100, merchant="shop_a")]
        >>> result = calculate_risk_score(txns)
        >>> print(result.score)
        0.23
    """
```

**TypeScript (JSDoc):**
```typescript
/**
 * Calculate the aggregate risk score for a set of transactions.
 *
 * Analyzes each transaction against fraud detection heuristics and
 * produces a weighted risk score. Scores above threshold trigger alerts.
 *
 * @param transactions - List of Transaction objects to analyze. Must contain at least one.
 * @param threshold - Risk threshold for alert triggering (0.0-1.0). Defaults to 0.8.
 * @returns RiskResult containing aggregate score and flagged transactions.
 * @throws {ValidationError} If transactions array is empty.
 * @throws {ConnectionError} If fraud detection service is unreachable.
 *
 * @example
 * const txns = [new Transaction({ amount: 100, merchant: "shop_a" })];
 * const result = calculateRiskScore(txns);
 * console.log(result.score); // 0.23
 */
```

**C# (XML Documentation):**
```csharp
/// <summary>
/// Calculate the aggregate risk score for a set of transactions.
/// </summary>
/// <remarks>
/// Analyzes each transaction against fraud detection heuristics and
/// produces a weighted risk score. Scores above threshold trigger alerts.
/// </remarks>
/// <param name="transactions">List of Transaction objects to analyze. Must contain at least one.</param>
/// <param name="threshold">Risk threshold for alert triggering (0.0-1.0). Defaults to 0.8.</param>
/// <returns>RiskResult containing aggregate score and flagged transactions.</returns>
/// <exception cref="ArgumentException">Thrown when transactions list is empty.</exception>
/// <exception cref="HttpRequestException">Thrown when fraud detection service is unreachable.</exception>
/// <example>
/// <code>
/// var txns = new List&lt;Transaction&gt; { new Transaction(100, "shop_a") };
/// var result = CalculateRiskScore(txns);
/// Console.WriteLine(result.Score); // 0.23
/// </code>
/// </example>
```

---

### 4. Logging Strutturato

#### 4.1 Principi

- Ogni applicazione (non libreria) deve implementare logging strutturato.
- I livelli di log seguono la severita standard: `DEBUG`, `INFO`, `WARNING`, `ERROR`, `CRITICAL`.
- Il formato dei log deve essere consistente e parsabile (preferire JSON in produzione,
  testo leggibile in sviluppo).

#### 4.2 Cosa loggare per livello

| Livello | Quando usarlo | Esempio |
|---------|--------------|---------|
| `DEBUG` | Dettagli utili per debugging. Disabilitato in produzione | Payload di una richiesta, stato intermedio di un calcolo |
| `INFO` | Operazioni normali significative | Avvio servizio, richiesta completata, job schedulato eseguito |
| `WARNING` | Situazione anomala ma gestita | Retry di una connessione, fallback attivato, deprecation usata |
| `ERROR` | Errore che impatta una singola operazione | Chiamata API fallita, validazione fallita, record non trovato |
| `CRITICAL` | Errore che compromette il sistema | Database irraggiungibile, configurazione mancante, out of memory |

#### 4.3 Struttura del messaggio di log

Ogni messaggio di log deve includere:

```json
{
  "timestamp": "2026-03-24T10:30:00.000Z",
  "level": "ERROR",
  "service": "payment-service",
  "module": "checkout.processor",
  "message": "Payment processing failed",
  "context": {
    "order_id": "ORD-12345",
    "payment_method": "credit_card",
    "error_code": "GATEWAY_TIMEOUT"
  },
  "trace_id": "abc-123-def"
}
```

Campi obbligatori: `timestamp`, `level`, `message`.
Campi raccomandati: `service`, `module`, `context`, `trace_id`.

#### 4.4 Cosa NON loggare MAI

- **Credenziali:** password, token API, chiavi segrete, certificati
- **PII (Personally Identifiable Information):** email, numeri di telefono, indirizzi,
  codici fiscali, numeri di carte di credito
- **Dati sensibili del business:** pricing interno, dati finanziari riservati
- **Dati di sessione completi:** solo session ID (troncato), mai il contenuto

**Regola di mascheramento:** Se un dato sensibile deve apparire nel log per debugging,
mascherarlo (es. `card: ****1234`, `email: m***@example.com`).

#### 4.5 Librerie raccomandate

| Linguaggio | Libreria | Note |
|------------|----------|------|
| Python | `structlog` o `logging` (stdlib) | `structlog` preferito per output JSON nativo |
| JavaScript/TypeScript | `pino` o `winston` | `pino` preferito per performance |
| C# | `Serilog` o `Microsoft.Extensions.Logging` | `Serilog` preferito per structured logging |
| Go | `slog` (stdlib 1.21+) o `zerolog` | `slog` preferito per progetti nuovi |
| Dart | `logging` (package) | Unica opzione mainstream |

---

### 5. Gestione degli Errori

#### 5.1 Principi fondamentali

- **Ogni operazione esterna ha error handling esplicito.** Operazioni esterne includono:
  I/O su file, chiamate di rete, query al database, parsing di dati esterni, accesso
  a risorse di sistema.
- **Catch specifico, mai generico.** Catturare l'eccezione specifica che ci si aspetta,
  non un generico `Exception` o `Error`. Se serve un catch-all, deve essere al livello
  piu alto (entry point) e deve loggare il dettaglio.
- **Mai "catch and swallow".** Ogni blocco catch deve fare almeno una di queste cose:
  loggare l'errore, propagarlo, trasformarlo in un errore del dominio, o eseguire un'azione
  di recovery.
- **Errori custom per il dominio.** Quando il contesto lo richiede, definire eccezioni/errori
  custom che portino informazioni rilevanti per il dominio.

#### 5.2 Pattern per linguaggio

**Python:**
```python
# CORRETTO: catch specifico con contesto
try:
    response = await http_client.post(url, json=payload)
    response.raise_for_status()
except httpx.TimeoutException as e:
    logger.error("API timeout", url=url, timeout=timeout, error=str(e))
    raise PaymentGatewayError(f"Gateway timeout for {url}") from e
except httpx.HTTPStatusError as e:
    logger.error("API error", url=url, status=e.response.status_code)
    raise PaymentGatewayError(f"Gateway returned {e.response.status_code}") from e

# SBAGLIATO: catch generico che inghiotte l'errore
try:
    response = await http_client.post(url, json=payload)
except Exception:
    pass  # MAI FARE QUESTO
```

**TypeScript:**
```typescript
// CORRETTO: gestione esplicita con tipo di errore
try {
  const response = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!response.ok) {
    throw new PaymentGatewayError(
      `Gateway returned ${response.status}`,
      { statusCode: response.status, url }
    );
  }
  return await response.json();
} catch (error) {
  if (error instanceof PaymentGatewayError) {
    throw error; // gia gestito, propaga
  }
  if (error instanceof DOMException && error.name === "AbortError") {
    logger.error("API timeout", { url, timeout: 5000 });
    throw new PaymentGatewayError(`Gateway timeout for ${url}`, { cause: error });
  }
  // Errore imprevisto — log completo e propaga
  logger.error("Unexpected error during payment", { url, error });
  throw new PaymentGatewayError("Unexpected payment error", { cause: error });
}
```

**C#:**
```csharp
// CORRETTO: catch specifico con logging strutturato
try
{
    var response = await _httpClient.PostAsJsonAsync(url, payload, cancellationToken);
    response.EnsureSuccessStatusCode();
    return await response.Content.ReadFromJsonAsync<PaymentResult>(cancellationToken);
}
catch (TaskCanceledException ex) when (ex.InnerException is TimeoutException)
{
    _logger.LogError(ex, "Payment gateway timeout for {Url}", url);
    throw new PaymentGatewayException($"Gateway timeout for {url}", ex);
}
catch (HttpRequestException ex)
{
    _logger.LogError(ex, "Payment gateway HTTP error for {Url}: {StatusCode}", url, ex.StatusCode);
    throw new PaymentGatewayException($"Gateway returned {ex.StatusCode}", ex);
}
```

#### 5.3 Errori custom — struttura raccomandata

Un errore custom dovrebbe portare:
- Un messaggio leggibile per l'umano
- Un codice di errore macchina-leggibile (opzionale ma raccomandato per API)
- Il contesto rilevante (senza dati sensibili)
- La causa originale (inner exception / cause)

```python
class AppError(Exception):
    """Base error for the application domain."""

    def __init__(self, message: str, code: str = "UNKNOWN", context: dict | None = None):
        super().__init__(message)
        self.code = code
        self.context = context or {}


class PaymentGatewayError(AppError):
    """Error communicating with the payment gateway."""

    def __init__(self, message: str, context: dict | None = None):
        super().__init__(message, code="PAYMENT_GATEWAY_ERROR", context=context)
```

---

### 6. Validazione Input/Output

#### 6.1 Principio

Ogni funzione o endpoint che riceve dati dall'esterno (input utente, payload API,
file caricati, variabili d'ambiente, risultati di query) deve validarli prima dell'uso.

La validazione segue una catena: **tipo → formato → range → coerenza logica**.

#### 6.2 Catena di validazione

```
INPUT RICEVUTO
    │
    ├── 1. Tipo: E del tipo atteso? (stringa, numero, array, oggetto)
    │       └── NO → Errore 400/ValidationError
    │
    ├── 2. Formato: Rispetta il formato atteso? (email, UUID, ISO date, URL)
    │       └── NO → Errore 400/ValidationError con dettaglio
    │
    ├── 3. Range: E dentro i limiti accettabili? (min/max, lunghezza, dimensione)
    │       └── NO → Errore 400/ValidationError con limiti
    │
    └── 4. Coerenza logica: Ha senso nel contesto? (data_fine > data_inizio, quantita > 0)
            └── NO → Errore 422/BusinessRuleError
```

#### 6.3 Strumenti di validazione raccomandati

| Linguaggio | Libreria | Uso |
|------------|----------|-----|
| Python | `pydantic` (v2) | Validazione modelli, API input, configurazione |
| TypeScript | `zod` | Schema validation con type inference |
| C# | `FluentValidation` o Data Annotations | Validazione modelli e DTO |
| Go | `go-playground/validator` | Struct tag validation |
| Dart | Built-in `assert` + package `formz` | Validazione form e modelli |

#### 6.4 Validazione dell'output

Prima di restituire un risultato all'utente o a un altro sistema:
- Verificare che l'output rispetti il contratto (schema, tipo, formato).
- Verificare che non contenga dati sensibili non previsti.
- Per API: validare la risposta contro lo schema OpenAPI/JSON Schema.

---

### 7. Organizzazione dei File e delle Directory

#### 7.1 Principio

La struttura delle directory deve riflettere l'architettura del progetto e rendere
immediatamente chiaro dove trovare ogni tipo di codice.

#### 7.2 Separazione per responsabilita

Indipendentemente dal linguaggio, il codice deve essere organizzato separando:

| Layer | Contenuto | Esempio di directory |
|-------|-----------|---------------------|
| **Entry point** | Bootstrap, configurazione, composizione | `src/main.py`, `src/index.ts`, `Program.cs` |
| **API/Interface** | Controller, route, handler, CLI | `src/api/`, `src/handlers/`, `src/controllers/` |
| **Business logic** | Regole di dominio, servizi, casi d'uso | `src/services/`, `src/domain/`, `src/usecases/` |
| **Data access** | Repository, ORM, query | `src/repositories/`, `src/data/`, `src/db/` |
| **Infrastructure** | Client esterni, messaggistica, cache | `src/infrastructure/`, `src/clients/` |
| **Shared/Common** | Utility, tipi condivisi, costanti | `src/common/`, `src/shared/`, `src/utils/` |
| **Configuration** | Config loader, schema config | `src/config/` |
| **Tests** | Test unitari, integrazione, E2E | `tests/unit/`, `tests/integration/`, `tests/e2e/` |

#### 7.3 Regole di organizzazione

- **Un file, una responsabilita principale.** Evitare file "contenitore" che raccolgono
  classi o funzioni non correlate.
- **Profondita massima consigliata: 4 livelli** dalla root del progetto. Oltre, la
  navigazione diventa difficile.
- **File di configurazione nella root.** Linter config, formatter config, CI config,
  package manager config — tutti nella root del progetto.
- **File README.md nella root.** Sempre presente, sempre aggiornato.

---

### 8. Checklist di Qualita Pre-Commit

Prima di considerare un blocco di codice completato, l'agent verifica:

```
CHECKLIST PRE-COMMIT
    │
    ├── [ ] Il codice compila/esegue senza errori
    ├── [ ] Il linter non produce errori o warning non giustificati
    ├── [ ] Il formatter e stato applicato
    ├── [ ] I nomi sono descrittivi e seguono le convenzioni del linguaggio
    ├── [ ] Le funzioni/metodi pubblici hanno documentazione
    ├── [ ] Commenti spiegano il "perche" dove necessario
    ├── [ ] Logging implementato per operazioni significative
    ├── [ ] Error handling esplicito per ogni operazione esterna
    ├── [ ] Input validati prima dell'uso
    ├── [ ] Nessun dato sensibile nel codice o nei commenti
    ├── [ ] Nessun codice commentato (dead code)
    ├── [ ] I test sono scritti e passano
    ├── [ ] I soft limits di dimensione sono rispettati (o documentata l'eccezione)
    └── [ ] Il file header comment e presente
```

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| Codice sorgente conforme | File sorgenti nel linguaggio del progetto | `src/` (o struttura equivalente) |
| Configurazione linter/formatter | File di configurazione | Root del progetto |
| Documentazione inline | Commenti e docstring | Nei file sorgente |
| Report di linting (se richiesto) | Output del linter | Console / CI log |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| Il linter non e installato | Ambiente non configurato | Installare il linter e aggiungerlo a `docs/tech-specs.md` |
| Conflitto tra linter e formatter | Configurazioni incompatibili | Verificare che linter e formatter siano configurati per cooperare (es. eslint-config-prettier) |
| Soft limit superato significativamente | Funzione/classe troppo complessa | Applicare refactoring: extract method, extract class, strategy pattern |
| Naming inconsistente nel progetto | Mancanza di configurazione automatica | Configurare regole di naming nel linter; eseguire rename batch |
| Header comment mancanti su file esistenti | File creati prima dell'adozione dello standard | Aggiungere header in batch; configurare rule nel linter se supportato |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
