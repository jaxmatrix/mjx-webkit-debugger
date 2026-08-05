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

use std::collections::VecDeque;
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
    /// Completes the glib handshake on a fresh socket, sends
    /// `Setup { connectionID, targetID }`, then relays: an outgoing frame
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
    pub async fn attach(&self, key: &TargetKey) -> Result<TcpTransport, TransportError> {
        let (connection_id, target_id) = key.as_ids().ok_or_else(|| {
            TransportError::Malformed(format!(
                "target key `{key}` is not a socket connectionID/targetID pair"
            ))
        })?;

        let mut stream =
            TcpStream::connect(&self.address)
                .await
                .map_err(|source| TransportError::Connect {
                    endpoint: self.address.clone(),
                    source: Box::new(source),
                })?;

        let hash = backend_commands_hash(&self.webkit_library)?;
        write_event(
            &mut stream,
            &SocketEvent::SetupInspectorClient {
                backend_commands_hash: hash,
            },
        )
        .await?;

        let mut buffer = Vec::new();
        wait_for_did_setup(&mut stream, &mut buffer, &self.address).await?;

        write_event(
            &mut stream,
            &SocketEvent::Setup {
                connection_id,
                target_id,
            },
        )
        .await?;

        Ok(TcpTransport {
            stream,
            buffer,
            pending: VecDeque::new(),
            connection_id,
            target_id,
            closed: false,
        })
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
    stream: TcpStream,
    buffer: Vec<u8>,
    pending: VecDeque<String>,
    connection_id: u64,
    target_id: u64,
    closed: bool,
}

#[async_trait]
impl Transport for TcpTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost(
                "send on a closed inspector transport".into(),
            ));
        }
        write_event(
            &mut self.stream,
            &SocketEvent::SendMessageToBackend {
                connection_id: self.connection_id,
                target_id: self.target_id,
                message: text,
            },
        )
        .await
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        if self.closed {
            return None;
        }
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Some(Ok(frame));
            }

            match self.pull_pending() {
                Ok(Pull::FrameReady) => continue,
                Ok(Pull::PeerClosed) => {
                    self.closed = true;
                    return None;
                }
                Ok(Pull::NeedMore) => {}
                Err(e) => return Some(Err(e)),
            }

            let mut chunk = [0u8; 65536];
            match self.stream.read(&mut chunk).await {
                Ok(0) => {
                    self.closed = true;
                    return None;
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(e) => return Some(Err(e.into())),
            }

            match self.pull_pending() {
                Ok(Pull::FrameReady) => {
                    if let Some(frame) = self.pending.pop_front() {
                        return Some(Ok(frame));
                    }
                }
                Ok(Pull::PeerClosed) => {
                    if let Some(frame) = self.pending.pop_front() {
                        self.closed = true;
                        return Some(Ok(frame));
                    }
                    self.closed = true;
                    return None;
                }
                Ok(Pull::NeedMore) => {}
                Err(e) => return Some(Err(e)),
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }
        let result = write_event(
            &mut self.stream,
            &SocketEvent::FrontendDidClose {
                connection_id: self.connection_id,
                target_id: self.target_id,
            },
        )
        .await;
        self.closed = true;
        let _ = self.stream.shutdown().await;
        result
    }

    fn dialect(&self) -> DialectKind {
        DialectKind::WebKitRwi
    }
}

async fn write_event(stream: &mut TcpStream, event: &SocketEvent) -> Result<(), TransportError> {
    let framed = encode_message(event)?;
    stream.write_all(&framed).await?;
    stream.flush().await?;
    Ok(())
}

async fn wait_for_did_setup(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
    endpoint: &str,
) -> Result<(), TransportError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for event in decode_messages(buffer)? {
            match event {
                SocketEvent::DidSetupInspectorClient { .. } => return Ok(()),
                // Empty or early listings are normal before Setup.
                SocketEvent::SetTargetList { .. } | SocketEvent::DidClose => {}
                other => {
                    return Err(TransportError::Malformed(format!(
                        "unexpected message during attach handshake: {other:?}"
                    )));
                }
            }
        }

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
                    "the inspector server closed the connection during attach".into(),
                ));
            }
            Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
            Ok(Err(e)) => return Err(e.into()),
        }
    }

    Err(TransportError::Discovery {
        endpoint: endpoint.to_owned(),
        reason: "no DidSetupInspectorClient within 15s during attach".into(),
    })
}

/// Decode complete messages into `pending`.
enum Pull {
    FrameReady,
    NeedMore,
    PeerClosed,
}

impl TcpTransport {
    fn pull_pending(&mut self) -> Result<Pull, TransportError> {
        let mut got_frame = false;
        let mut peer_closed = false;
        for event in decode_messages(&mut self.buffer)? {
            match event {
                SocketEvent::SendMessageToFrontend { message, .. } => {
                    self.pending.push_back(message);
                    got_frame = true;
                }
                SocketEvent::DidClose => peer_closed = true,
                SocketEvent::DidSetupInspectorClient { .. } | SocketEvent::SetTargetList { .. } => {
                }
                other => {
                    return Err(TransportError::Malformed(format!(
                        "unexpected SocketConnection message during session: {other:?}"
                    )));
                }
            }
        }
        if got_frame {
            Ok(Pull::FrameReady)
        } else if peer_closed {
            Ok(Pull::PeerClosed)
        } else {
            Ok(Pull::NeedMore)
        }
    }
}
