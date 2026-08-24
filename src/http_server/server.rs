use std::sync::Arc;
use axum::Router;
use crate::app::app::{App};
use crate::db::storage::{AppStorage, InMemoryStorage};

pub struct HttpServer<S: AppStorage> {
    routes: Router,
    bind_addr: String,
    app: Option<Arc<App<S>>>
}

impl<S: AppStorage> HttpServer<S> {
    pub fn new(bind_addr: String, routes: Router) -> Self {
        Self{
            bind_addr, routes,
            app: None,
        }
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        let lis = tokio::net::TcpListener::bind(self.bind_addr.clone())
            .await?;

        axum::serve(lis, self.routes.clone()).await
    }

    pub fn with_app(&mut self, app: Arc<App<S>>) {
        self.app = Some(app);
    }
}