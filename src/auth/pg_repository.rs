use std::sync::Arc;
use tokio_postgres::error::SqlState;
use crate::auth::models::UserModel;
use crate::auth::queries::{USER_BY_ID_QUERY, USER_BY_USERNAME_QUERY, USER_DISABLE_QUERY, USER_INSERT_QUERY};
use crate::auth::repository::UserRepository;
use crate::db::db::DbFactory;
use crate::error::CoreError;

#[derive(Clone)]
pub struct PgUserRepository {
    db: Arc<DbFactory>,
}

impl PgUserRepository {
    pub fn new(db: Arc<DbFactory>) -> Self {
        Self { db }
    }
}

impl UserRepository for PgUserRepository {
    async fn create_user(&self, username: &str, password_hash: &str, roles: &[String]) -> Result<UserModel, CoreError> {
        let pg = self.db.pg().await?;
        let roles = roles.to_vec();
        let row = pg.query_one(USER_INSERT_QUERY, &[&username, &password_hash, &roles]).await
            .map_err(|e| match e.code() {
                Some(&SqlState::UNIQUE_VIOLATION) => CoreError::UserAlreadyExists(username.to_string()),
                _ => CoreError::DbInsertFailed(e.to_string()),
            })?;
        Ok(UserModel::new_from_db_row(&row))
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserModel>, CoreError> {
        let pg = self.db.pg().await?;
        let row = pg.query_opt(USER_BY_USERNAME_QUERY, &[&username]).await
            .map_err(|e| CoreError::DbQueryFailed(e.to_string()))?;
        Ok(row.map(|r| UserModel::new_from_db_row(&r)))
    }

    async fn disable_user(&self, username: &str) -> Result<Option<i64>, CoreError> {
        let pg = self.db.pg().await?;
        let row = pg.query_opt(USER_DISABLE_QUERY, &[&username]).await
            .map_err(|e| CoreError::DbQueryFailed(e.to_string()))?;
        Ok(row.map(|r| r.get("id")))
    }

    async fn get_user_by_id(&self, id: i64) -> Result<Option<UserModel>, CoreError> {
        let pg = self.db.pg().await?;
        let row = pg.query_opt(USER_BY_ID_QUERY, &[&id]).await
            .map_err(|e| CoreError::DbQueryFailed(e.to_string()))?;
        Ok(row.map(|r| UserModel::new_from_db_row(&r)))
    }
}
