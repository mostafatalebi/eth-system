use std::sync::OnceLock;
use argon2::password_hash::{Error as HashError, PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use crate::error::CoreError;

static DUMMY_HASH: OnceLock<String> = OnceLock::new();

pub async fn hash_password(password: String) -> Result<String, CoreError> {
    tokio::task::spawn_blocking(move || {
        Argon2::default().hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| CoreError::PasswordHashFailed(e.to_string()))
    })
    .await
    .map_err(|e| CoreError::Internal(Box::new(e)))?
}

/// checks the password against the stored hash. With no stored hash (unknown user) it still
/// spends the same time verifying against a dummy hash, then returns false, so a login attempt
/// does not reveal whether the username exists
pub async fn verify_password(password: String, stored_hash: Option<String>) -> Result<bool, CoreError> {
    tokio::task::spawn_blocking(move || {
        let known_user = stored_hash.is_some();
        let hash = match stored_hash {
            Some(hash) => hash,
            None => DUMMY_HASH.get_or_init(|| {
                Argon2::default().hash_password(b"dummy password for timing").map(|h| h.to_string()).unwrap_or_default()
            }).clone(),
        };
        match Argon2::default().verify_password(password.as_bytes(), hash.as_str()) {
            Ok(()) => Ok(known_user),
            Err(HashError::PasswordInvalid) => Ok(false),
            Err(e) => Err(CoreError::PasswordHashFailed(e.to_string())),
        }
    })
    .await
    .map_err(|e| CoreError::Internal(Box::new(e)))?
}

#[cfg(test)]
mod tests {
    use super::*;

    // the stored value is an argon2 hash and not the password
    #[tokio::test]
    async fn test_hash_is_argon2_and_not_plaintext() {
        let hash = hash_password("correct horse".to_string()).await.unwrap();

        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.contains("correct horse"));
    }

    // only the right password verifies, and without a stored hash nothing verifies
    #[tokio::test]
    async fn test_verify_password() {
        let hash = hash_password("correct horse".to_string()).await.unwrap();

        assert!(verify_password("correct horse".to_string(), Some(hash.clone())).await.unwrap());
        assert!(!verify_password("wrong horse".to_string(), Some(hash)).await.unwrap());
        assert!(!verify_password("correct horse".to_string(), None).await.unwrap());
    }
}
