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
//! **Discovery and the session share one socket**, speaking the length-prefixed
//! JSON protocol described in [`crate::discovery`] — not HTTP, and not a
//! WebSocket. One connection carries the target list and then every target's
//! frames, multiplexed by `connectionID`/`targetID`.

use async_trait::async_trait;
use mjx_wk_dialect::DialectKind;

use crate::discovery::{Discovery, TargetDescriptor, TargetKey};
use crate::{Transport, TransportError};

/// A WebKit inspector server reachable over TCP.
#[derive(Debug, Clone)]
pub struct TcpInspectorServer {
    address: String,
}

impl TcpInspectorServer {
    /// Point at an inspector server, e.g. `"127.0.0.1:2999"`.
    ///
    /// Does no I/O; nothing is contacted until [`Discovery::list`] or
    /// [`TcpInspectorServer::attach`] is called.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
        }
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
    ///   arrives in many chunks, and so does `BackendCommands`;
    /// - send `FrontendDidClose` on `close()`, so the debuggee tears its side
    ///   down rather than leaving the target marked as being inspected.
    pub async fn attach(&self, _key: &TargetKey) -> Result<TcpTransport, TransportError> {
        todo!("T-002 — blocked on T-000; fixtures/attach.jsonl pins the handshake")
    }
}

#[async_trait]
impl Discovery for TcpInspectorServer {
    async fn list(&self) -> Result<Vec<TargetDescriptor>, TransportError> {
        todo!("T-001 — SetupInspectorClient, then read SetTargetList")
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
