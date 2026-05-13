# Project Intake Protocol

## Metadata

- **ID:** DIR-001
- **Versione:** 1.0
- **Ultimo aggiornamento:** 2026-03-24
- **Dipende da:** Nessuna (questa e la prima direttiva da eseguire)
- **Tipo di progetto:** Universale

## Obiettivo

Garantire che ogni progetto inizi con una raccolta requisiti strutturata, un'analisi
approfondita e una specifica tecnica approvata dall'utente, **prima** che venga scritta
qualsiasi riga di codice.

## Pre-condizioni

- L'utente ha espresso una richiesta di progetto (anche vaga o parziale).
- L'agent ha accesso a questo file e al template `templates/project-spec.md`.

## Principio Chiave

> "Mai scrivere codice senza una specifica approvata.
> La specifica e il contratto tra intenzione e implementazione."

---

## Procedura

La procedura si articola in 4 fasi sequenziali obbligatorie. L'agent non puo
saltare nessuna fase ne procedere alla successiva senza aver completato la corrente.

---

### Fase 1 — Raccolta Requisiti Core

**Azione:** Porre all'utente le seguenti domande obbligatorie, indipendentemente
dal tipo di progetto. Queste domande formano il nucleo minimo di comprensione
necessario per qualsiasi decisione architetturale.

**Domande core obbligatorie:**

| # | Domanda | Scopo | Esempio di risposta attesa |
|---|---------|-------|---------------------------|
| 1 | Qual e l'obiettivo del progetto in una frase? | Definire lo scope | "Un bot Telegram che monitora i prezzi crypto e avvisa l'utente" |
| 2 | Chi sono gli utenti/consumatori finali? | Definire il target | "Trader retail italiani, 50-200 utenti iniziali" |
| 3 | Quali sono i vincoli non negoziabili? | Identificare constraint | "Budget zero per infrastruttura, solo free tier. Deadline 2 settimane" |
| 4 | Esistono sistemi esistenti con cui deve integrarsi? | Mappare integrazioni | "Si integra con Binance API e un Google Sheet esistente" |
| 5 | Qual e la scala prevista (utenti, dati, throughput)? | Dimensionare l'architettura | "Max 200 utenti, ~1000 check prezzi/ora" |
| 6 | Esistono preferenze o vincoli tecnologici? | Rispettare le scelte dell'utente | "Preferisco Python, il deploy deve essere su Railway" |

**Regole operative della Fase 1:**

- L'agent pone TUTTE le domande in un unico blocco, non una alla volta
  (per ridurre il ping-pong conversazionale).
- Se l'utente risponde in modo incompleto, l'agent chiede chiarimenti SOLO
  sulle domande senza risposta.
- Se l'utente dice "decidi tu" per un punto, l'agent documenta la sua scelta
  nella specifica con la motivazione e la segna come "decisione dell'agent — da confermare".
- Se la richiesta iniziale dell'utente e gia sufficientemente dettagliata da
  rispondere a tutte le domande, l'agent puo saltare le domande gia coperte
  e chiedere solo quelle rimanenti.

**Output della Fase 1:**
Un oggetto strutturato (mentale o documentato) con le 6 risposte che alimenta la Fase 2.

**Criterio di successo:**
Tutte e 6 le domande hanno una risposta (anche se alcune sono "decisione dell'agent").

---

### Fase 2 — Domande Dinamiche Contestuali

**Azione:** In base al tipo di progetto rilevato dalle risposte della Fase 1,
l'agent genera e pone domande aggiuntive specifiche per quel contesto.

**Catalogo delle domande contestuali per tipo di progetto:**

#### Web Application

| Domanda | Motivazione |
|---------|-------------|
| Autenticazione richiesta? Se si, quali provider (email/password, OAuth, SSO)? | Impatta architettura, dipendenze, sicurezza |
| Multi-tenant o single-tenant? | Impatta schema DB, isolamento dati, routing |
| Internazionalizzazione (i18n) necessaria? Quali lingue? | Impatta struttura UI, gestione contenuti |
| Progressive Web App (PWA) richiesta? | Impatta service worker, manifest, caching |
| Server-Side Rendering (SSR) o Single Page Application (SPA)? | Impatta framework, SEO, performance |
| Dashboard admin necessaria? | Impatta scope e complessita |
| Gestione pagamenti? Se si, quale provider? | Impatta sicurezza, compliance PCI-DSS |

#### API / Backend Service

| Domanda | Motivazione |
|---------|-------------|
| REST, GraphQL o gRPC? | Impatta framework, tooling, documentazione |
| Rate limiting necessario? Quali soglie? | Impatta architettura, dipendenze |
| Versioning dell'API richiesto? Strategia preferita (URL, header)? | Impatta routing, manutenibilita |
| Formati di risposta (JSON, XML, entrambi)? | Impatta serializzazione, content negotiation |
| Autenticazione: API key, JWT, OAuth2? | Impatta middleware, sicurezza |
| Webhook in uscita necessari? | Impatta infrastruttura, retry logic |
| Documentazione API automatica (OpenAPI/Swagger)? | Impatta dipendenze, workflow |

#### Bot (Telegram, Discord, Slack, ecc.)

| Domanda | Motivazione |
|---------|-------------|
| Su quale piattaforma? (Telegram, Discord, Slack, multi-piattaforma) | Impatta libreria, API, vincoli |
| Comandi previsti? Lista dei comandi principali | Impatta struttura handler |
| Webhooks o polling? | Impatta deploy, latenza, complessita |
| Persistenza dello stato conversazionale necessaria? | Impatta DB, complessita logica |
| Inline mode / callback query / menu interattivi? | Impatta complessita UI bot |
| Gestione media (immagini, file, audio)? | Impatta storage, bandwidth |

#### Data Pipeline / ETL

| Domanda | Motivazione |
|---------|-------------|
| Sorgenti dati? (API, DB, file, scraping) | Impatta connettori, dipendenze |
| Frequenza di esecuzione? (real-time, batch orario, giornaliero) | Impatta architettura, scheduling |
| Volume dati previsto? (righe/MB per esecuzione) | Impatta memoria, storage, batching |
| Trasformazioni necessarie? (pulizia, aggregazione, arricchimento) | Impatta complessita logica |
| Destinazione dati? (DB, file, API, data warehouse) | Impatta connettori output |
| Gestione errori e retry? (skip, retry, dead letter queue) | Impatta affidabilita |
| Monitoraggio e alerting su fallimenti? | Impatta infrastruttura |

#### DevOps / Infrastructure

| Domanda | Motivazione |
|---------|-------------|
| Cloud provider? (AWS, GCP, Azure, self-hosted) | Impatta tutti gli strumenti |
| Orchestratore? (Kubernetes, Docker Compose, ECS, serverless) | Impatta complessita, costi |
| Monitoring e observability? (Prometheus, Grafana, Datadog, CloudWatch) | Impatta dipendenze, costi |
| Alerting? (PagerDuty, Slack, email) | Impatta integrazioni |
| IaC preferito? (Terraform, Pulumi, CloudFormation, CDK) | Impatta toolchain |
| Ambienti? (dev, staging, production) | Impatta pipeline, costi |

#### CLI Tool / Script di Automazione

| Domanda | Motivazione |
|---------|-------------|
| Destinato a uso interno o distribuzione pubblica? | Impatta packaging, documentazione |
| Interattivo o batch? | Impatta UI, input handling |
| Quali sistemi operativi deve supportare? | Impatta dipendenze, path handling |
| Configurazione tramite file, variabili d'ambiente o flag CLI? | Impatta parsing, struttura |
| Output atteso? (stdout, file, API call) | Impatta formattazione, destinazione |

#### Mobile App

| Domanda | Motivazione |
|---------|-------------|
| Piattaforme target? (iOS, Android, entrambe) | Impatta framework, testing |
| Nativo, cross-platform (Flutter, React Native) o ibrido? | Impatta stack completo |
| Offline-first necessario? | Impatta storage locale, sync |
| Push notification? | Impatta servizi backend, permessi |
| Accesso a sensori/hardware? (camera, GPS, bluetooth) | Impatta permessi, dipendenze native |
| Distribuzione? (App Store, sideload, enterprise) | Impatta build, signing, compliance |

**Regole operative della Fase 2:**

- L'agent seleziona SOLO le domande pertinenti al tipo di progetto rilevato.
- Se il progetto copre piu categorie (es. WebApp + API), l'agent combina
  le domande di entrambe le categorie, eliminando i duplicati.
- L'agent puo aggiungere domande non presenti nel catalogo se il contesto
  specifico lo richiede.
- Anche qui, le domande vengono poste in un unico blocco.
- L'agent deve spiegare brevemente PERCHE pone ogni domanda (la colonna
  "Motivazione" serve come guida, non va mostrata verbatim all'utente).

**Output della Fase 2:**
L'insieme completo di risposte (Fase 1 + Fase 2) che alimenta la generazione
della specifica tecnica.

**Criterio di successo:**
Tutte le domande contestuali hanno una risposta o una decisione documentata dell'agent.

---

### Fase 3 — Generazione della Specifica Tecnica

**Azione:** L'agent produce un documento `project-spec.md` utilizzando il template
in `templates/project-spec.md`, compilandolo con tutte le informazioni raccolte
nelle Fasi 1 e 2.

**Il documento deve includere:**

1. **Riepilogo del progetto** — obiettivo, target, vincoli
2. **Architettura proposta** — con diagramma testuale in formato Mermaid
3. **Stack tecnologico selezionato** — con motivazione per ogni scelta
4. **Scomposizione in moduli/componenti** — con descrizione e responsabilita di ciascuno
5. **Dipendenze esterne identificate** — con versione, licenza e note di compatibilita
6. **Rischi e mitigazioni** — rischi tecnici, di scope, di timeline
7. **Proposta di struttura directory** — albero completo della cartella di progetto
8. **Piano di testing** — quali livelli di test per quali componenti
9. **Piano di deploy** — come e dove viene deployato

**Regole operative della Fase 3:**

- L'agent DEVE consultare `L1-directives/02-architecture-patterns.md` (quando disponibile)
  per scegliere il pattern architetturale appropriato.
- L'agent DEVE consultare `L3-execution/06-dependency-management.md` (quando disponibile)
  per la selezione delle dipendenze.
- Se il progetto prevede UI, l'agent verifica l'esistenza di un file
  `brand-guidelines.md` e lo integra nella specifica.
- Ogni scelta tecnologica deve avere una motivazione esplicita.
  "E il piu popolare" non e una motivazione valida.
  "Supporta async nativo, ha type hints, e la community e attiva con 50k+ stars
  e release mensili" e una motivazione valida.
- Il diagramma Mermaid deve mostrare almeno: i componenti principali, le loro
  interazioni, i flussi di dati e le dipendenze esterne.

**Output della Fase 3:**
File `docs/project-spec.md` nella root del progetto (o nella directory specificata
dall'utente), compilato secondo il template.

**Criterio di successo:**
Tutte le sezioni del template sono compilate. Nessuna sezione contiene placeholder
o "TBD" senza motivazione.

---

### Fase 4 — Validazione con l'Utente

**Azione:** L'agent presenta la specifica tecnica all'utente e richiede
un'approvazione esplicita prima di procedere con lo sviluppo.

**Formato della presentazione:**

L'agent deve presentare la specifica in modo strutturato, evidenziando:

1. **Le scelte chiave** — le 3-5 decisioni piu importanti con la loro motivazione
2. **I punti aperti** — domande o decisioni che necessitano di input dell'utente
3. **I rischi identificati** — con le mitigazioni proposte
4. **La stima di complessita** — size complessiva del progetto (S/M/L/XL)
5. **La richiesta esplicita di approvazione:**
   - "Approvi questa specifica per procedere con lo sviluppo?"
   - "Ci sono modifiche che vuoi apportare?"
   - "Ci sono aspetti che vuoi approfondire?"

**Regole operative della Fase 4:**

- L'agent NON inizia a scrivere codice finche non riceve un'approvazione
  esplicita (un "ok", "procedi", "approvato", o equivalente).
- Se l'utente richiede modifiche, l'agent aggiorna la specifica e la
  ripresenta per una nuova approvazione (loop fino ad approvazione).
- Se l'utente approva con riserve ("ok ma cambia X"), l'agent aggiorna
  la specifica incorporando le riserve e procede (senza richiedere
  un'ulteriore approvazione, a meno che il cambiamento non sia sostanziale).
- Le modifiche alla specifica vengono tracciate con data e motivazione.

**Output della Fase 4:**
Specifica tecnica con status "APPROVATA" e data di approvazione.

**Criterio di successo:**
L'utente ha dato approvazione esplicita. La specifica riflette fedelmente
le intenzioni dell'utente.

---

## Gestione Errori

| Errore | Causa Probabile | Risoluzione |
|--------|-----------------|-------------|
| L'utente non risponde a tutte le domande core | Domande troppo tecniche o utente con fretta | Riformulare in modo piu semplice. Per le domande senza risposta, proporre un default motivato e chiedere conferma |
| Il tipo di progetto non rientra in nessuna categoria nota | Progetto ibrido o innovativo | Combinare le domande di piu categorie. Aggiungere domande specifiche. Documentare il nuovo tipo per aggiornare questa direttiva |
| L'utente cambia idea sui requisiti durante la Fase 3 | Naturale evoluzione della comprensione | Tornare alla fase pertinente, aggiornare le risposte e rigenerare la specifica |
| L'utente vuole saltare l'intake e andare diretto al codice | Impazienza o progetto percepito come semplice | Spiegare brevemente il valore dell'intake. Se il progetto e davvero banale (< 100 righe, singolo file, nessuna dipendenza esterna), proporre un intake "light" con sole domande 1, 3 e 6 |
| La specifica e troppo lunga/complessa per l'utente | Over-engineering per il contesto | Offrire una versione sintetica (executive summary) con link alla versione completa |

---

## Variante: Intake Light

Per progetti banali (script singoli, utility one-shot, fix rapidi), l'agent puo
attivare una versione ridotta dell'intake:

**Criteri di attivazione (TUTTI devono essere veri):**
- Il progetto e stimato in meno di 100 righe di codice
- Non ha dipendenze esterne significative
- Non ha requisiti di sicurezza critici
- Non deve integrarsi con sistemi esistenti complessi

**Domande dell'intake light:**
1. Qual e l'obiettivo? (Domanda core #1)
2. Ci sono vincoli? (Domanda core #3)
3. Preferenze tecnologiche? (Domanda core #6)

L'agent produce comunque una mini-specifica (3-5 righe) e chiede conferma
prima di procedere.

---

## Lezioni Apprese

*Questa sezione viene aggiornata dall'agent con le scoperte fatte durante l'uso
di questa direttiva. Ogni entry include data, contesto e lezione.*

| Data | Contesto | Lezione |
|------|----------|---------|
| — | — | Nessuna lezione registrata ancora. Questa sezione cresce con l'uso. |

---

## Riferimenti

- Template specifica tecnica: [`templates/project-spec.md`](../templates/project-spec.md)
- Pattern architetturali: [`L1-directives/02-architecture-patterns.md`](02-architecture-patterns.md)
- Interaction Protocol: [`L2-orchestration/04-interaction-protocol.md`](../L2-orchestration/04-interaction-protocol.md)
- Decision Engine: [`L2-orchestration/01-decision-engine.md`](../L2-orchestration/01-decision-engine.md)
