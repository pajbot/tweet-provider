#![allow(clippy::unnecessary_mut_passed)] // futures::select!

use crate::{config, Follows};
use anyhow::{Context, Result};
use egg_mode::{self as twitter, tweet::Tweet};
use futures::{future::Fuse, FutureExt, StreamExt};
use std::{ops::Not, path::Path, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    time::{delay_for, timeout},
};

const TWITTER_STALL: Duration = Duration::from_secs(90);

// starts the twitter stream,
// restarts it when it goes down
// restarts it when there are new users to follow
pub async fn supervisor(
    config: config::Twitter,
    rx_requested_follows: mpsc::Receiver<Follows>,
    tx_tweet: broadcast::Sender<Tweet>,
) {
    let default_follows = config.default_follows.iter().copied().collect();

    // TODO: change to LRU to prevent going over 5000 follows
    let mut requested_follows = Follows::new();

    let mut backing_off = false;
    let mut backoff = 0;

    // We don't start immediately.
    let restart = Fuse::terminated();
    let twitter_stream = Fuse::terminated();
    let mut rx_requested_follows = rx_requested_follows.fuse();

    // pin to stack
    futures::pin_mut!(restart, twitter_stream);

    loop {
        futures::select! {
            () = restart => {
                let follows : Vec<u64> = requested_follows.union(&default_follows).copied().collect();
                if !follows.is_empty() {
                    twitter_stream.set(stream_consumer(config.token(), follows, tx_tweet.clone()).fuse());

                    backing_off = false;
                } else {
                    log::warn!("not starting stream, nothing to follow yet");
                    restart.set(delay_for(Duration::from_secs(5)).fuse());
                }
            }

            res = twitter_stream => {
                backing_off = true;

                let error = res.expect_err("infinite loop cannot return Ok(())");
                log::error!("twitter stream error: {:#}", error);

                // TODO: shouldn't need to downcast
                let delay = match error.downcast() {
                    Ok(egg_mode::error::Error::BadStatus(status)) => {
                        let secs = if status.as_u16() == 420 {
                            log::warn!("reached twitter rate limit, blazin it for a while");

                            (60 * 2u64.pow(backoff)).min(960)
                        } else {
                            log::warn!("got bad status, slowing down");

                            (5 * 2u64.pow(backoff)).min(320)
                        };

                        backoff += 1;

                        Duration::from_secs(secs)
                    }

                    // TODO: anything more we need to handle?
                    _ => {
                        backoff = 0;

                        Duration::from_millis((250 * backoff as u64).min(16_000))
                    }
                };

                log::info!("restarting in {:?}", delay);
                restart.set(delay_for(delay).fuse());
            }

            new_follows = rx_requested_follows.next() => {
                // PANIC: fatal if all tx_requested_follows have dropped.
                // extremely unlikely, a panic is fine for now
                let new_follows = new_follows.expect("no tx_requested_follows remaining");

                if requested_follows.is_superset(&new_follows).not() {
                    log::info!("new set of requested follows: {:?}", new_follows);
                    requested_follows.extend(new_follows);

                    let res = write_cache(&config.follows_cache, &requested_follows).await;
                    if let Err(error) = res {
                        log::error!(
                            "writing to cache {:?} failed: {:#}",
                            config.follows_cache,
                            error
                        );
                    }

                    if backing_off.not() {
                        restart.set(delay_for(Duration::from_secs(10)).fuse());
                    }
                }
            }
        }
    }
}

async fn stream_consumer(
    token: twitter::Token,
    follows: Vec<u64>,
    tx_tweet: broadcast::Sender<Tweet>,
) -> Result<()> {
    use twitter::stream::StreamMessage;

    // follows must not be empty, asserted during init
    // follows must not exceed 5000 items, TODO
    anyhow::ensure!(
        (1..=5000).contains(&follows.len()),
        "follows must not be empty and must not exceed 5000 items (currently {})",
        follows.len()
    );

    log::info!("starting a new twitter stream: {:?}", follows);

    let mut stream = twitter::stream::filter().follow(&follows).start(&token);

    loop {
        let msg = timeout(TWITTER_STALL, stream.next()).await?; // timeout

        let msg = msg.context("twitter stream ran out")?;

        // TODO: read up on these errors
        let msg = msg?; // twitter error

        match msg {
            StreamMessage::Tweet(tweet) => {
                log::debug!(
                    "got a tweet from {}: {:?}",
                    tweet.user.as_ref().unwrap().name,
                    tweet.text
                );

                if tx_tweet.send(tweet).is_err() {
                    log::debug!("no rx_tweet available");
                }
            }

            StreamMessage::Disconnect(_, desc) => {
                // TODO: do we need to do anything? just run the stream out to be sure
                log::warn!("twitter sent disconnect: {}", desc);
            }

            msg => {
                // TODO: are we supposed to care about any of these?
                log::info!("unknown twitter stuff: {:#?}", msg);
            }
        }
    }
}

pub async fn read_cache(path: impl AsRef<Path>) -> Result<Follows> {
    Ok(serde_json::from_str(
        &tokio::fs::read_to_string(path).await?,
    )?)
}

pub async fn write_cache(path: impl AsRef<Path>, follows: &Follows) -> Result<()> {
    Ok(tokio::fs::write(path, serde_json::to_string(&follows)?).await?)
}
