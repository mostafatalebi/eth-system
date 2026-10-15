use validator::Validate;
use crate::auth::handlers::SignupRequest;
use crate::auth::models::UserModel;
use crate::auth::password::hash_password;
use crate::auth::repository::UserRepository;
use crate::error::CoreError;

const MAX_ROLE_LEN: usize = 32;

/// creates a user with the given roles; used by --mode create-user, the only way to create users
/// with roles other than the default ones
pub async fn create_user_command<U: UserRepository>(users: &U, username: &str, password: String, roles: &[String]) -> Result<UserModel, CoreError> {
    let request = SignupRequest::new(username, password);
    request.validate().map_err(|e| CoreError::ValidationFailed(e.to_string()))?;
    let roles = normalize_roles(roles)?;

    let password_hash = hash_password(request.password).await?;
    users.create_user(&request.username, &password_hash, &roles).await
}

/// disables a user: it can no longer log in or refresh, and its refresh tokens are revoked.
/// Access tokens already issued stay valid until they expire
pub async fn disable_user_command<U: UserRepository>(users: &U, username: &str) -> Result<(), CoreError> {
    let username = username.trim().to_lowercase();
    match users.disable_user(&username).await? {
        Some(_) => Ok(()),
        None => Err(CoreError::DbNotFound(format!("user {username}"))),
    }
}

fn normalize_roles(roles: &[String]) -> Result<Vec<String>, CoreError> {
    let mut normalized: Vec<String> = Vec::new();
    for role in roles {
        let role = role.trim().to_lowercase();
        let valid = !role.is_empty()
            && role.len() <= MAX_ROLE_LEN
            && role.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));
        if !valid {
            return Err(CoreError::ValidationFailed(format!("invalid role {role:?}: use 1 to {MAX_ROLE_LEN} characters from a-z, 0-9, '_' and '-'")));
        }
        if !normalized.contains(&role) {
            normalized.push(role);
        }
    }
    if normalized.is_empty() {
        return Err(CoreError::ValidationFailed("at least one role is required".to_string()));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::mock_repository::MockUserRepository;

    // a user is created with a hashed password and its roles trimmed, lowercased and deduplicated
    #[tokio::test]
    async fn test_create_user_stores_hashed_password_and_normalized_roles() {
        let users = MockUserRepository::new();
        let roles = vec!["Admin".to_string(), " user ".to_string(), "admin".to_string()];

        let user = create_user_command(&users, "Alice", "long-enough-password".to_string(), &roles).await.unwrap();

        assert_eq!(user.username, "alice");
        assert_eq!(user.roles, vec!["admin".to_string(), "user".to_string()]);
        assert!(user.password_hash.starts_with("$argon2id$"));
    }

    // a weak password, an invalid role or an empty role list is rejected and nothing is created
    #[tokio::test]
    async fn test_create_user_rejects_invalid_input() {
        let users = MockUserRepository::new();
        let user_role = vec!["user".to_string()];

        let weak_password = create_user_command(&users, "alice", "short".to_string(), &user_role).await;
        let bad_role = create_user_command(&users, "alice", "long-enough-password".to_string(), &["bad role!".to_string()]).await;
        let no_roles = create_user_command(&users, "alice", "long-enough-password".to_string(), &[]).await;

        assert!(matches!(weak_password, Err(CoreError::ValidationFailed(_))));
        assert!(matches!(bad_role, Err(CoreError::ValidationFailed(_))));
        assert!(matches!(no_roles, Err(CoreError::ValidationFailed(_))));
        assert_eq!(users.len_users(), 0);
    }

    // disabling a user marks it inactive, and an unknown username is an error
    #[tokio::test]
    async fn test_disable_user() {
        let users = MockUserRepository::new();
        create_user_command(&users, "alice", "long-enough-password".to_string(), &["user".to_string()]).await.unwrap();

        disable_user_command(&users, " Alice ").await.unwrap();
        let unknown = disable_user_command(&users, "nobody").await;

        assert!(!users.get_user_by_username("alice").await.unwrap().unwrap().is_active);
        assert!(matches!(unknown, Err(CoreError::DbNotFound(_))));
    }

    // creating the same username twice fails with UserAlreadyExists
    #[tokio::test]
    async fn test_create_user_rejects_duplicate_username() {
        let users = MockUserRepository::new();
        let roles = vec!["user".to_string()];
        create_user_command(&users, "alice", "long-enough-password".to_string(), &roles).await.unwrap();

        let result = create_user_command(&users, "alice", "long-enough-password".to_string(), &roles).await;

        assert!(matches!(result, Err(CoreError::UserAlreadyExists(_))));
    }
}
