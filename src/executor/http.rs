use std::sync::Arc;
use axum::http::StatusCode;
use axum::{Router};
use axum::extract::{Query, State};
use axum::routing::{get};
use revm::primitives::B256;
use serde::{Deserialize, Serialize};
use validator::Validate;
use crate::app::app::App;
use crate::db::models::TxExecModel;
use crate::db::storage::AppStorage;
use crate::indexer::repository::repository::BlockRepository;
use crate::http_server::response::{ApiResponse, ErrorResponse};


#[derive(Deserialize, Validate)]
pub struct TxListParams {
    #[validate(length(min = 64, max=66, message = "id must be a correct Ethereum transaction hash"))]
    id: String,
}

#[derive(Deserialize, Serialize)]
pub struct TxResponse {
}

pub struct SimulatorHandlers;

impl SimulatorHandlers {
    pub fn get_routes<S: AppStorage + 'static, R: BlockRepository+Clone>(app: Arc<App<S, R>>) -> Router {
        let router = Router::new()
            .route("/tx", get(Self::get_tx::<S, R>))
            .route("/list", get(Self::get_list::<S, R>))
            .with_state(app);

        let group = Router::new();
        group.nest("/simulator", router)
    }


    pub async fn get_tx<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>, Query(params) : Query<TxListParams>)
        -> ApiResponse<TxExecModel> {
        let mut api_response = ApiResponse::default();
        if let Err(e) = params.validate() {
            let mut err_r = ErrorResponse::new("validation error".to_string());
            e.field_errors().iter().for_each(|k| {
                err_r.add_field_error(k.0.to_string(), (*k.1).to_vec().get(0).unwrap().to_string());
            });
            api_response.set_status_code(StatusCode::BAD_REQUEST);
            api_response.set_error(err_r);
            return api_response;
        }
        let Ok(tx_hash) = params.id.parse::<B256>() else {
            let mut err_r = ErrorResponse::new("validation error".to_string());
            err_r.add_field_error("id".into(), "id must be a hex encoded Ethereum transaction hash".into());
            api_response.set_status_code(StatusCode::BAD_REQUEST);
            api_response.set_error(err_r);
            return api_response;
        };
        match app.app_storage().get_transaction(tx_hash) {
            Err(e) => ApiResponse::for_error(e.to_string()),
            Ok(None) => ApiResponse::not_found(format!("Transaction {} not found", params.id)),
            Ok(Some(tx)) => ApiResponse::for_success(Some(tx)),
        }
    }
    pub async fn get_list<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>)
        -> ApiResponse<Vec<B256>> {
        let res = app.app_storage().get_list();
        if res.is_err() {
            return ApiResponse::for_error(res.err().unwrap().to_string())
        }
        let res = res.unwrap();
        return ApiResponse::for_success(res);
    }
}