use thiserror::Error;

/// Grid-specific error types.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GridError {
    #[error("connection closed")]
    ConnectionClosed,

    #[error("connection is not in Connected state")]
    NotConnected,

    #[error("grid write pipeline already active for this connection")]
    WritePipelineActive,

    #[error("connection timeout")]
    ConnectionTimeout,

    #[error("handler not found: {0}")]
    HandlerNotFound(u8),

    #[error("stream not found: {0}")]
    StreamNotFound(u64),

    #[error("remote error: {0}")]
    Remote(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("invalid operation: {0}")]
    InvalidOperation(u8),

    #[error("invalid connection state: {0}")]
    InvalidState(&'static str),

    #[error("grid payload too large (max {max} bytes, got {got})")]
    PayloadTooLarge { max: usize, got: usize },

    #[error("mux connect error: {0}")]
    MuxConnectError(String),

    #[error("deadline exceeded")]
    DeadlineExceeded,

    #[error("cancelled")]
    Cancelled,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type GridResult<T> = Result<T, GridError>;

/// A remote error received from the peer.
#[derive(Debug, Clone)]
pub struct RemoteErr {
    pub msg: String,
}

impl std::fmt::Display for RemoteErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for RemoteErr {}

impl From<RemoteErr> for GridError {
    fn from(e: RemoteErr) -> Self {
        GridError::Remote(e.msg)
    }
}
