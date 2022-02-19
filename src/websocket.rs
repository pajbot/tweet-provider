use crate::{api, Follows};
use anyhow::{Context, Result};
use async_tungstenite::{
    self as ws,
    tungstenite::{error::Error as WsError, protocol::WebSocketConfig, Message},
};
use egg_mode::tweet::Tweet;
use futures::{sink::Sink, FutureExt, SinkExt, StreamExt};
use std::{net::SocketAddr, ops::Not, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc, Notify},
    time::{interval_at, timeout, Instant},
};

const WS_HEARTBEAT: Duration = Duration::from_secs(30);
const WS_SEND_QUEUE_CAPACITY: usize = 32;
const WS_STALL: Duration = Duration::from_secs(90);

pub async fn listener(
    listener: TcpListener,
    tx_requested_follows: mpsc::Sender<(SocketAddr, Follows)>,
    tx_tweet: broadcast::Sender<Tweet>,
    lifeline: &Arc<Notify>,
) -> Result<()> {
    log::info!("listening on {}", listener.local_addr().unwrap());

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::info!("new connection from {}", addr);

                let mut tx_requested_follows = tx_requested_follows.clone();
                let rx_tweet = tx_tweet.subscribe();

                let lifeline_clone = lifeline.clone();
                tokio::spawn(async move {
                    let res = handler(
                        stream,
                        addr,
                        &mut tx_requested_follows,
                        rx_tweet,
                        lifeline_clone,
                    )
                    .await;

                    if let Err(error) = res {
                        if let Some(WsError::ConnectionClosed) = error.downcast_ref() {
                            return;
                        }

                        log::error!("error processing websocket for {}: {:#}", addr, error);
                    }

                    if let Err(error) = tx_requested_follows.send((addr, Follows::new())).await {
                        log::warn!("failed to unsubscribe {}: {:#}", addr, error);
                    }
                });
            }

            Err(error) => log::error!("failed new connection: {:#}", error),
        }
    }
}

async fn handler(
    stream: TcpStream,
    addr: SocketAddr,
    tx_requested_follows: &mut mpsc::Sender<(SocketAddr, Follows)>,
    mut rx_tweet: broadcast::Receiver<Tweet>,
    lifeline: Arc<Notify>,
) -> Result<()> {
    let mut follows = Follows::new();

    let stream = ws::tokio::TokioAdapter::new(stream);
    let ws_config = WebSocketConfig {
        max_send_queue: Some(WS_SEND_QUEUE_CAPACITY),
        ..WebSocketConfig::default()
    };

    let ws = ws::accept_async_with_config(stream, Some(ws_config)).await?;
    let (mut tx_ws, rx_ws) = ws.split();

    let mut rx_ws = rx_ws.fuse();
    let mut heartbeat = interval_at(Instant::now(), WS_HEARTBEAT);

    loop {
        tokio::select! {
            ws_msg = timeout(WS_STALL, rx_ws.next()).fuse() => {
                let ws_msg = ws_msg.context("ws connection stalled")?;
                let ws_msg = ws_msg.context("ws stream ended")?;
                let ws_msg = ws_msg?; // websocket closed or error

                handle_ws_message(
                    ws_msg,
                    addr,
                    &mut follows,
                    &mut tx_ws,
                    tx_requested_follows,
                    &lifeline,
                )
                .await?;
            }

            tweet = rx_tweet.recv() => {
                let tweet = match tweet {
                    Ok(tweet) => tweet,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("lagging {} items behind", n);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // rx_tweet ran out, hopefully we're shutting down

                        use async_tungstenite::tungstenite::protocol;

                        log::info!("closing {}", addr);
                        tx_ws
                            .send(Message::Close(Some(protocol::CloseFrame {
                                code: protocol::frame::coding::CloseCode::Error,
                                reason: "tweet-provider was interrupted or encountered an error".into(),
                            })))
                        .await?;

                        break;
                    }
                };

                log::debug!("sending tweet to {}", addr);

                // send tweets to all clients during debug
                if cfg!(debug_assertions) || follows.contains(&tweet.user.as_ref().unwrap().id) {
                    send_json(
                        &mut tx_ws,
                        &api::ServerMessage::Tweet(api::SerializeWrapper(&tweet)),
                    )
                    .await?;
                }
            }

            _ = heartbeat.tick() => {
                log::debug!("pinging {}", addr);

                // TODO: send random data and verify when receiving pongs
                tx_ws.send(Message::Ping(b"xd".to_vec())).await?;
            }
        }
    }

    Ok(())
}

async fn handle_ws_message<S>(
    ws_msg: Message,
    addr: SocketAddr,
    follows: &mut Follows,
    mut tx_ws: S,
    tx_requested_follows: &mut mpsc::Sender<(SocketAddr, Follows)>,
    lifeline: &Arc<Notify>,
) -> Result<()>
where
    S: Sink<Message> + Send + Sync + Unpin,
    <S as Sink<Message>>::Error: 'static + Send + Sync + std::error::Error,
{
    let data = match ws_msg {
        Message::Text(data) => data,

        Message::Binary(_) => return Ok(()),

        // handled by tungstenite
        Message::Ping(_) => return Ok(()),

        Message::Pong(data) => {
            anyhow::ensure!(data == b"xd", "invalid pong");
            return Ok(());
        }

        Message::Close(reason) => {
            log::info!("websocket from {} sent close frame: {:?}", addr, reason);
            tx_ws.send(Message::Close(None)).await?;
            return Ok(());
        }

        // not received while reading
        Message::Frame(_) => return Ok(()),
    };

    match serde_json::from_str(&data) {
        Err(error) => {
            log::error!("json parse error: {:#}", error);

            send_json(
                &mut tx_ws,
                &api::ServerMessage::ProtocolError(&error.to_string()),
            )
            .await?;

            return Ok(());
        }

        Ok(api::ClientMessage::Exit) => {
            log::warn!("client {} requested exit", addr);
            lifeline.notify_one();
            return Ok(());
        }

        Ok(api::ClientMessage::SetSubscriptions(new_follows)) => {
            *follows = new_follows;
        }

        Ok(api::ClientMessage::InsertSubscriptions(new_follows)) => {
            follows.extend(new_follows);
        }

        Ok(api::ClientMessage::RemoveSubscriptions(new_follows)) => {
            follows.retain(|f| new_follows.contains(f).not());
        }
    }

    tx_requested_follows
        .send((addr, follows.clone()))
        .await
        .context("no rx_requested_follows remaining")?;

    send_json(&mut tx_ws, &api::ServerMessage::AckSubscriptions(&follows)).await?;

    Ok(())
}

async fn send_json<S>(mut tx_ws: S, data: impl serde::Serialize) -> Result<()>
where
    S: Sink<Message> + Send + Sync + Unpin,
    <S as Sink<Message>>::Error: 'static + Send + Sync + std::error::Error,
{
    Ok(tx_ws
        .send(Message::Text(serde_json::to_string(&data)?))
        .await?)
}
