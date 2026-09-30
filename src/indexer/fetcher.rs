use std::sync::Arc;
use std::time::Duration;
use alloy::rpc::types::{Block, BlockTransactions};
use futures_util::SinkExt;
use tokio::sync::mpsc::Receiver;
use tracing::{error, info};
use crate::error::CoreError;
use crate::indexer::client::eth::EthClient;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::transaction::TransactionModel;
use crate::indexer::repository::repository::BlockRepository;
use crate::indexer::validator::block_validator::BlockValidator;

pub struct FetchConfig {
    ws_url: String,
    rpc_fetch_batch_size: u64,
    db_persistence_batch_size: usize,
    blocks_tbl_name: String
}

impl FetchConfig {
    pub fn new() -> Self {
        Self {
            ws_url: String::new(),
            rpc_fetch_batch_size: 0,
            blocks_tbl_name: "".to_string(),
            db_persistence_batch_size: 5,
        }
    }

    pub fn set_ws_url(&mut self, ws_url: String) {
        self.ws_url = ws_url;
    }

    pub fn set_batch_size(&mut self, batch_size: u64) {
        self.rpc_fetch_batch_size = batch_size;
    }

    pub fn set_blocks_tbl_name(&mut self, blocks_tbl_name: String) {
        self.blocks_tbl_name = blocks_tbl_name;
    }
}


pub struct Fetcher<R: BlockRepository, C: EthClient> {
    pub inner: Arc<FetcherInner<R, C>>
}

impl<R: BlockRepository, C: EthClient> Clone for Fetcher<R, C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone()
        }
    }
}

impl<R: BlockRepository, C: EthClient> Fetcher<R, C> {
    pub fn new(config: FetchConfig, repo: R, client: C, cancellation: tokio_util::sync::CancellationToken) -> Self
        where R: BlockRepository{
        Self {
            inner: Arc::new(FetcherInner::new(config, repo, client, cancellation))
        }
    }
    pub fn new_with_fi(inner: Arc<FetcherInner<R, C>>) -> Self {
        Self { inner }
    }

    pub async fn listen_to_incoming_blocks(&self, in_chan: Receiver<Block>) -> Result<(), CoreError> {
        self.inner.block_processor(None, in_chan).await
    }
}



pub struct FetcherInner<R: BlockRepository, C> {
    config: FetchConfig,
    pub repo: R,
    client: C,
    cancellation: tokio_util::sync::CancellationToken
}


impl<'a, R: BlockRepository, C: EthClient> FetcherInner<R, C> {
    pub fn new(config: FetchConfig, repo: R, client: C, cancellation: tokio_util::sync::CancellationToken)
        -> FetcherInner<R, C>
        where R: BlockRepository
         {
        Self {
            config,
            repo,
            client,
            cancellation
        }
    }

    /// prepares an RPC batch operation and loads blocks from upstream ethereum
    /// network. It then publishes the retrieved blocks into an outgoing chan
    /// defined in FetchConfig
    /// It returns last processed block number
    pub async fn block_fetcher(&self, start_block: Option<u64>, out_chan: tokio::sync::mpsc::Sender<Block>) -> Result<u64,CoreError> {
        info!("starting block fetcher");
        let mut last_block_number = start_block;
        let blocks = self.client.get_blocks(last_block_number, self.config.rpc_fetch_batch_size).await?;
        let num_of_fetched_blocks = blocks.len();

        // @todo this is happy case; we need to add failure handling
        for block in blocks {
            if block.is_none() {
                // non-existing block
                continue
            }
            let res = out_chan.send(block.unwrap()).await;
            if res.is_err() {
                return Err(CoreError::Internal(Box::new(res.err().unwrap())));
            }
        }

        Ok(last_block_number.unwrap()+num_of_fetched_blocks as u64)
    }

    pub async fn block_processor(&self, start_block_number: Option<u64>, mut in_chan: Receiver<Block>) -> Result<(), CoreError> {
        info!("starting indexer block processor");
        let mut block_models_list = Vec::new();
        let mut transactions_list: Vec<Vec<TransactionModel>> = Vec::new();
        // number of the block that must come next; None until the first block arrives
        let mut expected_block_number = start_block_number;
        loop {
            tokio::select! {
                Some(block) = in_chan.recv() => {
                    let err = BlockValidator::validate_rpc_block(&block, expected_block_number);
                    if err.is_err() {
                        // @todo though this is weird
                        error!("block validation error: {:?}", err);
                    }
                    let block_model = BlockModel::new_from_rpc_block(&block);
                    if block_model.is_err() {
                        error!("block model error, cannot continue: {:?}", block_model);
                        return block_model.map(|_| ());
                    }

                    let transaction_models = Self::transaction_models_of_block(&block);
                    if transaction_models.is_err() {
                        error!("transaction model error, cannot continue: {:?}", transaction_models);
                        return transaction_models.map(|_| ());
                    }

                    block_models_list.push(block_model.unwrap());
                    transactions_list.push(transaction_models?);
                    expected_block_number = Some(block.header.number + 1);
                    let mut must_persist = true;
                    while must_persist {
                        if block_models_list.len() == self.config.db_persistence_batch_size {
                            let res = self.repo.upsert_blocks(&block_models_list, &transactions_list).await;
                            if res.is_err() {
                                error!("error on upserting blocks: {:?}", res);
                                // must retry
                                tokio::time::sleep(Duration::from_millis(400)).await;
                                continue;
                            }
                            must_persist = false;
                            block_models_list.truncate(0);
                            transactions_list.truncate(0);
                        } else {
                            break
                        }
                    }
                },

                _ = self.cancellation.cancelled() => {
                    if block_models_list.len() > 0 {
                            let res = self.repo.upsert_blocks(&block_models_list, &transactions_list).await;
                            if res.is_err() {
                                error!("error on upserting blocks: {:?}", res);
                                // must retry
                                tokio::time::sleep(Duration::from_millis(400)).await;
                                continue;
                            }
                            block_models_list.truncate(0);
                            transactions_list.truncate(0);
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// builds the transaction models of a block. Blocks must be fetched with full
    /// transaction objects; a block carrying only transaction hashes cannot be indexed
    fn transaction_models_of_block(block: &Block) -> Result<Vec<TransactionModel>, CoreError> {
        match &block.transactions {
            BlockTransactions::Full(txs) => txs.iter()
                .map(TransactionModel::new_from_rpc_transaction)
                .collect(),
            BlockTransactions::Hashes(hashes) if hashes.is_empty() => Ok(vec![]),
            _ => Err(CoreError::BadArgument(format!(
                "block {} does not contain full transactions", block.header.number))),
        }
    }
}


#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use alloy::providers::ProviderBuilder;
    use alloy::rpc::types::{BlockTransactions, Header};
    use super::*;
    use alloy::transports::mock::Asserter;
    use clap::Parser;
    use deadpool_postgres::GenericClient;
    use crate::app::cmd::CmdArgs;
    use crate::config::config::Config;
    use crate::db::db::DbFactory;
    use crate::db::db::DbType::Postgres;
    use crate::indexer::client::eth_mock::EthereumMockClient;
    use crate::indexer::repository::mock_repository::{MockRepository, MockRepositoryInner};
    use crate::indexer::repository::pg_repository::PgBlockRepository;
    use crate::indexer::types::mocks::mock_blocks_list;

    #[tokio::test]
    pub async fn test_fetching_blocks() {
        let mut eth_mock = EthereumMockClient::new();
        let mut blocks: Vec<Option<Block>> = mock_blocks_list(10, 1);
        let mut block_tx = BlockTransactions::Full(Default::default());
        let b1 = Some(Block::new(Header::new(alloy::consensus::Header::default()), block_tx));
        blocks.push(b1);
        let result = Ok(blocks);
        eth_mock.set_response(result);

        let cnf = FetchConfig::new();
        let db = DbFactory::default();
        let repo = MockRepository::new();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let fetcher = Fetcher::new(cnf, repo.clone(), eth_mock, cancellation.clone());
        let (sn, rc) = tokio::sync::mpsc::channel(100);

        let cancel2 = cancellation.clone();

        let res = fetcher.inner.block_fetcher(Some(1u64), sn).await;
        assert_eq!(res.is_ok(), true);

        let fetcher_cloned = fetcher.clone();

        tokio::spawn(async move {
            fetcher_cloned.listen_to_incoming_blocks(rc).await
        });


        tokio::time::sleep(Duration::from_millis(1000)).await;
        cancel2.cancel();

        assert_eq!(repo.len_blocks(), 10)
    }


    #[cfg(feature = "integration-tests")]
    #[tokio::test]
    pub async fn test_fetching_blocks_with_db() {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let mut eth_mock = EthereumMockClient::new();
        let mut blocks: Vec<Option<Block>> = mock_blocks_list(10, 1);
        let block_tx = BlockTransactions::Full(Default::default());
        let b1 = Some(Block::new(Header::new(alloy::consensus::Header::default()), block_tx));
        blocks.push(b1);
        let result = Ok(blocks);
        eth_mock.set_response(result);

        let cnf = FetchConfig::new();
        let config_file = std::env::var("CONFIG_FILE").unwrap_or_default();
        assert_ne!(config_file, "");
        let app_config = Config::load_from_file(config_file);
        assert_eq!(app_config.is_ok(), true);
        let app_config = app_config.unwrap();
        assert_ne!(app_config.db_pg_url, "");
        let db = Arc::new(DbFactory::connect(&app_config.db_pg_url, Postgres).await.unwrap());
        let mut client = db.pg().await.map_err(|e|{
            CoreError::DbConnFailed(e.to_string())
        });
        assert_eq!(client.is_err(), false, "DB connection error");
        if client.is_err() {
            error!("{}", client.err().unwrap());
            return;
        }
        let mut client = client.unwrap();
        let report = crate::migrations::migration_runner::migrations::runner()
            .run_async(std::ops::DerefMut::deref_mut(std::ops::DerefMut::deref_mut(&mut client))).await;
        assert!(report.is_ok(), "failed to run migrations: {:?}", report.err());
        let res = client.query("delete from blocks where number != -1", &[]).await;
        assert_eq!(res.is_ok(), true, "failed to clean up tables before running test");
        let repo = PgBlockRepository::new(db.clone());
        let cancellation = tokio_util::sync::CancellationToken::new();
        let fetcher = Fetcher::new(cnf, repo, eth_mock, cancellation.clone());
        let (sn, rc) = tokio::sync::mpsc::channel(100);

        let cancel2 = cancellation.clone();

        let res = fetcher.inner.block_fetcher(Some(164), sn).await;
        assert_eq!(res.is_ok(), true);

        let fetcher_cloned = fetcher.clone();

        tokio::spawn(async move {
            fetcher_cloned.listen_to_incoming_blocks(rc).await
        });


        tokio::time::sleep(Duration::from_millis(1000)).await;
        cancel2.cancel();


        let row = client.query_one("select count(*) as count from blocks", &[]).await.unwrap();
        let count: i64 = row.get("count");
        println!("count: {}", count);
        assert_eq!(count, 10, "expected 10 blocks persisted, found {}", count);
    }
}