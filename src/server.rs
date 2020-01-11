// xd
//! `PubSubServer` is an actor. It maintains list of connection client session.
//! And manages available rooms. Peers send messages to other peers in same
//! room through `PubSubServer`.

use actix::prelude::*;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::{HashMap, HashSet};
use tweet::Tweet as IncomingTweetData;

use crate::response;

/// Chat server sends this messages to session
#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

// Incoming tweet is sent from the tweet reader
#[derive(Message)]
#[rtype(result = "()")]
pub struct IncomingTweet {
    pub tweet: IncomingTweetData,
}

#[derive(Message)]
#[rtype(bool)]
pub struct Subscribe {
    pub twitter_user_id: u64,

    pub addr: Recipient<Message>,
}

#[derive(Message)]
#[rtype(bool)]
pub struct Unsubscribe {
    pub twitter_user_id: u64,

    pub addr: Recipient<Message>,
}

/// Session is disconnected
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub addr: Recipient<Message>,
}

#[derive(Default)]
pub struct PubSubServer {
    twitter_user: HashMap<u64, HashSet<Recipient<Message>>>,
}

impl PubSubServer {
    /// Send a tweet to all listeners
    fn send_tweet(&self, twitter_user_id: u64, tweet: response::Tweet) {
        let response = response::Message::new_tweet(tweet);

        let response_text = response.build();

        if let Some(listeners) = self.twitter_user.get(&twitter_user_id) {
            for listener in listeners {
                let msg = Message(response_text.clone());
                if let Err(e) = listener.do_send(msg) {
                    println!("Error sending message to listener: {}", e);
                }
            }
        }
    }
}

/// Make actor from `PubSubServer`
impl Actor for PubSubServer {
    /// We are going to use simple Context, we just need ability to communicate
    /// with other actors.
    type Context = Context<Self>;
}

impl Handler<Subscribe> for PubSubServer {
    type Result = bool;

    fn handle(&mut self, msg: Subscribe, _: &mut Context<Self>) -> Self::Result {
        let entry = self
            .twitter_user
            .entry(msg.twitter_user_id)
            .or_insert(HashSet::new());
        entry.insert(msg.addr)
    }
}

impl Handler<Unsubscribe> for PubSubServer {
    type Result = bool;

    fn handle(&mut self, msg: Unsubscribe, _: &mut Context<Self>) -> Self::Result {
        match self.twitter_user.entry(msg.twitter_user_id) {
            Occupied(mut entry) => {
                return entry.get_mut().remove(&msg.addr);
            }
            Vacant(_) => {
                return false;
            }
        }
    }
}

impl Handler<IncomingTweet> for PubSubServer {
    type Result = ();

    fn handle(&mut self, tweet: IncomingTweet, _: &mut Context<Self>) -> Self::Result {
        let tweet = tweet.tweet;
        println!("xd");
        println!("Tweet: {:#?}", tweet);

        // Make a tweet output
        let mut out_tweet_xd = response::Tweet {
            screen_name: tweet.user.screen_name.clone(),
            text: tweet.full_text().clone(),
            in_reply_to_screen_name: tweet.in_reply_to_screen_name,
            urls: vec![],
        };

        // Insert urls into the vec
        if let Some(entities) = tweet.entities {
            for url in entities.urls {
                out_tweet_xd.urls.push(response::TweetURL {
                    url: url.url,
                    display_url: url.display_url,
                    expanded_url: url.expanded_url,
                });
            }
        }

        self.send_tweet(tweet.user.id, out_tweet_xd);
    }
}

/// Handler for Disconnect message.
impl Handler<Disconnect> for PubSubServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        println!("Someone disconnected");

        for (_, listeners) in &mut self.twitter_user {
            listeners.remove(&msg.addr);
        }
    }
}
