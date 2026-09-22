use std::fmt;

#[derive(Debug)]
pub enum BlockError {
    BadBlockNumber(u64, u64)
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::BadBlockNumber(e, a) => write!(f, "bad block number. expected={e} actual={a}"),
        }
    }
}