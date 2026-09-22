use std::sync::Arc;
use tokio_postgres::{Error, IsolationLevel};
use tokio_postgres::types::ToSql;
use crate::db::db::DbFactory;
use crate::db::pg::pg::Pg;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::repository::repository::BlockRepository;
use crate::indexer::types::SqlNumeric;

pub struct PgBlockRepository {
    db: Arc<DbFactory>
}

impl PgBlockRepository {
    pub fn new(db: Arc<DbFactory>) -> Self {
        Self { db }
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
    async fn upsert_blocks(&self, blocks: &Vec<BlockModel>) -> Result<(), CoreError> {
        if blocks.is_empty() {
            return Err(CoreError::Empty("blocks list cannot be empty".to_string()));
        }

        let query_str = "insert into blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, parent_beacon_block_root,
                    withdrawals_root, fee_recipient, requests_hash, extra_data, size, gas_limit, gas_used, base_fee_per_gas, blob_gas_used, excess_blob_gas,
                        prev_randao, timestamp) values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)";
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
        let stmt = tx.prepare(query_str).await;
        if stmt.is_err() {
            return Err(CoreError::DbQueryFailed(stmt.err().unwrap().to_string()));
        }
        let stmt = stmt.unwrap();

        let mut i = 0usize;
        while i < blocks.len() {
            let block = &blocks[i];
            let values = block.get_prepared_values();
            let exec_res = tx.execute(&stmt, values.as_slice()).await;
            if exec_res.is_err() {
                match exec_res.as_ref() {
                    Err(e) => {
                        if e.code() == Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) {
                            // a reorg has been detected
                            let res_del = tx.execute("delete from blocks where number >= (select number from blocks where parent_hash = $1)", &[&block.parent_hash]).await;
                            // @todo other entities such as transaction, contracts etc. must also be deleted accordingly

                            if res_del.is_err() {
                                return Err(CoreError::DbQueryFailed(exec_res.err().unwrap().to_string()));
                            }
                        }
                    }
                    _ => {
                        return Err(CoreError::DbQueryFailed(exec_res.err().unwrap().to_string()));
                    }
                }
                return Err(CoreError::DbInsertFailed(exec_res.err().unwrap().to_string()));
            } else {
                i += 1;
            }
        }

        let res = tx.commit().await;
        if res.is_err() {
            return Err(CoreError::DbTxExecFailed(res.err().unwrap().to_string()));
        }

        return Ok(());
    }

    async fn delete_block(&self) -> Result<(), CoreError> {
        Ok(())
    }

    async fn last_block_number(&self) -> Result<u64, CoreError> {
        let pg = self.db.pg().await;
        if pg.is_err() {
            return Err(pg.err().unwrap());
        }
        let mut pg = pg?;

        let res = pg.query("select number from blocks order by created_at limit 1", &[]).await;

        if res.is_err() {
            return Err(CoreError::Internal(Box::new(res.err().unwrap())))
        }
        let res = res.unwrap();

        if res.len() != 1 {
            return Err(CoreError::Empty("no block number found".to_string()))
        }

        if res.len() > 0 {
            let bn = SqlNumeric::new_big_uint(res[0].get("number")).big_uint();

            if bn.is_some() {
                return Ok(u64::try_from(bn.unwrap()).unwrap())
            }
        }

        return Err(CoreError::Empty("no block number found".to_string()))
    }
}