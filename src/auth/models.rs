use chrono::NaiveDateTime;
use tokio_postgres::Row;

#[derive(Debug, Clone)]
pub struct UserModel {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl UserModel {
    pub fn new_from_db_row(row: &Row) -> UserModel {
        Self {
            id: row.get("id"),
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            roles: row.get("roles"),
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}
