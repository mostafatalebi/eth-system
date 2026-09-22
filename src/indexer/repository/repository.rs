use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;


pub trait BlockRepository {
    async fn upsert_blocks(&self, blocks: &Vec<BlockModel>)-> Result<(), CoreError> ;
    async fn delete_block(&self) -> Result<(), CoreError>;
    async fn last_block_number(&self) -> Result<u64, CoreError>;
    
}