use std::future::Future;
use crate::auth::models::UserModel;
use crate::error::CoreError;

pub trait UserRepository: Send + Sync + 'static {
    fn create_user(&self, username: &str, password_hash: &str, roles: &[String]) -> impl Future<Output = Result<UserModel, CoreError>> + Send;

    fn get_user_by_username(&self, username: &str) -> impl Future<Output = Result<Option<UserModel>, CoreError>> + Send;

    fn disable_user(&self, username: &str) -> impl Future<Output = Result<Option<i64>, CoreError>> + Send;

    fn get_user_by_id(&self, id: i64) -> impl Future<Output = Result<Option<UserModel>, CoreError>> + Send;
}

/// where refresh tokens live; only the hash of a token is stored
pub trait RefreshTokenRepository: Send + Sync + 'static {
    fn store(&self, user_id: i64, token_hash: &[u8], ttl_secs: i64) -> impl Future<Output = Result<(), CoreError>> + Send;

    /// returns the user of the token and removes the token, so it can be used only once;
    /// None for an unknown or expired token
    fn consume(&self, token_hash: &[u8]) -> impl Future<Output = Result<Option<i64>, CoreError>> + Send;
}
