ALTER TABLE blocks ADD COLUMN is_finalized_checkpoint boolean NOT NULL DEFAULT false;

CREATE INDEX blocks_finalized_checkpoint_idx ON blocks(number) WHERE is_finalized_checkpoint;
