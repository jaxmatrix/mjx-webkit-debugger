//! L2 — one attached debuggee.
//!
//! The session owns the transport and is the only thing in the process that
//! awaits on it. It does four jobs:
//!
//! 1. **Correlation** — hand a [`Command`] in, get its typed reply back.
//! 2. **Event fan-out** — deliver each event to everything subscribed.
//! 3. **Target demultiplexing** — via the [`Dialect`], so a multi-process page
//!    looks the same as a simple one.
//! 4. **Capability gating** — track what this debuggee actually announced, so
//!    a panel can grey itself out instead of erroring.
//!
//! # Threading
//!
//! One tokio task owns the socket. The UI thread never awaits and never blocks
//! on it: commands go out through a channel, and state comes back as
//! [`DomainAgent`] snapshots read through an `ArcSwap`. A 5 MB
//! `Debugger.getScriptSource` reply is parsed and indexed on this side before
//! the UI is ever handed a pointer to it.
//!
//! # Capability gating has two independent axes
//!
//! Both must pass before a command is sent, and they fail for different reasons:
//!
//! - **Dialect** — can the wire protocol express this at all? A CDP debuggee
//!   has no `Canvas` domain in any version.
//! - **Debuggee** — does *this build*, attached to *this kind of target*,
//!   expose it? WebKitGTK 2.52.3 ships `Security` in source but never
//!   activates it, and a `service-worker` target has no `Page` domain.

pub mod agent;
pub mod gating;

use std::fmt;

use mjx_wk_dialect::{DialectError, Support, TargetId};
use mjx_wk_protocol::{Command, Domain, Event, ProtocolError, RequestId};
use mjx_wk_transport::{Target, TransportError};

pub use agent::{AgentRegistry, DomainAgent};
pub use gating::Capabilities;

/// An attached debuggee.
///
/// Constructed from a [`Transport`](mjx_wk_transport::Transport) and a
/// [`Dialect`](mjx_wk_dialect::Dialect), then driven by its own task. Callers
/// hold a [`SessionHandle`] rather than this.
#[derive(Debug)]
pub struct Session {
    _private: (),
}

impl Session {
    /// Attach to a target and start the session task.
    ///
    /// **Owned by `docs/tasks/T-003-session-correlation.md`.**
    ///
    /// Must, before returning: send `Inspector.enable`, learn the debuggee's
    /// capabilities, and be ready to correlate. Must not enable feature domains
    /// — that is each [`DomainAgent`]'s decision, and enabling `Network` on a
    /// session nobody asked to profile is a real cost on a busy page.
    pub async fn attach(
        _transport: Box<dyn mjx_wk_transport::Transport>,
        _dialect: Box<dyn mjx_wk_dialect::Dialect>,
        _target: Target,
    ) -> Result<SessionHandle, SessionError> {
        todo!("T-003 — fixtures/attach.jsonl and target-multiplexed.jsonl pin this")
    }
}

/// A cheap, cloneable handle to a running session.
///
/// Every feature crate holds one of these. It is `Send + Sync + Clone`, so an
/// agent can be moved onto the session task without ceremony.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    _private: (),
}

impl SessionHandle {
    /// Send a command and await its typed reply.
    ///
    /// **Owned by `docs/tasks/T-003-session-correlation.md`.**
    ///
    /// Returns [`SessionError::Unsupported`] without touching the wire when
    /// either capability axis rejects the member — that is what makes
    /// `supports()` an honest pre-check rather than an optimistic guess.
    pub async fn call<C: Command>(&self, _command: C) -> Result<C::Returns, SessionError> {
        todo!("T-003")
    }

    /// Send a command to a specific sub-target.
    ///
    /// Equivalent to `self.for_target(id).call(command)`.
    pub async fn call_on<C: Command>(
        &self,
        _target: &TargetId,
        _command: C,
    ) -> Result<C::Returns, SessionError> {
        todo!("T-003")
    }

    /// Subscribe to one event type.
    ///
    /// Subscriptions are broadcast: every subscriber sees every matching event.
    /// A slow subscriber lags rather than blocking the session task — an agent
    /// that cannot keep up must not stall the socket.
    pub fn subscribe<E: Event>(&self) -> Subscription<E> {
        todo!("T-003")
    }

    /// Subscribe to every event in a domain, undecoded.
    ///
    /// For [`DomainAgent`]s, which fold whole domains and would otherwise need
    /// one subscription per event type.
    pub fn subscribe_domain(
        &self,
        _domain: Domain,
    ) -> Subscription<mjx_wk_dialect::NormalizedFrame> {
        todo!("T-003")
    }

    /// Whether a member can be used, checking both capability axes.
    pub fn supports(&self, _domain: Domain, _member: &str) -> Support {
        todo!("T-003")
    }

    /// The target this session is attached to.
    pub fn target(&self) -> &Target {
        todo!("T-003")
    }

    /// Sub-targets seen so far, from `Target.targetCreated`.
    pub fn sub_targets(&self) -> Vec<TargetId> {
        todo!("T-003")
    }

    /// A handle whose commands are routed to one sub-target.
    ///
    /// Cheap: shares the same connection and correlation table. The routing is
    /// applied by the dialect at encode time.
    pub fn for_target(&self, _target: &TargetId) -> SessionHandle {
        todo!("T-003")
    }

    /// Whether the debuggee is still attached.
    pub fn is_connected(&self) -> bool {
        todo!("T-003")
    }
}

/// A stream of events of one kind.
#[derive(Debug)]
pub struct Subscription<T> {
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: Send + 'static> Subscription<T> {
    /// Await the next event.
    ///
    /// `None` once the session ends.
    pub async fn next(&mut self) -> Option<T> {
        todo!("T-003")
    }
}

/// Something went wrong talking to the debuggee.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The debuggee rejected the command.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// The connection failed.
    #[error(transparent)]
    Transport(#[from] TransportError),

    /// A frame could not be translated.
    #[error(transparent)]
    Dialect(#[from] DialectError),

    /// The member is unavailable here, and we knew before sending.
    #[error("`{domain}.{member}` is unavailable: {reason}")]
    Unsupported {
        domain: Domain,
        member: String,
        reason: UnsupportedReason,
    },

    /// A reply never arrived.
    #[error("no reply to request {0} before the session ended")]
    Abandoned(RequestId),

    /// The session task is gone.
    #[error("the session has ended")]
    Closed,
}

/// Why a member is unavailable, which decides what the UI should say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// The wire protocol has no equivalent — e.g. `Canvas` over CDP.
    Dialect,
    /// This build of the debuggee does not expose it.
    DebuggeeBuild,
    /// This kind of target does not have it — e.g. `Page` on a service worker.
    TargetKind,
}

impl fmt::Display for UnsupportedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnsupportedReason::Dialect => "the wire protocol has no equivalent",
            UnsupportedReason::DebuggeeBuild => "this build of the debuggee does not expose it",
            UnsupportedReason::TargetKind => "this kind of target does not have it",
        })
    }
}
