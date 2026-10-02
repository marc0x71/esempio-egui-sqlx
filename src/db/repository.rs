use std::{str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::model::Todo;

pub async fn create_pool() -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str("sqlite://app.db")?
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

pub async fn initialize(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            done BOOLEAN NOT NULL DEFAULT FALSE
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

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
