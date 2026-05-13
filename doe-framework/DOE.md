# D.O.E. Framework — Direttiva, Orchestrazione, Esecuzione

> **Versione:** 1.0.0
> **Ultimo aggiornamento:** 2026-03-24
> **Compatibilita:** Agnostico — qualsiasi coding agent LLM-based

---

## Filosofia

Gli LLM sono probabilistici. La logica di business e richiede coerenza e determinismo.
Il framework D.O.E. risolve questa tensione separando **cosa fare** (Direttiva),
**come decidere** (Orchestrazione) e **come eseguire** (Esecuzione) in tre livelli distinti.

Il principio fondante e semplice: **spingere la complessita in codice deterministico
riduce il margine di errore probabilistico**. L'agent si concentra solo sul decision-making;
il lavoro concreto viene delegato a script testabili e ripetibili.

---

## Architettura a 3 Livelli

### Livello 1 — Direttiva (Cosa fare)

Le direttive sono SOP (Standard Operating Procedures) scritte in Markdown.
Definiscono obiettivi, input, strumenti da usare, output attesi e casi limite.
Sono istruzioni in linguaggio naturale, come le daresti a un ingegnere junior competente.

| Documento | Scopo |
|-----------|-------|
| [01-project-intake.md](L1-directives/01-project-intake.md) | Protocollo di raccolta requisiti e analisi pre-sviluppo |
| [02-architecture-patterns.md](L1-directives/02-architecture-patterns.md) | Pattern architetturali per tipo di progetto |
| [03-directive-template.md](L1-directives/03-directive-template.md) | Schema formale per creare nuove direttive |
| [04-directive-catalog.md](L1-directives/04-directive-catalog.md) | Indice di tutte le direttive attive |

### Livello 2 — Orchestrazione (Come decidere)

L'orchestrazione e il cuore dell'agent: routing intelligente, decomposizione dei task,
gestione degli errori, interazione con l'utente e mantenimento dello stato di progetto.

| Documento | Scopo |
|-----------|-------|
| [01-decision-engine.md](L2-orchestration/01-decision-engine.md) | Logica decisionale dell'agent |
| [02-task-decomposition.md](L2-orchestration/02-task-decomposition.md) | Come scomporre un progetto in task |
| [03-error-recovery.md](L2-orchestration/03-error-recovery.md) | Gestione errori e recovery a cascata |
| [04-interaction-protocol.md](L2-orchestration/04-interaction-protocol.md) | Quando e come interagire con l'utente |
| [05-state-management.md](L2-orchestration/05-state-management.md) | Memoria di progetto e stato persistente |

### Livello 3 — Esecuzione (Come fare il lavoro)

L'esecuzione e deterministica: script testabili, standard di qualita, sicurezza,
documentazione, CI/CD e gestione delle dipendenze.

| Documento | Scopo |
|-----------|-------|
| [01-code-standards.md](L3-execution/01-code-standards.md) | Standard di qualita del codice |
| [02-testing-strategy.md](L3-execution/02-testing-strategy.md) | Strategia di testing completa |
| [03-security-guidelines.md](L3-execution/03-security-guidelines.md) | Linee guida di sicurezza |
| [04-documentation-rules.md](L3-execution/04-documentation-rules.md) | Regole di documentazione |
| [05-cicd-setup.md](L3-execution/05-cicd-setup.md) | Setup CI/CD e version control |
| [06-dependency-management.md](L3-execution/06-dependency-management.md) | Gestione dipendenze e compatibilita |

### Template

| Template | Scopo |
|----------|-------|
| [project-spec.md](templates/project-spec.md) | Template per specifica tecnica di progetto |
| [adr-template.md](templates/adr-template.md) | Template Architecture Decision Record |
| [changelog-template.md](templates/changelog-template.md) | Template changelog |
| [project-state-template.md](templates/project-state-template.md) | Template stato del progetto (memoria persistente) |

---

## Workflow Operativo dell'Agent

Quando l'agent riceve una richiesta, segue questo flusso:

```
1. LEGGI questo file (DOE.md) per orientarti
2. ESEGUI il Project Intake Protocol (L1-directives/01-project-intake.md)
   - Raccogli requisiti obbligatori
   - Genera domande contestuali
   - Produci la specifica tecnica (templates/project-spec.md)
   - Ottieni approvazione dall'utente
3. CONSULTA il Decision Engine (L2-orchestration/01-decision-engine.md)
   - Cerca direttive applicabili
   - Classifica il task (atomico vs composto)
   - Valuta impatto e costi
4. DECOMPONI il lavoro (L2-orchestration/02-task-decomposition.md)
   - Identifica moduli e dipendenze
   - Definisci ordine di implementazione
5. ESEGUI seguendo gli standard (L3-execution/*)
   - Scrivi codice secondo code-standards.md
   - Testa secondo testing-strategy.md
   - Proteggi secondo security-guidelines.md
   - Documenta secondo documentation-rules.md
6. GESTISCI errori (L2-orchestration/03-error-recovery.md)
   - Classifica l'errore (E1-E5)
   - Applica il protocollo di recovery appropriato
7. AGGIORNA lo stato (L2-orchestration/05-state-management.md)
   - Aggiorna project-state.md
   - Registra decisioni e lezioni apprese
```

---

## Principi Fondamentali

1. **Mai scrivere codice senza una specifica approvata.** La specifica e il contratto
   tra intenzione e implementazione.

2. **Preferisci soluzioni deterministiche.** Quando puoi usare uno script, non generare
   codice inline. Quando due approcci sono equivalenti, scegli il piu testabile.

3. **Quando non sei sicuro, chiedi.** Il costo di una domanda e sempre inferiore
   al costo di un rifacimento.

4. **Le direttive sono documenti vivi.** Aggiornale quando impari qualcosa di nuovo.
   Ogni progetto completato rende il framework piu forte per il successivo.

5. **Sicurezza, stabilita, prestazioni — in quest'ordine.** Ottimizza prima per
   sicurezza, poi per stabilita e affidabilita, infine per prestazioni.

6. **Ogni regola ha una motivazione.** Se una regola non ha senso per un progetto
   specifico, segnalalo e proponi un'eccezione documentata. Non ignorarla silenziosamente.

---

## Struttura Directory di Riferimento

```
doe-framework/
├── DOE.md                          # Questo file — entry point
├── L1-directives/                  # Livello 1: Direttiva
│   ├── 01-project-intake.md
│   ├── 02-architecture-patterns.md
│   ├── 03-directive-template.md
│   └── 04-directive-catalog.md
├── L2-orchestration/               # Livello 2: Orchestrazione
│   ├── 01-decision-engine.md
│   ├── 02-task-decomposition.md
│   ├── 03-error-recovery.md
│   ├── 04-interaction-protocol.md
│   └── 05-state-management.md
├── L3-execution/                   # Livello 3: Esecuzione
│   ├── 01-code-standards.md
│   ├── 02-testing-strategy.md
│   ├── 03-security-guidelines.md
│   ├── 04-documentation-rules.md
│   ├── 05-cicd-setup.md
│   └── 06-dependency-management.md
└── templates/                      # Template pronti all'uso
    ├── project-spec.md
    ├── project-state-template.md
    ├── adr-template.md
    ├── changelog-template.md
    └── .gitignore-templates/
```
