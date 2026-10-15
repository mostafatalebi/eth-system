use std::fmt;
use alloy::primitives::B256;

#[derive(Debug)]
pub enum BlockError {
    BadBlockNumber(u64, u64),
    DuplicateBlock(B256, B256)
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::BadBlockNumber(e, a) => write!(f, "bad block number. expected={e} actual={a}"),
            BlockError::DuplicateBlock(e, a) => write!(f, "duplicate block number. expected={e} actual={a}"),
        }
    }
}

