use crate::Follows;
use egg_mode::{self as twitter};
use serde::{Deserialize, Serialize};
use std::{
    iter::FromIterator,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

// This file is mostly boilerplate code

// StructOpt derives an argument parser and environment reader
// The config is read in this order of fallbacks:
// Program arguments -> Environment -> Config file
#[derive(Clone, Debug, StructOpt)]
#[structopt(rename_all = "kebab")]
pub struct Args {
    #[structopt(
        short = "C",
        long = "conf",
        env = "PAJBOT_CONF",
        default_value = "tweet-provider.toml"
    )]
    pub config_path: PathBuf,

    #[structopt(flatten)]
    pub config: Config,

    #[structopt(short = "L", long = "log", default_value = "INFO", env = "PAJBOT_LOG")]
    pub log_level: log::Level,
}

#[derive(Clone, Debug, Deserialize, Serialize, StructOpt)]
pub struct Config {
    #[serde(default)]
    #[structopt(flatten)]
    pub websocket: WebSocket,

    #[serde(default)]
    #[structopt(flatten)]
    pub twitter: Twitter,
}

#[derive(Clone, Debug, Deserialize, Serialize, StructOpt)]
pub struct WebSocket {
    #[serde(default = "WebSocket::default_listen_addr")]
    #[structopt(
        short = "l",
        long = "listen",
        env = "PAJBOT_LISTEN",
        default_value = "127.0.0.1:8000"
    )]
    pub listen_addr: SocketAddr,
}

#[derive(Clone, Debug, Deserialize, Serialize, StructOpt)]
pub struct Twitter {
    #[structopt(
        long = "twitter-consumer-key",
        env = "PAJBOT_TWITTER_CONSUMER_KEY",
        hide_env_values = true
    )]
    pub consumer_key: Option<String>,

    #[structopt(
        long = "twitter-consumer-secret",
        env = "PAJBOT_TWITTER_CONSUMER_SECRET",
        hide_env_values = true
    )]
    pub consumer_secret: Option<String>,

    #[structopt(
        long = "twitter-access-token",
        env = "PAJBOT_TWITTER_ACCESS_TOKEN",
        hide_env_values = true
    )]
    pub access_token: Option<String>,

    #[structopt(
        long = "twitter-access-token-secret",
        env = "PAJBOT_TWITTER_ACCESS_TOKEN_SECRET",
        hide_env_values = true
    )]
    pub access_token_secret: Option<String>,

    #[serde(default)]
    #[structopt(short = "f", long = "follow")]
    pub default_follows: Vec<u64>,

    #[serde(default = "Twitter::default_follows_cache")]
    #[structopt(
        short = "c",
        long = "cache",
        default_value = "cache.json",
        env = "PAJBOT_CACHE"
    )]
    pub follows_cache: PathBuf,
    // TODO: follows_cache_size
}

impl Config {
    pub fn merge(self, other: Self) -> Self {
        Self {
            websocket: self.websocket.merge(other.websocket),
            twitter: self.twitter.merge(other.twitter),
        }
    }

    pub async fn from_toml(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(toml::from_str(&tokio::fs::read_to_string(path).await?)?)
    }
}

impl WebSocket {
    pub fn default_listen_addr() -> SocketAddr {
        "127.0.0.1:8000".parse().unwrap()
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            listen_addr: if self.listen_addr != Self::default_listen_addr() {
                self.listen_addr
            } else {
                other.listen_addr
            },
        }
    }
}

impl Default for WebSocket {
    fn default() -> Self {
        Self {
            listen_addr: Self::default_listen_addr(),
        }
    }
}

impl Twitter {
    fn default_follows_cache() -> PathBuf {
        "cache.json".into()
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            consumer_key: self.consumer_key.or(other.consumer_key),
            consumer_secret: self.consumer_secret.or(other.consumer_secret),
            access_token: self.access_token.or(other.access_token),
            access_token_secret: self.access_token_secret.or(other.access_token_secret),

            default_follows: Follows::from_iter(self.default_follows)
                .union(&Follows::from_iter(other.default_follows))
                .copied()
                .collect(),

            follows_cache: if self.follows_cache != Self::default_follows_cache() {
                self.follows_cache
            } else {
                other.follows_cache
            },
        }
    }

    pub fn token(&self) -> twitter::Token {
        let x = |s: &Option<String>| s.as_ref().cloned().unwrap();

        twitter::Token::Access {
            consumer: twitter::KeyPair::new(x(&self.consumer_key), x(&self.consumer_secret)),
            access: twitter::KeyPair::new(x(&self.access_token), x(&self.access_token_secret)),
        }
    }
}

impl Default for Twitter {
    fn default() -> Self {
        Self {
            consumer_key: None,
            consumer_secret: None,
            access_token: None,
            access_token_secret: None,

            default_follows: vec![],

            follows_cache: Self::default_follows_cache(),
        }
    }
}
