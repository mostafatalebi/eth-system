use std::sync::Arc;
use std::time::Duration;
use alloy::primitives::B256;
use alloy::rpc::types::{Block, BlockTransactions, TransactionReceipt};
use tokio::sync::mpsc::Receiver;
use tracing::{error, info};
use crate::error::CoreError;
use crate::indexer::clients::eth::EthClient;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::log::LogModel;
use crate::indexer::models::transaction::TransactionModel;
use crate::indexer::repository::repository::{BlockData, BlockRepository};
use crate::indexer::validator::block_validator::BlockValidator;

/// a block together with the receipts of its transactions, as the fetcher hands it to the processor
pub struct FetchedBlock {
    pub block: Block,
    pub receipts: Vec<TransactionReceipt>,
}

pub struct FetchConfig {
    ws_url: String,
    rpc_fetch_batch_size: u64,
    db_persistence_batch_size: usize,
}

impl FetchConfig {
    pub fn new() -> Self {
        Self {
            ws_url: String::new(),
            rpc_fetch_batch_size: 0,
            db_persistence_batch_size: 5,
        }
    }

    pub fn set_ws_url(&mut self, ws_url: String) {
        self.ws_url = ws_url;
    }

    pub fn set_batch_size(&mut self, batch_size: u64) {
        self.rpc_fetch_batch_size = batch_size;
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

    #[cfg(test)]
    pub async fn listen_to_incoming_blocks(&self, in_chan: Receiver<FetchedBlock>) -> Result<(), CoreError> {
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
    pub async fn block_fetcher(&self, start_block: Option<u64>, out_chan: tokio::sync::mpsc::Sender<FetchedBlock>) -> Result<u64,CoreError> {
        info!("starting block fetcher");
        let last_block_number = start_block;
        let blocks = self.client.get_blocks(last_block_number, self.config.rpc_fetch_batch_size).await?;
        let num_of_fetched_blocks = blocks.len();

        // receipts of the whole batch are fetched and validated before anything is sent, so a failure
        // makes the caller retry the batch without the processor having received part of it
        let fetched = futures::future::try_join_all(blocks.into_iter().flatten().map(|b| self.fetch_receipts(b))).await?;

        // @todo this is happy case; we need to add failure handling
        for fetched_block in fetched {
            let res = out_chan.send(fetched_block).await;
            if res.is_err() {
                return Err(CoreError::Internal(Box::new(res.err().unwrap())));
            }
        }

        Ok(last_block_number.unwrap()+num_of_fetched_blocks as u64)
    }

    /// fetches the receipts of the block and checks they match its transactions
    async fn fetch_receipts(&self, block: Block) -> Result<FetchedBlock, CoreError> {
        let receipts = self.client.get_block_receipts(block.header.hash).await?;
        BlockValidator::validate_receipts(&block, &receipts)?;
        Ok(FetchedBlock { block, receipts })
    }

    pub async fn block_processor(&self, start_block_number: Option<u64>, mut in_chan: Receiver<FetchedBlock>) -> Result<(), CoreError> {
        info!("starting indexer block processor");
        let mut block_models_list = Vec::new();
        let mut transactions_list: Vec<Vec<TransactionModel>> = Vec::new();
        let mut logs_list: Vec<Vec<LogModel>> = Vec::new();
        // number of the block that must come next; None until the first block arrives
        let mut expected_block_number = start_block_number;
        loop {
            tokio::select! {
                Some(fetched) = in_chan.recv() => {
                    let block = &fetched.block;
                    let err = BlockValidator::validate_rpc_block(block, expected_block_number);
                    if err.is_err() {
                        // @todo though this is weird
                        error!("block validation error: {:?}", err);
                    }
                    let models = Self::models_of_block(&fetched);
                    if models.is_err() {
                        error!("block model error, cannot continue: {:?}", models);
                        return models.map(|_| ());
                    }
                    let (block_model, transaction_models, log_models) = models?;

                    block_models_list.push(block_model);
                    transactions_list.push(transaction_models);
                    logs_list.push(log_models);
                    expected_block_number = Some(block.header.number + 1);
                    let mut must_persist = true;
                    while must_persist {
                        if block_models_list.len() == self.config.db_persistence_batch_size {
                            let res = self.repo.upsert_blocks(&BlockData::new(&block_models_list, &transactions_list, &logs_list)).await;
                            if let Err(CoreError::DbDuplicateBlockWithDifferentHash(_, incoming_hash, _)) = &res {
                                let conflicting = block_models_list.iter().find(|b| &b.hash == incoming_hash).cloned()
                                    .ok_or(CoreError::BadArgument("conflicting block is not in the batch".to_string()))?;

                                let (ancestor, new_blocks) = self.find_common_ancestor(&conflicting).await?;

                                let mut blocks = Vec::new();
                                let mut txs = Vec::new();
                                let mut logs = Vec::new();
                                for fetched_block in &new_blocks {
                                    let (block_model, transaction_models, log_models) = Self::models_of_block(fetched_block)?;
                                    blocks.push(block_model);
                                    txs.push(transaction_models);
                                    logs.push(log_models);
                                }
                                self.repo.replace_chain_above(ancestor, &BlockData::new(&blocks, &txs, &logs)).await?;
                                continue;
                            } else if res.as_ref().is_err() {
                                error!("error on upserting blocks: {:?}", res);
                                // must retry
                                tokio::time::sleep(Duration::from_millis(400)).await;
                                continue;
                            }
                            must_persist = false;
                            block_models_list.truncate(0);
                            transactions_list.truncate(0);
                            logs_list.truncate(0);
                        } else {
                            break
                        }
                    }
                },

                _ = self.cancellation.cancelled() => {
                    if block_models_list.len() > 0 {
                            let res = self.repo.upsert_blocks(&BlockData::new(&block_models_list, &transactions_list, &logs_list)).await;
                            if res.is_err() {
                                error!("error on upserting blocks: {:?}", res);
                                // must retry
                                tokio::time::sleep(Duration::from_millis(400)).await;
                                continue;
                            }
                            block_models_list.truncate(0);
                            transactions_list.truncate(0);
                            logs_list.truncate(0);
                    }
                    return Ok(());
                }
            }
        }
    }

    async fn find_common_ancestor(&self, conflicting: &BlockModel) -> Result<(i64, Vec<FetchedBlock>), CoreError> {
        let finalized = self.client.get_last_finalized_block().await?
            .map(|b| b.header.number as i64)
            .unwrap_or(0);

        let mut parent_hash = conflicting.parent_hash.clone();
        let mut new_chain = Vec::new();
        loop {
            if let Some(ancestor) = self.repo.get_block(Some(parent_hash.clone()), None).await? {
                new_chain.reverse();
                return Ok((ancestor.number, new_chain));
            }
            let block = self.client.get_block_by_hash(B256::from_slice(&parent_hash)).await?
                .ok_or(CoreError::ParentBlockNotFound(parent_hash.clone()))?;
            if block.header.number as i64 <= finalized {
                return Err(CoreError::ReorgBelowFinalized(block.header.number));
            }
            parent_hash = block.header.parent_hash.to_vec();
            new_chain.push(self.fetch_receipts(block).await?);
        }
    }

    /// builds the models to persist for a block: the block row, its transactions (completed with the
    /// data of their receipts) and its logs. The receipts must have been validated against the block
    pub(crate) fn models_of_block(fetched: &FetchedBlock) -> Result<(BlockModel, Vec<TransactionModel>, Vec<LogModel>), CoreError> {
        let block_model = BlockModel::new_from_rpc_block(&fetched.block)?;
        let mut transaction_models = Self::transaction_models_of_block(&fetched.block)?;
        if transaction_models.len() != fetched.receipts.len() {
            return Err(CoreError::ReceiptsInvalid(format!(
                "block {} has {} transactions but {} receipts", fetched.block.header.number, transaction_models.len(), fetched.receipts.len())));
        }
        for (transaction, receipt) in transaction_models.iter_mut().zip(fetched.receipts.iter()) {
            transaction.apply_receipt(receipt);
        }
        let log_models = fetched.receipts.iter()
            .flat_map(|receipt| receipt.inner.logs())
            .map(LogModel::new_from_rpc_log)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((block_model, transaction_models, log_models))
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
    use alloy::rpc::types::{BlockTransactions, Header};
    use super::*;
    use crate::db::db::DbFactory;
    use crate::indexer::clients::eth_mock::EthereumMockClient;
    use crate::indexer::repository::mock_repository::MockRepository;
    use crate::indexer::types::mocks::{mock_block_with_parent, mock_block_with_receipts, mock_blocks_list};

    type TestFetcherInner = FetcherInner<MockRepository, EthereumMockClient>;

    // the models of a block carry the data of its receipts: receipt columns on the transactions and one log row per log
    #[test]
    fn test_models_of_block_merge_receipts_and_flatten_logs() {
        let (block, receipts) = mock_block_with_receipts(7, B256::random(), &[2, 0, 3]);
        let tx_hashes: Vec<Vec<u8>> = receipts.iter().map(|r| r.transaction_hash.to_vec()).collect();

        let (block_model, transactions, logs) = TestFetcherInner::models_of_block(&FetchedBlock { block, receipts }).unwrap();

        assert_eq!(block_model.number, 7);
        assert_eq!(transactions.len(), 3);
        for transaction in &transactions {
            assert_eq!(transaction.receipt_status, Some(true));
            assert_eq!(transaction.receipt_gas_used, Some(21_000));
            assert_eq!(transaction.receipt_effective_gas_price.as_deref(), Some("1000000000"));
            assert_eq!(transaction.receipt_contract_address, None);
        }
        // 2 + 0 + 3 logs, numbered across the block, each pointing at the transaction that emitted it
        assert_eq!(logs.iter().map(|l| l.log_index).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
        let owners: Vec<&Vec<u8>> = logs.iter().map(|l| &l.tx_hash).collect();
        assert_eq!(owners, vec![&tx_hashes[0], &tx_hashes[0], &tx_hashes[2], &tx_hashes[2], &tx_hashes[2]]);
        assert!(logs.iter().all(|l| l.block_hash == block_model.hash && l.block_number == 7));
    }

    #[test]
    fn test_models_of_block_rejects_receipts_that_do_not_match_the_transactions() {
        let (block, mut receipts) = mock_block_with_receipts(7, B256::random(), &[1, 1]);
        receipts.pop();
        let res = TestFetcherInner::models_of_block(&FetchedBlock { block, receipts });
        assert!(matches!(res, Err(CoreError::ReceiptsInvalid(_))));
    }

    // the processor persists the logs of the blocks it receives
    #[tokio::test]
    async fn test_processor_persists_logs() {
        let (block, receipts) = mock_block_with_receipts(1, B256::random(), &[2, 1]);
        let repo = MockRepository::new();
        let mut config = FetchConfig::new();
        config.db_persistence_batch_size = 1;
        let fetcher = Fetcher::new(config, repo.clone(), EthereumMockClient::new(), tokio_util::sync::CancellationToken::new());
        let (sn, rc) = tokio::sync::mpsc::channel(10);
        let inner = fetcher.inner.clone();
        let handle = tokio::spawn(async move { inner.block_processor(None, rc).await });
        sn.send(FetchedBlock { block, receipts }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while repo.len_blocks() < 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("block was not persisted");
        fetcher.inner.cancellation.cancel();
        handle.await.unwrap().unwrap();

        assert_eq!(repo.logs().len(), 3);
    }

    // a batch whose receipts do not match its block is not sent on; the caller retries it
    #[tokio::test]
    async fn test_block_fetcher_rejects_a_batch_with_mismatched_receipts() {
        let (block, mut receipts) = mock_block_with_receipts(1, B256::random(), &[1, 1]);
        receipts.pop();
        let mut client = EthereumMockClient::new();
        client.set_response(Ok(vec![Some(block)]));
        client.set_receipts(receipts);
        let mut config = FetchConfig::new();
        config.set_batch_size(1);
        let fetcher = Fetcher::new(config, MockRepository::new(), client, tokio_util::sync::CancellationToken::new());
        let (sn, mut rc) = tokio::sync::mpsc::channel(10);

        let res = fetcher.inner.block_fetcher(Some(1), sn).await;

        assert!(matches!(res, Err(CoreError::ReceiptsInvalid(_))));
        assert!(rc.try_recv().is_err(), "nothing must be sent");
    }

    #[tokio::test]
    pub async fn test_fetching_blocks() {
        let mut eth_mock = EthereumMockClient::new();
        let mut blocks: Vec<Option<Block>> = mock_blocks_list(10, 1);
        let block_tx = BlockTransactions::Full(Default::default());
        let b1 = Some(Block::new(Header::new(alloy::consensus::Header::default()), block_tx));
        blocks.push(b1);
        let result = Ok(blocks);
        eth_mock.set_response(result);

        let cnf = FetchConfig::new();
        let _db = DbFactory::default();
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


    fn stored_chain() -> Vec<Block> {
        mock_blocks_list(5, 1).into_iter().flatten().collect()
    }

    fn model(block: &Block) -> BlockModel {
        BlockModel::new_from_rpc_block(block).unwrap()
    }

    fn setup(stored: &[Block], node: &[Block], finalized: Option<Block>) -> (MockRepository, Fetcher<MockRepository, EthereumMockClient>) {
        let repo = MockRepository::new();
        repo.add_blocks(stored.iter().map(model).collect());
        let mut client = EthereumMockClient::new();
        client.set_response(Ok(node.iter().cloned().map(Some).collect()));
        client.set_finalized(finalized);
        let fetcher = Fetcher::new(FetchConfig::new(), repo.clone(), client, tokio_util::sync::CancellationToken::new());
        (repo, fetcher)
    }

    // new block 5 builds on stored block 4: the ancestor is 4 and nothing has to be fetched
    #[tokio::test]
    async fn test_find_common_ancestor_same_height_reorg() {
        let stored = stored_chain();
        let new_5 = mock_block_with_parent(5, stored[3].header.hash);
        let (_, fetcher) = setup(&stored, &[], None);

        let (ancestor, new_chain) = fetcher.inner.find_common_ancestor(&model(&new_5)).await.unwrap();

        assert_eq!(ancestor, 4);
        assert!(new_chain.is_empty());
    }

    // new blocks 3..5 fork off stored block 2: the ancestor is 2 and new blocks 3, 4 come back oldest first
    #[tokio::test]
    async fn test_find_common_ancestor_deep_reorg_returns_new_chain_oldest_first() {
        let stored = stored_chain();
        let new_3 = mock_block_with_parent(3, stored[1].header.hash);
        let new_4 = mock_block_with_parent(4, new_3.header.hash);
        let new_5 = mock_block_with_parent(5, new_4.header.hash);
        let (_, fetcher) = setup(&stored, &[new_3.clone(), new_4.clone()], None);

        let (ancestor, new_chain) = fetcher.inner.find_common_ancestor(&model(&new_5)).await.unwrap();

        assert_eq!(ancestor, 2);
        let hashes: Vec<B256> = new_chain.iter().map(|b| b.block.header.hash).collect();
        assert_eq!(hashes, vec![new_3.header.hash, new_4.header.hash]);
    }

    // walking back reaches the finalized height without finding a stored parent: the walk stops with an error
    #[tokio::test]
    async fn test_find_common_ancestor_stops_at_finalized_block() {
        let stored = stored_chain();
        let new_4 = mock_block_with_parent(4, B256::random());
        let new_5 = mock_block_with_parent(5, new_4.header.hash);
        let finalized = mock_block_with_parent(4, B256::random());
        let (_, fetcher) = setup(&stored, &[new_4], Some(finalized));

        let result = fetcher.inner.find_common_ancestor(&model(&new_5)).await;

        assert!(matches!(result, Err(CoreError::ReorgBelowFinalized(4))));
    }

    // the parent is neither stored nor known to the node: the walk fails with that parent's hash
    #[tokio::test]
    async fn test_find_common_ancestor_errors_when_parent_missing_on_node() {
        let stored = stored_chain();
        let missing_parent = B256::random();
        let new_5 = mock_block_with_parent(5, missing_parent);
        let (_, fetcher) = setup(&stored, &[], None);

        let result = fetcher.inner.find_common_ancestor(&model(&new_5)).await;

        assert!(matches!(&result, Err(CoreError::ParentBlockNotFound(h)) if *h == missing_parent.to_vec()));
    }

    // a batch conflicting with stored block 5 replaces it and the whole new batch is stored after the retry
    #[tokio::test]
    async fn test_block_processor_replaces_chain_on_reorg() {
        let stored = stored_chain();
        let mut new_chain = vec![mock_block_with_parent(5, stored[3].header.hash)];
        for number in 6..=9 {
            let parent = new_chain.last().unwrap().header.hash;
            new_chain.push(mock_block_with_parent(number, parent));
        }
        let (mut repo, fetcher) = setup(&stored, &[], None);
        repo.expect_upsert_error(CoreError::DbDuplicateBlockWithDifferentHash(
            stored[3].header.hash.to_vec(), new_chain[0].header.hash.to_vec(), stored[4].header.hash.to_vec()));

        let (sn, rc) = tokio::sync::mpsc::channel(10);
        let inner = fetcher.inner.clone();
        let handle = tokio::spawn(async move { inner.block_processor(None, rc).await });
        for block in &new_chain {
            sn.send(FetchedBlock { block: block.clone(), receipts: vec![] }).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while repo.len_blocks() < 9 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("batch was not persisted");
        fetcher.inner.cancellation.cancel();
        handle.await.unwrap().unwrap();

        let hashes = repo.block_hashes();
        assert_eq!(hashes.len(), 9);
        assert!(!hashes.contains(&stored[4].header.hash.to_vec()));
        assert_eq!(hashes[4], new_chain[0].header.hash.to_vec());
    }


    #[cfg(feature = "integration-tests")]
    #[tokio::test]
    pub async fn test_fetching_blocks_with_db() {
        use crate::config::config::Config;
        use crate::db::db::DbType::Postgres;
        use crate::db::test_support::DB_LOCK;
        use crate::indexer::repository::pg_repository::PgBlockRepository;

        let _guard = DB_LOCK.lock().await;
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
        let client = db.pg().await.map_err(|e|{
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


        // wait until the processor has persisted the blocks instead of sleeping a fixed time
        let mut count: i64 = 0;
        for _ in 0..100 {
            let row = client.query_one("select count(*) as count from blocks", &[]).await.unwrap();
            count = row.get("count");
            if count == 10 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        cancel2.cancel();
        assert_eq!(count, 10, "expected 10 blocks persisted, found {}", count);
    }
}