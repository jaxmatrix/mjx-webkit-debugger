//! Protocol-level failure.
//!
//! [`ProtocolError`] is the error *the debuggee sent us*, so it is
//! `Serialize + Deserialize`: it travels on the wire inside a
//! [`crate::Frame::Error`]. Failures on our side of the socket belong to the
//! transport and session crates instead.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A convenience alias for results carrying a [`ProtocolError`].
pub type Result<T, E = ProtocolError> = std::result::Result<T, E>;

/// An error reported by the debuggee in reply to a command.
///
/// The `code` values follow JSON-RPC's conventions loosely; WebKit uses
/// `-32000` for most "you asked for something I can't do" cases and does not
/// document the full set, so match on [`ProtocolError::kind`] rather than on
/// raw numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct ProtocolError {
    /// The numeric code. See [`ErrorKind`] for the ones worth branching on.
    pub code: i64,
    /// Human-readable text, straight from the debuggee.
    pub message: String,
    /// Optional structured detail. WebKit sends an array of strings here when
    /// it sends anything at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ProtocolError {
    /// Classify the error into something worth branching on.
    pub fn kind(&self) -> ErrorKind {
        match self.code {
            -32700 => ErrorKind::ParseError,
            -32600 => ErrorKind::InvalidRequest,
            -32601 => ErrorKind::MethodNotFound,
            -32602 => ErrorKind::InvalidParams,
            -32603 => ErrorKind::InternalError,
            -32000 => ErrorKind::ServerError,
            _ => ErrorKind::Other,
        }
    }

    /// Whether the debuggee does not implement what we asked for.
    ///
    /// True for a domain or member this build does not have. Callers use this
    /// to degrade a panel to "unsupported" instead of surfacing an error — a
    /// newer debugger talking to an older WebKit is an ordinary situation.
    pub fn is_unsupported(&self) -> bool {
        matches!(self.kind(), ErrorKind::MethodNotFound)
    }
}

/// The coarse category of a [`ProtocolError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Our JSON was malformed.
    ParseError,
    /// The frame was not a valid request.
    InvalidRequest,
    /// The domain or member does not exist on this debuggee.
    MethodNotFound,
    /// The parameters did not match the member's signature.
    InvalidParams,
    /// The debuggee hit an internal failure.
    InternalError,
    /// The command was well-formed but could not be carried out.
    ServerError,
    /// An undocumented code.
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_not_found_reads_as_unsupported() {
        // The degrade-gracefully path: an older WebKit lacking a member must
        // grey a panel out, not raise an error at the user.
        let err = ProtocolError {
            code: -32601,
            message: "'Debugger.setPauseOnMicrotasks' was not found".into(),
            data: None,
        };
        assert_eq!(err.kind(), ErrorKind::MethodNotFound);
        assert!(err.is_unsupported());
    }

    #[test]
    fn an_ordinary_server_error_is_not_unsupported() {
        let err = ProtocolError {
            code: -32000,
            message: "Breakpoint for given location already exists.".into(),
            data: None,
        };
        assert_eq!(err.kind(), ErrorKind::ServerError);
        assert!(!err.is_unsupported());
    }

    #[test]
    fn round_trips_through_the_wire_without_a_data_member() {
        let err = ProtocolError {
            code: -32000,
            message: "no".into(),
            data: None,
        };
        let text = serde_json::to_string(&err).unwrap();
        assert_eq!(text, r#"{"code":-32000,"message":"no"}"#);
        assert_eq!(serde_json::from_str::<ProtocolError>(&text).unwrap(), err);
    }
}
