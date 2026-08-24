use alloy::rpc::types::Transaction;
use axum::response::{IntoResponse, Response};
use revm::context::result::{ExecResultAndState, ExecutionResult};
use revm::state::{AccountInfo};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionModel {
    pub tx: Transaction,
    pub exec_result: ExecResultAndState<ExecutionResult>,
    pub account: AccountInfo
}


impl TransactionModel {
    pub fn new(tx: Transaction, exec_result: ExecResultAndState<ExecutionResult>, account: AccountInfo) -> TransactionModel {
        Self { tx, exec_result, account }
    }

    pub fn with(&mut self, tx: Transaction, exec_result: ExecResultAndState<ExecutionResult>, account: AccountInfo) {
        self.tx = tx.clone();
        self.exec_result = exec_result.clone();
        self.account = account.clone();
    }
}