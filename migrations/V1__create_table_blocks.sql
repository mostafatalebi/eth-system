create table blocks (
    hash bytea NOT NULL check ( octet_length(hash) = 32 ) primary key ,
    number bigint NOT NULL,
    parent_hash bytea NOT NULL check ( octet_length(parent_hash) = 32 ),
    nonce bytea DEFAULT NULL check ( octet_length(nonce) = 8 ),
    sha3_uncles bytea DEFAULT NULL check ( octet_length(sha3_uncles) = 32 ),
    logs_bloom bytea NOT NULL check ( octet_length(logs_bloom) = 256 ),
    transactions_root bytea NOT NULL check ( octet_length(transactions_root) = 32 ),
    state_root bytea NOT NULL check ( octet_length(state_root) = 32 ),
    receipts_root bytea NOT NULL check ( octet_length(receipts_root) = 32 ),
    parent_beacon_block_root bytea DEFAULT  NULL check ( octet_length(parent_beacon_block_root) = 32 ),
    withdrawals_root bytea DEFAULT NULL check ( octet_length(withdrawals_root) = 32 ),
    fee_recipient bytea DEFAULT NULL check ( octet_length(fee_recipient) = 20 ),
    requests_hash bytea DEFAULT NULL check ( octet_length(requests_hash) = 32 ),
    extra_data bytea NOT NULL,
    size bigint NOT NULL,
    gas_limit bigint NOT NULL,
    gas_used bigint NOT NULL ,
    base_fee_per_gas bigint NOT NULL ,
    blob_gas_used bigint DEFAULT NULL ,
    excess_blob_gas bigint DEFAULT NULL ,
    prev_randao bytea NOT NULL check ( octet_length(prev_randao) = 32 ),
    timestamp bigint NOT NULL ,
    created_at timestamp NOT NULL DEFAULT NOW(),
    updated_at timestamp NOT NULL DEFAULT NOW()
);

create table logs (
    block_hash bytea NOT NULL check ( octet_length(block_hash) = 32 ) references blocks(hash) on delete cascade,
    block_number bigint NOT NULL,
    tx_hash bytea NOT NULL check ( octet_length(tx_hash) = 32 ),
    tx_index integer NOT NULL,
    -- position within the whole block, not within the transaction
    log_index integer NOT NULL,
    address bytea NOT NULL check ( octet_length(address) = 20 ),
    -- topic0 is the event signature hash; it is NULL only when the log has no topics at all (LOG0).
    -- topic1..topic3 are the indexed parameters in declaration order; unused slots are NULL
    topic0 bytea DEFAULT NULL check ( octet_length(topic0) = 32 ),
    topic1 bytea DEFAULT NULL check ( octet_length(topic1) = 32 ),
    topic2 bytea DEFAULT NULL check ( octet_length(topic2) = 32 ),
    topic3 bytea DEFAULT NULL check ( octet_length(topic3) = 32 ),
    -- the payload of the log (its non-indexed data), as JSON: {"raw": "0x…"} for an event that has not been decoded
    data_json jsonb NOT NULL,
    created_at timestamp NOT NULL DEFAULT NOW(),
    primary key (block_hash, log_index)
);

CREATE INDEX logs_address_block_number_idx ON logs(address, block_number);
CREATE INDEX logs_topic0_block_number_idx ON logs(topic0, block_number);
CREATE INDEX logs_topic1_idx ON logs(topic1) WHERE topic1 IS NOT NULL;
CREATE INDEX logs_topic2_idx ON logs(topic2) WHERE topic2 IS NOT NULL;
CREATE INDEX logs_tx_hash_idx ON logs(tx_hash);
