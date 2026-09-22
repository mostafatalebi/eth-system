use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::Hash;
use std::ops::Index;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use alloy::network::TransactionResponse;
use alloy::primitives::B256;
use redb::{Database, TableDefinition};
use revm::context::TxEnv;
use revm::primitives::U256;
use tokio_postgres::types::IsNull::No;
use tracing::error;
use crate::db::models::TxExecModel;
use crate::error::CoreError;


pub trait AppStorage: Send + Sync + 'static {
    // @todo later we need to set a write id for its return type
    fn insert_transaction(&self, tx: TxExecModel) -> Result<(), CoreError>;
    fn get_transaction(&self, tx: B256) -> Result<Option<TxExecModel>, CoreError>;

    fn get_list(&self) -> Result<Option<Vec<B256>>, CoreError>;
}




pub struct InMemoryStorage {
    serial:  AtomicU64,
    indexes: RwLock<BTreeMap<u64, B256>>,
    storage: RwLock<HashMap<B256, TxExecModel>>
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            serial: AtomicU64::new(0),
            indexes: RwLock::new(BTreeMap::new()),
            storage: RwLock::new(HashMap::new())
        }
    }
}

impl AppStorage for InMemoryStorage {
    fn insert_transaction(&self, tx: TxExecModel) -> Result<(), CoreError> {
        let mut s = self.storage.write().unwrap();
        if s.contains_key(&tx.tx.tx_hash()) {
            return Err(CoreError::StorageErrorHashExists(format!("hash={:?}", tx.tx.tx_hash())))
        }
        let hash = tx.tx.tx_hash();
        s.insert(hash, tx);
        self.indexes.write().unwrap().insert(self.serial.load(Ordering::Relaxed), hash);
        self.serial.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn get_transaction(&self, tx: B256) -> Result<Option<TxExecModel>, CoreError> {
        let s = self.storage.read().unwrap();
        let item = s.get(&tx);

        Ok(Some(item.unwrap().clone()))
    }

    fn get_list(&self) -> Result<Option<Vec<B256>>, CoreError> {
        if self.serial.load(Ordering::Relaxed) == 0 {
            return Ok(None)
        }
        let mut response_list: Vec<B256> = Vec::new();
        let s = self.storage.read().unwrap();
        let mut i = 1;
        for (_, v) in self.indexes.read().unwrap().iter() {
            response_list.push(v.clone())
        }
        return Ok(Some(response_list))
    }
}


pub async fn dispatch_storage_worker(mut storage: Arc<impl AppStorage>, mut rc: tokio::sync::mpsc::Receiver<TxExecModel>) {
    while let Some(tx_model) = rc.recv().await {
        if let Err(e) =storage.insert_transaction(tx_model.clone()) {
            error!(error = %e, tx = %(tx_model.tx.tx_hash()), "failed to insert transaction" );
        }
    }
}