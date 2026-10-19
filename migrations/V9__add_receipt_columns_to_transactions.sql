-- data from the transaction receipt (eth_getBlockReceipts); NULL for rows indexed before receipts were fetched
ALTER TABLE transactions
    ADD COLUMN receipt_status boolean DEFAULT NULL,
    ADD COLUMN receipt_gas_used bigint DEFAULT NULL,
    ADD COLUMN receipt_effective_gas_price numeric(78, 0) DEFAULT NULL,
    ADD COLUMN receipt_contract_address bytea DEFAULT NULL check ( octet_length(receipt_contract_address) = 20 ),
    ADD COLUMN receipt_blob_gas_used bigint DEFAULT NULL,
    ADD COLUMN receipt_blob_gas_price numeric(78, 0) DEFAULT NULL;
