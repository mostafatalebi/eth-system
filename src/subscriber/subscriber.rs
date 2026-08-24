use std::time::Duration;
use alloy::network::{Ethereum};
use alloy::providers::{Provider};
use alloy::rpc::types::Transaction;
use tracing::{debug, error, info};
use crate::error::AppErr;

pub struct SubscriptionConfig {
    pub timeout: Duration,
    pub ws_url: String,
    pub ch: tokio::sync::mpsc::Sender<Transaction>
}

pub struct Subscriber {

}


// @todo we need to make Subscriber to listen to various statuses of transactions
impl Subscriber {
    pub async fn start_listening(provider: impl Provider<Ethereum>+Clone, ic: &SubscriptionConfig) -> Result<(), AppErr> {
        info!("starting subscriber...");
        let sub = provider.subscribe_pending_transactions().await;
        if sub.is_err() {
            return Err(AppErr::SubscriptionError(sub.err().unwrap().to_string()))
        }
        let mut stream = sub.unwrap().into_stream();

        while let Some(tx) = futures_util::StreamExt::next(&mut stream).await {
            debug!(%tx,"received a new transaction");
            let transaction = provider.get_transaction_by_hash(tx).await;
            if transaction.is_err() {
                error!(%tx, ?transaction, "fetching transaction failed");
            }
            let transaction = transaction.unwrap().unwrap();
            let res = ic.ch.send(transaction).await;
            if res.is_err() {
                error!("publishing fetched transaction failed: {:?}", res.err().unwrap().to_string());
            }

        }
        info!("subscriber stopped");
        Ok(())
    }
}