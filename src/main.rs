use std::time::{Duration, Instant};
use actix::*;
use tokio::signal;
use actix::{Actor, StreamHandler, Addr, Handler, Running, fut};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix_web_actors::ws;
use tokio::runtime::Runtime;
use futures::prelude::*;
use futures::stream::TryForEach;
use serde::{Deserialize, Serialize};
use twitter_stream::{Token, FutureTwitterStream};

use std::env;
use std::collections::HashSet;
// use serde_json::{Result, Value};

mod server;

async fn pubsub_client_route(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<server::PubSubServer>>,
    ) -> Result<HttpResponse, Error>{
    let resp = ws::start(PubSubClient{
        id: 0,
        hb: Instant::now(),
        addr: srv.get_ref().clone(),
        topics: HashSet::new(),
    }, &req, stream);
    // let resp = ws::start(WebsocketConnection::new(), &req, stream);
    println!("{:?}", resp);
    resp
}

struct PubSubClient {
    id: usize,
    hb: Instant,
    addr: Addr<server::PubSubServer>,

    topics: HashSet<String>,
}

impl Actor for PubSubClient {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start.
    /// We register ws session with ChatServer
    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // notify chat server
        self.addr.do_send(server::Disconnect { id: self.id });
        Running::Stop
    }
}

/// Handle messages from chat server, we simply send it to peer websocket
impl Handler<server::Message> for PubSubClient {
    type Result = ();

    fn handle(&mut self, msg: server::Message, ctx: &mut Self::Context) {
        println!("Handle server::Message");
        ctx.text(msg.0);
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct TweetURL {
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

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for PubSubClient {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Ping(msg) => {
                println!("Received ping");
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                println!("Received pong");
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => {
                let request = serde_json::from_str::<ServiceRequest>(&text);
                match request {
                    Ok(f) => match f {
                        ServiceRequest::Subscribe(m) => {
                            let addr = ctx.address();
                            self.addr
                                .send(server::Subscribe {
                                    twitter_user_id: 81085011,
                                    addr: addr.recipient(),
                                })
                                .into_actor(self)
                                .then(move |res, _, ctx| {
                                    match res {
                                        Ok(res) => match res {
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
                                        }
                                        // something is wrong with chat server
                                        _ => ctx.stop(),
                                    }
                                    fut::ready(())
                                })
                                .wait(ctx);
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


                            let addr = ctx.address();
                            self.addr
                                .send(server::Poll {})
                                .into_actor(self)
                                .then(move |res, _, ctx| {
                                    match res {
                                        Ok(_) => {
                                            println!("ok");
                                        }
                                        // something is wrong with chat server
                                        _ => ctx.stop(),
                                    }
                                    fut::ready(())
                                })
                                .wait(ctx);
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

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

impl PubSubClient {
    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // notify chat server
                act.addr.do_send(server::Disconnect { id: act.id });

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}

struct Waker;

struct Context<'a> {
        waker: &'a Waker,
        
}

impl<'a> Context<'a> {
    fn from_waker(waker: &'a Waker) -> Self {
                Context { waker  }
                    
    }

    fn waker(&self) -> &'a Waker {
                &self.waker
                        
    }
    
}

async fn start_twitter_listener(server: Addr<server::PubSubServer>) {
    let token = Token::new(
        env::var("PAJBOT_TWITTER_CONSUMER_KEY").unwrap(),
        env::var("PAJBOT_TWITTER_CONSUMER_SECRET").unwrap(),
        env::var("PAJBOT_TWITTER_ACCESS_TOKEN").unwrap(),
        env::var("PAJBOT_TWITTER_ACCESS_TOKEN_SECRET").unwrap(),
    );


    println!("Start twitter listener!");

    let a = [81085011];
    let uids : &[u64]= &a;

    twitter_stream::Builder::filter(token)
        .follow(Some(uids))
        .listen()
        .try_flatten_stream()
        .try_for_each(|json| {
            server.do_send(server::Poll{});
            println!("{}", json);
            future::ok(())
        })
    .await
        .unwrap();
}

fn main() {
    let sys = System::new("xd");

    let server = server::PubSubServer::default().start();

    let srv = server.clone();

    Arbiter::spawn(async {
        println!("xd");
        start_twitter_listener(srv).await;
        println!("end of twitter listener");
    });

    // let deadline = tokio::time::delay_until(Instant::now() + Duration::from_secs(5))
    //             .map(|()| println!("5 seconds are over"))
    //                     .map_err(|e| eprintln!("Failed to wait: {}", e));


    // Arbiter::spawn(deadline);

//     Arbiter::spawn(async {
//         tokio::signal::ctrl_c().await.unwrap();
//         println!("ctrl c pressed?");
//         System::current().stop();
//     });

    Arbiter::spawn(async {
        HttpServer::new(move || {
            App::new()
                .data(server.clone())
                .service(web::resource("/ws/").to(pubsub_client_route))
        })
        .bind("127.0.0.1:1235").unwrap()
            .run()
            .await
            .unwrap();
        println!("end of web listener");
    });

    sys.run().unwrap();

    println!("xdisdfghkkjdfghkjfdg");
}
