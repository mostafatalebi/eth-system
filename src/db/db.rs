use std::sync::Arc;
use deadpool_postgres::Object;
use tokio_postgres::Client;
use crate::db::pg::pg::Pg;
use crate::error::CoreError;

pub enum DbType {
    Postgres,
}

#[derive(Clone, Default)]
pub struct DbFactory {
    // @todo must refactor to an abstract client, not per-impl
    inner_pg: Option<deadpool_postgres::Pool>,
}


impl DbFactory {
    pub async fn connect(url: &str, db_type: DbType) -> Result<Self, CoreError> {
        match db_type {
            DbType::Postgres => {
                let pg = Pg::new(url).await;
                if pg.is_err() {
                    return Err(CoreError::DbConnFailed(pg.err().unwrap().to_string()));
                }
                Ok(Self {
                    inner_pg: Some(pg.unwrap()),
                })
            }
        }
    }


    pub async fn pg(&self) -> Result<Object, CoreError> {
        let res = self.inner_pg.as_ref().unwrap().get().await;
        if res.is_err() {
            return Err(CoreError::DbGetClientFailed(res.err().unwrap().to_string()));
        }
        return Ok(res.unwrap());
    }
}