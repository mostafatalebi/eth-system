use std::sync::{Arc, Mutex};
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::models::log::LogModel;
use crate::indexer::repository::repository::{BlockData, BlockRepository, LogFilter};


#[derive(Clone)]
pub struct MockRepository {
    pub inner: Arc<Mutex<MockRepositoryInner>>,
}



pub struct MockRepositoryInner {
    blocks: Vec<BlockModel>,
    logs: Vec<LogModel>,
    expected_upsert_error: Option<CoreError>,
}


impl MockRepository {
    pub fn new() -> MockRepository {
        Self {
            inner: Arc::new(Mutex::new(MockRepositoryInner{
            blocks: vec![],
            logs: vec![],
            expected_upsert_error: None})),
        }
    }

    pub fn expect_upsert_error(&mut self, err: CoreError) {
        self.inner.lock().unwrap().expected_upsert_error = Some(err);
    }

    pub fn len_blocks(&self) -> usize {
        self.inner.lock().unwrap().blocks.len()
    }

    pub fn add_blocks(&self, blocks: Vec<BlockModel>) {
        self.inner.lock().unwrap().blocks.extend(blocks);
    }

    /// what the foreign key cascade does in the database
    fn drop_logs_of_missing_blocks(inner: &mut MockRepositoryInner) {
        let hashes: Vec<Vec<u8>> = inner.blocks.iter().map(|b| b.hash.clone()).collect();
        inner.logs.retain(|l| hashes.contains(&l.block_hash));
    }

    pub fn add_logs(&self, logs: Vec<LogModel>) {
        self.inner.lock().unwrap().logs.extend(logs);
    }

    pub fn logs(&self) -> Vec<LogModel> {
        self.inner.lock().unwrap().logs.clone()
    }

    pub fn block_hashes(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().blocks.iter().map(|b| b.hash.clone()).collect()
    }
}


impl BlockRepository for MockRepository {
    async fn upsert_blocks(&self, blocks: &BlockData<'_>) -> Result<(), CoreError> {
        if self.inner.lock().unwrap().expected_upsert_error.is_some() {
            return Err(self.inner.lock().unwrap().expected_upsert_error.take().unwrap());
        }
        let mut inner = self.inner.lock().unwrap();
        for (i, b) in blocks.blocks.iter().enumerate() {
            inner.blocks.push(b.clone());
            inner.logs.extend(blocks.logs[i].iter().cloned());
        }
        Ok(())
    }

    async fn delete_block_cascading(&self, oldest_block: Vec<u8>, newest_block: Vec<u8>) -> Result<(), CoreError> {
        let mut inner = self.inner.lock().unwrap();
        let number_of = |hash: &Vec<u8>| inner.blocks.iter().find(|b| &b.hash == hash).map(|b| b.number)
            .ok_or(CoreError::DbNotFound(format!("block with hash 0x{}", alloy::hex::encode(hash))));
        let oldest_number = number_of(&oldest_block)?;
        let newest_number = number_of(&newest_block)?;
        // tolerate swapped arguments: always delete the range between the two
        let (from_number, to_number) = (oldest_number.min(newest_number), oldest_number.max(newest_number));
        inner.blocks.retain(|b| b.number < from_number || b.number > to_number);
        Self::drop_logs_of_missing_blocks(&mut inner);
        Ok(())
    }

    async fn last_block_number(&self) -> Result<Option<u64>, CoreError> {
        Ok(self.inner.lock().unwrap().blocks.iter().map(|b| b.number as u64).max())
    }

    async fn block_exists(&self, _number: i64, _hash: &Vec<u8>, _parent_hash: &Vec<u8>) -> Result<bool, CoreError> {
        Ok(true)
    }

    async fn get_block(&self, hash: Option<Vec<u8>>, number: Option<i64>) -> Result<Option<BlockModel>, CoreError> {
        let inner = self.inner.lock().unwrap();
        let found = inner.blocks.iter().find(|b| {
            hash.as_ref().map_or(true, |h| &b.hash == h) && number.map_or(true, |n| b.number == n)
        });
        Ok(found.cloned())
    }
    async fn list_blocks(&self, limit: i64, offset: i64) -> Result<Vec<BlockModel>, CoreError> {
        let mut blocks = self.inner.lock().unwrap().blocks.clone();
        blocks.sort_by(|a, b| b.number.cmp(&a.number));
        Ok(blocks.into_iter().skip(offset as usize).take(limit as usize).collect())
    }
    async fn list_logs(&self, filter: &LogFilter, limit: i64, offset: i64) -> Result<Vec<LogModel>, CoreError> {
        let mut logs: Vec<LogModel> = self.inner.lock().unwrap().logs.iter().filter(|l| {
            let topics = [&l.topic0, &l.topic1, &l.topic2, &l.topic3];
            filter.address.as_ref().map_or(true, |a| &l.address == a)
                && filter.topics.iter().zip(topics).all(|(wanted, actual)| wanted.is_none() || wanted == actual)
                && filter.from_block.map_or(true, |from| l.block_number >= from)
                && filter.to_block.map_or(true, |to| l.block_number <= to)
        }).cloned().collect();
        logs.sort_by(|a, b| (b.block_number, b.log_index).cmp(&(a.block_number, a.log_index)));
        Ok(logs.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn replace_chain_above(&self, ancestor_number: i64, new_chain: &BlockData<'_>) -> Result<(), CoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.blocks.retain(|b| b.number <= ancestor_number);
        Self::drop_logs_of_missing_blocks(&mut inner);
        for (i, b) in new_chain.blocks.iter().enumerate() {
            inner.blocks.push(b.clone());
            inner.logs.extend(new_chain.logs[i].iter().cloned());
        }
        Ok(())
    }

    async fn mark_finalized_checkpoint(&self, hash: Vec<u8>) -> Result<bool, CoreError> {
        Ok(self.inner.lock().unwrap().blocks.iter().any(|b| b.hash == hash))
    }

    async fn is_block_finalized(&self, _number: i64) -> Result<bool, CoreError> {
        Ok(false)
    }

    async fn last_finalized_checkpoint(&self) -> Result<Option<i64>, CoreError> {
        Ok(None)
    }

    async fn get_block_by_parent_hash(&self, _p_hash: Vec<u8>) -> Result<Option<BlockModel>, CoreError> {
        Ok(None)
    }
}
