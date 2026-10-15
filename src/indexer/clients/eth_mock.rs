use alloy::primitives::B256;
use alloy::rpc::types::Block;
use crate::error::CoreError;
use crate::indexer::clients::eth::EthClient;

pub struct EthereumMockClient {
    response_blocks: Result<Vec<Option<Block>>, CoreError>,
    finalized_block: Option<Block>,
}

impl EthereumMockClient {
    pub fn new() -> EthereumMockClient {
        Self {
            response_blocks: Ok(Vec::new()),
            finalized_block: None,
        }
    }

    pub fn set_response(&mut self, res: Result<Vec<Option<Block>>, CoreError>) {
        self.response_blocks = res;
    }

    pub fn set_finalized(&mut self, block: Option<Block>) {
        self.finalized_block = block;
    }
}


impl EthClient for EthereumMockClient {
    async fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> Result<Vec<Option<Block>>, CoreError> {
        return Ok(self.response_blocks.as_ref().unwrap().clone())
    }

    async fn get_last_finalized_block(&self) -> Result<Option<Block>, CoreError> {
        Ok(self.finalized_block.clone())
    }

    async fn get_block_by_hash(&self, hash: B256) -> Result<Option<Block>, CoreError> {
        let blocks = self.response_blocks.as_ref().map_err(|e| CoreError::RpcRequestFailed(e.to_string()))?;
        Ok(blocks.iter().flatten().find(|b| b.header.hash == hash).cloned())
    }
}