//! L4 — network activity.
//!
//! **Phase 3.**
//!
//! # WebKit has no `Fetch` domain
//!
//! Interception is `Network.addInterception` plus `interceptContinue` /
//! `interceptWithRequest` / `interceptWithResponse` / `interceptRequestWithError`.
//! Code written from CDP habits will look for `Fetch` and not find it.
//!
//! # A request is folded from five events
//!
//! `requestWillBeSent` → `responseReceived` → `dataReceived`* →
//! `loadingFinished` | `loadingFailed`. Any of them may be the last one seen if
//! the page navigates mid-flight, so every field after the first event is
//! optional and the UI must render a half-known request without complaint.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// How a request ended, if it has.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestOutcome {
    Pending,
    Finished { encoded_bytes: f64 },
    Failed { error: String, cancelled: bool },
    ServedFromMemoryCache,
}

/// Where the response came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseSource {
    Unknown,
    Network,
    MemoryCache,
    DiskCache,
    ServiceWorker,
    InspectorOverride,
}

/// The waterfall segments for one request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timing {
    pub start: f64,
    pub dns: Option<(f64, f64)>,
    pub connect: Option<(f64, f64)>,
    pub tls: Option<(f64, f64)>,
    pub request_sent: Option<f64>,
    pub response_start: Option<f64>,
    pub response_end: Option<f64>,
}

/// One request, folded across its whole lifecycle.
#[derive(Debug, Clone)]
pub struct NetworkRequest {
    pub id: mjx_wk_source::RequestId,
    pub url: String,
    pub method: String,
    pub status: Option<i64>,
    pub mime_type: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub response_headers: Vec<(String, String)>,
    pub timing: Timing,
    pub outcome: RequestOutcome,
    pub source: ResponseSource,
    /// What caused it — a script, the parser, or another request.
    pub initiator: Option<mjx_wk_source::SourceLocation>,
    /// Bodies are fetched on demand via `Network.getResponseBody`; holding
    /// every body for every request would dwarf everything else the debugger
    /// keeps.
    pub body_fetched: bool,
}

/// A WebSocket and its frames.
#[derive(Debug, Clone)]
pub struct WebSocketChannel {
    pub id: mjx_wk_source::RequestId,
    pub url: String,
    pub frames: Vec<WsFrame>,
    pub closed: bool,
}

/// One WebSocket frame.
#[derive(Debug, Clone)]
pub struct WsFrame {
    pub outgoing: bool,
    pub opcode: i64,
    pub payload: String,
    pub timestamp: f64,
}

/// Everything the network panel shows.
#[derive(Debug, Default)]
pub struct NetworkModel {
    pub requests: Vec<NetworkRequest>,
    pub sockets: Vec<WebSocketChannel>,
    /// Whether the log survives navigation — Chrome's "Preserve log".
    pub preserve_log: bool,
}

/// Owns Domain::Network.
#[derive(Debug, Default)]
pub struct NetworkAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for NetworkAgent {
    type Model = NetworkModel;

    const DOMAINS: &'static [Domain] = &[Domain::Network];
    const NAME: &'static str = "mjx-wk-network";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 3 — docs/tasks/T-301-network-panel.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 3 — docs/tasks/T-301-network-panel.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 3 — docs/tasks/T-301-network-panel.md")
    }
}
