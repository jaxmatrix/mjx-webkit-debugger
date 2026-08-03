//! L1 — the WebKit/Chromium seam.
//!
//! Everything above this crate speaks **one vocabulary: WebKit's**. That is a
//! deliberate asymmetry rather than a neutral middle ground — WebKit is the
//! primary target, and in several places it is the richer of the two.
//!
//! A [`Dialect`] translates between that vocabulary and what is actually on the
//! wire. It is why one debugger can drive a Tauri app on Linux (WebKitGTK,
//! WebKit RWI) and the same app on Windows (WebView2, Chrome DevTools
//! Protocol) without a single feature crate knowing which it is talking to.
//!
//! ```text
//!   feature crates ──► always WebKit vocabulary
//!                          │
//!                      Dialect
//!              ┌───────────┴───────────┐
//!        WebKitDialect            CdpDialect
//!        (identity)               (translating)
//!   WebKitGTK / WPE / WKWebView   WebView2 / Android WebView / Chrome
//! ```
//!
//! # Why not a neutral third vocabulary
//!
//! Because every neutral model is a third thing to learn, and it would have to
//! be lossy in whichever direction it did not favour. WebKit has members Chrome
//! has no equivalent for — breakpoint *probe* actions, `setPauseOnMicrotasks`,
//! `Canvas` shader inspection — and modelling those as "extensions to a neutral
//! core" buys nothing over simply being WebKit-shaped.

pub mod cdp;
pub mod webkit;

use std::fmt;

use mjx_wk_protocol::{Domain, Frame};
use serde::{Deserialize, Serialize};

pub use cdp::CdpDialect;
pub use webkit::WebKitDialect;

/// Which wire protocol a connection actually speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DialectKind {
    /// WebKit's Remote Inspector protocol. The native vocabulary.
    WebKitRwi,
    /// The Chrome DevTools Protocol, as spoken by WebView2, Android System
    /// WebView, and Chrome itself.
    ChromeDevToolsProtocol,
}

impl fmt::Display for DialectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DialectKind::WebKitRwi => "WebKit RWI",
            DialectKind::ChromeDevToolsProtocol => "Chrome DevTools Protocol",
        })
    }
}

/// How well a dialect can serve a given protocol member.
///
/// This is what a panel consults to decide whether to render a control or grey
/// it out. Discovering the answer by sending a command and getting an error
/// back is too late — the user has already clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Support {
    /// The wire has this member directly.
    Native,
    /// The dialect produces the same effect from different members. A CDP
    /// logpoint, for instance, is a conditional breakpoint whose condition has
    /// a side effect: the same outcome by another route.
    Emulated,
    /// No equivalent exists. The UI must not offer it.
    Unsupported,
}

impl Support {
    /// Whether the member can be used at all.
    pub fn is_available(self) -> bool {
        matches!(self, Support::Native | Support::Emulated)
    }
}

/// A protocol-level target id, as carried by `Target.sendMessageToTarget` on
/// WebKit and by `sessionId` on CDP.
///
/// Distinct from a transport's discovery identifier: that one names a socket to
/// open, this one names a target *within* an already-open connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TargetId(pub String);

impl fmt::Display for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A decoded frame in WebKit vocabulary, plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedFrame {
    /// Always WebKit-shaped, whatever the wire was.
    pub frame: Frame,
    /// The target that produced it, when the connection is multiplexed.
    ///
    /// `None` means the connection's own target. On WebKit this is unwrapped
    /// from `Target.dispatchMessageFromTarget`; on CDP, from `sessionId`.
    pub target: Option<TargetId>,
}

/// Translation between WebKit vocabulary and a wire protocol.
///
/// Implementations are stateless and cheap to call: a dialect is consulted on
/// every frame in both directions.
pub trait Dialect: Send + Sync + fmt::Debug {
    /// Which wire protocol this speaks.
    fn kind(&self) -> DialectKind;

    /// Turn a WebKit-vocabulary outbound frame into wire form.
    ///
    /// `target` routes the frame at the protocol level — wrapping it in
    /// `Target.sendMessageToTarget` for WebKit, or attaching a `sessionId` for
    /// CDP.
    fn encode(&self, frame: Frame, target: Option<&TargetId>) -> Result<Frame, DialectError>;

    /// Turn an inbound wire frame into WebKit vocabulary, unwrapping any target
    /// multiplexing.
    fn decode(&self, frame: Frame) -> Result<NormalizedFrame, DialectError>;

    /// How well this dialect serves a member, without asking the debuggee.
    fn supports(&self, domain: Domain, member: &str) -> Support;
}

/// A frame could not be translated.
#[derive(Debug, thiserror::Error)]
pub enum DialectError {
    /// The member has no counterpart in the target dialect.
    #[error("`{domain}.{member}` has no equivalent in {dialect}")]
    Unsupported {
        domain: Domain,
        member: String,
        dialect: DialectKind,
    },

    /// A multiplexing envelope was malformed — for instance a
    /// `Target.dispatchMessageFromTarget` whose `message` is not JSON.
    #[error("malformed target envelope: {0}")]
    Envelope(String),

    /// The frame's own JSON could not be handled.
    #[error("frame error: {0}")]
    Frame(#[from] mjx_wk_protocol::frame::FrameError),

    /// A translation needed a field the frame did not carry.
    #[error("translating `{method}`: {reason}")]
    Translation { method: String, reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emulated_counts_as_available_but_unsupported_does_not() {
        // The UI branches on exactly this: a control is offered when a feature
        // is reachable at all, however it is implemented underneath.
        assert!(Support::Native.is_available());
        assert!(Support::Emulated.is_available());
        assert!(!Support::Unsupported.is_available());
    }
}
