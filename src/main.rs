use actix::{Actor, StreamHandler};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};

use std::collections::HashSet;
// use serde_json::{Result, Value};

fn index() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

fn ws(req: HttpRequest, stream: web::Payload) -> impl Responder {
    let resp = ws::start(WebsocketConnection::new(), &req, stream);
    println!("{:?}", resp);
    resp
}

struct WebsocketConnection {
    topics: HashSet<String>,
}

impl WebsocketConnection {
    fn new() -> WebsocketConnection {
        WebsocketConnection {
            topics: HashSet::new(),
        }
    }
}

impl Actor for WebsocketConnection {
    type Context = ws::WebsocketContext<Self>;
}

#[derive(Deserialize, Serialize, Debug)]
struct TweetURL {
    url: String,
    expanded_url: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct Tweet {
    screen_name: String,
    text: String,
    in_reply_to_screen_name: Option<String>,
    urls: Vec<TweetURL>,
}

#[derive(Deserialize, Serialize)]
struct ErrorDescription {
    error: String,
}

#[derive(Serialize, Debug)]
struct SubscribeResponse {
    message: String,
}

#[derive(Serialize, Debug)]
struct UnsubscribeResponse {
    message: String,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum ServiceResponse {
    SubscribeResponse(String),
    UnsubscribeResponse(String),
    Tweet(Tweet),
    ErrorDescription(ErrorDescription),
}

#[derive(Deserialize, Debug)]
struct Subscribe {
    topic: String,
}

#[derive(Deserialize, Debug)]
struct Unsubscribe {
    topic: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum ServiceRequest {
    // TODO: make trait which has a "handle" function maybe?
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Poll,

    // TODO: Tweets should not be insert like this
    Tweet(Tweet),
}

impl StreamHandler<ws::Message, ws::ProtocolError> for WebsocketConnection {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => {
                let request = serde_json::from_str::<ServiceRequest>(&text);
                match request {
                    Ok(f) => match f {
                        ServiceRequest::Subscribe(m) => match self.topics.insert(m.topic.clone()) {
                            true => {
                                let r = ServiceResponse::SubscribeResponse(format!(
                                    "Successfully subscribed to topic {}",
                                    m.topic
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            false => {
                                let r = ServiceResponse::SubscribeResponse(format!(
                                    "You are already subscribed to the topic {}",
                                    m.topic
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                        },

                        ServiceRequest::Unsubscribe(m) => match self.topics.remove(&m.topic) {
                            true => {
                                let r = ServiceResponse::UnsubscribeResponse(format!(
                                    "Successfully unsubscribed from topic {}",
                                    m.topic
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            false => {
                                let r = ServiceResponse::UnsubscribeResponse(format!(
                                    "You are not subscribed to {}",
                                    m.topic
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                        },

                        ServiceRequest::Poll => {
                            // TODO: automatically post tweets to listeners as they come in
                            println!("got poll");
                            let t = Tweet {
                                screen_name: "pajlada".to_owned(),
                                text: format!("This is a test fake tweet! lol: {}", text),
                                in_reply_to_screen_name: None,
                                urls: vec![],
                            };
                            let r = ServiceResponse::Tweet(t);
                            ctx.text(serde_json::to_string(&r).unwrap());
                        }

                        ServiceRequest::Tweet(m) => {
                            // TODO: 1. Find user who is subscribed to "tweet" topic
                            // TODO: 2. Find user who is interested in this specific users' tweets
                            println!("got tweet: {:?}", m);
                        }
                    },
                    Err(e) => {
                        let response = ServiceResponse::ErrorDescription(ErrorDescription {
                            error: format!("error lol {}", e),
                        });
                        ctx.text(serde_json::to_string(&response).unwrap());
                        println!("error xd {:?}", e);
                    }
                }
            }
            ws::Message::Binary(bin) => ctx.binary(bin),
            _ => (),
        }
    }
}

fn main() {
    println!("Hello, world!");
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .route("/ws/", web::get().to(ws))
    })
    .bind("127.0.0.1:1235")
    .unwrap()
    .run()
    .unwrap();
}
