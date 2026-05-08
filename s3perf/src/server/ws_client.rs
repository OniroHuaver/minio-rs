//! Coordinator-side WebSockets client connecting to remote benchmark agents.

use crate::server::protocol::*;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// WebSocket tunnel to one remote benchmarking agent.
pub struct ClientWs {
    pub host: String,
    pub info: ServerInfo,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl ClientWs {
    /// Negotiate handshake (ServerInfo announcement + acknowledgement).
    pub async fn connect(host: &str, server_info: &ServerInfo) -> Result<Self, String> {
        let url = format!("ws://{host}/ws");
        let (mut ws, _resp) =
            connect_async(&url).await.map_err(|e| format!("connect failed ({host}): {e}"))?;

        let info_json =
            serde_json::to_string(server_info).map_err(|e| format!("serialize error: {e}"))?;
        ws.send(Message::Text(info_json.into()))
            .await
            .map_err(|e| format!("failed to send server info: {e}"))?;

        let reply: ClientReply = read_json(&mut ws)
            .await
            .map_err(|e| format!("failed to read acknowledgement: {e}"))?;

        if let Some(err) = &reply.err {
            if !err.is_empty() {
                return Err(format!("remote agent rejected handshake: {err}"));
            }
        }

        Ok(Self {
            host: host.to_string(),
            info: server_info.clone(),
            ws,
        })
    }

    pub async fn round_trip(&mut self, req: &ServerRequest) -> Result<ClientReply, String> {
        let json = serde_json::to_string(req).map_err(|e| format!("serialize request: {e}"))?;
        self.ws
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| format!("failed to send request: {e}"))?;
        read_json_with_timeout(&mut self.ws, Duration::from_secs(30)).await
    }

    pub async fn send(&mut self, req: &ServerRequest) -> Result<(), String> {
        let json = serde_json::to_string(req).map_err(|e| format!("serialize: {e}"))?;
        self.ws
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| format!("send failed: {e}"))
    }

    pub async fn read_reply(&mut self) -> Result<ClientReply, String> {
        read_json_with_timeout(&mut self.ws, Duration::from_secs(300)).await
    }

    pub async fn close(mut self) -> Result<(), String> {
        self.ws
            .close(None)
            .await
            .map_err(|e| format!("failed to close connection: {e}"))
    }
}

async fn read_json_with_timeout<T: serde::de::DeserializeOwned>(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<T, String> {
    tokio::time::timeout(timeout, read_json::<T>(ws))
        .await
        .map_err(|_| "read timed out".to_string())?
}

async fn read_json<T: serde::de::DeserializeOwned>(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<T, String> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str::<T>(&text)
                    .map_err(|e| format!("JSON parse failure: {e}"));
            }
            Some(Ok(Message::Close(_))) => return Err("connection closed by peer".into()),
            Some(Err(e)) => return Err(format!("WebSocket error: {e}")),
            None => return Err("socket closed".into()),
            _ => {}
        }
    }
}

pub async fn connect_with_retry(
    host: &str,
    server_info: &ServerInfo,
    max_retries: usize,
) -> Result<ClientWs, String> {
    let mut last_err = String::from("(no attempts)");
    let mut delay = Duration::from_secs(1);

    for attempt in 0..=max_retries {
        match ClientWs::connect(host, server_info).await {
            Ok(ws) => return Ok(ws),
            Err(e) => {
                last_err = e;
                if attempt < max_retries {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(10));
                }
            }
        }
    }

    Err(format!(
        "connect to {host} failed after {max_retries} retries: {last_err}"
    ))
}
