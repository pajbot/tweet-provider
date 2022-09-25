use clap::{Parser, ValueEnum};
use egg_mode::{self as twitter};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogTimestamps {
    Local,
    UTC,
    Off,
}

// This file is mostly boilerplate code

// StructOpt derives an argument parser and environment reader
// The config is read in this order of fallbacks:
// Program arguments -> Environment -> Config file
#[derive(Clone, Debug, Parser)]
#[clap(version, rename_all = "kebab")]
pub struct Args {
    /// Path to config file in TOML format
    #[clap(
        short = 'C',
        long = "conf",
        env = "PAJBOT_CONF",
        default_value = "tweet-provider.toml"
    )]
    pub config_path: PathBuf,

    #[clap(flatten)]
    pub config: Config,

    /// Log level filter, either: OFF, ERROR, WARN, INFO, DEBUG, TRACE
    #[clap(short = 'L', long = "log", default_value = "INFO", env = "PAJBOT_LOG")]
    pub log_level: log::LevelFilter,

    /// Log message timestamp method
    #[clap(
        long = "log-timestamps",
        arg_enum,
        default_value = "utc",
        env = "PAJBOT_LOG_TIMESTAMPS"
    )]
    pub log_timestamps: LogTimestamps,
}

#[derive(Clone, Debug, Deserialize, Serialize, Parser)]
pub struct Config {
    #[serde(default)]
    #[clap(flatten)]
    pub websocket: WebSocket,

    #[serde(default)]
    #[clap(flatten)]
    pub twitter: Twitter,
}

#[derive(Clone, Debug, Deserialize, Serialize, Parser)]
pub struct WebSocket {
    /// address:port to bind the websocket listener to
    #[serde(default = "WebSocket::default_listen_addr")]
    #[clap(
        short = 'l',
        long = "listen",
        env = "PAJBOT_LISTEN",
        default_value = "127.0.0.1:2356"
    )]
    pub listen_addr: SocketAddr,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Parser)]
pub struct Twitter {
    /// Consumer API key. Found in App's Keys and tokens on https://developer.twitter.com
    #[clap(
        long = "twitter-consumer-key",
        env = "PAJBOT_TWITTER_CONSUMER_KEY",
        hide_env_values = true
    )]
    pub consumer_key: Option<String>,

    /// Consumer API secret key
    #[clap(
        long = "twitter-consumer-secret",
        env = "PAJBOT_TWITTER_CONSUMER_SECRET",
        hide_env_values = true
    )]
    pub consumer_secret: Option<String>,

    /// Access token. Found in App's Keys and tokens on https://developer.twitter.com
    #[clap(
        long = "twitter-access-token",
        env = "PAJBOT_TWITTER_ACCESS_TOKEN",
        hide_env_values = true
    )]
    pub access_token: Option<String>,

    /// Access token secret
    #[clap(
        long = "twitter-access-token-secret",
        env = "PAJBOT_TWITTER_ACCESS_TOKEN_SECRET",
        hide_env_values = true
    )]
    pub access_token_secret: Option<String>,

    /// Always restart the twitter consumer when the requested follows change,
    /// as opposed to only when new follows are added
    #[serde(default)]
    #[clap(
        long = "twitter-always-restart",
        env = "PAJBOT_TWITTER_ALWAYS_RESTART",
        hide_env_values = true
    )]
    pub always_restart: bool,
}

impl Config {
    pub fn merge(self, other: Self) -> Self {
        Self {
            websocket: self.websocket.merge(&other.websocket),
            twitter: self.twitter.merge(other.twitter),
        }
    }

    pub async fn from_toml<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path> + Send,
    {
        Ok(toml::from_str(&tokio::fs::read_to_string(path).await?)?)
    }
}

impl WebSocket {
    pub fn default_listen_addr() -> SocketAddr {
        "127.0.0.1:2356".parse().unwrap()
    }

    pub fn merge(self, other: &Self) -> Self {
        Self {
            listen_addr: if self.listen_addr == Self::default_listen_addr() {
                other.listen_addr
            } else {
                self.listen_addr
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
    pub fn merge(self, other: Self) -> Self {
        Self {
            consumer_key: self.consumer_key.or(other.consumer_key),
            consumer_secret: self.consumer_secret.or(other.consumer_secret),
            access_token: self.access_token.or(other.access_token),
            access_token_secret: self.access_token_secret.or(other.access_token_secret),
            always_restart: self.always_restart || other.always_restart,
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
