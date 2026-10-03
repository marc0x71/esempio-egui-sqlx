use crate::{
    db::{DBBackend, DbCommand, DbEvent},
    model::Todo,
};

pub struct UpdateResult {
    pub changed: bool,
    pub error: Option<String>,
}

pub struct TodoApp<B: DBBackend> {
    db: B,
    todos: Vec<Todo>,
}

impl<B: DBBackend> TodoApp<B> {
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
