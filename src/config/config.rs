use crate::config::deserializers::deserialize_duration;
use std::time::Duration;
use serde::Deserialize;
use validator::Validate;
use crate::app::cmd::{AppMode, CmdArgs};
use crate::config::LogLevel;
use crate::error::CoreError;

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct Config {
    // this gets overridden from args
    #[serde(skip)]
    pub app_mode: AppMode,
    // runs http server to serve
    // requests for processed transactions
    pub enable_http_service: bool,

    /// http server for APIs of
    /// simulator and indexer
    pub http_bind_addr: String,

    pub db_pg_url: String,

    /// address of remote upstream to connect to
    /// for fetching data [sim and index mode]
    pub eth_ws_addr: String,
    #[serde(deserialize_with = "deserialize_duration")]
    pub eth_tx_fetch_rpc_timeout: Duration,

    /// [simulator] the block to start getting
    /// transactions from
    pub start_block_number: u64,
    pub log_level: LogLevel,

    /// number of blocks to fetch per each
    /// network requests.
    #[validate(range(min = 1))]
    pub block_retrieval_batch_size: usize,

    /// if enabled, it allows certain configs
    /// starting with test_* to take effect
    pub test_mode_enable: bool,

    /// [indexer :: test mode] stops fetching blocks when the specified
    /// amount is fetched
    pub test_mode_max_block_fetch_count: usize,

    /// [indexer :: test mode] if true, erases blocks data
    /// table to start afresh
    pub test_mode_erase_blocks_in_db: bool
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
            block_retrieval_batch_size: 0,
            test_mode_enable: false,
            test_mode_max_block_fetch_count: 5,
            test_mode_erase_blocks_in_db: false,
        }
    }
}

impl Config {
    pub fn load_from_file(file: String) -> Result<Config, CoreError> {
        let result = dotenvy::from_filename_override(file);

        if let Ok(..) = result {
            let c_res = envy::from_env::<Config>();
            if let Ok(c) = c_res {
                return Ok(c);
            }
            return Err(CoreError::ConfigLoadingFailed(c_res.err().unwrap().to_string()));
        }
        return Err(CoreError::ConfigLoadingFailed(result.err().unwrap().to_string()));
    }
}