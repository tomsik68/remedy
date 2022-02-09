//! Remedy is a multi-threaded rust-imap-maildir synchronization program.
//!
//! Please note that remedy is under heavy development.
//!
//! Current features:
//! - IMAP
//! - TLS
//! - maildir
//! - configurable via toml (see `config.example.toml`)
//! - multiple accounts
//! - basic logging so it's possible to see what's going on
//!
//! Missing features:
//! - save local state to prevent synchronization of already downloaded e-mails
//! - other formats

mod config;
mod getmail;

use config::Config;

#[tokio::main]
async fn main() {
    env_logger::init();
    // TODO move config elsewhere
    let config = Config::read_from("config.toml").expect("failed to load config");
    let handles: Vec<_> = config
        .accounts
        .into_iter()
        .map(|acc| tokio::spawn(async move { getmail::get(acc).await }))
        .collect();

    futures::future::try_join_all(handles).await.unwrap();
}
