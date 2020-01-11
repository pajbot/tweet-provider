// xd
//! `PubSubServer` is an actor. It maintains list of connection client session.
//! And manages available rooms. Peers send messages to other peers in same
//! room through `PubSubServer`.

use actix::prelude::*;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::{HashMap, HashSet};

/// Chat server sends this messages to session
#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

// Incoming tweet is sent from the tweet reader
#[derive(Message)]
#[rtype(result = "()")]
pub struct IncomingTweet(pub String);

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
    fn send_tweet(&self, twitter_user_id: u64, tweet: String) {
        if let Some(listeners) = self.twitter_user.get(&twitter_user_id) {
            for listener in listeners {
                let msg = Message(tweet.clone());

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
            .or_insert_with(HashSet::new);
        entry.insert(msg.addr)
    }
}

impl Handler<Unsubscribe> for PubSubServer {
    type Result = bool;

    fn handle(&mut self, msg: Unsubscribe, _: &mut Context<Self>) -> Self::Result {
        match self.twitter_user.entry(msg.twitter_user_id) {
            Occupied(mut entry) => entry.get_mut().remove(&msg.addr),
            Vacant(_) => false,
        }
    }
}

impl Handler<IncomingTweet> for PubSubServer {
    type Result = ();

    fn handle(&mut self, inc: IncomingTweet, _: &mut Context<Self>) -> Self::Result {
        println!("xd {}", inc.0);

        #[derive(serde::Deserialize)]
        struct Tweet {
            user: User,
        }

        #[derive(serde::Deserialize)]
        struct User {
            id: u64,
        }

        let tweet = serde_json::from_str::<Tweet>(&inc.0).unwrap();

        self.send_tweet(tweet.user.id, inc.0);
    }
}

/// Handler for Disconnect message.
impl Handler<Disconnect> for PubSubServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        println!("Someone disconnected");

        for listeners in self.twitter_user.values_mut() {
            listeners.remove(&msg.addr);
        }
    }
}
