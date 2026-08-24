use std::any::Any;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};
use alloy::network::TransactionResponse;
use alloy::primitives::B256;
use redb::{Database, TableDefinition};
use revm::context::TxEnv;
use revm::primitives::U256;
use tracing::error;
use crate::db::models::TransactionModel;
use crate::error::AppErr;


pub trait AppStorage: Send + Sync + 'static {
    // @todo later we need to set a write id for its return type
    fn insert_transaction(&self, tx: TransactionModel) -> Result<(),AppErr>;
    fn get_transaction(&self, tx: B256) -> Result<Option<TransactionModel>,AppErr>;
}




pub struct InMemoryStorage {
    storage: RwLock<HashMap<B256, TransactionModel>>
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(HashMap::new())
        }
    }
}

impl AppStorage for InMemoryStorage {
    fn insert_transaction(&self, tx: TransactionModel) -> Result<(), AppErr> {
        let mut s = self.storage.write().unwrap();
        if s.contains_key(&tx.tx.tx_hash()) {
            return Err(AppErr::StorageErrorHashExists(format!("hash={:?}", tx.tx.tx_hash())))
        }
        s.insert(tx.tx.tx_hash(), tx);
        Ok(())
    }

    fn get_transaction(&self, tx: B256) -> Result<Option<TransactionModel>, AppErr> {
        let s = self.storage.read().unwrap();
        let item = s.get(&tx);

        Ok(Some(item.unwrap().clone()))
    }
}


pub async fn dispatch_storage_worker(mut storage: Arc<impl AppStorage>, mut rc: tokio::sync::mpsc::Receiver<TransactionModel>) {
    while let Some(tx_model) = rc.recv().await {
        if let Err(e) =storage.insert_transaction(tx_model.clone()) {
            error!(error = %e, tx = %(tx_model.tx.tx_hash()), "failed to insert transaction" );
        }
    }
}