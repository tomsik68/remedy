extern crate env_logger;
#[macro_use]
extern crate log;
extern crate imap;
extern crate native_tls;
extern crate serde;
#[macro_use]
extern crate quick_error;

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
        .map(|acc| {
            tokio::spawn(async move {
                getmail::get(acc).await;
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }
}
