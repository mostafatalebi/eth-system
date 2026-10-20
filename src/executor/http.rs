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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;
    use crate::db::storage::InMemoryStorage;
    use crate::indexer::repository::mock_repository::MockRepository;

    fn router() -> Router {
        let app: App<InMemoryStorage, MockRepository> = App::new_for_test(InMemoryStorage::new(), None);
        SimulatorHandlers::get_routes(Arc::new(app))
    }

    async fn get(path: &str) -> (StatusCode, Value) {
        let response = router().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn test_unknown_transaction_is_not_found() {
        let hash = B256::random();
        for id in [format!("{:?}", hash), alloy::hex::encode(hash)] {
            let (status, body) = get(&format!("/simulator/tx?id={}", id)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{}", id);
            assert_eq!(body["ok"], false);
        }
    }

    #[tokio::test]
    async fn test_transaction_id_of_the_right_length_but_not_hex_is_a_validation_error() {
        let (status, body) = get(&format!("/simulator/tx?id={}", "z".repeat(64))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]["fields"]["id"].is_string());
    }

    #[tokio::test]
    async fn test_transaction_id_of_the_wrong_length_is_a_validation_error() {
        for id in ["0x1234", &"a".repeat(67)] {
            let (status, body) = get(&format!("/simulator/tx?id={}", id)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{}", id);
            assert!(body["error"]["fields"]["id"].is_string());
        }
    }

    #[tokio::test]
    async fn test_transaction_without_an_id_is_rejected() {
        let (status, _) = get("/simulator/tx").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_of_an_empty_storage_is_ok() {
        let (status, body) = get("/simulator/list").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
    }
}
