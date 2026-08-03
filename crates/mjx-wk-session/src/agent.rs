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
//! [`DomainAgent::snapshot`] returns an `Arc`. The UI thread clones the `Arc`
//! and reads from it for the duration of a frame; the agent, meanwhile, builds
//! the next version. Neither waits for the other, which is what keeps a 5 MB
//! script arriving from ever costing a dropped frame.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;

use crate::{SessionError, SessionHandle};

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
    /// Cheap, and called at least once per rendered frame.
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
/// to.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    _private: (),
}

impl AgentRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Register an agent and attach it.
    ///
    /// Skips the agent, without failing, when the session supports none of its
    /// domains — the feature is simply unavailable here.
    pub async fn register<A: DomainAgent>(
        &mut self,
        _agent: A,
        _session: &SessionHandle,
    ) -> Result<(), SessionError> {
        todo!("T-010")
    }

    /// The names of agents that attached successfully.
    pub fn active(&self) -> Vec<&'static str> {
        todo!("T-010")
    }
}
