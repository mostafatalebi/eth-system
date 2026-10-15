use std::future::Future;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::B256;
use alloy::providers::{ProviderBuilder, WsConnect};
use alloy::rpc::client::{ClientBuilder, RpcClient};
use alloy::rpc::types::Block;
use clap::builder::Str;
use crate::error::CoreError;

pub trait EthClient {
    fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> impl Future<Output = Result<Vec<Option<Block>>, CoreError>> + Send;

    fn get_last_finalized_block(&self) -> impl Future<Output = Result<Option<Block>, CoreError>> + Send;

    fn get_block_by_hash(&self, hash: B256) -> impl Future<Output = Result<Option<Block>, CoreError>> + Send;
}


pub struct AlloyRpcClient {
    inner_rpc: RpcClient
}

impl AlloyRpcClient {
    pub async fn new(upstream_addr: &str) -> Result<Self, CoreError> {
        let client = ClientBuilder::default().http(upstream_addr.parse().unwrap());
        Ok(Self {
            inner_rpc: client
        })
    }
}

impl EthClient for AlloyRpcClient {
    async fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> Result<Vec<Option<Block>>, CoreError> {
        let mut batch = self.inner_rpc.new_batch();

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

    async fn get_last_finalized_block(&self) -> Result<Option<Block>, CoreError> {
        let finalized_block: Option<Block> = self.inner_rpc.request("eth_getBlockByNumber", (BlockNumberOrTag::Finalized, false))
            .await.map_err(|e|{
            CoreError::RpcRequestFailed(e.to_string())
        })?;


        Ok(finalized_block)
    }

    async fn get_block_by_hash(&self, hash: B256) -> Result<Option<Block>, CoreError> {
        self.inner_rpc.request("eth_getBlockByHash", (hash, true)).await
            .map_err(|e| CoreError::RpcRequestFailed(e.to_string()))
    }
}