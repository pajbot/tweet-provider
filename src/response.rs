use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct StringResponse {
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TweetURL {
    pub url: String,
    pub display_url: String,
    pub expanded_url: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Tweet {
    pub screen_name: String,
    pub text: String,
    pub in_reply_to_screen_name: Option<String>,
    pub urls: Vec<TweetURL>,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Message {
    SubscribeResponse(StringResponse),
    UnsubscribeResponse(StringResponse),
    Error(StringResponse),
    Tweet(Tweet),
}

impl From<String> for StringResponse {
    fn from(s: String) -> Self {
        StringResponse { message: s }
    }
}

impl Message {
    pub fn new_subscribe<T: Into<StringResponse>>(message: T) -> Self {
        Message::SubscribeResponse(message.into())
    }

    pub fn new_unsubscribe<T: Into<StringResponse>>(message: T) -> Self {
        Message::UnsubscribeResponse(message.into())
    }

    pub fn new_error<T: Into<StringResponse>>(message: T) -> Self {
        Message::Error(message.into())
    }

    pub fn new_tweet(tweet: Tweet) -> Self {
        Message::Tweet(tweet)
    }

    pub fn build(&self) -> String {
        return serde_json::to_string(self).unwrap();
    }
}
