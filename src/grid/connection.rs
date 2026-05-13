use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock, oneshot, watch};

use tokio_tungstenite::tungstenite;

use crate::grid::AuthValidateFn;
use crate::grid::connection_state::ConnectionState;
use crate::grid::error::{GridError, GridResult, RemoteErr};
use crate::grid::handler::HandlerRegistry;
use crate::grid::message::{Flags, HANDLER_INVALID, HandlerId, Message, Op};
use crate::grid::msg_types::{ConnectReq, ConnectResp};

#[cfg(test)]
use crate::grid::debug::DebugMsg;

/// Max decoded MessagePack payload per WebSocket binary/text frame (DoS guard).
const MAX_GRID_WIRE_BYTES: usize = 64 * 1024 * 1024;

/// Max accepted clock skew for legacy `ConnectReq` (`nonce == 0`).
const CONNECT_CLOCK_SKEW_SECS: f64 = 300.0;

/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Max messages merged into a single frame.
const MAX_MERGED_MSGS: usize = 50;

/// Ping interval (client side).
const PING_INTERVAL: Duration = Duration::from_secs(15);

/// Connection-level ping interval.
#[allow(dead_code)]
const CONN_PING_INTERVAL: Duration = Duration::from_secs(10);

/// Max time without pong before disconnect.
const PONG_TIMEOUT: Duration = Duration::from_secs(30);

/// Response channel: mux_id → oneshot sender.
type OutgoingMap = Mutex<HashMap<u64, oneshot::Sender<Result<Vec<u8>, RemoteErr>>>>;

#[derive(Default)]
pub(crate) struct ConnectReplayState {
    /// Last accepted non-zero handshake nonce (strict anti-replay).
    last_nonce: u64,
}

/// Builds a token validator that accepts only `expected`.
pub(crate) fn auth_validate_token(expected: String) -> AuthValidateFn {
    Arc::new(move |t: &str| t == expected.as_str())
}

/// Inner state shared between Connection and its background tasks.
pub(crate) struct ConnectionInner {
    /// Local node id (handshake).
    pub local_id: [u8; 16],
    pub local: String,
    pub remote: String,
    pub state: watch::Sender<ConnectionState>,
    /// Single-request response waiters.
    pub outgoing: OutgoingMap,
    /// Next mux_id.
    pub next_mux_id: std::sync::atomic::AtomicU64,
    /// Outgoing message queue (feeds write_task).
    pub out_queue: tokio::sync::mpsc::UnboundedSender<Message>,
    /// Receiver side of out_queue — taken by write_task on connect.
    pub out_rx: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Message>>>,
    /// Handler registry reference (from Manager).
    pub handlers: Arc<HandlerRegistry>,
    /// Timestamp of last pong received.
    pub last_pong: RwLock<Instant>,
    /// Whether this side initiated the connection.
    pub is_client: bool,
    /// Token sent in [`ConnectReq`] during handshake.
    pub auth_token: String,
    /// Validates the peer's connect token (same rules as [`crate::grid::Manager::validate_auth`]).
    pub auth_validate: AuthValidateFn,
    /// Anti-replay for [`ConnectReq`] (`nonce` monotonic when non-zero).
    pub connect_replay: Mutex<ConnectReplayState>,
    /// Monotonic nonce source for outbound [`ConnectReq`].
    pub next_handshake_nonce: AtomicU64,
    /// Client waits first [`Op::MuxServerMsg`] (or error) for [`Op::ConnectMux`] / `mux_request`.
    pub mux_first_wait: Mutex<HashMap<u64, oneshot::Sender<Result<Vec<u8>, RemoteErr>>>>,

    // ── Debug / fault-injection (only used under #[cfg(test)]) ──
    pub debug_kill_inbound: AtomicBool,
    pub debug_kill_outbound: AtomicBool,
    pub debug_block_inbound: AtomicBool,
    pub debug_add_deadline_ms: AtomicU64,
    pub debug_ping_interval_ms: AtomicU64,
    pub debug_exit_notify: tokio::sync::Notify,
}

/// A single WebSocket connection to a remote peer.
///
/// Cloning is cheap — wraps `Arc<ConnectionInner>`.
#[derive(Clone)]
pub struct Connection {
    pub(crate) inner: Arc<ConnectionInner>,
}

impl Connection {
    /// Create a new (unconnected) Connection handle.
    pub(crate) fn new(
        local_id: [u8; 16],
        local: String,
        remote: String,
        is_client: bool,
        auth_token: String,
        auth_validate: AuthValidateFn,
        handlers: Arc<HandlerRegistry>,
    ) -> Self {
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        let (state_tx, _state_rx) = watch::channel(ConnectionState::Unconnected);
        let inner = Arc::new(ConnectionInner {
            local_id,
            local,
            remote,
            state: state_tx,
            outgoing: Mutex::new(HashMap::new()),
            next_mux_id: std::sync::atomic::AtomicU64::new(1),
            out_queue: out_tx,
            out_rx: std::sync::Mutex::new(Some(out_rx)),
            handlers,
            last_pong: RwLock::new(Instant::now()),
            is_client,
            auth_token,
            auth_validate,
            connect_replay: Mutex::new(ConnectReplayState::default()),
            next_handshake_nonce: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64,
            ),
            mux_first_wait: Mutex::new(HashMap::new()),
            debug_kill_inbound: AtomicBool::new(false),
            debug_kill_outbound: AtomicBool::new(false),
            debug_block_inbound: AtomicBool::new(false),
            debug_add_deadline_ms: AtomicU64::new(0),
            debug_ping_interval_ms: AtomicU64::new(0),
            debug_exit_notify: tokio::sync::Notify::new(),
        });
        Connection { inner }
    }

    /// Local endpoint.
    pub fn local(&self) -> &str {
        &self.inner.local
    }

    /// Remote endpoint.
    pub fn remote(&self) -> &str {
        &self.inner.remote
    }

    /// Current connection state.
    pub async fn state(&self) -> ConnectionState {
        *self.inner.state.borrow()
    }

    /// Set connection state (notifies `watch` subscribers such as `write_task`).
    pub(crate) fn set_state(&self, s: ConnectionState) {
        let from = *self.inner.state.borrow();
        if !ConnectionState::allows_transition(from, s) {
            tracing::warn!(
                remote = %self.inner.remote,
                ?from,
                to = ?s,
                "grid connection state transition outside supported graph (applying anyway)",
            );
        }
        let _ = self.inner.state.send_if_modified(|cur| {
            if *cur == s {
                false
            } else {
                *cur = s;
                true
            }
        });
    }

    /// Take the out_rx for spawning write_task.
    #[allow(dead_code)]
    pub(crate) fn take_out_rx(&self) -> Option<tokio::sync::mpsc::UnboundedReceiver<Message>> {
        self.inner
            .out_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Spawn background tasks for a client-side WebSocket connection.
    ///
    /// Sets [`ConnectionState::Connecting`], starts read/write/ping tasks on `inner.state`,
    /// then sends [`Op::Connect`] with a [`ConnectReq`] payload. The peer must reply with
    /// [`Op::ConnectResponse`] before application requests are accepted on the wire.
    ///
    /// Returns [`GridError::WritePipelineActive`] if `write_task` was already started
    /// (`out_rx` was taken).
    #[allow(dead_code)]
    pub(crate) async fn spawn_client_tasks(
        &self,
        ws_stream: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> GridResult<()> {
        self.set_state(ConnectionState::Connecting);

        use futures_util::{SinkExt, StreamExt};
        let (ws_tx_sink, ws_rx_stream) = ws_stream.split();

        let Some(out_rx) = self.take_out_rx() else {
            tracing::warn!(
                remote = %self.inner.remote,
                "grid spawn_client_tasks: write pipeline already active"
            );
            self.set_state(ConnectionState::ConnectionError);
            return Err(GridError::WritePipelineActive);
        };
        let (sink_tx, mut sink_rx) = tokio::sync::mpsc::unbounded_channel::<tungstenite::Message>();
        tokio::spawn(async move {
            let mut ws_tx = ws_tx_sink;
            while let Some(msg) = sink_rx.recv().await {
                if ws_tx.send(msg).await.is_err() {
                    break;
                }
            }
        });

        let conn_write = self.clone();
        tokio::spawn(write_task(out_rx, sink_tx, conn_write));

        let conn_read = self.clone();
        tokio::spawn(read_task(ws_rx_stream, conn_read));

        let conn_ping = self.clone();
        tokio::spawn(ping_task(conn_ping));

        let req = build_connect_req(
            self.inner.local_id,
            &self.inner.local,
            &self.inner.auth_token,
            self.inner
                .next_handshake_nonce
                .fetch_add(1, Ordering::Relaxed),
        );
        match rmp_serde::to_vec(&req) {
            Ok(bytes) => {
                let mut msg = Message::new(Op::Connect, HANDLER_INVALID);
                msg.payload = Some(bytes);
                msg.set_zero_payload_flag();
                if self.inner.out_queue.send(msg).is_err() {
                    self.set_state(ConnectionState::ConnectionError);
                    self.drain_outgoing().await;
                    return Err(GridError::ConnectionClosed);
                }
            }
            Err(e) => {
                tracing::error!(%e, remote = %self.inner.remote, "failed to encode grid ConnectReq");
                self.set_state(ConnectionState::ConnectionError);
                self.drain_outgoing().await;
                return Err(GridError::Serialization(e.to_string()));
            }
        }
        Ok(())
    }

    /// Send a single request and wait for the response.
    ///
    /// Returns `Err(GridError::DeadlineExceeded)` on timeout,
    /// `Err(GridError::ConnectionClosed)` if disconnected,
    /// `Err(GridError::NotConnected)` if the connection is not in [`ConnectionState::Connected`].
    pub async fn request(
        &self,
        handler: HandlerId,
        payload: Option<Vec<u8>>,
        deadline: Option<Duration>,
    ) -> GridResult<Vec<u8>> {
        if !(*self.inner.state.borrow()).is_connected() {
            return Err(GridError::NotConnected);
        }

        let mut deadline = deadline.unwrap_or(DEFAULT_TIMEOUT);
        let extra_ms = self.inner.debug_add_deadline_ms.load(Ordering::SeqCst);
        if extra_ms > 0 {
            deadline += Duration::from_millis(extra_ms);
        }
        let mux_id = self
            .inner
            .next_mux_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let (tx, rx) = oneshot::channel();

        // Register the response waiter.
        {
            let mut outgoing = self.inner.outgoing.lock().await;
            outgoing.insert(mux_id, tx);
        }

        let mut msg = Message::new(Op::Request, handler);
        msg.mux_id = mux_id;
        msg.payload = payload;
        msg.set_zero_payload_flag();

        // Send to write queue. If the queue is closed, clean up.
        if self.inner.out_queue.send(msg).is_err() {
            let mut outgoing = self.inner.outgoing.lock().await;
            outgoing.remove(&mux_id);
            return Err(GridError::ConnectionClosed);
        }

        // Wait for response or timeout.
        let result = tokio::time::timeout(deadline, rx).await;
        match result {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(e))) => Err(GridError::Remote(e.msg)),
            Ok(Err(_)) => {
                // oneshot sender dropped → connection closed
                Err(GridError::ConnectionClosed)
            }
            Err(_) => {
                // Deadline exceeded → clean up waiter
                let mut outgoing = self.inner.outgoing.lock().await;
                outgoing.remove(&mux_id);
                Err(GridError::DeadlineExceeded)
            }
        }
    }

    /// One-shot multiplexed RPC: opens a mux, runs the registered single handler on the peer,
    /// returns the first [`Op::MuxServerMsg`] payload (or error).
    pub async fn mux_request(
        &self,
        handler: HandlerId,
        payload: Option<Vec<u8>>,
        deadline: Option<Duration>,
    ) -> GridResult<Vec<u8>> {
        if !(*self.inner.state.borrow()).is_connected() {
            return Err(GridError::NotConnected);
        }

        let mut deadline = deadline.unwrap_or(DEFAULT_TIMEOUT);
        let extra_ms = self.inner.debug_add_deadline_ms.load(Ordering::SeqCst);
        if extra_ms > 0 {
            deadline += Duration::from_millis(extra_ms);
        }

        let mux_id = self.inner.next_mux_id.fetch_add(1, Ordering::Relaxed);

        let (tx, rx) = oneshot::channel();
        {
            let mut w = self.inner.mux_first_wait.lock().await;
            w.insert(mux_id, tx);
        }

        let mut msg = Message::new(Op::ConnectMux, handler);
        msg.mux_id = mux_id;
        msg.payload = payload;
        msg.set_zero_payload_flag();

        if self.inner.out_queue.send(msg).is_err() {
            let mut w = self.inner.mux_first_wait.lock().await;
            w.remove(&mux_id);
            return Err(GridError::ConnectionClosed);
        }

        match tokio::time::timeout(deadline, rx).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(e))) => Err(GridError::Remote(e.msg)),
            Ok(Err(_)) => Err(GridError::ConnectionClosed),
            Err(_) => {
                let mut w = self.inner.mux_first_wait.lock().await;
                w.remove(&mux_id);
                Err(GridError::DeadlineExceeded)
            }
        }
    }

    /// Move from [`ConnectionState::ConnectionError`] to [`ConnectionState::Reconnecting`] before
    /// re-dialing WebSocket (caller should then drive a new handshake).
    pub async fn enter_reconnecting(&self) -> GridResult<()> {
        {
            let st = *self.inner.state.borrow();
            if st != ConnectionState::ConnectionError {
                return Err(GridError::InvalidState(
                    "expected ConnectionError before enter_reconnecting",
                ));
            }
        }
        self.set_state(ConnectionState::Reconnecting);
        let mut w = self.inner.mux_first_wait.lock().await;
        for (_, tx) in w.drain() {
            let _ = tx.send(Err(RemoteErr {
                msg: "reconnecting".to_string(),
            }));
        }
        Ok(())
    }

    /// Dispatch an incoming message to the appropriate handler.
    ///
    /// Called from `read_task`.
    pub(crate) async fn dispatch(&self, msg: Message) {
        match msg.op {
            Op::Response => self.handle_response(msg).await,
            Op::Request => self.handle_request(msg).await,
            Op::Ping => self.handle_ping(msg).await,
            Op::Pong => self.handle_pong(msg).await,
            Op::Connect => self.handle_connect(msg).await,
            Op::ConnectResponse => self.handle_connect_response(msg).await,
            Op::Disconnect => self.handle_disconnect(msg).await,
            Op::ConnectMux => self.handle_connect_mux(msg).await,
            Op::MuxServerMsg => self.handle_mux_server_msg(msg).await,
            Op::MuxConnectError => self.handle_peer_mux_connect_error(msg).await,
            Op::DisconnectServerMux => self.handle_disconnect_server_mux(msg).await,
            Op::DisconnectClientMux => self.handle_disconnect_client_mux(msg).await,
            Op::AckMux => { /* one-shot mux does not wait on Ack */ }
            Op::MuxClientMsg => self.handle_mux_client_msg(msg).await,
            Op::UnblockSrvMux | Op::UnblockClMux => {
                tracing::debug!(op = ?msg.op, mux_id = msg.mux_id, "grid unblock mux (no-op stub)");
            }
            Op::Invalid => {
                tracing::warn!(
                    remote = %self.inner.remote,
                    "received grid message with Op::Invalid (ignored)",
                );
            }
            _ => {
                tracing::warn!(op = ?msg.op, "unhandled grid message op");
            }
        }
    }

    /// Route a Response to the waiting oneshot.
    async fn handle_response(&self, msg: Message) {
        let mux_id = msg.mux_id;
        let mut outgoing = self.inner.outgoing.lock().await;
        if let Some(tx) = outgoing.remove(&mux_id) {
            if msg.is_error() {
                let err_msg = msg
                    .payload
                    .map(|p| String::from_utf8_lossy(&p).to_string())
                    .unwrap_or_else(|| "unknown remote error".to_string());
                let _ = tx.send(Err(RemoteErr { msg: err_msg }));
            } else {
                let _ = tx.send(Ok(msg.payload.unwrap_or_default()));
            }
        } else {
            tracing::debug!(mux_id, "grid response for unknown mux_id (timed out?)");
        }
    }

    /// Process an incoming Request: look up handler, spawn, send response.
    async fn handle_request(&self, msg: Message) {
        let handler_id = msg.handler;
        let mux_id = msg.mux_id;
        let payload = msg.payload.clone();
        let out_queue = self.inner.out_queue.clone();

        // Look up the handler.
        let handler = {
            let singles = self.inner.handlers.singles.read().await;
            singles.get(&handler_id).cloned()
        };

        let result = if let Some(handler) = handler {
            handler(payload.unwrap_or_default()).await
        } else {
            Err(RemoteErr {
                msg: format!("handler not found: {}", handler_id),
            })
        };

        let mut resp = Message::new(Op::Response, handler_id);
        resp.mux_id = mux_id;

        match result {
            Ok(data) => {
                resp.payload = Some(data);
                resp.set_zero_payload_flag();
            }
            Err(e) => {
                resp.flags.insert(Flags::PAYLOAD_IS_ERR);
                resp.payload = Some(e.msg.into_bytes());
            }
        }

        let _ = out_queue.send(resp);
    }

    /// Respond to a Ping with a Pong.
    async fn handle_ping(&self, msg: Message) {
        let mut pong = Message::new(Op::Pong, msg.handler);
        pong.mux_id = msg.mux_id;
        let _ = self.inner.out_queue.send(pong);
    }

    /// Update last_pong timestamp on Pong.
    async fn handle_pong(&self, _msg: Message) {
        *self.inner.last_pong.write().await = Instant::now();
    }

    /// Accept an incoming [`Op::Connect`] handshake.
    async fn handle_connect(&self, msg: Message) {
        {
            let st = *self.inner.state.borrow();
            if st == ConnectionState::Connected {
                self.fail_connect_handshake(
                    msg.mux_id,
                    "unexpected Connect while already connected",
                )
                .await;
                return;
            }
        }

        {
            let _ = self.inner.state.send_if_modified(|cur| {
                if *cur == ConnectionState::Unconnected {
                    *cur = ConnectionState::Connecting;
                    true
                } else {
                    false
                }
            });
        }

        let mux_id = msg.mux_id;

        let Some(raw) = msg.payload.as_ref() else {
            self.fail_connect_handshake(mux_id, "missing connect payload")
                .await;
            return;
        };

        let req: ConnectReq = match rmp_serde::from_slice(raw) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%e, remote = %self.inner.remote, "invalid grid ConnectReq");
                self.fail_connect_handshake(mux_id, "invalid connect payload")
                    .await;
                return;
            }
        };

        if !(self.inner.auth_validate)(req.token.as_str()) {
            self.fail_connect_handshake(mux_id, "auth failed").await;
            return;
        }

        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let mut g = self.inner.connect_replay.lock().await;
            if req.nonce != 0 {
                if req.nonce <= g.last_nonce {
                    drop(g);
                    self.fail_connect_handshake(mux_id, "connect nonce replay")
                        .await;
                    return;
                }
                g.last_nonce = req.nonce;
            } else if (now - req.time).abs() > CONNECT_CLOCK_SKEW_SECS {
                drop(g);
                self.fail_connect_handshake(mux_id, "connect timestamp skew")
                    .await;
                return;
            }
        }

        self.set_state(ConnectionState::Connected);
        self.send_connect_response(
            mux_id,
            ConnectResp {
                id: self.inner.local_id,
                accepted: true,
                rejected_reason: String::new(),
            },
        )
        .await;
        tracing::debug!(remote = %self.inner.remote, "grid connect accepted (server)");
    }

    async fn fail_connect_handshake(&self, mux_id: u64, reason: &str) {
        tracing::warn!(remote = %self.inner.remote, reason = %reason, "grid connect rejected");
        self.send_connect_response(
            mux_id,
            ConnectResp {
                id: self.inner.local_id,
                accepted: false,
                rejected_reason: reason.to_string(),
            },
        )
        .await;
        self.set_state(ConnectionState::ConnectionError);
        self.drain_outgoing().await;
    }

    /// Client-side handling of [`Op::ConnectResponse`].
    async fn handle_connect_response(&self, msg: Message) {
        let Some(raw) = msg.payload.as_ref() else {
            tracing::warn!(remote = %self.inner.remote, "ConnectResponse without payload");
            self.set_state(ConnectionState::ConnectionError);
            self.drain_outgoing().await;
            return;
        };

        let resp: ConnectResp = match rmp_serde::from_slice(raw) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%e, remote = %self.inner.remote, "invalid grid ConnectResp");
                self.set_state(ConnectionState::ConnectionError);
                self.drain_outgoing().await;
                return;
            }
        };

        if !resp.accepted {
            tracing::warn!(
                remote = %self.inner.remote,
                reason = %resp.rejected_reason,
                "grid connect rejected by peer",
            );
            self.set_state(ConnectionState::ConnectionError);
            self.drain_outgoing().await;
            return;
        }

        self.set_state(ConnectionState::Connected);
        tracing::debug!(remote = %self.inner.remote, "grid connect accepted (client)");
    }

    async fn send_connect_response(&self, mux_id: u64, resp: ConnectResp) {
        let payload = match rmp_serde::to_vec(&resp) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(%e, remote = %self.inner.remote, "encode ConnectResp failed");
                return;
            }
        };
        let mut out = Message::new(Op::ConnectResponse, HANDLER_INVALID);
        out.mux_id = mux_id;
        out.payload = Some(payload);
        out.set_zero_payload_flag();
        let _ = self.inner.out_queue.send(out);
    }

    /// Handle peer disconnect.
    async fn handle_disconnect(&self, _msg: Message) {
        tracing::warn!(remote = %self.inner.remote, "grid peer disconnected");
        self.set_state(ConnectionState::ConnectionError);
        self.drain_outgoing().await;
    }

    async fn handle_connect_mux(&self, msg: Message) {
        let mux_id = msg.mux_id;
        let handler = msg.handler;
        let payload = msg.payload.clone().unwrap_or_default();

        let handler_fn = {
            let r = self.inner.handlers.singles.read().await;
            r.get(&handler).cloned()
        };
        let Some(handler_fn) = handler_fn else {
            let mut m = Message::new(Op::MuxConnectError, HANDLER_INVALID);
            m.mux_id = mux_id;
            if let Ok(b) = rmp_serde::to_vec(&crate::grid::msg_types::MuxConnectError {
                error: format!("handler not found: {handler}"),
            }) {
                m.payload = Some(b);
            }
            let _ = self.inner.out_queue.send(m);
            return;
        };

        let mut ack = Message::new(Op::AckMux, handler);
        ack.mux_id = mux_id;
        let _ = self.inner.out_queue.send(ack);

        let out_q = self.inner.out_queue.clone();
        tokio::spawn(async move {
            let res = handler_fn(payload).await;
            match res {
                Ok(data) => {
                    let mut m = Message::new(Op::MuxServerMsg, handler);
                    m.mux_id = mux_id;
                    m.payload = Some(data);
                    m.set_zero_payload_flag();
                    let _ = out_q.send(m);
                }
                Err(e) => {
                    let mut m = Message::new(Op::MuxConnectError, HANDLER_INVALID);
                    m.mux_id = mux_id;
                    if let Ok(b) =
                        rmp_serde::to_vec(&crate::grid::msg_types::MuxConnectError { error: e.msg })
                    {
                        m.payload = Some(b);
                    }
                    let _ = out_q.send(m);
                }
            }
            let mut d = Message::new(Op::DisconnectServerMux, HANDLER_INVALID);
            d.mux_id = mux_id;
            let _ = out_q.send(d);
        });
    }

    async fn handle_mux_server_msg(&self, msg: Message) {
        let mux_id = msg.mux_id;
        let mut w = self.inner.mux_first_wait.lock().await;
        if let Some(tx) = w.remove(&mux_id) {
            let data = msg.payload.unwrap_or_default();
            let _ = tx.send(Ok(data));
        }
    }

    async fn handle_peer_mux_connect_error(&self, msg: Message) {
        let mux_id = msg.mux_id;
        let err_str = msg
            .payload
            .as_ref()
            .map(|p| {
                rmp_serde::from_slice::<crate::grid::msg_types::MuxConnectError>(p)
                    .map(|e| e.error)
                    .unwrap_or_else(|_| String::from_utf8_lossy(p).into_owned())
            })
            .unwrap_or_else(|| "mux connect failed".to_string());
        let mut w = self.inner.mux_first_wait.lock().await;
        if let Some(tx) = w.remove(&mux_id) {
            let _ = tx.send(Err(RemoteErr { msg: err_str }));
        }
    }

    async fn handle_disconnect_server_mux(&self, msg: Message) {
        let mux_id = msg.mux_id;
        let mut w = self.inner.mux_first_wait.lock().await;
        if let Some(tx) = w.remove(&mux_id) {
            let _ = tx.send(Err(RemoteErr {
                msg: "mux closed by server".to_string(),
            }));
        }
    }

    async fn handle_disconnect_client_mux(&self, msg: Message) {
        tracing::debug!(
            mux_id = msg.mux_id,
            remote = %self.inner.remote,
            "grid DisconnectClientMux"
        );
    }

    async fn handle_mux_client_msg(&self, msg: Message) {
        tracing::warn!(
            mux_id = msg.mux_id,
            handler = msg.handler,
            remote = %self.inner.remote,
            "MuxClientMsg on server (streaming driver not implemented; ignored)",
        );
    }

    /// Notify all pending request waiters that the connection is lost.
    pub(crate) async fn drain_outgoing(&self) {
        let mut outgoing = self.inner.outgoing.lock().await;
        for (_, tx) in outgoing.drain() {
            let _ = tx.send(Err(RemoteErr {
                msg: "connection closed".to_string(),
            }));
        }
        let mut mux = self.inner.mux_first_wait.lock().await;
        for (_, tx) in mux.drain() {
            let _ = tx.send(Err(RemoteErr {
                msg: "connection closed".to_string(),
            }));
        }
    }

    // ── Connection topology ──────────────────────────────────

    /// Determine which side initiates the WebSocket connection.
    ///
    /// Both peers compute this independently and arrive at opposite results,
    /// preventing duplicate connections.
    pub fn should_connect(local: &str, remote: &str) -> bool {
        use xxhash_rust::xxh3::Xxh3;
        let mut hasher = Xxh3::new();
        hasher.update(local.as_bytes());
        hasher.update(remote.as_bytes());
        let h0 = hasher.digest();

        let mut hasher = Xxh3::new();
        hasher.update(remote.as_bytes());
        hasher.update(local.as_bytes());
        let h1 = hasher.digest();

        h0 < h1
    }
}

// ── Debug / fault-injection ─────────────────────────────────

impl Connection {
    /// Inject a fault-injection command (only meaningful under `#[cfg(test)]`).
    #[cfg(test)]
    pub(crate) fn debug_msg(&self, msg: DebugMsg) {
        match msg {
            DebugMsg::Shutdown => {
                self.inner.debug_kill_inbound.store(true, Ordering::SeqCst);
                self.inner.debug_kill_outbound.store(true, Ordering::SeqCst);
            }
            DebugMsg::KillInbound => {
                self.inner.debug_kill_inbound.store(true, Ordering::SeqCst);
            }
            DebugMsg::KillOutbound => {
                self.inner.debug_kill_outbound.store(true, Ordering::SeqCst);
            }
            DebugMsg::BlockInboundMessages(block) => {
                self.inner
                    .debug_block_inbound
                    .store(block, Ordering::SeqCst);
            }
            DebugMsg::WaitForExit => {}
            DebugMsg::SetClientPingDuration(ms) => {
                self.inner
                    .debug_ping_interval_ms
                    .store(ms, Ordering::SeqCst);
            }
            DebugMsg::SetConnPingDuration(ms) => {
                self.inner
                    .debug_ping_interval_ms
                    .store(ms, Ordering::SeqCst);
            }
            DebugMsg::AddToDeadline(ms) => {
                self.inner
                    .debug_add_deadline_ms
                    .fetch_add(ms, Ordering::SeqCst);
            }
            DebugMsg::IsOutgoingClosed => {}
        }
    }

    /// Wait for background tasks to exit (with timeout).
    #[cfg(test)]
    pub(crate) async fn debug_wait_for_exit(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, self.inner.debug_exit_notify.notified())
            .await
            .is_ok()
    }

    /// Notify that a background task has exited.
    pub(crate) fn debug_notify_exit(&self) {
        self.inner.debug_exit_notify.notify_one();
    }
}

// ── Background tasks ────────────────────────────────────────

/// Build a connect request message.
pub(crate) fn build_connect_req(
    local_id: [u8; 16],
    host: &str,
    token: &str,
    nonce: u64,
) -> ConnectReq {
    ConnectReq {
        id: local_id,
        host: host.to_string(),
        time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        token: token.to_string(),
        nonce,
    }
}

/// Write task: drains out_queue and writes to the WebSocket sink.
///
/// Merges consecutive messages up to `MAX_MERGED_MSGS` into an `Op::Merged` message.
pub async fn write_task(
    mut out_rx: tokio::sync::mpsc::UnboundedReceiver<Message>,
    ws_tx: tokio::sync::mpsc::UnboundedSender<tungstenite::Message>,
    conn: Connection,
) {
    let mut buf = Vec::with_capacity(64);
    let mut merged: Vec<Message> = Vec::with_capacity(MAX_MERGED_MSGS);
    let mut pending: VecDeque<Message> = VecDeque::new();
    let mut state_watch = conn.inner.state.subscribe();

    loop {
        if conn.inner.debug_kill_outbound.load(Ordering::SeqCst) {
            conn.debug_notify_exit();
            return;
        }

        let first = if let Some(m) = pending.pop_front() {
            m
        } else {
            match out_rx.recv().await {
                Some(msg) => msg,
                None => {
                    conn.debug_notify_exit();
                    return;
                }
            }
        };

        let mut blocked_logged = false;
        loop {
            let st = *state_watch.borrow();
            if st.terminates_write_path() {
                pending.push_front(first);
                drop_pending_and_rx(&mut pending, &mut out_rx, &conn, "terminal writer state");
                conn.debug_notify_exit();
                return;
            }
            if st.allows_outgoing_wire() {
                break;
            }
            if !blocked_logged {
                tracing::warn!(
                    remote = %conn.inner.remote,
                    op = ?first.op,
                    mux_id = first.mux_id,
                    handler = first.handler,
                    "grid write deferred: connection state not wire-ready",
                );
                blocked_logged = true;
            }
            if state_watch.changed().await.is_err() {
                pending.push_front(first);
                drop_pending_and_rx(&mut pending, &mut out_rx, &conn, "state watch closed");
                conn.debug_notify_exit();
                return;
            }
        }

        merged.clear();
        merged.push(first);
        while merged.len() < MAX_MERGED_MSGS {
            if let Some(m) = pending.pop_front() {
                merged.push(m);
                continue;
            }
            match out_rx.try_recv() {
                Ok(msg) => merged.push(msg),
                Err(_) => break,
            }
        }

        let encoded: Result<Vec<u8>, rmp_serde::encode::Error> = (|| {
            if merged.len() == 1 {
                return merged[0].encode();
            }
            buf.clear();
            for m in &merged {
                let part = m.encode()?;
                let len = part.len() as u32;
                buf.extend_from_slice(&len.to_be_bytes());
                buf.extend_from_slice(&part);
            }
            let mut merge_msg = Message::new(Op::Merged, HANDLER_INVALID);
            merge_msg.payload = Some(std::mem::take(&mut buf));
            merge_msg.encode()
        })();

        let data = match encoded {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(%e, remote = %conn.inner.remote, "grid message encode failed");
                conn.set_state(ConnectionState::ConnectionError);
                conn.drain_outgoing().await;
                conn.debug_notify_exit();
                return;
            }
        };

        if ws_tx.send(tungstenite::Message::Binary(data)).is_err() {
            tracing::warn!(remote = %conn.inner.remote, "grid websocket send failed");
            conn.set_state(ConnectionState::ConnectionError);
            conn.drain_outgoing().await;
            conn.debug_notify_exit();
            return;
        }
    }
}

fn drop_pending_and_rx(
    pending: &mut VecDeque<Message>,
    out_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Message>,
    conn: &Connection,
    reason: &'static str,
) {
    let n = pending.len();
    pending.clear();
    let mut drained = 0usize;
    while out_rx.try_recv().is_ok() {
        drained += 1;
    }
    if n > 0 || drained > 0 {
        tracing::warn!(
            remote = %conn.inner.remote,
            dropped_pending = n,
            drained_queue = drained,
            reason,
            "discarding unsent grid messages",
        );
    }
}

/// Decode one WebSocket payload as grid MessagePack. Returns `false` when the read loop should stop.
async fn decode_and_dispatch_grid_payload(
    data: &[u8],
    conn: &Connection,
    from_text_frame: bool,
) -> bool {
    if data.len() > MAX_GRID_WIRE_BYTES {
        tracing::warn!(
            remote = %conn.inner.remote,
            len = data.len(),
            max = MAX_GRID_WIRE_BYTES,
            "grid websocket payload exceeds decode limit",
        );
        conn.set_state(ConnectionState::ConnectionError);
        conn.drain_outgoing().await;
        return false;
    }
    match Message::decode(data) {
        Ok(msg) if msg.op == Op::Merged => match &msg.payload {
            Some(payload) => {
                if let Err(e) = dispatch_merged(payload, conn).await {
                    tracing::error!(%e, remote = %conn.inner.remote, "invalid merged grid payload");
                    conn.set_state(ConnectionState::ConnectionError);
                    conn.drain_outgoing().await;
                    return false;
                }
            }
            None => {
                tracing::warn!(remote = %conn.inner.remote, "merged grid message missing payload");
                conn.set_state(ConnectionState::ConnectionError);
                conn.drain_outgoing().await;
                return false;
            }
        },
        Ok(msg) => {
            conn.dispatch(msg).await;
        }
        Err(e) => {
            if from_text_frame {
                tracing::debug!(%e, remote = %conn.inner.remote, "ignored websocket text (not grid msgpack)");
            } else {
                tracing::error!(%e, remote = %conn.inner.remote, "failed to decode grid message");
            }
        }
    }
    true
}

/// Read task: reads from the WebSocket stream and dispatches messages.
pub async fn read_task(
    mut ws_rx: impl futures_util::Stream<Item = Result<tungstenite::Message, tungstenite::Error>>
    + Unpin,
    conn: Connection,
) {
    use futures_util::StreamExt;

    while let Some(ws_msg) = ws_rx.next().await {
        if conn.inner.debug_kill_inbound.load(Ordering::SeqCst) {
            break;
        }

        match ws_msg {
            Ok(tungstenite::Message::Binary(data)) => {
                if conn.inner.debug_block_inbound.load(Ordering::SeqCst) {
                    continue;
                }
                if !decode_and_dispatch_grid_payload(&data, &conn, false).await {
                    break;
                }
            }
            Ok(tungstenite::Message::Text(t)) => {
                if conn.inner.debug_block_inbound.load(Ordering::SeqCst) {
                    continue;
                }
                if !decode_and_dispatch_grid_payload(t.as_bytes(), &conn, true).await {
                    break;
                }
            }
            Ok(tungstenite::Message::Ping(_data)) => {
                let _ = conn
                    .inner
                    .out_queue
                    .send(Message::new(Op::Pong, HANDLER_INVALID));
            }
            Ok(tungstenite::Message::Close(_)) => {
                tracing::warn!(remote = %conn.inner.remote, "grid websocket closed by peer");
                break;
            }
            Err(e) => {
                tracing::error!(%e, remote = %conn.inner.remote, "grid websocket read error");
                break;
            }
            _ => {}
        }
    }

    conn.set_state(ConnectionState::ConnectionError);
    conn.drain_outgoing().await;
    conn.debug_notify_exit();
}

/// Dispatch messages inside a Merged payload.
///
/// Merged payload format: repeated (4-byte BE length prefix + encoded Message).
async fn dispatch_merged(data: &[u8], conn: &Connection) -> Result<(), GridError> {
    let mut remaining = data;
    while remaining.len() >= 4 {
        let len =
            u32::from_be_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]) as usize;
        if len > MAX_GRID_WIRE_BYTES {
            return Err(GridError::PayloadTooLarge {
                max: MAX_GRID_WIRE_BYTES,
                got: len,
            });
        }
        remaining = &remaining[4..];
        if remaining.len() < len {
            return Err(GridError::Serialization(format!(
                "merged grid payload truncated: need {len} bytes, have {}",
                remaining.len()
            )));
        }
        let slice = &remaining[..len];
        remaining = &remaining[len..];
        let msg = Message::decode(slice).map_err(|e| GridError::Serialization(e.to_string()))?;
        conn.dispatch(msg).await;
    }
    if !remaining.is_empty() {
        return Err(GridError::Serialization(format!(
            "merged grid payload has {} trailing bytes",
            remaining.len()
        )));
    }
    Ok(())
}

/// Ping task: periodically sends pings and checks for pong timeouts.
pub async fn ping_task(conn: Connection) {
    let interval_ms = conn.inner.debug_ping_interval_ms.load(Ordering::SeqCst);
    let ping_dur = if interval_ms > 0 {
        Duration::from_millis(interval_ms)
    } else {
        PING_INTERVAL
    };
    let mut interval = tokio::time::interval(ping_dur);
    loop {
        interval.tick().await;

        // Check debug kill flags.
        if conn.inner.debug_kill_outbound.load(Ordering::SeqCst)
            || conn.inner.debug_kill_inbound.load(Ordering::SeqCst)
        {
            conn.debug_notify_exit();
            return;
        }

        if !conn.inner.is_client {
            continue;
        }

        if !(*conn.inner.state.borrow()).is_connected() {
            continue;
        }

        let elapsed = conn.inner.last_pong.read().await.elapsed();
        if elapsed > PONG_TIMEOUT {
            tracing::warn!(
                remote = %conn.inner.remote,
                ?elapsed,
                "grid pong timeout, disconnecting",
            );
            conn.set_state(ConnectionState::ConnectionError);
            conn.drain_outgoing().await;
            conn.debug_notify_exit();
            return;
        }

        // Send a ping.
        let ping = Message::new(Op::Ping, HANDLER_INVALID);
        let _ = conn.inner.out_queue.send(ping);
    }
}
