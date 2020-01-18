use crate::Follows;
use egg_mode::*;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

// Stuff that the Client sends over websocket
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClientMessage {
    SetSubscriptions(Follows),
    InsertSubscriptions(Follows),
    RemoveSubscriptions(Follows),
    // This exits the program, careful with it.
    Exit,
}

// Stuff that the Server sends over websocket
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerMessage<'a> {
    // Sent after Set/Insert/Remove Subscriptions
    AckSubscriptions(&'a Follows),
    Tweet(SerializeWrapper<&'a tweet::Tweet>),
    // Sent when the client's text frame could not be decoded to a `ClientMessage`
    ProtocolError(&'a str),
}

// Instead of deriving a bunch of data types that won't serve a purpose except to serialize JSON,
// we just implement `Serialize` ourselves, it's not too hard.
// It also means we're not moving any data around needlessly.
// We can't direclty implement Serialize on `egg_mode`'s types because of Rust laws, so we just
// make a generic wrapper that holds anything and implement `Serialize` over that.
#[derive(Debug)]
pub struct SerializeWrapper<T>(pub T);

impl Serialize for SerializeWrapper<&tweet::Tweet> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("text", &self.0.text)?;
        map.serialize_entry("id", &self.0.id)?;
        map.serialize_entry("created_at", &self.0.created_at.timestamp())?;
        map.serialize_entry("text", &self.0.text)?;
        map.serialize_entry(
            "user",
            &SerializeWrapper(&self.0.user.as_ref().unwrap() as &user::TwitterUser),
        )?;
        map.serialize_entry("truncated", &self.0.truncated)?;
        map.serialize_entry("in_reply_to_user_id", &self.0.in_reply_to_user_id)?;
        map.serialize_entry("in_reply_to_screen_name", &self.0.in_reply_to_screen_name)?;
        map.serialize_entry("in_reply_to_status_id", &self.0.in_reply_to_status_id)?;
        map.serialize_entry(
            "urls",
            &SerializeWrapper(&self.0.entities.urls as &[entities::UrlEntity]),
        )?;

        map.end()
    }
}

impl Serialize for SerializeWrapper<&user::TwitterUser> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("id", &self.0.id)?;
        map.serialize_entry("screen_name", &self.0.screen_name)?;
        map.serialize_entry("name", &self.0.name)?;

        map.end()
    }
}

impl Serialize for SerializeWrapper<&[entities::UrlEntity]> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(None)?;

        for entity in self.0 {
            seq.serialize_element(&SerializeWrapper(entity))?;
        }

        seq.end()
    }
}

impl Serialize for SerializeWrapper<&entities::UrlEntity> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("url", &self.0.url)?;
        map.serialize_entry("display_url", &self.0.display_url)?;
        map.serialize_entry("expanded_url", &self.0.expanded_url.as_ref().unwrap())?;
        map.serialize_entry("range_start", &self.0.range.0)?;
        map.serialize_entry("range_end", &self.0.range.1)?;

        map.end()
    }
}
