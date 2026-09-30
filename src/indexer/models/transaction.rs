use alloy::consensus::{Transaction as ConsensusTransaction, TxEnvelope};
use alloy::eips::Typed2718;
use alloy::network::TransactionResponse;
use alloy::rpc::types::Transaction;
use chrono::NaiveDateTime;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use crate::error::CoreError;

#[derive(Debug, Clone)]
pub struct TransactionModel {
    pub hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub block_number: i64,
    pub transaction_index: i32,
    pub tx_type: i16,
    pub chain_id: Option<i64>,
    pub nonce: i64,
    pub from_address: Vec<u8>,
    pub to_address: Option<Vec<u8>>,
    pub value: String,
    pub gas_limit: i64,
    pub gas_price: Option<String>,
    pub max_fee_per_gas: Option<String>,
    pub max_priority_fee_per_gas: Option<String>,
    pub max_fee_per_blob_gas: Option<String>,
    pub input: Vec<u8>,
    pub access_list: Option<String>,
    pub blob_versioned_hashes: Option<Vec<Vec<u8>>>,
    pub v: Option<String>,
    pub r: Vec<u8>,
    pub s: Vec<u8>,
    pub y_parity: Option<bool>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}


impl TransactionModel {

    pub fn new_from_row(row: &Row) -> TransactionModel {
        return Self {
            hash: row.get("hash"),
            block_hash: row.get("block_hash"),
            block_number: row.get("block_number"),
            transaction_index: row.get("transaction_index"),
            tx_type: row.get("tx_type"),
            chain_id: row.get("chain_id"),
            nonce: row.get("nonce"),
            from_address: row.get("from_address"),
            to_address: row.get("to_address"),
            value: row.get("value"),
            gas_limit: row.get("gas_limit"),
            gas_price: row.get("gas_price"),
            max_fee_per_gas: row.get("max_fee_per_gas"),
            max_priority_fee_per_gas: row.get("max_priority_fee_per_gas"),
            max_fee_per_blob_gas: row.get("max_fee_per_blob_gas"),
            input: row.get("input"),
            access_list: row.get("access_list"),
            blob_versioned_hashes: row.get("blob_versioned_hashes"),
            v: row.get("v"),
            r: row.get("r"),
            s: row.get("s"),
            y_parity: row.get("y_parity"),

            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }


    pub fn new_from_rpc_transaction(tx: &Transaction) -> Result<TransactionModel, CoreError> {
        let block_hash = tx.block_hash
            .ok_or(CoreError::TxTypeCastFailed("transaction has no block_hash (pending?)".to_string()))?;
        let block_number = tx.block_number
            .ok_or(CoreError::TxTypeCastFailed("transaction has no block_number (pending?)".to_string()))?;
        let transaction_index = tx.transaction_index
            .ok_or(CoreError::TxTypeCastFailed("transaction has no transaction_index (pending?)".to_string()))?;

        let env: TxEnvelope = tx.clone().try_into()
            .map_err(|e| CoreError::TxTypeCastFailed(format!("{:?}", e)))?;
        let sig = env.signature();
        let chain_id = ConsensusTransaction::chain_id(tx);

        // legacy: EIP-155 v (or 27/28 pre-EIP-155); typed txs: v == y_parity
        let parity = sig.v() as u64;
        let v = if env.is_legacy() {
            match chain_id {
                Some(c) => (c as u128) * 2 + 35 + parity as u128,
                None => 27 + parity as u128,
            }
        } else {
            parity as u128
        };

        let access_list = match ConsensusTransaction::access_list(tx) {
            Some(al) => Some(serde_json::to_string(al)
                .map_err(|e| CoreError::TxTypeCastFailed(e.to_string()))?),
            None => None,
        };

        Ok(Self {
            hash: TransactionResponse::tx_hash(tx).to_vec(),
            block_hash: block_hash.to_vec(),
            block_number: block_number as i64,
            transaction_index: transaction_index as i32,
            tx_type: env.ty() as i16,
            chain_id: chain_id.map(|c| c as i64),
            nonce: ConsensusTransaction::nonce(tx) as i64,
            from_address: TransactionResponse::from(tx).to_vec(),
            to_address: ConsensusTransaction::to(tx).map(|a| a.to_vec()),
            value: ConsensusTransaction::value(tx).to_string(),
            gas_limit: ConsensusTransaction::gas_limit(tx) as i64,
            gas_price: ConsensusTransaction::gas_price(tx).map(|g| g.to_string()),
            max_fee_per_gas: if ConsensusTransaction::is_dynamic_fee(tx) {
                Some(ConsensusTransaction::max_fee_per_gas(tx).to_string())
            } else {
                None
            },
            max_priority_fee_per_gas: ConsensusTransaction::max_priority_fee_per_gas(tx).map(|g| g.to_string()),
            max_fee_per_blob_gas: ConsensusTransaction::max_fee_per_blob_gas(tx).map(|g| g.to_string()),
            input: ConsensusTransaction::input(tx).to_vec(),
            access_list,
            blob_versioned_hashes: ConsensusTransaction::blob_versioned_hashes(tx)
                .map(|hs| hs.iter().map(|h| h.to_vec()).collect()),
            v: Some(v.to_string()),
            r: sig.r().to_be_bytes::<32>().to_vec(),
            s: sig.s().to_be_bytes::<32>().to_vec(),
            y_parity: Some(sig.v()),
            created_at: Default::default(),
            updated_at: Default::default(),
        })
    }

    /// values in the same order as the columns of the transactions insert query
    pub fn get_prepared_values(&self) -> Vec<&(dyn ToSql + Sync)> {
        let mut values: Vec<&(dyn ToSql+Sync)> = vec![];
        values.push(&self.hash);
        values.push(&self.block_hash);
        values.push(&self.block_number);
        values.push(&self.transaction_index);
        values.push(&self.tx_type);
        values.push(&self.chain_id);
        values.push(&self.nonce);
        values.push(&self.from_address);
        values.push(&self.to_address);
        values.push(&self.value);
        values.push(&self.gas_limit);
        values.push(&self.gas_price);
        values.push(&self.max_fee_per_gas);
        values.push(&self.max_priority_fee_per_gas);
        values.push(&self.max_fee_per_blob_gas);
        values.push(&self.input);
        values.push(&self.access_list);
        values.push(&self.blob_versioned_hashes);
        values.push(&self.v);
        values.push(&self.r);
        values.push(&self.s);
        values.push(&self.y_parity);
        values
    }
}
