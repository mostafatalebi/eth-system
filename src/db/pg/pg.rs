use std::sync::Arc;
use deadpool_postgres::{PoolConfig, Runtime};
use tokio_postgres::{Client, NoTls};
use crate::error::CoreError;

pub struct Pg {}



impl Pg {
    pub async fn new(url: &str) -> Result<deadpool_postgres::Pool, CoreError> {
        let mut config = deadpool_postgres::Config::new();
        config.url = Some(url.to_string());
        config.pool = Some(PoolConfig{
            max_size: 8,
            timeouts: Default::default(),
            queue_mode: Default::default(),
        });

        let res = config.create_pool(Some(Runtime::Tokio1), NoTls);
        if res.is_err() {
            return Err(CoreError::DbConnFailed(res.err().unwrap().to_string()));
        }
        return Ok(res.unwrap())
        // let (client, connection) = tokio_postgres::connect(&url, NoTls).await?;
        //
        // tokio::spawn(async move {
        //     if let Err(e) = connection.await {
        //         eprintln!("connection error: {}", e);
        //     }
        // });
        //
        // Ok(Arc::new(client))
    }
}