use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub done: bool,
}
