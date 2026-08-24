use std::sync::Arc;
use tokio_postgres::Client;
use crate::db::pg::pg::Pg;
use crate::error::AppErr;

#[derive(Clone)]
pub struct Persistence {
    pg: Option<Arc<Client>>,
}


impl Persistence {
    pub async fn pg(url: &str) -> Result<Self,AppErr> {
        let pg = Pg::new(url).await;

        if pg.is_err() {
            return Err(AppErr::DbConnFailed(pg.err().unwrap().to_string()));
        }


        Ok(Self {
            pg: Some(pg.unwrap()),
        })
    }
}