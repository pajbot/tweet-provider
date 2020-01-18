#![recursion_limit = "1024"] // futures::select!

use anyhow::Result;
use config::Config;
use std::collections::HashSet;
use structopt::StructOpt;
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};

mod api;
mod config;
mod twitter;
mod websocket;

type Follows = HashSet<u64>;

const REQUESTED_FOLLOWS_CHANNEL_CAPACITY: usize = 16;
const TWEET_CHANNEL_CAPACITY: usize = 16;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        log::error!("fatal: {:#}", error);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = config::Args::from_args();

    simple_logger::init_with_level(args.log_level)?;

    log::info!("initializing");

    let config = match Config::from_toml(&args.config_path).await {
        Ok(config) => args.config.merge(config),
        Err(error) => {
            log::warn!(
                "reading config from {:?} failed: {:#}",
                args.config_path,
                error
            );
            args.config
        }
    };

    anyhow::ensure!(
        config.twitter.consumer_key.is_some()
            && config.twitter.consumer_secret.is_some()
            && config.twitter.access_token.is_some()
            && config.twitter.access_token_secret.is_some(),
        "secrets in twitter config must be configured"
    );

    log::info!("config has been loaded:");
    log::info!("- listen address: {}", config.websocket.listen_addr);
    log::info!("- default follows: {:?}", config.twitter.default_follows);
    log::info!("- follows cache: {:?}", config.twitter.follows_cache);

    let (mut tx_requested_follows, rx_requested_follows) =
        mpsc::channel(REQUESTED_FOLLOWS_CHANNEL_CAPACITY);

    // TODO: change to watch::channel?
    // - attempt #1: ownership issues in twitter::supervisor
    let (tx_tweet, _) = broadcast::channel(TWEET_CHANNEL_CAPACITY);

    match twitter::read_cache(&config.twitter.follows_cache).await {
        Ok(cache) => tx_requested_follows.send(cache).await?,
        Err(error) => log::warn!(
            "reading cache from {:?} failed: {:#}",
            config.twitter.follows_cache,
            error
        ),
    }

    log::info!("starting");

    // TODO: replace tokio::spawn with a select, allowing us to return errors

    // `TcpListener::bind` happens here because it is fatal,
    // moving it in `websocket::listener` would force us to panic
    tokio::spawn(websocket::listener(
        TcpListener::bind(config.websocket.listen_addr).await?,
        tx_requested_follows,
        tx_tweet.clone(),
    ));

    tokio::spawn(twitter::supervisor(
        config.twitter,
        rx_requested_follows,
        tx_tweet,
    ));

    tokio::signal::ctrl_c().await?;
    println!(/* most terms will show a ^C with no newline */);
    log::info!("interrupted, exiting");

    Ok(())
}
