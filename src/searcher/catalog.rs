use std::sync::Arc;
use crate::db::db::DbSelector;

pub struct Catalog {
}


impl Catalog {
    pub fn new(db_sel: Option<Arc<DbSelector>>) -> Catalog {
        Self {
            db: db_sel,
        }
    }
}