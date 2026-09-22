use alloy::consensus::Header as ConsensusHeader;
use alloy::primitives::{Address, Bloom, Bytes, B256, B64, U256};
use alloy::rpc::types::{Block, BlockTransactions, Header};


pub fn mock_block_with_parent(number: u64, parent_hash: B256) -> Block {
    let consensus_header = ConsensusHeader {
        parent_hash,
        ommers_hash: B256::random(),
        beneficiary: Address::random(),
        state_root: B256::random(),
        transactions_root: B256::random(),
        receipts_root: B256::random(),
        logs_bloom: Bloom::default(),
        difficulty: U256::ZERO,
        number,
        gas_limit: 30_000_000,
        gas_used: 21_000,
        timestamp: 1_700_000_000 + number,
        extra_data: Bytes::default(),
        mix_hash: B256::random(),
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        withdrawals_root: Some(B256::random()),
        blob_gas_used: Some(0),
        excess_blob_gas: Some(0),
        parent_beacon_block_root: Some(B256::random()),
        requests_hash: None,
        block_access_list_hash: None,
        slot_number: None,
    };

    let header = Header {
        hash: B256::random(),
        inner: consensus_header,
        total_difficulty: Some(U256::ZERO),
        size: Some(U256::from(1000)),
    };

    Block {
        header,
        uncles: vec![],
        transactions: BlockTransactions::Hashes(vec![]),
        withdrawals: None,
    }
}

pub fn mock_blocks_list(count: usize, start_number: u64) -> Vec<Option<Block>> {
    let mut blocks = Vec::with_capacity(count);
    let mut parent_hash = B256::random();

    for i in 0..count {
        let number = start_number + i as u64;
        let block = mock_block_with_parent(number, parent_hash);
        parent_hash = block.header.hash;
        blocks.push(Some(block));
    }

    blocks
}