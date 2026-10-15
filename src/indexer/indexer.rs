use alloy::rpc::types::Block;
use tokio::join;
use tokio::time::{sleep};
use tracing::error;
use crate::error::CoreError;
use crate::indexer::clients::eth::EthClient;
use crate::indexer::fetcher::Fetcher;
use crate::indexer::repository::repository::BlockRepository;

pub struct Indexer<B: BlockRepository, C: EthClient> {
    fetcher: Fetcher<B, C>,
}


impl<B: BlockRepository, C: EthClient> Indexer<B, C> {
    pub fn new(fetcher: Fetcher<B, C>) -> Indexer<B, C> {
        Self{
            fetcher,
        }
    }

    pub async fn start_workers(&mut self, start_block_number: Option<u64>, out_chan: tokio::sync::mpsc::Sender<Block>, in_chan: tokio::sync::mpsc::Receiver<Block>) {
        _ = join!(
            self.fetcher.inner.block_processor(start_block_number, in_chan),
            self.rpc_workers(start_block_number, out_chan),
        );
    }
    pub async fn rpc_workers(&self, start_block_number: Option<u64>,out_chan: tokio::sync::mpsc::Sender<Block>) -> Result<(), CoreError> {
        let mut retry_sec = 1u64;
        let interval_stop_dur = 1u64;
        let mut start_block_number = start_block_number;
        loop {
            // @todo we need to get last block number from outside of get_next_block_batch()
            let res = self.fetcher.inner.block_fetcher(start_block_number, out_chan.clone()).await;
            if res.is_err() {
                error!("Error in getting batch operation. Retry in {:} second(s) (error{:})", retry_sec, res.err().unwrap());
                sleep(core::time::Duration::from_secs(retry_sec)).await;
                retry_sec += 1;
                continue
            }
            start_block_number = Some(res.unwrap());
            sleep(core::time::Duration::from_secs(interval_stop_dur)).await;
        }
    }

}