use crate::{
    db::{DbBackend, DbCommand, DbEvent},
    model::Todo,
};

pub struct UpdateResult {
    pub changed: bool,
    pub error: Option<String>,
}

pub struct TodoApp<B: DbBackend> {
    db: B,
    todos: Vec<Todo>,
}

impl<B: DbBackend> TodoApp<B> {
    pub fn new(db: B) -> Self {
        let mut app = Self {
            db,
            todos: Vec::new(),
        };

        app.load_todos();

        app
    }

    fn load_todos(&mut self) {
        let _ = self.db.send(DbCommand::LoadTodos);
    }

    pub(crate) fn add_todo(&self, new_title: String) {
        let title = new_title.trim();

        if title.is_empty() {
            return;
        }

        let _ = self.db.send(DbCommand::AddTodo {
            title: title.to_owned(),
        });
    }

    pub(crate) fn update(&mut self) -> UpdateResult {
        let mut changed = false;
        let mut error = None;
        while let Some(event) = self.db.try_recv() {
            match event {
                DbEvent::TodosLoaded(todos) => {
                    self.todos = todos;
                    changed = true;
                }
                DbEvent::TodoAdded | DbEvent::TodoUpdated | DbEvent::TodoDeleted => {
                    self.load_todos();
                }
                DbEvent::Error(err) => {
                    error = Some(err);
                    changed = true;
                }
            }
        }
        UpdateResult { changed, error }
    }

    pub(crate) fn set_todo_done(&self, id: i64, done: bool) {
        let _ = self.db.send(DbCommand::SetTodoDone { id, done });
    }

    pub(crate) fn delete_todo(&self, id: i64) {
        let _ = self.db.send(DbCommand::DeleteTodo { id });
    }

    pub fn todos(&self) -> &[Todo] {
        &self.todos
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use super::*;
    use crate::db::{DbBackend, DbCommand, DbEvent};

    struct FakeDb {
        commands: Rc<RefCell<Vec<DbCommand>>>,
        events: VecDeque<DbEvent>,
    }

    impl FakeDb {
        fn new() -> (Self, Rc<RefCell<Vec<DbCommand>>>) {
            let commands = Rc::new(RefCell::new(Vec::new()));

            (
                Self {
                    commands: Rc::clone(&commands),
                    events: VecDeque::new(),
                },
                commands,
            )
        }

        fn with_events(
            events: impl IntoIterator<Item = DbEvent>,
        ) -> (Self, Rc<RefCell<Vec<DbCommand>>>) {
            let commands = Rc::new(RefCell::new(Vec::new()));

            (
                Self {
                    commands: Rc::clone(&commands),
                    events: events.into_iter().collect(),
                },
                commands,
            )
        }
    }

    impl DbBackend for FakeDb {
        fn send(&self, command: DbCommand) -> Result<(), DbCommand> {
            self.commands.borrow_mut().push(command);
            Ok(())
        }

        fn try_recv(&mut self) -> Option<DbEvent> {
            self.events.pop_front()
        }
    }

    #[test]
    fn new_requests_initial_todo_load() {
        let (db, commands) = FakeDb::new();

        let _app = TodoApp::new(db);

        let commands = commands.borrow();

        assert_eq!(commands.len(), 1);
        assert!(matches!(commands.first(), Some(DbCommand::LoadTodos)));
    }

    #[test]
    fn add_todo_sends_add_command() {
        let (db, commands) = FakeDb::new();
        let app = TodoApp::new(db);

        app.add_todo("Buy milk".into());

        let commands = commands.borrow();

        assert!(matches!(
            commands.last(),
            Some(DbCommand::AddTodo { title })
                if title == "Buy milk"
        ));
    }

    #[test]
    fn add_todo_trims_title() {
        let (db, commands) = FakeDb::new();
        let app = TodoApp::new(db);

        app.add_todo("   Buy milk   ".into());

        let commands = commands.borrow();

        assert!(matches!(
            commands.last(),
            Some(DbCommand::AddTodo { title })
                if title == "Buy milk"
        ));
    }

    #[test]
    fn add_todo_ignores_empty_title() {
        let (db, commands) = FakeDb::new();
        let app = TodoApp::new(db);

        let before = commands.borrow().len();

        app.add_todo("    ".into());

        let after = commands.borrow().len();

        assert_eq!(before, after);
    }

    #[test]
    fn set_todo_done_sends_update_command() {
        let (db, commands) = FakeDb::new();
        let app = TodoApp::new(db);

        app.set_todo_done(42, true);

        let commands = commands.borrow();

        assert!(matches!(
            commands.last(),
            Some(DbCommand::SetTodoDone { id: 42, done: true })
        ));
    }

    #[test]
    fn delete_todo_sends_delete_command() {
        let (db, commands) = FakeDb::new();
        let app = TodoApp::new(db);

        app.delete_todo(42);

        let commands = commands.borrow();

        assert!(matches!(
            commands.last(),
            Some(DbCommand::DeleteTodo { id: 42 })
        ));
    }

    #[test]
    fn update_replaces_todos_when_loaded() {
        let todos = vec![
            Todo {
                id: 1,
                title: "Buy milk".into(),
                done: false,
            },
            Todo {
                id: 2,
                title: "Learn Rust".into(),
                done: true,
            },
        ];

        let (db, _commands) = FakeDb::with_events([DbEvent::TodosLoaded(todos)]);

        let mut app = TodoApp::new(db);

        let result = app.update();

        assert!(result.changed);
        assert!(result.error.is_none());

        assert_eq!(app.todos().len(), 2);

        assert_eq!(app.todos()[0].id, 1);
        assert_eq!(app.todos()[0].title, "Buy milk");
        assert!(!app.todos()[0].done);

        assert_eq!(app.todos()[1].id, 2);
        assert_eq!(app.todos()[1].title, "Learn Rust");
        assert!(app.todos()[1].done);
    }

    #[test]
    fn update_returns_database_error() {
        let (db, _commands) = FakeDb::with_events([DbEvent::Error("database error".into())]);

        let mut app = TodoApp::new(db);

        let result = app.update();

        assert!(result.changed);
        assert_eq!(result.error.as_deref(), Some("database error"));
    }

    #[test]
    fn todo_added_triggers_reload() {
        let (db, commands) = FakeDb::with_events([DbEvent::TodoAdded]);

        let mut app = TodoApp::new(db);

        assert_eq!(commands.borrow().len(), 1);

        let result = app.update();

        assert_eq!(commands.borrow().len(), 2);

        let commands = commands.borrow();

        assert!(matches!(commands.last(), Some(DbCommand::LoadTodos)));

        assert!(!result.changed);
    }

    #[test]
    fn todo_updated_triggers_reload() {
        let (db, commands) = FakeDb::with_events([DbEvent::TodoUpdated]);

        let mut app = TodoApp::new(db);

        app.update();

        let commands = commands.borrow();

        assert_eq!(commands.len(), 2);
        assert!(matches!(commands.last(), Some(DbCommand::LoadTodos)));
    }

    #[test]
    fn todo_deleted_triggers_reload() {
        let (db, commands) = FakeDb::with_events([DbEvent::TodoDeleted]);

        let mut app = TodoApp::new(db);

        app.update();

        let commands = commands.borrow();

        assert_eq!(commands.len(), 2);
        assert!(matches!(commands.last(), Some(DbCommand::LoadTodos)));
    }

    #[test]
    fn update_processes_multiple_events() {
        let todos = vec![Todo {
            id: 1,
            title: "Test".into(),
            done: false,
        }];

        let (db, _commands) = FakeDb::with_events([
            DbEvent::TodosLoaded(todos),
            DbEvent::Error("something went wrong".into()),
        ]);

        let mut app = TodoApp::new(db);

        let result = app.update();

        assert!(result.changed);
        assert_eq!(app.todos().len(), 1);
        assert_eq!(result.error.as_deref(), Some("something went wrong"));
    }
}
