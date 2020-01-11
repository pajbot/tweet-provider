use actix::*;
use actix::{fut, Actor, Addr, Handler, Running, StreamHandler};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use futures::prelude::*;
// use futures::stream::TryForEach;
use std::time::{Duration, Instant};
// use tokio::runtime::Runtime;
// use tokio::signal;
use tweet::Tweet as IncomingTweetData;
use twitter_stream::Token;

use request::Message as RequestMessage;
use response::Message as ResponseMessage;

use std::env;

mod request;
mod response;
mod server;

async fn pubsub_client_route(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<server::PubSubServer>>,
) -> Result<HttpResponse, Error> {
    let resp = ws::start(
        PubSubClient {
            hb: Instant::now(),
            server_addr: srv.get_ref().clone(),
        },
        &req,
        stream,
    );
    // let resp = ws::start(WebsocketConnection::new(), &req, stream);
    println!("Connected: {:#?}", req.connection_info());
    resp
}

struct PubSubClient {
    hb: Instant,
    server_addr: Addr<server::PubSubServer>,
}

impl Actor for PubSubClient {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start.
    /// We register ws session with ChatServer
    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        let addr = ctx.address();

        // notify chat server
        self.server_addr.do_send(server::Disconnect {
            addr: addr.recipient(),
        });

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

impl PubSubClient {
    fn handle_text(
        &mut self,
        text: String,
        ctx: &mut <PubSubClient as Actor>::Context,
    ) -> Result<String, serde_json::Error> {
        let request: RequestMessage = RequestMessage::parse(&text)?;

        match request {
            RequestMessage::Subscribe(m) => {
                let addr = ctx.address();
                self.server_addr
                    .send(server::Subscribe {
                        twitter_user_id: m.twitter_user_id,
                        addr: addr.recipient(),
                    })
                    .into_actor(self)
                    .then(move |res, _, ctx| {
                        match res {
                            Ok(true) => {
                                let r = ResponseMessage::new_subscribe(format!(
                                    "Successfully subscribed to twitter user {}",
                                    m.twitter_user_id
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            Ok(false) => {
                                let r = ResponseMessage::new_subscribe(format!(
                                    "You are already subscribed to the twitter user {}",
                                    m.twitter_user_id
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            // something is wrong with chat server
                            Err(_) => ctx.stop(),
                        }
                        fut::ready(())
                    })
                    .wait(ctx);
            }

            RequestMessage::Unsubscribe(m) => {
                let addr = ctx.address();
                self.server_addr
                    .send(server::Unsubscribe {
                        twitter_user_id: m.twitter_user_id,
                        addr: addr.recipient(),
                    })
                    .into_actor(self)
                    .then(move |res, _, ctx| {
                        match res {
                            Ok(true) => {
                                let r = ResponseMessage::new_unsubscribe(format!(
                                    "Successfully unsubscribed from twitter user {}",
                                    m.twitter_user_id
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            Ok(false) => {
                                let r = ResponseMessage::new_unsubscribe(format!(
                                    "You are not subscribed to twitter user {}",
                                    m.twitter_user_id
                                ));
                                ctx.text(serde_json::to_string(&r).unwrap());
                            }
                            // something is wrong with chat server
                            _ => ctx.stop(),
                        }
                        fut::ready(())
                    })
                    .wait(ctx);
            }
        };

        let str = String::from("asd");

        Ok(str)
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for PubSubClient {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }

            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }

            ws::Message::Text(text) => {
                if let Err(e) = self.handle_text(text, ctx) {
                    let response = ResponseMessage::new_error(format!("error lol {}", e));
                    ctx.text(serde_json::to_string(&response).unwrap());
                    println!("error xd {:?}", e);
                }
            }

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
                act.server_addr.do_send(server::Disconnect {
                    addr: ctx.address().recipient(),
                });

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
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

    let a = [81_085_011];
    let uids: &[u64] = &a;

    twitter_stream::Builder::filter(token)
        .follow(Some(uids))
        .listen()
        .try_flatten_stream()
        .try_for_each(|tweet| {
            // Create tweet from JSON
            server.do_send(server::IncomingTweet(tweet.to_string()));
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

    Arbiter::spawn(async {
        HttpServer::new(move || {
            App::new()
                .data(server.clone())
                .service(web::resource("/ws/").to(pubsub_client_route))
        })
        .bind("127.0.0.1:1235")
        .unwrap()
        .run()
        .await
        .unwrap();
        println!("end of web listener");
    });

    sys.run().unwrap();

    println!("xdisdfghkkjdfghkjfdg");
}
