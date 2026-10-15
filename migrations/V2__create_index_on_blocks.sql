CREATE UNIQUE INDEX blocks_hash_idx ON blocks(hash);
CREATE UNIQUE INDEX blocks_number_idx ON blocks(number);
CREATE UNIQUE INDEX blocks_parent_hash_idx ON blocks(parent_hash);
CREATE INDEX blocks_timestamp_idx ON blocks(timestamp);
CREATE INDEX blocks_created_at_idx ON blocks(created_at);
CREATE INDEX blocks_updated_at_idx ON blocks(updated_at);
