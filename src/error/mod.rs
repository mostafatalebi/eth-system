mod block_error;

use std::error::Error;
use std::fmt;
use revm::primitives::{B256, U256};

// @todo we need to split this enum into several other modules (such as DbError etc.)
#[derive(Debug)]
pub enum CoreError {
    Internal(Box<dyn Error+Send+Sync>),
    Empty(String),
    BadArgument(String),
    FileNotFound(String),
    FailedToStartHttpServer(String),
    ConfigLoadingFailed(String),
    DbConnFailed(String),
    DbGetClientFailed(String),
    DbTxOpenFailed(String),
    DbTxExecFailed(String),
    DbTxRollbackFailed(String),
    DbTxCommitFailed(String),
    DbWriteFailed(String),
    DbQueryFailed(String),
    DbInsertFailed(String),
    // parent_hash, new_block_hash, old_block_hash
    DbDuplicateBlockWithDifferentHash(Vec<u8>, Vec<u8>, Vec<u8>),
    SubscriptionError(String),
    TxTypeCastFailed(String),
    TxValidationFailed(String),
    SigRecoveryFailed(String),
    SigInvalid(String),
    ValidationFailed(String),
    FailedToRetrieveChainId(String),
    NonceIsLower(String),
    NonceIsHigher(String),
    InsufficientBalance(String),
    TransactFailed(String),
    FailedToFetchBlockNumber(String),
    StorageErrorHashExists(String),
    DbTypeDecodingFailed(String),
    DbTypeEncodingFailed(String),
    DbNotFound(String),
    WsConnFailed(String),
    HttpConnFailed(String),
    RpcRequestFailed(String),
    DataSizeUnexpected(usize,usize),
    BlockNumberUnexpected(u64, u64),
    ParentBlockNotFound(Vec<u8>),
    ReorgBelowFinalized(u64),
    ParentHashMismatch(Vec<u8>),
    UserAlreadyExists(String),
    PasswordHashFailed(String),
    JwtFailed(String),
    Unauthorized(String),
    MigrationFailed(String),
    RedisFailed(String),
}


impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Internal(e) => write!(f, "{}", e),
            CoreError::Empty(msg) => write!(f, "empty: {msg}"),
            CoreError::BadArgument(msg) => write!(f, "invalid input: {msg}"),
            CoreError::FileNotFound(msg) => write!(f, "file not found: {msg}"),
            CoreError::FailedToStartHttpServer(msg) => write!(f, "failed to start http server: {msg}"),
            CoreError::ConfigLoadingFailed(msg) => write!(f, "config loading failed: {msg}"),
            CoreError::DbConnFailed(msg) => write!(f, "failed to connect to DB: {msg}"),
            CoreError::DbTxRollbackFailed(msg) => write!(f, "failed to rollback tx: {msg}"),
            CoreError::DbTxCommitFailed(msg) => write!(f, "failed to commit tx: {msg}"),
            CoreError::DbGetClientFailed(msg) => write!(f, "failed to get clients to Pg's pool: {msg}"),
            CoreError::DbInsertFailed(msg) => write!(f, "failed to insert to DB: {msg}"),
            CoreError::DbDuplicateBlockWithDifferentHash(parent, old, new) => {
                let parent = String::from_utf8_lossy(parent).to_string();
                let old = String::from_utf8_lossy(old).to_string();
                let new = String::from_utf8_lossy(new).to_string();
                write!(f, "db error: duplicate block: parent={parent} old={old}, new={new}")
            },
            CoreError::DbTxOpenFailed(msg) => write!(f, "failed to open tx to DB: {msg}"),
            CoreError::DbTxExecFailed(msg) => write!(f, "failed to execute tx: {msg}"),
            CoreError::DbQueryFailed(msg) => write!(f, "failed to query DB: {msg}"),
            CoreError::DbWriteFailed(msg) => write!(f, "failed to write to DB: {msg}"),
            CoreError::SubscriptionError(msg) => write!(f, "subscription error: {msg}"),
            CoreError::TxTypeCastFailed(msg) => write!(f, "failed to type-case tx: {msg}"),
            CoreError::TxValidationFailed(msg) => write!(f, "failed to validator tx: {msg}"),
            CoreError::SigRecoveryFailed(msg) => write!(f, "failed to recover address: {msg}"),
            CoreError::SigInvalid(msg) => write!(f, "invalid signature: {msg}"),
            CoreError::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            CoreError::FailedToRetrieveChainId(msg) => write!(f, "failed to retrieve chain_id: {msg}"),
            CoreError::NonceIsLower(msg) => write!(f, "nonce is lower: {msg}"),
            CoreError::NonceIsHigher(msg) => write!(f, "nonce is higher: {msg}"),
            CoreError::InsufficientBalance(msg) => write!(f, "insufficient balance: {msg}"),
            CoreError::TransactFailed(msg) => write!(f, "transaction failed: {msg}"),
            CoreError::FailedToFetchBlockNumber(msg) => write!(f, "failed to fetch block number: {msg}"),
            CoreError::StorageErrorHashExists(msg) => write!(f, "tx hash already exists: {msg}"),
            CoreError::DbTypeDecodingFailed(msg) => write!(f, "db type-decoding failed: {msg}"),
            CoreError::DbTypeEncodingFailed(msg) => write!(f, "db type-encoding failed: {msg}"),
            CoreError::DbNotFound(msg) => write!(f, "not found in the db: {msg}"),
            CoreError::WsConnFailed(msg) => write!(f, "websocket connect failed: {msg}"),
            CoreError::HttpConnFailed(msg) => write!(f, "http connect failed: {msg}"),
            CoreError::RpcRequestFailed(msg) => write!(f, "rpc req failed: {msg}"),
            CoreError::DataSizeUnexpected(expected, actual) => write!(f, "unexpected data size: must be={expected} but is {actual}"),
            CoreError::BlockNumberUnexpected(expected, actual) => write!(f, "unexpected block number: must be={expected} but is {actual}"),
            CoreError::ParentBlockNotFound(p) => {
                let p = String::from_utf8_lossy(p);
                write!(f, "block search by parent_hash={p} yielded no result")
            }
            CoreError::ReorgBelowFinalized(n) => write!(f, "reorg reaches finalized block {n}: database is on a different chain"),
            CoreError::UserAlreadyExists(u) => write!(f, "user already exists: {u}"),
            CoreError::PasswordHashFailed(msg) => write!(f, "failed to hash password: {msg}"),
            CoreError::JwtFailed(msg) => write!(f, "jwt error: {msg}"),
            CoreError::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            CoreError::MigrationFailed(msg) => write!(f, "migration failed: {msg}"),
            CoreError::RedisFailed(msg) => write!(f, "redis error: {msg}"),
            CoreError::ParentHashMismatch(h) => write!(f, "parent of block 0x{} is not stored", alloy::hex::encode(h)),
        }
    }
}

impl CoreError {
    /// returns true if `self` and `other` are the same variant, ignoring their payloads:
    /// `err.is(&CoreError::DbQueryFailed(String::new()))`
    pub fn is(&self, other: &CoreError) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Error for CoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CoreError::Internal(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_compares_variants_not_payloads() {
        let err = CoreError::DbQueryFailed("timeout".to_string());
        assert!(err.is(&CoreError::DbQueryFailed(String::new())));
        assert!(!err.is(&CoreError::DbInsertFailed("timeout".to_string())));
        assert!(CoreError::DataSizeUnexpected(1, 2).is(&CoreError::DataSizeUnexpected(0, 0)));
    }
}
