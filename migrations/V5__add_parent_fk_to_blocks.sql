ALTER TABLE blocks ADD COLUMN parent_ref bytea DEFAULT NULL CHECK (octet_length(parent_ref) = 32);

UPDATE blocks b SET parent_ref = b.parent_hash
WHERE EXISTS (SELECT 1 FROM blocks p WHERE p.hash = b.parent_hash);

CREATE UNIQUE INDEX blocks_parent_ref_idx ON blocks(parent_ref);

ALTER TABLE blocks ADD CONSTRAINT blocks_parent_ref_fkey
    FOREIGN KEY (parent_ref) REFERENCES blocks(hash) ON DELETE CASCADE;
