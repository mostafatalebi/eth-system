

pub const BLOCK_INSERT_QUERY: &str = "insert into blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, parent_beacon_block_root,
                    withdrawals_root, fee_recipient, requests_hash, extra_data, size, gas_limit, gas_used, base_fee_per_gas, blob_gas_used, excess_blob_gas,
                        prev_randao, timestamp, parent_ref) values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22,
                            (select case when exists (select 1 from blocks) then $3::bytea end))";

pub const TX_INSERT_QUERY: &str = "insert into transactions (hash, block_hash, block_number, transaction_index, tx_type, chain_id, nonce, from_address, to_address,
                    value, gas_limit, gas_price, max_fee_per_gas, max_priority_fee_per_gas, max_fee_per_blob_gas, input, access_list, blob_versioned_hashes,
                        v, r, s, y_parity) values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::text::numeric, $11, $12::text::numeric, $13::text::numeric,
                            $14::text::numeric, $15::text::numeric, $16, $17::text::jsonb, $18, $19::text::numeric, $20, $21, $22)";

pub const ACCOUNT_UPSERT_QUERY: &str = "insert into accounts (address, account_type, nonce, value, code_hash, first_block_number, last_block_number, delegate_address,
    delegate_updated_at_block_number, created_at, updated_at) values($1, $2, $3, $4::text::numeric, $5, $6, $7, $8, $9, NOW(), NOW())
    on conflict (address) do update set
    updated_at = excluded.updated_at, nonce = excluded.nonce, value = excluded.value, last_block_number = excluded.last_block_number, delegate_address = excluded.delegate_address,
    code_hash = excluded.code_hash,
    delegate_updated_at_block_number = CASE WHEN accounts.delegate_address IS DISTINCT FROM excluded.delegate_address
        THEN excluded.last_block_number
        ELSE accounts.delegate_updated_at_block_number END,
    account_type = case when excluded.account_type = 'contract' then excluded.account_type else accounts.account_type end
    where accounts.last_block_number <= excluded.last_block_number";



pub const BLOCK_MARK_FINALIZED_CHECKPOINT_QUERY: &str = "update blocks set is_finalized_checkpoint = true where hash = $1";

pub const BLOCK_IS_FINALIZED_QUERY: &str = "select exists (select 1 from blocks where is_finalized_checkpoint and number >= $1) as finalized";

pub const LAST_FINALIZED_CHECKPOINT_QUERY: &str = "select max(number) as number from blocks where is_finalized_checkpoint";