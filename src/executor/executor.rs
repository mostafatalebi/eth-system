use alloy::consensus::Transaction as ConsensusTransaction;
use alloy::network::TransactionResponse;
use alloy::providers::{Provider, Network};
use alloy::rpc::types::Transaction;
use revm::{Context, Database, DatabaseCommit, ExecuteEvm, MainBuilder, MainContext};
use revm::context::CfgEnv;
use revm::context::result::{EVMError, ExecResultAndState, ExecutionResult, HaltReason};
use crate::error::AppErr;
use revm::database::{AlloyDB, AlloyDBError, CacheDB, WrapDatabaseAsync};
use revm::primitives::B256;
use tracing::{error, info};
use crate::common::constants::MIN_GAS_LIMIT;
use crate::common::helpers::{alloy_tx_to_revm_tx, validate_balance, validate_nonce, validate_tx_signature};
use crate::db::models::TransactionModel;
use crate::mem::pool::MemPool;

pub struct Executor<N: Network, P: Provider<N> + Clone> {
    provider: P,
    incoming_ch: tokio::sync::mpsc::Receiver<Transaction>,
    done_tx_ch: tokio::sync::mpsc::Sender<TransactionModel>,
    cache_db: CacheDB<WrapDatabaseAsync<AlloyDB<N, P>>>,
    chain_id: u64
}


impl<N: Network, P: Provider<N> + Clone>  Executor<N, P> {
    pub async fn new(provider: P, cache_db: CacheDB<WrapDatabaseAsync<AlloyDB<N, P>>>,
                     incoming_ch: tokio::sync::mpsc::Receiver<Transaction>,
                     done_tx_ch: tokio::sync::mpsc::Sender<TransactionModel>) -> Result<Self, AppErr> {
        let mut ex = Self{ provider, incoming_ch, cache_db, chain_id: 0, done_tx_ch };

        let r = ex.initialize_chain_id().await;

        if r.is_err() {
            return Err(r.err().unwrap());
        }
        Ok(ex)
    }

    pub async fn initialize_chain_id(&mut self) -> Result<(), AppErr> {
        match self.provider.get_chain_id().await {
            Ok(chain_id) => {
                self.chain_id += chain_id;
                Ok(())
            }
            Err(e) => {
                Err(AppErr::FailedToRetrieveChainId(e.to_string()))
            }
        }
    }

    pub async fn start_worker(&mut self) {
        let span = tracing::error_span!("indexing_operation");
        let _enter = span.enter();
        while let Some(tx) = self.incoming_ch.recv().await {
            let res = self.validate(&tx).await;
            if res.is_err() {
                error!("failed to validate tx: {res:?}");
                continue;
            }
            let revm_tx = alloy_tx_to_revm_tx(&tx);
            if revm_tx.is_err() {
                error!("{}", AppErr::TxTypeCastFailed(revm_tx.err().unwrap().to_string()));
                continue;
            }
            let revm_tx = revm_tx.unwrap();
            let ctx = Context::mainnet()
                .with_db(&mut self.cache_db)
                .with_cfg(CfgEnv::new().with_chain_id(self.chain_id));
            let mut evm_exec = ctx.build_mainnet();
            let tx_result = evm_exec.transact(revm_tx);

            if self.check_result(&tx.inner.hash(), &tx_result) {
                self.cache_db.commit(tx_result.as_ref().unwrap().state.clone());
                let account = self.cache_db.basic(tx.from());
                if account.is_err() {
                    error!("failed to get account info: {:?}", tx.from());
                }
                let tx_model = TransactionModel::new(tx.clone(), tx_result.as_ref().unwrap().clone(), account.unwrap().unwrap());
                _ = self.done_tx_ch.send(tx_model).await;
                info!("tx successfully executed: {}", tx_result.as_ref().unwrap().result.clone());
            } else {
                error!("tx execution finished with error: {tx_result:?}");
            }
        }
    }

    pub fn check_result(&self, tx_hash: &B256, result: &Result<ExecResultAndState<ExecutionResult>, EVMError<AlloyDBError>>) -> bool {
        if result.is_err() {
            error!("{:?}", result.as_ref().err().unwrap().to_string());
            return false;
        }

        match &(result.as_ref().unwrap().result) {
            ExecutionResult::Success { .. } => {
                return true;
            }
            ExecutionResult::Revert { .. } => {
                info!("tx reverted: hash={tx_hash}");
                return false;
            }
            ExecutionResult::Halt { reason,.. } => {
                match reason {
                    HaltReason::OutOfGas(out_of_gas_type) => {
                        error!("transaction halted: {:?}", out_of_gas_type);
                    },
                    _ => {
                        error!("transaction halted: {:?}", reason);
                    }
                }
                false
            }
        }
    }

    pub async fn validate(&self, tx: &Transaction) -> Result<(), AppErr> {
        if let Err(e) = validate_tx_signature(tx) {
            return Err(AppErr::ValidationFailed(e.to_string()));
        }

        // confirm chain_id
        let tx_chain_id = ConsensusTransaction::chain_id(tx).unwrap_or(0u64);
        if self.chain_id != tx_chain_id {
            return Err(AppErr::ValidationFailed(format!("chain_id mismatch {:?} is not {:?}", self.chain_id, tx_chain_id)));
        }

        // sender nonce
        let current_nonce = self.provider.get_transaction_count(tx.from()).await;
        if current_nonce.is_err() {
            return Err(AppErr::ValidationFailed(format!("failed to retrieve nonce info {:?}", current_nonce.err())));
        } else {
            if let Err(e) = validate_nonce(tx.nonce(), current_nonce.unwrap()) {
                return Err(AppErr::ValidationFailed(format!("failed to validate nonce {:?}", e)));
            }
        }

        // sufficient balance
        let curr_balance = self.provider.get_balance(tx.from()).await;
        if curr_balance.is_err() {
            return Err(AppErr::ValidationFailed(format!("failed to retrieve balance info {:?}", curr_balance.err())));
        }
        let max_fee_per_gas = alloy::primitives::U256::from(TransactionResponse::max_fee_per_gas(tx).unwrap());
        if let Err(e) = validate_balance(curr_balance.unwrap(), tx.value(), alloy::primitives::U256::from(tx.gas_limit()), max_fee_per_gas) {
            return Err(AppErr::ValidationFailed(format!("failed to validate balance {:?}", e)));
        }

        // basic naive gas limit check
        if tx.gas_limit() < MIN_GAS_LIMIT {
            return Err(AppErr::ValidationFailed("gas limit insufficient".to_string()));
        }

        Ok(())
    }
}