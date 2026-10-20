use std::sync::Arc;
use axum::http::StatusCode;
use axum::{Router};
use axum::extract::{Path, Query, State};
use axum::routing::{get};
use revm::primitives::B256;
use serde::{Deserialize, Serialize};
use validator::{Validate};
use crate::app::app::App;
use crate::db::storage::AppStorage;
use crate::indexer::repository::repository::BlockRepository;
use crate::http_server::response::{ApiResponse, ErrorResponse};
use crate::indexer::models::block::BlockModel;

#[derive(Deserialize, Validate)]
pub struct ListParams {
    #[serde(default = "default_limit")]
    #[validate(range(min = 1, max = 100, message = "limit must be between 1 and 100"))]
    limit: i64,
    #[serde(default)]
    #[validate(range(min = 0, message = "offset must not be negative"))]
    offset: i64,
}

fn default_limit() -> i64 {
    20
}

pub struct IndexerHandlers;

impl IndexerHandlers {
    pub fn get_routes<S: AppStorage + 'static, R: BlockRepository+Clone>(app: Arc<App<S, R>>) -> Router {
        let router = Router::new()
            .route("/blocks/list", get(Self::get_list::<S, R>))
            .route("/blocks/hash/{hash}", get(Self::get_block_by_hash::<S, R>))
            .route("/blocks/number/{number}", get(Self::get_block_by_number::<S, R>))
            // .route("/transactions/list", get(Self::get_list::<S, R>))
            // .route("/transactions/{{hash}}", get(Self::get_list::<S, R>))
            .with_state(app);

        let group = Router::new();
        group.nest("/indexer", router)
    }


    /// GET /indexer/blocks/hash/{hash}; hash is 32 bytes hex, with or without 0x prefix
    pub async fn get_block_by_hash<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>, Path(identifier): Path<String>)
        -> ApiResponse<BlockModel> {
        // B256 parsing accepts both 64 and 66 (0x-prefixed) character forms and decodes
        // the hex into the 32 raw bytes stored in the hash column
        let Ok(hash) = identifier.parse::<B256>() else {
            return Self::validation_error("hash", "hash must be a 32-byte hex string (64 hex characters or 66 long with 0x prefix)");
        };

        Self::find_block(&app, Some(hash.to_vec()), None, &identifier).await
    }

    /// GET /indexer/blocks/number/{number}
    pub async fn get_block_by_number<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>, Path(identifier): Path<String>)
        -> ApiResponse<BlockModel> {
        // parsed by hand (instead of Path<i64>) so a bad value gets our ApiResponse error format
        let number = match identifier.parse::<i64>() {
            Ok(n) if n >= 0 => n,
            _ => return Self::validation_error("number", "number must be a non-negative integer"),
        };

        Self::find_block(&app, None, Some(number), &identifier).await
    }

    async fn find_block<S: AppStorage, R: BlockRepository+Clone>(app: &App<S, R>, hash: Option<Vec<u8>>, number: Option<i64>, identifier: &str)
        -> ApiResponse<BlockModel> {
        let Some(repo) = app.block_repository.as_ref() else {
            return ApiResponse::for_error("block repository is not set".to_string());
        };

        match repo.get_block(hash, number).await {
            Err(e) => ApiResponse::for_error(e.to_string()),
            Ok(None) => ApiResponse::not_found(format!("Block {} not found", identifier)),
            Ok(Some(block)) => ApiResponse::for_success(Some(block)),
        }
    }

    fn validation_error<T: Serialize>(field: &str, msg: &str) -> ApiResponse<T> where ApiResponse<T>: Default {
        let mut api_response = ApiResponse::default();
        let mut err_r = ErrorResponse::new("validation error".to_string());
        err_r.add_field_error(field.into(), msg.into());
        api_response.set_status_code(StatusCode::BAD_REQUEST);
        api_response.set_error(err_r);
        api_response
    }

    /// GET /indexer/blocks/list?limit=&offset=; newest blocks first
    pub async fn get_list<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>, Query(params): Query<ListParams>)
        -> ApiResponse<Vec<BlockModel>> {
        if let Err(e) = params.validate() {
            let mut err_r = ErrorResponse::new("validation error".to_string());
            e.field_errors().iter().for_each(|k| {
                err_r.add_field_error(k.0.to_string(), (*k.1).to_vec().get(0).unwrap().to_string());
            });
            let mut api_response = ApiResponse::default();
            api_response.set_status_code(StatusCode::BAD_REQUEST);
            api_response.set_error(err_r);
            return api_response;
        }
        let Some(repo) = app.block_repository.as_ref() else {
            return ApiResponse::for_error("block repository is not set".to_string());
        };
        match repo.list_blocks(params.limit, params.offset).await {
            Err(e) => ApiResponse::for_error(e.to_string()),
            Ok(blocks) => ApiResponse::for_success(Some(blocks)),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::B256;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;
    use crate::db::storage::InMemoryStorage;
    use crate::indexer::repository::mock_repository::MockRepository;
    use crate::indexer::types::mocks::mock_block_with_parent;

    /// a router over a mock repository holding blocks numbered 1..=count, plus the hashes of those blocks
    fn setup(count: u64) -> (Router, Vec<B256>) {
        let repo = MockRepository::new();
        let blocks: Vec<_> = (1..=count).map(|n| mock_block_with_parent(n, B256::random())).collect();
        let hashes = blocks.iter().map(|b| b.header.hash).collect();
        repo.add_blocks(blocks.iter().map(|b| BlockModel::new_from_rpc_block(b).unwrap()).collect());
        let app = App::new_for_test(InMemoryStorage::new(), Some(repo));
        (IndexerHandlers::get_routes(Arc::new(app)), hashes)
    }

    async fn get(router: Router, path: &str) -> (StatusCode, Value) {
        let response = router.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn numbers(body: &Value) -> Vec<i64> {
        body["data"].as_array().unwrap().iter().map(|b| b["number"].as_i64().unwrap()).collect()
    }

    #[tokio::test]
    async fn test_list_returns_newest_blocks_first() {
        let (router, _) = setup(3);
        let (status, body) = get(router, "/indexer/blocks/list").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(numbers(&body), vec![3, 2, 1]);
    }

    #[tokio::test]
    async fn test_list_honours_limit_and_offset() {
        let (router, _) = setup(5);
        let (status, body) = get(router, "/indexer/blocks/list?limit=2&offset=1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(numbers(&body), vec![4, 3]);
    }

    #[tokio::test]
    async fn test_list_of_an_empty_repository_is_an_empty_array() {
        let (router, _) = setup(0);
        let (status, body) = get(router, "/indexer/blocks/list").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_list_rejects_out_of_range_limit_and_offset() {
        for (query, field) in [("limit=0", "limit"), ("limit=101", "limit"), ("offset=-1", "offset")] {
            let (router, _) = setup(1);
            let (status, body) = get(router, &format!("/indexer/blocks/list?{}", query)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{}", query);
            assert_eq!(body["ok"], false);
            assert!(body["error"]["fields"][field].is_string(), "{} must be reported as a field error", field);
        }
    }

    #[tokio::test]
    async fn test_list_rejects_a_non_numeric_limit() {
        let (router, _) = setup(1);
        let (status, _) = get(router, "/indexer/blocks/list?limit=abc").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_block_by_number() {
        let (router, _) = setup(3);
        let (status, body) = get(router, "/indexer/blocks/number/2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["number"], 2);
    }

    #[tokio::test]
    async fn test_get_unknown_block_by_number_is_not_found() {
        let (router, _) = setup(3);
        let (status, body) = get(router, "/indexer/blocks/number/99").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["ok"], false);
    }

    #[tokio::test]
    async fn test_get_block_by_invalid_number_is_a_validation_error() {
        for number in ["abc", "-1"] {
            let (router, _) = setup(1);
            let (status, body) = get(router, &format!("/indexer/blocks/number/{}", number)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{}", number);
            assert!(body["error"]["fields"]["number"].is_string());
        }
    }

    #[tokio::test]
    async fn test_get_block_by_hash_with_and_without_prefix() {
        let (router, hashes) = setup(3);
        let hash = hashes[1];
        for identifier in [format!("{:?}", hash), alloy::hex::encode(hash)] {
            let (status, body) = get(router.clone(), &format!("/indexer/blocks/hash/{}", identifier)).await;
            assert_eq!(status, StatusCode::OK, "{}", identifier);
            assert_eq!(body["data"]["number"], 2);
        }
    }

    #[tokio::test]
    async fn test_get_unknown_block_by_hash_is_not_found() {
        let (router, _) = setup(3);
        let (status, _) = get(router, &format!("/indexer/blocks/hash/{:?}", B256::random())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_block_by_invalid_hash_is_a_validation_error() {
        for hash in ["zzzz", "0x1234", &"z".repeat(64)] {
            let (router, _) = setup(1);
            let (status, body) = get(router, &format!("/indexer/blocks/hash/{}", hash)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{}", hash);
            assert!(body["error"]["fields"]["hash"].is_string());
        }
    }

    #[tokio::test]
    async fn test_missing_block_repository_is_an_internal_error() {
        let app: App<InMemoryStorage, MockRepository> = App::new_for_test(InMemoryStorage::new(), None);
        let router = IndexerHandlers::get_routes(Arc::new(app));
        let (status, _) = get(router.clone(), "/indexer/blocks/list").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let (status, _) = get(router, "/indexer/blocks/number/1").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
