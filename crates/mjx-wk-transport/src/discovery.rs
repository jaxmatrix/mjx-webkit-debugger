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
//! What listens on `WEBKIT_INSPECTOR_SERVER` on GTK/WPE is a GLib
//! `SocketConnection` protocol (`RemoteInspectorServer`). The *"Inspectable
//! targets"* HTML page that appears in WebKit's binary is generated
//! **client-side**, by the inspecting browser's `inspector://` scheme handler,
//! from a target list it received over this socket. It is never served to
//! anyone.
//!
//! ## Framing
//!
//! Each message is WTF `SocketConnection` framing (`SocketConnection.cpp`):
//!
//! ```text
//! +--------+-------+------------------+---------------------------+
//! |  size  | flags | name\0           | GVariant body             |
//! | 4 bytes| 1 byte| NUL-terminated   | `size - len(name) - 1` B  |
//! | (BE)   |       |                  |                           |
//! +--------+-------+------------------+---------------------------+
//! ```
//!
//! `size` is the body length (name + NUL + GVariant), **big-endian** (`htonl`).
//! Flags bit 0 is `ByteOrderLittleEndian`; Linux WebKitGTK always sets it.
//! Little-endian *size* is quietly wrong: the server reads an enormous length
//! and closes.
//!
//! Do **not** confuse this with the PlayStation length-prefixed JSON dialect in
//! `UIProcess/Inspector/socket/RemoteInspectorClient.cpp` — sending that at
//! WebKitGTK produces no reply.
//!
//! ## Messages
//!
//! From `Source/WebKit/UIProcess/Inspector/glib/RemoteInspectorClient.cpp`.
//!
//! Client → server:
//!
//! | Message | Parameters |
//! |---|---|
//! | `SetupInspectorClient` | `(ay)` — SHA-1 hex digest of `InspectorBackendCommands.js` |
//! | `Setup` | `(tt)` connectionID, targetID |
//! | `SendMessageToBackend` | `(tts)` connectionID, targetID, frame JSON |
//! | `FrontendDidClose` | `(tt)` |
//!
//! Server → client:
//!
//! | Message | Parameters |
//! |---|---|
//! | `DidSetupInspectorClient` | `(ay)` — backend-commands script, or empty when digests match |
//! | `SetTargetList` | `(ta(tsssb))` — connectionID + `(targetID, type, name, url, hasLocalDebugger)` |
//! | `SendMessageToFrontend` | `(tts)` |
//! | `DidClose` | none |
//!
//! `type` is glib spelling (`WebPage` / `JavaScript` / `ServiceWorker`). The
//! inspector-protocol frame still travels as a **string** inside those
//! GVariants, which is why [`crate::Transport`]'s seam — one JSON string in
//! each direction — sits exactly where it does.

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;

use async_trait::async_trait;
#[cfg(unix)]
use glib::prelude::*;
#[cfg(unix)]
use glib::{Variant, VariantTy};
use mjx_wk_dialect::DialectKind;
use mjx_wk_protocol::TargetType;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use sha1::{Digest, Sha1};

use crate::{Target, TransportError, TransportOrigin};

/// Why WebKitGTK inspector TCP is unavailable on non-Unix platforms.
#[cfg(not(unix))]
pub(crate) const WEBKITGTK_TCP_UNAVAILABLE: &str =
    "WebKitGTK inspector TCP (GLib SocketConnection) is unavailable on this platform; \
     use ReplayTransport for offline fixtures (CDP attach is planned for Windows)";

/// Flag bit: GVariant payload is little-endian. Linux WebKitGTK always sets it.
pub const BYTE_ORDER_LITTLE_ENDIAN: u8 = 1 << 0;

/// Refuse to allocate a body larger than this. WTF allows 512 MB;
/// `DidSetupInspectorClient` is ~70 KB.
pub const MAX_MESSAGE_BODY_SIZE: usize = 16 * 1024 * 1024;

#[cfg(unix)]
const BACKEND_COMMANDS_PATH: &str =
    "/org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js";

/// Default shared library used to hash `InspectorBackendCommands.js`.
#[cfg(unix)]
pub fn default_webkit_library() -> PathBuf {
    PathBuf::from("/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0")
}

/// Default shared library used to hash `InspectorBackendCommands.js`.
#[cfg(not(unix))]
pub fn default_webkit_library() -> PathBuf {
    PathBuf::new()
}

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

/// One message of the WebKitGTK inspector socket protocol.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Serialised with GLib `SocketConnection` framing (BE length + flags + name +
/// GVariant), not the PlayStation JSON dialect.
#[derive(Debug, Clone, PartialEq)]
pub enum SocketEvent {
    // ---- client → server ----
    /// SHA-1 hex digest of the client's `InspectorBackendCommands.js`, as bytes.
    SetupInspectorClient {
        backend_commands_hash: Vec<u8>,
    },
    Setup {
        connection_id: u64,
        target_id: u64,
    },
    SendMessageToBackend {
        connection_id: u64,
        target_id: u64,
        /// An inspector protocol frame, as a string.
        message: String,
    },
    FrontendDidClose {
        connection_id: u64,
        target_id: u64,
    },

    // ---- server → client ----
    /// Backend-commands script when digests differ; empty when they match.
    DidSetupInspectorClient {
        backend_commands: Vec<u8>,
    },
    SetTargetList {
        connection_id: u64,
        target_list: Vec<SocketTarget>,
    },
    SendMessageToFrontend {
        connection_id: u64,
        target_id: u64,
        message: String,
    },
    /// The far end closed the inspector connection.
    DidClose,
}

/// One entry of a `SetTargetList`.
#[derive(Debug, Clone, PartialEq)]
pub struct SocketTarget {
    pub target_id: u64,
    pub name: String,
    pub url: String,
    /// Glib spelling: `"WebPage"`, `"JavaScript"`, `"ServiceWorker"`, …
    pub kind: String,
}

/// Map a glib `SetTargetList` `type` string onto our [`TargetType`].
pub fn target_type_from_glib(kind: &str) -> Option<TargetType> {
    match kind {
        "WebPage" => Some(TargetType::WebPage),
        "JavaScript" => Some(TargetType::JavaScript),
        "ServiceWorker" => Some(TargetType::ServiceWorker),
        _ => None,
    }
}

/// Turn a decoded `SetTargetList` into attachable descriptors, preserving order
/// and skipping unknown target kinds.
pub fn descriptors_from_target_list(
    connection_id: u64,
    targets: &[SocketTarget],
    origin: TransportOrigin,
) -> Vec<TargetDescriptor> {
    targets
        .iter()
        .filter_map(|t| {
            let kind = target_type_from_glib(&t.kind)?;
            Some(Target {
                key: TargetKey::from_ids(connection_id, t.target_id),
                name: t.name.clone(),
                url: t.url.clone(),
                kind,
                dialect: DialectKind::WebKitRwi,
                origin: origin.clone(),
            })
        })
        .collect()
}

/// SHA-1 hex digest (ASCII bytes) of `InspectorBackendCommands.js` inside a
/// WebKit shared library.
#[cfg(unix)]
pub fn backend_commands_hash(library: &Path) -> Result<Vec<u8>, TransportError> {
    let output = Command::new("gresource")
        .args([
            "extract",
            &library.display().to_string(),
            BACKEND_COMMANDS_PATH,
        ])
        .output()
        .map_err(|e| TransportError::Discovery {
            endpoint: library.display().to_string(),
            reason: format!(
                "running `gresource extract` for InspectorBackendCommands.js \
                 (install glib2 tooling, or pass a webkit library): {e}"
            ),
        })?;
    if !output.status.success() {
        return Err(TransportError::Discovery {
            endpoint: library.display().to_string(),
            reason: format!(
                "gresource extract failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    let digest = Sha1::digest(&output.stdout);
    Ok(hex::encode(digest).into_bytes())
}

/// SHA-1 hex digest (ASCII bytes) of `InspectorBackendCommands.js` inside a
/// WebKit shared library.
#[cfg(not(unix))]
pub fn backend_commands_hash(library: &Path) -> Result<Vec<u8>, TransportError> {
    let _ = library;
    Err(TransportError::Discovery {
        endpoint: "WebKitGTK".into(),
        reason: WEBKITGTK_TCP_UNAVAILABLE.into(),
    })
}

/// Frame a message for the wire.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Big-endian body length, little-endian payload flag, then name NUL + GVariant.
/// Rejects a payload larger than [`u32::MAX`] rather than truncating.
#[cfg(unix)]
pub fn encode_message(event: &SocketEvent) -> Result<Vec<u8>, TransportError> {
    let (name, parameters) = event_to_variant(event)?;
    let mut body = Vec::new();
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    if let Some(parameters) = parameters {
        body.extend_from_slice(parameters.data());
    }
    let len = u32::try_from(body.len()).map_err(|_| {
        TransportError::Malformed("SocketConnection message larger than u32::MAX".into())
    })?;
    let mut out = Vec::with_capacity(4 + 1 + body.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.push(BYTE_ORDER_LITTLE_ENDIAN);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Frame a message for the wire.
#[cfg(not(unix))]
pub fn encode_message(_event: &SocketEvent) -> Result<Vec<u8>, TransportError> {
    Err(TransportError::Malformed(format!(
        "{WEBKITGTK_TCP_UNAVAILABLE}: cannot encode SocketConnection messages"
    )))
}

/// Pull complete messages out of a receive buffer.
///
/// **Owned by `docs/tasks/T-001-target-discovery.md`.**
///
/// Returns the decoded messages and leaves any partial tail in `buffer`. A
/// `DidSetupInspectorClient` payload is tens of kilobytes and will routinely
/// arrive split across several reads.
#[cfg(unix)]
pub fn decode_messages(buffer: &mut Vec<u8>) -> Result<Vec<SocketEvent>, TransportError> {
    let mut events = Vec::new();
    while let Some(event) = take_one_message(buffer)? {
        events.push(event);
    }
    Ok(events)
}

/// Pull complete messages out of a receive buffer.
#[cfg(not(unix))]
pub fn decode_messages(_buffer: &mut Vec<u8>) -> Result<Vec<SocketEvent>, TransportError> {
    Err(TransportError::Malformed(format!(
        "{WEBKITGTK_TCP_UNAVAILABLE}: cannot decode SocketConnection messages"
    )))
}

#[cfg(unix)]
fn take_one_message(buffer: &mut Vec<u8>) -> Result<Option<SocketEvent>, TransportError> {
    if buffer.len() < 4 {
        return Ok(None);
    }
    let mut size_bytes = [0u8; 4];
    size_bytes.copy_from_slice(&buffer[..4]);
    let body_size = u32::from_be_bytes(size_bytes) as usize;
    if body_size < 2 {
        return Err(TransportError::Malformed(
            "inspector server sent a body smaller than a message name".into(),
        ));
    }
    if body_size > MAX_MESSAGE_BODY_SIZE {
        return Err(TransportError::Malformed(format!(
            "inspector server announced a {body_size}-byte body; refusing to allocate"
        )));
    }

    let total = 4 + 1 + body_size;
    if buffer.len() < total {
        return Ok(None);
    }

    let flags = buffer[4];
    if flags & BYTE_ORDER_LITTLE_ENDIAN == 0 {
        return Err(TransportError::Malformed(
            "inspector server sent a big-endian SocketConnection message".into(),
        ));
    }
    let body = buffer[5..total].to_vec();
    buffer.drain(..total);

    let nul = body.iter().position(|&b| b == 0).ok_or_else(|| {
        TransportError::Malformed("SocketConnection message name is not NUL-terminated".into())
    })?;
    let name = std::str::from_utf8(&body[..nul]).map_err(|_| {
        TransportError::Malformed("SocketConnection message name is not UTF-8".into())
    })?;
    let payload = &body[nul + 1..];
    Ok(Some(variant_to_event(name, payload)?))
}

#[cfg(unix)]
fn parse_payload(type_str: &str, payload: &[u8]) -> Result<Variant, TransportError> {
    let ty = VariantTy::new(type_str).map_err(|e| {
        TransportError::Malformed(format!("invalid GVariant type `{type_str}`: {e}"))
    })?;
    Ok(Variant::from_data_with_type(payload, ty))
}

#[cfg(unix)]
fn event_to_variant(
    event: &SocketEvent,
) -> Result<(&'static str, Option<Variant>), TransportError> {
    Ok(match event {
        SocketEvent::SetupInspectorClient {
            backend_commands_hash,
        } => {
            let ty = VariantTy::new("(ay)").map_err(|e| {
                TransportError::Malformed(format!("invalid GVariant type `(ay)`: {e}"))
            })?;
            let params =
                Variant::from_data_with_type(bytestring_tuple_payload(backend_commands_hash), ty);
            ("SetupInspectorClient", Some(params))
        }
        SocketEvent::Setup {
            connection_id,
            target_id,
        } => ("Setup", Some((*connection_id, *target_id).to_variant())),
        SocketEvent::SendMessageToBackend {
            connection_id,
            target_id,
            message,
        } => (
            "SendMessageToBackend",
            Some((*connection_id, *target_id, message.as_str()).to_variant()),
        ),
        SocketEvent::FrontendDidClose {
            connection_id,
            target_id,
        } => (
            "FrontendDidClose",
            Some((*connection_id, *target_id).to_variant()),
        ),
        SocketEvent::DidSetupInspectorClient { backend_commands } => {
            let ty = VariantTy::new("(ay)").map_err(|e| {
                TransportError::Malformed(format!("invalid GVariant type `(ay)`: {e}"))
            })?;
            let params =
                Variant::from_data_with_type(bytestring_tuple_payload(backend_commands), ty);
            ("DidSetupInspectorClient", Some(params))
        }
        SocketEvent::SetTargetList {
            connection_id,
            target_list,
        } => {
            let entries: Vec<(u64, String, String, String, bool)> = target_list
                .iter()
                .map(|t| {
                    (
                        t.target_id,
                        t.kind.clone(),
                        t.name.clone(),
                        t.url.clone(),
                        false,
                    )
                })
                .collect();
            (
                "SetTargetList",
                Some((*connection_id, entries).to_variant()),
            )
        }
        SocketEvent::SendMessageToFrontend {
            connection_id,
            target_id,
            message,
        } => (
            "SendMessageToFrontend",
            Some((*connection_id, *target_id, message.as_str()).to_variant()),
        ),
        SocketEvent::DidClose => ("DidClose", None),
    })
}

#[cfg(unix)]
fn variant_to_event(name: &str, payload: &[u8]) -> Result<SocketEvent, TransportError> {
    match name {
        "DidSetupInspectorClient" => {
            let variant = parse_payload("(ay)", payload)?;
            let backend_commands = bytestring_from_ay_tuple(&variant)?;
            Ok(SocketEvent::DidSetupInspectorClient { backend_commands })
        }
        "SetTargetList" => {
            let variant = parse_payload("(ta(tsssb))", payload)?;
            let (connection_id, target_list) = parse_target_list(&variant)?;
            Ok(SocketEvent::SetTargetList {
                connection_id,
                target_list,
            })
        }
        "SendMessageToFrontend" => {
            let variant = parse_payload("(tts)", payload)?;
            let (connection_id, target_id, message) = parse_tts(&variant)?;
            Ok(SocketEvent::SendMessageToFrontend {
                connection_id,
                target_id,
                message,
            })
        }
        "DidClose" => {
            if !payload.is_empty() {
                return Err(TransportError::Malformed(
                    "DidClose carries unexpected parameters".into(),
                ));
            }
            Ok(SocketEvent::DidClose)
        }
        // Client→server names are not received on a live socket, but decoding
        // them keeps round-trip tests honest.
        "SetupInspectorClient" => {
            let variant = parse_payload("(ay)", payload)?;
            Ok(SocketEvent::SetupInspectorClient {
                backend_commands_hash: bytestring_from_ay_tuple(&variant)?,
            })
        }
        "Setup" => {
            let variant = parse_payload("(tt)", payload)?;
            let (connection_id, target_id) = parse_tt(&variant)?;
            Ok(SocketEvent::Setup {
                connection_id,
                target_id,
            })
        }
        "SendMessageToBackend" => {
            let variant = parse_payload("(tts)", payload)?;
            let (connection_id, target_id, message) = parse_tts(&variant)?;
            Ok(SocketEvent::SendMessageToBackend {
                connection_id,
                target_id,
                message,
            })
        }
        "FrontendDidClose" => {
            let variant = parse_payload("(tt)", payload)?;
            let (connection_id, target_id) = parse_tt(&variant)?;
            Ok(SocketEvent::FrontendDidClose {
                connection_id,
                target_id,
            })
        }
        other => Err(TransportError::Malformed(format!(
            "unexpected SocketConnection message `{other}`"
        ))),
    }
}

#[cfg(unix)]
/// GVariant serialization of `(ay)` for a bytestring.
///
/// When `(ay)` is the whole value, the bytestring is `data` + NUL — the parent
/// size implies the array length (fixed-width `y` elements).
fn bytestring_tuple_payload(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 1);
    out.extend_from_slice(data);
    out.push(0);
    out
}

#[cfg(unix)]
fn bytestring_from_ay_tuple(variant: &Variant) -> Result<Vec<u8>, TransportError> {
    let child = variant
        .try_child_value(0)
        .ok_or_else(|| TransportError::Malformed("`(ay)` tuple missing bytestring child".into()))?;
    // GLib bytestrings are `ay` with a trailing NUL that is not part of the
    // logical content. `data_as_bytes` includes it; strip one trailing zero.
    let bytes = child.data_as_bytes();
    let slice: &[u8] = bytes.as_ref();
    if let Some((last, rest)) = slice.split_last()
        && *last == 0
    {
        return Ok(rest.to_vec());
    }
    Ok(slice.to_vec())
}

#[cfg(unix)]
fn parse_tt(variant: &Variant) -> Result<(u64, u64), TransportError> {
    let a = variant
        .try_child_value(0)
        .and_then(|v| v.get::<u64>())
        .ok_or_else(|| TransportError::Malformed("expected u64 connectionID".into()))?;
    let b = variant
        .try_child_value(1)
        .and_then(|v| v.get::<u64>())
        .ok_or_else(|| TransportError::Malformed("expected u64 targetID".into()))?;
    Ok((a, b))
}

#[cfg(unix)]
fn parse_tts(variant: &Variant) -> Result<(u64, u64, String), TransportError> {
    let (a, b) = parse_tt(variant)?;
    let message = variant
        .try_child_value(2)
        .and_then(|v| v.get::<String>())
        .ok_or_else(|| TransportError::Malformed("expected string message".into()))?;
    Ok((a, b, message))
}

#[cfg(unix)]
fn parse_target_list(variant: &Variant) -> Result<(u64, Vec<SocketTarget>), TransportError> {
    let connection_id = variant
        .try_child_value(0)
        .and_then(|v| v.get::<u64>())
        .ok_or_else(|| TransportError::Malformed("SetTargetList missing connectionID".into()))?;
    let array = variant
        .try_child_value(1)
        .ok_or_else(|| TransportError::Malformed("SetTargetList missing target array".into()))?;

    let mut targets = Vec::with_capacity(array.n_children());
    for child in array.iter() {
        let target_id = child
            .try_child_value(0)
            .and_then(|v| v.get::<u64>())
            .ok_or_else(|| TransportError::Malformed("SetTargetList targetID".into()))?;
        let kind = child
            .try_child_value(1)
            .and_then(|v| v.get::<String>())
            .ok_or_else(|| TransportError::Malformed("SetTargetList type".into()))?;
        let name = child
            .try_child_value(2)
            .and_then(|v| v.get::<String>())
            .ok_or_else(|| TransportError::Malformed("SetTargetList name".into()))?;
        let url = child
            .try_child_value(3)
            .and_then(|v| v.get::<String>())
            .ok_or_else(|| TransportError::Malformed("SetTargetList url".into()))?;
        // child 4 = hasLocalDebugger — unused for listing.
        targets.push(SocketTarget {
            target_id,
            name,
            url,
            kind,
        });
    }
    Ok((connection_id, targets))
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
}

#[cfg(all(test, unix))]
mod framing_tests {
    use super::*;

    #[test]
    fn setup_round_trips_through_glib_framing() {
        let event = SocketEvent::Setup {
            connection_id: 1,
            target_id: 2,
        };
        let framed = encode_message(&event).unwrap();
        let mut buf = framed;
        let decoded = decode_messages(&mut buf).unwrap();
        assert!(buf.is_empty());
        assert_eq!(decoded, vec![event]);
    }

    #[test]
    fn setup_inspector_client_round_trips() {
        let event = SocketEvent::SetupInspectorClient {
            backend_commands_hash: b"abc123".to_vec(),
        };
        let framed = encode_message(&event).unwrap();
        let mut buf = framed;
        let decoded = decode_messages(&mut buf).unwrap();
        assert_eq!(decoded, vec![event]);
    }

    #[test]
    fn a_message_fed_one_byte_at_a_time_decodes_once_on_the_last_byte() {
        let framed = encode_message(&SocketEvent::DidSetupInspectorClient {
            backend_commands: Vec::new(),
        })
        .unwrap();

        let mut buffer = Vec::new();
        for (i, byte) in framed.iter().enumerate() {
            buffer.push(*byte);
            let decoded = decode_messages(&mut buffer).unwrap();
            if i + 1 < framed.len() {
                assert!(decoded.is_empty(), "decoded early at byte {i}");
                assert!(!buffer.is_empty());
            } else {
                assert_eq!(decoded.len(), 1);
                assert!(buffer.is_empty());
            }
        }
    }

    #[test]
    fn two_messages_in_one_buffer_both_decode() {
        let mut buf = encode_message(&SocketEvent::DidClose).unwrap();
        buf.extend(
            encode_message(&SocketEvent::Setup {
                connection_id: 9,
                target_id: 8,
            })
            .unwrap(),
        );
        let decoded = decode_messages(&mut buf).unwrap();
        assert!(buf.is_empty());
        assert_eq!(
            decoded,
            vec![
                SocketEvent::DidClose,
                SocketEvent::Setup {
                    connection_id: 9,
                    target_id: 8,
                },
            ]
        );
    }

    #[test]
    fn an_oversized_length_is_rejected_without_allocating() {
        let mut buf = (MAX_MESSAGE_BODY_SIZE as u32 + 1).to_be_bytes().to_vec();
        buf.push(BYTE_ORDER_LITTLE_ENDIAN);
        // Do not append a body — the decoder must refuse before needing one.
        let err = decode_messages(&mut buf).unwrap_err();
        assert!(
            matches!(err, TransportError::Malformed(ref m) if m.contains("refusing to allocate")),
            "{err}"
        );
    }

    #[test]
    fn set_target_list_with_three_targets_yields_three_descriptors_in_order() {
        let event = SocketEvent::SetTargetList {
            connection_id: 1,
            target_list: vec![
                SocketTarget {
                    target_id: 10,
                    name: "A".into(),
                    url: "http://a/".into(),
                    kind: "WebPage".into(),
                },
                SocketTarget {
                    target_id: 11,
                    name: "B".into(),
                    url: "http://b/".into(),
                    kind: "JavaScript".into(),
                },
                SocketTarget {
                    target_id: 12,
                    name: "C".into(),
                    url: "http://c/".into(),
                    kind: "ServiceWorker".into(),
                },
            ],
        };
        let framed = encode_message(&event).unwrap();
        let mut buf = framed;
        let decoded = decode_messages(&mut buf).unwrap();
        let SocketEvent::SetTargetList {
            connection_id,
            target_list,
        } = &decoded[0]
        else {
            panic!("expected SetTargetList");
        };
        let descriptors = descriptors_from_target_list(
            *connection_id,
            target_list,
            TransportOrigin::TcpInspectorServer {
                address: "127.0.0.1:2999".into(),
            },
        );
        assert_eq!(descriptors.len(), 3);
        assert_eq!(descriptors[0].name, "A");
        assert_eq!(descriptors[0].kind, TargetType::WebPage);
        assert_eq!(descriptors[0].key, TargetKey::from_ids(1, 10));
        assert_eq!(descriptors[1].name, "B");
        assert_eq!(descriptors[1].kind, TargetType::JavaScript);
        assert_eq!(descriptors[2].name, "C");
        assert_eq!(descriptors[2].kind, TargetType::ServiceWorker);
    }

    #[test]
    fn an_empty_target_list_yields_no_descriptors_and_no_error() {
        let event = SocketEvent::SetTargetList {
            connection_id: 1,
            target_list: vec![],
        };
        let framed = encode_message(&event).unwrap();
        let mut buf = framed;
        let decoded = decode_messages(&mut buf).unwrap();
        let SocketEvent::SetTargetList {
            connection_id,
            target_list,
        } = &decoded[0]
        else {
            panic!("expected SetTargetList");
        };
        let descriptors = descriptors_from_target_list(
            *connection_id,
            target_list,
            TransportOrigin::TcpInspectorServer {
                address: "127.0.0.1:2999".into(),
            },
        );
        assert!(descriptors.is_empty());
    }

    #[test]
    fn set_target_list_parses_a_captured_payload() {
        // Captured from WebKitGTK 2.52.3 MiniBrowser — one WebPage on connection 1.
        let payload = hex::decode(
            "0100000000000000010000000000000057656250616765006d6a782d7765626b\
             69742d64656275676765722066697874757265207061676500687474703a2f2f\
             3132372e302e302e313a383733312f696e6465782e68746d6c000052311056",
        )
        .unwrap();
        let mut body = b"SetTargetList\0".to_vec();
        body.extend_from_slice(&payload);
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.push(BYTE_ORDER_LITTLE_ENDIAN);
        framed.extend_from_slice(&body);

        let mut buf = framed;
        let decoded = decode_messages(&mut buf).unwrap();
        assert_eq!(
            decoded,
            vec![SocketEvent::SetTargetList {
                connection_id: 1,
                target_list: vec![SocketTarget {
                    target_id: 1,
                    name: "mjx-webkit-debugger fixture page".into(),
                    url: "http://127.0.0.1:8731/index.html".into(),
                    kind: "WebPage".into(),
                }],
            }]
        );
    }
}
