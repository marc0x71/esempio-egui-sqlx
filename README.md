# Todo SQLx: egui + sqlx in un contesto async

> 🇬🇧 **English version below:** [jump to the English version](#english-version)

Una piccola app di todo scritta in Rust con [egui](https://github.com/emilk/egui)/eframe per l'interfaccia e [sqlx](https://github.com/launchbadge/sqlx) su SQLite per la persistenza.

Non è un'applicazione da usare, ma un **modello di riferimento**: mostra un modo semplice e corretto per far dialogare un'interfaccia immediate mode, sincrona per natura, con una libreria di accesso ai dati asincrona.

## Il problema

egui ridisegna l'interfaccia decine di volte al secondo chiamando codice **sincrono**: dentro la funzione che disegna non si può fare `await`, e qualunque operazione lenta blocca la finestra.

sqlx, invece, è **asincrono**: ogni query restituisce un `Future` che deve essere eseguito da un runtime come Tokio.

Servono quindi due mondi separati che comunicano senza aspettarsi a vicenda.

## L'architettura in breve

```mermaid
flowchart LR
    subgraph UI["Thread della UI (eframe)"]
        App["TodoApp<br/>stato + disegno"]
    end

    subgraph RT["Runtime Tokio"]
        Worker["Worker<br/>esegue i comandi in ordine"]
        Pool[("SqlitePool")]
    end

    App -- "DbCommand<br/>(canale mpsc)" --> Worker
    Worker -- "DbEvent<br/>(canale mpsc)" --> App
    Worker -- "request_repaint()" --> App
    Worker <--> Pool
```

- La UI **non esegue mai query**: invia un comando (`DbCommand`) su un canale e prosegue con il disegno.
- Un **worker** che vive nel runtime Tokio riceve i comandi, li esegue sul database e restituisce il risultato come evento (`DbEvent`) su un secondo canale.
- A ogni frame la UI svuota il canale degli eventi con `try_recv()`, che non blocca mai, e aggiorna il proprio stato.
- Dopo aver inviato un evento, il worker chiama `ctx.request_repaint()`: senza questa chiamata egui, che ridisegna solo quando succede qualcosa, mostrerebbe i nuovi dati solo al successivo movimento del mouse.

## Il ciclo di un comando

Ecco cosa succede quando l'utente aggiunge un todo:

```mermaid
sequenceDiagram
    participant U as UI (TodoApp)
    participant W as Worker (Tokio)
    participant DB as SQLite

    U->>W: DbCommand::AddTodo { title }
    Note over U: la UI continua a disegnare
    W->>DB: INSERT INTO todos ...
    DB-->>W: ok
    W-->>U: DbEvent::TodoAdded
    W->>U: request_repaint()
    U->>W: DbCommand::LoadTodos
    W->>DB: SELECT ... FROM todos
    DB-->>W: righe
    W-->>U: DbEvent::TodosLoaded(Vec<Todo>)
    W->>U: request_repaint()
    Note over U: al frame successivo la lista è aggiornata
```

Dopo ogni modifica la UI ricarica l'intera lista. È una scelta voluta per tenere il modello semplice: lo stato mostrato coincide sempre con quello del database. Nella sezione [Possibili evoluzioni](#possibili-evoluzioni) trovi un'alternativa più efficiente.

## Struttura del codice

```
src/
├── main.rs            avvio: runtime Tokio, pool, inizializzazione del DB, eframe
├── model.rs           struct Todo (FromRow)
├── app.rs             TodoApp: stato dell'interfaccia, gestione degli eventi, disegno
└── db/
    ├── mod.rs         DbCommand, DbEvent, DbHandle (canali + worker)
    ├── worker.rs      handle_command: traduce un comando in una chiamata al repository
    └── repository.rs  le query SQL, una funzione async per operazione
```

Ogni livello conosce solo quello immediatamente sotto:

| Modulo | Conosce | Non conosce |
|---|---|---|
| `app.rs` | `DbHandle`, `DbCommand`, `DbEvent` | Tokio, sqlx, SQL |
| `db/mod.rs` | canali, runtime, worker | l'interfaccia (solo `egui::Context` per il repaint) |
| `db/worker.rs` | comandi, eventi, repository | canali e UI |
| `db/repository.rs` | sqlx e SQL | tutto il resto |

## Aspetto grafico

L'interfaccia usa [modern-egui](https://crates.io/crates/modern-egui), una mia piccola libreria che aggiunge a egui un tema moderno chiaro e scuro, una palette di colori, valori predefiniti per spaziature e dimensioni dei caratteri e alcuni metodi di comodo su `egui::Ui` (come `primary_button` o `muted_label`). Dipende solo da egui, quindi funziona con qualsiasi integrazione, non solo con eframe.

In questo progetto serve soltanto per lo stile: la parte che riguarda egui e sqlx non dipende da essa.

## Scelte di progetto

### Il runtime è creato a mano

`main` non usa `#[tokio::main]`: costruisce il runtime esplicitamente, lo usa con `block_on` **solo all'avvio** (creazione del pool e della tabella) e poi lo cede a `DbHandle`. In questo modo il thread principale resta libero per eframe, che vuole governare il proprio event loop.

### Due canali, due tipi

`DbCommand` (dalla UI al worker) e `DbEvent` (dal worker alla UI) sono enum distinti. Chi legge il codice vede subito tutte le operazioni possibili e tutti i risultati possibili, e il compilatore obbliga a gestire ogni caso nel `match`.

Entrambi i canali sono `tokio::sync::mpsc` senza limite di capacità: l'invio è sincrono (`send` non richiede `await`), quindi si può usare direttamente dal codice della UI.

### Il worker è sequenziale

Il worker elabora un comando alla volta, nell'ordine in cui arrivano:

```rust
while let Some(command) = command_rx.recv().await {
    let event = handle_command(&pool, command).await;
    // ...
}
```

Questo garantisce che un `LoadTodos` inviato dopo un `AddTodo` veda il nuovo record. Con SQLite, che ammette un solo scrittore alla volta, eseguire i comandi in parallelo non porterebbe comunque grandi vantaggi.

### Chiusura pulita

Alla chiusura della finestra, il `Drop` di `DbHandle`:

1. chiude il canale dei comandi, così il worker esegue quelli ancora in coda ed esce dal ciclo;
2. aspetta la fine del worker con `block_on`, lecito perché `drop` gira sul thread della UI e non dentro il runtime;
3. solo a quel punto lascia distruggere il runtime.

Quando il worker termina, il pool che possedeva viene distrutto e le connessioni vengono rilasciate. Senza questa sequenza il runtime verrebbe distrutto per primo e cancellerebbe il worker anche nel mezzo di una query.

## Avvio

```sh
cargo run
```

Al primo avvio viene creato il file `app.db` nella **cartella da cui lanci il comando**, con la tabella `todos`.

## Possibili evoluzioni

Il progetto resta volutamente minimale. Alcuni passi naturali per portarlo verso un'app reale:

- **Eventi che trasportano i dati.** Invece di ricaricare l'intera lista dopo ogni modifica, `TodoAdded(Todo)` (con `INSERT ... RETURNING`), `TodoUpdated { id, done }` e `TodoDeleted { id }` permettono di aggiornare lo stato locale direttamente.
- **Errori di lettura e di scrittura separati.** Dopo un errore di scrittura conviene ricaricare i dati per riallineare la UI; dopo un errore di lettura no, altrimenti si rischia un ciclo di tentativi.
- **Aggiornamento ottimistico.** Applicare subito le modifiche allo stato locale e riallinearlo solo in caso di errore, così l'interfaccia risponde senza aspettare il database.
- **Migrazioni** con `sqlx::migrate!()` al posto di `CREATE TABLE IF NOT EXISTS`.
- **Percorso del database** nella cartella dati dell'utente invece che in quella corrente.

## Licenza

Distribuito con licenza MIT. Il testo completo è nel file [LICENSE](LICENSE).

---

## English version

> 🇮🇹 [Torna alla versione italiana](#todo-sqlx-egui--sqlx-in-un-contesto-async)

A small todo app written in Rust, using [egui](https://github.com/emilk/egui)/eframe for the UI and [sqlx](https://github.com/launchbadge/sqlx) on SQLite for persistence.

It is not meant to be used as an application, but as a **reference model**: it shows a simple and correct way to make an immediate mode UI, which is synchronous by nature, work with an asynchronous data access library.

### The problem

egui redraws the interface dozens of times per second by calling **synchronous** code: you cannot `await` inside the drawing function, and any slow operation freezes the window.

sqlx, on the other hand, is **asynchronous**: every query returns a `Future` that must be driven by a runtime such as Tokio.

So we need two separate worlds that communicate without waiting on each other.

### Architecture at a glance

```mermaid
flowchart LR
    subgraph UI["UI thread (eframe)"]
        App["TodoApp<br/>state + drawing"]
    end

    subgraph RT["Tokio runtime"]
        Worker["Worker<br/>runs commands in order"]
        Pool[("SqlitePool")]
    end

    App -- "DbCommand<br/>(mpsc channel)" --> Worker
    Worker -- "DbEvent<br/>(mpsc channel)" --> App
    Worker -- "request_repaint()" --> App
    Worker <--> Pool
```

- The UI **never runs queries**: it sends a command (`DbCommand`) over a channel and keeps drawing.
- A **worker** living in the Tokio runtime receives commands, runs them against the database and sends the result back as an event (`DbEvent`) over a second channel.
- On every frame the UI drains the event channel with `try_recv()`, which never blocks, and updates its state.
- After sending an event, the worker calls `ctx.request_repaint()`: without it, egui, which only redraws when something happens, would show the new data only on the next mouse movement.

### Lifecycle of a command

Here is what happens when the user adds a todo:

```mermaid
sequenceDiagram
    participant U as UI (TodoApp)
    participant W as Worker (Tokio)
    participant DB as SQLite

    U->>W: DbCommand::AddTodo { title }
    Note over U: the UI keeps drawing
    W->>DB: INSERT INTO todos ...
    DB-->>W: ok
    W-->>U: DbEvent::TodoAdded
    W->>U: request_repaint()
    U->>W: DbCommand::LoadTodos
    W->>DB: SELECT ... FROM todos
    DB-->>W: rows
    W-->>U: DbEvent::TodosLoaded(Vec<Todo>)
    W->>U: request_repaint()
    Note over U: on the next frame the list is up to date
```

After every change the UI reloads the whole list. This is a deliberate choice to keep the model simple: what is displayed always matches the database. The [Possible improvements](#possible-improvements) section describes a more efficient alternative.

### Code structure

```
src/
├── main.rs            startup: Tokio runtime, pool, DB initialization, eframe
├── model.rs           Todo struct (FromRow)
├── app.rs             TodoApp: UI state, event handling, drawing
└── db/
    ├── mod.rs         DbCommand, DbEvent, DbHandle (channels + worker)
    ├── worker.rs      handle_command: maps a command to a repository call
    └── repository.rs  SQL queries, one async function per operation
```

Each layer only knows about the one directly below it:

| Module | Knows about | Doesn't know about |
|---|---|---|
| `app.rs` | `DbHandle`, `DbCommand`, `DbEvent` | Tokio, sqlx, SQL |
| `db/mod.rs` | channels, runtime, worker | the UI (only `egui::Context` for repainting) |
| `db/worker.rs` | commands, events, repository | channels and UI |
| `db/repository.rs` | sqlx and SQL | everything else |

### Look and feel

The UI uses [modern-egui](https://crates.io/crates/modern-egui), a small library of mine that adds to egui a modern light and dark theme, a color palette, design tokens for spacing and font sizes, and a few convenience methods on `egui::Ui` (such as `primary_button` or `muted_label`). It depends only on egui, so it works with any egui integration, not just eframe.

In this project it is used for styling only: the egui + sqlx part doesn't depend on it.

### Design choices

#### The runtime is built manually

`main` does not use `#[tokio::main]`: it builds the runtime explicitly, uses it with `block_on` **only at startup** (creating the pool and the table) and then hands it over to `DbHandle`. This keeps the main thread free for eframe, which wants to own its event loop.

#### Two channels, two types

`DbCommand` (UI → worker) and `DbEvent` (worker → UI) are separate enums. Anyone reading the code immediately sees every possible operation and every possible result, and the compiler forces every case to be handled in `match`.

Both channels are unbounded `tokio::sync::mpsc` channels: sending is synchronous (`send` needs no `await`), so it can be called directly from UI code.

#### The worker is sequential

The worker processes one command at a time, in the order they arrive:

```rust
while let Some(command) = command_rx.recv().await {
    let event = handle_command(&pool, command).await;
    // ...
}
```

This guarantees that a `LoadTodos` sent after an `AddTodo` sees the new record. With SQLite, which allows only one writer at a time, running commands in parallel wouldn't bring much benefit anyway.

#### Clean shutdown

When the window is closed, the `Drop` implementation of `DbHandle`:

1. closes the command channel, so the worker runs any queued commands and exits its loop;
2. waits for the worker to finish with `block_on`, which is allowed because `drop` runs on the UI thread, not inside the runtime;
3. only then lets the runtime be destroyed.

When the worker ends, the pool it owns is dropped and its connections are released. Without this sequence the runtime would be destroyed first, cancelling the worker even in the middle of a query.

### Running

```sh
cargo run
```

On first launch, an `app.db` file with the `todos` table is created in the **directory you run the command from**.

### Possible improvements

The project is intentionally minimal. Some natural next steps towards a real application:

- **Events that carry data.** Instead of reloading the whole list after each change, `TodoAdded(Todo)` (with `INSERT ... RETURNING`), `TodoUpdated { id, done }` and `TodoDeleted { id }` let the UI update its local state directly.
- **Separate read and write errors.** After a write error it makes sense to reload the data to resync the UI; after a read error it doesn't, otherwise you risk an endless retry loop.
- **Optimistic updates.** Apply changes to the local state immediately and resync only on error, so the UI responds without waiting for the database.
- **Migrations** with `sqlx::migrate!()` instead of `CREATE TABLE IF NOT EXISTS`.
- **Database path** in the user's data directory instead of the current one.

### License

Released under the MIT License. See the [LICENSE](LICENSE) file for the full text.
