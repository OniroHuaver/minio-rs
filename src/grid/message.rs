use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// Handler ID — identifies which handler processes a message.
pub type HandlerId = u8;

/// Invalid handler ID sentinel.
pub const HANDLER_INVALID: HandlerId = 0;

/// Operation codes for grid messages.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Op {
    /// Sentinel / uninitialized — must not appear on the wire as a meaningful request.
    Invalid = 0,
    Connect = 1,
    ConnectResponse = 2,
    Ping = 3,
    Pong = 4,
    ConnectMux = 5,
    MuxConnectError = 6,
    DisconnectClientMux = 7,
    DisconnectServerMux = 8,
    MuxClientMsg = 9,
    MuxServerMsg = 10,
    UnblockSrvMux = 11,
    UnblockClMux = 12,
    AckMux = 13,
    Request = 14,
    Response = 15,
    Disconnect = 16,
    Merged = 17,
}

impl TryFrom<u8> for Op {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Op::Invalid),
            1 => Ok(Op::Connect),
            2 => Ok(Op::ConnectResponse),
            3 => Ok(Op::Ping),
            4 => Ok(Op::Pong),
            5 => Ok(Op::ConnectMux),
            6 => Ok(Op::MuxConnectError),
            7 => Ok(Op::DisconnectClientMux),
            8 => Ok(Op::DisconnectServerMux),
            9 => Ok(Op::MuxClientMsg),
            10 => Ok(Op::MuxServerMsg),
            11 => Ok(Op::UnblockSrvMux),
            12 => Ok(Op::UnblockClMux),
            13 => Ok(Op::AckMux),
            14 => Ok(Op::Request),
            15 => Ok(Op::Response),
            16 => Ok(Op::Disconnect),
            17 => Ok(Op::Merged),
            _ => Err("Invalid Op type"),
        }
    }
}

bitflags! {
    /// Per-message flags set on the wire.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct Flags: u8 {
        /// Lower 32 bits of xxh3 of the serialized message will be sent.
        const CRCXXH3         = 1 << 0;
        /// The stream (either direction) is at EOF.
        const EOF             = 1 << 1;
        /// The message is stateless.
        const STATELESS       = 1 << 2;
        /// Payload is a string error converted to byte slice.
        const PAYLOAD_IS_ERR  = 1 << 3;
        /// Payload is a 0-length slice (not None/nil).
        const PAYLOAD_IS_ZERO = 1 << 4;
        /// The message carries a subroute.
        const SUBROUTE        = 1 << 5;
    }
}

/// The core wire message.
///
/// All fields are serialized with MessagePack (rmp-serde).
/// Payload is `Option<Vec<u8>>` — `None` means no payload,
/// `Some([])` means 0-length payload (distinguished by `PAYLOAD_IS_ZERO` flag).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub mux_id: u64,
    pub seq: u32,
    pub deadline_ms: u32,
    pub handler: HandlerId,
    pub op: Op,
    pub flags: Flags,
    #[serde(with = "serde_bytes")]
    pub payload: Option<Vec<u8>>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            mux_id: 0,
            seq: 0,
            deadline_ms: 0,
            handler: HANDLER_INVALID,
            op: Op::Invalid,
            flags: Flags::empty(),
            payload: None,
        }
    }
}

impl Message {
    /// Create a new outgoing message with the given op and handler.
    pub fn new(op: Op, handler: HandlerId) -> Self {
        Self {
            op,
            handler,
            ..Default::default()
        }
    }

    /// Set the `PAYLOAD_IS_ZERO` flag based on payload content.
    pub fn set_zero_payload_flag(&mut self) {
        self.flags.remove(Flags::PAYLOAD_IS_ZERO);
        if let Some(ref payload) = self.payload
            && payload.is_empty()
        {
            self.flags.insert(Flags::PAYLOAD_IS_ZERO);
        }
    }

    /// Serialize to MessagePack bytes.
    pub fn encode(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }

    /// Deserialize from MessagePack bytes.
    pub fn decode(data: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(data)
    }

    /// Check if this message is a request (Request op).
    pub fn is_request(&self) -> bool {
        self.op == Op::Request
    }

    /// Check if this message is a response (Response op).
    pub fn is_response(&self) -> bool {
        self.op == Op::Response
    }

    /// Check if payload signals an error.
    pub fn is_error(&self) -> bool {
        self.flags.contains(Flags::PAYLOAD_IS_ERR)
    }
}
