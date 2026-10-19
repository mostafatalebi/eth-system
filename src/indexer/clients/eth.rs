use std::future::Future;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::B256;
use alloy::providers::{ProviderBuilder, WsConnect};
use alloy::rpc::client::{ClientBuilder, RpcClient};
use alloy::rpc::types::{Block, TransactionReceipt};
use clap::builder::Str;
use crate::error::CoreError;

pub trait EthClient {
    fn get_blocks(&self, start_block_number: Option<u64>, size: u64) -> impl Future<Output = Result<Vec<Option<Block>>, CoreError>> + Send;

    fn get_last_finalized_block(&self) -> impl Future<Output = Result<Option<Block>, CoreError>> + Send;

    fn get_block_by_hash(&self, hash: B256) -> impl Future<Output = Result<Option<Block>, CoreError>> + Send;

    /// returns the receipts of every transaction of the block, in transaction order;
    /// asking by hash makes a block replaced by a reorg fail instead of returning another block's receipts
    fn get_block_receipts(&self, hash: B256) -> impl Future<Output = Result<Vec<TransactionReceipt>, CoreError>> + Send;
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

    async fn get_block_receipts(&self, hash: B256) -> Result<Vec<TransactionReceipt>, CoreError> {
        let receipts: Option<Vec<TransactionReceipt>> = self.inner_rpc.request("eth_getBlockReceipts", (hash,)).await
            .map_err(|e| CoreError::RpcRequestFailed(e.to_string()))?;
        receipts.ok_or(CoreError::RpcRequestFailed(format!("receipts of block {} not found", hash)))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::clients::eth_mock::EthereumMockClient;

    fn receipt() -> TransactionReceipt {
        let zero_hash = format!("0x{}", "00".repeat(32));
        serde_json::from_value(serde_json::json!({
            "type": "0x2", "status": "0x1", "cumulativeGasUsed": "0x5208", "logs": [],
            "logsBloom": format!("0x{}", "00".repeat(256)),
            "transactionHash": zero_hash, "transactionIndex": "0x0",
            "blockHash": zero_hash, "blockNumber": "0x1", "gasUsed": "0x5208", "effectiveGasPrice": "0x1",
            "from": format!("0x{}", "00".repeat(20)), "to": format!("0x{}", "00".repeat(20)), "contractAddress": null
        })).unwrap()
    }

    #[tokio::test]
    async fn test_mock_returns_the_configured_receipts() {
        let mut client = EthereumMockClient::new();
        assert!(client.get_block_receipts(B256::ZERO).await.unwrap().is_empty());
        client.set_receipts(vec![receipt()]);
        assert_eq!(client.get_block_receipts(B256::ZERO).await.unwrap().len(), 1);
    }

    /// talks to the real upstream from the config file; the receipts must line up with the block's transactions
    #[cfg(feature = "integration-tests")]
    #[tokio::test]
    async fn test_block_receipts_match_the_transactions_of_the_block() {
        let config_file = std::env::var("CONFIG_FILE").unwrap_or_default();
        assert_ne!(config_file, "");
        let config = crate::config::config::Config::load_from_file(config_file).unwrap();
        let client = AlloyRpcClient::new(&config.block_upstream_http_addr).await.unwrap();

        let block = client.get_last_finalized_block().await.unwrap().unwrap();
        let block = client.get_block_by_hash(block.header.hash).await.unwrap().unwrap();
        let receipts = client.get_block_receipts(block.header.hash).await.unwrap();

        let tx_hashes: Vec<B256> = block.transactions.hashes().collect();
        assert!(!tx_hashes.is_empty());
        assert_eq!(receipts.len(), tx_hashes.len());
        for (i, receipt) in receipts.iter().enumerate() {
            assert_eq!(receipt.transaction_hash, tx_hashes[i]);
            assert_eq!(receipt.transaction_index, Some(i as u64));
        }
        assert!(receipts.iter().any(|r| !r.inner.logs().is_empty()), "a mainnet block is expected to contain logs");
    }
}
