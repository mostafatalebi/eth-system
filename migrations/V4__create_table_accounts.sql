create type account_type as ENUM (
    'eoa',
    'contract'
);

create table accounts (
    address bytea primary key check(octet_length(address) = 20) ,
    account_type account_type not null,
    nonce bigint NOT NULL,
    value numeric(78, 0) NOT NULL,
    code_hash bytea NOT NULL check(octet_length(code_hash) = 32),
    first_block_number bigint not null,
    last_block_number bigint not null,
    delegate_address bytea check(octet_length(delegate_address) = 20),
    delegate_updated_at_block_number bigint,
    created_at timestamp NOT NULL DEFAULT NOW(),
    updated_at timestamp NOT NULL DEFAULT NOW()
);

CREATE INDEX accounts_account_type_idx ON accounts(account_type);
CREATE INDEX accounts_first_block_number_idx ON accounts(first_block_number);
CREATE INDEX accounts_last_block_number_idx ON accounts(last_block_number);
CREATE INDEX accounts_created_at_idx ON accounts(created_at);
CREATE INDEX accounts_updated_at_idx ON accounts(updated_at);
