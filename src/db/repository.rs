//! SQL queries, one async function per operation.

use std::{path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::model::Todo;

/// Migrations from the `migrations/` folder, embedded at compile time.
static MIGRATOR: Migrator = sqlx::migrate!();

/// Opens the SQLite database at `path`, creating it if missing.
///
/// Uses WAL journaling and a 5-second busy timeout, the recommended
/// settings for a desktop application.
pub async fn create_pool(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

/// Applies any pending migrations. Safe to call on every startup.
pub async fn initialize(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// Returns all todos, newest first.
pub async fn load_todos(pool: &SqlitePool) -> Result<Vec<Todo>, sqlx::Error> {
    sqlx::query_as::<_, Todo>(
        r#"
        SELECT id, title, done
        FROM todos
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Inserts a new, not yet completed todo and returns its id.
pub async fn add_todo(pool: &SqlitePool, title: &str) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO todos (title, done)
        VALUES (?, false)
        "#,
    )
    .bind(title)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Marks a todo as done or not done. Does nothing if `id` doesn't exist.
pub async fn set_todo_done(pool: &SqlitePool, id: i64, done: bool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE todos
        SET done = ?
        WHERE id = ?
        "#,
    )
    .bind(done)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Deletes a todo. Does nothing if `id` doesn't exist.
pub async fn delete_todo(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create in-memory SQLite database");

        initialize(&pool)
            .await
            .expect("failed to initialize test database");

        pool
    }

    #[tokio::test]
    async fn initialize_creates_todos_table() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        initialize(&pool).await.unwrap();

        let exists: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name = 'todos'
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(exists.0, 1);
    }

    #[tokio::test]
    async fn initialize_can_be_called_multiple_times() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        initialize(&pool).await.unwrap();
        initialize(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn load_todos_returns_empty_list_initially() {
        let pool = test_pool().await;

        let todos = load_todos(&pool).await.unwrap();

        assert!(todos.is_empty());
    }

    #[tokio::test]
    async fn add_todo_inserts_todo() {
        let pool = test_pool().await;

        let id = add_todo(&pool, "Buy milk").await.unwrap();

        assert!(id > 0);

        let todos = load_todos(&pool).await.unwrap();

        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].id, id);
        assert_eq!(todos[0].title, "Buy milk");
        assert!(!todos[0].done);
    }

    #[tokio::test]
    async fn add_todo_generates_different_ids() {
        let pool = test_pool().await;

        let first_id = add_todo(&pool, "First").await.unwrap();
        let second_id = add_todo(&pool, "Second").await.unwrap();

        assert_ne!(first_id, second_id);
        assert!(second_id > first_id);
    }

    #[tokio::test]
    async fn load_todos_returns_newest_first() {
        let pool = test_pool().await;

        let first_id = add_todo(&pool, "First").await.unwrap();
        let second_id = add_todo(&pool, "Second").await.unwrap();
        let third_id = add_todo(&pool, "Third").await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        assert_eq!(todos.len(), 3);

        assert_eq!(todos[0].id, third_id);
        assert_eq!(todos[0].title, "Third");

        assert_eq!(todos[1].id, second_id);
        assert_eq!(todos[1].title, "Second");

        assert_eq!(todos[2].id, first_id);
        assert_eq!(todos[2].title, "First");
    }

    #[tokio::test]
    async fn set_todo_done_marks_todo_as_done() {
        let pool = test_pool().await;

        let id = add_todo(&pool, "Learn Rust").await.unwrap();

        set_todo_done(&pool, id, true).await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        assert_eq!(todos.len(), 1);
        assert!(todos[0].done);
    }

    #[tokio::test]
    async fn set_todo_done_can_mark_todo_as_not_done_again() {
        let pool = test_pool().await;

        let id = add_todo(&pool, "Learn Rust").await.unwrap();

        set_todo_done(&pool, id, true).await.unwrap();
        set_todo_done(&pool, id, false).await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        assert_eq!(todos.len(), 1);
        assert!(!todos[0].done);
    }

    #[tokio::test]
    async fn set_todo_done_only_updates_requested_todo() {
        let pool = test_pool().await;

        let first_id = add_todo(&pool, "First").await.unwrap();
        let second_id = add_todo(&pool, "Second").await.unwrap();

        set_todo_done(&pool, first_id, true).await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        let first = todos.iter().find(|todo| todo.id == first_id).unwrap();

        let second = todos.iter().find(|todo| todo.id == second_id).unwrap();

        assert!(first.done);
        assert!(!second.done);
    }

    #[tokio::test]
    async fn delete_todo_removes_todo() {
        let pool = test_pool().await;

        let id = add_todo(&pool, "Delete me").await.unwrap();

        delete_todo(&pool, id).await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        assert!(todos.is_empty());
    }

    #[tokio::test]
    async fn delete_todo_only_removes_requested_todo() {
        let pool = test_pool().await;

        let first_id = add_todo(&pool, "First").await.unwrap();
        let second_id = add_todo(&pool, "Second").await.unwrap();

        delete_todo(&pool, first_id).await.unwrap();

        let todos = load_todos(&pool).await.unwrap();

        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].id, second_id);
        assert_eq!(todos[0].title, "Second");
    }

    #[tokio::test]
    async fn deleting_unknown_todo_does_not_fail() {
        let pool = test_pool().await;

        let result = delete_todo(&pool, 999_999).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn updating_unknown_todo_does_not_fail() {
        let pool = test_pool().await;

        let result = set_todo_done(&pool, 999_999, true).await;

        assert!(result.is_ok());
    }
}
