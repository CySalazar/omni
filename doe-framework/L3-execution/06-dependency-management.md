# Dependency Management — Gestione Dipendenze e Compatibilita

## Metadata

- **ID:** DIR-015
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** DIR-010 (Code Standards), DIR-013 (Documentation Rules)
- **Tipo di progetto:** universale

---

## Obiettivo

Definire un protocollo strutturato per la selezione, l'integrazione, il versionamento e
la manutenzione delle dipendenze di progetto, prevenendo conflitti di compatibilita,
vulnerabilita note e bloat non necessario. Il documento stabilisce regole operative
che l'agent deve seguire PRIMA di aggiungere qualsiasi dipendenza e DURANTE l'intero
ciclo di vita del progetto.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio e il framework sono stati selezionati e documentati.
- L'agent ha consultato `docs/tech-specs.md` per conoscere le versioni e i vincoli gia presenti.
- Gli standard di codice (DIR-010) e le regole di documentazione (DIR-013) sono stati consultati.

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Linguaggio scelto | stringa | Il linguaggio di programmazione del progetto |
| Framework scelto | stringa | Il framework principale (se presente) |
| `docs/tech-specs.md` | file Markdown | Specifiche tecniche con versioni, dipendenze e vincoli di compatibilita |
| Package manager | stringa | Il package manager in uso (npm, pip, dotnet, go mod, pub, ecc.) |
| File di lock esistente | file (opzionale) | `package-lock.json`, `poetry.lock`, `go.sum`, ecc. |

---

## Procedura

### 1. Il File `docs/tech-specs.md` come Source of Truth

Il file `docs/tech-specs.md` e il **registro centrale** di tutte le dipendenze del progetto.
L'agent lo consulta SEMPRE prima di aggiungere, aggiornare o rimuovere una dipendenza.
Nessuna dipendenza deve esistere nel progetto senza essere documentata in questo file.

#### 1.1 Struttura obbligatoria della sezione dipendenze in `docs/tech-specs.md`

```markdown
# Specifiche Tecniche

## Runtime
- Linguaggio: <linguaggio> <versione>
- Framework: <framework> <versione>
- Package Manager: <package manager> <versione>
- Database: <database> <versione> (se applicabile)
- Runtime aggiuntivi: <Node.js, .NET Runtime, JVM, ecc.> <versione>

## Dipendenze Principali (production)
| Pacchetto | Versione | Scopo | Licenza | Note compatibilita |
|-----------|----------|-------|---------|-------------------|
| esempio | 2.1.x | Descrizione breve | MIT | Non compatibile con v1 API |

## Dipendenze di Sviluppo (dev)
| Pacchetto | Versione | Scopo | Note |
|-----------|----------|-------|------|
| esempio-test | 4.x | Framework di test | — |

## Vincoli di Compatibilita
- <vincolo 1>: descrizione del vincolo e motivazione
- <vincolo 2>: descrizione del vincolo e motivazione

## Dipendenze Deprecate / Da Sostituire
| Pacchetto | Motivo | Sostituto proposto | Priorita |
|-----------|--------|-------------------|----------|
| — | — | — | — |
```

#### 1.2 Regole di aggiornamento del file

- Ogni nuova dipendenza aggiunta al progetto DEVE essere registrata in `docs/tech-specs.md`
  **contestualmente** all'installazione (non dopo).
- Ogni aggiornamento di versione di una dipendenza esistente DEVE aggiornare la riga
  corrispondente.
- Ogni rimozione di una dipendenza DEVE rimuovere la riga corrispondente.
- I vincoli di compatibilita scoperti durante lo sviluppo DEVONO essere documentati
  nella sezione "Vincoli di Compatibilita".

---

### 2. Selezione di una Nuova Dipendenza — Checklist Obbligatoria

Prima di aggiungere una nuova dipendenza, l'agent deve completare questa checklist.
Se uno dei punti critici (marcati con `[CRITICO]`) fallisce, la dipendenza NON deve
essere aggiunta senza approvazione esplicita dell'utente.

```
NUOVA DIPENDENZA PROPOSTA: <nome-pacchetto>
    |
    +-- 1. [CRITICO] Necessita reale
    |       La funzionalita puo essere implementata con il linguaggio/framework base
    |       senza dipendenza aggiuntiva?
    |       +-- SI --> Non aggiungere la dipendenza. Implementa internamente.
    |       +-- NO --> Procedi al punto 2.
    |       +-- PARZIALMENTE --> Valuta il rapporto costo/beneficio:
    |                            - Complessita dell'implementazione interna
    |                            - Rischio di bug nell'implementazione interna
    |                            - Tempo di sviluppo vs tempo di integrazione
    |                            --> Documenta la decisione in un ADR se significativa
    |
    +-- 2. [CRITICO] Manutenzione attiva
    |       - Ultimo commit: < 6 mesi?
    |       - Issue aperte risposte/gestite?
    |       - Release regolari?
    |       - Numero di maintainer > 1? (riduce il bus factor)
    |       +-- Se ABBANDONATO o MORIBONDO --> Cerca alternativa o implementa internamente
    |
    +-- 3. [CRITICO] Sicurezza
    |       - Ha CVE note non risolte? (verificare su osv.dev, snyk.io, npm audit, pip-audit)
    |       - Ha una security policy documentata?
    |       +-- Se CVE CRITICHE aperte --> NON aggiungere. Segnala all'utente.
    |
    +-- 4. [CRITICO] Licenza compatibile
    |       - La licenza e compatibile con il progetto?
    |       - Licenze permissive (MIT, Apache 2.0, BSD): generalmente sicure
    |       - Licenze copyleft (GPL, AGPL): richiedono attenzione
    |       - Licenze proprietarie: richiedono approvazione dell'utente
    |       +-- Se INCOMPATIBILE --> NON aggiungere. Segnala all'utente.
    |
    +-- 5. Compatibilita con lo stack esistente
    |       - E compatibile con la versione del linguaggio in uso?
    |       - E compatibile con il framework in uso?
    |       - Conflitti con dipendenze gia presenti?
    |       --> Consultare docs/tech-specs.md per verificare vincoli
    |       +-- Se CONFLITTO --> Cercare versione compatibile o alternativa
    |
    +-- 6. Peso delle dipendenze transitive
    |       - Quante dipendenze transitive porta con se?
    |       - Preferire dipendenze "leggere" (poche o zero transitive)
    |       - Verificare che le transitive non introducano conflitti
    |       --> npm: `npm ls <pacchetto>`
    |       --> pip: `pipdeptree -p <pacchetto>`
    |       --> dotnet: `dotnet list package --include-transitive`
    |       --> go: `go mod graph | grep <pacchetto>`
    |
    +-- 7. Qualita della documentazione
    |       - Ha documentazione ufficiale?
    |       - Ha esempi d'uso?
    |       - Ha una community attiva (Stack Overflow, GitHub Discussions)?
    |
    +-- 8. Alternativa valutata
            - Sono state considerate almeno 2 alternative?
            - La scelta e motivata e documentata?
            --> Per dipendenze significative: creare un ADR
```

#### 2.1 Matrice di decisione rapida

Per decisioni rapide su dipendenze minori, l'agent puo usare questa matrice semplificata:

| Criterio | Peso | Soglia minima |
|----------|------|---------------|
| Necessita reale | Bloccante | Non implementabile internamente in < 2h |
| Manutenzione | Bloccante | Ultimo commit < 12 mesi |
| Sicurezza (CVE) | Bloccante | Zero CVE critiche/alte aperte |
| Licenza | Bloccante | Compatibile con il progetto |
| Compatibilita stack | Alto | Nessun conflitto con dipendenze esistenti |
| Dipendenze transitive | Medio | < 20 dipendenze transitive (soft limit) |
| Documentazione | Medio | README + esempi base |
| Community | Basso | > 100 stelle GitHub (soft limit, non applicabile a tutti) |

---

### 3. Versionamento delle Dipendenze

#### 3.1 Principio fondamentale: Pin esplicito

Le versioni delle dipendenze devono essere **pinnate esplicitamente**. Mai usare range
aperti o `latest`. Il motivo e semplice: una build che funziona oggi deve funzionare
identicamente domani.

#### 3.2 Strategie di pinning per package manager

| Package Manager | File di specifica | File di lock | Strategia di pinning |
|----------------|-------------------|-------------|---------------------|
| npm / yarn / pnpm | `package.json` | `package-lock.json` / `yarn.lock` / `pnpm-lock.yaml` | Usare versioni esatte (`"express": "4.18.2"`) o range stretti (`"~4.18.2"`). MAI `"*"` o `"latest"`. Il lock file e SEMPRE committato. |
| pip (Python) | `pyproject.toml` o `requirements.txt` | `poetry.lock` o `pip-compile` output | Pinnare con `==` in requirements.txt o con range in pyproject.toml + lock file. Il lock file e SEMPRE committato. |
| dotnet (C#) | `.csproj` | `packages.lock.json` | Usare versioni esatte nei PackageReference. Abilitare `RestorePackagesWithLockFile` in `.csproj`. |
| go mod | `go.mod` | `go.sum` | Go pinna automaticamente. `go.sum` e SEMPRE committato. Usare `go mod tidy` regolarmente. |
| pub (Dart) | `pubspec.yaml` | `pubspec.lock` | Usare range compatibili (`^1.2.3`). Il lock file e SEMPRE committato per applicazioni (non per pacchetti/librerie). |

#### 3.3 Regole sul lock file

- Il lock file **DEVE essere committato** nel repository per applicazioni e servizi.
- Il lock file **NON deve essere committato** per librerie/pacchetti pubblicati
  (il consumatore generera il proprio).
- Il lock file non deve MAI essere modificato manualmente.
- Se il lock file genera conflitti in un merge, rigenerarlo con un install pulito
  piuttosto che risolvere i conflitti manualmente.

#### 3.4 Semantic Versioning — comprensione e applicazione

L'agent deve comprendere e applicare SemVer (https://semver.org) per valutare
l'impatto degli aggiornamenti:

```
MAJOR.MINOR.PATCH

MAJOR: breaking changes — richiede review del codice e dei test
MINOR: nuove funzionalita retrocompatibili — aggiornamento generalmente sicuro
PATCH: bugfix retrocompatibili — aggiornamento sicuro
```

| Tipo di aggiornamento | Rischio | Azione dell'agent |
|----------------------|---------|-------------------|
| Patch (1.2.3 --> 1.2.4) | Basso | Aggiorna, esegui test, procedi |
| Minor (1.2.3 --> 1.3.0) | Medio | Aggiorna, verifica changelog, esegui test |
| Major (1.2.3 --> 2.0.0) | Alto | STOP: leggi migration guide, valuta impatto, chiedi approvazione utente |
| Pre-release (2.0.0-beta.1) | Variabile | Solo con approvazione esplicita dell'utente |

---

### 4. Installazione e Integrazione

#### 4.1 Procedura di installazione

Quando l'agent aggiunge una dipendenza, deve seguire questa sequenza:

```
1. CONSULTA docs/tech-specs.md
       --> Verificare compatibilita con stack esistente
       --> Verificare che non esista gia una dipendenza con lo stesso scopo

2. ESEGUI la checklist di selezione (Sezione 2)

3. INSTALLA la dipendenza con il package manager del progetto
       --> npm install <pacchetto>@<versione>
       --> pip install <pacchetto>==<versione>
       --> dotnet add package <pacchetto> --version <versione>
       --> go get <pacchetto>@v<versione>
       --> dart pub add <pacchetto>:<versione>

4. AGGIORNA docs/tech-specs.md
       --> Aggiungi la riga nella tabella appropriata
       --> Aggiungi eventuali vincoli di compatibilita scoperti

5. VERIFICA il lock file
       --> Assicurati che il lock file sia stato aggiornato
       --> Verifica che non ci siano conflitti con dipendenze esistenti

6. ESEGUI i test esistenti
       --> Tutti i test devono passare dopo l'aggiunta
       --> Se un test fallisce, investigare l'incompatibilita

7. COMMITTA con messaggio descrittivo
       --> deps(<scope>): add <pacchetto> v<versione> for <scopo>
```

#### 4.2 Separazione dipendenze production vs development

Le dipendenze devono essere separate in base al loro scopo:

| Tipo | Descrizione | Esempio | Comando |
|------|-------------|---------|---------|
| **Production** | Necessarie a runtime | Express, Pydantic, Serilog | `npm install`, `pip install`, `dotnet add package` |
| **Development** | Solo per sviluppo/test/build | Jest, Pytest, Ruff, ESLint | `npm install --save-dev`, `pip install` (in group dev), `dotnet add package` (in ItemGroup Condition) |
| **Peer** | Richieste ma non installate (per librerie) | React (per un componente React) | `npm install --save-peer` |
| **Optional** | Funzionalita aggiuntiva non critica | Driver DB specifico | Documentare in README come opzionale |

**Regola:** Mai installare una dipendenza di sviluppo come dipendenza di produzione.
Questo aumenta il bundle size e la superficie di attacco in produzione.

---

### 5. Aggiornamento delle Dipendenze

#### 5.1 Strategia di aggiornamento

Le dipendenze devono essere aggiornate regolarmente per beneficiare di bugfix, patch
di sicurezza e miglioramenti. Tuttavia, gli aggiornamenti devono essere controllati
e verificati.

#### 5.2 Frequenza raccomandata

| Tipo di aggiornamento | Frequenza | Automazione |
|----------------------|-----------|-------------|
| Patch di sicurezza | Immediato (appena disponibile) | Dependabot / Renovate con auto-merge |
| Patch (bugfix) | Settimanale o bi-settimanale | Dependabot / Renovate con review |
| Minor (nuove feature) | Mensile | Dependabot / Renovate con review |
| Major (breaking changes) | Pianificato (sprint/milestone) | Manuale con migration plan |

#### 5.3 Procedura di aggiornamento

```
AGGIORNAMENTO DIPENDENZA: <pacchetto> da <vecchia-versione> a <nuova-versione>
    |
    +-- 1. LEGGI il changelog / release notes della dipendenza
    |       --> Identificare breaking changes
    |       --> Identificare deprecation
    |       --> Identificare nuove feature rilevanti
    |
    +-- 2. CLASSIFICA il tipo di aggiornamento (patch / minor / major)
    |       --> Se MAJOR: leggi la migration guide completa
    |
    +-- 3. VERIFICA compatibilita
    |       --> Consultare docs/tech-specs.md per vincoli noti
    |       --> Verificare compatibilita con le altre dipendenze
    |
    +-- 4. AGGIORNA la dipendenza
    |       --> Usa il package manager del progetto
    |       --> Aggiorna il lock file
    |
    +-- 5. ESEGUI tutti i test
    |       +-- Test PASSANO --> Procedi al punto 6
    |       +-- Test FALLISCONO -->
    |           +-- Errore comprensibile --> Correggi il codice, riesegui
    |           +-- Errore non chiaro --> Rollback, segnala all'utente
    |
    +-- 6. AGGIORNA docs/tech-specs.md
    |       --> Nuova versione nella tabella
    |       --> Eventuali nuovi vincoli di compatibilita
    |
    +-- 7. COMMITTA con messaggio descrittivo
            --> deps(<scope>): update <pacchetto> from <v-old> to <v-new>
```

#### 5.4 Automazione degli aggiornamenti

L'agent deve configurare almeno uno strumento di automazione per il monitoraggio
degli aggiornamenti:

| Strumento | Piattaforma | Configurazione |
|-----------|-------------|----------------|
| **Dependabot** | GitHub | `.github/dependabot.yml` |
| **Renovate** | GitHub, GitLab, Bitbucket | `renovate.json` |
| **pip-audit** | Python (qualsiasi) | Esecuzione in CI |
| **npm audit** | Node.js (qualsiasi) | Esecuzione in CI |
| **dotnet list package --outdated** | .NET (qualsiasi) | Esecuzione in CI |

**Esempio di configurazione Dependabot (GitHub):**

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "<ecosistema>"  # npm, pip, nuget, gomod, pub
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
    open-pull-requests-limit: 10
    reviewers:
      - "<utente-o-team>"
    labels:
      - "dependencies"
      - "automated"
    # Raggruppamento aggiornamenti minori/patch per ridurre rumore
    groups:
      minor-and-patch:
        update-types:
          - "minor"
          - "patch"
```

**Esempio di configurazione Renovate:**

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": [
    "config:recommended",
    ":automergeMinor",
    ":automergeDigest",
    "group:allNonMajor"
  ],
  "labels": ["dependencies", "automated"],
  "vulnerabilityAlerts": {
    "enabled": true,
    "labels": ["security"]
  },
  "packageRules": [
    {
      "matchUpdateTypes": ["major"],
      "automerge": false
    }
  ]
}
```

---

### 6. Rimozione di Dipendenze

#### 6.1 Quando rimuovere

Una dipendenza deve essere rimossa quando:

- Non e piu usata nel codice (dipendenza orfana).
- E stata sostituita da un'alternativa migliore.
- E stata deprecata senza sostituto (funzionalita implementata internamente).
- Introduce rischi di sicurezza non risolvibili.
- La sua licenza e diventata incompatibile.

#### 6.2 Procedura di rimozione

```
1. VERIFICA che la dipendenza non sia usata
       --> Cerca riferimenti nel codice: import, require, using, ecc.
       --> Cerca riferimenti nei file di configurazione
       --> Cerca riferimenti nei test

2. RIMUOVI la dipendenza con il package manager
       --> npm uninstall <pacchetto>
       --> pip uninstall <pacchetto>
       --> dotnet remove package <pacchetto>
       --> Rimuovi da go.mod + go mod tidy
       --> dart pub remove <pacchetto>

3. ESEGUI tutti i test
       --> Verificare che nulla si rompa

4. AGGIORNA docs/tech-specs.md
       --> Rimuovi la riga dalla tabella

5. COMMITTA con messaggio descrittivo
       --> deps(<scope>): remove <pacchetto> — <motivo breve>
```

#### 6.3 Audit periodico delle dipendenze orfane

L'agent deve verificare periodicamente (o su richiesta dell'utente) che non ci siano
dipendenze installate ma non utilizzate:

| Linguaggio | Strumento | Comando |
|------------|-----------|---------|
| JavaScript/TypeScript | `depcheck` | `npx depcheck` |
| Python | `pip-extra-reqs` + `pip-missing-reqs` | `pip-extra-reqs src/` |
| Go | built-in | `go mod tidy` (rimuove automaticamente) |
| Dart | `dart pub deps --no-dev` | Verifica manuale |
| C# | Roslyn Analyzers | IDE integration (warning su using inutilizzati) |

---

### 7. Gestione dei Conflitti di Versione

#### 7.1 Tipi di conflitto

| Tipo | Descrizione | Esempio |
|------|-------------|---------|
| **Diretto** | Due dipendenze richiedono versioni incompatibili della stessa dipendenza transitiva | A richiede X@1.x, B richiede X@2.x |
| **Peer** | Una dipendenza richiede una peer dependency non presente o di versione sbagliata | React component richiede react@18, progetto ha react@17 |
| **Runtime** | Compatibilita a livello di runtime (versione linguaggio, OS, architettura) | Pacchetto non supporta la versione di Node.js in uso |
| **Licenza** | Incompatibilita tra licenze delle dipendenze | Progetto MIT con dipendenza GPL |

#### 7.2 Strategia di risoluzione

```
CONFLITTO RILEVATO
    |
    +-- 1. IDENTIFICA il tipo di conflitto
    |
    +-- 2. CONSULTA docs/tech-specs.md per vincoli noti
    |
    +-- 3. STRATEGIE per tipo:
    |       |
    |       +-- Diretto:
    |       |   a) Cercare una versione delle dipendenze che condividano
    |       |      una versione compatibile della transitiva
    |       |   b) Usare resolution/overrides del package manager
    |       |      (npm overrides, pip constraints, dotnet binding redirects)
    |       |   c) Sostituire una delle dipendenze in conflitto
    |       |
    |       +-- Peer:
    |       |   a) Aggiornare la peer dependency alla versione richiesta
    |       |   b) Se non possibile, cercare versione della dipendenza
    |       |      compatibile con la peer attuale
    |       |
    |       +-- Runtime:
    |       |   a) Aggiornare il runtime alla versione richiesta
    |       |   b) Se non possibile, cercare versione della dipendenza
    |       |      compatibile con il runtime attuale
    |       |   c) Documentare il vincolo di runtime in docs/tech-specs.md
    |       |
    |       +-- Licenza:
    |           a) Sostituire la dipendenza con una a licenza compatibile
    |           b) Implementare la funzionalita internamente
    |           c) Se inevitabile: STOP, segnala all'utente per decisione legale
    |
    +-- 4. DOCUMENTA la risoluzione in docs/tech-specs.md
    |       --> Sezione "Vincoli di Compatibilita"
    |
    +-- 5. ESEGUI tutti i test dopo la risoluzione
```

#### 7.3 Override e resolution — uso con cautela

Gli override dei package manager (npm `overrides`, yarn `resolutions`, pip `constraints`)
sono strumenti potenti ma pericolosi. L'agent deve:

- Usarli SOLO come ultima risorsa, dopo aver esaurito le altre strategie.
- Documentare SEMPRE il motivo dell'override in `docs/tech-specs.md`.
- Verificare che l'override non introduca incompatibilita a runtime.
- Aggiungere un commento nel file di configurazione del package manager.

```json
// package.json — esempio di override documentato
{
  "overrides": {
    // OVERRIDE: forza lodash 4.17.21 per risolvere CVE-2021-23337
    // Conflitto tra dep-a (lodash@4.17.15) e dep-b (lodash@4.17.21)
    // Verificato: dep-a funziona correttamente con 4.17.21
    // Rimuovere quando dep-a aggiorna la sua dipendenza
    "lodash": "4.17.21"
  }
}
```

---

### 8. Sicurezza delle Dipendenze

#### 8.1 Scansione delle vulnerabilita

L'agent deve verificare le vulnerabilita delle dipendenze in due momenti:

1. **All'installazione:** Prima di committare una nuova dipendenza.
2. **In CI/CD:** Come step della pipeline (vedi `05-cicd-setup.md`).

#### 8.2 Strumenti di scansione per linguaggio

| Linguaggio | Strumento | Comando | Database CVE |
|------------|-----------|---------|-------------|
| JavaScript/TypeScript | `npm audit` | `npm audit --audit-level=high` | GitHub Advisory DB |
| Python | `pip-audit` | `pip-audit` | OSV (osv.dev) |
| Python | `safety` | `safety check` | SafetyCLI DB |
| C# / .NET | `dotnet list package --vulnerable` | Built-in | GitHub Advisory DB |
| Go | `govulncheck` | `govulncheck ./...` | Go Vulnerability DB |
| Dart | — | Verifica manuale su osv.dev | OSV |
| Multi-linguaggio | `trivy` | `trivy fs .` | Multiple (NVD, GitHub, OSV) |
| Multi-linguaggio | `snyk` | `snyk test` | Snyk Vulnerability DB |

#### 8.3 Classificazione e risposta alle vulnerabilita

| Severita (CVSS) | Azione | Tempistica |
|-----------------|--------|------------|
| Critica (9.0-10.0) | Aggiornamento immediato o rimozione della dipendenza | Entro 24h |
| Alta (7.0-8.9) | Aggiornamento prioritario | Entro 1 settimana |
| Media (4.0-6.9) | Aggiornamento pianificato | Entro 1 mese |
| Bassa (0.1-3.9) | Monitoraggio | Prossimo ciclo di aggiornamento |

**Quando l'aggiornamento non e possibile:**

- Verificare se esiste un workaround documentato.
- Valutare se la vulnerabilita e raggiungibile nel contesto del progetto
  (non tutte le CVE impattano tutti gli usi).
- Documentare la vulnerabilita accettata in `docs/tech-specs.md` con la motivazione.
- STOP: segnalare all'utente per decisione.

---

### 9. Dipendenze Interne e Monorepo

Per progetti multi-servizio o monorepo, le dipendenze condivise tra i servizi
richiedono attenzione aggiuntiva.

#### 9.1 Regole per dipendenze condivise

- Il codice condiviso deve vivere in un pacchetto interno dedicato
  (es. `packages/shared/`, `libs/common/`).
- Il pacchetto condiviso deve avere la propria versione (SemVer).
- I servizi che consumano il pacchetto condiviso devono referenziarlo
  con versione esplicita (anche se e nello stesso repo).
- Le modifiche al pacchetto condiviso che sono breaking changes
  richiedono aggiornamento coordinato di tutti i consumatori.

#### 9.2 Workspace manager raccomandati

| Ecosistema | Strumento | Configurazione |
|------------|-----------|----------------|
| Node.js | npm workspaces / pnpm workspaces / Turborepo | `workspaces` in package.json |
| Python | uv workspaces / poetry (con path dependencies) | `pyproject.toml` |
| .NET | Solution files (`.sln`) con ProjectReference | `.sln` + `.csproj` |
| Go | Go workspaces (`go.work`) | `go.work` |
| Dart | Melos | `melos.yaml` |

---

### 10. Checklist Pre-Commit per le Dipendenze

Prima di committare modifiche che coinvolgono dipendenze, l'agent verifica:

```
CHECKLIST DIPENDENZE PRE-COMMIT
    |
    +-- [ ] La dipendenza e necessaria (non implementabile con lo stack base)
    +-- [ ] La versione e pinnata esplicitamente
    +-- [ ] Il lock file e aggiornato e committato
    +-- [ ] docs/tech-specs.md e aggiornato con la nuova dipendenza
    +-- [ ] Nessuna CVE critica o alta aperta
    +-- [ ] La licenza e compatibile con il progetto
    +-- [ ] La dipendenza e nel gruppo corretto (production vs development)
    +-- [ ] I test esistenti passano dopo l'aggiunta/aggiornamento
    +-- [ ] I vincoli di compatibilita sono documentati (se scoperti)
    +-- [ ] Il commit message segue il formato: deps(<scope>): <azione> <pacchetto>
```

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| `docs/tech-specs.md` aggiornato | File Markdown | `docs/tech-specs.md` |
| Lock file aggiornato | File specifico del package manager | Root del progetto |
| Configurazione Dependabot/Renovate | YAML/JSON | `.github/dependabot.yml` o `renovate.json` |
| ADR per scelte significative (opzionale) | File Markdown | `docs/adr/` |
| Report audit sicurezza (se richiesto) | Output CLI o Markdown | Console / CI log |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| Conflitto di versione all'installazione | Due dipendenze richiedono versioni incompatibili di una transitiva | Applicare la strategia di risoluzione conflitti (Sezione 7). Se non risolvibile, segnalare all'utente |
| `npm audit` / `pip-audit` segnala vulnerabilita | Dipendenza con CVE nota | Classificare per severita e applicare la risposta appropriata (Sezione 8.3) |
| Lock file con conflitti dopo merge | Merge di branch con modifiche diverse alle dipendenze | Eliminare il lock file, eseguire install pulito, verificare che le versioni siano corrette, committare il nuovo lock file |
| Dipendenza non trovata / versione non esistente | Pacchetto rimosso dal registry o versione ritirata | Cercare nel changelog del registry; se rimossa, trovare alternativa e aggiornare docs/tech-specs.md |
| Incompatibilita runtime dopo aggiornamento | Breaking change non documentata o non rilevata | Rollback alla versione precedente; leggere il changelog completo; aprire issue se la breaking change non era documentata |
| Dipendenza orfana trovata durante audit | Dipendenza installata ma non piu usata nel codice | Eseguire la procedura di rimozione (Sezione 6.2) |
| `docs/tech-specs.md` non allineato con il lock file | Dipendenza aggiunta/rimossa senza aggiornare la documentazione | Riconciliare: verificare il lock file, aggiornare docs/tech-specs.md per riflettere la realta |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
