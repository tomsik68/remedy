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

    futures::future::try_join_all(handles).await.unwrap();
}
