use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::auth::models::UserModel;
use crate::auth::repository::UserRepository;
use crate::error::CoreError;

#[derive(Default)]
struct MockUserRepositoryInner {
    users: Vec<UserModel>,
    refresh_tokens: Vec<(Vec<u8>, i64, Instant)>,
}

#[derive(Clone, Default)]
pub struct MockUserRepository {
    inner: Arc<Mutex<MockUserRepositoryInner>>,
}

impl MockUserRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len_users(&self) -> usize {
        self.inner.lock().unwrap().users.len()
    }
}

impl UserRepository for MockUserRepository {
    async fn create_user(&self, username: &str, password_hash: &str, roles: &[String]) -> Result<UserModel, CoreError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.users.iter().any(|u| u.username == username) {
            return Err(CoreError::UserAlreadyExists(username.to_string()));
        }
        let user = UserModel {
            id: inner.users.len() as i64 + 1,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            roles: roles.to_vec(),
            is_active: true,
            created_at: Default::default(),
            updated_at: Default::default(),
        };
        inner.users.push(user.clone());
        Ok(user)
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserModel>, CoreError> {
        Ok(self.inner.lock().unwrap().users.iter().find(|u| u.username == username).cloned())
    }

    async fn disable_user(&self, username: &str) -> Result<Option<i64>, CoreError> {
        let mut inner = self.inner.lock().unwrap();
        let Some(user) = inner.users.iter_mut().find(|u| u.username == username) else {
            return Ok(None);
        };
        user.is_active = false;
        let id = user.id;
        inner.refresh_tokens.retain(|(_, user_id, _)| *user_id != id);
        Ok(Some(id))
    }

    async fn count_users(&self) -> Result<i64, CoreError> {
        Ok(self.inner.lock().unwrap().users.len() as i64)
    }

    async fn get_user_by_id(&self, id: i64) -> Result<Option<UserModel>, CoreError> {
        Ok(self.inner.lock().unwrap().users.iter().find(|u| u.id == id).cloned())
    }

    async fn store_refresh_token(&self, user_id: i64, token_hash: &[u8], ttl_secs: i64) -> Result<(), CoreError> {
        let expires_at = Instant::now() + Duration::from_secs(ttl_secs as u64);
        self.inner.lock().unwrap().refresh_tokens.push((token_hash.to_vec(), user_id, expires_at));
        Ok(())
    }

    async fn consume_refresh_token(&self, token_hash: &[u8]) -> Result<Option<i64>, CoreError> {
        let mut inner = self.inner.lock().unwrap();
        let Some(position) = inner.refresh_tokens.iter().position(|(hash, _, _)| hash == token_hash) else {
            return Ok(None);
        };
        let (_, user_id, expires_at) = inner.refresh_tokens.remove(position);
        Ok((expires_at > Instant::now()).then_some(user_id))
    }
}
