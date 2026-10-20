use std::sync::Arc;
use axum::http::StatusCode;
use axum::{Router};
use axum::extract::{Path, Query, State};
use axum::routing::{get};
use alloy::primitives::Address;
use revm::primitives::B256;
use serde::{Deserialize, Serialize};
use validator::{Validate};
use crate::app::app::App;
use crate::db::storage::AppStorage;
use crate::indexer::repository::repository::BlockRepository;
use crate::http_server::response::{ApiResponse, ErrorResponse};
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::log::LogModel;
use crate::indexer::repository::repository::LogFilter;

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

const MAX_LOGS_LIMIT: i64 = 1000;

#[derive(Deserialize)]
pub struct LogListParams {
    address: Option<String>,
    topic0: Option<String>,
    topic1: Option<String>,
    topic2: Option<String>,
    topic3: Option<String>,
    from_block: Option<i64>,
    to_block: Option<i64>,
    #[serde(default = "default_logs_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_logs_limit() -> i64 {
    100
}

impl LogListParams {
    /// turns the query into a filter; every invalid field is reported, not just the first
    fn to_filter(&self) -> Result<LogFilter, ErrorResponse> {
        let mut error = ErrorResponse::new("validation error".to_string());
        let mut invalid = false;
        let mut reject = |field: &str, msg: &str| {
            error.add_field_error(field.to_string(), msg.to_string());
            invalid = true;
        };

        let mut filter = LogFilter::default();
        match self.address.as_deref().map(str::parse::<Address>) {
            Some(Ok(address)) => filter.address = Some(address.to_vec()),
            Some(Err(_)) => reject("address", "address must be a 20-byte hex string (40 hex characters, optionally 0x-prefixed)"),
            None => {}
        }
        let topics = [&self.topic0, &self.topic1, &self.topic2, &self.topic3];
        for (i, topic) in topics.iter().enumerate() {
            match topic.as_deref().map(str::parse::<B256>) {
                Some(Ok(topic)) => filter.topics[i] = Some(topic.to_vec()),
                Some(Err(_)) => reject(&format!("topic{}", i), "topic must be a 32-byte hex string (64 hex characters, optionally 0x-prefixed)"),
                None => {}
            }
        }
        for (field, block) in [("from_block", self.from_block), ("to_block", self.to_block)] {
            if block.is_some_and(|b| b < 0) {
                reject(field, &format!("{} must not be negative", field));
            }
        }
        if self.from_block.zip(self.to_block).is_some_and(|(from, to)| from > to) {
            reject("from_block", "from_block must not be greater than to_block");
        }
        if !(1..=MAX_LOGS_LIMIT).contains(&self.limit) {
            reject("limit", &format!("limit must be between 1 and {}", MAX_LOGS_LIMIT));
        }
        if self.offset < 0 {
            reject("offset", "offset must not be negative");
        }

        filter.from_block = self.from_block;
        filter.to_block = self.to_block;
        if invalid { Err(error) } else { Ok(filter) }
    }
}

pub struct IndexerHandlers;

impl IndexerHandlers {
    pub fn get_routes<S: AppStorage + 'static, R: BlockRepository+Clone>(app: Arc<App<S, R>>) -> Router {
        let router = Router::new()
            .route("/blocks/list", get(Self::get_list::<S, R>))
            .route("/blocks/hash/{hash}", get(Self::get_block_by_hash::<S, R>))
            .route("/logs", get(Self::get_logs::<S, R>))
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

    /// GET /indexer/logs?address=&topic0=..topic3=&from_block=&to_block=&limit=&offset=; newest logs first
    pub async fn get_logs<S: AppStorage, R: BlockRepository+Clone>(State(app) : State<Arc<App<S, R>>>, Query(params): Query<LogListParams>)
        -> ApiResponse<Vec<LogModel>> {
        let filter = match params.to_filter() {
            Ok(filter) => filter,
            Err(error) => {
                let mut api_response = ApiResponse::default();
                api_response.set_status_code(StatusCode::BAD_REQUEST);
                api_response.set_error(error);
                return api_response;
            }
        };
        let Some(repo) = app.block_repository.as_ref() else {
            return ApiResponse::for_error("block repository is not set".to_string());
        };
        match repo.list_logs(&filter, params.limit, params.offset).await {
            Err(e) => ApiResponse::for_error(e.to_string()),
            Ok(logs) => ApiResponse::for_success(Some(logs)),
        }
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
    use crate::indexer::types::mocks::{mock_block_with_parent, mock_block_with_receipts};

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

    // logs

    /// a router over a mock repository holding the logs of block 1 (transactions with 2 and 3 logs) and
    /// block 2 (one transaction with 1 log), and those logs in the order they were created
    fn logs_setup() -> (Router, Vec<LogModel>) {
        let repo = MockRepository::new();
        let mut logs = Vec::new();
        for (number, logs_per_tx) in [(1u64, vec![2usize, 3]), (2, vec![1])] {
            let (_, receipts) = mock_block_with_receipts(number, B256::random(), &logs_per_tx);
            logs.extend(receipts.iter().flat_map(|r| r.inner.logs()).map(|l| LogModel::new_from_rpc_log(l).unwrap()));
        }
        repo.add_logs(logs.clone());
        let app = App::new_for_test(InMemoryStorage::new(), Some(repo));
        (IndexerHandlers::get_routes(Arc::new(app)), logs)
    }

    /// (block number, log index) of every log in a response
    fn log_positions(body: &Value) -> Vec<(i64, i64)> {
        body["data"].as_array().unwrap().iter().map(|l| (l["block_number"].as_i64().unwrap(), l["log_index"].as_i64().unwrap())).collect()
    }

    #[tokio::test]
    async fn test_logs_are_listed_newest_first_with_hex_fields() {
        let (router, _) = logs_setup();
        let (status, body) = get(router, "/indexer/logs").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(log_positions(&body), vec![(2, 0), (1, 4), (1, 3), (1, 2), (1, 1), (1, 0)]);
        let log = &body["data"][0];
        assert_eq!(log["address"], format!("0x{}", "aa".repeat(20)));
        assert_eq!(log["data_json"], serde_json::json!({ "raw": "0x00ff" }));
        // a log without topics has all four as null
        assert!(log["topic0"].is_null());
        // log 4 of block 1 has four topics, each a 0x-prefixed 32-byte hex string
        assert_eq!(body["data"][1]["topic3"].as_str().unwrap().len(), 66);
    }

    #[tokio::test]
    async fn test_logs_can_be_filtered_by_address() {
        let (router, _) = logs_setup();
        let (_, body) = get(router.clone(), &format!("/indexer/logs?address=0x{}", "aa".repeat(20))).await;
        assert_eq!(log_positions(&body).len(), 6);
        // the 0x prefix is optional
        let (_, body) = get(router.clone(), &format!("/indexer/logs?address={}", "aa".repeat(20))).await;
        assert_eq!(log_positions(&body).len(), 6);
        let (status, body) = get(router, &format!("/indexer/logs?address=0x{}", "bb".repeat(20))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_logs_can_be_filtered_by_topics() {
        let (router, logs) = logs_setup();
        // block 1's log 3 is the one with three topics; topics are random, so each identifies its log
        let log = logs.iter().find(|l| l.block_number == 1 && l.log_index == 3).unwrap();
        let topic0 = alloy::hex::encode(log.topic0.as_ref().unwrap());
        let topic2 = alloy::hex::encode(log.topic2.as_ref().unwrap());

        let (_, body) = get(router.clone(), &format!("/indexer/logs?topic0=0x{}", topic0)).await;
        assert_eq!(log_positions(&body), vec![(1, 3)]);
        let (_, body) = get(router.clone(), &format!("/indexer/logs?topic0={}&topic2={}", topic0, topic2)).await;
        assert_eq!(log_positions(&body), vec![(1, 3)]);
        // right topic0 but a topic2 that does not match
        let (_, body) = get(router, &format!("/indexer/logs?topic0={}&topic2={}", topic0, "00".repeat(32))).await;
        assert!(body["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_logs_can_be_filtered_by_block_range() {
        let (router, _) = logs_setup();
        let (_, body) = get(router.clone(), "/indexer/logs?from_block=2").await;
        assert_eq!(log_positions(&body), vec![(2, 0)]);
        let (_, body) = get(router.clone(), "/indexer/logs?to_block=1").await;
        assert_eq!(log_positions(&body).len(), 5);
        let (_, body) = get(router, "/indexer/logs?from_block=1&to_block=1").await;
        assert_eq!(log_positions(&body).len(), 5);
    }

    #[tokio::test]
    async fn test_logs_honour_limit_and_offset() {
        let (router, _) = logs_setup();
        let (_, body) = get(router.clone(), "/indexer/logs?limit=2&offset=1").await;
        assert_eq!(log_positions(&body), vec![(1, 4), (1, 3)]);
        let (_, body) = get(router, "/indexer/logs?offset=100").await;
        assert!(body["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_logs_report_every_invalid_field_at_once() {
        let (router, _) = logs_setup();
        let (status, body) = get(router, "/indexer/logs?address=zzz&topic1=1234&limit=0&offset=-1&to_block=-5").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        for field in ["address", "topic1", "limit", "offset", "to_block"] {
            assert!(body["error"]["fields"][field].is_string(), "{} must be reported", field);
        }
    }

    #[tokio::test]
    async fn test_logs_reject_out_of_range_values() {
        for query in ["limit=1001", "offset=-1", "from_block=5&to_block=2", "from_block=-1", "topic0=0x12", "address=0x12"] {
            let (router, _) = logs_setup();
            let (status, body) = get(router, &format!("/indexer/logs?{}", query)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{}", query);
            assert_eq!(body["ok"], false);
        }
    }

    #[tokio::test]
    async fn test_logs_without_a_repository_is_an_internal_error() {
        let app: App<InMemoryStorage, MockRepository> = App::new_for_test(InMemoryStorage::new(), None);
        let (status, _) = get(IndexerHandlers::get_routes(Arc::new(app)), "/indexer/logs").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}

