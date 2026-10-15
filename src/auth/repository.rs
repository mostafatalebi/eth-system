use std::future::Future;
use crate::auth::models::UserModel;
use crate::error::CoreError;

pub trait UserRepository: Send + Sync + 'static {
    fn create_user(&self, username: &str, password_hash: &str, roles: &[String]) -> impl Future<Output = Result<UserModel, CoreError>> + Send;

    fn get_user_by_username(&self, username: &str) -> impl Future<Output = Result<Option<UserModel>, CoreError>> + Send;

    fn disable_user(&self, username: &str) -> impl Future<Output = Result<Option<i64>, CoreError>> + Send;

    fn count_users(&self) -> impl Future<Output = Result<i64, CoreError>> + Send;

    fn get_user_by_id(&self, id: i64) -> impl Future<Output = Result<Option<UserModel>, CoreError>> + Send;

    fn store_refresh_token(&self, user_id: i64, token_hash: &[u8], ttl_secs: i64) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn consume_refresh_token(&self, token_hash: &[u8]) -> impl Future<Output = Result<Option<i64>, CoreError>> + Send;
}
