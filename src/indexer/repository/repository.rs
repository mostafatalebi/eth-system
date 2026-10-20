use std::future::Future;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::log::LogModel;
use crate::indexer::models::transaction::TransactionModel;

/// methods return `impl Future + Send` instead of using `async fn`, so that generic
/// callers (e.g. axum handlers over `R: BlockRepository`) can rely on the futures being Send.
/// implementations can still be written as `async fn`
pub trait BlockRepository: Send + Sync + 'static {
    fn upsert_blocks(&self, blocks: &BlockData) -> impl Future<Output = Result<(), CoreError>> + Send;
    fn delete_block_cascading(&self, oldest_block: Vec<u8>, newest_block: Vec<u8>) -> impl Future<Output = Result<(), CoreError>> + Send;
    /// returns the highest stored block number, or `None` if no block is stored yet
    fn last_block_number(&self) -> impl Future<Output = Result<Option<u64>, CoreError>> + Send;

    fn block_exists(&self, number: i64, hash: &Vec<u8>, parent_hash: &Vec<u8>) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn get_block(&self, hash: Option<Vec<u8>>, number: Option<i64>) -> impl Future<Output = Result<Option<BlockModel>, CoreError>> + Send;
    /// returns blocks ordered by number, newest first
    fn list_blocks(&self, limit: i64, offset: i64) -> impl Future<Output = Result<Vec<BlockModel>, CoreError>> + Send;
    /// returns logs matching the filter, newest first (by block number, then log index)
    fn list_logs(&self, filter: &LogFilter, limit: i64, offset: i64) -> impl Future<Output = Result<Vec<LogModel>, CoreError>> + Send;
    fn get_block_by_parent_hash(&self, p_hash: Vec<u8>) -> impl Future<Output = Result<Option<BlockModel>, CoreError>> + Send;

    fn replace_chain_above(&self, ancestor_number: i64, new_chain: &BlockData) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn mark_finalized_checkpoint(&self, hash: Vec<u8>) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn is_block_finalized(&self, number: i64) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn last_finalized_checkpoint(&self) -> impl Future<Output = Result<Option<i64>, CoreError>> + Send;
}


/// every field that is set must match; block numbers are inclusive
#[derive(Debug, Default, Clone)]
pub struct LogFilter {
    pub address: Option<Vec<u8>>,
    /// topics[i] is compared with topic{i}
    pub topics: [Option<Vec<u8>>; 4],
    pub from_block: Option<i64>,
    pub to_block: Option<i64>,
}

pub struct BlockData<'a> {
    pub blocks: &'a Vec<BlockModel>,
    pub transactions: &'a Vec<Vec<TransactionModel>>,
    /// logs[i] are the logs of blocks[i]
    pub logs: &'a Vec<Vec<LogModel>>,
}

impl<'a> BlockData<'a> {
    pub fn new(blocks: &'a Vec<BlockModel>, transactions: &'a Vec<Vec<TransactionModel>>, logs: &'a Vec<Vec<LogModel>>) -> Self {
        Self {
            blocks,
            transactions,
            logs,
        }
    }
}