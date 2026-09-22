use alloy::primitives::BlockNumber;
use alloy::providers::WsConnect;
use alloy::rpc::client::{ClientBuilder, RpcClient};
use alloy::rpc::types::Block;
use clap::builder::Str;
use crate::error::CoreError;

pub trait EthClient {
    async fn get_blocks(&self, start_block_number: usize, size: usize) -> Result<Vec<Option<Block>>, CoreError>;
}


pub struct AlloyRpcClient {
    inner: RpcClient
}

impl AlloyRpcClient {
    pub async fn new(ws_url: String) -> Result<Self, CoreError> {
        let ws = WsConnect::new(ws_url.clone());
        let ws_client = ClientBuilder::default().ws(ws).await;
        if ws_client.is_err() {
            return Err(CoreError::WsConnFailed(ws_client.err().unwrap().to_string()))
        }
        let ws_client = ws_client.unwrap();
        Ok(Self {
            inner: ws_client
        })
    }
}

impl EthClient for AlloyRpcClient {
    async fn get_blocks(&self, last_block_number: usize, size: usize) -> Result<Vec<Option<Block>>, CoreError> {
        let mut batch = self.inner.new_batch();

        let mut waiters = Vec::new();
        for block_num in last_block_number..last_block_number + size {
            let a = batch.add_call::<_, Option<Block>>(
                "eth_getBlockByNumber",
                &(block_num as BlockNumber, true),
            );
            // @todo must handle error case
            if a.is_ok() {
                waiters.push(a.unwrap());
            }
        }

        batch.send();

        let blocks = futures::future::try_join_all(waiters).await;

        if blocks.is_err() {
            return Err(CoreError::RpcRequestFailed(blocks.err().unwrap().to_string()))
        }
        
        Ok(blocks.unwrap())
    }
}