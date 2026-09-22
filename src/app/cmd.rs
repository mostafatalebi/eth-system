use std::cmp::PartialEq;
use std::fmt::format;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use clap::{Error, Parser, ValueEnum};
use serde::Deserialize;
use tokio::time::{sleep_until, Instant};
use tracing::{error, info};
use crate::app::app::{App};
use crate::db::storage::{AppStorage, InMemoryStorage};
use crate::error::CoreError;
use crate::http_server::routes::get_router;
use crate::http_server::server::HttpServer;

pub struct Cmd {
    args: CmdArgs
}

impl Cmd {
    pub fn new() -> Result<Cmd, Error> {
        let args = CmdArgs::try_parse();

        if args.is_ok() {
            return Ok(Self{
                args: args?
            });
        }
        Err(args.err().unwrap())
    }

    pub async fn init(&self) -> Result<Option<Arc<App<impl AppStorage>>>, CoreError> {
        let app_storage = InMemoryStorage::new();
        let app = App::new_from_args(&self.args, app_storage);
        if app.is_ok() {
            let mut app = app?;

            let app_res = app.init().await;
            if app_res.is_err() {
                return Err(app_res.err().unwrap())
            }
            let app_arc = Arc::new(app);

            return Ok(Some(app_arc));
        } else {
            return Err(app.err().unwrap())
        }
    }

    /// executes the corresponding entry command
    /// based on the given input arg
    pub async fn relay(&self, app: Arc<App<impl AppStorage>>) -> Result<(), CoreError> {
        match self.args.mode {
            AppMode::Simulator => {
                let ws = WsConnect::new(app.config.eth_ws_addr.clone());
                let provider = ProviderBuilder::new().connect_ws(ws).await;
                if provider.is_err() {
                    return Err(CoreError::SubscriptionError(provider.err().unwrap().to_string()))
                }
                if app.config.enable_http_service {
                    _ = self.run_http_server(app.clone());
                }
                app.run_eth_simulator().await?;
            },
            AppMode::Indexer => {

            },
            AppMode::Migration => {
                app.migrate().await?;
            },
        }
        Ok(())
    }

    pub fn run_http_server(&self, app: Arc<App<impl AppStorage+'static>>) {
        _ = tokio::spawn(async move {
            let mut http_server = HttpServer::new(app.config.http_bind_addr.to_string(), get_router(app.clone()));
            http_server.with_app(app.clone());
            info!("starting http server on {:?}", app.config.http_bind_addr);
            let e = http_server.run().await;
            if e.is_err() {
                error!("failed to start http server: bind_addr={:?} error={:?}", app.config.http_bind_addr, e.err().unwrap().to_string())
            }
        });
    }



    pub fn is_mode_server_http(&self) -> bool {
        self.args.mode == AppMode::Indexer
    }
    pub fn is_mode_migration(&self) -> bool {
        self.args.mode == AppMode::Migration
    }


}

#[derive(Deserialize, Default, ValueEnum, Debug, Clone)]
pub enum AppMode {
    #[default]
    Simulator,
    Indexer,
    Migration,
}

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct CmdArgs {
    #[arg(long)]
    pub mode: AppMode,
    #[arg(long)]
    pub config: String,
}

impl FromStr for AppMode {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, CoreError> {
        if s == "migration" {
            return Ok(AppMode::Migration)
        } else if s == "server-http" {
            return Ok(AppMode::Indexer)
        }
        return Err(CoreError::BadArgument(s.to_string()))
    }
}

impl PartialEq for AppMode {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (AppMode::Simulator, AppMode::Simulator) | (AppMode::Indexer, AppMode::Indexer) | (AppMode::Migration, AppMode::Migration))
    }
}