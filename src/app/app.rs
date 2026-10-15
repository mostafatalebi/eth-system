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
use crate::app::cmd::AppMode::{Migration, Simulator};
use crate::auth::pg_repository::PgUserRepository;
use crate::auth::seed::seed_default_user;
use crate::config::config::Config;
use crate::db::db::{DbFactory, DbType};
use crate::db::models::TxExecModel;
use crate::db::storage::{dispatch_storage_worker, AppStorage};
use crate::error::CoreError;
use crate::executor::executor::Executor;
use crate::indexer::clients::eth::AlloyRpcClient;
use crate::indexer::fetcher::{FetchConfig, Fetcher};
use crate::indexer::indexer::Indexer;
use crate::indexer::repository::repository::BlockRepository;
use crate::migrations::migration_runner::migrations;
use crate::subscriber::subscriber::{SubscriptionConfig, Subscriber};

pub struct App<S: AppStorage, R: BlockRepository+Clone> {
    pub config: Arc<Config>,
    pub persistence: Option<Arc<DbFactory>>,
    pub block_repository: Option<R>,
    pub storage: Arc<S>,
    provider: Option<AlloyProviderConcreteImpl>
}

impl<S: AppStorage, R: BlockRepository+Clone> App<S, R> {

    pub async fn init(&mut self) -> Result<(), CoreError> {
        let db_res = DbFactory::connect(self.config.db_pg_url.as_str(), DbType::Postgres).await;
        if db_res.is_err() {
            return Err(CoreError::DbConnFailed(db_res.err().unwrap().to_string()));
        }
        self.persistence = Some(Arc::new(db_res.unwrap()));

        if self.config.app_mode == Simulator {
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


    /// injects the block repository; must be called after init(), since concrete
    /// repositories (e.g. PgBlockRepository) depend on the persistence created there
    pub fn set_block_repository(&mut self, block_repository: R) {
        self.block_repository = Some(block_repository);
    }

    pub fn new_from_args(args: &CmdArgs, app_storage: S) -> Result<Self, CoreError> {
        let c = Config::load_from_file(args.config.to_string());

        if c.is_ok() {
            let mut cnf = c.unwrap();
            cnf.init(&args);
            Ok(Self {
                config: Arc::new(cnf.clone()),
                persistence: None,
                block_repository: None,
                storage: Arc::new(app_storage),
                provider: None,
                
            })
        } else {
            Err(CoreError::ConfigLoadingFailed(c.err().unwrap().to_string()))
        }
    }

    #[tracing::instrument(name = "simulator", level = "error", skip_all)]
    pub async fn run_eth_simulator(&self) -> Result<(), CoreError> {
        let (tx_sn, tx_rc) = tokio::sync::mpsc::channel(1000);
        let (storage_sn, storage_rc) = tokio::sync::mpsc::channel::<TxExecModel>(1000);
        let sub_config = SubscriptionConfig{
            timeout: self.config.eth_tx_fetch_rpc_timeout,
            ws_url: self.config.eth_ws_addr.clone(),
            ch: tx_sn,
        };


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


    #[tracing::instrument(name = "indexer", level = "error", skip_all)]
    pub async fn run_eth_indexer(&self) -> Result<(), CoreError> {
        let (block_sn, block_rc) = tokio::sync::mpsc::channel(1000);

        let mut fetcher_cnf = FetchConfig::new();
        fetcher_cnf.set_batch_size(self.config.block_retrieval_batch_size);
        fetcher_cnf.set_ws_url(self.config.eth_ws_addr.clone());
        let repository = self.block_repository.clone()
            .ok_or(CoreError::Empty("block repository is not set".to_string()))?;
        let eth_client = AlloyRpcClient::new(&self.config.block_upstream_http_addr).await?;
        let cancellation_token = CancellationToken::new();
        let fetcher = Fetcher::new(fetcher_cnf, repository, eth_client, cancellation_token);
        let mut indexer = Indexer::new(fetcher);
        let mut last_block_number = self.block_repository.as_ref().unwrap().last_block_number().await?;
        if last_block_number.is_none() {
            last_block_number.replace(0);
        } else {
            last_block_number = Some(last_block_number.unwrap() + 1)
        }

        if self.config.start_block_number > last_block_number.unwrap() {
            last_block_number.replace(self.config.start_block_number);
        }
        indexer.start_workers(last_block_number, block_sn, block_rc).await;
        Ok(())
    }


    pub fn app_storage(&self) -> Arc<S> {
        self.storage.clone()
    }


    pub async fn migrate(&self) -> Result<(), CoreError> {
        let persistence = self.persistence.as_ref().unwrap();
        let mut client = persistence.pg().await?;
        let client = client.deref_mut().deref_mut();
        let report = migrations::runner().run_async(client).await
            .map_err(|e| CoreError::MigrationFailed(e.to_string()))?;
        info!("applied {} migration(s)", report.applied_migrations().len());

        seed_default_user(&PgUserRepository::new(persistence.clone()), &self.config).await?;
        Ok(())
    }
}

pub type AlloyProviderConcreteImpl = FillProvider<JoinFill<Identity, JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>>, RootProvider>;
