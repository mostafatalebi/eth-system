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
pub struct TxListParams {
    #[validate(length(min = 64, max=66, message = "id must be a correct Ethereum transaction hash"))]
    id: String,
}

#[derive(Deserialize, Serialize)]
pub struct TxResponse {
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
            // .route("/accounts/list", get(Self::get_list::<S, R>))
            // .route("/accounts/{{address}}", get(Self::get_list::<S, R>))
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