//! Remote benchmarking agent listener (`client` mode).

use crate::server::protocol::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

static CONNECTED: std::sync::OnceLock<Arc<Mutex<Option<ServerInfo>>>> =
    std::sync::OnceLock::new();

pub async fn run_client(listen_addr: &str) -> Result<(), String> {
    let addr: SocketAddr = listen_addr
        .parse()
        .map_err(|e| format!("invalid listen address {listen_addr}: {e}"))?;

    CONNECTED.get_or_init(|| Arc::new(Mutex::new(None)));

    let app = Router::new().route("/ws", get(ws_handler));

    info!("benchmark agent listening on ws://{}", listen_addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("failed to bind: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("HTTP server exited: {e}"))
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut ws: WebSocket) {
    let s_info: ServerInfo = match read_server_info(&mut ws).await {
        Ok(s) => s,
        Err(e) => {
            error!("failed to read coordinator ServerInfo: {e}");
            return;
        }
    };

    if let Err(e) = s_info.validate() {
        let reply = ClientReply {
            time: chrono::Utc::now(),
            stage_info: StageInfo {
                custom: None,
                progress: 0.0,
                started: false,
                finished: false,
            },
            reply_type: ClientReplyType::BenchmarkStarted,
            err: Some(e),
            ops: None,
        };
        let json = serde_json::to_string(&reply)
            .unwrap_or_else(|e| format!("{{\"error\":\"ser: {e}\"}}"));
        let _ = ws.send(Message::Text(json.into())).await;
        return;
    }

    {
        let mut guard = CONNECTED
            .get()
            .expect("CONNECTED OnceLock not initialized — call run_client first")
            .lock()
            .await;
        if let Some(existing) = guard.as_ref() {
            if !existing.id.is_empty() && existing.id != s_info.id {
                let reply = ClientReply {
                    time: chrono::Utc::now(),
                    stage_info: StageInfo {
                        custom: None,
                        progress: 0.0,
                        started: false,
                        finished: false,
                    },
                    reply_type: ClientReplyType::BenchmarkStarted,
                    err: Some("another coordinator is already attached".into()),
                    ops: None,
                };
                let json = serde_json::to_string(&reply)
                    .unwrap_or_else(|e| format!("{{\"error\":\"ser: {e}\"}}"));
                let _ = ws.send(Message::Text(json.into())).await;
                return;
            }
        }
        *guard = Some(s_info.clone());
    }

    info!("accepted coordinator connection {}", s_info.id);

    {
        let reply = ClientReply {
            time: chrono::Utc::now(),
            stage_info: StageInfo {
                custom: None,
                progress: 0.0,
                started: false,
                finished: false,
            },
            reply_type: ClientReplyType::BenchmarkStarted,
            err: None,
            ops: None,
        };
        let json = serde_json::to_string(&reply)
            .unwrap_or_else(|e| format!("{{\"error\":\"ser: {e}\"}}"));
        let _ = ws.send(Message::Text(json.into())).await;
    }

    loop {
        match read_request(&mut ws).await {
            Ok(req) => {
                let reply = build_reply(&req);
                let json = serde_json::to_string(&reply)
            .unwrap_or_else(|e| format!("{{\"error\":\"ser: {e}\"}}"));
                if let Err(e) = ws.send(Message::Text(json.into())).await {
                    error!("failed to send reply: {e}");
                    break;
                }
                if matches!(req.op, ServerRequestOp::Disconnect) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let mut guard = CONNECTED
        .get()
        .expect("CONNECTED OnceLock not initialized")
        .lock()
        .await;
    *guard = None;
    info!("coordinator websocket closed");
}

fn build_reply(req: &ServerRequest) -> ClientReply {
    let now = chrono::Utc::now();
    match req.op {
        ServerRequestOp::Disconnect => ClientReply {
            time: now,
            stage_info: StageInfo {
                custom: None,
                progress: 1.0,
                started: false,
                finished: true,
            },
            reply_type: ClientReplyType::BenchmarkStatus,
            err: None,
            ops: None,
        },
        ServerRequestOp::Benchmark => {
            info!(
                "received benchmark: {}",
                req.benchmark.as_ref().map(|b| b.command.as_str()).unwrap_or("?")
            );
            ClientReply {
                time: now,
                stage_info: StageInfo {
                    custom: None,
                    progress: 0.0,
                    started: true,
                    finished: false,
                },
                reply_type: ClientReplyType::BenchmarkStarted,
                err: None,
                ops: None,
            }
        }
        ServerRequestOp::StageStatus => ClientReply {
            time: now,
            stage_info: StageInfo {
                custom: None,
                progress: 0.5,
                started: true,
                finished: true,
            },
            reply_type: ClientReplyType::BenchmarkStatus,
            err: None,
            ops: None,
        },
        ServerRequestOp::SendOps => ClientReply {
            time: now,
            stage_info: StageInfo {
                custom: None,
                progress: 1.0,
                started: false,
                finished: true,
            },
            reply_type: ClientReplyType::Ops,
            err: None,
            ops: Some(Vec::new()),
        },
        _ => ClientReply {
            time: now,
            stage_info: StageInfo {
                custom: None,
                progress: 0.0,
                started: false,
                finished: false,
            },
            reply_type: ClientReplyType::BenchmarkStatus,
            err: None,
            ops: None,
        },
    }
}

async fn read_server_info(ws: &mut WebSocket) -> Result<ServerInfo, String> {
    match ws.recv().await {
        Some(Ok(Message::Text(text))) => {
            serde_json::from_str::<ServerInfo>(&text).map_err(|e| format!("JSON: {e}"))
        }
        Some(Ok(Message::Close(_))) => Err("connection closed".into()),
        Some(Err(e)) => Err(format!("WS: {e}")),
        None => Err("connection closed".into()),
        _ => Err("unexpected message".into()),
    }
}

async fn read_request(ws: &mut WebSocket) -> Result<ServerRequest, String> {
    loop {
        match ws.recv().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str::<ServerRequest>(&text)
                    .map_err(|e| format!("JSON: {e}"));
            }
            Some(Ok(Message::Close(_))) => return Err("connection closed".into()),
            Some(Err(e)) => return Err(format!("WS: {e}")),
            None => return Err("connection closed".into()),
            _ => {}
        }
    }
}
