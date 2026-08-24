use std::sync::{Arc, Mutex, RwLock};
use alloy::eips::{BlockId};
use alloy::network::{Ethereum, Network};
use alloy::providers::{Identity, Provider, ProviderBuilder, RootProvider, WsConnect};
use alloy::providers::fillers::{BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller};
use revm::database::{AlloyDB, CacheDB, WrapDatabaseAsync};
use tracing::{info};
use crate::app::cmd::{CmdArgs};
use crate::config::config::Config;
use crate::db::db::Persistence;
use crate::db::models::TransactionModel;
use crate::db::storage::{dispatch_storage_worker, AppStorage, InMemoryStorage};
use crate::error::AppErr;
use crate::executor::executor::Executor;
use crate::http_server::routes::get_router;
use crate::http_server::server::HttpServer;
use crate::subscriber::subscriber::{SubscriptionConfig, Subscriber};

pub struct App<S: AppStorage> {
    pub config: Arc<Config>,
    pub persistence: Option<Arc<Persistence>>,
    pub storage: Arc<S>,
    provider: Option<AlloyProviderConcreteImpl>
}

impl<S: AppStorage> App<S> {

    pub async fn init(&mut self) -> Result<(), AppErr> {
        let db_res = Persistence::pg(self.config.db_pg_url.as_str()).await;
        if db_res.is_err() {
            return Err(AppErr::DbConnFailed(db_res.err().unwrap().to_string()));
        }
        self.persistence = Some(Arc::new(db_res.unwrap()));


        let ws = WsConnect::new(self.config.eth_ws_addr.clone());
        let provider = ProviderBuilder::new().connect_ws(ws).await;
        if provider.is_err() {
            return Err(AppErr::SubscriptionError(provider.err().unwrap().to_string()))
        }
        let provider = provider.unwrap();

        self.provider = Some(provider);
        Ok(())
    }


    pub fn new_from_args(args: &CmdArgs, app_storage: S) -> Result<Self, AppErr> {
        let c = Config::load_from_file(args.config.to_string());

        if c.is_ok() {
            let mut cnf = c.unwrap();
            cnf.init(&args);
            Ok(Self {
                config: Arc::new(cnf.clone()),
                persistence: None,
                storage: Arc::new(app_storage),
                provider: None,
            })
        } else {
            Err(AppErr::ConfigLoadingFailed(c.err().unwrap().to_string()))
        }
    }

    pub async fn run_eth_simulator(&self) -> Result<(), AppErr> {
        let (tx_sn, tx_rc) = tokio::sync::mpsc::channel(1000);
        let (storage_sn, storage_rc) = tokio::sync::mpsc::channel::<TransactionModel>(1000);
        let sub_config = SubscriptionConfig{
            timeout: self.config.eth_tx_fetch_rpc_timeout,
            ws_url: self.config.eth_ws_addr.clone(),
            ch: tx_sn,
        };
        let span = tracing::error_span!("indexing_operation");
        let _enter = span.enter();


        // Executor
        let block_number = self.provider.as_ref().unwrap().get_block_number().await;
        if block_number.is_err() {
            return Err(AppErr::FailedToFetchBlockNumber(block_number.err().unwrap().to_string()));
        }
        let alloy_db = WrapDatabaseAsync::new(AlloyDB::new(self.provider.clone().unwrap(), BlockId::from(block_number.unwrap())));
        let mut exec = Executor::new(self.provider.clone().unwrap(), CacheDB::new(alloy_db.unwrap()),  tx_rc, storage_sn).await;
        if exec.is_err() {
            return Err(exec.err().unwrap());
        }
        let mut exec = exec.unwrap();
        tokio::join!(
            dispatch_storage_worker(self.storage.clone(), storage_rc),
            exec.start_worker(),
            Subscriber::start_listening(self.provider.clone().unwrap(), &sub_config)
        );
        Ok(())
    }


    pub fn app_storage(&self) -> Arc<S> {
        self.storage.clone()
    }
}

pub type AlloyProviderConcreteImpl = FillProvider<JoinFill<Identity, JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>>, RootProvider>;
