//! The envelope every protocol message rides in.
//!
//! WebKit's inspector wire format is JSON-RPC-shaped but not JSON-RPC: there is
//! no `"jsonrpc": "2.0"` member, and the four message kinds are told apart by
//! which fields are present rather than by a tag. So the classification is
//! written out by hand rather than derived with `#[serde(untagged)]`, which
//! would silently pick the first arm that happens to deserialize.
//!
//! ```text
//! request   {"id":1,"method":"Debugger.enable","params":{}}
//! response  {"id":1,"result":{}}
//! error     {"id":1,"error":{"code":-32000,"message":"Not enabled"}}
//! event     {"method":"Debugger.scriptParsed","params":{…}}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::ProtocolError;

/// Correlates a request with its reply.
///
/// Monotonic per connection, allocated by the session. A response carrying an
/// id nobody is waiting for is a protocol violation on the debuggee's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(pub u64);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One decoded protocol message.
///
/// Params and results stay as [`Value`] here. Turning them into domain types is
/// the caller's job, because only the caller knows which [`crate::Command`] a
/// response belongs to.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// Debugger → debuggee.
    Request {
        id: RequestId,
        /// Qualified, e.g. `"Debugger.setBreakpointByUrl"`.
        method: String,
        params: Value,
    },
    /// Debuggee → debugger, in reply to a request that succeeded.
    Response { id: RequestId, result: Value },
    /// Debuggee → debugger, in reply to a request that failed.
    Error { id: RequestId, error: ProtocolError },
    /// Debuggee → debugger, unsolicited.
    Event {
        /// Qualified, e.g. `"Debugger.paused"`.
        method: String,
        params: Value,
    },
}

impl Frame {
    /// The request id, for the three kinds that carry one.
    ///
    /// [`Frame::Event`] has none — that is what makes it an event.
    pub fn id(&self) -> Option<RequestId> {
        match self {
            Frame::Request { id, .. } | Frame::Response { id, .. } | Frame::Error { id, .. } => {
                Some(*id)
            }
            Frame::Event { .. } => None,
        }
    }

    /// The qualified method name, for the two kinds that carry one.
    pub fn method(&self) -> Option<&str> {
        match self {
            Frame::Request { method, .. } | Frame::Event { method, .. } => Some(method),
            Frame::Response { .. } | Frame::Error { .. } => None,
        }
    }

    /// Split a qualified method into its domain and member halves.
    ///
    /// Returns `None` for a response or error, or for a malformed method name.
    pub fn split_method(&self) -> Option<(&str, &str)> {
        self.method()?.split_once('.')
    }

    /// Decode one message from the wire.
    pub fn from_json(text: &str) -> Result<Self, FrameError> {
        let raw: RawFrame = serde_json::from_str(text)?;
        raw.classify()
    }

    /// Encode this message for the wire.
    pub fn to_json(&self) -> Result<String, FrameError> {
        let value = match self {
            Frame::Request { id, method, params } => {
                json!({ "id": id, "method": method, "params": params })
            }
            Frame::Response { id, result } => json!({ "id": id, "result": result }),
            Frame::Error { id, error } => json!({ "id": id, "error": error }),
            Frame::Event { method, params } => json!({ "method": method, "params": params }),
        };
        Ok(serde_json::to_string(&value)?)
    }
}

/// The shape a frame has before we decide which kind it is.
#[derive(Deserialize)]
struct RawFrame {
    id: Option<RequestId>,
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ProtocolError>,
}

impl RawFrame {
    fn classify(self) -> Result<Frame, FrameError> {
        // Order matters. `error` is checked before `result` because a failed
        // reply may legitimately carry both, and the error is what the caller
        // must see. `method` is checked last so that a response is never
        // mistaken for a request.
        match (self.id, self.method, self.error, self.result) {
            (Some(id), _, Some(error), _) => Ok(Frame::Error { id, error }),
            (Some(id), None, None, result) => Ok(Frame::Response {
                id,
                result: result.unwrap_or(Value::Null),
            }),
            (Some(id), Some(method), None, None) => Ok(Frame::Request {
                id,
                method,
                params: self.params.unwrap_or_else(|| json!({})),
            }),
            (None, Some(method), None, None) => Ok(Frame::Event {
                method,
                params: self.params.unwrap_or_else(|| json!({})),
            }),
            // A reply that carries both an id and a method, or a message with
            // neither, is not something this protocol defines.
            (id, method, _, _) => Err(FrameError::Unclassifiable {
                has_id: id.is_some(),
                has_method: method.is_some(),
            }),
        }
    }
}

/// A message could not be decoded from, or encoded to, the wire.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The bytes were not JSON at all.
    #[error("frame is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The JSON parsed, but matched none of the four message shapes.
    #[error("frame matches no known message shape (id: {has_id}, method: {has_method})")]
    Unclassifiable { has_id: bool, has_method: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_four_message_shapes() {
        let request =
            Frame::from_json(r#"{"id":1,"method":"Debugger.enable","params":{}}"#).unwrap();
        assert!(matches!(request, Frame::Request { .. }));
        assert_eq!(request.id(), Some(RequestId(1)));
        assert_eq!(request.split_method(), Some(("Debugger", "enable")));

        let response = Frame::from_json(r#"{"id":1,"result":{"scriptSource":"x"}}"#).unwrap();
        assert!(matches!(response, Frame::Response { .. }));
        assert_eq!(response.method(), None);

        let error = Frame::from_json(r#"{"id":2,"error":{"code":-32000,"message":"no"}}"#).unwrap();
        assert!(matches!(error, Frame::Error { .. }));

        let event = Frame::from_json(r#"{"method":"Debugger.paused","params":{}}"#).unwrap();
        assert!(matches!(event, Frame::Event { .. }));
        assert_eq!(event.id(), None);
    }

    #[test]
    fn a_reply_carrying_both_error_and_result_is_an_error() {
        // Seen in the wild when a command partially applies. The caller must be
        // told it failed, not handed a half-built result.
        let frame =
            Frame::from_json(r#"{"id":3,"result":{},"error":{"code":-1,"message":"x"}}"#).unwrap();
        assert!(matches!(frame, Frame::Error { .. }));
    }

    #[test]
    fn a_response_with_no_result_member_decodes_as_null() {
        // Commands that return nothing reply `{"id":N}` on some builds and
        // `{"id":N,"result":{}}` on others. Both must decode.
        let frame = Frame::from_json(r#"{"id":4}"#).unwrap();
        assert_eq!(
            frame,
            Frame::Response {
                id: RequestId(4),
                result: Value::Null
            }
        );
    }

    #[test]
    fn rejects_messages_that_match_no_shape() {
        assert!(matches!(
            Frame::from_json(r#"{"params":{}}"#),
            Err(FrameError::Unclassifiable { .. })
        ));
        assert!(matches!(
            Frame::from_json("not json"),
            Err(FrameError::Json(_))
        ));
    }

    #[test]
    fn round_trips_through_the_wire() {
        for text in [
            r#"{"id":1,"method":"Debugger.enable","params":{}}"#,
            r#"{"method":"Debugger.paused","params":{"reason":"Breakpoint"}}"#,
        ] {
            let frame = Frame::from_json(text).unwrap();
            let reparsed = Frame::from_json(&frame.to_json().unwrap()).unwrap();
            assert_eq!(frame, reparsed);
        }
    }
}
