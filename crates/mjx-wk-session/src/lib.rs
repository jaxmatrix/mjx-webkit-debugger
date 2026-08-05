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

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mjx_wk_dialect::{Dialect, DialectError, NormalizedFrame, Support, TargetId};
use mjx_wk_protocol::frame::FrameError;
use mjx_wk_protocol::generated::inspector;
use mjx_wk_protocol::generated::target::events as target_events;
use mjx_wk_protocol::{Command, Domain, Event, Frame, ProtocolError, RequestId};
use mjx_wk_transport::{Target, Transport, TransportError};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot};

pub use agent::{AgentRegistry, DomainAgent};
pub use gating::Capabilities;

/// How many events a subscriber may buffer before it starts lagging.
///
/// Sized for a burst of scriptParsed / network events without retaining
/// unbounded history. A slow reader drops, never stalls the recv loop.
const EVENT_BROADCAST_CAPACITY: usize = 256;

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
        transport: Box<dyn Transport>,
        dialect: Box<dyn Dialect>,
        target: Target,
    ) -> Result<SessionHandle, SessionError> {
        let handle = SessionHandle::spawn(transport, dialect, target);
        // Only Inspector.enable here. Feature domains wait for their agents.
        match handle.call(inspector::commands::Enable {}).await {
            Ok(_) => Ok(handle),
            Err(err) => {
                // Dropping the last handle closes the command channel so the
                // session task exits and abandons anything still pending.
                drop(handle);
                Err(err)
            }
        }
    }
}

/// Shared state reachable from every [`SessionHandle`] clone.
struct Shared {
    target: Target,
    dialect: Arc<dyn Dialect>,
    caps: Mutex<Capabilities>,
    sub_targets: Mutex<Vec<TargetId>>,
    connected: AtomicBool,
    next_id: AtomicU64,
    cmd_tx: mpsc::UnboundedSender<SessionCmd>,
    event_tx: broadcast::Sender<NormalizedFrame>,
}

/// A cheap, cloneable handle to a running session.
///
/// Every feature crate holds one of these. It is `Send + Sync + Clone`, so an
/// agent can be moved onto the session task without ceremony.
#[derive(Clone)]
pub struct SessionHandle {
    shared: Arc<Shared>,
    /// When set, outbound commands are routed through the dialect to this
    /// sub-target (`Target.sendMessageToTarget` on WebKit).
    route: Option<TargetId>,
}

impl fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionHandle")
            .field("target", &self.shared.target.key)
            .field("route", &self.route)
            .field("connected", &self.is_connected())
            .finish()
    }
}

impl SessionHandle {
    fn spawn(transport: Box<dyn Transport>, dialect: Box<dyn Dialect>, target: Target) -> Self {
        let dialect: Arc<dyn Dialect> = Arc::from(dialect);
        let caps = Capabilities::new(target.kind);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(EVENT_BROADCAST_CAPACITY);

        let shared = Arc::new(Shared {
            target,
            dialect: Arc::clone(&dialect),
            caps: Mutex::new(caps),
            sub_targets: Mutex::new(Vec::new()),
            connected: AtomicBool::new(true),
            next_id: AtomicU64::new(1),
            cmd_tx,
            event_tx: event_tx.clone(),
        });

        let task_shared = Arc::clone(&shared);
        tokio::spawn(session_task(
            transport,
            dialect,
            task_shared,
            cmd_rx,
            event_tx,
        ));

        Self {
            shared,
            route: None,
        }
    }

    /// Send a command and await its typed reply.
    ///
    /// **Owned by `docs/tasks/T-003-session-correlation.md`.**
    ///
    /// Returns [`SessionError::Unsupported`] without touching the wire when
    /// either capability axis rejects the member — that is what makes
    /// `supports()` an honest pre-check rather than an optimistic guess.
    pub async fn call<C: Command>(&self, command: C) -> Result<C::Returns, SessionError> {
        self.ensure_supported(C::DOMAIN, C::METHOD)?;

        let id = RequestId(self.shared.next_id.fetch_add(1, Ordering::Relaxed));
        let params = serde_json::to_value(&command).map_err(json_error)?;
        let (reply_tx, reply_rx) = oneshot::channel();

        self.shared
            .cmd_tx
            .send(SessionCmd::Call {
                id,
                method: C::qualified_method(),
                params,
                domain: C::DOMAIN,
                member: C::METHOD,
                target: self.route.clone(),
                reply: reply_tx,
            })
            .map_err(|_| SessionError::Closed)?;

        let frame = reply_rx.await.map_err(|_| SessionError::Closed)??;
        match frame {
            Frame::Response { result, .. } => serde_json::from_value(result).map_err(json_error),
            Frame::Error { error, .. } => Err(SessionError::Protocol(error)),
            other => Err(SessionError::Dialect(DialectError::Envelope(format!(
                "correlated reply was not a response or error: {other:?}"
            )))),
        }
    }

    /// Send a command to a specific sub-target.
    ///
    /// Equivalent to `self.for_target(id).call(command)`.
    pub async fn call_on<C: Command>(
        &self,
        target: &TargetId,
        command: C,
    ) -> Result<C::Returns, SessionError> {
        self.for_target(target).call(command).await
    }

    /// Subscribe to one event type.
    ///
    /// Subscriptions are broadcast: every subscriber sees every matching event.
    /// A slow subscriber lags rather than blocking the session task — an agent
    /// that cannot keep up must not stall the socket.
    pub fn subscribe<E: Event>(&self) -> Subscription<E> {
        let method = E::qualified_method();
        Subscription {
            rx: self.shared.event_tx.subscribe(),
            map: Box::new(move |nf| match &nf.frame {
                Frame::Event {
                    method: m, params, ..
                } if *m == method => serde_json::from_value(params.clone()).ok(),
                _ => None,
            }),
        }
    }

    /// Subscribe to every event in a domain, undecoded.
    ///
    /// For [`DomainAgent`]s, which fold whole domains and would otherwise need
    /// one subscription per event type.
    pub fn subscribe_domain(&self, domain: Domain) -> Subscription<NormalizedFrame> {
        let domain_name = domain.as_str();
        Subscription {
            rx: self.shared.event_tx.subscribe(),
            map: Box::new(move |nf| match &nf.frame {
                Frame::Event { method, .. }
                    if method.starts_with(domain_name)
                        && method.as_bytes().get(domain_name.len()) == Some(&b'.') =>
                {
                    Some(nf.clone())
                }
                _ => None,
            }),
        }
    }

    /// Whether a member can be used, checking both capability axes.
    pub fn supports(&self, domain: Domain, member: &str) -> Support {
        let caps = lock(&self.shared.caps);
        caps.supports(&*self.shared.dialect, domain, member)
    }

    /// The target this session is attached to.
    pub fn target(&self) -> &Target {
        &self.shared.target
    }

    /// Sub-targets seen so far, from `Target.targetCreated`.
    pub fn sub_targets(&self) -> Vec<TargetId> {
        lock(&self.shared.sub_targets).clone()
    }

    /// A handle whose commands are routed to one sub-target.
    ///
    /// Cheap: shares the same connection and correlation table. The routing is
    /// applied by the dialect at encode time.
    pub fn for_target(&self, target: &TargetId) -> SessionHandle {
        SessionHandle {
            shared: Arc::clone(&self.shared),
            route: Some(target.clone()),
        }
    }

    /// Whether the debuggee is still attached.
    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Acquire)
    }

    fn ensure_supported(&self, domain: Domain, member: &str) -> Result<(), SessionError> {
        let caps = lock(&self.shared.caps);
        if caps
            .supports(&*self.shared.dialect, domain, member)
            .is_available()
        {
            return Ok(());
        }
        let reason = caps.reason(&*self.shared.dialect, domain, member);
        Err(SessionError::Unsupported {
            domain,
            member: member.to_owned(),
            reason,
        })
    }
}

/// How a [`Subscription`] decides whether an inbound frame is theirs.
type EventMapFn<T> = Box<dyn Fn(&NormalizedFrame) -> Option<T> + Send>;

/// A stream of events of one kind.
pub struct Subscription<T> {
    rx: broadcast::Receiver<NormalizedFrame>,
    map: EventMapFn<T>,
}

impl<T> fmt::Debug for Subscription<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription").finish_non_exhaustive()
    }
}

impl<T: Send + 'static> Subscription<T> {
    /// Await the next event.
    ///
    /// `None` once the session ends.
    pub async fn next(&mut self) -> Option<T> {
        loop {
            match self.rx.recv().await {
                Ok(frame) => {
                    if let Some(mapped) = (self.map)(&frame) {
                        return Some(mapped);
                    }
                }
                // Slow readers skip missed messages rather than stalling the
                // socket. Keep waiting for something we can still decode.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
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

enum SessionCmd {
    Call {
        id: RequestId,
        method: String,
        params: Value,
        domain: Domain,
        member: &'static str,
        target: Option<TargetId>,
        reply: oneshot::Sender<Result<Frame, SessionError>>,
    },
}

struct Pending {
    reply: oneshot::Sender<Result<Frame, SessionError>>,
    domain: Domain,
    member: &'static str,
    /// Routed through `Target.sendMessageToTarget`. The immediate outer ack
    /// must not complete the call — the real reply arrives unwrapped from
    /// `Target.dispatchMessageFromTarget`.
    routed: bool,
}

async fn session_task(
    mut transport: Box<dyn Transport>,
    dialect: Arc<dyn Dialect>,
    shared: Arc<Shared>,
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCmd>,
    event_tx: broadcast::Sender<NormalizedFrame>,
) {
    let mut pending: HashMap<RequestId, Pending> = HashMap::new();
    // ReplayTransport reports `recv() == None` when the next trace line is a
    // send — that is "nothing to read yet", not a closed socket. A live TCP
    // transport blocks instead. We only poll recv after the first outbound
    // frame, which matches attach (Inspector.enable) and every real session.
    let mut reading = false;

    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else {
                    break;
                };
                match handle_cmd(
                    cmd,
                    &mut transport,
                    &*dialect,
                    &mut pending,
                ).await {
                    Ok(true) => {
                        reading = true;
                    }
                    Ok(false) => {}
                    Err(err) => {
                        tracing::warn!(error = %err, "session command failed; closing");
                        break;
                    }
                }
            }
            inbound = transport.recv(), if reading => {
                match inbound {
                    Some(Ok(text)) => {
                        handle_inbound(
                            &text,
                            &*dialect,
                            &shared,
                            &mut pending,
                            &event_tx,
                        );
                    }
                    Some(Err(err)) => {
                        tracing::warn!(error = %err, "transport recv failed; closing");
                        break;
                    }
                    None => {
                        // Live socket: peer closed. ReplayTransport: no queued
                        // receives because the next trace line is a send.
                        // Park on the next command instead of tearing down —
                        // the following send re-fills the recv queue on replay,
                        // and fails with ConnectionLost on a truly dead peer.
                        // Leave `connected` alone here: between replayed
                        // commands the peer is still the session we attached to.
                        let Some(cmd) = cmd_rx.recv().await else {
                            break;
                        };
                        match handle_cmd(
                            cmd,
                            &mut transport,
                            &*dialect,
                            &mut pending,
                        ).await {
                            Ok(true) => {
                                reading = true;
                            }
                            Ok(false) => {}
                            Err(err) => {
                                tracing::warn!(error = %err, "session command failed; closing");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    shared.connected.store(false, Ordering::Release);
    for (id, waiter) in pending.drain() {
        let _ = waiter.reply.send(Err(SessionError::Abandoned(id)));
    }
    let _ = transport.close().await;
}

/// Returns `Ok(true)` when a frame was written to the transport.
async fn handle_cmd(
    cmd: SessionCmd,
    transport: &mut Box<dyn Transport>,
    dialect: &dyn Dialect,
    pending: &mut HashMap<RequestId, Pending>,
) -> Result<bool, SessionError> {
    let SessionCmd::Call {
        id,
        method,
        params,
        domain,
        member,
        target,
        reply,
    } = cmd;

    let frame = Frame::Request { id, method, params };
    let routed = target.is_some();
    let encoded = match dialect.encode(frame, target.as_ref()) {
        Ok(f) => f,
        Err(err) => {
            let _ = reply.send(Err(SessionError::Dialect(err)));
            return Ok(false);
        }
    };
    let text = match encoded.to_json() {
        Ok(t) => t,
        Err(err) => {
            let _ = reply.send(Err(SessionError::Dialect(DialectError::Frame(err))));
            return Ok(false);
        }
    };

    pending.insert(
        id,
        Pending {
            reply,
            domain,
            member,
            routed,
        },
    );

    if let Err(err) = transport.send(text).await {
        if let Some(waiter) = pending.remove(&id) {
            let _ = waiter.reply.send(Err(SessionError::Transport(err)));
        }
        return Err(SessionError::Closed);
    }
    Ok(true)
}

fn handle_inbound(
    text: &str,
    dialect: &dyn Dialect,
    shared: &Shared,
    pending: &mut HashMap<RequestId, Pending>,
    event_tx: &broadcast::Sender<NormalizedFrame>,
) {
    let frame = match Frame::from_json(text) {
        Ok(f) => f,
        Err(err) => {
            tracing::warn!(error = %err, "dropping unclassifiable inbound frame");
            return;
        }
    };

    let normalized = match dialect.decode(frame) {
        Ok(n) => n,
        Err(err) => {
            tracing::warn!(error = %err, "dropping frame the dialect could not decode");
            return;
        }
    };

    match normalized.frame {
        Frame::Response { id, result } => {
            complete_pending(
                NormalizedFrame {
                    frame: Frame::Response { id, result },
                    target: normalized.target,
                },
                id,
                pending,
                shared,
            );
        }
        Frame::Error { id, error } => {
            complete_pending(
                NormalizedFrame {
                    frame: Frame::Error { id, error },
                    target: normalized.target,
                },
                id,
                pending,
                shared,
            );
        }
        frame @ Frame::Event { .. } => {
            if let Frame::Event { method, params } = &frame {
                note_target_lifecycle(method, params, shared);
            }
            let _ = event_tx.send(NormalizedFrame {
                frame,
                target: normalized.target,
            });
        }
        Frame::Request { .. } => {
            tracing::warn!("dropping unexpected inbound request frame");
        }
    }
}

fn complete_pending(
    normalized: NormalizedFrame,
    id: RequestId,
    pending: &mut HashMap<RequestId, Pending>,
    shared: &Shared,
) {
    let Some(waiter) = pending.get(&id) else {
        // The debuggee is untrusted: an id nobody asked for is logged, never a panic.
        tracing::warn!(%id, "dropping unsolicited response");
        return;
    };

    // Outer `Target.sendMessageToTarget` ack: same id, no target attribution.
    // The real typed reply arrives later via dispatchMessageFromTarget.
    if waiter.routed && normalized.target.is_none() {
        return;
    }

    let Some(waiter) = pending.remove(&id) else {
        return;
    };

    let result = match normalized.frame {
        frame @ Frame::Response { .. } => Ok(frame),
        Frame::Error { id, error } => {
            {
                let mut caps = lock(&shared.caps);
                caps.learn_from_failure(waiter.domain, waiter.member, &error);
            }
            Ok(Frame::Error { id, error })
        }
        other => Err(SessionError::Dialect(DialectError::Envelope(format!(
            "pending reply resolved to {other:?}"
        )))),
    };
    let _ = waiter.reply.send(result);
}

fn note_target_lifecycle(method: &str, params: &Value, shared: &Shared) {
    if method == target_events::TargetCreated::qualified_method()
        && let Ok(ev) = serde_json::from_value::<target_events::TargetCreated>(params.clone())
    {
        let mut subs = lock(&shared.sub_targets);
        let id = TargetId(ev.target_info.target_id);
        if !subs.contains(&id) {
            subs.push(id);
        }
        return;
    }
    if method == target_events::TargetDestroyed::qualified_method()
        && let Ok(ev) = serde_json::from_value::<target_events::TargetDestroyed>(params.clone())
    {
        let id = TargetId(ev.target_id);
        lock(&shared.sub_targets).retain(|t| t != &id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn json_error(err: serde_json::Error) -> SessionError {
    SessionError::Dialect(DialectError::Frame(FrameError::Json(err)))
}
