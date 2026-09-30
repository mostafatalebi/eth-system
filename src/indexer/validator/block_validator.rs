use alloy::rpc::types::Block;
use crate::error::CoreError;

pub struct BlockValidator {
}


impl BlockValidator {

    /// validates an incoming block against the stream so far. `expected_block_number` is
    /// the number following the previously received block; None for the first block,
    /// since there is nothing to compare against yet.
    ///
    /// note: header.hash is a B256, so its size is guaranteed by the type and not checked here
    pub fn validate_rpc_block(block: &Block, expected_block_number: Option<u64>) -> Result<(), CoreError> {
        if let Some(expected) = expected_block_number {
            if expected != block.header.number {
                return Err(CoreError::BlockNumberUnexpected(expected, block.header.number))
            }
        }
        return Ok(())
    }


}
