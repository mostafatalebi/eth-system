use tracing::{info, warn};
use crate::auth::cli::create_user_command;
use crate::auth::models::UserModel;
use crate::auth::repository::UserRepository;
use crate::config::config::Config;
use crate::error::CoreError;

/// creates the configured default user, but only when default_user_enabled is set and no user
/// exists yet, so it never brings back a default account that was removed or replaced later
pub async fn seed_default_user<U: UserRepository>(users: &U, config: &Config) -> Result<Option<UserModel>, CoreError> {
    if !config.default_user_enabled {
        return Ok(None);
    }
    if users.count_users().await? > 0 {
        info!("users already exist, skipping the default user");
        return Ok(None);
    }

    let user = create_user_command(users, &config.default_user_username, config.default_user_password.clone(), &config.default_user_roles).await?;
    warn!("created default user {} with roles {:?}: change or remove it before exposing the API", user.username, user.roles);
    Ok(Some(user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::mock_repository::MockUserRepository;

    fn config(enabled: bool) -> Config {
        let mut config = Config::default();
        config.default_user_enabled = enabled;
        config.default_user_username = "root".to_string();
        config.default_user_password = "a-long-password".to_string();
        config.default_user_roles = vec!["admin".to_string(), "user".to_string()];
        config
    }

    // nothing is created while default_user_enabled is false
    #[tokio::test]
    async fn test_disabled_default_user_is_not_created() {
        let users = MockUserRepository::new();

        let result = seed_default_user(&users, &config(false)).await.unwrap();

        assert!(result.is_none());
        assert_eq!(users.len_users(), 0);
    }

    // with an empty users table the default user is created with the configured roles and a hashed password
    #[tokio::test]
    async fn test_default_user_is_created_in_an_empty_table() {
        let users = MockUserRepository::new();

        let user = seed_default_user(&users, &config(true)).await.unwrap().unwrap();

        assert_eq!(user.username, "root");
        assert_eq!(user.roles, vec!["admin".to_string(), "user".to_string()]);
        assert!(user.password_hash.starts_with("$argon2id$"));
    }

    // when a user already exists nothing is created, even if the default user itself is gone
    #[tokio::test]
    async fn test_default_user_is_not_created_when_users_exist() {
        let users = MockUserRepository::new();
        users.create_user("someone", "hash", &["user".to_string()]).await.unwrap();

        let result = seed_default_user(&users, &config(true)).await.unwrap();

        assert!(result.is_none());
        assert_eq!(users.len_users(), 1);
    }

    // a disabled default user still counts as an existing user, so it is not created again
    #[tokio::test]
    async fn test_disabled_default_user_is_not_created_again() {
        let users = MockUserRepository::new();
        seed_default_user(&users, &config(true)).await.unwrap();
        users.disable_user("root").await.unwrap();

        let result = seed_default_user(&users, &config(true)).await.unwrap();

        assert!(result.is_none());
        assert_eq!(users.len_users(), 1);
    }

    // an enabled default user without a usable password is an error and nothing is created
    #[tokio::test]
    async fn test_default_user_with_empty_password_is_rejected() {
        let users = MockUserRepository::new();
        let mut config = config(true);
        config.default_user_password = String::new();

        let result = seed_default_user(&users, &config).await;

        assert!(matches!(result, Err(CoreError::ValidationFailed(_))));
        assert_eq!(users.len_users(), 0);
    }
}
