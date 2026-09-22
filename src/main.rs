
use tracing_subscriber::EnvFilter;
use app::cmd::{Cmd};

pub mod subscriber;
pub mod db;
pub mod http_server;
pub mod app;
pub mod config;
pub mod error;
pub mod executor;
pub mod common;
mod mem;
mod indexer;
pub mod migrations;

#[tokio::main]
async fn main() {
    let cmd_args = Cmd::new();

    if cmd_args.is_err() {
        panic!("{}", cmd_args.err().unwrap());
    }
    let cmd_args = cmd_args.unwrap();
    let app = cmd_args.init().await;

    if app.is_err() {
        panic!("{}", app.err().unwrap());
    }
    let app = app.unwrap().unwrap();

    tracing_subscriber::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_max_level(app.config.log_level.to_tracing_crate())
        .init();


    if let Err(e) = cmd_args.relay(app).await {
        panic!("Exit: {}", e);
    }
}
