// xd
//! `PubSubServer` is an actor. It maintains list of connection client session.
//! And manages available rooms. Peers send messages to other peers in same
//! room through `PubSubServer`.

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, Serialize, Debug)]
pub struct TweetURL {
    url: String,
    expanded_url: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Tweet {
    screen_name: String,
    text: String,
    in_reply_to_screen_name: Option<String>,
    urls: Vec<TweetURL>,
}

/// Chat server sends this messages to session
#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

/// Message for chat server communications

#[derive(Message)]
#[rtype(result = "()")]
pub struct Poll {}

#[derive(Message)]
#[rtype(bool)]
pub struct Subscribe {
    pub twitter_user_id: u64,

    pub addr: Recipient<Message>,
}

/// Session is disconnected
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

/// Send message to specific room
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientMessage {
    /// Id of the client session
    pub id: usize,
    /// Peer message
    pub msg: String,
    /// Room name
    pub room: String,
}

/// List of available rooms
pub struct ListRooms;

impl actix::Message for ListRooms {
    type Result = Vec<String>;
}

/// Join room, if room does not exists create new one.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Join {
    /// Client id
    pub id: usize,
    /// Room name
    pub name: String,
}

/// `PubSubServer` manages chat rooms and responsible for coordinating chat
/// session. implementation is super primitive
pub struct PubSubServer {
    twitter_user: HashMap<u64, HashSet<Recipient<Message>>>,
}

impl Default for PubSubServer {
    fn default() -> PubSubServer {
        PubSubServer {
            twitter_user: HashMap::new(),
        }
    }
}

impl PubSubServer {
    // Publish a tweet to any interested listeners
    fn publish_tweet(&self, twitter_user_id: u64, tweet: Tweet) {}

    /// Send message to all users in the room
    fn send_message(&self, room: &str, message: &str, skip_id: usize) {
        /*
        if let Some(sessions) = self.rooms.get(room) {
            for id in sessions {
                if *id != skip_id {
                    if let Some(addr) = self.sessions.get(id) {
                        let _ = addr.do_send(Message(message.to_owned()));
                    }
                }
            }
        }
        */
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

impl Handler<Poll> for PubSubServer {
    type Result = ();

    fn handle(&mut self, _: Poll, _: &mut Context<Self>) -> Self::Result {
        println!("xd");
        let uid = 81085011;
        if let Some(listeners) = self.twitter_user.get(&uid) {
            for listener in listeners {
                listener.do_send(Message(String::from("hi"))).unwrap();
            }
        }
    }
}

/// Handler for Disconnect message.
impl Handler<Disconnect> for PubSubServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        println!("Someone disconnected");

        // TODO: Remove user from all topics

        let mut rooms: Vec<String> = Vec::new();

        // // remove address
        // if self.sessions.remove(&msg.id).is_some() {
        //     // remove session from all rooms
        //     for (name, sessions) in &mut self.rooms {
        //         if sessions.remove(&msg.id) {
        //             rooms.push(name.to_owned());
        //         }
        //     }
        // }
        // // send message to other users
        // for room in rooms {
        //     self.send_message(&room, "Someone disconnected", 0);
        // }
    }
}

/// Handler for Message message.
impl Handler<ClientMessage> for PubSubServer {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) {
        self.send_message(&msg.room, msg.msg.as_str(), msg.id);
    }
}

/// Handler for `ListRooms` message.
impl Handler<ListRooms> for PubSubServer {
    type Result = MessageResult<ListRooms>;

    fn handle(&mut self, _: ListRooms, _: &mut Context<Self>) -> Self::Result {
        // let mut rooms = Vec::new();

        // for key in self.rooms.keys() {
        //     rooms.push(key.to_owned())
        // }

        MessageResult(Vec::new())
    }
}

/// Join room, send disconnect message to old room
/// send join message to new room
impl Handler<Join> for PubSubServer {
    type Result = ();

    fn handle(&mut self, msg: Join, _: &mut Context<Self>) {
        // let Join { id, name } = msg;
        // let mut rooms = Vec::new();

        // // remove session from all rooms
        // for (n, sessions) in &mut self.rooms {
        //     if sessions.remove(&id) {
        //         rooms.push(n.to_owned());
        //     }
        // }
        // // send message to other users
        // for room in rooms {
        //     self.send_message(&room, "Someone disconnected", 0);
        // }

        // if self.rooms.get_mut(&name).is_none() {
        //     self.rooms.insert(name.clone(), HashSet::new());
        // }
        // self.send_message(&name, "Someone connected", id);
        // self.rooms.get_mut(&name).unwrap().insert(id);
    }
}
