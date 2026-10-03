//! Domain model shared by the database layer and the UI.

use sqlx::FromRow;

/// A todo item, as stored in the `todos` table.
#[derive(Debug, Clone, FromRow)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub done: bool,
}
