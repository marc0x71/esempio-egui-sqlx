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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repository::initialize;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        initialize(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn load_todos_command_returns_todos_loaded() {
        let pool = test_pool().await;

        let event = handle_command(&pool, DbCommand::LoadTodos).await;

        match event {
            DbEvent::TodosLoaded(todos) => {
                assert!(todos.is_empty());
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }

    #[tokio::test]
    async fn add_todo_command_returns_todo_added() {
        let pool = test_pool().await;

        let event = handle_command(
            &pool,
            DbCommand::AddTodo {
                title: "Buy milk".into(),
            },
        )
        .await;

        assert!(matches!(event, DbEvent::TodoAdded));
    }

    #[tokio::test]
    async fn set_todo_done_command_returns_todo_updated() {
        let pool = test_pool().await;

        let id = crate::db::repository::add_todo(&pool, "Test")
            .await
            .unwrap();

        let event = handle_command(&pool, DbCommand::SetTodoDone { id, done: true }).await;

        assert!(matches!(event, DbEvent::TodoUpdated));
    }

    #[tokio::test]
    async fn delete_todo_command_returns_todo_deleted() {
        let pool = test_pool().await;

        let id = crate::db::repository::add_todo(&pool, "Delete me")
            .await
            .unwrap();

        let event = handle_command(&pool, DbCommand::DeleteTodo { id }).await;

        assert!(matches!(event, DbEvent::TodoDeleted));
    }

    #[tokio::test]
    async fn repository_error_becomes_db_error_event() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // niente initialize()

        let event = handle_command(&pool, DbCommand::LoadTodos).await;

        match event {
            DbEvent::Error(message) => {
                assert!(!message.is_empty());
            }
            other => {
                panic!("unexpected event: {other:?}");
            }
        }
    }
}
