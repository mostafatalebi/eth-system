use std::time::{Duration, SystemTime, UNIX_EPOCH};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use crate::auth::models::UserModel;
use crate::error::CoreError;

const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserData {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub data: UserData,
    pub roles: Vec<String>,
    pub iat: u64,
    pub exp: u64,
}

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    expiry: Duration,
}

impl JwtService {
    pub fn new(secret: &str, expiry: Duration) -> Result<Self, CoreError> {
        if secret.len() < MIN_SECRET_LEN {
            return Err(CoreError::BadArgument(format!("jwt_secret must be at least {MIN_SECRET_LEN} characters")));
        }
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        Ok(Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            validation,
            expiry,
        })
    }

    pub fn issue(&self, user: &UserModel) -> Result<String, CoreError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|e| CoreError::JwtFailed(e.to_string()))?.as_secs();
        let claims = Claims {
            data: UserData { id: user.id, username: user.username.clone() },
            roles: user.roles.clone(),
            iat: now,
            exp: now + self.expiry.as_secs(),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|e| CoreError::JwtFailed(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, CoreError> {
        decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map(|decoded| decoded.claims)
            .map_err(|e| CoreError::Unauthorized(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn user() -> UserModel {
        UserModel {
            id: 7,
            username: "alice".to_string(),
            password_hash: "hash".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            is_active: true,
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }

    // a token carries the user in data, the roles, and an expiry of issued-at plus the configured lifetime
    #[test]
    fn test_issued_token_round_trips_claims() {
        let jwt = JwtService::new(SECRET, Duration::from_secs(60)).unwrap();

        let claims = jwt.verify(&jwt.issue(&user()).unwrap()).unwrap();

        assert_eq!(claims.data, UserData { id: 7, username: "alice".to_string() });
        assert_eq!(claims.roles, vec!["user".to_string(), "admin".to_string()]);
        assert_eq!(claims.exp, claims.iat + 60);
    }

    // a token signed with a different secret is rejected
    #[test]
    fn test_token_signed_with_another_secret_is_rejected() {
        let other = JwtService::new("another-secret-another-secret-12345", Duration::from_secs(60)).unwrap();
        let jwt = JwtService::new(SECRET, Duration::from_secs(60)).unwrap();

        let result = jwt.verify(&other.issue(&user()).unwrap());

        assert!(matches!(result, Err(CoreError::Unauthorized(_))));
    }

    // a token whose expiry is in the past is rejected
    #[test]
    fn test_expired_token_is_rejected() {
        let jwt = JwtService::new(SECRET, Duration::from_secs(60)).unwrap();
        let claims = Claims { data: UserData { id: 7, username: "alice".to_string() }, roles: vec![], iat: 1_000, exp: 1_001 };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &jwt.encoding_key).unwrap();

        let result = jwt.verify(&token);

        assert!(matches!(result, Err(CoreError::Unauthorized(_))));
    }

    // a secret shorter than 32 characters is refused
    #[test]
    fn test_short_secret_is_rejected() {
        let result = JwtService::new("too-short", Duration::from_secs(60));

        assert!(matches!(result, Err(CoreError::BadArgument(_))));
    }
}
