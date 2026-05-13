/// Connection lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionState {
    Unconnected,
    Connecting,
    Connected,
    Reconnecting,
    ConnectionError,
}

impl ConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, ConnectionState::Connected | ConnectionState::Reconnecting)
    }

    /// Whether the wire write loop may send application frames (handshake or active).
    pub fn allows_outgoing_wire(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting | ConnectionState::Connected | ConnectionState::Reconnecting
        )
    }

    /// Whether the write task should exit and drop queued frames (no further wire I/O).
    pub fn terminates_write_path(&self) -> bool {
        matches!(
            self,
            ConnectionState::ConnectionError | ConnectionState::Unconnected
        )
    }

    /// Returns `true` when `from → to` is part of the supported lifecycle graph.
    ///
    /// Extra edges (e.g. tests forcing `Unconnected → Connected`) are allowed so
    /// harness code can simulate the wire without a full dial/handshake.
    pub fn allows_transition(from: Self, to: Self) -> bool {
        if from == to {
            return true;
        }
        use ConnectionState::*;
        matches!(
            (from, to),
            // Bootstrap / client dial
            (Unconnected, Connecting | Connected | ConnectionError)
                // Handshake completion or failure
                | (Connecting, Connected | ConnectionError | Reconnecting | Unconnected)
                // Normal operation
                | (Connected, ConnectionError | Reconnecting)
                // Recovery
                | (ConnectionError, Connecting | Unconnected | Reconnecting)
                | (Reconnecting, Connecting | Connected | ConnectionError | Unconnected)
        )
    }

    /// If `from → to` is allowed, returns `Ok(to)`; otherwise `Err(())`.
    pub fn transition(from: Self, to: Self) -> Result<Self, ()> {
        if Self::allows_transition(from, to) {
            Ok(to)
        } else {
            Err(())
        }
    }
}
