# State Management — Memoria di Progetto e Stato Persistente

> **Versione:** 1.0.0
> **Ultimo aggiornamento:** 2026-03-24
> **Livello:** L2 — Orchestrazione
> **Dipende da:** [DOE.md](../DOE.md), [01-decision-engine.md](01-decision-engine.md), [02-task-decomposition.md](02-task-decomposition.md)

---

## Scopo

Questo documento definisce **come l'agent gestisce la memoria di progetto** attraverso
le sessioni di lavoro. Senza un meccanismo di stato persistente, ogni sessione riparte
da zero: l'agent perde il contesto delle decisioni prese, dello stato di avanzamento
e dei vincoli scoperti. Il Project Memory System risolve questo problema con un file
strutturato (`project-state.md`) che funge da memoria a lungo termine del progetto.

**Principio fondante:** "Il contesto non deve vivere nella memoria dell'agent — deve
vivere in un file versionato, leggibile e aggiornabile. L'agent è effimero; lo stato
del progetto no."

---

## Il File `project-state.md`

Il file `project-state.md` è il cuore del sistema di stato persistente. Viene generato
automaticamente dall'agent alla prima sessione di lavoro e aggiornato alla fine di ogni
sessione successiva. Risiede nella **root del progetto**, accanto al README.md.

### Struttura Obbligatoria

Il file segue un formato rigido per garantire che l'agent possa leggerlo e interpretarlo
in modo affidabile. Usa il template completo in
[`templates/project-state-template.md`](../templates/project-state-template.md).

Le sezioni obbligatorie sono:

| Sezione | Scopo | Aggiornamento |
|---------|-------|---------------|
| Metadata | Versione, date, agent che ha lavorato | Automatico ad ogni sessione |
| Decisioni Architetturali | Registro cronologico delle decisioni | Append-only — non cancellare mai |
| Debito Tecnico Noto | Lista del tech debt accumulato | Aggiungere e aggiornare stato |
| Vincoli Scoperti | Limitazioni scoperte durante lo sviluppo | Append-only |
| Stato dei Moduli | Avanzamento per modulo con metriche | Aggiornare ad ogni sessione |
| Piano dei Task | Task correnti con stato e dipendenze | Aggiornare ad ogni sessione |
| Prossimi Passi | Azioni prioritizzate per la prossima sessione | Riscrivere ad ogni sessione |
| Registro Sessioni | Log di cosa è stato fatto in ogni sessione | Append-only |

---

## Ciclo di Vita dello Stato

### Inizio Sessione — Ricostruzione del Contesto

All'inizio di ogni sessione di lavoro, l'agent esegue questa sequenza **prima di
qualsiasi altra attività**:

```
INIZIO SESSIONE
    │
    ├─[1] Esiste project-state.md nella root del progetto?
    │   │
    │   ├── SÌ → Leggi il file per intero
    │   │         Ricostruisci il contesto:
    │   │         - Quali decisioni sono state prese?
    │   │         - Quali moduli sono completi, in corso, da fare?
    │   │         - Quali vincoli sono stati scoperti?
    │   │         - Quale debito tecnico è noto?
    │   │         - Quali sono i prossimi passi pianificati?
    │   │
    │   └── NO → È la prima sessione su questo progetto.
    │             Dopo il Project Intake (01-project-intake.md),
    │             genera project-state.md con il template.
    │
    ├─[2] Verifica la coerenza dello stato
    │   │
    │   ├── Lo stato è coerente con i file nel progetto?
    │   │   ├── SÌ → Prosegui
    │   │   └── NO → Segnala le discrepanze all'utente (S8)
    │   │             Proponi aggiornamento dello stato
    │   │
    │   └── Ci sono task in_progress dalla sessione precedente?
    │       ├── SÌ → Verifica lo stato reale del task
    │       │         (il codice è stato scritto? i test passano?)
    │       │         Aggiorna lo stato del task di conseguenza
    │       └── NO → Prosegui
    │
    └─[3] Presenta un riepilogo all'utente
          "Nella sessione precedente [DATA] è stato fatto:
           - [Riepilogo delle attività completate]
           I prossimi passi pianificati sono:
           - [Lista prossimi passi]
           Vuoi procedere con il piano o hai modifiche?"
```

### Durante la Sessione — Aggiornamento Incrementale

Durante il lavoro, l'agent aggiorna lo stato in memoria e persiste le modifiche
nei seguenti momenti:

| Evento | Azione sullo Stato |
|--------|-------------------|
| Task completato | Aggiorna stato del task → `completed`. Aggiorna stato del modulo. |
| Task iniziato | Aggiorna stato del task → `in_progress`. |
| Decisione architettuale presa | Aggiungi voce a "Decisioni Architetturali" con data e motivazione. |
| Vincolo scoperto | Aggiungi voce a "Vincoli Scoperti" con descrizione e impatto. |
| Debito tecnico introdotto | Aggiungi voce a "Debito Tecnico Noto" con priorità e contesto. |
| Errore E3/E4 risolto | Aggiorna la sezione pertinente con la risoluzione. |
| Checkpoint raggiunto | Persisti lo stato su file (flush intermedio). |
| Modifica al piano dei task | Aggiorna "Piano dei Task" e "Prossimi Passi". |

**Regola di persistenza intermedia:** Lo stato viene scritto su file (flush) ad ogni
**checkpoint** (vedi [02-task-decomposition.md](02-task-decomposition.md), Fase 5) e
ad ogni **evento critico** (decisione architetturale, scoperta di vincolo, errore E3+).
Questo previene la perdita di contesto in caso di interruzione improvvisa della sessione.

### Fine Sessione — Consolidamento

Alla fine di ogni sessione, l'agent esegue obbligatoriamente:

```
FINE SESSIONE
    │
    ├─[1] Aggiorna lo "Stato dei Moduli"
    │     Per ogni modulo tocccato nella sessione:
    │     - Stato: da fare | in corso | completo
    │     - Coverage: percentuale di test coverage attuale
    │     - Note: problemi aperti, TODO, osservazioni
    │
    ├─[2] Aggiorna il "Piano dei Task"
    │     - Segna come completati i task finiti
    │     - Aggiorna le stime di complessità se necessario
    │     - Aggiungi nuovi task emersi durante la sessione
    │
    ├─[3] Riscrivi "Prossimi Passi"
    │     Lista ordinata per priorità di cosa fare nella prossima sessione.
    │     Ogni passo deve essere specifico e azionabile.
    │     Non più di 5-7 passi (focus sulle priorità).
    │
    ├─[4] Aggiungi voce al "Registro Sessioni"
    │     Formato:
    │     ### Sessione [N] — [DATA]
    │     **Durata stimata:** [tempo]
    │     **Task completati:** [lista]
    │     **Task avviati:** [lista]
    │     **Decisioni prese:** [lista o "nessuna"]
    │     **Problemi riscontrati:** [lista o "nessuno"]
    │     **Note:** [osservazioni libere]
    │
    └─[5] Scrivi project-state.md su file
          Verifica che il file sia ben formato.
          Verifica che non contenga dati sensibili.
```

---

## Sezioni del `project-state.md` in Dettaglio

### Decisioni Architetturali

Questa sezione è un registro **append-only**: le decisioni non vengono mai cancellate,
solo aggiunte. Questo fornisce uno storico completo del ragionamento dietro il progetto.

Formato per ogni voce:

```markdown
- **[YYYY-MM-DD]** [Descrizione della decisione] — *Motivazione:* [perché questa scelta]
  *Alternative considerate:* [cosa è stato scartato e perché]
  *Stato:* Attiva | Superata da [data, nuova decisione] | Da rivalutare
```

Per decisioni significative che meritano un'analisi più approfondita, l'agent crea
un ADR separato usando il template `templates/adr-template.md` e lo referenzia qui.

**Quando registrare una decisione:**
- Scelta dello stack tecnologico o di una specifica libreria
- Scelta del pattern architetturale o di un design pattern
- Trade-off espliciti (es. "sacrifichiamo X per avere Y")
- Deviazioni dalla specifica originale
- Scelte di sicurezza non ovvie
- Workaround per limitazioni di terze parti

### Debito Tecnico Noto

Il debito tecnico viene tracciato con priorità e contesto sufficiente per poterlo
risolvere in futuro senza dover ricostruire il ragionamento.

Formato per ogni voce:

```markdown
- [ ] **[Priorità: Alta|Media|Bassa]** [Descrizione del debito]
      *Contesto:* [Perché è stato introdotto]
      *Impatto:* [Cosa succede se non viene risolto]
      *Soluzione proposta:* [Come risolverlo quando sarà il momento]
      *Moduli impattati:* [Lista moduli]
      *Introdotto:* [Data, sessione]
```

Priorità del debito tecnico:

| Priorità | Criterio | Azione |
|----------|----------|--------|
| **Alta** | Impatta sicurezza, stabilità o blocca nuove feature | Risolvere nella prossima sessione o appena possibile |
| **Media** | Degrada la qualità del codice o rallenta lo sviluppo | Pianificare la risoluzione entro le prossime 3-5 sessioni |
| **Bassa** | Miglioria cosmetica o ottimizzazione non urgente | Risolvere quando si lavora sul modulo interessato |

### Vincoli Scoperti

I vincoli sono limitazioni scoperte durante lo sviluppo che non erano note al momento
della specifica. Questa sezione è append-only e serve a prevenire errori ripetuti.

Formato per ogni voce:

```markdown
- **[YYYY-MM-DD]** [Descrizione del vincolo]
  *Scoperto durante:* [Task o attività]
  *Impatto:* [Cosa limita o impedisce]
  *Workaround:* [Se esiste, come aggirarlo]
  *Permanente:* Sì | No (se no, indicare quando potrebbe essere risolto)
```

Esempi tipici di vincoli:
- Rate limit di API esterne
- Incompatibilità tra versioni di librerie
- Limitazioni del provider cloud
- Requisiti non documentati di sistemi legacy
- Comportamenti non previsti di dipendenze

### Stato dei Moduli

Tabella che riflette lo stato reale e attuale di ogni modulo del progetto.

```markdown
| Modulo | Stato | Coverage | Ultimo Aggiornamento | Note |
|--------|-------|----------|---------------------|------|
| [nome] | da fare | — | — | [note] |
| [nome] | in corso | 45% | 2026-03-24 | Manca endpoint /users |
| [nome] | completo | 87% | 2026-03-24 | — |
```

Stati possibili:
- **da fare** — Nessun codice scritto per questo modulo
- **in corso** — Implementazione iniziata ma non completata
- **completo** — Implementazione finita, tutti i test passano, documentazione aggiornata
- **bloccato** — Non può procedere per un vincolo o dipendenza non risolta

### Piano dei Task

Riflette il piano corrente dei task (originato dalla Task Decomposition) con lo stato
aggiornato. Questa sezione è il collegamento diretto con
[02-task-decomposition.md](02-task-decomposition.md).

```markdown
| Task ID | Descrizione | Modulo | Stato | Complessità | Note |
|---------|-------------|--------|-------|-------------|------|
| TASK-001 | Setup progetto | — | completed | S | — |
| TASK-002 | Schema database | db | completed | S | — |
| TASK-003 | Modello User | user-model | in_progress | S | Validazione email in corso |
| TASK-004 | Unit test User | user-model | pending | S | Dipende da TASK-003 |
```

Stati dei task:
- **pending** — Non ancora iniziato
- **in_progress** — L'agent ci sta lavorando
- **completed** — Finito e verificato (tutti i test passano)
- **blocked** — Bloccato da un vincolo o dipendenza
- **cancelled** — Cancellato (con motivazione nelle note)

### Registro Sessioni

Log cronologico di tutte le sessioni di lavoro. Fornisce uno storico di chi ha fatto
cosa e quando, utile per debugging e per ricostruire la timeline del progetto.

```markdown
### Sessione 1 — 2026-03-24
**Task completati:** TASK-001 (Setup progetto), TASK-002 (Schema database)
**Task avviati:** TASK-003 (Modello User)
**Decisioni prese:** Scelto PostgreSQL 16 su SQLite (vedi ADR-001)
**Problemi riscontrati:** Nessuno
**Note:** Prima sessione. Progetto inizializzato con successo.

### Sessione 2 — 2026-03-25
**Task completati:** TASK-003 (Modello User), TASK-004 (Unit test User)
**Task avviati:** TASK-005 (Auth service)
**Decisioni prese:** Scelto bcrypt per hashing password
**Problemi riscontrati:** Rate limit API esterna scoperto (100 req/min)
**Note:** Aggiunto vincolo su rate limit. Proposto caching come mitigazione.
```

---

## Interazione con Altri Documenti

Il `project-state.md` non è un documento isolato. Interagisce attivamente con gli
altri documenti del framework D.O.E.:

```
project-state.md
    │
    ├── LEGGE ← project-spec.md (specifica approvata — fonte di verità per i requisiti)
    │
    ├── LEGGE ← docs/tech-specs.md (vincoli di compatibilità delle dipendenze)
    │
    ├── AGGIORNA → Piano dei Task (riflette 02-task-decomposition.md)
    │
    ├── ALIMENTA → ADR (decisioni architetturali significative → templates/adr-template.md)
    │
    ├── INFORMA → Interaction Protocol (04-interaction-protocol.md)
    │               L'agent usa lo stato per calibrare la comunicazione con l'utente
    │
    ├── INFORMA → Error Recovery (03-error-recovery.md)
    │               Gli errori E3/E4 aggiornano lo stato e possono modificare il piano
    │
    └── INFORMA → Decision Engine (01-decision-engine.md)
                  Le decisioni pregresse e i vincoli influenzano le decisioni future
```

---

## Regole di Gestione dello Stato

### Regole Fondamentali

1. **Lo stato è sempre sincronizzato con la realtà del codice.** Se il codice dice
   una cosa e lo stato un'altra, lo stato è sbagliato e va corretto. Il codice è
   la fonte di verità.

2. **Le sezioni append-only non vengono mai cancellate.** Decisioni Architetturali,
   Vincoli Scoperti e Registro Sessioni sono registri storici. Si possono aggiornare
   gli stati (es. una decisione diventa "Superata") ma non si cancella la voce.

3. **Lo stato non contiene mai dati sensibili.** Niente credenziali, token, IP di
   produzione, dati personali. Se serve referenziare un segreto, usare il nome
   della variabile d'ambiente (es. "vedi `DATABASE_URL` in `.env`").

4. **Lo stato è leggibile da un essere umano.** Non è un formato binario o un JSON
   compresso. È Markdown pensato per essere letto, capito e modificato anche
   manualmente dall'utente.

5. **Ogni modifica allo stato è motivata.** Non aggiornare lo stato senza una ragione.
   Ogni cambio di stato di un task, ogni nuova decisione, ogni vincolo ha un
   contesto esplicito.

### Gestione dei Conflitti

Se l'utente modifica manualmente il `project-state.md` (cosa legittima e incoraggiata),
l'agent alla sessione successiva:

1. Rileva le differenze tra l'ultimo stato scritto dall'agent e lo stato attuale
2. Presenta le differenze all'utente per conferma
3. Integra le modifiche dell'utente come fonte di verità
4. Se le modifiche dell'utente sono in conflitto con vincoli tecnici noti, lo segnala

### Recupero da Stato Corrotto o Mancante

```
STATO NON LEGGIBILE O MANCANTE
    │
    ├── Il file esiste ma è malformato?
    │   ├── SÌ → Tenta il parsing delle sezioni leggibili
    │   │         Ricostruisci le sezioni mancanti dal codice e dalla specifica
    │   │         Presenta il risultato all'utente per validazione
    │   └── NO → Il file non esiste
    │
    ├── Il file non esiste ma il progetto ha codice?
    │   ├── SÌ → Ricostruisci lo stato dal codice esistente:
    │   │         - Analizza la struttura del progetto
    │   │         - Identifica i moduli e il loro stato apparente
    │   │         - Verifica i test esistenti e la loro copertura
    │   │         - Leggi docs/tech-specs.md e gli ADR se presenti
    │   │         Genera un project-state.md di ricostruzione
    │   │         Marca come "[RICOSTRUITO]" nel metadata
    │   │         Presenta all'utente per validazione
    │   │
    │   └── NO → Il progetto è nuovo. Procedi con il Project Intake.
    │
    └── In tutti i casi: non procedere con implementazione fino a quando
        lo stato è ricostruito e validato dall'utente.
```

---

## Metriche di Progetto

Il `project-state.md` include metriche aggregate che danno una vista sintetica
della salute del progetto. L'agent le calcola automaticamente dalle altre sezioni.

```markdown
## Metriche

| Metrica | Valore | Trend |
|---------|--------|-------|
| Task completati | 12/20 (60%) | ↑ +4 dalla sessione precedente |
| Task bloccati | 1/20 (5%) | → invariato |
| Coverage media | 72% | ↑ +8% dalla sessione precedente |
| Debito tecnico aperto | 3 (1 Alta, 1 Media, 1 Bassa) | ↑ +1 (nuova voce Media) |
| Vincoli scoperti | 4 | → invariato |
| Decisioni attive | 7 | ↑ +1 |
| Sessioni totali | 5 | — |
```

Le metriche servono a dare all'utente una **visione immediata** dello stato del
progetto senza dover leggere tutto il documento. Sono sempre nella parte alta
del file, subito dopo il metadata.

---

## Anti-Pattern da Evitare

| Anti-Pattern | Perché è Sbagliato | Cosa Fare Invece |
|-------------|-------------------|-----------------|
| Stato non aggiornato a fine sessione | La prossima sessione riparte senza contesto | Aggiornare SEMPRE, anche se la sessione è stata breve |
| Stato troppo generico ("progetto in corso") | Non fornisce informazioni utili | Essere specifici: quale modulo, quale task, quale percentuale |
| Cancellare decisioni superate | Si perde lo storico del ragionamento | Marcare come "Superata da [nuova decisione]" |
| Stato che contraddice il codice | Causa confusione e errori | Il codice è la fonte di verità — aggiornare lo stato |
| Dati sensibili nello stato | Rischio di sicurezza se il file viene committato | Referenziare variabili d'ambiente, mai valori |
| Prossimi Passi vaghi ("continuare lo sviluppo") | L'agent non sa cosa fare | Ogni passo deve essere specifico e azionabile |
| Ignorare lo stato all'inizio sessione | Si rischia di duplicare lavoro o contraddire decisioni | Leggere SEMPRE project-state.md come prima cosa |

---

## Integrazione con il Version Control

Il `project-state.md` è un file versionato e committato nel repository. Questo
significa che:

1. **Ogni aggiornamento dello stato è un commit separato** con messaggio descrittivo
   (tipo: `chore(state): update project state after session N`)
2. **La history di git fornisce un audit trail** di come lo stato è evoluto nel tempo
3. **I branch di feature possono avere stati divergenti** — il merge riconcilia lo
   stato come qualsiasi altro file
4. **Il file è incluso nella code review** — le decisioni e i vincoli sono visibili
   a tutto il team

**Nota:** Il `project-state.md` NON va nel `.gitignore`. È un artefatto di progetto
a tutti gli effetti, come il README.md o il CHANGELOG.md.

---

*Documento del framework D.O.E. — [DOE.md](../DOE.md)*
