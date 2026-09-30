use alloy::eips::BlockNumberOrTag;
use alloy::providers::{ProviderBuilder, WsConnect};
use alloy::rpc::client::{ClientBuilder, RpcClient};
use alloy::rpc::types::Block;
use clap::builder::Str;
use crate::error::CoreError;

pub trait EthClient {
    async fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> Result<Vec<Option<Block>>, CoreError>;
}


pub struct AlloyRpcClient {
    inner: RpcClient
}

impl AlloyRpcClient {
    pub async fn new(upstream_addr: &str) -> Result<Self, CoreError> {
        let client = ClientBuilder::default().http(upstream_addr.parse().unwrap());
        Ok(Self {
            inner: client
        })
    }
}

impl EthClient for AlloyRpcClient {
    async fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> Result<Vec<Option<Block>>, CoreError> {
        let mut batch = self.inner.new_batch();

        let mut waiters = Vec::new();
        for block_num in start_block_number.unwrap()..start_block_number.unwrap() + size {
            let a = batch.add_call::<_, Option<Block>>(
                "eth_getBlockByNumber",
                &(BlockNumberOrTag::Number(block_num as u64), true),
            );
            // @todo must handle error case
            if a.is_ok() {
                waiters.push(a.unwrap());
            }
        }

        batch.send().await.map_err(|e|{
            CoreError::RpcRequestFailed(e.to_string())
        })?;

        let blocks = futures::future::try_join_all(waiters).await;

        if blocks.is_err() {
            return Err(CoreError::RpcRequestFailed(blocks.err().unwrap().to_string()))
        }
        
        Ok(blocks.unwrap())
    }
}