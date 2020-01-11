use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Subscribe {
    pub twitter_user_id: u64,
}

#[derive(Deserialize, Debug)]
pub struct Unsubscribe {
    pub twitter_user_id: u64,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Message {
    // TODO: make trait which has a "handle" function maybe?
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
}

impl Message {
    pub fn parse(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Message>(s)
    }
}
