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
    time::{sleep, timeout},
};

type RequestedFollows = HashMap<u64, HashSet<SocketAddr>>;

const NEW_FOLLOWS_RESTART_DELAY: Duration = Duration::from_secs(10);
const TWITTER_STALL: Duration = Duration::from_secs(90);

#[derive(Clone, Copy)]
enum ErrorKind {
    RateLimited,
    BadStatus,
    NetError,
    Unspecific,
}

impl ErrorKind {
    fn from_error(error: anyhow::Error) -> Self {
        // TODO: shouldn't need to downcast
        match error.downcast() {
            Ok(egg_mode::error::Error::BadStatus(status)) => {
                if status.as_u16() == 420 {
                    Self::RateLimited
                } else {
                    Self::BadStatus
                }
            }

            Ok(egg_mode::error::Error::NetError(_)) => Self::NetError,

            // TODO: anything more we need to handle?
            _ => Self::Unspecific,
        }
    }
}

// starts the twitter stream
// restarts it when it goes down
// restarts it when there are new users to follow
pub async fn supervisor(
    config: config::Twitter,
    mut rx_requested_follows: mpsc::Receiver<(SocketAddr, Follows)>,
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
    // See https://docs.rs/tokio/1.0.1/tokio/stream/index.html
    let rx_requested_follows = async_stream::stream! {
        while let Some(item) = rx_requested_follows.recv().await {
            yield item;
        }
    }
    .fuse();

    // pin to stack
    futures::pin_mut!(restart, twitter_stream, rx_requested_follows);

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

                let follows: Follows = requested_follows.keys().copied().collect();
                twitter_stream
                    .set(stream_consumer(config.token(), follows, tx_tweet.clone()).fuse());
            }

            // The stream has ended, we inspect the given error to know how much we should be
            // delaying the restart, and schedule said restart
            res = twitter_stream => {
                let error = res.err().context("infinite loop cannot return Ok(())")?;
                log::error!("twitter stream error: {:#}", error);

                let error_kind = ErrorKind::from_error(error);
                let delay = inspect_error(error_kind, &mut backoff);
                backing_off = true;

                log::info!("restarting in {:?}", delay);
                restart.set(sleep(delay).fuse());
            }

            // A client has requested new follows, if any are new, we schedule a restart.
            // If a normal (not backing off) restart was already scheduled, we ignore it and
            // re-schedule to 10 seconds.
            msg = rx_requested_follows.next() => {
                let (addr, new_follows) = msg.context("no tx_requested_follows remaining")?;

                // We remove addr from subscriptions it doesn't want anymore
                for (follow, subscribers) in &mut requested_follows {
                    if new_follows.contains(follow) {
                        continue;
                    }

                    subscribers.retain(|s| s != &addr);
                }

                let num_follows_before = requested_follows.len();

                // We drop follows that nobody wants anymore
                requested_follows.retain(|_, s| s.is_empty().not());

                let mut requires_restart =
                    config.always_restart && num_follows_before < requested_follows.len();
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
                    restart.set(sleep(NEW_FOLLOWS_RESTART_DELAY).fuse());
                }
            }

            complete => {
                anyhow::bail!("twitter::supervisor should never complete");
            }
        }
    }
}

fn inspect_error(error_kind: ErrorKind, backoff: &mut u32) -> Duration {
    match error_kind {
        ErrorKind::RateLimited => {
            let backoff_multiplier = 2u64.saturating_pow(*backoff);

            let secs = 60u64.saturating_mul(backoff_multiplier).min(960);

            *backoff = backoff.saturating_add(1);

            Duration::from_secs(secs)
        }

        ErrorKind::BadStatus => {
            let backoff_multiplier = 2u64.saturating_pow(*backoff);

            let secs = 5u64.saturating_mul(backoff_multiplier).min(320);

            *backoff = backoff.saturating_add(1);

            Duration::from_secs(secs)
        }

        ErrorKind::NetError => {
            let millis = (250 * u64::from(*backoff).max(1)).min(16_000);

            *backoff = backoff.saturating_add(1);

            Duration::from_millis(millis)
        }

        ErrorKind::Unspecific => {
            *backoff = 0;

            Duration::from_millis(250)
        }
    }
}

async fn stream_consumer(
    token: twitter::Token,
    follows: HashSet<u64>,
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

    let mut stream = twitter::stream::filter()
        .follow(&follows.iter().copied().collect::<Vec<_>>())
        .start(&token);

    loop {
        let msg = timeout(TWITTER_STALL, stream.next()).await?; // timeout

        let msg = msg.context("twitter stream ran out")?;

        // TODO: read up on these errors
        let msg = msg?; // twitter error

        match msg {
            StreamMessage::Tweet(tweet) => {
                let user = tweet.user.as_ref().unwrap();

                if follows.contains(&user.id).not() {
                    continue;
                }

                log::info!("got a tweet from {}: {:?}", user.name, tweet.text);

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

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, Duration::from_secs(60), 1)]
    #[case(1, Duration::from_secs(120), 2)]
    #[case(2, Duration::from_secs(240), 3)]
    #[case(3, Duration::from_secs(480), 4)]
    #[case(4, Duration::from_secs(960), 5)]
    #[case(5, Duration::from_secs(960), 6)]
    #[case(100, Duration::from_secs(960), 101)]
    #[case(u32::MAX-1, Duration::from_secs(960), u32::MAX)]
    #[case(u32::MAX, Duration::from_secs(960), u32::MAX)]
    fn test_420(
        #[case] mut initial_backoff: u32,
        #[case] expected_duration: Duration,
        #[case] expected_backoff: u32,
    ) {
        let error = ErrorKind::RateLimited;

        let dur = inspect_error(error, &mut initial_backoff);
        assert_eq!(initial_backoff, expected_backoff);
        assert_eq!(dur, expected_duration);
    }

    #[rstest]
    #[case(0, Duration::from_secs(5), 1)]
    #[case(1, Duration::from_secs(10), 2)]
    #[case(2, Duration::from_secs(20), 3)]
    #[case(3, Duration::from_secs(40), 4)]
    #[case(4, Duration::from_secs(80), 5)]
    #[case(5, Duration::from_secs(160), 6)]
    #[case(6, Duration::from_secs(320), 7)]
    #[case(7, Duration::from_secs(320), 8)]
    #[case(100, Duration::from_secs(320), 101)]
    #[case(u32::MAX-1, Duration::from_secs(320), u32::MAX)]
    #[case(u32::MAX, Duration::from_secs(320), u32::MAX)]
    fn test_bad_status(
        #[case] mut initial_backoff: u32,
        #[case] expected_duration: Duration,
        #[case] expected_backoff: u32,
    ) {
        let error = ErrorKind::BadStatus;

        let dur = inspect_error(error, &mut initial_backoff);
        assert_eq!(initial_backoff, expected_backoff);
        assert_eq!(dur, expected_duration);
    }

    #[rstest]
    #[case(0, Duration::from_millis(250), 1)]
    #[case(1, Duration::from_millis(250), 2)]
    #[case(2, Duration::from_millis(500), 3)]
    #[case(3, Duration::from_millis(750), 4)]
    #[case(4, Duration::from_millis(1_000), 5)]
    #[case(5, Duration::from_millis(1_250), 6)]
    #[case(100, Duration::from_secs(16), 101)]
    #[case(u32::MAX-1, Duration::from_secs(16), u32::MAX)]
    #[case(u32::MAX, Duration::from_secs(16), u32::MAX)]
    fn test_net(
        #[case] mut initial_backoff: u32,
        #[case] expected_duration: Duration,
        #[case] expected_backoff: u32,
    ) {
        let error = ErrorKind::NetError;

        let dur = inspect_error(error, &mut initial_backoff);
        assert_eq!(initial_backoff, expected_backoff);
        assert_eq!(dur, expected_duration);
    }

    #[rstest]
    #[case(0, Duration::from_millis(250), 0)]
    #[case(1, Duration::from_millis(250), 0)]
    #[case(2, Duration::from_millis(250), 0)]
    #[case(100, Duration::from_millis(250), 0)]
    #[case(u32::MAX-1, Duration::from_millis(250), 0)]
    #[case(u32::MAX, Duration::from_millis(250), 0)]
    fn test_unspecific(
        #[case] mut initial_backoff: u32,
        #[case] expected_duration: Duration,
        #[case] expected_backoff: u32,
    ) {
        let error = ErrorKind::Unspecific;

        let dur = inspect_error(error, &mut initial_backoff);
        assert_eq!(initial_backoff, expected_backoff);
        assert_eq!(dur, expected_duration);
    }
}
