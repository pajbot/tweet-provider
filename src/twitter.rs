#![allow(clippy::unnecessary_mut_passed)] // futures::select!

use crate::{config, Follows};
use anyhow::{Context, Result};
use egg_mode::{self as twitter, tweet::Tweet};
use futures::{
    future::{Fuse, FusedFuture},
    FutureExt, StreamExt,
};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    ops::Not,
    time::Duration,
};
use tokio::{
    sync::{broadcast, mpsc},
    time::{delay_for, timeout},
};

type RequestedFollows = HashMap<u64, HashSet<SocketAddr>>;

const NEW_FOLLOWS_RESTART_DELAY: Duration = Duration::from_secs(10);
const TWITTER_STALL: Duration = Duration::from_secs(90);

// starts the twitter stream
// restarts it when it goes down
// restarts it when there are new users to follow
pub async fn supervisor(
    config: config::Twitter,
    rx_requested_follows: mpsc::Receiver<(SocketAddr, Follows)>,
    tx_tweet: broadcast::Sender<Tweet>,
) -> Result<()> {
    // The follows requested and their subscribers
    let mut requested_follows = RequestedFollows::new();

    // Whether we are currently backing off
    let mut backing_off = false;
    // Backoff exponent
    let mut backoff = 0;

    // This is this the initial state, restart and twitter_stream are terminated,
    // the only live future is rx_requested_follows
    // This means that the only way for this select to pick up is for a client to subscribe
    // This state is reached again when no subscriptions remain
    let restart = Fuse::terminated();
    let twitter_stream = Fuse::terminated();
    let mut rx_requested_follows = rx_requested_follows.fuse();

    // pin to stack
    futures::pin_mut!(restart, twitter_stream);

    loop {
        futures::select! {
            // We were scheduled to restart, we verify that anyone is subscribed
            // and start a new stream
            () = restart => {
                backing_off = false;

                if requested_follows.is_empty() {
                    if twitter_stream.is_terminated().not() {
                        log::warn!("closing existing stream");
                        twitter_stream.set(Fuse::terminated());
                    }

                    log::info!("no follows were requested, let's wait some more");
                    continue;
                }

                let follows = requested_follows.keys().copied().collect();
                twitter_stream
                    .set(stream_consumer(config.token(), follows, tx_tweet.clone()).fuse());
            }

            // The stream has ended, we inspect the given error to know how much we should be
            // delaying the restart, and schedule said restart
            res = twitter_stream => {
                let error = res.err().context("infinite loop cannot return Ok(())")?;
                log::error!("twitter stream error: {:#}", error);

                let delay = inspect_error(error, &mut backoff);
                backing_off = true;

                log::info!("restarting in {:?}", delay);
                restart.set(delay_for(delay).fuse());
            }

            // A client has requested new follows, if any are new, we schedule a restart.
            // If a normal (not backing off) restart was already scheduled, we ignore it and
            // re-schedule to 10 seconds.
            msg = rx_requested_follows.next() => {
                let (addr, new_follows) = msg.context("no tx_requested_follows remaining")?;

                // We remove addr from subscriptions it doesn't want anymore
                for (follow, subscribers) in &mut requested_follows {
                    if new_follows.contains(&follow) {
                        continue;
                    }

                    subscribers.retain(|s| s != &addr);
                }

                // We drop follows that nobody wants anymore
                requested_follows.retain(|_, s| s.is_empty().not());

                let mut requires_restart = false;
                for follow in new_follows {
                    requires_restart = requires_restart || requested_follows.contains_key(&follow).not();

                    requested_follows
                        .entry(follow)
                        .or_insert_with(HashSet::new)
                        .insert(addr);
                }

                requires_restart = requires_restart
                    || (twitter_stream.is_terminated().not() && requested_follows.is_empty());

                if requires_restart && backing_off.not() {
                    log::info!("found new follows, restarting in {:?}", NEW_FOLLOWS_RESTART_DELAY);
                    if restart.is_terminated().not() {
                        log::info!("intercepted an existing scheduled restart");
                    }
                    restart.set(delay_for(NEW_FOLLOWS_RESTART_DELAY).fuse());
                }
            }

            complete => {
                anyhow::bail!("twitter::supervisor should never complete");
            }
        }
    }
}

fn inspect_error(error: anyhow::Error, backoff: &mut u32) -> Duration {
    // TODO: shouldn't need to downcast
    match error.downcast() {
        Ok(egg_mode::error::Error::BadStatus(status)) => {
            let secs = if status.as_u16() == 420 {
                log::warn!("reached twitter rate limit, blazin it for a while");

                (60 * 2u64.pow(*backoff)).min(960)
            } else {
                log::warn!("got bad status, slowing down");

                (5 * 2u64.pow(*backoff)).min(320)
            };

            *backoff += 1;

            Duration::from_secs(secs)
        }

        Ok(egg_mode::error::Error::NetError(_)) => {
            *backoff += 1;

            Duration::from_millis((250 * *backoff as u64).min(16_000))
        }

        // TODO: anything more we need to handle?
        _ => {
            *backoff = 0;

            Duration::from_millis(250)
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

    log::info!("starting a new twitter stream with follows: {:?}", follows);

    let mut stream = twitter::stream::filter().follow(&follows).start(&token);

    loop {
        let msg = timeout(TWITTER_STALL, stream.next()).await?; // timeout

        let msg = msg.context("twitter stream ran out")?;

        // TODO: read up on these errors
        let msg = msg?; // twitter error

        match msg {
            StreamMessage::Tweet(tweet) => {
                log::info!(
                    "got a tweet from {}: {:?}",
                    tweet.user.as_ref().unwrap().name,
                    tweet.text
                );

                if tx_tweet.send(tweet).is_err() {
                    log::debug!("no rx_tweet available");
                }
            }

            StreamMessage::Ping => {
                log::debug!("twitter ping");
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
