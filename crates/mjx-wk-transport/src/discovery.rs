//! Finding inspectable targets.
//!
//! # WebKitGTK's inspector server is not an HTTP server
//!
//! This is the single most surprising thing about it if you arrive from Chrome,
//! and it is also not what the WebKit *documentation* leads you to expect.
//! There is no `/json/list`, and — verified against WebKitGTK 2.52.3 — there is
//! no HTTP endpoint at all. Connecting and sending `GET / HTTP/1.1` gets the
//! connection closed.
//!
//! What listens on `WEBKIT_INSPECTOR_SERVER` is a **length-prefixed JSON socket
//! protocol** (`RemoteInspectorSocketEndpoint`). The *"Inspectable targets"*
//! HTML page that appears in WebKit's binary is generated **client-side**, by
//! the inspecting browser's `inspector://` scheme handler, from a target list it
//! received over this socket. It is never served to anyone.
//!
//! ## Framing
//!
//! Each message is a 4-byte **big-endian** length followed by that many bytes of
//! JSON. From `RemoteInspectorMessageParser.cpp`, which uses `htonl`:
//!
//! ```text
//! +--------+---------------------------+
//! |  size  |          payload          | (next message)
//! | 4 bytes|      `size` bytes         |
//! +--------+---------------------------+
//! ```
//!
//! Little-endian is not merely wrong, it is *quietly* wrong: the server reads an
//! enormous length, rejects it as invalid, and closes without a word.
//!
//! ## Messages
//!
//! From `Source/WebKit/UIProcess/Inspector/socket/RemoteInspectorClient.cpp` at
//! the pinned ref.
//!
//! Client → server:
//!
//! ```json
//! {"event": "SetupInspectorClient"}
//! {"event": "Setup",                "connectionID": 1, "targetID": 2}
//! {"event": "SendMessageToBackend",  "connectionID": 1, "targetID": 2, "message": "…"}
//! {"event": "FrontendDidClose",      "connectionID": 1, "targetID": 2}
//! ```
//!
//! Server → client:
//!
//! ```json
//! {"event": "BackendCommands",       "backendCommands": "…"}
//! {"event": "SetTargetList",         "connectionID": 1, "targetList": [
//!    {"targetID": 2, "name": "…", "url": "…", "type": "web-page"}]}
//! {"event": "SendMessageToFrontend", "connectionID": 1, "targetID": 2, "message": "…"}
//! ```
//!
//! `message` carries the **inspector protocol frame as a string**, which is why
//! [`crate::Transport`]'s seam — one JSON string in each direction — sits
//! exactly where it does. The envelope is this crate's business; nothing above
//! sees it.
//!
//! ## Unfinished: the handshake does not complete
//!
//! **Sending `SetupInspectorClient` with correct framing produces no reply**
//! against MiniBrowser 2.52.3 started with
//! `--enable-developer-extras=true`, even though the socket stays open and the
//! server is listening. Something further is required before the server emits
//! `SetTargetList` — most likely a registration step on the debuggable side, or
//! an ordering constraint not visible in `RemoteInspectorClient.cpp` alone.
//!
//! Settling this is [`docs/tasks/T-000-inspector-handshake.md`], and it blocks
//! T-001 and T-002 and the live fixture corpus. It is recorded here rather than
//! guessed at, because a plausible-looking wrong handshake is worse than none.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Target, TransportError};

/// How to reach a specific target again.
///
/// Opaque on purpose: this backend puts a `connectionID`/`targetID` pair here,
/// an Apple backend puts an application/page identifier pair, and nothing above
/// this crate should care which.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetKey(pub String);

impl TargetKey {
    /// Build a key for the socket protocol's two-part address.
    pub fn from_ids(connection_id: u64, target_id: u64) -> Self {
        Self(format!("{connection_id}/{target_id}"))
    }

    /// Recover the two ids, if this key came from [`TargetKey::from_ids`].
    pub fn as_ids(&self) -> Option<(u64, u64)> {
        let (c, t) = self.0.split_once('/')?;
        Some((c.parse().ok()?, t.parse().ok()?))
    }
}

impl std::fmt::Display for TargetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One row of a target list, before a connection is opened.
pub type TargetDescriptor = Target;

/// Enumerates what can be attached to.
#[async_trait]
pub trait Discovery: Send + Sync + std::fmt::Debug {
    /// List every currently inspectable target.
    ///
    /// An empty list is a normal answer — the debuggee may have been started
    /// without developer extras enabled — and must not be reported as an error.
    async fn list(&self) -> Result<Vec<TargetDescriptor>, TransportError>;

    /// A human-readable description of where this is looking, for error
    /// messages and the target picker's header.
    fn endpoint(&self) -> String;
}

/// One message of the inspector socket protocol.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Serialised with the 4-byte big-endian length prefix described above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum SocketEvent {
    // ---- client → server ----
    SetupInspectorClient,
    Setup {
        #[serde(rename = "connectionID")]
        connection_id: u64,
        #[serde(rename = "targetID")]
        target_id: u64,
    },
    SendMessageToBackend {
        #[serde(rename = "connectionID")]
        connection_id: u64,
        #[serde(rename = "targetID")]
        target_id: u64,
        /// An inspector protocol frame, as a string.
        message: String,
    },
    FrontendDidClose {
        #[serde(rename = "connectionID")]
        connection_id: u64,
        #[serde(rename = "targetID")]
        target_id: u64,
    },

    // ---- server → client ----
    /// The generated protocol description this build speaks. We already have
    /// our own generated types, so this is used only to detect version drift.
    BackendCommands {
        #[serde(rename = "backendCommands")]
        backend_commands: String,
    },
    SetTargetList {
        #[serde(rename = "connectionID")]
        connection_id: u64,
        #[serde(rename = "targetList")]
        target_list: Vec<SocketTarget>,
    },
    SendMessageToFrontend {
        #[serde(rename = "connectionID")]
        connection_id: u64,
        #[serde(rename = "targetID")]
        target_id: u64,
        message: String,
    },
}

/// One entry of a `SetTargetList`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SocketTarget {
    #[serde(rename = "targetID")]
    pub target_id: u64,
    pub name: String,
    pub url: String,
    /// `"web-page"`, `"javascript"`, `"service-worker"`, …
    #[serde(rename = "type")]
    pub kind: String,
}

/// Frame a message for the wire.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Big-endian length, then the JSON. Must reject a payload larger than
/// `u32::MAX` rather than truncating the length.
pub fn encode_message(_event: &SocketEvent) -> Result<Vec<u8>, TransportError> {
    todo!("T-001")
}

/// Pull complete messages out of a receive buffer.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Returns the decoded messages and leaves any partial tail in `buffer`. A
/// `BackendCommands` payload is tens of kilobytes and will routinely arrive
/// split across several reads, so a parser that assumes one message per read
/// works right up until it does not.
pub fn decode_messages(_buffer: &mut Vec<u8>) -> Result<Vec<SocketEvent>, TransportError> {
    todo!("T-001")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_key_round_trips_through_its_two_ids() {
        let key = TargetKey::from_ids(1, 42);
        assert_eq!(key.to_string(), "1/42");
        assert_eq!(key.as_ids(), Some((1, 42)));
    }

    #[test]
    fn a_key_from_another_backend_has_no_socket_ids() {
        // Apple and Android backends put their own addressing in here; asking
        // for socket ids must fail rather than parse nonsense.
        assert_eq!(TargetKey("com.example.app/page-3".into()).as_ids(), None);
    }

    #[test]
    fn socket_events_use_the_protocols_exact_field_spellings() {
        // `connectionID` and `targetID`, not snake_case and not `connectionId`.
        // A rename typo here fails silently: the server ignores the message.
        let json = serde_json::to_string(&SocketEvent::Setup {
            connection_id: 1,
            target_id: 2,
        })
        .unwrap();
        assert!(json.contains(r#""event":"Setup""#), "{json}");
        assert!(json.contains(r#""connectionID":1"#), "{json}");
        assert!(json.contains(r#""targetID":2"#), "{json}");
    }

    #[test]
    fn a_target_list_decodes_from_the_servers_shape() {
        let raw = r#"{"event":"SetTargetList","connectionID":1,
            "targetList":[{"targetID":2,"name":"Page","url":"http://x/","type":"web-page"}]}"#;
        let SocketEvent::SetTargetList { target_list, .. } = serde_json::from_str(raw).unwrap()
        else {
            panic!("expected a target list");
        };
        assert_eq!(target_list[0].target_id, 2);
        assert_eq!(target_list[0].kind, "web-page");
    }
}
