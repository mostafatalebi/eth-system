use alloy::primitives::{Bloom, B256};
use alloy::rpc::types::{Block, TransactionReceipt};
use crate::error::CoreError;

pub struct BlockValidator {
}


impl BlockValidator {

    /// validates an incoming block against the stream so far. `expected_block_number` is
    /// the number following the previously received block; None for the first block,
    /// since there is nothing to compare against yet.
    ///
    /// note: header.hash is a B256, so its size is guaranteed by the type and not checked here
    pub fn validate_rpc_block(block: &Block, expected_block_number: Option<u64>) -> Result<(), CoreError> {
        if let Some(expected) = expected_block_number {
            if expected != block.header.number {
                return Err(CoreError::BlockNumberUnexpected(expected, block.header.number))
            }
        }
        return Ok(())
    }

    /// checks that the receipts fetched for a block are exactly the receipts of its transactions:
    /// one per transaction, in the same order, belonging to this block, with the log indexes
    /// running 0, 1, 2... across the whole block, and with logs that rebuild the bloom filter of
    /// the block header (so no log has been dropped, added or altered)
    pub fn validate_receipts(block: &Block, receipts: &[TransactionReceipt]) -> Result<(), CoreError> {
        let tx_hashes: Vec<B256> = block.transactions.hashes().collect();
        if receipts.len() != tx_hashes.len() {
            return Err(CoreError::ReceiptsInvalid(format!(
                "block {} has {} transactions but {} receipts", block.header.number, tx_hashes.len(), receipts.len())));
        }

        let mut expected_log_index = 0u64;
        let mut bloom = Bloom::default();
        for (i, (receipt, tx_hash)) in receipts.iter().zip(tx_hashes.iter()).enumerate() {
            if &receipt.transaction_hash != tx_hash {
                return Err(CoreError::ReceiptsInvalid(format!(
                    "block {}: receipt {} is for transaction {:?}, expected {:?}", block.header.number, i, receipt.transaction_hash, tx_hash)));
            }
            if receipt.block_hash.is_some_and(|h| h != block.header.hash) {
                return Err(CoreError::ReceiptsInvalid(format!(
                    "block {}: receipt {} belongs to another block", block.header.number, i)));
            }
            for log in receipt.inner.logs() {
                if log.log_index != Some(expected_log_index) {
                    return Err(CoreError::ReceiptsInvalid(format!(
                        "block {}: expected log index {} but got {:?}", block.header.number, expected_log_index, log.log_index)));
                }
                expected_log_index += 1;
                bloom.accrue_log(&log.inner);
            }
        }
        if bloom != block.header.logs_bloom {
            return Err(CoreError::ReceiptsInvalid(format!(
                "block {}: the logs do not rebuild the logs bloom of the block header", block.header.number)));
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::types::mocks::mock_block_with_receipts;

    fn is_invalid(res: Result<(), CoreError>) -> bool {
        matches!(res, Err(CoreError::ReceiptsInvalid(_)))
    }

    #[test]
    fn test_matching_receipts_are_valid() {
        let (block, receipts) = mock_block_with_receipts(1, B256::random(), &[2, 0, 3]);
        assert!(BlockValidator::validate_receipts(&block, &receipts).is_ok());
    }

    #[test]
    fn test_block_without_transactions_needs_no_receipts() {
        let (block, receipts) = mock_block_with_receipts(1, B256::random(), &[]);
        assert!(receipts.is_empty());
        assert!(BlockValidator::validate_receipts(&block, &receipts).is_ok());
    }

    #[test]
    fn test_missing_or_extra_receipts_are_invalid() {
        let (block, mut receipts) = mock_block_with_receipts(1, B256::random(), &[1, 1]);
        let extra = receipts[0].clone();
        receipts.pop();
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
        receipts.push(extra.clone());
        receipts.push(extra);
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
    }

    #[test]
    fn test_receipts_in_the_wrong_order_are_invalid() {
        let (block, mut receipts) = mock_block_with_receipts(1, B256::random(), &[1, 1]);
        receipts.swap(0, 1);
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
    }

    #[test]
    fn test_receipt_of_another_block_is_invalid() {
        let (block, mut receipts) = mock_block_with_receipts(1, B256::random(), &[1]);
        receipts[0].block_hash = Some(B256::random());
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
    }

    #[test]
    fn test_logs_must_rebuild_the_bloom_of_the_block_header() {
        let (mut block, receipts) = mock_block_with_receipts(1, B256::random(), &[3]);
        block.header.inner.logs_bloom = Bloom::default();
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
    }

    #[test]
    fn test_log_indexes_must_run_without_gaps_across_the_block() {
        let (block, mut receipts) = mock_block_with_receipts(1, B256::random(), &[2, 2]);
        // dropping a log leaves a gap in the numbering
        receipts[0].inner.as_receipt_with_bloom_mut().unwrap().receipt.logs.remove(0);
        assert!(is_invalid(BlockValidator::validate_receipts(&block, &receipts)));
    }
}
