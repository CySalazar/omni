# Security Guidelines — Linee Guida di Sicurezza

## Metadata

- **ID:** DIR-012
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** DIR-010 (Code Standards), DIR-011 (Testing Strategy)
- **Tipo di progetto:** universale

---

## Obiettivo

Garantire che ogni progetto implementi sicurezza by-default attraverso regole operative
concrete per la gestione di credenziali, la validazione degli input, la protezione delle
dipendenze e l'hardening dell'infrastruttura, seguendo il principio del framework D.O.E.:
**sicurezza, stabilita, prestazioni — in quest'ordine**.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio, il framework e l'architettura sono stati selezionati e documentati.
- L'agent ha consultato `docs/tech-specs.md` per conoscere le versioni e i vincoli del progetto.
- I Code Standards (DIR-010) sono stati letti e compresi.

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Tipo di progetto | stringa | webapp, api, cli, bot, pipeline, libreria, ecc. |
| Stack tecnologico | da `docs/tech-specs.md` | Linguaggio, framework, database, servizi esterni |
| Requisiti normativi | testo (opzionale) | GDPR, HIPAA, PCI-DSS, SOC 2 o altri vincoli regolamentari |
| Modello di deployment | stringa | Cloud (AWS/GCP/Azure), on-premise, container, serverless |
| Superficie di attacco | testo (opzionale) | Input utente, API pubbliche, file upload, webhook, ecc. |

---

## Procedura

### 1. Gestione Credenziali e Segreti

#### 1.1 Principio fondamentale

**Nessun segreto deve mai apparire nel codice sorgente, nei commenti, nei log o nella
storia di Git.** Questo include: password, token API, chiavi private, connection string,
certificati, chiavi di cifratura e qualsiasi dato il cui disclosure comprometterebbe
la sicurezza del sistema.

#### 1.2 Regole operative

| Regola | Descrizione | Verifica |
|--------|-------------|----------|
| **Segreti in `.env`** | Tutti i segreti sono variabili d'ambiente caricate da `.env` | `grep -r` per pattern di segreti nel codice |
| **`.env` nel `.gitignore`** | Il file `.env` non deve MAI essere committato | Verificare `.gitignore` prima del primo commit |
| **`.env.example` committato** | Template con chiavi vuote e commenti esplicativi | Presente nella root del progetto |
| **No segreti nei default** | I valori di default nel codice non devono essere segreti reali | Review del codice di configurazione |
| **No segreti nei test** | I test usano valori fittizi, mai segreti reali | Review dei file di test |

#### 1.3 Pattern di caricamento segreti per linguaggio

**Python:**
```python
import os
from dotenv import load_dotenv

load_dotenv()

# CORRETTO: caricamento con validazione
DATABASE_URL = os.environ["DATABASE_URL"]  # Fallisce esplicitamente se mancante

# CORRETTO: con valore di default NON sensibile
DEBUG_MODE = os.getenv("DEBUG_MODE", "false").lower() == "true"

# SBAGLIATO: default con segreto reale
# API_KEY = os.getenv("API_KEY", "sk-real-key-here")  # MAI FARE QUESTO
```

**TypeScript:**
```typescript
import { z } from "zod";
import dotenv from "dotenv";

dotenv.config();

// CORRETTO: validazione schema con zod
const envSchema = z.object({
  DATABASE_URL: z.string().url(),
  API_KEY: z.string().min(1),
  PORT: z.coerce.number().default(3000),
  NODE_ENV: z.enum(["development", "staging", "production"]).default("development"),
});

// Fallisce all'avvio se le variabili mancano o sono invalide
export const env = envSchema.parse(process.env);
```

**C#:**
```csharp
// CORRETTO: configurazione tipizzata con validazione
public class DatabaseSettings
{
    [Required]
    public string ConnectionString { get; set; } = string.Empty;

    [Range(1, 100)]
    public int MaxPoolSize { get; set; } = 10;
}

// In Program.cs / Startup.cs
builder.Services.AddOptions<DatabaseSettings>()
    .Bind(builder.Configuration.GetSection("Database"))
    .ValidateDataAnnotations()
    .ValidateOnStart();  // Fallisce all'avvio se invalido
```

**Go:**
```go
// CORRETTO: struct con validazione esplicita
type Config struct {
    DatabaseURL string `env:"DATABASE_URL,required"`
    APIKey      string `env:"API_KEY,required"`
    Port        int    `env:"PORT" envDefault:"3000"`
}

func LoadConfig() (*Config, error) {
    cfg := &Config{}
    if err := env.Parse(cfg); err != nil {
        return nil, fmt.Errorf("failed to load configuration: %w", err)
    }
    return cfg, nil
}
```

#### 1.4 Secrets management per ambienti cloud

Per ambienti di produzione, i segreti non devono risiedere in file `.env` ma in un
secrets manager dedicato. L'agent deve documentare quale sistema usare in `docs/deployment.md`.

| Provider | Servizio | Quando usarlo |
|----------|----------|---------------|
| AWS | Secrets Manager / SSM Parameter Store | Progetti su AWS. SSM per config semplici, Secrets Manager per rotazione automatica |
| GCP | Secret Manager | Progetti su Google Cloud |
| Azure | Key Vault | Progetti su Azure |
| Multi-cloud / On-premise | HashiCorp Vault | Ambienti ibridi o requisiti avanzati di rotazione e audit |
| Kubernetes | External Secrets Operator + backend | Cluster K8s che devono sincronizzare segreti da un provider |

**Regola:** In produzione, il sistema deve poter avviarsi senza file `.env`. I segreti
vengono iniettati dall'ambiente (variabili d'ambiente del container, secrets mount, o SDK
del secrets manager).

#### 1.5 Rotazione delle chiavi

L'agent deve documentare la procedura di rotazione per ogni segreto in `docs/security.md`:

```markdown
## Procedura di Rotazione Chiavi

### API_KEY (servizio esterno X)
1. Generare nuova chiave dal pannello di X
2. Aggiornare il valore nel secrets manager
3. Verificare che il servizio funzioni con la nuova chiave
4. Revocare la vecchia chiave
5. Aggiornare la data di rotazione nel registro

### DATABASE_PASSWORD
1. Creare nuovo utente DB con nuova password (o usare rotazione nativa del secrets manager)
2. Aggiornare la connection string nel secrets manager
3. Eseguire rolling restart dei servizi
4. Verificare connettivita
5. Rimuovere vecchio utente / revocare vecchia password
```

---

### 2. Validazione e Sanitizzazione degli Input

#### 2.1 Principio fondamentale

**TUTTI gli input provenienti dall'esterno sono considerati non fidati** fino a prova
contraria. "Esterno" include: input utente (form, query string, header, body), dati da
API di terze parti, file caricati, webhook, messaggi da code, e qualsiasi dato che non
sia stato generato e validato dal sistema stesso.

#### 2.2 SQL Injection — Prevenzione

**Regola assoluta: SEMPRE query parametrizzate, MAI concatenazione di stringhe.**

**Python (SQLAlchemy):**
```python
# CORRETTO: query parametrizzata
stmt = select(User).where(User.email == email_input)
result = await session.execute(stmt)

# CORRETTO: raw query parametrizzata (quando necessario)
stmt = text("SELECT * FROM users WHERE email = :email")
result = await session.execute(stmt, {"email": email_input})

# SBAGLIATO: concatenazione — vulnerabile a SQL injection
# query = f"SELECT * FROM users WHERE email = '{email_input}'"  # MAI
```

**TypeScript (Prisma / raw query):**
```typescript
// CORRETTO: ORM con parametri automatici
const user = await prisma.user.findUnique({ where: { email: emailInput } });

// CORRETTO: raw query parametrizzata
const users = await prisma.$queryRaw`SELECT * FROM users WHERE email = ${emailInput}`;

// SBAGLIATO: template literal non sicuro
// const users = await prisma.$queryRawUnsafe(`SELECT * FROM users WHERE email = '${emailInput}'`);
```

**C# (Entity Framework / Dapper):**
```csharp
// CORRETTO: LINQ (EF Core)
var user = await _context.Users.FirstOrDefaultAsync(u => u.Email == emailInput);

// CORRETTO: raw SQL parametrizzato (EF Core)
var users = await _context.Users
    .FromSqlInterpolated($"SELECT * FROM Users WHERE Email = {emailInput}")
    .ToListAsync();

// CORRETTO: Dapper parametrizzato
var user = await connection.QueryFirstOrDefaultAsync<User>(
    "SELECT * FROM Users WHERE Email = @Email",
    new { Email = emailInput });

// SBAGLIATO: concatenazione
// var query = $"SELECT * FROM Users WHERE Email = '{emailInput}'";  // MAI
```

#### 2.3 Cross-Site Scripting (XSS) — Prevenzione

**Regola: tutto l'output inserito nel DOM deve essere escaped.**

| Contesto | Azione richiesta | Esempio |
|----------|------------------|---------|
| HTML body | HTML-encode (`<`, `>`, `&`, `"`, `'`) | `<p>{{ user_input | escape }}</p>` |
| Attributi HTML | Attributo-encode + quotare sempre | `<div title="{{ input | attr_escape }}">` |
| JavaScript inline | JSON-encode o evitare del tutto | Preferire `data-*` attributes + `JSON.parse` |
| URL / href | URL-encode + validare contro whitelist | Verificare che inizi con `https://` o path relativo |
| CSS inline | Evitare input utente in CSS | Non inserire MAI input utente in `style=""` |

**Framework con protezione automatica (usare SEMPRE):**

| Framework | Protezione XSS built-in | Attenzione a |
|-----------|------------------------|--------------|
| React | JSX escapa automaticamente | `dangerouslySetInnerHTML` — MAI con input utente |
| Angular | Template escapa automaticamente | `bypassSecurityTrust*` — MAI con input utente |
| Vue | Template escapa automaticamente | `v-html` — MAI con input utente |
| ASP.NET Razor | Escapa automaticamente | `@Html.Raw()` — MAI con input utente |
| Jinja2 (Python) | Autoescaping abilitabile | Verificare che `autoescape=True` sia attivo |
| Go `html/template` | Escapa automaticamente | Non usare `text/template` per HTML |

#### 2.4 File Upload — Validazione

I file caricati dagli utenti sono un vettore di attacco critico. L'agent deve implementare
validazione a piu livelli:

```
FILE RICEVUTO
    |
    +-- 1. Verifica dimensione (prima di leggere il contenuto)
    |       Max size configurabile, default conservativo (es. 10 MB)
    |       Se supera → Errore 413 Payload Too Large
    |
    +-- 2. Verifica tipo MIME (magic bytes, NON estensione)
    |       Usare libreria per leggere i magic bytes del file
    |       Confrontare con whitelist di tipi permessi
    |       Se non in whitelist → Errore 415 Unsupported Media Type
    |
    +-- 3. Verifica estensione
    |       Confrontare con whitelist (es. .jpg, .png, .pdf)
    |       L'estensione DEVE corrispondere al tipo MIME rilevato
    |       Se mismatch → Errore 400 Bad Request
    |
    +-- 4. Scansione contenuto (se applicabile)
    |       Per immagini: verificare che siano parsabili
    |       Per documenti: verificare assenza di macro/script
    |       Per ZIP: verificare che non contengano path traversal (../../)
    |
    +-- 5. Rinomina e salvataggio
            Generare nome univoco (UUID) — MAI usare il nome originale nel filesystem
            Salvare in directory dedicata, fuori dalla web root
            Impostare permessi restrittivi (es. 0640)
```

**Librerie consigliate per verifica magic bytes:**

| Linguaggio | Libreria | Note |
|------------|----------|------|
| Python | `python-magic` o `filetype` | `filetype` e puro Python, `python-magic` wrappa libmagic |
| TypeScript/Node | `file-type` | Puro JavaScript, supporta stream |
| C# | `Mime` o `MimeDetective` | Analisi basata su signature |
| Go | `http.DetectContentType` (stdlib) | Basato sui primi 512 bytes |

#### 2.5 URL e Redirect — Validazione

**Regola: ogni redirect basato su input utente deve essere validato contro una whitelist.**

```python
# CORRETTO: validazione redirect
from urllib.parse import urlparse

ALLOWED_HOSTS = {"example.com", "app.example.com"}

def validate_redirect_url(url: str) -> str:
    """Validate that a redirect URL is safe and allowed.

    Args:
        url: The URL to validate.

    Returns:
        The validated URL if safe.

    Raises:
        ValueError: If the URL is not in the allowed hosts.
    """
    parsed = urlparse(url)

    # Permettere solo path relativi o host nella whitelist
    if not parsed.netloc:
        # Path relativo — OK (es. /dashboard)
        if parsed.path.startswith("/"):
            return url
        raise ValueError(f"Relative path must start with /: {url}")

    if parsed.scheme not in ("https",):
        raise ValueError(f"Only HTTPS redirects allowed: {url}")

    if parsed.netloc not in ALLOWED_HOSTS:
        raise ValueError(f"Host not in allowed list: {parsed.netloc}")

    return url
```

**Attacchi comuni da prevenire:**

| Attacco | Esempio | Prevenzione |
|---------|---------|-------------|
| Open redirect | `?next=https://evil.com` | Whitelist di host permessi |
| JavaScript URI | `?url=javascript:alert(1)` | Permettere solo `https://` e path relativi |
| Data URI | `?url=data:text/html,<script>...` | Rifiutare schema `data:` |
| Protocol-relative | `?url=//evil.com` | Rifiutare URL che iniziano con `//` |

---

### 3. Principi di Sicurezza Architetturale

#### 3.1 Minimo Privilegio (Principle of Least Privilege)

Ogni componente del sistema deve avere solo i permessi strettamente necessari per
svolgere la propria funzione.

| Componente | Applicazione del principio |
|------------|---------------------------|
| Utenti DB | Utente applicativo con solo `SELECT/INSERT/UPDATE/DELETE` sulle tabelle necessarie. MAI `GRANT ALL` o utente `root`/`admin` |
| Service account | Permessi IAM specifici per risorsa. MAI `*:*` o policy `AdministratorAccess` |
| Container | Eseguire come utente non-root. `USER 1001` nel Dockerfile |
| File system | Permessi restrittivi su file di configurazione e segreti (0600 o 0640) |
| API key esterne | Scope limitato a cio che serve. Preferire token con scadenza |
| Token JWT | Claim minimali. Non inserire dati sensibili nel payload (e visibile in base64) |

#### 3.2 Defense in Depth

Non affidarsi a un singolo layer di sicurezza. Ogni livello deve implementare
le proprie protezioni.

```
RICHIESTA UTENTE
    |
    +-- Layer 1: WAF / Rate Limiting / DDoS Protection
    |       Filtra traffico malevolo prima che raggiunga l'applicazione
    |
    +-- Layer 2: Autenticazione
    |       Verifica identita (JWT, session, API key)
    |
    +-- Layer 3: Autorizzazione
    |       Verifica permessi per la risorsa richiesta (RBAC, ABAC)
    |
    +-- Layer 4: Validazione Input
    |       Valida e sanitizza tutti gli input (tipo, formato, range)
    |
    +-- Layer 5: Business Logic
    |       Regole di dominio che impediscono operazioni non permesse
    |
    +-- Layer 6: Data Access
    |       Query parametrizzate, ORM, accesso dati controllato
    |
    +-- Layer 7: Database
            Permessi restrittivi, encryption at rest, audit log
```

#### 3.3 Fail Secure

In caso di errore, il sistema deve negare l'accesso, non concederlo.

```python
# CORRETTO: fail secure
def check_permission(user: User, resource: Resource) -> bool:
    """Check if user has permission to access resource.

    Returns False (deny) on any error — fail secure.
    """
    try:
        permissions = permission_service.get_permissions(user.id)
        return resource.id in permissions.allowed_resources
    except Exception as e:
        logger.error("Permission check failed", user_id=user.id, error=str(e))
        return False  # DENY on error

# SBAGLIATO: fail open
# def check_permission(user, resource):
#     try:
#         ...
#     except Exception:
#         return True  # MAI concedere accesso in caso di errore
```

#### 3.4 Audit Trail

Le operazioni critiche devono essere loggate con dettaglio sufficiente per ricostruire
cosa e successo, chi lo ha fatto e quando.

**Operazioni che richiedono audit logging:**

| Operazione | Dati da loggare |
|------------|-----------------|
| Login (successo e fallimento) | user_id, IP, user_agent, timestamp, risultato |
| Cambio password / credenziali | user_id, timestamp, metodo (reset, cambio volontario) |
| Modifica permessi / ruoli | chi ha modificato, cosa e cambiato (before/after), timestamp |
| Accesso a dati sensibili | user_id, risorsa acceduta, timestamp, motivo (se richiesto) |
| Operazioni CRUD su risorse critiche | user_id, azione, risorsa, dati modificati (before/after) |
| Operazioni amministrative | admin_id, azione, target, timestamp |
| Export / download dati | user_id, tipo di export, filtri applicati, numero di record |

**Formato del log di audit:**

```json
{
  "timestamp": "2026-03-24T14:30:00.000Z",
  "event_type": "AUTH_LOGIN_SUCCESS",
  "actor": {
    "user_id": "usr_abc123",
    "ip": "192.168.1.100",
    "user_agent": "Mozilla/5.0..."
  },
  "action": "LOGIN",
  "resource": {
    "type": "session",
    "id": "sess_xyz789"
  },
  "result": "SUCCESS",
  "metadata": {
    "auth_method": "password_mfa",
    "mfa_type": "totp"
  }
}
```

**Regole per l'audit log:**

- I log di audit devono essere **immutabili** (append-only). Nessun processo deve poter
  modificare o cancellare log di audit.
- I log di audit devono essere conservati per un periodo definito dalla policy aziendale
  o dai requisiti normativi (documentare in `docs/security.md`).
- I log di audit NON devono contenere dati sensibili (password, token, PII non necessario).
  Usare ID di riferimento, non valori.

---

### 4. Sicurezza delle Dipendenze

#### 4.1 Principio

Le dipendenze sono codice di terze parti che viene eseguito con gli stessi privilegi
del codice dell'applicazione. Una dipendenza vulnerabile e un punto di ingresso per
un attaccante.

#### 4.2 Verifica CVE

L'agent deve verificare che le dipendenze non abbiano vulnerabilita note (CVE) prima
di includerle e periodicamente durante lo sviluppo.

**Strumenti di scansione per linguaggio:**

| Linguaggio | Strumento | Comando | Integrazione CI |
|------------|-----------|---------|-----------------|
| Python | `pip-audit` | `pip-audit` | GitHub Action disponibile |
| Python | `safety` | `safety check` | GitHub Action disponibile |
| JavaScript/TypeScript | `npm audit` | `npm audit --audit-level=high` | Built-in in npm |
| JavaScript/TypeScript | `snyk` | `snyk test` | GitHub Action disponibile |
| C# | `dotnet list package --vulnerable` | Built-in in .NET 8+ | `dotnet` CLI in CI |
| Go | `govulncheck` | `govulncheck ./...` | GitHub Action disponibile |
| Multi-linguaggio | `trivy` | `trivy fs .` | Ottimo per container e IaC |
| Multi-linguaggio | Dependabot / Renovate | Configurazione nel repo | Nativo GitHub / self-hosted |

#### 4.3 Criteri di accettazione di una dipendenza (aspetto sicurezza)

Oltre ai criteri generali di DIR-014 (Dependency Management), per la sicurezza l'agent
verifica:

```
NUOVA DIPENDENZA PROPOSTA
    |
    +-- 1. Ha CVE note non risolte?
    |       SI → NON usare. Cercare alternativa o implementare internamente
    |       NO → Procedi
    |
    +-- 2. Il progetto ha una security policy? (SECURITY.md, bug bounty)
    |       SI → Buon segnale
    |       NO → Cautela maggiore, valutare alternative
    |
    +-- 3. Le vulnerabilita passate sono state corrette rapidamente?
    |       SI → Segnale di manutenzione attiva
    |       NO → Rischio elevato di vulnerabilita future non patchate
    |
    +-- 4. Il pacchetto richiede permessi eccessivi?
    |       (es. accesso a rete per una libreria di parsing)
    |       SI → Sospetto. Verificare e documentare il motivo
    |       NO → OK
    |
    +-- 5. Il pacchetto e firmato / pubblicato da fonte verificata?
            Verificare che il maintainer sia legittimo (typosquatting)
            Verificare la supply chain (lockfile integro)
```

#### 4.4 Pinning delle versioni e lockfile

- Le versioni devono essere **pinnate** nel file di dipendenze (non usare `latest`, `*`,
  o range troppo ampi come `>=`).
- Il **lockfile** deve essere sempre committato nel repository.
- Il lockfile deve essere verificato in CI per garantire integrita.

| Linguaggio | File dipendenze | Lockfile | Da committare |
|------------|----------------|----------|---------------|
| Python (pip) | `requirements.txt` | — (il file stesso e il lock) | Si |
| Python (Poetry) | `pyproject.toml` | `poetry.lock` | Si |
| Python (uv) | `pyproject.toml` | `uv.lock` | Si |
| JavaScript/TypeScript | `package.json` | `package-lock.json` / `yarn.lock` / `pnpm-lock.yaml` | Si |
| C# | `.csproj` | `packages.lock.json` (abilitare `RestorePackagesWithLockFile`) | Si |
| Go | `go.mod` | `go.sum` | Si |
| Dart | `pubspec.yaml` | `pubspec.lock` | Si |

#### 4.5 Configurazione Dependabot / Renovate

L'agent deve configurare un sistema di aggiornamento automatico delle dipendenze.

**GitHub — Dependabot (`.github/dependabot.yml`):**
```yaml
version: 2
updates:
  - package-ecosystem: "pip"  # Adattare al linguaggio
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 10
    labels:
      - "dependencies"
      - "security"
    # Raggruppare aggiornamenti minor e patch per ridurre rumore
    groups:
      minor-and-patch:
        update-types:
          - "minor"
          - "patch"
```

**Regola:** Ogni PR di aggiornamento dipendenze deve essere trattata come codice:
review, test automatici, e merge solo se i test passano.

---

### 5. Hardening per Tipo di Progetto

#### 5.1 Web Application — Security Headers

Ogni webapp deve configurare gli header HTTP di sicurezza. L'agent li configura nel
middleware o nel reverse proxy.

| Header | Valore raccomandato | Scopo |
|--------|-------------------|-------|
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains; preload` | Forza HTTPS |
| `Content-Security-Policy` | Policy specifica per il progetto (vedi sotto) | Previene XSS e injection |
| `X-Content-Type-Options` | `nosniff` | Previene MIME-type sniffing |
| `X-Frame-Options` | `DENY` o `SAMEORIGIN` | Previene clickjacking |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Controlla informazioni nel Referer |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=()` | Disabilita API non necessarie |
| `X-XSS-Protection` | `0` | Disabilitare (il filtro e deprecato e puo causare vulnerabilita) |

**Content-Security-Policy — approccio:**
```
# Partire restrittivo e rilassare solo dove necessario
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self' 'unsafe-inline';  # Rilassare solo se necessario per il framework CSS
  img-src 'self' data: https:;
  font-src 'self';
  connect-src 'self' https://api.example.com;
  frame-ancestors 'none';
  base-uri 'self';
  form-action 'self';
```

L'agent deve adattare la CSP al progetto specifico e documentare ogni eccezione con
il motivo.

#### 5.2 API — Configurazione CORS

**Regola: CORS deve essere configurato esplicitamente. Mai usare `*` in produzione.**

```python
# CORRETTO: CORS specifico (FastAPI)
from fastapi.middleware.cors import CORSMiddleware

ALLOWED_ORIGINS_BY_ENV = {
    "development": ["http://localhost:3000", "http://localhost:5173"],
    "staging": ["https://staging.example.com"],
    "production": ["https://app.example.com"],
}

app.add_middleware(
    CORSMiddleware,
    allow_origins=ALLOWED_ORIGINS_BY_ENV[settings.ENVIRONMENT],
    allow_credentials=True,
    allow_methods=["GET", "POST", "PUT", "DELETE"],  # Solo i metodi necessari
    allow_headers=["Authorization", "Content-Type"],   # Solo gli header necessari
)

# SBAGLIATO: wildcard in produzione
# app.add_middleware(CORSMiddleware, allow_origins=["*"])  # MAI in produzione
```

#### 5.3 API — Rate Limiting

Ogni API pubblica deve avere rate limiting per prevenire abusi e DDoS a livello applicativo.

| Tipo di endpoint | Rate limit suggerito | Note |
|------------------|---------------------|------|
| Login / Auth | 5-10 req/min per IP | Prevenzione brute force |
| Registrazione | 3-5 req/min per IP | Prevenzione account spam |
| API generica (autenticata) | 100-1000 req/min per utente | Dipende dal caso d'uso |
| API pubblica (non autenticata) | 30-60 req/min per IP | Piu restrittivo |
| Upload file | 5-10 req/min per utente | Protezione risorse server |
| Password reset | 3 req/15min per email | Prevenzione email bombing |

**Regola:** I rate limit devono restituire `429 Too Many Requests` con header
`Retry-After` e un body che spiega il limite.

#### 5.4 Autenticazione — Best Practice

| Pratica | Implementazione |
|---------|----------------|
| Password hashing | `bcrypt` (cost >= 12) o `argon2id`. MAI MD5, SHA-1, SHA-256 senza salt |
| Confronto token | Confronto constant-time (`hmac.compare_digest` in Python, `crypto.timingSafeEqual` in Node) |
| Session management | ID sessione rigenerato dopo login. Scadenza configurabile. Invalidazione su logout |
| JWT | Algoritmo `RS256` o `ES256` (asimmetrico). MAI `none`. Verificare `iss`, `aud`, `exp`. Durata breve (15-60 min) + refresh token |
| MFA | Raccomandato per operazioni sensibili. TOTP preferito a SMS |
| Brute force protection | Account lockout temporaneo dopo N tentativi falliti (es. 5 tentativi → lock 15 min) |

#### 5.5 Protezione dei Dati

| Aspetto | Regola |
|---------|--------|
| Encryption at rest | I dati sensibili nel database devono essere cifrati (column-level o TDE) |
| Encryption in transit | Tutte le comunicazioni su HTTPS/TLS 1.2+. No HTTP in produzione |
| Backup encryption | I backup devono essere cifrati e l'accesso deve essere controllato |
| Data masking | Dati PII nei log e in ambienti non-produzione devono essere mascherati |
| Data retention | Definire e implementare policy di retention. Documentare in `docs/security.md` |
| Data deletion | Implementare hard delete o anonymization per richieste GDPR/privacy |

---

### 6. Sicurezza dell'Infrastruttura (Container e Deploy)

#### 6.1 Dockerfile — Best Practice di Sicurezza

```dockerfile
# CORRETTO: immagine base specifica e minimale
FROM python:3.12-slim AS base

# Non eseguire come root
RUN groupadd -r appuser && useradd -r -g appuser -d /app -s /sbin/nologin appuser

WORKDIR /app

# Copiare prima i file di dipendenze per sfruttare il layer cache
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copiare il codice
COPY --chown=appuser:appuser . .

# Passare a utente non-root
USER appuser

# Esporre solo la porta necessaria
EXPOSE 8000

# Healthcheck
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

CMD ["gunicorn", "app.main:app", "--bind", "0.0.0.0:8000"]
```

**Checklist sicurezza Dockerfile:**

- [ ] Immagine base specifica e pinned (tag con digest, non `latest`)
- [ ] Multi-stage build per ridurre superficie di attacco
- [ ] Utente non-root (`USER`)
- [ ] No `--no-cache-dir` per pip (riduce dimensione immagine)
- [ ] `.dockerignore` configurato per escludere `.env`, `.git`, `node_modules`, test
- [ ] Nessun segreto nel Dockerfile o nei layer dell'immagine
- [ ] Scansione vulnerabilita dell'immagine (`trivy image`, `docker scout`)

#### 6.2 Scansione immagini container

L'agent deve configurare la scansione delle immagini container nella pipeline CI:

```yaml
# GitHub Actions — esempio
- name: Scan container image
  uses: aquasecurity/trivy-action@master
  with:
    image-ref: "${{ env.IMAGE_NAME }}:${{ github.sha }}"
    format: "sarif"
    output: "trivy-results.sarif"
    severity: "CRITICAL,HIGH"
    exit-code: "1"  # Fallisce la pipeline se trova vulnerabilita critiche
```

---

### 7. Checklist di Sicurezza Pre-Commit

Prima di considerare qualsiasi codice pronto per il commit, l'agent verifica:

```
CHECKLIST DI SICUREZZA PRE-COMMIT
    |
    +-- [ ] Nessun segreto nel codice, nei commenti o nei file di test
    |       Verifica: grep per pattern (API_KEY, password, token, secret, key)
    |
    +-- [ ] Nessun segreto nella storia Git
    |       Se un segreto e stato committato per errore: rotare il segreto IMMEDIATAMENTE,
    |       poi pulire la storia con git-filter-repo (non basta un nuovo commit)
    |
    +-- [ ] Input utente validato e sanitizzato
    |       Ogni endpoint/funzione che riceve input esterni ha validazione esplicita
    |
    +-- [ ] Query parametrizzate (nessuna concatenazione SQL)
    |       Verifica: grep per pattern di concatenazione in query
    |
    +-- [ ] Error handling che non espone dettagli interni
    |       In produzione: messaggi generici all'utente, dettagli nei log
    |       Mai esporre: stack trace, path del filesystem, versioni di librerie
    |
    +-- [ ] Security headers configurati (se webapp)
    |       Verificare con securityheaders.com o strumento equivalente
    |
    +-- [ ] CORS configurato esplicitamente (se API)
    |       Nessun wildcard (*) in produzione
    |
    +-- [ ] Rate limiting attivo su endpoint sensibili
    |       Login, registrazione, password reset, upload
    |
    +-- [ ] Dipendenze senza CVE critiche note
    |       Eseguire lo strumento di audit del linguaggio
    |
    +-- [ ] Autenticazione e autorizzazione verificate per ogni endpoint protetto
    |       Nessun endpoint sensibile accessibile senza autenticazione
    |
    +-- [ ] Logging di audit per operazioni critiche
            Login, modifica permessi, accesso dati sensibili, operazioni admin
```

---

### 8. Gestione Incidenti di Sicurezza

Quando l'agent rileva o sospetta un problema di sicurezza durante lo sviluppo, deve
seguire questo protocollo:

```
PROBLEMA DI SICUREZZA RILEVATO
    |
    +-- 1. STOP immediato — non continuare lo sviluppo
    |
    +-- 2. Classifica la severita
    |       CRITICO: segreto esposto, vulnerabilita sfruttabile, data breach
    |       ALTO: vulnerabilita confermata ma non ancora sfruttata
    |       MEDIO: configurazione debole, dipendenza vulnerabile con mitigazione parziale
    |       BASSO: best practice non rispettata, rischio teorico
    |
    +-- 3. Azione per severita
    |       CRITICO → Informare l'utente IMMEDIATAMENTE. Proporre azioni di mitigazione urgenti
    |       ALTO → Informare l'utente. Correggere prima di procedere
    |       MEDIO → Documentare. Proporre fix nel prossimo ciclo
    |       BASSO → Documentare in TODO con tag SECURITY. Proporre fix
    |
    +-- 4. Se un segreto e stato esposto
    |       a. Segnalare all'utente IMMEDIATAMENTE
    |       b. Proporre rotazione del segreto
    |       c. Se committato in Git: proporre pulizia storia con git-filter-repo
    |       d. Aggiornare docs/security.md con la lezione appresa
    |
    +-- 5. Documentare
            Aggiornare la sezione "Lezioni Apprese" della direttiva rilevante
            Aggiornare docs/security.md se applicabile
```

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| Codice sicuro conforme alle linee guida | File sorgenti | `src/` (o struttura equivalente) |
| Configurazione `.env.example` | File di esempio | Root del progetto |
| Configurazione `.gitignore` (aspetto sicurezza) | File | Root del progetto |
| `docs/security.md` | Markdown | `docs/` |
| Configurazione security headers | File di configurazione o middleware | Dipende dal framework |
| Configurazione CORS | File di configurazione o middleware | Dipende dal framework |
| Configurazione rate limiting | File di configurazione o middleware | Dipende dal framework |
| Configurazione Dependabot/Renovate | YAML | `.github/` o root |
| Configurazione scansione container | YAML (CI) | `.github/workflows/` o equivalente |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| Segreto trovato nel codice durante review | Developer ha hardcodato un valore | Rimuovere dal codice, spostare in `.env`, verificare che non sia nella storia Git |
| Dipendenza con CVE critica | Vulnerabilita scoperta dopo l'inclusione | Aggiornare alla versione patchata. Se non disponibile: valutare alternativa o applicare mitigazione documentata |
| CORS error in produzione | Origins non configurati correttamente | Verificare la lista di origini permesse per l'ambiente di produzione |
| Rate limiting troppo aggressivo | Limiti impostati troppo bassi | Analizzare il traffico reale, regolare i limiti e documentare la decisione |
| Security header bloccano funzionalita | CSP troppo restrittiva | Aggiungere l'eccezione specifica nella CSP con commento che spiega il motivo |
| File upload rifiutato legittimo | Magic bytes non riconosciuti | Verificare il tipo, aggiungere alla whitelist se legittimo, documentare |
| Autenticazione fallisce dopo deploy | Segreti non configurati nell'ambiente | Verificare che il secrets manager contenga tutti i segreti necessari (checklist in docs/deployment.md) |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
