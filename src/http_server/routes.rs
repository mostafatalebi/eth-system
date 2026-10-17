use std::sync::Arc;
use axum::{middleware, Router};
use tracing::warn;
use crate::app::app::{App};
use crate::app::cmd::AppMode::Indexer;
use crate::db::storage::{AppStorage, InMemoryStorage};
use crate::executor::http::SimulatorHandlers;
use crate::http_server::middleware::{require_auth, with_shared_layers};
use crate::auth::handlers::{AuthHandlers, AuthState};
use crate::auth::jwt::JwtService;
use crate::auth::pg_repository::PgUserRepository;
use crate::auth::redis_repository::RedisRefreshTokenRepository;
use crate::config::config::DEFAULT_JWT_SECRET;
use crate::error::CoreError;
use crate::indexer::http::handlers::IndexerHandlers;
use crate::indexer::repository::repository::BlockRepository;
use crate::indexer::validator::block_validator::BlockValidator;

pub async fn get_router(app: Arc<App<impl AppStorage + 'static, impl BlockRepository+Clone>>) -> Result<Router, CoreError> {
    if app.config.jwt_secret == DEFAULT_JWT_SECRET {
        warn!("jwt_secret is the built-in dummy default: anyone can forge tokens, set your own before exposing the API");
    }
    let jwt = Arc::new(JwtService::new(&app.config.jwt_secret, app.config.access_token_expiry)?);
    let db = app.persistence.clone().ok_or(CoreError::Empty("database is not initialized".to_string()))?;
    let tokens = RedisRefreshTokenRepository::connect(&app.config.redis_url, &app.config.redis_prefix).await?;
    let auth_state = Arc::new(AuthState {
        users: PgUserRepository::new(db),
        tokens,
        jwt: jwt.clone(),
        refresh_token_expiry: app.config.refresh_token_expiry,
        enable_signup: app.config.enable_http_signup,
    });

    let protected = Router::new()
        .merge(SimulatorHandlers::get_routes(app.clone()))
        .merge(IndexerHandlers::get_routes(app.clone()))
        .route_layer(middleware::from_fn_with_state(jwt, require_auth));
    let api = protected.merge(AuthHandlers::get_routes(auth_state));

    Ok(with_shared_layers(Router::new().nest("/api", api)))
}

