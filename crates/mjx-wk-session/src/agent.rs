//! The extension point every feature crate implements.
//!
//! A [`DomainAgent`] owns one slice of the debuggee's state: it enables the
//! domains it needs, folds their events into a model, and publishes that model
//! as an immutable snapshot the UI can read without locking.
//!
//! **This trait plus `Panel` is how a phase nobody has scoped yet already has a
//! shape.** A new feature is: one crate implementing `DomainAgent`, one `Panel`
//! implementation over its model, one line in the registry. Nothing already
//! written changes. That is the whole reason the seam was frozen up front.
//!
//! # The snapshot rule
//!
//! [`DomainAgent::snapshot`] returns an `Arc`. The registry stores that behind
//! an [`ArcSwap`](arc_swap::ArcSwap) and republishes after attach and after
//! every successful `on_event`. The UI thread clones the `Arc` from the swap
//! and reads for the duration of a frame; the agent, meanwhile, builds the
//! next version. Neither waits for the other, which is what keeps a 5 MB
//! script arriving from ever costing a dropped frame.

use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{SessionError, SessionHandle};

/// Published model handle the UI reads without locking.
///
/// Cheap to clone: it is an `Arc` around an `ArcSwap`.
pub type AgentSnapshot<M> = Arc<ArcSwap<M>>;

/// One feature's view of a debuggee.
#[async_trait]
pub trait DomainAgent: Send + std::fmt::Debug + 'static {
    /// The state this agent maintains. Read by the UI, never mutated by it.
    type Model: Send + Sync + 'static;

    /// The domains this agent needs enabled.
    ///
    /// The session enables them on attach and gates on them: if none are
    /// available, the agent is never started and its panel greys out.
    const DOMAINS: &'static [Domain];

    /// A stable identifier, used in the registry and in logs.
    const NAME: &'static str;

    /// Enable domains and fetch whatever initial state is needed.
    ///
    /// Called once, before any event is delivered. Returning an error here
    /// means the feature is unavailable on this debuggee; it must not tear down
    /// the session, since the other agents may be fine.
    async fn attach(&mut self, session: &SessionHandle) -> Result<(), SessionError>;

    /// Fold one event into the model.
    ///
    /// Called for every event in [`DomainAgent::DOMAINS`]. Must be quick: it
    /// runs on the session task, and time spent here is time the socket is not
    /// being read. Anything expensive belongs on a spawned task that publishes
    /// its result later.
    async fn on_event(&mut self, event: &NormalizedFrame) -> Result<(), SessionError>;

    /// Publish the current model.
    ///
    /// Cheap, and called at least once after attach and after every successful
    /// `on_event` so the [`AgentRegistry`] can push into the agent's
    /// [`AgentSnapshot`].
    fn snapshot(&self) -> Arc<Self::Model>;

    /// Release debuggee-side resources.
    ///
    /// The common case is `Runtime.releaseObjectGroup`: remote object handles
    /// pin JavaScript values in the debuggee's heap, and leaving them behind
    /// leaks memory in the program under test.
    async fn detach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        Ok(())
    }
}

/// The agents attached to one session.
///
/// **Owned by `docs/tasks/T-010-app-shell.md`.**
///
/// Holds each agent behind its own task, so a slow fold in one feature cannot
/// stall another, and routes each event to the agents whose domains it belongs
/// to. Each successful registration returns an [`AgentSnapshot`] the host holds
/// for the UI thread.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    active: Vec<&'static str>,
    tasks: Vec<JoinHandle<()>>,
}

impl AgentRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            active: Vec::new(),
            tasks: Vec::new(),
        }
    }

    /// Register an agent and attach it.
    ///
    /// Returns `Ok(Some(snapshot))` when the agent attached and is folding
    /// events. Returns `Ok(None)` when the agent is skipped — none of its
    /// domains are available, or `attach` failed — without failing the
    /// registry, so one broken feature cannot take the rest of the session down.
    ///
    /// Domain subscriptions are opened **before** `attach`, so events published
    /// while enabling (e.g. `Debugger.scriptParsed`) are buffered into the agent
    /// task rather than lost to a late subscriber.
    ///
    /// The returned [`AgentSnapshot`] is seeded after a successful `attach` and
    /// republished after every successful `on_event`.
    pub async fn register<A: DomainAgent>(
        &mut self,
        mut agent: A,
        session: &SessionHandle,
    ) -> Result<Option<AgentSnapshot<A::Model>>, SessionError> {
        if !domains_available(session, A::DOMAINS) {
            tracing::info!(
                agent = A::NAME,
                "skipping agent: none of its domains are available on this target"
            );
            return Ok(None);
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel::<NormalizedFrame>();
        for &domain in A::DOMAINS {
            let mut sub = session.subscribe_domain(domain);
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                while let Some(frame) = sub.next().await {
                    if event_tx.send(frame).is_err() {
                        break;
                    }
                }
            });
        }
        // Drop the registry's clone so `event_rx` closes when every subscriber
        // task exits (session ended).
        drop(event_tx);

        if let Err(err) = agent.attach(session).await {
            tracing::warn!(
                agent = A::NAME,
                error = %err,
                "agent attach failed; feature unavailable for this session"
            );
            return Ok(None);
        }

        let published = Arc::new(ArcSwap::from(agent.snapshot()));
        let ui_handle = Arc::clone(&published);

        let name = A::NAME;
        let session = session.clone();
        let task = tokio::spawn(async move {
            run_agent(agent, session, event_rx, published).await;
        });

        self.active.push(name);
        self.tasks.push(task);
        Ok(Some(ui_handle))
    }

    /// The names of agents that attached successfully.
    pub fn active(&self) -> Vec<&'static str> {
        self.active.clone()
    }
}

impl Drop for AgentRegistry {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

/// Whether any of `domains` looks usable on this session.
///
/// Uses `Domain.enable` as the probe member: that is what every agent calls
/// first, and capability gating already treats a missing enable as "domain
/// absent".
fn domains_available(session: &SessionHandle, domains: &[Domain]) -> bool {
    domains
        .iter()
        .any(|&domain| session.supports(domain, "enable").is_available())
}

/// Fold buffered and live domain events into one agent until the session ends.
async fn run_agent<A: DomainAgent>(
    mut agent: A,
    session: SessionHandle,
    mut events: mpsc::UnboundedReceiver<NormalizedFrame>,
    published: AgentSnapshot<A::Model>,
) {
    while let Some(frame) = events.recv().await {
        if let Err(err) = agent.on_event(&frame).await {
            tracing::warn!(
                agent = A::NAME,
                error = %err,
                "agent on_event failed; continuing"
            );
            continue;
        }
        published.store(agent.snapshot());
    }

    if let Err(err) = agent.detach(&session).await {
        tracing::warn!(agent = A::NAME, error = %err, "agent detach failed");
    }
}
