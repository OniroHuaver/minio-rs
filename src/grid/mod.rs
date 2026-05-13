//! Grid — distributed RPC communication layer.
//!
//! WebSocket-based long-lived connections with message multiplexing.
//! Carries all internal RPC: distributed locks, storage operations,
//! cluster management, and S3 peer operations.
//!
//! ## Architecture
//!
//! ```text
//! Manager → Connection pool ─→ WebSocket (tokio-tungstenite)
//!         → Handler registry (single + stream)
//!
//! Connection → read_task  (incoming messages → dispatch)
//!           → write_task (outgoing messages → merged writes)
//!           → ping_task  (heartbeat / keepalive)
//! ```
//!
//! ## Message flow
//!
//! **Single request**: `conn.Request(handler, payload)` → Op::Request →
//!   server processes → Op::Response (with same mux_id).
//!
//! **Stream**: `conn.NewStream(handler, payload)` → Op::ConnectMux →
//!   bidirectional mpsc channels, cancel propagates to server.

pub mod connection;
pub mod connection_state;

/// Validates a peer-supplied auth token (shared secret or custom policy).
pub type AuthValidateFn = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;
#[cfg(test)]
pub mod debug;
pub mod error;
pub mod handler;
pub mod manager;
pub mod message;
pub mod msg_types;

pub use connection::Connection;
pub use connection_state::ConnectionState;
pub use error::{GridError, GridResult, RemoteErr};
pub use handler::{SingleHandler, SingleHandlerFn, single_handler_fn};
pub use manager::{Manager, ManagerOptions};
/// Backwards-compatible alias for [`AuthValidateFn`].
pub type AuthFn = AuthValidateFn;
pub use message::{Flags, HANDLER_INVALID, HandlerId, Message, Op};
pub use msg_types::{
    Bytes, ConnectReq, ConnectResp, MSS, MuxConnectError, PongMsg, TestRequest, TestResponse,
};

#[cfg(test)]
mod tests;

/// Grid RPC node information.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub endpoint: String,
    pub online: bool,
}
