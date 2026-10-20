use alloy::rpc::types::Log;
use serde::{Serialize, Serializer};
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use crate::error::CoreError;

fn serialize_hex<S: Serializer>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{}", alloy::hex::encode(bytes)))
}

fn serialize_optional_hex<S: Serializer>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    match bytes {
        Some(bytes) => serialize_hex(bytes, serializer),
        None => serializer.serialize_none(),
    }
}

/// an event log emitted while executing a transaction; one row of the logs table.
/// Serialized (for the API) with its binary fields as 0x-prefixed hex strings
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogModel {
    #[serde(serialize_with = "serialize_hex")]
    pub block_hash: Vec<u8>,
    pub block_number: i64,
    #[serde(serialize_with = "serialize_hex")]
    pub tx_hash: Vec<u8>,
    pub tx_index: i32,
    /// position within the whole block, not within the transaction
    pub log_index: i32,
    #[serde(serialize_with = "serialize_hex")]
    pub address: Vec<u8>,
    #[serde(serialize_with = "serialize_optional_hex")]
    pub topic0: Option<Vec<u8>>,
    #[serde(serialize_with = "serialize_optional_hex")]
    pub topic1: Option<Vec<u8>>,
    #[serde(serialize_with = "serialize_optional_hex")]
    pub topic2: Option<Vec<u8>>,
    #[serde(serialize_with = "serialize_optional_hex")]
    pub topic3: Option<Vec<u8>>,
    /// the non-indexed data of the log; {"raw": "0x…"} for an event that has not been decoded
    pub data_json: serde_json::Value,
}

impl LogModel {
    pub fn new_from_row(row: &Row) -> LogModel {
        Self {
            block_hash: row.get("block_hash"),
            block_number: row.get("block_number"),
            tx_hash: row.get("tx_hash"),
            tx_index: row.get("tx_index"),
            log_index: row.get("log_index"),
            address: row.get("address"),
            topic0: row.get("topic0"),
            topic1: row.get("topic1"),
            topic2: row.get("topic2"),
            topic3: row.get("topic3"),
            data_json: row.get("data_json"),
        }
    }

    pub fn new_from_rpc_log(log: &Log) -> Result<LogModel, CoreError> {
        let missing = |field: &str| CoreError::ReceiptsInvalid(format!("log has no {field} (pending?)"));
        let block_hash = log.block_hash.ok_or(missing("block_hash"))?;
        let block_number = log.block_number.ok_or(missing("block_number"))?;
        let tx_hash = log.transaction_hash.ok_or(missing("transaction_hash"))?;
        let tx_index = log.transaction_index.ok_or(missing("transaction_index"))?;
        let log_index = log.log_index.ok_or(missing("log_index"))?;

        let topics = log.topics();
        if topics.len() > 4 {
            return Err(CoreError::ReceiptsInvalid(format!("log {log_index} has {} topics, at most 4 are allowed", topics.len())));
        }
        let topic = |i: usize| topics.get(i).map(|t| t.to_vec());

        Ok(Self {
            block_hash: block_hash.to_vec(),
            block_number: block_number as i64,
            tx_hash: tx_hash.to_vec(),
            tx_index: tx_index as i32,
            log_index: log_index as i32,
            address: log.address().to_vec(),
            topic0: topic(0),
            topic1: topic(1),
            topic2: topic(2),
            topic3: topic(3),
            data_json: serde_json::json!({ "raw": format!("0x{}", alloy::hex::encode(&log.data().data)) }),
        })
    }

    /// values in the same order as the columns of the logs insert query
    pub fn get_prepared_values(&self) -> Vec<&(dyn ToSql + Sync)> {
        vec![
            &self.block_hash, &self.block_number, &self.tx_hash, &self.tx_index, &self.log_index, &self.address,
            &self.topic0, &self.topic1, &self.topic2, &self.topic3, &self.data_json,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::B256;
    use crate::indexer::types::mocks::mock_block_with_receipts;

    fn logs_of_a_block() -> Vec<LogModel> {
        // 5 logs, indexes 0..=4, with 0, 1, 2, 3 and 4 topics
        let (_, receipts) = mock_block_with_receipts(9, B256::random(), &[5]);
        receipts[0].inner.logs().iter().map(|l| LogModel::new_from_rpc_log(l).unwrap()).collect()
    }

    #[test]
    fn test_unused_topic_slots_are_none() {
        let logs = logs_of_a_block();
        let used = |l: &LogModel| [&l.topic0, &l.topic1, &l.topic2, &l.topic3].iter().filter(|t| t.is_some()).count();
        assert_eq!(logs.iter().map(used).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
        // topics fill the slots in order: a log with 2 topics has topic0 and topic1
        assert!(logs[2].topic0.is_some() && logs[2].topic1.is_some() && logs[2].topic2.is_none());
    }

    #[test]
    fn test_log_fields_are_copied() {
        let log = &logs_of_a_block()[3];
        assert_eq!(log.block_number, 9);
        assert_eq!(log.log_index, 3);
        assert_eq!(log.tx_index, 0);
        assert_eq!(log.address, vec![0xaa; 20]);
        assert_eq!(log.data_json, serde_json::json!({ "raw": "0x00ff" }));
        assert_eq!(log.topic0.as_ref().unwrap().len(), 32);
    }

    #[test]
    fn test_pending_log_is_rejected() {
        let (_, receipts) = mock_block_with_receipts(9, B256::random(), &[1]);
        let mut log = receipts[0].inner.logs()[0].clone();
        log.log_index = None;
        assert!(matches!(LogModel::new_from_rpc_log(&log), Err(CoreError::ReceiptsInvalid(_))));
    }

    #[test]
    fn test_prepared_values_match_the_insert_query() {
        let log = &logs_of_a_block()[0];
        assert_eq!(log.get_prepared_values().len(), 11);
        assert_eq!(crate::indexer::repository::queries::LOG_INSERT_QUERY.matches('$').count(), 11);
    }
}
