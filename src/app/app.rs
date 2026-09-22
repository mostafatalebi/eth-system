use std::ops::DerefMut;
use std::sync::{Arc, RwLock};
use alloy::eips::{BlockId};
use alloy::network::{Network};
use alloy::providers::{Identity, Provider, ProviderBuilder, RootProvider, WsConnect};
use alloy::providers::fillers::{BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller};
use refinery::embed_migrations;
use revm::database::{AlloyDB, CacheDB, WrapDatabaseAsync};
use tokio_util::sync::CancellationToken;
use tracing::{info};
use tracing_subscriber::util::SubscriberInitExt;
use crate::app::cmd::{CmdArgs};
use crate::app::cmd::AppMode::Migration;
use crate::config::config::Config;
use crate::db::db::{DbFactory, DbType};
use crate::db::models::TxExecModel;
use crate::db::storage::{dispatch_storage_worker, AppStorage};
use crate::error::CoreError;
use crate::executor::executor::Executor;
use crate::indexer::client::eth::AlloyRpcClient;
use crate::indexer::fetcher::{FetchConfig, Fetcher};
use crate::indexer::indexer::Indexer;
use crate::indexer::repository::pg_repository::PgBlockRepository;
use crate::migrations::migration_runner::migrations;
use crate::subscriber::subscriber::{SubscriptionConfig, Subscriber};

pub struct App<S: AppStorage> {
    pub config: Arc<Config>,
    pub persistence: Option<Arc<DbFactory>>,
    pub storage: Arc<S>,
    provider: Option<AlloyProviderConcreteImpl>
}

impl<S: AppStorage> App<S> {

    pub async fn init(&mut self) -> Result<(), CoreError> {
        let db_res = DbFactory::connect(self.config.db_pg_url.as_str(), DbType::Postgres).await;
        if db_res.is_err() {
            return Err(CoreError::DbConnFailed(db_res.err().unwrap().to_string()));
        }
        self.persistence = Some(Arc::new(db_res.unwrap()));

        if self.config.app_mode != Migration {
            let ws = WsConnect::new(self.config.eth_ws_addr.clone());
            let provider = ProviderBuilder::new().connect_ws(ws).await;
            if provider.is_err() {
                return Err(CoreError::SubscriptionError(provider.err().unwrap().to_string()))
            }
            let provider = provider.unwrap();
            self.provider = Some(provider);
        }
        Ok(())
    }


    pub fn new_from_args(args: &CmdArgs, app_storage: S) -> Result<Self, CoreError> {
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
            Err(CoreError::ConfigLoadingFailed(c.err().unwrap().to_string()))
        }
    }

    pub async fn run_eth_simulator(&self) -> Result<(), CoreError> {
        let (tx_sn, tx_rc) = tokio::sync::mpsc::channel(1000);
        let (storage_sn, storage_rc) = tokio::sync::mpsc::channel::<TxExecModel>(1000);
        let sub_config = SubscriptionConfig{
            timeout: self.config.eth_tx_fetch_rpc_timeout,
            ws_url: self.config.eth_ws_addr.clone(),
            ch: tx_sn,
        };
        let span = tracing::error_span!("simulator");
        let _enter = span.enter();


        // Executor
        let block_number = self.provider.as_ref().unwrap().get_block_number().await;
        if block_number.is_err() {
            return Err(CoreError::FailedToFetchBlockNumber(block_number.err().unwrap().to_string()));
        }
        let alloy_db = WrapDatabaseAsync::new(AlloyDB::new(self.provider.clone().unwrap(), BlockId::from(block_number.unwrap())));
        let mut exec = Executor::new(self.provider.clone().unwrap(), CacheDB::new(alloy_db.unwrap()),  tx_rc, storage_sn).await;
        if exec.is_err() {
            return Err(exec.err().unwrap());
        }
        let mut exec = exec.unwrap();
        _ = tokio::join!(
            dispatch_storage_worker(self.storage.clone(), storage_rc),
            exec.start_worker(),
            Subscriber::start_listening_to_pending_transactions(self.provider.clone().unwrap(), &sub_config)
        );
        Ok(())
    }


    pub async fn run_eth_indexer(&self) -> Result<(), CoreError> {
        let (block_sn, block_rc) = tokio::sync::mpsc::channel(1000);
        let span = tracing::error_span!("indexer");
        let _enter = span.enter();

        // Executor
        // @todo must get block number from database or start from zero if non existing (or latest)
        let block_number = self.provider.as_ref().unwrap().get_block_number().await;
        if block_number.is_err() {
            return Err(CoreError::FailedToFetchBlockNumber(block_number.err().unwrap().to_string()));
        }
        let block_number = block_number.unwrap();
        let mut fetcher_cnf = FetchConfig::new();
        fetcher_cnf.set_batch_size(100);
        fetcher_cnf.set_ws_url(self.config.eth_ws_addr.clone());
        fetcher_cnf.set_next_block_number(block_number.clone());
        let repository = PgBlockRepository::new(self.persistence.clone().unwrap());
        let eth_client = AlloyRpcClient::new(self.config.eth_ws_addr.clone()).await?;
        let cancellation_token = CancellationToken::new();
        let fetcher = Fetcher::new(fetcher_cnf, repository, eth_client, cancellation_token);
        let mut indexer = Indexer::new(fetcher);
        _  = indexer.start_workers(block_sn, block_rc);
        Ok(())
    }


    pub fn app_storage(&self) -> Arc<S> {
        self.storage.clone()
    }


    pub async fn migrate(&self) -> Result<(), CoreError> {
        let mut client = self.persistence.as_ref().unwrap().pg().await?;
        let client = client.deref_mut().deref_mut();
        let report = migrations::runner().run_async(client).await;
        info!("Migrate report: {:?}", report);

        Ok(())
    }
}

pub type AlloyProviderConcreteImpl = FillProvider<JoinFill<Identity, JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>>, RootProvider>;
