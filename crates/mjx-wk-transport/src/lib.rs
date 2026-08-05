//! L1 — how frames reach a debuggee.
//!
//! Two traits: [`Discovery`] finds inspectable targets, [`Transport`] carries
//! frames to and from one of them. Everything above this crate is written
//! against those two and knows nothing about sockets, USB, or XPC.
//!
//! # The seam is one JSON string in each direction
//!
//! That placement is load-bearing rather than convenient. Apple's transports do
//! not carry JSON on the wire at all: they carry **binary plists**, and the
//! WebKit frame rides inside one as `WIRSocketDataKey`:
//!
//! ```text
//! { __selector: "_rpc_forwardSocketData:",
//!   __argument: { WIRConnectionIdentifierKey: …,
//!                 WIRApplicationIdentifierKey: …,
//!                 WIRPageIdentifierKey: …,
//!                 WIRSocketDataKey: <the JSON frame, as bytes> } }
//! ```
//!
//! Because the seam is "send me one frame, hand me back one frame", the plist
//! wrapping lives entirely inside the Apple backend and no other crate changes
//! when it lands.
//!
//! # Backends
//!
//! | Backend | Reaches | Phase |
//! |---|---|---|
//! | [`tcp::TcpInspectorServer`] | WebKitGTK, WPE, WinCairo, Playwright, Bun | 1 |
//! | [`replay::ReplayTransport`] | a recorded trace, offline | 1 |
//! | `apple::AppleLocalTransport` | macOS `webinspectord` | 3 |
//! | `apple::AppleUsbTransport` | iOS ≤16 over usbmux | 3 |
//! | `android::AndroidAdbTransport` | Android System WebView | 4 |
//! | `cdp::CdpTransport` | WebView2, Chrome | 4 |

pub mod discovery;
pub mod replay;
pub mod tcp;

use async_trait::async_trait;
use mjx_wk_dialect::DialectKind;
use mjx_wk_protocol::TargetType;
use serde::{Deserialize, Serialize};

pub use discovery::{Discovery, TargetDescriptor, TargetKey};
pub use replay::ReplayTransport;
pub use tcp::TcpInspectorServer;

/// A bidirectional stream of protocol frames, as text.
///
/// Frames are `String` rather than a parsed type because a transport's job is
/// carriage, not interpretation — and because keeping it textual is what lets
/// [`ReplayTransport`] be substituted for a real socket in tests.
#[async_trait]
pub trait Transport: Send + std::fmt::Debug {
    /// Send one frame. Returns once it is handed to the underlying stream, not
    /// once the debuggee has acted on it.
    async fn send(&mut self, text: String) -> Result<(), TransportError>;

    /// Receive the next frame.
    ///
    /// `None` means the debuggee closed the connection cleanly — an ordinary
    /// end of session, not a failure.
    async fn recv(&mut self) -> Option<Result<String, TransportError>>;

    /// Close the connection. Idempotent.
    async fn close(&mut self) -> Result<(), TransportError>;

    /// Which protocol the far end speaks, so the session can pick a dialect.
    fn dialect(&self) -> DialectKind;
}

/// Where a transport reaches, for display and for choosing a backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransportOrigin {
    /// A WebKit inspector server over TCP, as enabled by
    /// `WEBKIT_INSPECTOR_SERVER`. Speaks the GLib `SocketConnection`
    /// protocol (GVariant bodies), not HTTP.
    TcpInspectorServer { address: String },
    /// A recorded trace replayed from disk.
    Replay { fixture: String },
    /// macOS `webinspectord`, over its local Mach service.
    AppleLocal,
    /// An iOS device over usbmux and lockdown.
    AppleUsb { device_udid: String },
    /// An Android device over `adb forward`.
    AndroidAdb { serial: String, socket: String },
    /// A Chromium DevTools endpoint over TCP.
    Cdp { address: String },
}

/// A transport could not carry a frame.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The connection could not be established.
    #[error("connecting to {endpoint}: {source}")]
    Connect {
        endpoint: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The connection dropped mid-session.
    #[error("connection lost: {0}")]
    ConnectionLost(String),

    /// The far end sent something that is not a protocol frame.
    #[error("malformed frame: {0}")]
    Malformed(String),

    /// Discovery could not enumerate targets.
    #[error("discovering targets at {endpoint}: {reason}")]
    Discovery { endpoint: String, reason: String },

    /// The debuggee must opt in and has not.
    ///
    /// Apple platforms need `WKWebView.isInspectable = true` (iOS 16.4+ /
    /// macOS 13.3+), and macOS additionally needs the debuggee to carry the
    /// `com.apple.security.get-task-allow` entitlement. Neither is something
    /// the debugger can arrange, so this is reported as its own case with
    /// instructions rather than as a generic connection failure.
    #[error("the debuggee is not inspectable: {0}")]
    NotInspectable(String),

    /// Underlying I/O failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Everything needed to describe an attachable target, whatever found it.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// How to reach it again.
    pub key: TargetKey,
    /// A human label, e.g. the page title.
    pub name: String,
    /// The document URL, when there is one.
    pub url: String,
    /// What kind of target it is.
    pub kind: TargetType,
    /// Which protocol it speaks.
    pub dialect: DialectKind,
    /// Which backend found it.
    pub origin: TransportOrigin,
}
