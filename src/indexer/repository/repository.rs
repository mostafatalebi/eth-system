use alloy::consensus::TxEnvelope;
use alloy::rpc::types::Transaction;
use revm::primitives::U256;
use std::future::Future;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::transaction::TransactionModel;

/// methods return `impl Future + Send` instead of using `async fn`, so that generic
/// callers (e.g. axum handlers over `R: BlockRepository`) can rely on the futures being Send.
/// implementations can still be written as `async fn`
pub trait BlockRepository: Send + Sync + 'static {
    fn upsert_blocks(&self, blocks: &Vec<BlockModel>, transactions: &Vec<Vec<TransactionModel>>) -> impl Future<Output = Result<(), CoreError>> + Send;
    fn delete_block(&self) -> impl Future<Output = Result<(), CoreError>> + Send;
    /// returns the highest stored block number, or `None` if no block is stored yet
    fn last_block_number(&self) -> impl Future<Output = Result<Option<u64>, CoreError>> + Send;

    fn block_exists(&self, number: i64, hash: &Vec<u8>, parent_hash: &Vec<u8>) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn get_block(&self, hash: Option<Vec<u8>>, number: Option<i64>) -> impl Future<Output = Result<Option<BlockModel>, CoreError>> + Send;
}