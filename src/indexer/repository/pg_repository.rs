use std::sync::Arc;
use tokio_postgres::{IsolationLevel, Statement, Transaction};
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;
use crate::db::db::DbFactory;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::log::LogModel;
use crate::indexer::models::transaction::TransactionModel;
use crate::indexer::repository::queries::{BLOCK_IS_FINALIZED_QUERY, BLOCK_MARK_FINALIZED_CHECKPOINT_QUERY, LAST_FINALIZED_CHECKPOINT_QUERY, BLOCK_INSERT_QUERY, LOG_INSERT_QUERY, TX_INSERT_QUERY};
use crate::indexer::repository::repository::{BlockData, BlockRepository, LogFilter};

#[derive(Clone)]
pub struct PgBlockRepository {
    db: Arc<DbFactory>
}

impl PgBlockRepository {
    pub fn new(db: Arc<DbFactory>) -> Self {
        Self { db }
    }
}

/// tokio_postgres's Display only prints "db error" for server errors; include the server message
fn pg_err_msg(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{} ({}): {}", db.message(), db.code().code(), db.detail().unwrap_or("")),
        None => e.to_string(),
    }
}

impl BlockRepository for PgBlockRepository {
    /// tries to insert the list of blocks into the backend database. If any error
    /// be raised, the whole list operation will be aborted. The whole operation
    /// would be executed within a transaction, and re-org conditions will be
    /// detected and the corresponding obsolete blocks will be deleted for the
    /// new blocks to be inserted [form the canonical chain]. If At any point
    /// any error be thrown, the transaction will be reverted and retried. If retry doesn't
    /// suffice and fix it, then the operation will be stopped and an error will be inserted
    /// in block_errors table until a manual intervention solve it (this last step is @todo)
    ///
    /// note: any error returned from this function indicates NO change to the database and
    ///       hence a retry is safe.
    async fn upsert_blocks(&self, block_data: &BlockData<'_>) -> Result<(), CoreError> {
        if block_data.blocks.is_empty() {
            return Err(CoreError::Empty("blocks list cannot be empty".to_string()));
        }

        let pg = self.db.pg().await;
        if pg.is_err() {
            return Err(pg.err().unwrap());
        }
        let mut pg = pg?;
        let tx = pg.build_transaction().isolation_level(IsolationLevel::RepeatableRead).start().await;
        if tx.is_err() {
            return Err(CoreError::DbTxOpenFailed(tx.err().unwrap().to_string()));
        }
        let tx = tx.unwrap();
        let bck_stmt = tx.prepare(BLOCK_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        let tx_stmt = tx.prepare(TX_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        let log_stmt = tx.prepare(LOG_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        Self::check_block_data(block_data)?;
        for (i, block) in block_data.blocks.iter().enumerate() {
            let res = self.insert_block_with_children(&[&bck_stmt, &tx_stmt, &log_stmt], &tx, block, &block_data.transactions[i], &block_data.logs[i]).await;
            if let Err(e) = res {
                tx.rollback().await.map_err(|e| CoreError::DbTxRollbackFailed(pg_err_msg(&e)))?;
                return Err(e);
            }
        }

        let res = tx.commit().await;
        if res.is_err() {
            return Err(CoreError::DbTxExecFailed(res.err().unwrap().to_string()));
        }

        return Ok(());
    }

    /// deletes the stored blocks from `oldest_block` through `newest_block` (both inclusive),
    /// together with their transactions (`on delete cascade`).
    /// Runs in a single db transaction: either the whole range is deleted or nothing.
    async fn delete_block_cascading(&self, oldest_block: Vec<u8>, newest_block: Vec<u8>) -> Result<(), CoreError> {

        let mut pg = self.db.pg().await?;
        let tx = pg.build_transaction().isolation_level(IsolationLevel::RepeatableRead).start().await
            .map_err(|e| CoreError::DbTxOpenFailed(pg_err_msg(&e)))?;

        let oldest_number = Self::block_number_by_hash(&tx, &oldest_block).await?;
        let newest_number = Self::block_number_by_hash(&tx, &newest_block).await?;
        // tolerate swapped arguments: always delete the range between the two
        let (from_number, to_number) = (oldest_number.min(newest_number), oldest_number.max(newest_number));

        tx.execute("delete from blocks where number between $1 and $2", &[&from_number, &to_number]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        tx.commit().await
            .map_err(|e| CoreError::DbTxCommitFailed(pg_err_msg(&e)))?;
        Ok(())
    }

    async fn last_block_number(&self) -> Result<Option<u64>, CoreError> {
        let pg = self.db.pg().await?;

        // max() always returns exactly one row; NULL when the table is empty
        let row = pg.query_one("select max(number) from blocks", &[]).await.map_err(|e|{
            return CoreError::DbQueryFailed(e.to_string())
        })?;
        let number: Option<i64> = row.get(0);

        number.map(|n| u64::try_from(n).map_err(|e|{
            return CoreError::DbQueryFailed(format!("invalid block number {n}: {e}"))
        })).transpose()
    }

    async fn block_exists(&self, number: i64, hash: &Vec<u8>, parent_hash: &Vec<u8>) -> Result<bool, CoreError> {
        let pg = self.db.pg().await?;

        let res = pg.query_opt("select number from blocks where number=$1 AND hash=$2 AND parent_hash=$3", &[&number, &hash, &parent_hash]).await.map_err(|e|{
            return CoreError::DbQueryFailed(e.to_string())
        })?;

        Ok(res.is_some())
    }


    async fn get_block(&self, hash: Option<Vec<u8>>, number: Option<i64>) -> Result<Option<BlockModel>, CoreError> {
        let pg = self.db.pg().await?;
        let where_clause;
        let mut bindings: Vec<&(dyn ToSql+Sync)> = Vec::new();
        if hash.is_some() && number.is_some() {
            where_clause = "hash=$1 AND number=$2";
            bindings.push(hash.as_ref().unwrap());
            bindings.push(number.as_ref().unwrap());
        } else if hash.is_some() {
            where_clause = "hash=$1";
            bindings.push(hash.as_ref().unwrap());
        } else {
            where_clause = "number=$1";
            bindings.push(number.as_ref().unwrap());
        }
        let query = format!("select * from blocks where {}", where_clause);
        let Some(res) = pg.query_opt(&query, &bindings.as_slice()).await.map_err(|e|{
            return CoreError::DbQueryFailed(e.to_string())
        })? else {
            return Ok(None);
        };

        let bm = BlockModel::new_from_db_row(&res);

        Ok(Some(bm))
    }

    async fn list_blocks(&self, limit: i64, offset: i64) -> Result<Vec<BlockModel>, CoreError> {
        let pg = self.db.pg().await?;
        let rows = pg.query("select * from blocks order by number desc limit $1 offset $2", &[&limit, &offset]).await
            .map_err(|e| CoreError::DbQueryFailed(e.to_string()))?;
        Ok(rows.iter().map(BlockModel::new_from_db_row).collect())
    }

    async fn list_logs(&self, filter: &LogFilter, limit: i64, offset: i64) -> Result<Vec<LogModel>, CoreError> {
        let pg = self.db.pg().await?;
        let mut conditions: Vec<String> = Vec::new();
        let mut bindings: Vec<&(dyn ToSql + Sync)> = Vec::new();
        // a condition's placeholder is numbered after the values bound so far
        if let Some(address) = &filter.address {
            bindings.push(address);
            conditions.push(format!("address = ${}", bindings.len()));
        }
        for (i, topic) in filter.topics.iter().enumerate() {
            if let Some(topic) = topic {
                bindings.push(topic);
                conditions.push(format!("topic{} = ${}", i, bindings.len()));
            }
        }
        if let Some(from_block) = &filter.from_block {
            bindings.push(from_block);
            conditions.push(format!("block_number >= ${}", bindings.len()));
        }
        if let Some(to_block) = &filter.to_block {
            bindings.push(to_block);
            conditions.push(format!("block_number <= ${}", bindings.len()));
        }
        let where_clause = if conditions.is_empty() { String::new() } else { format!("where {}", conditions.join(" and ")) };
        bindings.push(&limit);
        bindings.push(&offset);
        let query = format!("select * from logs {} order by block_number desc, log_index desc limit ${} offset ${}",
            where_clause, bindings.len() - 1, bindings.len());

        let rows = pg.query(&query, bindings.as_slice()).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        Ok(rows.iter().map(LogModel::new_from_row).collect())
    }

    async fn replace_chain_above(&self, ancestor_number: i64, new_chain: &BlockData<'_>) -> Result<(), CoreError> {
        let mut pg = self.db.pg().await?;
        let tx = pg.build_transaction().isolation_level(IsolationLevel::RepeatableRead).start().await
            .map_err(|e| CoreError::DbTxOpenFailed(pg_err_msg(&e)))?;

        tx.execute("delete from blocks where number > $1", &[&ancestor_number]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;

        let stmt = tx.prepare(BLOCK_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        let tx_stmt = tx.prepare(TX_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        let log_stmt = tx.prepare(LOG_INSERT_QUERY).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;

        Self::check_block_data(new_chain)?;
        for (i, block) in new_chain.blocks.iter().enumerate() {
            self.insert_block_with_children(&[&stmt, &tx_stmt, &log_stmt], &tx, block, &new_chain.transactions[i], &new_chain.logs[i]).await?;
        }

        tx.commit().await
            .map_err(|e| CoreError::DbTxCommitFailed(pg_err_msg(&e)))?;
        Ok(())
    }

    async fn mark_finalized_checkpoint(&self, hash: Vec<u8>) -> Result<bool, CoreError> {
        let pg = self.db.pg().await?;
        let updated = pg.execute(BLOCK_MARK_FINALIZED_CHECKPOINT_QUERY, &[&hash]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        Ok(updated == 1)
    }

    async fn is_block_finalized(&self, number: i64) -> Result<bool, CoreError> {
        let pg = self.db.pg().await?;
        let row = pg.query_one(BLOCK_IS_FINALIZED_QUERY, &[&number]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        Ok(row.get("finalized"))
    }

    async fn last_finalized_checkpoint(&self) -> Result<Option<i64>, CoreError> {
        let pg = self.db.pg().await?;
        let row = pg.query_one(LAST_FINALIZED_CHECKPOINT_QUERY, &[]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        Ok(row.get("number"))
    }

    async fn get_block_by_parent_hash(&self, p_hash: Vec<u8>) -> Result<Option<BlockModel>, CoreError> {
        let pg = self.db.pg().await?;

        let Some(res) = pg.query_opt("select * from blocks where parent_hash = $1", &[&p_hash]).await.map_err(|e|{
            return CoreError::DbQueryFailed(e.to_string())
        })? else {
            return Ok(None);
        };

        let bm = BlockModel::new_from_db_row(&res);

        Ok(Some(bm))
    }
}


impl PgBlockRepository {
    async fn block_number_by_hash(pg_tx: &Transaction<'_>, hash: &Vec<u8>) -> Result<i64, CoreError> {
        let row = pg_tx.query_opt("select number from blocks where hash = $1", &[hash]).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?
            .ok_or(CoreError::DbNotFound(format!("block with hash 0x{}", alloy::hex::encode(hash))))?;
        Ok(row.get("number"))
    }

    /// returns false if the very same block is already stored, true if it has been inserted
    async fn upsert_single_block(&self, stmt: &Statement, pg_tx: &Transaction<'_>, block: &BlockModel) -> Result<bool, CoreError> {
            let existing = pg_tx.query_opt("select hash from blocks where number = $1", &[&block.number]).await
                .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
            if let Some(row) = existing {
                let existing_hash: Vec<u8> = row.get("hash");
                if existing_hash == block.hash {
                    // this ensures exactly same block cannot exist; and helps us to have a safe
                    // and simpler retry
                    return Ok(false);
                }
                return Err(CoreError::DbDuplicateBlockWithDifferentHash(block.parent_hash.clone(), block.hash.clone(), existing_hash));
            }

            let values = block.get_prepared_values();
            pg_tx.execute(stmt, values.as_slice()).await
                .map_err(|e| match e.code() {
                    Some(&SqlState::FOREIGN_KEY_VIOLATION) => CoreError::ParentHashMismatch(block.hash.clone()),
                    _ => CoreError::DbInsertFailed(pg_err_msg(&e)),
                })?;
        Ok(true)
    }

    /// inserts a block, then its transactions, then its logs; `statements` are the prepared
    /// block, transaction and log inserts, in that order
    async fn insert_block_with_children(&self, statements: &[&Statement; 3], pg_tx: &Transaction<'_>, block: &BlockModel,
                                        transactions: &Vec<TransactionModel>, logs: &Vec<LogModel>) -> Result<(), CoreError> {
        // a block that is already stored has been stored with its transactions and logs
        if !self.upsert_single_block(statements[0], pg_tx, block).await? {
            return Ok(());
        }
        self.insert_transactions_of_a_block(statements[1], pg_tx, block, transactions).await?;
        self.insert_logs_of_a_block(statements[2], pg_tx, block, logs).await
    }

    fn check_block_data(block_data: &BlockData<'_>) -> Result<(), CoreError> {
        if block_data.transactions.len() != block_data.blocks.len() || block_data.logs.len() != block_data.blocks.len() {
            return Err(CoreError::BadArgument(format!(
                "{} blocks but {} transaction lists and {} log lists", block_data.blocks.len(), block_data.transactions.len(), block_data.logs.len())));
        }
        Ok(())
    }

    async fn insert_logs_of_a_block(&self, stmt: &Statement, pg_tx: &Transaction<'_>, block: &BlockModel, logs: &Vec<LogModel>) -> Result<(), CoreError> {
        for log in logs {
            if log.block_hash != block.hash {
                return Err(CoreError::BadArgument(format!(
                    "log {} of transaction 0x{} does not belong to block {:?}", log.log_index, alloy::hex::encode(&log.tx_hash), block.hash)));
            }
            pg_tx.execute(stmt, log.get_prepared_values().as_slice()).await
                .map_err(|e| CoreError::DbTxExecFailed(pg_err_msg(&e)))?;
        }
        Ok(())
    }

    async fn insert_transactions_of_a_block(&self, stmt: &Statement, pg_tx: &Transaction<'_>, block: &BlockModel, transactions: &Vec<TransactionModel>) -> Result<(), CoreError> {
        for transaction in transactions {
            if transaction.block_hash != block.hash {
                return Err(CoreError::BadArgument(format!(
                    "transaction {:?} does not belong to block {:?}", transaction.hash, block.hash)));
            }
            pg_tx.execute(stmt, transaction.get_prepared_values().as_slice()).await
                .map_err(|e| CoreError::DbTxExecFailed(pg_err_msg(&e)))?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use alloy::primitives::B256;
    use alloy::rpc::types::Block;
    use crate::config::config::Config;
    use crate::db::db::DbType::Postgres;
    use crate::db::test_support::DB_LOCK;
    use crate::indexer::clients::eth_mock::EthereumMockClient;
    use crate::indexer::models::log::LogModel;
    use crate::indexer::types::mocks::{mock_block_with_parent, mock_block_with_receipts, mock_blocks_list};

    async fn setup() -> (PgBlockRepository, Arc<DbFactory>, tokio::sync::MutexGuard<'static, ()>) {
        let guard = DB_LOCK.lock().await;
        let config_file = std::env::var("CONFIG_FILE").expect("CONFIG_FILE must point to a config with test DB credentials");
        let config = Config::load_from_file(config_file).unwrap();
        let db = Arc::new(DbFactory::connect(&config.db_pg_url, Postgres).await.unwrap());
        let mut client = db.pg().await.unwrap();
        crate::migrations::migration_runner::migrations::runner()
            .run_async(std::ops::DerefMut::deref_mut(std::ops::DerefMut::deref_mut(&mut client))).await.unwrap();
        client.batch_execute("truncate blocks, transactions, logs").await.unwrap();
        (PgBlockRepository::new(db.clone()), db, guard)
    }

    fn chain(count: usize, start: u64) -> Vec<Block> {
        mock_blocks_list(count, start).into_iter().flatten().collect()
    }

    async fn insert_blocks(repo: &PgBlockRepository, blocks: &[Block]) -> Result<(), CoreError> {
        let models: Vec<BlockModel> = blocks.iter().map(|b| BlockModel::new_from_rpc_block(b).unwrap()).collect();
        let transactions = vec![vec![]; models.len()];
        let logs = vec![vec![]; models.len()];
        repo.upsert_blocks(&BlockData::new(&models, &transactions, &logs)).await
    }

    async fn stored_hashes(db: &DbFactory) -> Vec<Vec<u8>> {
        let client = db.pg().await.unwrap();
        let rows = client.query("select hash from blocks order by number", &[]).await.unwrap();
        rows.iter().map(|r| r.get("hash")).collect()
    }

    async fn parent_ref(db: &DbFactory, block: &Block) -> Option<Vec<u8>> {
        let client = db.pg().await.unwrap();
        let row = client.query_one("select parent_ref from blocks where hash = $1", &[&block.header.hash.to_vec()]).await.unwrap();
        row.get("parent_ref")
    }

    // the first block in an empty table has no stored parent, so parent_ref is NULL
    #[tokio::test]
    async fn test_first_block_gets_null_parent_ref() {
        let (repo, db, _lock) = setup().await;
        let blocks = chain(1, 1);

        insert_blocks(&repo, &blocks).await.unwrap();

        assert_eq!(parent_ref(&db, &blocks[0]).await, None);
    }

    // a block whose parent is stored gets parent_ref = parent_hash
    #[tokio::test]
    async fn test_block_with_stored_parent_gets_parent_ref() {
        let (repo, db, _lock) = setup().await;
        let blocks = chain(2, 1);

        insert_blocks(&repo, &blocks).await.unwrap();

        assert_eq!(parent_ref(&db, &blocks[1]).await, Some(blocks[0].header.hash.to_vec()));
    }

    // a block whose parent is not stored is rejected as a parent mismatch
    #[tokio::test]
    async fn test_block_with_unstored_parent_returns_parent_hash_mismatch() {
        let (repo, db, _lock) = setup().await;
        insert_blocks(&repo, &chain(1, 1)).await.unwrap();
        let orphan = mock_block_with_parent(2, B256::random());

        let result = insert_blocks(&repo, &[orphan.clone()]).await;

        assert!(matches!(&result, Err(CoreError::ParentHashMismatch(h)) if *h == orphan.header.hash.to_vec()));
        assert_eq!(stored_hashes(&db).await.len(), 1);
    }

    // re-inserting identical blocks succeeds and stores nothing new
    #[tokio::test]
    async fn test_duplicate_block_with_same_hash_is_skipped() {
        let (repo, db, _lock) = setup().await;
        let blocks = chain(2, 1);
        insert_blocks(&repo, &blocks).await.unwrap();

        let result = insert_blocks(&repo, &blocks).await;

        assert!(result.is_ok());
        assert_eq!(stored_hashes(&db).await.len(), 2);
    }

    // a different block at a stored height is reported with its parent, its own hash and the stored hash
    #[tokio::test]
    async fn test_duplicate_height_with_different_hash_returns_error() {
        let (repo, _db, _lock) = setup().await;
        let stored = chain(2, 1);
        insert_blocks(&repo, &stored).await.unwrap();
        let conflicting = mock_block_with_parent(2, stored[0].header.hash);

        let result = insert_blocks(&repo, &[conflicting.clone()]).await;

        let Err(CoreError::DbDuplicateBlockWithDifferentHash(parent, incoming, existing)) = result else {
            panic!("expected DbDuplicateBlockWithDifferentHash, got {:?}", result);
        };
        assert_eq!(parent, stored[0].header.hash.to_vec());
        assert_eq!(incoming, conflicting.header.hash.to_vec());
        assert_eq!(existing, stored[1].header.hash.to_vec());
    }

    // last_block_number is None for an empty table and the highest stored number otherwise
    #[tokio::test]
    async fn test_last_block_number() {
        let (repo, _db, _lock) = setup().await;
        assert_eq!(repo.last_block_number().await.unwrap(), None);

        insert_blocks(&repo, &chain(3, 10)).await.unwrap();

        assert_eq!(repo.last_block_number().await.unwrap(), Some(12));
    }

    // replace_chain_above keeps the ancestor and below, and swaps everything above it for the new chain
    #[tokio::test]
    async fn test_replace_chain_above_replaces_blocks_above_ancestor() {
        let (repo, db, _lock) = setup().await;
        let stored = chain(4, 1);
        insert_blocks(&repo, &stored).await.unwrap();
        let new_3 = mock_block_with_parent(3, stored[1].header.hash);
        let models = vec![BlockModel::new_from_rpc_block(&new_3).unwrap()];

        repo.replace_chain_above(2, &BlockData::new(&models, &vec![vec![]], &vec![vec![]])).await.unwrap();

        assert_eq!(stored_hashes(&db).await, vec![
            stored[0].header.hash.to_vec(),
            stored[1].header.hash.to_vec(),
            new_3.header.hash.to_vec(),
        ]);
    }

    // deleting a block also deletes its descendants through the parent_ref cascade
    #[tokio::test]
    async fn test_delete_block_cascading_also_deletes_descendants() {
        let (repo, db, _lock) = setup().await;
        let stored = chain(4, 1);
        insert_blocks(&repo, &stored).await.unwrap();

        repo.delete_block_cascading(stored[1].header.hash.to_vec(), stored[2].header.hash.to_vec()).await.unwrap();

        assert_eq!(stored_hashes(&db).await, vec![stored[0].header.hash.to_vec()]);
    }

    // marking a finalized checkpoint succeeds for a stored hash and reports false for an unknown one
    #[tokio::test]
    async fn test_mark_finalized_checkpoint_only_for_stored_hash() {
        let (repo, _db, _lock) = setup().await;
        let stored = chain(2, 1);
        insert_blocks(&repo, &stored).await.unwrap();

        assert!(repo.mark_finalized_checkpoint(stored[1].header.hash.to_vec()).await.unwrap());
        assert!(!repo.mark_finalized_checkpoint(B256::random().to_vec()).await.unwrap());
    }

    // blocks at or below a finalized checkpoint are finalized, blocks above it are not
    #[tokio::test]
    async fn test_is_block_finalized_at_or_below_checkpoint() {
        let (repo, _db, _lock) = setup().await;
        let stored = chain(3, 1);
        insert_blocks(&repo, &stored).await.unwrap();
        repo.mark_finalized_checkpoint(stored[1].header.hash.to_vec()).await.unwrap();

        assert!(repo.is_block_finalized(1).await.unwrap());
        assert!(repo.is_block_finalized(2).await.unwrap());
        assert!(!repo.is_block_finalized(3).await.unwrap());
    }

    // last_finalized_checkpoint is None until a checkpoint is flagged, then its block number
    #[tokio::test]
    async fn test_last_finalized_checkpoint() {
        let (repo, _db, _lock) = setup().await;
        let stored = chain(3, 1);
        insert_blocks(&repo, &stored).await.unwrap();
        assert_eq!(repo.last_finalized_checkpoint().await.unwrap(), None);

        repo.mark_finalized_checkpoint(stored[1].header.hash.to_vec()).await.unwrap();

        assert_eq!(repo.last_finalized_checkpoint().await.unwrap(), Some(2));
    }

    // blocks with transactions and logs

    type Parts = (BlockModel, Vec<TransactionModel>, Vec<LogModel>);

    /// the models of a block holding one transaction per entry of `logs_per_tx`, each with that many logs
    fn parts(number: u64, parent_hash: B256, logs_per_tx: &[usize]) -> (Parts, B256) {
        let (block, receipts) = mock_block_with_receipts(number, parent_hash, logs_per_tx);
        let hash = block.header.hash;
        let fetched = crate::indexer::fetcher::FetchedBlock { block, receipts };
        let models = crate::indexer::fetcher::FetcherInner::<PgBlockRepository, EthereumMockClient>::models_of_block(&fetched).unwrap();
        (models, hash)
    }

    fn block_data_of<'a>(parts: &'a [Parts], blocks: &'a mut Vec<BlockModel>, txs: &'a mut Vec<Vec<TransactionModel>>, logs: &'a mut Vec<Vec<LogModel>>) -> BlockData<'a> {
        for (b, t, l) in parts {
            blocks.push(b.clone());
            txs.push(t.clone());
            logs.push(l.clone());
        }
        BlockData::new(blocks, txs, logs)
    }

    async fn upsert_parts(repo: &PgBlockRepository, parts: &[Parts]) -> Result<(), CoreError> {
        let (mut blocks, mut txs, mut logs) = (vec![], vec![], vec![]);
        repo.upsert_blocks(&block_data_of(parts, &mut blocks, &mut txs, &mut logs)).await
    }

    async fn count(db: &DbFactory, table: &str) -> i64 {
        let client = db.pg().await.unwrap();
        client.query_one(&format!("select count(*) as count from {}", table), &[]).await.unwrap().get("count")
    }

    #[tokio::test]
    async fn test_logs_and_receipt_data_are_stored_with_the_block() {
        let (repo, db, _lock) = setup().await;
        let (block, _) = parts(1, B256::random(), &[2, 0, 3]);

        upsert_parts(&repo, &[block]).await.unwrap();

        assert_eq!(count(&db, "transactions").await, 3);
        assert_eq!(count(&db, "logs").await, 5);
        let client = db.pg().await.unwrap();
        let rows = client.query("select log_index, topic0, topic1, topic2, topic3 from logs order by log_index", &[]).await.unwrap();
        // log i has i topics, so unused topic slots are NULL
        for row in &rows {
            let log_index: i32 = row.get("log_index");
            let topics: Vec<Option<Vec<u8>>> = vec![row.get("topic0"), row.get("topic1"), row.get("topic2"), row.get("topic3")];
            assert_eq!(topics.iter().filter(|t| t.is_some()).count() as i32, log_index);
        }
        let row = client.query_one("select receipt_status, receipt_gas_used, receipt_effective_gas_price::text as price, receipt_contract_address, receipt_blob_gas_used from transactions limit 1", &[]).await.unwrap();
        assert_eq!(row.get::<_, Option<bool>>("receipt_status"), Some(true));
        assert_eq!(row.get::<_, Option<i64>>("receipt_gas_used"), Some(21_000));
        assert_eq!(row.get::<_, Option<String>>("price"), Some("1000000000".to_string()));
        assert_eq!(row.get::<_, Option<Vec<u8>>>("receipt_contract_address"), None);
        assert_eq!(row.get::<_, Option<i64>>("receipt_blob_gas_used"), None);
    }

    // a block that is already stored is skipped together with its transactions and logs
    #[tokio::test]
    async fn test_storing_the_same_block_again_does_not_duplicate_its_children() {
        let (repo, db, _lock) = setup().await;
        let (block, _) = parts(1, B256::random(), &[2, 1]);

        upsert_parts(&repo, &[block.clone()]).await.unwrap();
        upsert_parts(&repo, &[block]).await.unwrap();

        assert_eq!(count(&db, "blocks").await, 1);
        assert_eq!(count(&db, "transactions").await, 2);
        assert_eq!(count(&db, "logs").await, 3);
    }

    // the logs of a replaced block go with it: this is the foreign key cascade, with real rows
    #[tokio::test]
    async fn test_replace_chain_above_deletes_the_logs_of_replaced_blocks() {
        let (repo, db, _lock) = setup().await;
        let (b1, h1) = parts(1, B256::random(), &[1]);
        let (b2, h2) = parts(2, h1, &[2]);
        let (b3, _) = parts(3, h2, &[3]);
        upsert_parts(&repo, &[b1.clone(), b2.clone(), b3.clone()]).await.unwrap();
        assert_eq!(count(&db, "logs").await, 6);

        let (new_2, _) = parts(2, h1, &[4]);
        let (mut blocks, mut txs, mut logs) = (vec![], vec![], vec![]);
        repo.replace_chain_above(1, &block_data_of(&[new_2.clone()], &mut blocks, &mut txs, &mut logs)).await.unwrap();

        let client = db.pg().await.unwrap();
        let old_hashes = vec![b2.0.hash.clone(), b3.0.hash.clone()];
        let stale: i64 = client.query_one("select count(*) as count from logs where block_hash = any($1)", &[&old_hashes]).await.unwrap().get("count");
        assert_eq!(stale, 0);
        assert_eq!(count(&db, "logs").await, 1 + 4);
        assert_eq!(count(&db, "transactions").await, 1 + 1);
    }

    #[tokio::test]
    async fn test_delete_block_cascading_deletes_the_logs() {
        let (repo, db, _lock) = setup().await;
        let (b1, h1) = parts(1, B256::random(), &[1]);
        let (b2, _) = parts(2, h1, &[2]);
        upsert_parts(&repo, &[b1.clone(), b2.clone()]).await.unwrap();

        repo.delete_block_cascading(b2.0.hash.clone(), b2.0.hash.clone()).await.unwrap();

        assert_eq!(count(&db, "logs").await, 1);
    }

    // a log that belongs to another block makes the whole write fail, leaving nothing behind
    #[tokio::test]
    async fn test_log_of_another_block_is_rejected_and_nothing_is_stored() {
        let (repo, db, _lock) = setup().await;
        let (mut block, _) = parts(1, B256::random(), &[2]);
        block.2[1].block_hash = vec![9u8; 32];

        let res = upsert_parts(&repo, &[block]).await;

        assert!(matches!(res, Err(CoreError::BadArgument(_))));
        assert_eq!(count(&db, "blocks").await, 0);
        assert_eq!(count(&db, "logs").await, 0);
    }

    #[tokio::test]
    async fn test_mismatched_block_data_lists_are_rejected() {
        let (repo, _db, _lock) = setup().await;
        let (block, _) = parts(1, B256::random(), &[1]);
        let res = repo.upsert_blocks(&BlockData::new(&vec![block.0], &vec![block.1], &vec![])).await;
        assert!(matches!(res, Err(CoreError::BadArgument(_))));
    }

    #[tokio::test]
    async fn test_list_logs_filters_orders_and_pages() {
        let (repo, _db, _lock) = setup().await;
        let (b1, h1) = parts(1, B256::random(), &[2, 3]);
        let (b2, _) = parts(2, h1, &[1]);
        upsert_parts(&repo, &[b1.clone(), b2.clone()]).await.unwrap();
        let positions = |logs: &Vec<LogModel>| logs.iter().map(|l| (l.block_number, l.log_index)).collect::<Vec<_>>();
        let no_filter = LogFilter::default();

        // newest first, by block and then by log index
        let all = repo.list_logs(&no_filter, 100, 0).await.unwrap();
        assert_eq!(positions(&all), vec![(2, 0), (1, 4), (1, 3), (1, 2), (1, 1), (1, 0)]);
        // topics round-trip, with unused slots NULL
        assert!(all[0].topic0.is_none());
        assert_eq!(all[1].topic3.as_ref().map(|t| t.len()), Some(32));

        // paging
        assert_eq!(positions(&repo.list_logs(&no_filter, 2, 1).await.unwrap()), vec![(1, 4), (1, 3)]);

        // address
        let by_address = LogFilter { address: Some(vec![0xaa; 20]), ..Default::default() };
        assert_eq!(repo.list_logs(&by_address, 100, 0).await.unwrap().len(), 6);
        let other_address = LogFilter { address: Some(vec![0xbb; 20]), ..Default::default() };
        assert!(repo.list_logs(&other_address, 100, 0).await.unwrap().is_empty());

        // topics: block 1's log 3 has topic0..topic2
        let log3 = b1.2.iter().find(|l| l.log_index == 3).unwrap();
        let mut by_topics = LogFilter { topics: [log3.topic0.clone(), None, log3.topic2.clone(), None], ..Default::default() };
        assert_eq!(positions(&repo.list_logs(&by_topics, 100, 0).await.unwrap()), vec![(1, 3)]);
        by_topics.topics[2] = Some(vec![0u8; 32]);
        assert!(repo.list_logs(&by_topics, 100, 0).await.unwrap().is_empty());

        // block range, inclusive, combined with an address
        let by_range = LogFilter { address: Some(vec![0xaa; 20]), from_block: Some(2), to_block: Some(2), ..Default::default() };
        assert_eq!(positions(&repo.list_logs(&by_range, 100, 0).await.unwrap()), vec![(2, 0)]);
        let up_to_one = LogFilter { to_block: Some(1), ..Default::default() };
        assert_eq!(repo.list_logs(&up_to_one, 100, 0).await.unwrap().len(), 5);
    }
}

