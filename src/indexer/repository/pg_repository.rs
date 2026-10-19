use std::sync::Arc;
use alloy::primitives::U256;
use tokio_postgres::{IsolationLevel, Statement, Transaction};
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;
use crate::db::db::DbFactory;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::transaction::TransactionModel;
use crate::indexer::repository::queries::{BLOCK_IS_FINALIZED_QUERY, BLOCK_MARK_FINALIZED_CHECKPOINT_QUERY, LAST_FINALIZED_CHECKPOINT_QUERY, BLOCK_INSERT_QUERY, TX_INSERT_QUERY};
use crate::indexer::repository::repository::{BlockData, BlockRepository};

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
        for (block, block_transactions) in block_data.blocks.iter().zip(block_data.transactions.iter()) {
            let mut res = self.upsert_single_block(&bck_stmt, &tx, block).await;
            if res.is_err() {
                tx.rollback().await.map_err(|e| CoreError::DbTxRollbackFailed(pg_err_msg(&e)))?;
                return Err(res.unwrap_err());
            } else {
                res = self.insert_transactions_of_a_block(&tx_stmt, &tx, &block, block_transactions).await;
                if res.is_err() {
                    tx.rollback().await.map_err(|e| CoreError::DbTxRollbackFailed(pg_err_msg(&e)))?;
                    return Err(res.unwrap_err());
                }
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
        let mut where_clause = "";
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

        for (i, block) in new_chain.blocks.iter().enumerate() {
            self.upsert_single_block(&stmt, &tx, block).await?;
            self.insert_transactions_of_a_block(&tx_stmt, &tx, block, &new_chain.transactions[i]).await?;
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

    async fn upsert_single_block(&self, stmt: &Statement, pg_tx: &Transaction<'_>, block: &BlockModel) -> Result<(), CoreError> {
            let existing = pg_tx.query_opt("select hash from blocks where number = $1", &[&block.number]).await
                .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
            if let Some(row) = existing {
                let existing_hash: Vec<u8> = row.get("hash");
                if existing_hash == block.hash {
                    // this ensures exactly same block cannot exist; and helps us to have a safe
                    // and simpler retry
                    return Ok(());
                }
                return Err(CoreError::DbDuplicateBlockWithDifferentHash(block.parent_hash.clone(), block.hash.clone(), existing_hash));
            }

            let values = block.get_prepared_values();
            pg_tx.execute(stmt, values.as_slice()).await
                .map_err(|e| match e.code() {
                    Some(&SqlState::FOREIGN_KEY_VIOLATION) => CoreError::ParentHashMismatch(block.hash.clone()),
                    _ => CoreError::DbInsertFailed(pg_err_msg(&e)),
                })?;
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
    use crate::indexer::types::mocks::{mock_block_with_parent, mock_blocks_list};

    static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn setup() -> (PgBlockRepository, Arc<DbFactory>, tokio::sync::MutexGuard<'static, ()>) {
        let guard = DB_LOCK.lock().await;
        let config_file = std::env::var("CONFIG_FILE").expect("CONFIG_FILE must point to a config with test DB credentials");
        let config = Config::load_from_file(config_file).unwrap();
        let db = Arc::new(DbFactory::connect(&config.db_pg_url, Postgres).await.unwrap());
        let mut client = db.pg().await.unwrap();
        crate::migrations::migration_runner::migrations::runner()
            .run_async(std::ops::DerefMut::deref_mut(std::ops::DerefMut::deref_mut(&mut client))).await.unwrap();
        client.batch_execute("truncate blocks, transactions").await.unwrap();
        (PgBlockRepository::new(db.clone()), db, guard)
    }

    fn chain(count: usize, start: u64) -> Vec<Block> {
        mock_blocks_list(count, start).into_iter().flatten().collect()
    }

    async fn insert_blocks(repo: &PgBlockRepository, blocks: &[Block]) -> Result<(), CoreError> {
        let models: Vec<BlockModel> = blocks.iter().map(|b| BlockModel::new_from_rpc_block(b).unwrap()).collect();
        let transactions = vec![vec![]; models.len()];
        repo.upsert_blocks(&BlockData::new(&models, &transactions)).await
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

        repo.replace_chain_above(2, &BlockData::new(&models, &vec![vec![]])).await.unwrap();

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
}
