use std::sync::Arc;
use axum::Router;
use crate::app::app::{App};
use crate::app::cmd::AppMode::Indexer;
use crate::db::storage::{AppStorage, InMemoryStorage};
use crate::executor::http::SimulatorHandlers;
use crate::indexer::http::handlers::IndexerHandlers;
use crate::indexer::repository::repository::BlockRepository;
use crate::indexer::validator::block_validator::BlockValidator;

pub fn get_router(app: Arc<App<impl AppStorage + 'static, impl BlockRepository+Clone>>) -> Router {
    let router = Router::new()
        .merge(SimulatorHandlers::get_routes(app.clone()))
        .merge(IndexerHandlers::get_routes(app.clone()));

    Router::new()
        .nest("/api", router)
}

