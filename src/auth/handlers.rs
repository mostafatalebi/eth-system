use std::sync::Arc;
use std::time::Duration;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::error;
use validator::{Validate, ValidationError, ValidationErrors};
use crate::auth::jwt::JwtService;
use crate::auth::models::UserModel;
use crate::auth::password::{hash_password, verify_password};
use crate::auth::refresh::{hash_refresh_token, new_refresh_token};
use crate::auth::repository::{RefreshTokenRepository, UserRepository};
use crate::error::CoreError;
use crate::http_server::response::{ApiResponse, ErrorResponse};

const DEFAULT_ROLES: [&str; 1] = ["user"];

pub struct AuthState<U: UserRepository, T: RefreshTokenRepository> {
    pub users: U,
    pub tokens: T,
    pub jwt: Arc<JwtService>,
    pub refresh_token_expiry: Duration,
    pub enable_signup: bool,
}

impl<U: UserRepository, T: RefreshTokenRepository> AuthState<U, T> {
    async fn issue_tokens(&self, user: &UserModel) -> Result<TokenPair, CoreError> {
        let access_token = self.jwt.issue(user)?;
        let (refresh_token, refresh_token_hash) = new_refresh_token();
        self.tokens.store(user.id, refresh_token_hash.as_slice(), self.refresh_token_expiry.as_secs() as i64).await?;
        Ok(TokenPair { access_token, refresh_token })
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SignupRequest {
    #[validate(length(min = 3, max = 32, message = "username must be 3 to 32 characters"), custom(function = validate_username_chars))]
    pub username: String,
    #[validate(length(min = 8, max = 128, message = "password must be 8 to 128 characters"))]
    pub password: String,
}

impl SignupRequest {
    pub fn new(username: &str, password: String) -> Self {
        let mut request = Self { username: username.to_string(), password };
        request.normalize();
        request
    }

    pub fn normalize(&mut self) {
        self.username = self.username.trim().to_lowercase();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    refresh_token: String,
}

#[derive(Serialize)]
pub struct TokenPair {
    access_token: String,
    refresh_token: String,
}

fn validate_username_chars(username: &str) -> Result<(), ValidationError> {
    if username.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.')) {
        return Ok(());
    }
    Err(ValidationError::new("username_chars").with_message("username may only contain letters, digits, '_', '-' and '.'".into()))
}

pub struct AuthHandlers;

impl AuthHandlers {
    pub fn get_routes<U: UserRepository, T: RefreshTokenRepository>(state: Arc<AuthState<U, T>>) -> Router {
        let mut router = Router::new()
            .route("/auth/login", post(Self::login::<U, T>))
            .route("/auth/refresh", post(Self::refresh::<U, T>));
        if state.enable_signup {
            router = router.route("/auth/signup", post(Self::signup::<U, T>));
        }
        router.with_state(state)
    }

    /// POST /auth/signup; creates a user with the default roles and returns its tokens
    pub async fn signup<U: UserRepository, T: RefreshTokenRepository>(State(state): State<Arc<AuthState<U, T>>>, payload: Result<Json<SignupRequest>, JsonRejection>)
        -> ApiResponse<TokenPair> {
        let Ok(Json(mut request)) = payload else {
            return Self::error_response(StatusCode::BAD_REQUEST, "invalid request body");
        };
        request.normalize();
        if let Err(e) = request.validate() {
            return Self::validation_response(e);
        }

        let password_hash = match hash_password(request.password).await {
            Ok(hash) => hash,
            Err(e) => return Self::internal_error("signup", e),
        };
        let roles: Vec<String> = DEFAULT_ROLES.iter().map(|r| r.to_string()).collect();
        let user = match state.users.create_user(&request.username, &password_hash, &roles).await {
            Ok(user) => user,
            Err(CoreError::UserAlreadyExists(_)) => return Self::error_response(StatusCode::CONFLICT, "username is already taken"),
            Err(e) => return Self::internal_error("signup", e),
        };

        match state.issue_tokens(&user).await {
            Ok(tokens) => {
                let mut response = ApiResponse::for_success(Some(tokens));
                response.set_status_code(StatusCode::CREATED);
                response
            }
            Err(e) => Self::internal_error("signup", e),
        }
    }

    /// POST /auth/login; returns new tokens for a correct username and password
    pub async fn login<U: UserRepository, T: RefreshTokenRepository>(State(state): State<Arc<AuthState<U, T>>>, payload: Result<Json<LoginRequest>, JsonRejection>)
        -> ApiResponse<TokenPair> {
        let Ok(Json(request)) = payload else {
            return Self::error_response(StatusCode::BAD_REQUEST, "invalid request body");
        };
        let username = request.username.trim().to_lowercase();

        let user = match state.users.get_user_by_username(&username).await {
            Ok(user) => user,
            Err(e) => return Self::internal_error("login", e),
        };
        let password_ok = match verify_password(request.password, user.as_ref().map(|u| u.password_hash.clone())).await {
            Ok(ok) => ok,
            Err(e) => return Self::internal_error("login", e),
        };
        let Some(user) = user.filter(|u| password_ok && u.is_active) else {
            return Self::error_response(StatusCode::UNAUTHORIZED, "invalid username or password");
        };

        match state.issue_tokens(&user).await {
            Ok(tokens) => ApiResponse::for_success(Some(tokens)),
            Err(e) => Self::internal_error("login", e),
        }
    }

    /// POST /auth/refresh; exchanges a refresh token for new tokens. The refresh token is
    /// single use: it is consumed here and a new one is returned
    pub async fn refresh<U: UserRepository, T: RefreshTokenRepository>(State(state): State<Arc<AuthState<U, T>>>, payload: Result<Json<RefreshRequest>, JsonRejection>)
        -> ApiResponse<TokenPair> {
        let Ok(Json(request)) = payload else {
            return Self::error_response(StatusCode::BAD_REQUEST, "invalid request body");
        };
        let invalid = || Self::error_response(StatusCode::UNAUTHORIZED, "invalid or expired refresh token");

        let token_hash = hash_refresh_token(&request.refresh_token);
        let user_id = match state.tokens.consume(token_hash.as_slice()).await {
            Ok(Some(user_id)) => user_id,
            Ok(None) => return invalid(),
            Err(e) => return Self::internal_error("refresh", e),
        };
        let user = match state.users.get_user_by_id(user_id).await {
            Ok(Some(user)) if user.is_active => user,
            Ok(_) => return invalid(),
            Err(e) => return Self::internal_error("refresh", e),
        };

        match state.issue_tokens(&user).await {
            Ok(tokens) => ApiResponse::for_success(Some(tokens)),
            Err(e) => Self::internal_error("refresh", e),
        }
    }

    fn error_response(status: StatusCode, msg: &str) -> ApiResponse<TokenPair> {
        let mut response = ApiResponse::for_error(msg.to_string());
        response.set_status_code(status);
        response
    }

    fn internal_error(action: &str, e: CoreError) -> ApiResponse<TokenPair> {
        error!("{action} failed: {e}");
        ApiResponse::for_error("internal error".to_string())
    }

    fn validation_response(errors: ValidationErrors) -> ApiResponse<TokenPair> {
        let mut error_response = ErrorResponse::new("validation error".to_string());
        for (field, field_errors) in errors.field_errors() {
            let msg = field_errors.first()
                .and_then(|e| e.message.as_ref().map(|m| m.to_string()))
                .unwrap_or_else(|| "invalid value".to_string());
            error_response.add_field_error(field.to_string(), msg);
        }
        let mut response = ApiResponse::default();
        response.set_status_code(StatusCode::BAD_REQUEST);
        response.set_error(error_response);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};
    use serde_json::{json, Value};
    use tower::ServiceExt;
    use crate::auth::mock_repository::{MockRefreshTokenRepository, MockUserRepository};

    const PASSWORD: &str = "long-enough-password";

    fn setup(enable_signup: bool, refresh_token_expiry: Duration) -> (Router, MockUserRepository, Arc<JwtService>) {
        let users = MockUserRepository::new();
        let jwt = Arc::new(JwtService::new("0123456789abcdef0123456789abcdef", Duration::from_secs(60)).unwrap());
        let state = Arc::new(AuthState { users: users.clone(), tokens: MockRefreshTokenRepository::new(), jwt: jwt.clone(), refresh_token_expiry, enable_signup });
        (AuthHandlers::get_routes(state), users, jwt)
    }

    fn default_setup() -> (Router, MockUserRepository, Arc<JwtService>) {
        setup(true, Duration::from_secs(60))
    }

    async fn post_json(router: Router, path: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder().method("POST").uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())).unwrap();
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn signed_up_user(router: &Router) -> Value {
        let (_, body) = post_json(router.clone(), "/auth/signup", json!({"username": "alice", "password": PASSWORD})).await;
        body["data"].clone()
    }

    // signing up returns 201 with an access token for the lowercased username with only the default "user" role, and a refresh token
    #[tokio::test]
    async fn test_signup_returns_tokens_with_default_role() {
        let (router, users, jwt) = default_setup();

        let (status, body) = post_json(router, "/auth/signup", json!({"username": " Alice ", "password": PASSWORD})).await;

        assert_eq!(status, StatusCode::CREATED);
        let claims = jwt.verify(body["data"]["access_token"].as_str().unwrap()).unwrap();
        assert_eq!(claims.data.username, "alice");
        assert_eq!(claims.roles, vec!["user".to_string()]);
        assert_eq!(body["data"]["refresh_token"].as_str().unwrap().len(), 64);
        assert_eq!(users.len_users(), 1);
    }

    // signing up with a taken username returns 409
    #[tokio::test]
    async fn test_signup_with_taken_username_returns_conflict() {
        let (router, users, _jwt) = default_setup();
        signed_up_user(&router).await;

        let (status, _) = post_json(router, "/auth/signup", json!({"username": "alice", "password": PASSWORD})).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(users.len_users(), 1);
    }

    // a too-short password returns 400 naming the password field, and no user is created
    #[tokio::test]
    async fn test_signup_with_short_password_returns_validation_error() {
        let (router, users, _jwt) = default_setup();

        let (status, body) = post_json(router, "/auth/signup", json!({"username": "alice", "password": "short"})).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["fields"]["password"], "password must be 8 to 128 characters");
        assert_eq!(users.len_users(), 0);
    }

    // a request that tries to choose its own roles is rejected, so nobody can sign up as admin
    #[tokio::test]
    async fn test_signup_rejects_roles_in_body() {
        let (router, users, _jwt) = default_setup();

        let (status, _) = post_json(router, "/auth/signup", json!({"username": "alice", "password": PASSWORD, "roles": ["admin"]})).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(users.len_users(), 0);
    }

    // with signup disabled the signup route does not exist, while login is still served
    #[tokio::test]
    async fn test_signup_route_is_missing_when_disabled() {
        let (router, users, _jwt) = setup(false, Duration::from_secs(60));

        let (signup_status, _) = post_json(router.clone(), "/auth/signup", json!({"username": "alice", "password": PASSWORD})).await;
        let (login_status, _) = post_json(router, "/auth/login", json!({"username": "alice", "password": PASSWORD})).await;

        assert_eq!(signup_status, StatusCode::NOT_FOUND);
        assert_eq!(login_status, StatusCode::UNAUTHORIZED);
        assert_eq!(users.len_users(), 0);
    }

    // logging in with the right credentials returns an access token for the user and a refresh token
    #[tokio::test]
    async fn test_login_returns_tokens() {
        let (router, _users, jwt) = default_setup();
        signed_up_user(&router).await;

        let (status, body) = post_json(router, "/auth/login", json!({"username": "Alice", "password": PASSWORD})).await;

        assert_eq!(status, StatusCode::OK);
        let claims = jwt.verify(body["data"]["access_token"].as_str().unwrap()).unwrap();
        assert_eq!(claims.data.username, "alice");
        assert_eq!(body["data"]["refresh_token"].as_str().unwrap().len(), 64);
    }

    // a wrong password and an unknown username give the same 401 answer
    #[tokio::test]
    async fn test_login_failures_are_indistinguishable() {
        let (router, _users, _jwt) = default_setup();
        signed_up_user(&router).await;

        let (wrong_status, wrong_body) = post_json(router.clone(), "/auth/login", json!({"username": "alice", "password": "wrong-password"})).await;
        let (unknown_status, unknown_body) = post_json(router, "/auth/login", json!({"username": "nobody", "password": PASSWORD})).await;

        assert_eq!(wrong_status, StatusCode::UNAUTHORIZED);
        assert_eq!(unknown_status, StatusCode::UNAUTHORIZED);
        assert_eq!(wrong_body, unknown_body);
    }

    // refreshing returns a new token pair and uses up the old refresh token
    #[tokio::test]
    async fn test_refresh_returns_new_tokens_and_consumes_the_old_one() {
        let (router, _users, jwt) = default_setup();
        let tokens = signed_up_user(&router).await;
        let old_refresh_token = tokens["refresh_token"].as_str().unwrap().to_string();

        let (status, body) = post_json(router.clone(), "/auth/refresh", json!({"refresh_token": old_refresh_token})).await;
        let (reuse_status, _) = post_json(router, "/auth/refresh", json!({"refresh_token": old_refresh_token})).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(jwt.verify(body["data"]["access_token"].as_str().unwrap()).unwrap().data.username, "alice");
        assert_ne!(body["data"]["refresh_token"].as_str().unwrap(), old_refresh_token);
        assert_eq!(reuse_status, StatusCode::UNAUTHORIZED);
    }

    // a disabled user can neither log in nor use a refresh token issued before it was disabled
    #[tokio::test]
    async fn test_disabled_user_cannot_login_or_refresh() {
        let (router, users, _jwt) = default_setup();
        let tokens = signed_up_user(&router).await;
        users.disable_user("alice").await.unwrap();

        let (login_status, _) = post_json(router.clone(), "/auth/login", json!({"username": "alice", "password": PASSWORD})).await;
        let (refresh_status, _) = post_json(router, "/auth/refresh", json!({"refresh_token": tokens["refresh_token"]})).await;

        assert_eq!(login_status, StatusCode::UNAUTHORIZED);
        assert_eq!(refresh_status, StatusCode::UNAUTHORIZED);
    }

    // an unknown refresh token and an expired one are both rejected with 401
    #[tokio::test]
    async fn test_refresh_rejects_unknown_and_expired_tokens() {
        let (router, _users, _jwt) = setup(true, Duration::from_secs(0));
        let expired = signed_up_user(&router).await["refresh_token"].as_str().unwrap().to_string();

        let (unknown_status, _) = post_json(router.clone(), "/auth/refresh", json!({"refresh_token": "0".repeat(64)})).await;
        let (expired_status, _) = post_json(router, "/auth/refresh", json!({"refresh_token": expired})).await;

        assert_eq!(unknown_status, StatusCode::UNAUTHORIZED);
        assert_eq!(expired_status, StatusCode::UNAUTHORIZED);
    }
}
