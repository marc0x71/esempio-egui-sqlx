use sqlx::SqlitePool;

use crate::db::{
    DbCommand, DbEvent,
    repository::{add_todo, delete_todo, load_todos, set_todo_done},
};

pub async fn handle_command(pool: &SqlitePool, command: DbCommand) -> DbEvent {
    match command {
        DbCommand::LoadTodos => match load_todos(pool).await {
            Ok(todos) => DbEvent::TodosLoaded(todos),
            Err(err) => DbEvent::Error(err.to_string()),
        },

        DbCommand::AddTodo { title } => match add_todo(pool, &title).await {
            Ok(_) => DbEvent::TodoAdded,
            Err(err) => DbEvent::Error(err.to_string()),
        },

        DbCommand::SetTodoDone { id, done } => match set_todo_done(pool, id, done).await {
            Ok(()) => DbEvent::TodoUpdated,
            Err(err) => DbEvent::Error(err.to_string()),
        },

        DbCommand::DeleteTodo { id } => match delete_todo(pool, id).await {
            Ok(()) => DbEvent::TodoDeleted,
            Err(err) => DbEvent::Error(err.to_string()),
        },
    }
}
