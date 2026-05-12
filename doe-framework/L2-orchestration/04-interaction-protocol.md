# Interaction Protocol — Quando e Come Interagire con l'Utente

> **Versione:** 1.0.0
> **Ultimo aggiornamento:** 2026-03-24
> **Livello:** L2 — Orchestrazione
> **Dipende da:** [DOE.md](../DOE.md)

---

## Scopo

Questo documento definisce le regole che governano l'interazione tra l'agent e l'utente
durante l'intero ciclo di vita di un progetto. L'obiettivo è eliminare le ambiguità:
l'agent deve sapere esattamente **quando fermarsi e chiedere**, **quando procedere
autonomamente** e **come comunicare** in ogni situazione.

---

## Regola d'Oro

> **L'agent non deve mai fare supposizioni su ciò che l'utente vuole.
> Se c'è ambiguità, chiedi. Se non c'è, agisci.**

Il costo di una domanda è sempre inferiore al costo di un rifacimento.
Ma una domanda inutile interrompe il flusso di lavoro dell'utente.
L'equilibrio sta nel chiedere solo quando la risposta influenza materialmente il risultato.

---

## Matrice delle Situazioni

La tabella seguente è la reference principale. L'agent la consulta ogni volta che
deve decidere se interagire con l'utente o procedere autonomamente.

| # | Situazione | Azione dell'Agent | Livello |
|---|------------|-------------------|---------|
| S1 | Requisito ambiguo o incompleto | **STOP** — Chiedi chiarimento specifico, formulando la domanda in modo che la risposta sia azionabile | Obbligatorio |
| S2 | Scelta tecnologica con trade-off significativi | **STOP** — Presenta le opzioni con pro/contro in formato tabellare, includi una raccomandazione motivata, chiedi la preferenza dell'utente | Obbligatorio |
| S3 | Errore risolvibile autonomamente (classe E1) | Risolvi → Informa l'utente nel prossimo report di stato con una riga che spiega cosa è successo e come è stato risolto | Informativo |
| S4 | Errore non risolvibile autonomamente (classe E2+) | **STOP** — Descrivi il problema, le cause probabili e le opzioni disponibili. Non procedere senza una decisione dell'utente | Obbligatorio |
| S5 | Operazione distruttiva (delete, overwrite, drop) | **STOP** — Chiedi conferma esplicita descrivendo cosa verrà eliminato/sovrascritto e se l'operazione è reversibile | Obbligatorio |
| S6 | Operazione che usa risorse a pagamento (API, cloud) | **STOP** — Chiedi conferma fornendo una stima dei costi previsti | Obbligatorio |
| S7 | Task completato (singolo o milestone) | Report sintetico: cosa è stato fatto, test eseguiti, prossimi passi | Informativo |
| S8 | Deviazione dalla specifica necessaria | **STOP** — Proponi la modifica spiegando il motivo, l'impatto e le alternative. Attendi approvazione prima di implementare | Obbligatorio |
| S9 | Scoperta di rischio o vincolo non previsto | **STOP** — Segnala il rischio/vincolo, valuta l'impatto e proponi una mitigazione | Obbligatorio |
| S10 | Scelta tecnologica minore senza impatto architetturale | Procedi autonomamente, documenta la scelta nell'ADR o nel report di stato | Autonomo |
| S11 | Checkpoint di validazione (ogni N task completati) | Presenta il riepilogo dei task completati, lo stato attuale e i prossimi passi. Chiedi conferma per procedere | Checkpoint |

---

## Classificazione dei Livelli di Interazione

### Obbligatorio (STOP)

L'agent si ferma immediatamente. Non procede finché l'utente non risponde.

Regole per le domande obbligatorie:
- Formulare domande **specifiche e chiuse** quando possibile (sì/no, scelta tra opzioni)
- Se la domanda è aperta, fornire contesto sufficiente perché l'utente possa rispondere senza ricerche aggiuntive
- Non raggruppare più di **3 domande** in un singolo messaggio — se ce ne sono di più, dare priorità e chiedere le più critiche prima
- Ogni domanda deve spiegare **perché** la risposta è necessaria e **cosa succede** se viene data una risposta piuttosto che un'altra

### Informativo

L'agent continua a lavorare e informa l'utente nel prossimo punto di contatto naturale
(report di stato, checkpoint, completamento task).

Le informazioni vanno aggregate in modo leggibile:
- Errori risolti: una riga con problema, causa e soluzione
- Decisioni autonome: una riga con la scelta e la motivazione
- Anomalie notate: una riga con l'osservazione e l'eventuale azione suggerita

### Autonomo

L'agent procede senza interazione. La decisione viene documentata nel log di progetto
o nell'ADR per tracciabilità.

### Checkpoint

L'agent si ferma a intervalli prestabiliti (configurabili per progetto) per sincronizzarsi
con l'utente. Il checkpoint include:
- Riepilogo dei task completati dall'ultimo checkpoint
- Stato dei test (pass/fail/coverage)
- Problemi incontrati e come sono stati risolti
- Prossimi task pianificati
- Domande accumulate (se ce ne sono)

---

## Formato delle Comunicazioni

### Richiesta di Chiarimento (STOP)

```
## Chiarimento Necessario

**Contesto:** [Descrizione breve di cosa stai facendo e perché ti sei fermato]

**Domanda:** [La domanda specifica]

**Opzioni (se applicabile):**
- **A)** [Opzione] — Pro: [vantaggi] / Contro: [svantaggi]
- **B)** [Opzione] — Pro: [vantaggi] / Contro: [svantaggi]

**Raccomandazione:** [La tua raccomandazione motivata, se ne hai una]

**Impatto:** [Cosa cambia a seconda della risposta]
```

### Report di Stato (Informativo / Checkpoint)

```
## Report di Stato — [Data/Ora]

**Task completati:**
- [x] [Task 1] — [Risultato sintetico]
- [x] [Task 2] — [Risultato sintetico]

**Test:** [N pass / M fail / Coverage X%]

**Problemi risolti:**
- [Problema] → [Soluzione applicata]

**Prossimi passi:**
1. [Task successivo]
2. [Task successivo]

**Domande (se presenti):**
- [Domanda non bloccante, a cui l'utente può rispondere quando vuole]
```

### Segnalazione di Rischio (STOP)

```
## Rischio Rilevato

**Tipo:** [Tecnico / Sicurezza / Compatibilità / Performance / Costo]
**Severità:** [Alta / Media / Bassa]

**Descrizione:** [Cosa è stato scoperto]

**Impatto:** [Cosa succede se non si interviene]

**Mitigazione proposta:** [Azione suggerita]

**Necessito di:** [Approvazione per procedere / Informazione aggiuntiva / Nessuna azione richiesta]
```

---

## Configurazione per Progetto

Alcuni parametri dell'interazione sono configurabili nel file `project-state.md`
o nella specifica tecnica del progetto:

| Parametro | Default | Descrizione |
|-----------|---------|-------------|
| `checkpoint_interval` | Ogni 5 task completati | Frequenza dei checkpoint di validazione |
| `auto_resolve_threshold` | Classe E1 | Fino a quale classe di errore l'agent può risolvere autonomamente |
| `cost_confirmation_threshold` | Qualsiasi costo > $0 | Soglia sopra la quale chiedere conferma per risorse a pagamento |
| `max_questions_per_message` | 3 | Numero massimo di domande in un singolo messaggio |
| `report_detail_level` | Medio | Livello di dettaglio nei report (Minimo / Medio / Dettagliato) |

L'utente può sovrascrivere questi valori nella specifica tecnica del progetto.
Se non specificati, valgono i default.

---

## Anti-Pattern da Evitare

1. **Chiedere ciò che puoi dedurre.** Se la risposta è nel codice, nella specifica o nel
   contesto della conversazione, non chiedere.

2. **Chiedere permesso per ogni micro-decisione.** Le scelte di implementazione che non
   influenzano architettura, sicurezza o comportamento visibile all'utente non richiedono
   conferma.

3. **Accumulare domande senza lavorare.** Se hai molte domande, rispondi a quelle che puoi
   da solo e chiedi solo quelle dove la tua risposta potrebbe essere sbagliata.

4. **Report troppo dettagliati.** L'utente non ha bisogno di sapere ogni riga di codice
   modificata. Comunica i risultati, non i passi intermedi (a meno che non siano rilevanti).

5. **Procedere senza conferma dopo uno STOP.** Se la matrice dice STOP, l'agent si ferma.
   Non esistono eccezioni.

6. **Interpretare il silenzio come approvazione.** Se l'utente non risponde a una domanda
   obbligatoria, l'agent attende. Non assume una risposta default.
