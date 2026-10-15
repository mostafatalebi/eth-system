use std::sync::Arc;
use std::time::Duration;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use crate::auth::jwt::JwtService;
use crate::http_server::response::ApiResponse;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY_BYTES: usize = 1024 * 1024;

fn bearer_token(value: &HeaderValue) -> Option<&str> {
    let (scheme, token) = value.to_str().ok()?.split_once(' ')?;
    scheme.eq_ignore_ascii_case("Bearer").then_some(token.trim())
}

/// accepts only requests carrying a valid Bearer JWT; the verified claims are added to the
/// request extensions so handlers can read them with `Extension<Claims>`
pub async fn require_auth(State(jwt): State<Arc<JwtService>>, mut request: Request, next: Next) -> Response {
    let claims = request.headers().get(header::AUTHORIZATION)
        .and_then(bearer_token)
        .and_then(|token| jwt.verify(token).ok());

    match claims {
        Some(claims) => {
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        None => {
            let mut response = ApiResponse::<()>::unauthorized("invalid or missing token".to_string()).into_response();
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"eth_app\""));
            response
        }
    }
}

/// tracing, panic catching, request timeout and body size limit for every request
pub fn with_shared_layers(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(CatchPanicLayer::new())
            .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use axum::Extension;
    use tower::ServiceExt;
    use crate::auth::jwt::Claims;
    use crate::auth::models::UserModel;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn jwt() -> Arc<JwtService> {
        Arc::new(JwtService::new(SECRET, Duration::from_secs(60)).unwrap())
    }

    fn token(jwt: &JwtService) -> String {
        let user = UserModel {
            id: 1,
            username: "alice".to_string(),
            password_hash: String::new(),
            roles: vec!["user".to_string()],
            is_active: true,
            created_at: Default::default(),
            updated_at: Default::default(),
        };
        jwt.issue(&user).unwrap()
    }

    fn app(jwt: Arc<JwtService>) -> Router {
        Router::new()
            .route("/me", get(|Extension(claims): Extension<Claims>| async move { claims.data.username }))
            .route_layer(middleware::from_fn_with_state(jwt, require_auth))
    }

    async fn get_with(app: Router, path: &str, authorization: Option<String>) -> (StatusCode, String) {
        let mut request = Request::builder().uri(path);
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        let response = app.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    // a request without an Authorization header is rejected with 401
    #[tokio::test]
    async fn test_missing_token_is_rejected() {
        let (status, _) = get_with(app(jwt()), "/me", None).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // a Bearer value that is not a valid JWT is rejected with 401
    #[tokio::test]
    async fn test_invalid_token_is_rejected() {
        let (status, _) = get_with(app(jwt()), "/me", Some("Bearer not-a-jwt".to_string())).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // a valid token is let through and the handler can read the claims from the request
    #[tokio::test]
    async fn test_valid_token_is_accepted_and_claims_reach_the_handler() {
        let jwt = jwt();
        let authorization = format!("Bearer {}", token(&jwt));

        let (status, body) = get_with(app(jwt), "/me", Some(authorization)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "alice");
    }

    // auth only guards matched routes, so an unknown path is 404 even without a token
    #[tokio::test]
    async fn test_unknown_path_is_not_found_without_token() {
        let (status, _) = get_with(app(jwt()), "/unknown", None).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // a panicking handler becomes a 500 response instead of a dropped connection
    #[tokio::test]
    async fn test_panicking_handler_returns_500() {
        async fn boom() -> &'static str { panic!("boom") }
        let router = with_shared_layers(Router::new().route("/boom", get(boom)));

        let (status, _) = get_with(router, "/boom", None).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
