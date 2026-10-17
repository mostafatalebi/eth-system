use redis::aio::ConnectionManager;
use crate::auth::repository::RefreshTokenRepository;
use crate::error::CoreError;

#[derive(Clone)]
pub struct RedisRefreshTokenRepository {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisRefreshTokenRepository {
    pub async fn connect(url: &str, prefix: &str) -> Result<Self, CoreError> {
        let client = redis::Client::open(url).map_err(|e| CoreError::RedisFailed(e.to_string()))?;
        let conn = ConnectionManager::new(client).await.map_err(|e| CoreError::RedisFailed(e.to_string()))?;
        Ok(Self { conn, prefix: prefix.trim_end_matches(':').to_string() })
    }

    fn key(&self, token_hash: &[u8]) -> String {
        let hash = alloy::hex::encode(token_hash);
        if self.prefix.is_empty() {
            return format!("refresh_token:{hash}");
        }
        format!("{}:refresh_token:{hash}", self.prefix)
    }
}

impl RefreshTokenRepository for RedisRefreshTokenRepository {
    async fn store(&self, user_id: i64, token_hash: &[u8], ttl_secs: i64) -> Result<(), CoreError> {
        if ttl_secs <= 0 {
            return Ok(());
        }
        let mut conn = self.conn.clone();
        redis::cmd("SET").arg(self.key(token_hash)).arg(user_id).arg("EX").arg(ttl_secs)
            .query_async::<()>(&mut conn).await
            .map_err(|e| CoreError::RedisFailed(e.to_string()))
    }

    async fn consume(&self, token_hash: &[u8]) -> Result<Option<i64>, CoreError> {
        let mut conn = self.conn.clone();
        let user_id: Option<String> = redis::cmd("GETDEL").arg(self.key(token_hash))
            .query_async(&mut conn).await
            .map_err(|e| CoreError::RedisFailed(e.to_string()))?;
        user_id
            .map(|id| id.parse::<i64>().map_err(|e| CoreError::RedisFailed(format!("invalid user id in token entry: {e}"))))
            .transpose()
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use std::time::Duration;
    use alloy::primitives::B256;
    use crate::config::config::Config;

    async fn repo_with_prefix(suffix: &str) -> RedisRefreshTokenRepository {
        let config_file = std::env::var("CONFIG_FILE").expect("CONFIG_FILE must point to a config with a test redis_url");
        let config = Config::load_from_file(config_file).unwrap();
        RedisRefreshTokenRepository::connect(&config.redis_url, &format!("{}_test_{}", config.redis_prefix, suffix)).await.unwrap()
    }

    fn unique_suffix() -> String {
        alloy::hex::encode(&B256::random()[..6])
    }

    // a stored token returns its user once; the second use and an unknown token return nothing
    #[tokio::test]
    async fn test_token_is_single_use() {
        let repo = repo_with_prefix(&unique_suffix()).await;
        repo.store(7, &[1u8; 32], 60).await.unwrap();

        assert_eq!(repo.consume(&[1u8; 32]).await.unwrap(), Some(7));
        assert_eq!(repo.consume(&[1u8; 32]).await.unwrap(), None);
        assert_eq!(repo.consume(&[2u8; 32]).await.unwrap(), None);
    }

    // a token is gone once its ttl has passed
    #[tokio::test]
    async fn test_token_expires() {
        let repo = repo_with_prefix(&unique_suffix()).await;
        repo.store(7, &[1u8; 32], 1).await.unwrap();

        tokio::time::sleep(Duration::from_millis(1500)).await;

        assert_eq!(repo.consume(&[1u8; 32]).await.unwrap(), None);
    }

    // the same token hash under another prefix is a different key
    #[tokio::test]
    async fn test_prefixes_are_isolated() {
        let first = repo_with_prefix(&unique_suffix()).await;
        let second = repo_with_prefix(&unique_suffix()).await;
        first.store(7, &[1u8; 32], 60).await.unwrap();

        assert_eq!(second.consume(&[1u8; 32]).await.unwrap(), None);
        assert_eq!(first.consume(&[1u8; 32]).await.unwrap(), Some(7));
    }
}
