// for futures::select!
#![recursion_limit = "1024"]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// #![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::upper_case_acronyms)]

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, LogTimestamps};
use simple_logger::SimpleLogger;
use std::{collections::HashSet, sync::Arc};
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
    let rt = tokio::runtime::Runtime::new().unwrap();
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
    let args = config::Args::parse();

    if std::env::var_os("TWEET_PROVIDER_DUMP_ARGS_AND_EXIT").is_some() {
        println!("{:#?}", args);
        return Ok(());
    }

    let mut logger = SimpleLogger::new().with_level(args.log_level);
    logger = match args.log_timestamps {
        LogTimestamps::Local => logger.with_local_timestamps(),
        LogTimestamps::UTC => logger.with_utc_timestamps(),
        LogTimestamps::Off => logger.without_timestamps(),
    };
    logger.init()?;

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

    if std::env::var_os("TWEET_PROVIDER_DUMP_CONFIG_AND_EXIT").is_some() {
        println!("{:#?}", config);
        return Ok(());
    }

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
