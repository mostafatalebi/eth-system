use alloy::rpc::types::Block;
use crate::error::CoreError;

pub struct BlockValidator {
}


impl BlockValidator {

    pub fn validate_rpc_block(block: &Block, expected_block_number: u64) -> Result<(), CoreError> {
        if block.header.hash.len() != 32 {
            return Err(CoreError::DataSizeUnexpected(32, block.header.hash.len()))
        } 
        if expected_block_number != block.header.number {
            return Err(CoreError::DataSizeUnexpected(32, block.header.hash.len()))
        }
        return Ok(())
    }
}