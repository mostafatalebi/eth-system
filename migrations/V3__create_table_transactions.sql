create table transactions (
    hash bytea NOT NULL check ( octet_length(hash) = 32 ) primary key,
    block_hash bytea NOT NULL check ( octet_length(block_hash) = 32 ) references blocks(hash) on delete cascade,
    block_number bigint NOT NULL,
    transaction_index integer NOT NULL,
    tx_type smallint NOT NULL,
    chain_id bigint DEFAULT NULL,
    nonce bigint NOT NULL,
    from_address bytea NOT NULL check ( octet_length(from_address) = 20 ),
    to_address bytea DEFAULT NULL check ( octet_length(to_address) = 20 ),
    value numeric(78, 0) NOT NULL,
    gas_limit bigint NOT NULL,
    gas_price numeric(78, 0) DEFAULT NULL,
    max_fee_per_gas numeric(78, 0) DEFAULT NULL,
    max_priority_fee_per_gas numeric(78, 0) DEFAULT NULL,
    max_fee_per_blob_gas numeric(78, 0) DEFAULT NULL,
    input bytea NOT NULL,
    access_list jsonb DEFAULT NULL,
    blob_versioned_hashes bytea[] DEFAULT NULL,
    v numeric(78, 0) DEFAULT NULL,
    r bytea NOT NULL check ( octet_length(r) = 32 ),
    s bytea NOT NULL check ( octet_length(s) = 32 ),
    y_parity boolean DEFAULT NULL,
    -- from the transaction receipt (eth_getBlockReceipts)
    receipt_status boolean DEFAULT NULL,
    receipt_gas_used bigint DEFAULT NULL,
    receipt_effective_gas_price numeric(78, 0) DEFAULT NULL,
    receipt_contract_address bytea DEFAULT NULL check ( octet_length(receipt_contract_address) = 20 ),
    receipt_blob_gas_used bigint DEFAULT NULL,
    receipt_blob_gas_price numeric(78, 0) DEFAULT NULL,
    created_at timestamp NOT NULL DEFAULT NOW(),
    updated_at timestamp NOT NULL DEFAULT NOW(),
    unique (block_hash, transaction_index)
);

CREATE INDEX transactions_block_number_idx ON transactions(block_number);
CREATE INDEX transactions_from_address_idx ON transactions(from_address);
CREATE INDEX transactions_to_address_idx ON transactions(to_address);
CREATE INDEX transactions_created_at_idx ON transactions(created_at);
