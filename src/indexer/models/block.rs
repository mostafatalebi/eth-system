use alloy::consensus::BlockHeader;
use alloy::rpc::types::Block;
use chrono::NaiveDateTime;
use num_bigint::BigUint;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use crate::error::CoreError;
use crate::indexer::types::SqlNumeric;

#[derive(Debug, Clone)]
pub struct BlockModel {
    pub number: i64,
    pub hash: Vec<u8>,
    pub parent_hash: Vec<u8>,
    pub nonce: Vec<u8>,
    pub sha3_uncles: Vec<u8>,
    pub logs_bloom: Vec<u8>,
    pub transactions_root: Vec<u8>,
    pub state_root: Vec<u8>,
    pub receipts_root: Vec<u8>,
    pub parent_beacon_block_root: Option<Vec<u8>>,
    pub withdrawals_root: Option<Vec<u8>>,
    pub fee_recipient: Vec<u8>,
    pub requests_hash: Option<Vec<u8>>,
    pub extra_data: Vec<u8>,
    pub size: i64,
    pub gas_limit: i64,
    pub gas_used: i64,
    pub base_fee_per_gas: Option<i64>,
    pub blob_gas_used: Option<i64>,
    pub excess_blob_gas: Option<i64>,
    pub prev_randao: Vec<u8>,
    pub timestamp: i64,
    pub create_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}


impl BlockModel {

    fn new_from_row(row: &Row) -> BlockModel {
        return Self {
            number: row.get("number"),
            hash: row.get("hash"),
            parent_hash: row.get("parent_hash"),

            nonce: row.get("nonce"),
            sha3_uncles: row.get("ommers_hash"),
            logs_bloom: row.get("logs_bloom"),
            transactions_root: row.get("transactions_root"),
            state_root: row.get("state_root"),
            receipts_root: row.get("receipts_root"),

            parent_beacon_block_root: row.get("parent_beacon_block_root"),
            withdrawals_root: row.get("withdrawals_root"),
            fee_recipient: row.get("fee_recipient"),
            requests_hash: row.get("requests_hash"),


            extra_data: row.get("extra_data"),

            size: row.get("size"),
            gas_limit: row.get("gas_limit"),
            gas_used: row.get("gas_used"),
            base_fee_per_gas: row.get("base_fee_per_gas"),

            blob_gas_used: row.get("blob_gas_used"),
            excess_blob_gas: row.get("excess_blob_gas"),

            prev_randao: row.get("prev_randao"),
            timestamp: row.get("timestamp"),

            create_at: row.get("create_at"),
            updated_at: row.get("updated_at"),
        }
    }


    pub fn new_from_rpc_block(block: &Block) -> Result<BlockModel, CoreError> {
        Ok(Self {
            number: block.header.number as i64,
            hash: block.header.hash.to_vec(),
            parent_hash: block.header.parent_hash.to_vec(),
            nonce: block.header.nonce.to_vec(),
            sha3_uncles: block.header.ommers_hash.to_vec(),
            logs_bloom: block.header.logs_bloom.to_vec(),
            transactions_root: block.header.transactions_root.to_vec(),
            state_root: block.header.state_root.to_vec(),
            receipts_root: block.header.receipts_root.to_vec(),
            parent_beacon_block_root: Some(block.header.parent_beacon_block_root.unwrap_or_default().to_vec()),
            withdrawals_root: Some(block.header.withdrawals_root.unwrap_or_default().to_vec()),
            fee_recipient: block.header.beneficiary().to_vec(),
            requests_hash: Some(block.header.requests_hash.unwrap_or_default().to_vec()),
            extra_data: block.header.extra_data.to_vec(),
            size: block.header.size.unwrap_or_default().to::<i64>(),
            gas_limit: block.header.gas_limit as i64,
            gas_used: block.header.gas_used as i64,
            base_fee_per_gas: Some(block.header.base_fee_per_gas.unwrap_or_default() as i64),
            blob_gas_used: Some(block.header.blob_gas_used.unwrap_or_default() as i64),
            excess_blob_gas: Some(block.header.excess_blob_gas.unwrap_or_default() as i64),
            prev_randao: block.header.mix_hash.to_vec(),
            timestamp: block.header.timestamp as i64,
            create_at: Default::default(),
            updated_at: Default::default(),
        })
    }

    pub fn get_prepared_values(&self) -> Vec<&(dyn ToSql + Sync)> {
        let mut values: Vec<&(dyn ToSql+Sync)> = vec![];
        values.push(&self.number);
        values.push(&self.hash);
        values.push(&self.parent_hash);
        values.push(&self.nonce);
        values.push(&self.sha3_uncles);
        values.push(&self.logs_bloom);
        values.push(&self.transactions_root);
        values.push(&self.state_root);
        values.push(&self.receipts_root);
        values.push(&self.parent_beacon_block_root);
        values.push(&self.withdrawals_root);
        values.push(&self.fee_recipient);
        values.push(&self.requests_hash);
        values.push(&self.extra_data);
        values.push(&self.size);
        values.push(&self.gas_limit);
        values.push(&self.gas_used);
        values.push(&self.base_fee_per_gas);
        values.push(&self.blob_gas_used);
        values.push(&self.excess_blob_gas);
        values.push(&self.prev_randao);
        values.push(&self.timestamp);
        values
    }
}