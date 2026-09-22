use alloy::rpc::types::Block;
use crate::error::CoreError;
use crate::indexer::client::eth::EthClient;

pub struct EthereumMockClient {
    response_blocks: Result<Vec<Option<Block>>, CoreError>,
}

impl EthereumMockClient {
    pub fn new() -> EthereumMockClient {
        Self {
            response_blocks: Ok(Vec::new()),
        }
    }

    pub fn set_response(&mut self, res: Result<Vec<Option<Block>>, CoreError>) {
        self.response_blocks = res;
    }
}


impl EthClient for EthereumMockClient {
    async fn get_blocks(&self, start_block_number: usize, size: usize) -> Result<Vec<Option<Block>>, CoreError> {
        return Ok(self.response_blocks.as_ref().unwrap().clone())
    }
}