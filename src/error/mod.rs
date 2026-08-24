use std::fmt;

#[derive(Debug)]
#[derive(PartialEq)]
pub enum AppErr {
    BadArgument(String),
    FileNotFound(String),
    FailedToStartHttpServer(String),
    ConfigLoadingFailed(String),
    DbConnFailed(String),
    DbWriteFailed(String),
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
}

impl fmt::Display for AppErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppErr::BadArgument(msg) => write!(f, "invalid input: {msg}"),
            AppErr::FileNotFound(msg) => write!(f, "file not found: {msg}"),
            AppErr::FailedToStartHttpServer(msg) => write!(f, "failed to start http server: {msg}"),
            AppErr::ConfigLoadingFailed(msg) => write!(f, "config loading failed: {msg}"),
            AppErr::DbConnFailed(msg) => write!(f, "failed to connect to DB: {msg}"),
            AppErr::DbWriteFailed(msg) => write!(f, "failed to write to DB: {msg}"),
            AppErr::SubscriptionError(msg) => write!(f, "subscription error: {msg}"),
            AppErr::TxTypeCastFailed(msg) => write!(f, "failed to type-case tx: {msg}"),
            AppErr::TxValidationFailed(msg) => write!(f, "failed to validator tx: {msg}"),
            AppErr::SigRecoveryFailed(msg) => write!(f, "failed to recover address: {msg}"),
            AppErr::SigInvalid(msg) => write!(f, "invalid signature: {msg}"),
            AppErr::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            AppErr::FailedToRetrieveChainId(msg) => write!(f, "failed to retrieve chain_id: {msg}"),
            AppErr::NonceIsLower(msg) => write!(f, "nonce is lower: {msg}"),
            AppErr::NonceIsHigher(msg) => write!(f, "nonce is higher: {msg}"),
            AppErr::InsufficientBalance(msg) => write!(f, "insufficient balance: {msg}"),
            AppErr::TransactFailed(msg) => write!(f, "transaction failed: {msg}"),
            AppErr::FailedToFetchBlockNumber(msg) => write!(f, "failed to fetch block number: {msg}"),
            AppErr::StorageErrorHashExists(msg) => write!(f, "tx hash already exists: {msg}"),
        }
    }
}

impl std::error::Error for AppErr {}