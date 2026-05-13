//! Debug / fault-injection messages for grid connection testing.
//!
//! Only compiled under `#[cfg(test)]`. Inject failures into Connection
//! via `conn.debug_msg(msg)` to verify timeout, disconnect, and error
//! recovery paths without real network faults.

/// Fault-injection commands sent to a Connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMsg {
    /// Gracefully shut down background tasks.
    Shutdown,
    /// Kill inbound processing — read_task stops dispatching.
    KillInbound,
    /// Kill outbound processing — write_task stops sending.
    KillOutbound,
    /// Block (buffer) inbound messages instead of dispatching.
    BlockInboundMessages(bool),
    /// Wait for background task exit signal.
    WaitForExit,
    /// Override client ping interval (millis).
    SetClientPingDuration(u64),
    /// Override connection-level ping interval (millis).
    SetConnPingDuration(u64),
    /// Add extra millis to every request deadline.
    AddToDeadline(u64),
    /// Query whether the outbound channel is closed.
    IsOutgoingClosed,
}
