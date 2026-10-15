use crate::config::deserializers::deserialize_duration;
use std::time::Duration;
use serde::Deserialize;
use validator::Validate;
use crate::app::cmd::{AppMode, CmdArgs};
use crate::config::LogLevel;
use crate::error::CoreError;

/// placeholder secret used when jwt_secret is not configured; it is public, so anyone
/// can forge tokens signed with it. Always set your own jwt_secret outside development
pub const DEFAULT_JWT_SECRET: &str = "dummy-jwt-secret-change-me-in-production";

fn default_jwt_secret() -> String {
    DEFAULT_JWT_SECRET.to_string()
}

fn default_user_roles() -> Vec<String> {
    vec!["admin".to_string()]
}

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

    /// secret used to sign and verify JWTs (HS256); at least 32 characters
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    /// lifetime of an issued access token (JWT)
    #[serde(deserialize_with = "deserialize_duration")]
    pub access_token_expiry: Duration,

    /// lifetime of an issued refresh token
    #[serde(deserialize_with = "deserialize_duration")]
    pub refresh_token_expiry: Duration,

    /// when disabled, POST /api/auth/signup is not served; users can still
    /// be created with --mode create-user
    pub enable_http_signup: bool,

    /// when enabled, --mode migration creates the default user below, but only
    /// while the users table is still empty
    #[serde(default)]
    pub default_user_enabled: bool,
    #[serde(default)]
    pub default_user_username: String,
    #[serde(default)]
    pub default_user_password: String,
    /// comma separated roles of the default user
    #[serde(default = "default_user_roles")]
    pub default_user_roles: Vec<String>,

    /// address of remote upstream to connect to
    /// for fetching data [sim and index mode]
    pub eth_ws_addr: String,
    pub block_upstream_http_addr: String,
    pub beacon_upstream_http_addr: String,

    #[serde(deserialize_with = "deserialize_duration")]
    pub eth_tx_fetch_rpc_timeout: Duration,

    /// [simulator] the block to start getting
    /// transactions from
    pub start_block_number: u64,
    pub log_level: LogLevel,

    /// number of blocks to fetch per each
    /// network requests.
    #[validate(range(min = 1))]
    pub block_retrieval_batch_size: u64,

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
            jwt_secret: default_jwt_secret(),
            access_token_expiry: Duration::from_secs(900),
            refresh_token_expiry: Duration::from_secs(7 * 24 * 3600),
            enable_http_signup: false,
            default_user_enabled: false,
            default_user_username: "".to_string(),
            default_user_password: "".to_string(),
            default_user_roles: default_user_roles(),
            eth_ws_addr: "127.0.0.1:5439".to_string(),
            block_upstream_http_addr: "".to_string(),
            beacon_upstream_http_addr: "".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    // example.env ships the same dummy jwt_secret as the built-in default
    #[test]
    fn test_example_env_uses_the_default_jwt_secret() {
        let line = include_str!("../../example.env").lines().find(|l| l.starts_with("jwt_secret=")).unwrap();

        assert_eq!(line, format!("jwt_secret={DEFAULT_JWT_SECRET}"));
        assert_eq!(Config::default().jwt_secret, DEFAULT_JWT_SECRET);
    }

    fn base_env() -> Vec<(String, String)> {
        [
            ("ENABLE_HTTP_SERVICE", "true"), ("HTTP_BIND_ADDR", "0.0.0.0:8080"), ("DB_PG_URL", ""),
            ("ACCESS_TOKEN_EXPIRY", "15m"), ("REFRESH_TOKEN_EXPIRY", "7d"), ("ENABLE_HTTP_SIGNUP", "true"),
            ("ETH_WS_ADDR", ""), ("BLOCK_UPSTREAM_HTTP_ADDR", ""), ("BEACON_UPSTREAM_HTTP_ADDR", ""),
            ("ETH_TX_FETCH_RPC_TIMEOUT", "1s"), ("START_BLOCK_NUMBER", "0"), ("LOG_LEVEL", "debug"),
            ("BLOCK_RETRIEVAL_BATCH_SIZE", "1"), ("TEST_MODE_ENABLE", "false"),
            ("TEST_MODE_MAX_BLOCK_FETCH_COUNT", "1"), ("TEST_MODE_ERASE_BLOCKS_IN_DB", "false"),
        ].map(|(k, v)| (k.to_string(), v.to_string())).to_vec()
    }

    // a config without jwt_secret falls back to the dummy default
    #[test]
    fn test_missing_jwt_secret_falls_back_to_the_default() {
        let config: Config = envy::from_iter(base_env()).unwrap();

        assert_eq!(config.jwt_secret, DEFAULT_JWT_SECRET);
    }

    // the default user is off unless configured, and its roles are read as a comma separated list
    #[test]
    fn test_default_user_config() {
        let unset: Config = envy::from_iter(base_env()).unwrap();
        let mut env = base_env();
        env.extend([
            ("DEFAULT_USER_ENABLED", "true"), ("DEFAULT_USER_USERNAME", "root"),
            ("DEFAULT_USER_PASSWORD", "a-long-password"), ("DEFAULT_USER_ROLES", "admin,user"),
        ].map(|(k, v)| (k.to_string(), v.to_string())));
        let configured: Config = envy::from_iter(env).unwrap();

        assert!(!unset.default_user_enabled);
        assert_eq!(unset.default_user_roles, vec!["admin".to_string()]);
        assert!(configured.default_user_enabled);
        assert_eq!(configured.default_user_username, "root");
        assert_eq!(configured.default_user_roles, vec!["admin".to_string(), "user".to_string()]);
    }
}
