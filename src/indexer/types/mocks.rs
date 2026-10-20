use alloy::consensus::Header as ConsensusHeader;
use alloy::primitives::{Address, Bloom, Bytes, B256, B64, U256};
use alloy::rpc::types::{Block, BlockTransactions, Header, Transaction, TransactionReceipt};


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

fn hex_u64(n: u64) -> String {
    format!("0x{:x}", n)
}

fn mock_transaction(hash: B256, index: u64, block_hash: B256, block_number: u64) -> Transaction {
    serde_json::from_value(serde_json::json!({
        "type": "0x2", "hash": hash, "nonce": hex_u64(index), "blockHash": block_hash, "blockNumber": hex_u64(block_number),
        "transactionIndex": hex_u64(index), "from": Address::repeat_byte(0x11), "to": Address::repeat_byte(0x22),
        "value": "0x1", "gas": "0x5208", "maxFeePerGas": "0x3b9aca00", "maxPriorityFeePerGas": "0x3b9aca00",
        "gasPrice": "0x3b9aca00", "input": "0x", "chainId": "0x1", "accessList": [],
        "v": "0x0", "r": "0x1", "s": "0x1", "yParity": "0x0"
    })).unwrap()
}

fn mock_log(address: Address, topics: Vec<B256>, data: &str, tx_hash: B256, tx_index: u64, log_index: u64, block_hash: B256, block_number: u64) -> serde_json::Value {
    serde_json::json!({
        "address": address, "topics": topics, "data": data, "blockHash": block_hash, "blockNumber": hex_u64(block_number),
        "transactionHash": tx_hash, "transactionIndex": hex_u64(tx_index), "logIndex": hex_u64(log_index), "removed": false
    })
}

/// a block with one transaction per entry of `logs_per_tx`, plus the receipts of those transactions. The receipt
/// of transaction i holds `logs_per_tx[i]` logs; log indexes run across the whole block, and the number of topics
/// of a log cycles through 0..=4 by its index so that every topic slot gets exercised
pub fn mock_block_with_receipts(number: u64, parent_hash: B256, logs_per_tx: &[usize]) -> (Block, Vec<TransactionReceipt>) {
    let mut block = mock_block_with_parent(number, parent_hash);
    let block_hash = block.header.hash;
    let mut transactions = Vec::new();
    let mut receipts: Vec<TransactionReceipt> = Vec::new();
    let mut log_index = 0u64;
    for (i, log_count) in logs_per_tx.iter().enumerate() {
        let (i, tx_hash) = (i as u64, B256::random());
        transactions.push(mock_transaction(tx_hash, i, block_hash, number));
        let mut logs = Vec::new();
        for _ in 0..*log_count {
            let topics = (0..(log_index % 5)).map(|_| B256::random()).collect();
            logs.push(mock_log(Address::repeat_byte(0xaa), topics, "0x00ff", tx_hash, i, log_index, block_hash, number));
            log_index += 1;
        }
        receipts.push(serde_json::from_value(serde_json::json!({
            "type": "0x2", "status": "0x1", "cumulativeGasUsed": hex_u64(21_000 * (i + 1)), "logs": logs,
            "logsBloom": format!("0x{}", "00".repeat(256)), "transactionHash": tx_hash, "transactionIndex": hex_u64(i),
            "blockHash": block_hash, "blockNumber": hex_u64(number), "gasUsed": "0x5208", "effectiveGasPrice": "0x3b9aca00",
            "from": Address::repeat_byte(0x11), "to": Address::repeat_byte(0x22), "contractAddress": null
        })).unwrap());
    }
    // blooms as a node reports them: per receipt from its logs, and in the header from all of them
    let mut block_bloom = Bloom::default();
    for receipt in receipts.iter_mut() {
        let mut bloom = Bloom::default();
        for log in receipt.inner.logs() {
            bloom.accrue_log(&log.inner);
        }
        receipt.inner.as_receipt_with_bloom_mut().unwrap().logs_bloom = bloom;
        block_bloom.accrue_bloom(&bloom);
    }
    block.header.inner.logs_bloom = block_bloom;
    block.transactions = BlockTransactions::Full(transactions);
    (block, receipts)
}
