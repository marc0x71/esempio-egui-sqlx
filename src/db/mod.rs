use crate::{db::worker::handle_command, model::Todo};
use eframe::egui;
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc, task::JoinHandle};

pub mod repository;
pub mod worker;

pub enum DbCommand {
    LoadTodos,
    AddTodo { title: String },
    SetTodoDone { id: i64, done: bool },
    DeleteTodo { id: i64 },
}

pub enum DbEvent {
    TodosLoaded(Vec<Todo>),
    TodoAdded,
    TodoUpdated,
    TodoDeleted,

    Error(String),
}

pub struct DbHandle {
    command_tx: Option<mpsc::UnboundedSender<DbCommand>>,
    event_rx: mpsc::UnboundedReceiver<DbEvent>,
    worker: Option<JoinHandle<()>>,
    runtime: Runtime,
}

impl DbHandle {
    pub fn new(runtime: Runtime, pool: SqlitePool, ctx: egui::Context) -> DbHandle {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel::<DbCommand>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<DbEvent>();

        let worker = runtime.spawn(async move {
            while let Some(command) = command_rx.recv().await {
                let event = handle_command(&pool, command).await;
                if event_tx.send(event).is_ok() {
                    ctx.request_repaint();
                }
            }
        });

        DbHandle {
            command_tx: Some(command_tx),
            event_rx,
            worker: Some(worker),
            runtime,
        }
    }

    pub fn send(&self, command: DbCommand) -> Result<(), DbCommand> {
        match &self.command_tx {
            Some(tx) => tx.send(command).map_err(|e| e.0),
            None => Err(command),
        }
    }

    pub fn try_recv(&mut self) -> Option<DbEvent> {
        self.event_rx.try_recv().ok()
    }
}

impl Drop for DbHandle {
    fn drop(&mut self) {
        // 1. Close channel
        self.command_tx.take();

        // 2. Wait worker
        if let Some(worker) = self.worker.take() {
            let _ = self.runtime.block_on(worker);
        }

        // 3. Destroy runtime
    }
}
