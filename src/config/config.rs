use crate::config::deserializers::deserialize_duration;
use std::time::Duration;
use serde::Deserialize;
use crate::app::cmd::{AppMode, CmdArgs};
use crate::config::LogLevel;
use crate::error::AppErr;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    // this gets overridden from args
    #[serde(skip)]
    pub app_mode: AppMode,
    // runs http server to serve
    // requests for processed transactions
    pub enable_http_service: bool,
    pub http_bind_addr: String,
    pub db_pg_url: String,
    pub eth_ws_addr: String,
    #[serde(deserialize_with = "deserialize_duration")]
    pub eth_tx_fetch_rpc_timeout: Duration,
    pub start_block_number: u64,
    pub log_level: LogLevel
}


impl Config {
    pub fn init(&mut self, args: &CmdArgs) {
        self.app_mode = args.mode.clone();
    }
}

impl Default for Config {
    fn default() -> Config {
        Self {
            app_mode: AppMode::Simulator,
            enable_http_service: true,
            http_bind_addr: "0.0.0.0:8080".to_string(),
            db_pg_url: "".to_string(),
            eth_ws_addr: "127.0.0.1:5439".to_string(),
            eth_tx_fetch_rpc_timeout: Duration::from_secs(1),
            start_block_number: 0,
            log_level: LogLevel::Debug,
        }
    }
}

impl Config {
    pub fn load_from_file(file: String) -> Result<Config, AppErr> {
        let result = dotenvy::from_filename_override(file);

        if let Ok(..) = result {
            let c_res = envy::from_env::<Config>();
            if let Ok(c) = c_res {
                return Ok(c);
            }
            return Err(AppErr::ConfigLoadingFailed(c_res.err().unwrap().to_string()));
        }
        return Err(AppErr::ConfigLoadingFailed(result.err().unwrap().to_string()));
    }
}