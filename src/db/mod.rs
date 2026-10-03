use crate::{db::worker::handle_command, model::Todo};
use eframe::egui;
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc, task::JoinHandle};

pub mod repository;
pub mod worker;

#[derive(Debug)]
pub enum DbCommand {
    LoadTodos,
    AddTodo { title: String },
    SetTodoDone { id: i64, done: bool },
    DeleteTodo { id: i64 },
}

#[derive(Debug)]
pub enum DbEvent {
    TodosLoaded(Vec<Todo>),
    TodoAdded,
    TodoUpdated,
    TodoDeleted,

    Error(String),
}

pub trait DBBackend {
    fn send(&self, command: DbCommand) -> Result<(), DbCommand>;
    fn try_recv(&mut self) -> Option<DbEvent>;
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
}

impl DBBackend for DbHandle {
    fn send(&self, command: DbCommand) -> Result<(), DbCommand> {
        match &self.command_tx {
            Some(tx) => tx.send(command).map_err(|e| e.0),
            None => Err(command),
        }
    }

    fn try_recv(&mut self) -> Option<DbEvent> {
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

#[cfg(test)]
mod test {
    use crate::db::repository::initialize;

    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn test_db_handle() -> DbHandle {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let pool = runtime.block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();

            initialize(&pool).await.unwrap();

            pool
        });

        DbHandle::new(runtime, pool, egui::Context::default())
    }

    fn wait_for_event(db: &mut DbHandle) -> DbEvent {
        for _ in 0..100 {
            if let Some(event) = db.try_recv() {
                return event;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("timeout waiting for database event");
    }

    #[test]
    fn load_todos_returns_empty_list() {
        let mut db = test_db_handle();

        db.send(DbCommand::LoadTodos).unwrap();
        let event = wait_for_event(&mut db);

        match event {
            DbEvent::TodosLoaded(todos) => {
                assert!(todos.is_empty());
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }

    #[test]
    fn add_todo_emits_todo_added() {
        let mut db = test_db_handle();

        db.send(DbCommand::AddTodo {
            title: "Buy milk".into(),
        })
        .unwrap();

        let event = wait_for_event(&mut db);

        assert!(matches!(event, DbEvent::TodoAdded));
    }

    #[test]
    fn add_todo_persists_todo() {
        let mut db = test_db_handle();

        db.send(DbCommand::AddTodo {
            title: "Buy milk".into(),
        })
        .unwrap();

        assert!(matches!(wait_for_event(&mut db), DbEvent::TodoAdded));

        db.send(DbCommand::LoadTodos).unwrap();

        let event = wait_for_event(&mut db);

        match event {
            DbEvent::TodosLoaded(todos) => {
                assert_eq!(todos.len(), 1);
                assert_eq!(todos[0].title, "Buy milk");
                assert!(!todos[0].done);
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }

    #[test]
    fn set_todo_done_updates_todo() {
        let mut db = test_db_handle();

        db.send(DbCommand::AddTodo {
            title: "Learn Rust".into(),
        })
        .unwrap();

        assert!(matches!(wait_for_event(&mut db), DbEvent::TodoAdded));

        db.send(DbCommand::LoadTodos).unwrap();

        let todo = match wait_for_event(&mut db) {
            DbEvent::TodosLoaded(mut todos) => todos.remove(0),
            other => {
                panic!("unexpected event: {other:?}");
            }
        };

        db.send(DbCommand::SetTodoDone {
            id: todo.id,
            done: true,
        })
        .unwrap();

        assert!(matches!(wait_for_event(&mut db), DbEvent::TodoUpdated));

        db.send(DbCommand::LoadTodos).unwrap();

        match wait_for_event(&mut db) {
            DbEvent::TodosLoaded(todos) => {
                assert_eq!(todos.len(), 1);
                assert!(todos[0].done);
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }

    #[test]
    fn delete_todo_removes_todo() {
        let mut db = test_db_handle();

        db.send(DbCommand::AddTodo {
            title: "Temporary todo".into(),
        })
        .unwrap();

        assert!(matches!(wait_for_event(&mut db), DbEvent::TodoAdded));

        db.send(DbCommand::LoadTodos).unwrap();

        let id = match wait_for_event(&mut db) {
            DbEvent::TodosLoaded(todos) => todos[0].id,
            other => {
                panic!("unexpected event: {other:?}");
            }
        };

        db.send(DbCommand::DeleteTodo { id }).unwrap();

        assert!(matches!(wait_for_event(&mut db), DbEvent::TodoDeleted));

        db.send(DbCommand::LoadTodos).unwrap();

        match wait_for_event(&mut db) {
            DbEvent::TodosLoaded(todos) => {
                assert!(todos.is_empty());
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }
}
