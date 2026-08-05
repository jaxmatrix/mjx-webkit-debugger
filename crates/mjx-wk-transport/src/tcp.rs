//! The TCP inspector server backend.
//!
//! Reaches anything that honours `WEBKIT_INSPECTOR_SERVER`: WebKitGTK, WPE,
//! WinCairo, Playwright's WebKit, and Bun. That covers every Linux Tauri app
//! and the local MiniBrowser this repo develops against.
//!
//! ```sh
//! WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 ./your-app
//! ```
//!
//! **Discovery and the session share one socket**, speaking the GLib
//! `SocketConnection` protocol described in [`crate::discovery`] — not HTTP,
//! and not a WebSocket. One connection carries the target list and then every
//! target's frames, multiplexed by `connectionID`/`targetID`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use mjx_wk_dialect::DialectKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::discovery::{
    Discovery, SocketEvent, TargetDescriptor, TargetKey, backend_commands_hash, decode_messages,
    default_webkit_library, descriptors_from_target_list, encode_message,
};
use crate::{Transport, TransportError, TransportOrigin};

/// A WebKit inspector server reachable over TCP.
#[derive(Debug, Clone)]
pub struct TcpInspectorServer {
    address: String,
    webkit_library: PathBuf,
}

impl TcpInspectorServer {
    /// Point at an inspector server, e.g. `"127.0.0.1:2999"`.
    ///
    /// Does no I/O; nothing is contacted until [`Discovery::list`] or
    /// [`TcpInspectorServer::attach`] is called.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            webkit_library: default_webkit_library(),
        }
    }

    /// Override the WebKit shared library used to hash
    /// `InspectorBackendCommands.js` for the handshake.
    pub fn with_webkit_library(mut self, path: impl Into<PathBuf>) -> Self {
        self.webkit_library = path.into();
        self
    }

    /// Shared library path used for the backend-commands digest.
    pub fn webkit_library(&self) -> &Path {
        &self.webkit_library
    }

    /// Open a session to one target.
    ///
    /// **Owned by `docs/tasks/T-002-socket-transport.md`.**
    ///
    /// Sends `Setup { connectionID, targetID }`, then relays: an outgoing frame
    /// becomes `SendMessageToBackend`, and each `SendMessageToFrontend`'s
    /// `message` is handed back as a received frame. Must:
    ///
    /// - surface a refused connection as [`TransportError::Connect`] naming the
    ///   endpoint, since "is the app running with inspection enabled?" is the
    ///   first thing a user needs to know;
    /// - treat a clean close as `recv() -> None`, not an error;
    /// - reassemble messages across reads — a 5 MB `getScriptSource` reply
    ///   arrives in many chunks, and so does `DidSetupInspectorClient`;
    /// - send `FrontendDidClose` on `close()`, so the debuggee tears its side
    ///   down rather than leaving the target marked as being inspected.
    pub async fn attach(&self, _key: &TargetKey) -> Result<TcpTransport, TransportError> {
        todo!("T-002 — fixtures/attach.jsonl pins the handshake")
    }
}

#[async_trait]
impl Discovery for TcpInspectorServer {
    async fn list(&self) -> Result<Vec<TargetDescriptor>, TransportError> {
        let mut stream =
            TcpStream::connect(&self.address)
                .await
                .map_err(|source| TransportError::Connect {
                    endpoint: self.address.clone(),
                    source: Box::new(source),
                })?;

        let hash = backend_commands_hash(&self.webkit_library)?;
        let framed = encode_message(&SocketEvent::SetupInspectorClient {
            backend_commands_hash: hash,
        })?;
        stream.write_all(&framed).await?;
        stream.flush().await?;

        let origin = TransportOrigin::TcpInspectorServer {
            address: self.address.clone(),
        };
        let mut buffer = Vec::new();
        let mut last_empty: Vec<TargetDescriptor> = Vec::new();
        let mut saw_setup = false;
        let deadline = Instant::now() + Duration::from_secs(15);

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut chunk = [0u8; 65536];
            match tokio::time::timeout(
                remaining.min(Duration::from_secs(3)),
                stream.read(&mut chunk),
            )
            .await
            {
                Err(_) => continue,
                Ok(Ok(0)) => {
                    return Err(TransportError::ConnectionLost(
                        "the inspector server closed the connection during discovery".into(),
                    ));
                }
                Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
                Ok(Err(e)) => return Err(e.into()),
            }

            for event in decode_messages(&mut buffer)? {
                match event {
                    SocketEvent::DidSetupInspectorClient { .. } => {
                        saw_setup = true;
                    }
                    SocketEvent::SetTargetList {
                        connection_id,
                        target_list,
                    } => {
                        let descriptors = descriptors_from_target_list(
                            connection_id,
                            &target_list,
                            origin.clone(),
                        );
                        if !descriptors.is_empty() {
                            return Ok(descriptors);
                        }
                        // Empty lists are normal early on; keep waiting.
                        last_empty = descriptors;
                    }
                    SocketEvent::DidClose => {}
                    other => {
                        return Err(TransportError::Discovery {
                            endpoint: self.address.clone(),
                            reason: format!("unexpected message during discovery: {other:?}"),
                        });
                    }
                }
            }
        }

        if !saw_setup {
            return Err(TransportError::Discovery {
                endpoint: self.address.clone(),
                reason: "no DidSetupInspectorClient within 15s. Confirm \
                         WEBKIT_INSPECTOR_SERVER points at a WebKitGTK/WPE \
                         debuggee (not a CDP endpoint)."
                    .into(),
            });
        }
        // Handshake succeeded but nothing registered — missing developer
        // extras, or the page has not loaded yet. That is not an error.
        Ok(last_empty)
    }

    fn endpoint(&self) -> String {
        self.address.clone()
    }
}

/// A live session with one inspectable target, multiplexed over the
/// inspector server's socket.
#[derive(Debug)]
pub struct TcpTransport {
    _private: (),
}

#[async_trait]
impl Transport for TcpTransport {
    async fn send(&mut self, _text: String) -> Result<(), TransportError> {
        todo!("T-002")
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        todo!("T-002")
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        todo!("T-002")
    }

    fn dialect(&self) -> DialectKind {
        DialectKind::WebKitRwi
    }
}
