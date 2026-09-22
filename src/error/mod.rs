mod block_error;

use std::error::Error;
use std::fmt;


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
    DbWriteFailed(String),
    DbQueryFailed(String),
    DbInsertFailed(String),
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
    RpcRequestFailed(String),
    DataSizeUnexpected(usize,usize),
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
            CoreError::DbGetClientFailed(msg) => write!(f, "failed to get client to Pg's pool: {msg}"),
            CoreError::DbInsertFailed(msg) => write!(f, "failed to insert to DB: {msg}"),
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
            CoreError::RpcRequestFailed(msg) => write!(f, "rpc req failed: {msg}"),
            CoreError::DataSizeUnexpected(expected, actual) => write!(f, "unexpected data size: must be={expected} but is {actual}"),
        }
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