use std::sync::Arc;
use axum::{Json, Router};
use axum::extract::State;
use axum::routing::{get, post};
use crate::app::app::App;
use crate::indexer::models::product_model::ProductModel;



pub struct ProductHandlers {}

impl ProductHandlers {
    pub fn get_routes(app: Arc<App>) -> Router {
        let router = Router::new()
            .route("/create", post(ProductHandlers::post_create))
            .with_state(app);

        let group = Router::new();
        group.nest("/products", router)
    }


    pub async fn post_create(State(app) : State<Arc<App>>, Json(data): Json<ProductModel>) -> String  {
        return "".to_owned();
    }
}