use std::fmt::format;
use std::sync::Arc;
use alloy::primitives::U256;
use tokio_postgres::{Error, IsolationLevel};
use tokio_postgres::types::ToSql;
use crate::db::db::DbFactory;
use crate::db::pg::pg::Pg;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::transaction::TransactionModel;
use crate::indexer::repository::repository::BlockRepository;

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
    async fn upsert_blocks(&self, blocks: &Vec<BlockModel>, transactions: &Vec<Vec<TransactionModel>>) -> Result<(), CoreError> {
        if blocks.is_empty() {
            return Err(CoreError::Empty("blocks list cannot be empty".to_string()));
        } else if blocks.len() != transactions.len() {
            return Err(CoreError::BadArgument("number of transaction lists must equal the number of blocks".to_string()));
        }

        let query_str = "insert into blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, parent_beacon_block_root,
                    withdrawals_root, fee_recipient, requests_hash, extra_data, size, gas_limit, gas_used, base_fee_per_gas, blob_gas_used, excess_blob_gas,
                        prev_randao, timestamp) values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)";
        let tx_insert = "insert into transactions (hash, block_hash, block_number, transaction_index, tx_type, chain_id, nonce, from_address, to_address,
                    value, gas_limit, gas_price, max_fee_per_gas, max_priority_fee_per_gas, max_fee_per_blob_gas, input, access_list, blob_versioned_hashes,
                        v, r, s, y_parity) values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::text::numeric, $11, $12::text::numeric, $13::text::numeric,
                            $14::text::numeric, $15::text::numeric, $16, $17::text::jsonb, $18, $19::text::numeric, $20, $21, $22)";

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
        let stmt = tx.prepare(query_str).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
        let tx_stmt = tx.prepare(tx_insert).await
            .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;

        for (block, block_transactions) in blocks.iter().zip(transactions.iter()) {
            let existing = tx.query_opt("select hash from blocks where number = $1", &[&block.number]).await
                .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
            if let Some(row) = existing {
                let existing_hash: Vec<u8> = row.get("hash");
                if existing_hash == block.hash {
                    // this ensures exactly same block cannot exist; and helps us to have a safe
                    // and simpler retry
                    continue;
                }
                // a reorg has been detected: drop the obsolete block and everything after it.
                // child transactions are deleted too, since delete cascade is defined in transactions table
                // @todo deeper reorgs (parent_hash not matching stored block N-1) are not handled yet
                tx.execute("delete from blocks where number >= $1", &[&block.number]).await
                    .map_err(|e| CoreError::DbQueryFailed(pg_err_msg(&e)))?;
            }

            let values = block.get_prepared_values();
            tx.execute(&stmt, values.as_slice()).await
                .map_err(|e| CoreError::DbInsertFailed(pg_err_msg(&e)))?;

            for transaction in block_transactions {
                if transaction.block_hash != block.hash {
                    return Err(CoreError::BadArgument(format!(
                        "transaction {:?} does not belong to block {:?}", transaction.hash, block.hash)));
                }
                tx.execute(&tx_stmt, transaction.get_prepared_values().as_slice()).await
                    .map_err(|e| CoreError::DbTxExecFailed(pg_err_msg(&e)))?;
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
}