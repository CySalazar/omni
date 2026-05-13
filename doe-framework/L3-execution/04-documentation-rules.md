# Documentation Rules — Regole di Documentazione

## Metadata

- **ID:** DIR-013
- **Versione:** 1.0.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** DIR-010 (Code Standards)
- **Tipo di progetto:** universale

---

## Obiettivo

Definire regole chiare e azionabili su cosa documentare, quando documentarlo e in quale
formato, garantendo che ogni progetto sia comprensibile, manutenibile e utilizzabile
da chiunque — incluso chi non ha partecipato allo sviluppo.

---

## Pre-condizioni

- Il progetto ha una specifica tecnica approvata (`docs/tech-specs.md` o `project-spec.md`).
- Il linguaggio e il framework sono stati selezionati e documentati.
- L'agent ha familiarita con gli standard di codice del progetto (`01-code-standards.md`).

---

## Input

| Input | Tipo | Descrizione |
|-------|------|-------------|
| Tipo di progetto | stringa | Webapp, API, bot, CLI, pipeline, libreria, ecc. |
| Stack tecnologico | `docs/tech-specs.md` | Linguaggio, framework, dipendenze, versioni |
| Architettura | `docs/architecture.md` | Pattern architetturale, componenti, diagrammi |
| Specifiche di deploy | testo (opzionale) | Ambienti, requisiti infrastrutturali |

---

## Procedura

### 1. Documenti Obbligatori per Ogni Progetto

Ogni progetto prodotto con il framework D.O.E. deve avere i seguenti documenti.
L'agent li crea durante la fase di setup iniziale e li mantiene aggiornati durante
tutto il ciclo di vita del progetto.

#### 1.1 Matrice dei documenti

| Documento | Posizione | Scopo | Obbligatorio |
|-----------|-----------|-------|:------------:|
| `README.md` | root | Entry point per chiunque si avvicini al progetto: descrizione, setup, requisiti, quick start | Si |
| `CHANGELOG.md` | root | Registro cronologico delle modifiche per versione, leggibile da utenti e sviluppatori | Si |
| `docs/architecture.md` | docs/ | Architettura del sistema: pattern, componenti, diagrammi, decisioni strutturali | Si |
| `docs/tech-specs.md` | docs/ | Specifiche tecniche: linguaggi, librerie, versioni, vincoli di compatibilita | Si |
| `docs/deployment.md` | docs/ | Come fare deploy in ogni ambiente: requisiti, step, configurazione, segreti necessari | Si |
| `docs/api.md` | docs/ | Documentazione API: endpoint, request/response, autenticazione, errori | Se il progetto espone API |
| `docs/adr/` | docs/adr/ | Architecture Decision Records: una decisione architetturale per file | Se ci sono decisioni non ovvie |

#### 1.2 README.md — Struttura obbligatoria

Il README.md e la porta d'ingresso del progetto. Un nuovo sviluppatore deve poter
partire da qui e avere il progetto funzionante in meno di 15 minuti.

```markdown
# [Nome Progetto]

[Una frase che descrive cosa fa il progetto e perche esiste.]

## Indice

- [Requisiti](#requisiti)
- [Setup Rapido](#setup-rapido)
- [Struttura del Progetto](#struttura-del-progetto)
- [Sviluppo](#sviluppo)
- [Testing](#testing)
- [Deploy](#deploy)
- [Documentazione](#documentazione)
- [Contribuire](#contribuire)
- [Licenza](#licenza)

## Requisiti

[Lista dei prerequisiti con versioni minime.]

- [Linguaggio] >= [versione]
- [Database] >= [versione] (opzionale, se applicabile)
- [Altro tool] >= [versione]

## Setup Rapido

[Comandi esatti per avere il progetto funzionante in locale.
Devono essere copia-incolla-esegui, senza ambiguita.]

## Struttura del Progetto

[Overview della struttura directory con una riga di spiegazione per ogni cartella principale.]

## Sviluppo

[Comandi per eseguire il progetto in modalita sviluppo, hot-reload, ecc.]

## Testing

[Come eseguire i test: unit, integration, e2e. Comandi esatti.]

## Deploy

[Panoramica rapida. Per i dettagli rimandare a docs/deployment.md.]

## Documentazione

[Link alla documentazione completa in docs/.]

## Contribuire

[Regole per contribuire: branching strategy, commit conventions, PR process.]

## Licenza

[Tipo di licenza e link al file LICENSE.]
```

**Regole per il README.md:**

- Deve essere sempre funzionante: se il setup cambia, il README deve essere aggiornato
  immediatamente.
- I comandi devono essere testati e funzionanti. Mai inserire comandi "di esempio" che
  non funzionano.
- Deve linkare alla documentazione piu dettagliata in `docs/` per evitare ridondanza.
- Non deve contenere dettagli implementativi interni — quelli vanno in `docs/architecture.md`.

#### 1.3 CHANGELOG.md — Struttura e regole

Il CHANGELOG segue il formato [Keep a Changelog](https://keepachangelog.com/)
con versionamento [Semantic Versioning](https://semver.org/).

Il template completo e disponibile in `templates/changelog-template.md`.

**Categorie ammesse per ogni versione:**

| Categoria | Quando usarla |
|-----------|---------------|
| `Added` | Nuove funzionalita |
| `Changed` | Modifiche a funzionalita esistenti |
| `Deprecated` | Funzionalita che verranno rimosse in futuro |
| `Removed` | Funzionalita rimosse |
| `Fixed` | Bug fix |
| `Security` | Correzioni di vulnerabilita |

**Regole operative:**

- Il CHANGELOG viene aggiornato ad ogni modifica rilasciabile, non solo a fine versione.
- Le entry nella sezione `[Unreleased]` si accumulano durante lo sviluppo.
- Al momento del rilascio, `[Unreleased]` diventa `[X.Y.Z] - YYYY-MM-DD`.
- Ogni entry deve essere comprensibile dall'utente finale (non gergo tecnico interno).
- Se una modifica ha un impatto di breaking change, deve essere esplicitamente segnalata.

#### 1.4 docs/architecture.md — Struttura

```markdown
# Architettura — [Nome Progetto]

## Panoramica

[Descrizione ad alto livello dell'architettura: pattern scelto, motivazione,
componenti principali e come interagiscono.]

## Diagramma del Sistema

[Diagramma Mermaid che mostra i componenti e le loro interazioni.]

## Componenti

### [Nome Componente 1]

- **Responsabilita:** [Cosa fa]
- **Tecnologia:** [Stack]
- **Interfacce:** [API/eventi che espone e consuma]
- **Directory:** [Dove si trova nel codice]

### [Nome Componente N]
[...]

## Flussi Principali

### [Nome Flusso 1 — es. "Autenticazione Utente"]

[Diagramma di sequenza Mermaid + descrizione testuale del flusso.]

## Decisioni Architetturali

[Riepilogo delle decisioni principali. Per il dettaglio completo, rimandare
ai singoli ADR in docs/adr/.]

| Data | Decisione | ADR |
|------|-----------|-----|
| YYYY-MM-DD | [Breve descrizione] | [Link a docs/adr/NNN-titolo.md] |

## Vincoli e Limitazioni Note

[Vincoli tecnici, di performance, di scalabilita noti e accettati.]
```

#### 1.5 docs/tech-specs.md — Struttura

Questo e il documento di riferimento tecnico del progetto. L'agent lo consulta
PRIMA di scegliere qualsiasi libreria o tool.

```markdown
# Specifiche Tecniche — [Nome Progetto]

> **Ultimo aggiornamento:** YYYY-MM-DD

## Runtime

| Componente | Tecnologia | Versione | Note |
|------------|-----------|----------|------|
| Linguaggio | [es. Python] | [es. 3.12] | [Note] |
| Framework | [es. FastAPI] | [es. 0.115.x] | [Note] |
| Database | [es. PostgreSQL] | [es. 16] | [Note] |
| Runtime | [es. Node.js] | [es. 20 LTS] | [Note] |

## Dipendenze Principali

| Pacchetto | Versione | Scopo | Note di Compatibilita |
|-----------|----------|-------|----------------------|
| [Nome] | [X.Y.Z] | [A cosa serve] | [Vincoli noti] |

## Vincoli di Compatibilita

[Elenco dei vincoli noti tra le dipendenze che richiedono attenzione.]

| Dipendenza A | Dipendenza B | Vincolo | Impatto |
|-------------|-------------|---------|---------|
| [Pacchetto] | [Pacchetto] | [Descrizione] | [Cosa succede se violato] |

## Tool di Sviluppo

| Tool | Versione | Scopo | Configurazione |
|------|----------|-------|----------------|
| [Linter] | [X.Y] | Analisi statica | [File di config] |
| [Formatter] | [X.Y] | Formattazione | [File di config] |
| [Test runner] | [X.Y] | Esecuzione test | [File di config] |

## Storico Modifiche

| Data | Modifica | Motivazione |
|------|----------|-------------|
| YYYY-MM-DD | [Cosa e cambiato] | [Perche] |
```

**Regola chiave:** L'agent DEVE consultare `docs/tech-specs.md` prima di aggiungere
qualsiasi nuova dipendenza al progetto, per verificare compatibilita e coerenza.
Ogni nuova dipendenza aggiunta deve essere immediatamente registrata in questo file.

#### 1.6 docs/deployment.md — Struttura

```markdown
# Deployment Guide — [Nome Progetto]

## Ambienti

| Ambiente | Piattaforma | URL | Note |
|----------|------------|-----|------|
| Development | Locale / Docker | localhost:XXXX | [Note] |
| Staging | [Provider] | [URL] | [Note] |
| Production | [Provider] | [URL] | [Note] |

## Prerequisiti

[Cosa deve essere installato e configurato prima del deploy.]

## Variabili d'Ambiente

[Lista completa delle variabili — SENZA valori, con descrizione.]

| Variabile | Descrizione | Obbligatoria | Esempio |
|-----------|-------------|:------------:|---------|
| [NOME] | [A cosa serve] | Si/No | [Valore d'esempio] |

## Procedura di Deploy

### Development (Locale)

[Step esatti per deploy locale.]

### Staging

[Step esatti per deploy in staging.]

### Production

[Step esatti per deploy in produzione.]

## Rollback

[Procedura di rollback per ogni ambiente.]

## Monitoraggio Post-Deploy

[Cosa verificare dopo il deploy: healthcheck, log, metriche.]

## Troubleshooting

| Problema | Causa Probabile | Soluzione |
|----------|----------------|-----------|
| [Problema] | [Causa] | [Soluzione] |
```

#### 1.7 docs/api.md — Struttura (se applicabile)

Se il progetto espone API (REST, GraphQL, gRPC), la documentazione API e obbligatoria.

**Approccio preferito: generazione automatica.**

| Linguaggio/Framework | Tool di generazione | Formato |
|---------------------|--------------------|---------|
| FastAPI (Python) | Swagger/ReDoc integrato | OpenAPI 3.x |
| Express/NestJS (Node) | `swagger-jsdoc` + `swagger-ui-express` | OpenAPI 3.x |
| ASP.NET (C#) | Swashbuckle / NSwag | OpenAPI 3.x |
| Go (Gin/Echo) | `swaggo/swag` | OpenAPI 3.x |

**Se la generazione automatica non e possibile**, il file `docs/api.md` deve documentare
manualmente ogni endpoint con:

```markdown
### [METHOD] /api/v1/resource

**Descrizione:** [Cosa fa questo endpoint]

**Autenticazione:** [Tipo: Bearer token / API Key / Nessuna]

**Request:**

| Parametro | Posizione | Tipo | Obbligatorio | Descrizione |
|-----------|-----------|------|:------------:|-------------|
| [nome] | [path/query/body] | [tipo] | Si/No | [Descrizione] |

**Response 200:**
[Schema JSON di esempio]

**Errori:**

| Status | Codice | Descrizione |
|--------|--------|-------------|
| 400 | VALIDATION_ERROR | [Quando si verifica] |
| 401 | UNAUTHORIZED | [Quando si verifica] |
| 404 | NOT_FOUND | [Quando si verifica] |
```

#### 1.8 docs/adr/ — Architecture Decision Records

Per ogni decisione architetturale non ovvia, l'agent crea un file ADR.
Il template e disponibile in `templates/adr-template.md`.

**Naming convention:** `docs/adr/NNN-titolo-decisione.md` (es. `001-scelta-database.md`).

**Quando creare un ADR:**

- Scelta tra due o piu tecnologie/approcci con trade-off significativi.
- Deviazione da un pattern standard del framework.
- Decisione che vincola le scelte future (es. scelta del database, pattern di autenticazione).
- Rinuncia consapevole a una best practice (con motivazione).

**Quando NON creare un ADR:**

- Scelte ovvie o senza alternative realistiche.
- Decisioni reversibili a basso costo.
- Scelte imposte dall'utente come vincolo non negoziabile (documentarle in `project-spec.md`).

---

### 2. Regole di Aggiornamento della Documentazione

#### 2.1 Principio fondamentale

**La documentazione viene aggiornata CONTESTUALMENTE al codice, non dopo.**

Questo significa che quando l'agent modifica il codice, nella stessa sessione di lavoro
aggiorna anche la documentazione impattata. La documentazione non e un task separato:
e parte integrante del task corrente.

#### 2.2 Matrice di aggiornamento

| Tipo di modifica nel codice | Documenti da aggiornare |
|----------------------------|------------------------|
| Nuovo endpoint API | `docs/api.md`, `README.md` (se cambia il quick start) |
| Nuova dipendenza aggiunta | `docs/tech-specs.md`, `CHANGELOG.md` |
| Dipendenza aggiornata | `docs/tech-specs.md`, `CHANGELOG.md` |
| Modifica architetturale | `docs/architecture.md`, ADR se necessario, `CHANGELOG.md` |
| Nuovo ambiente di deploy | `docs/deployment.md` |
| Nuova variabile d'ambiente | `docs/deployment.md`, `.env.example` |
| Modifica al setup del progetto | `README.md` (sezione Setup Rapido) |
| Nuova funzionalita utente | `CHANGELOG.md`, `README.md` (se cambia l'uso) |
| Bug fix | `CHANGELOG.md` |
| Breaking change | `CHANGELOG.md` (con segnalazione esplicita), `docs/api.md` (se API) |
| Nuova decisione architetturale | `docs/architecture.md`, `docs/adr/NNN-*.md` |
| Modifica alla pipeline CI/CD | `docs/deployment.md` |
| Rimozione di funzionalita | `CHANGELOG.md`, `docs/api.md` (se API), `README.md` (se impatta l'uso) |

#### 2.3 Verifica di allineamento

Alla fine di ogni sessione di lavoro significativa, l'agent esegue una verifica
di allineamento tra codice e documentazione:

```
CHECKLIST ALLINEAMENTO DOCUMENTAZIONE
    |
    +-- [ ] README.md: i comandi di setup funzionano con lo stato attuale del codice?
    +-- [ ] docs/tech-specs.md: tutte le dipendenze nel lock file sono documentate?
    +-- [ ] docs/architecture.md: i diagrammi riflettono i componenti attuali?
    +-- [ ] docs/api.md: tutti gli endpoint attivi sono documentati?
    +-- [ ] docs/deployment.md: le variabili d'ambiente sono allineate con .env.example?
    +-- [ ] CHANGELOG.md: le modifiche di questa sessione sono registrate?
```

Se la checklist rivela disallineamenti, l'agent li corregge prima di dichiarare
il task completato.

---

### 3. Commenti nel Codice — Integrazione con Code Standards

Le regole per i commenti inline sono definite in dettaglio in `01-code-standards.md`
(sezioni 3.1-3.5). Questo documento le estende con linee guida specifiche per la
documentazione strutturata.

#### 3.1 Riepilogo dei principi (da `01-code-standards.md`)

- I commenti spiegano il **perche**, non il **cosa**.
- Le funzioni/metodi pubblici hanno sempre docstring/JSDoc/XML docs.
- I marker `TODO`, `FIXME`, `HACK`, `NOTE`, `PERF`, `SECURITY` sono standardizzati.
- Il codice commentato (dead code) va eliminato, non lasciato nel sorgente.

#### 3.2 Regole aggiuntive per la documentazione di progetto

**Header di file — regola di sincronizzazione:**

L'header comment di ogni file (definito in `01-code-standards.md`, sezione 2.1) deve
essere aggiornato quando:
- Cambiano le dipendenze principali del file.
- Cambia significativamente lo scopo del file.
- Il file viene spostato o rinominato.

**Documentazione di moduli/package:**

Ogni modulo o package deve avere un file descrittivo (`__init__.py` con docstring
per Python, `index.ts` con JSDoc per TypeScript, `package-info.java` per Java) che
spiega:
- Lo scopo del modulo nel contesto del progetto.
- Le classi/funzioni principali esportate.
- Eventuali dipendenze o prerequisiti.

**Documentazione di configurazione:**

Ogni file di configurazione non ovvio (es. configurazione webpack, nginx config,
docker-compose, CI pipeline) deve avere commenti che spiegano:
- Perche quella configurazione esiste.
- Quali valori sono critici e non devono essere cambiati senza comprensione.
- Link alla documentazione ufficiale per le opzioni usate.

---

### 4. Strumenti di Generazione Automatica della Documentazione

Dove possibile, l'agent deve configurare strumenti che generano documentazione
automaticamente dal codice, riducendo il rischio di disallineamento.

#### 4.1 Strumenti raccomandati

| Linguaggio | Tool | Tipo di documentazione | Note |
|------------|------|----------------------|------|
| Python | `mkdocs` + `mkdocstrings` | Documentazione API da docstring | Richiede docstring Google/NumPy style |
| Python | `sphinx` + `autodoc` | Documentazione completa | Piu potente ma piu complesso |
| TypeScript | `typedoc` | Documentazione API da JSDoc/tipi | Sfrutta i tipi TypeScript |
| C# | `docfx` | Documentazione da XML comments | Integrazione nativa con .NET |
| Go | `godoc` (stdlib) | Documentazione da commenti | Segue le convenzioni Go |
| Generale | `openapi-generator` | Client SDK da spec OpenAPI | Per API documentate con OpenAPI |

#### 4.2 Regole per la documentazione generata

- La documentazione generata NON sostituisce quella scritta manualmente. La documentazione
  manuale fornisce contesto, motivazioni e tutorial. Quella generata fornisce riferimento
  tecnico preciso.
- I file generati automaticamente devono essere esclusi dal version control (aggiunti
  a `.gitignore`) oppure rigenerati in CI/CD.
- La configurazione del tool di generazione deve essere committata nel progetto.

---

### 5. Documentazione per Tipo di Progetto

Oltre ai documenti obbligatori universali, alcuni tipi di progetto richiedono
documentazione aggiuntiva.

#### 5.1 Progetti con UI (WebApp, App Desktop, App Mobile)

| Documento aggiuntivo | Posizione | Contenuto |
|---------------------|-----------|-----------|
| `docs/ui-guide.md` | docs/ | Componenti UI, design system usato, pattern di navigazione |
| `docs/accessibility.md` | docs/ | Standard di accessibilita applicati (WCAG level), test effettuati |

#### 5.2 Librerie / Package pubblicabili

| Documento aggiuntivo | Posizione | Contenuto |
|---------------------|-----------|-----------|
| `docs/getting-started.md` | docs/ | Tutorial per l'utente della libreria: installazione, primo uso, esempi |
| `docs/migration-guide.md` | docs/ | Guida alla migrazione tra versioni major (se applicabile) |
| `CONTRIBUTING.md` | root | Regole per contribuire: setup dev, testing, PR process |

#### 5.3 Data Pipeline / ETL

| Documento aggiuntivo | Posizione | Contenuto |
|---------------------|-----------|-----------|
| `docs/data-dictionary.md` | docs/ | Schema dei dati: sorgenti, trasformazioni, destinazioni, tipi |
| `docs/pipeline-diagram.md` | docs/ | Diagramma del flusso dati end-to-end |

#### 5.4 Microservizi / Multi-servizio

| Documento aggiuntivo | Posizione | Contenuto |
|---------------------|-----------|-----------|
| `docs/service-catalog.md` | docs/ | Elenco dei servizi: responsabilita, owner, endpoint, dipendenze |
| `docs/contracts.md` | docs/ | Contratti API tra servizi: chi produce, chi consuma, schema |

---

### 6. Qualita della Documentazione

#### 6.1 Principi di buona documentazione

- **Accuratezza:** La documentazione deve riflettere lo stato attuale del codice.
  Documentazione obsoleta e peggio di nessuna documentazione (crea falsa confidenza).
- **Chiarezza:** Usare frasi brevi, vocabolario preciso. Evitare gergo non definito.
  Se un termine tecnico e necessario, definirlo alla prima occorrenza.
- **Completezza:** Ogni documento deve essere auto-sufficiente per il suo scopo.
  Un lettore non deve dover indovinare informazioni mancanti.
- **Concisione:** Dire solo cio che serve. Non ripetere informazioni gia presenti
  in altri documenti — linkare invece.
- **Azionabilita:** I comandi devono essere copia-incolla. Le procedure devono essere
  passo-passo. Le configurazioni devono includere esempi concreti.

#### 6.2 Checklist di qualita per ogni documento

```
CHECKLIST QUALITA DOCUMENTO
    |
    +-- [ ] Il documento ha uno scopo chiaro dichiarato?
    +-- [ ] Le informazioni sono accurate rispetto allo stato attuale del codice?
    +-- [ ] I comandi e gli esempi funzionano se eseguiti?
    +-- [ ] Non ci sono riferimenti a documenti o file inesistenti (broken links)?
    +-- [ ] Il linguaggio e chiaro e non ambiguo?
    +-- [ ] Il documento non duplica informazioni presenti altrove (usa link)?
    +-- [ ] I diagrammi (se presenti) sono in formato Mermaid e renderizzano correttamente?
    +-- [ ] La data di ultimo aggiornamento e corretta?
```

#### 6.3 Lingua della documentazione

- Il codice, i commenti nel codice e la documentazione tecnica (docs/) sono in **inglese**.
- Eventuali documenti destinati a utenti finali non tecnici seguono la lingua
  indicata dall'utente.
- Se il progetto e multilingue, la lingua di default della documentazione tecnica
  rimane l'inglese.

---

## Output

| Output | Formato | Destinazione |
|--------|---------|-------------|
| README.md | Markdown | Root del progetto |
| CHANGELOG.md | Markdown (Keep a Changelog) | Root del progetto |
| docs/architecture.md | Markdown con diagrammi Mermaid | docs/ |
| docs/tech-specs.md | Markdown con tabelle | docs/ |
| docs/deployment.md | Markdown | docs/ |
| docs/api.md (se applicabile) | Markdown o generato (OpenAPI) | docs/ |
| docs/adr/*.md (se applicabile) | Markdown (ADR template) | docs/adr/ |
| Documentazione aggiuntiva per tipo | Markdown | docs/ |

---

## Gestione Errori

| Errore | Causa probabile | Risoluzione |
|--------|----------------|-------------|
| README.md non aggiornato dopo modifica al setup | L'agent non ha eseguito la checklist di allineamento | Eseguire la checklist (sezione 2.3), correggere il README |
| docs/tech-specs.md non contiene una dipendenza presente nel lock file | Dipendenza aggiunta senza aggiornare la documentazione | Aggiungere la dipendenza a tech-specs.md con versione e note |
| docs/api.md non riflette gli endpoint attuali | Endpoint aggiunto/modificato senza aggiornare la documentazione | Rigenerare la documentazione API o aggiornare manualmente |
| Diagrammi Mermaid non renderizzano | Sintassi Mermaid errata | Verificare la sintassi su mermaid.live, correggere |
| Link interni rotti tra documenti | File rinominato o spostato senza aggiornare i riferimenti | Cercare tutti i riferimenti al vecchio path e aggiornarli |
| CHANGELOG.md non aggiornato per una release | Entry non aggiunte durante lo sviluppo | Ricostruire le entry dai commit (usare `git log`) e aggiornare |
| Documentazione in lingua sbagliata | Non verificate le preferenze dell'utente | Chiedere all'utente la lingua preferita per i documenti utente |

---

## Lezioni Apprese

> Questa sezione viene aggiornata dall'agent man mano che vengono scoperti pattern,
> eccezioni e best practice specifiche durante l'uso del framework.

- *(nessuna lezione registrata — documento appena creato)*
