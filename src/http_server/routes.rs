use std::sync::Arc;
use axum::Router;
use crate::app::app::{App};
use crate::db::storage::{AppStorage, InMemoryStorage};
use crate::executor::http::IndexerHandlers;


pub fn get_router(app: Arc<App<impl AppStorage + 'static>>) -> Router {
    let router = Router::new()
        .merge(IndexerHandlers::get_routes(app));

    Router::new()
        .nest("/api", router)
}


