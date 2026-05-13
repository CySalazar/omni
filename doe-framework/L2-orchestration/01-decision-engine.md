# Decision Engine — Logica Decisionale dell'Agent

> **Versione:** 1.0.0
> **Ultimo aggiornamento:** 2026-03-24
> **Livello:** L2 — Orchestrazione
> **Dipende da:** [DOE.md](../DOE.md), [04-interaction-protocol.md](04-interaction-protocol.md)

---

## Scopo

Questo documento definisce **come l'agent prende decisioni** di fronte a una richiesta.
Il Decision Engine è un albero decisionale esplicito che elimina l'improvvisazione:
per ogni situazione, l'agent sa quale percorso seguire.

---

## Albero Decisionale Principale

Quando l'agent riceve una richiesta (nuova o come continuazione di un progetto in corso),
segue questo flusso in ordine:

```
RICEVI RICHIESTA
    │
    ├─[1] La richiesta è chiara e completa?
    │   │
    │   ├── NO → Attiva Interaction Protocol (S1: chiedi chiarimenti)
    │   │         Non procedere finché la richiesta non è univoca.
    │   │
    │   └── SÌ → Prosegui al passo [2]
    │
    ├─[2] Esistono direttive applicabili nel catalogo?
    │   │
    │   ├── SÌ → Carica la direttiva e segui la sua procedura
    │   │         Verifica che le pre-condizioni siano soddisfatte.
    │   │         Se non lo sono, risolvile prima di eseguire.
    │   │
    │   └── NO → Valuta se:
    │             ├── Il task è ricorrente o generalizzabile?
    │             │   └── SÌ → Crea una nuova direttiva (usando 03-directive-template.md)
    │             │             Esegui il task seguendo la nuova direttiva.
    │             │             Aggiungi la direttiva al catalogo.
    │             │
    │             └── NO → Procedi ad-hoc. Documenta le decisioni nell'ADR.
    │
    ├─[3] Il task è atomico o composto?
    │   │
    │   ├── ATOMICO → Esegui direttamente seguendo gli standard L3.
    │   │              Un task è atomico se può essere completato in un singolo
    │   │              passo logico senza dipendenze interne.
    │   │
    │   └── COMPOSTO → Attiva Task Decomposition (02-task-decomposition.md).
    │                   Scomponi in task atomici, mappa le dipendenze,
    │                   definisci l'ordine di esecuzione.
    │
    ├─[4] L'operazione ha impatto irreversibile?
    │   │
    │   ├── SÌ → Attiva Interaction Protocol (S5: conferma operazione distruttiva)
    │   │         Esempi: delete di file/dati, drop di tabelle, overwrite di
    │   │         configurazioni, push force, deploy in produzione.
    │   │
    │   └── NO → Prosegui al passo [5]
    │
    ├─[5] L'operazione usa risorse a pagamento?
    │   │
    │   ├── SÌ → Attiva Interaction Protocol (S6: conferma costi)
    │   │         Stima il costo previsto e presentalo all'utente.
    │   │         Esempi: chiamate API a pagamento, provisioning cloud,
    │   │         servizi SaaS con costi per utilizzo.
    │   │
    │   └── NO → Prosegui all'esecuzione
    │
    └─[6] ESEGUI
          Segui gli standard definiti in L3-execution/.
          Applica il nodo di verifica deterministico dopo ogni fase.
          Gestisci gli errori secondo 03-error-recovery.md.
          Aggiorna lo stato secondo 05-state-management.md.
```

---

## Principi di Decisione

Questi principi guidano l'agent quando l'albero decisionale non copre un caso specifico
o quando deve scegliere tra opzioni equivalenti.

### 1. Determinismo sopra probabilismo

Preferisci soluzioni deterministiche (script, query, configurazioni) a soluzioni
generate inline dall'LLM. Il codice generato una volta, testato e salvato in `execution/`
è più affidabile del codice rigenerato ad ogni esecuzione.

### 2. Testabilità come criterio di scelta

Quando due approcci sono funzionalmente equivalenti, scegli quello più testabile.
Un approccio è più testabile se:
- Produce output prevedibili dato lo stesso input
- Ha meno dipendenze esterne
- Può essere verificato con asserzioni precise
- Ha stati intermedi osservabili

### 3. Il costo di una domanda è sempre inferiore al costo di un rifacimento

Quando non sei sicuro, chiedi. Ma formula la domanda in modo che la risposta
sia immediatamente azionabile (vedi Interaction Protocol).

### 4. Registra ogni decisione significativa

Ogni decisione che influenza architettura, stack tecnologico, sicurezza o
comportamento dell'applicazione deve essere registrata in un ADR
(Architecture Decision Record) usando il template in `templates/adr-template.md`.

### 5. Sicurezza, Stabilità, Prestazioni — in quest'ordine

Quando c'è un conflitto tra questi tre attributi, la priorità è:
1. **Sicurezza** — Mai compromettere la sicurezza per performance o comodità
2. **Stabilità** — Un sistema lento ma affidabile è meglio di uno veloce ma fragile
3. **Prestazioni** — Ottimizza solo dopo che sicurezza e stabilità sono garantite

---

## Sotto-Alberi Decisionali per Scenari Comuni

### Scelta dello Stack Tecnologico

```
ANALIZZA IL REQUISITO
    │
    ├── L'utente ha specificato tecnologie?
    │   ├── SÌ → Usa quelle specificate. Se ci sono rischi, segnalali (S9).
    │   └── NO → Prosegui alla selezione automatica
    │
    ├── Consulta docs/tech-specs.md (se esiste un progetto in corso)
    │   └── Ci sono vincoli di compatibilità con lo stack esistente?
    │       ├── SÌ → Rispetta i vincoli. Se limitanti, segnala (S8).
    │       └── NO → Procedi alla selezione libera
    │
    ├── Identifica il tipo di progetto:
    │   ├── WebApp → Valuta: framework frontend, backend, database, hosting
    │   ├── API → Valuta: framework, protocollo (REST/GraphQL/gRPC), database
    │   ├── CLI tool → Valuta: linguaggio, parser argomenti, distribuzione
    │   ├── Bot → Valuta: piattaforma, libreria, persistenza
    │   ├── Data pipeline → Valuta: orchestratore, storage, processing engine
    │   ├── DevOps/Infra → Valuta: IaC tool, cloud provider, CI/CD platform
    │   └── Altro → Analisi caso per caso
    │
    ├── Per ogni componente dello stack, applica i criteri:
    │   1. Maturità e stabilità della tecnologia
    │   2. Dimensione e attività della community
    │   3. Compatibilità con il resto dello stack
    │   4. Curva di apprendimento vs benefici
    │   5. Licenza compatibile
    │   6. Performance adeguate alla scala prevista
    │
    └── Presenta la proposta all'utente (S2) con:
        - Stack selezionato con motivazioni per ogni scelta
        - Alternative considerate e perché scartate
        - Rischi noti e mitigazioni
```

### Gestione di un Conflitto tra Direttive

```
CONFLITTO RILEVATO
    │
    ├── Le direttive sono dello stesso livello?
    │   ├── SÌ → La direttiva più specifica ha la precedenza
    │   │         (es. una direttiva per "API REST" prevale su "progetto generico")
    │   └── NO → La direttiva di livello superiore ha la precedenza
    │             (L1 > L2 > L3 per questioni di principio)
    │
    ├── Il conflitto è risolvibile con un'eccezione documentata?
    │   ├── SÌ → Documenta l'eccezione nell'ADR, procedi
    │   └── NO → Attiva Interaction Protocol (S8: deviazione dalla specifica)
    │
    └── Aggiorna il catalogo delle direttive per prevenire il conflitto in futuro
```

### Richiesta di Modifica in Corso d'Opera

```
L'UTENTE CHIEDE UNA MODIFICA
    │
    ├── La modifica è compatibile con l'architettura attuale?
    │   ├── SÌ → Valuta l'impatto sui task in corso e completati
    │   │   ├── Impatto minimo (< 3 file, nessun cambio architetturale)
    │   │   │   → Implementa, aggiorna la specifica e i test
    │   │   └── Impatto significativo
    │   │       → Presenta l'analisi di impatto all'utente (S8)
    │   │         Includi: file impattati, test da riscrivere, stima effort
    │   │
    │   └── NO → La modifica richiede refactoring architetturale
    │       → STOP — Presenta le opzioni:
    │         A) Refactoring completo (stima tempo e rischio)
    │         B) Workaround con debito tecnico documentato
    │         C) Rinuncia alla modifica con motivazione
    │
    └── Dopo approvazione:
        - Aggiorna la specifica tecnica
        - Aggiorna project-state.md
        - Rigenera il piano dei task (02-task-decomposition.md)
        - Se necessario, crea un nuovo ADR
```

---

## Nodo di Verifica Deterministico

Dopo ogni fase di scrittura codice, l'agent esegue **obbligatoriamente** questa
sequenza di verifica prima di dichiarare il task completato:

```
CODICE SCRITTO
    │
    ├─[V1] Linting (tool specifico del linguaggio)
    │   ├── Pass → Prosegui
    │   └── Fail → Correggi → Ri-esegui V1
    │
    ├─[V2] Type checking (se applicabile al linguaggio)
    │   ├── Pass → Prosegui
    │   └── Fail → Correggi → Ri-esegui V2
    │
    ├─[V3] Unit test
    │   ├── Pass → Prosegui
    │   └── Fail → Analizza:
    │       ├── Bug nel codice → Correggi il codice → Ri-esegui da V1
    │       └── Bug nel test → Correggi il test → Ri-esegui V3
    │
    ├─[V4] Integration test (se applicabile)
    │   ├── Pass → Prosegui
    │   └── Fail → Analizza impatto (vedi 03-error-recovery.md)
    │
    └─[V5] Verifica di sicurezza (checklist da 03-security-guidelines.md)
        ├── Pass → Task completato ✓
        └── Fail → Correggi → Ri-esegui da V5

REGOLA ANTI-LOOP: Se uno step fallisce 3 volte consecutive dopo tentativi
di correzione, l'agent si ferma e coinvolge l'utente (S4).
```

---

## Registro delle Decisioni

L'agent mantiene un registro delle decisioni significative prese durante il progetto.
Ogni voce include:

| Campo | Descrizione |
|-------|-------------|
| Data | Quando la decisione è stata presa |
| Contesto | Cosa ha portato alla necessità di una decisione |
| Decisione | Cosa è stato deciso |
| Motivazione | Perché questa opzione e non le alternative |
| Conseguenze | Impatto previsto della decisione |
| Stato | Attiva / Superata / Rivalutare |

Per decisioni architetturali importanti, si usa il template ADR completo
(`templates/adr-template.md`). Per decisioni operative minori, basta una riga
nel `project-state.md`.
