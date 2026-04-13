use rmpv::Value;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Kind discriminants (§2 of protocol/spec.md)
// ---------------------------------------------------------------------------

/// Message kind byte values as defined in the protocol specification.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Req = 0,
    Rep = 1,
    Err = 2,
    Pub = 3,
    Oneshot = 4,
    Sub = 5,
    Unsub = 6,
}

impl TryFrom<u8> for Kind {
    type Error = ProtocolError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Kind::Req),
            1 => Ok(Kind::Rep),
            2 => Ok(Kind::Err),
            3 => Ok(Kind::Pub),
            4 => Ok(Kind::Oneshot),
            5 => Ok(Kind::Sub),
            6 => Ok(Kind::Unsub),
            _ => Err(ProtocolError::UnknownKind(v)),
        }
    }
}

// ---------------------------------------------------------------------------
// Standard error codes (§4 of protocol/spec.md — all 11 codes)
// ---------------------------------------------------------------------------

/// All 11 standard error codes from protocol spec §4.
/// Phase 1 uses only the first four; the remaining five are reserved for future phases.
pub const E_PROTO_VERSION: &str = "E_PROTO_VERSION";
pub const E_PROTO_MALFORMED: &str = "E_PROTO_MALFORMED";
pub const E_HANDSHAKE_TIMEOUT: &str = "E_HANDSHAKE_TIMEOUT";
pub const E_TOPIC_UNKNOWN: &str = "E_TOPIC_UNKNOWN";
#[allow(dead_code)]
pub const E_ACTION_UNKNOWN: &str = "E_ACTION_UNKNOWN";
#[allow(dead_code)]
pub const E_ACTION_PAYLOAD: &str = "E_ACTION_PAYLOAD";
pub const E_SERVICE_INTERNAL: &str = "E_SERVICE_INTERNAL";
pub const E_SERVICE_UNAVAILABLE: &str = "E_SERVICE_UNAVAILABLE";
#[allow(dead_code)]
pub const E_PERMISSION: &str = "E_PERMISSION";
#[allow(dead_code)]
pub const E_RATE_LIMITED: &str = "E_RATE_LIMITED";
#[allow(dead_code)]
pub const E_CANCELED: &str = "E_CANCELED";

/// Expected protocol version string that clients must present in `Hello`.
pub const PROTO_VERSION: &str = "qsov/1";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Common envelope wrapping all post-handshake messages (§2).
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    /// Raw `Kind` discriminant byte; use [`Kind::try_from`] to decode.
    pub kind: u8,
    pub topic: String,
    #[serde(default)]
    pub action: String,
    /// Arbitrary msgpack payload; schema determined by `(topic, action)`.
    #[serde(default = "default_payload")]
    pub payload: Value,
}

fn default_payload() -> Value {
    Value::Nil
}

/// Client → Server handshake initiation message (§3.1).
#[derive(Debug, Deserialize)]
pub struct Hello {
    pub proto_version: String,
    pub client_name: String,
    pub client_version: String,
}

/// Server → Client handshake acknowledgement (§3.2).
#[derive(Debug, Serialize)]
pub struct HelloAck {
    pub server_version: String,
    pub capabilities: Vec<String>,
    pub session_id: u64,
}

/// Structured error payload embedded in `ERR` envelopes and pre-handshake errors (§4).
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

// ---------------------------------------------------------------------------
// Protocol error type
// ---------------------------------------------------------------------------

/// Errors arising from encoding or decoding protocol messages.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("msgpack encode error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("msgpack decode error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("unknown kind byte: {0}")]
    UnknownKind(u8),
    /// Reserved for future envelope validation; not triggered in Phase 1.
    #[allow(dead_code)]
    #[error("envelope missing required field: {0}")]
    MissingField(&'static str),
}
