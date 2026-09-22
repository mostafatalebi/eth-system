use std::ops::Deref;
use std::sync::{Arc, Mutex};
use alloy::rpc::types::Block;
use revm::bytecode::bitvec::ptr::Mut;
use crate::error::CoreError;
use crate::indexer::models::block::BlockModel;
use crate::indexer::repository::repository::BlockRepository;


#[derive(Clone)]
pub struct MockRepository {
    pub inner: Arc<Mutex<MockRepositoryInner>>,
}



pub struct MockRepositoryInner {
    block_number: u64,
    blocks: Vec<BlockModel>,
    expected_upsert_error: Option<CoreError>,
}


impl MockRepository {
    pub fn new() -> MockRepository {
        Self {
            inner: Arc::new(Mutex::new(MockRepositoryInner{block_number: 0,
            blocks: vec![],
            expected_upsert_error: None})),
        }
    }

    pub fn expect_upsert_error(&mut self, err: CoreError) {
        self.inner.lock().unwrap().expected_upsert_error = Some(err);
    }

    pub fn len_blocks(&self) -> usize {
        self.inner.lock().unwrap().blocks.len()
    }
}


impl BlockRepository for MockRepository {
    async fn upsert_blocks(&self, blocks: &Vec<BlockModel>) -> Result<(), CoreError> {
        if self.inner.lock().unwrap().expected_upsert_error.is_some() {
            return Err(self.inner.lock().unwrap().expected_upsert_error.take().unwrap());
        }
        for b in blocks {
            self.inner.lock().unwrap().blocks.push(b.clone())
        }
        Ok(())
    }

    async fn delete_block(&self) -> Result<(), CoreError> {
        todo!()
    }

    async fn last_block_number(&self) -> Result<u64, CoreError> {
        todo!()
    }
}