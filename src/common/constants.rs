use std::string::ToString;

// we know this changes based on a number
// of factors (calldata, contract creation etc.)
// but it is never below this
pub const MIN_GAS_LIMIT: u64 = 21_000;

pub const BLOCKS_TABLE_NAME: &str = "blocks";