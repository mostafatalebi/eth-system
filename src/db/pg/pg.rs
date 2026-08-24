use std::sync::Arc;
use tokio_postgres::{Client, NoTls};

pub struct Pg {}



impl Pg {
    pub async fn new(url: &str) -> Result<Arc<Client>, tokio_postgres::Error> {
        let (client, connection) = tokio_postgres::connect(&url, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        Ok(Arc::new(client))
    }
}