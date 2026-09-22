use alloy::rpc::types::Block;
use tokio::join;
use tokio::time::{sleep};
use tracing::error;
use crate::indexer::client::eth::EthClient;
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

    pub async fn start_workers(&mut self, out_chan: tokio::sync::mpsc::Sender<Block>, in_chan: tokio::sync::mpsc::Receiver<Block>) {
        _ = join!(
            self.fetcher.inner.listen_to_incoming_blocks(in_chan),
            self.rpc_workers(out_chan),
        );
    }
    pub async fn rpc_workers(&self, out_chan: tokio::sync::mpsc::Sender<Block>) {
        let mut retry_sec = 1u64;
        let interval_stop_dur = 1u64;

        loop {
            // @todo we need to get last block number from outside of get_next_block_batch()
            let res = self.fetcher.inner.get_next_block_batch(None, out_chan.clone()).await;
            if res.is_err() {
                error!("Error in getting batch operation. Retry in {:} second(s) (error{:})", retry_sec, res.err().unwrap());
                sleep(core::time::Duration::from_secs(retry_sec)).await;
                retry_sec += 1;
                continue
            }
            sleep(core::time::Duration::from_secs(interval_stop_dur)).await;
        }
    }

}