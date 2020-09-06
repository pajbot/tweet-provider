#![recursion_limit = "1024"] // futures::select!

use anyhow::{Context, Result};
use config::Config;
use simple_logger::SimpleLogger;
use std::{collections::HashSet, sync::Arc};
use structopt::StructOpt;
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc, Notify},
};

mod api;
mod config;
mod twitter;
mod websocket;

type Follows = HashSet<u64>;

const REQUESTED_FOLLOWS_CHANNEL_CAPACITY: usize = 16;
const TWEET_CHANNEL_CAPACITY: usize = 16;

fn main() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let mut failure = false;

    if let Err(error) = rt.block_on(run()) {
        log::error!("fatal: {:#}", error);
        failure = true;
    }

    log::info!("waiting one second for tasks to end");
    rt.shutdown_timeout(std::time::Duration::from_secs(1));

    if failure {
        log::error!("error exit");
        std::process::exit(1);
    }

    log::info!("exiting");
}

async fn run() -> Result<()> {
    let mut args = config::Args::from_args();

    // https://github.com/clap-rs/clap/issues/1476
    args.config.twitter.always_restart |=
        std::env::var_os("PAJBOT_TWITTER_ALWAYS_RESTART").is_some();

    SimpleLogger::new().with_level(args.log_level).init()?;

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
    log::info!(
        "- websocket listen address: {}",
        config.websocket.listen_addr
    );
    log::info!(
        "- always restart twitter consumer: {}",
        config.twitter.always_restart
    );

    let (tx_requested_follows, rx_requested_follows) =
        mpsc::channel(REQUESTED_FOLLOWS_CHANNEL_CAPACITY);

    // TODO: change to watch::channel?
    // - attempt #1: ownership issues in twitter::supervisor
    let (tx_tweet, _) = broadcast::channel(TWEET_CHANNEL_CAPACITY);

    let lifeline = Arc::new(Notify::new());

    log::info!("starting");

    let websocket_listener = websocket::listener(
        TcpListener::bind(config.websocket.listen_addr).await?,
        tx_requested_follows,
        tx_tweet.clone(),
        &lifeline,
    );

    let twitter_supervisor = twitter::supervisor(config.twitter, rx_requested_follows, tx_tweet);

    tokio::select! {
        res = websocket_listener => {
            res.context("websocket listener stopped")?;
        }

        res = twitter_supervisor => {
            res.context("twitter supervisor stopped")?;
        }

        _ = lifeline.notified() => {
            log::info!("lifeline cut, shutting down");
        }

        _ = tokio::signal::ctrl_c() => {
            log::info!("interrupted, shutting down");
        }
    }

    Ok(())
}
