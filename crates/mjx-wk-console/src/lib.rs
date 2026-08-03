//! L4 — console messages and expression evaluation.
//!
//! **Phase 2.**
//!
//! # Evaluation routes two ways
//!
//! When execution is paused, an expression must go to
//! `Debugger.evaluateOnCallFrame` so it sees the local scope; otherwise to
//! `Runtime.evaluate`. Sending everything to `Runtime.evaluate` is the common
//! mistake, and it makes the console useless at exactly the moment it matters —
//! you stop at a breakpoint and cannot see the local variable you stopped for.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// Where a message came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSource {
    Javascript,
    Network,
    ConsoleApi,
    Storage,
    Appcache,
    Rendering,
    Css,
    Security,
    Other,
}

/// How loud a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageLevel {
    Debug,
    Log,
    Info,
    Warning,
    Error,
}

/// One console message.
#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub source: MessageSource,
    pub level: MessageLevel,
    pub text: String,
    /// Remote handles for the arguments, so objects stay expandable.
    pub argument_object_ids: Vec<String>,
    pub location: Option<mjx_wk_source::SourceLocation>,
    /// Collapsed repeats, from `Console.messageRepeatCountUpdated`. A message
    /// logged in a render loop must not push everything else off screen.
    pub repeat_count: u32,
}

/// The console log.
#[derive(Debug, Default)]
pub struct ConsoleModel {
    /// Bounded: a page can log without limit, and the debugger must not grow
    /// without limit alongside it.
    pub messages: Vec<ConsoleMessage>,
    /// Messages dropped to stay within the bound, so the UI can say so rather
    /// than silently losing history.
    pub dropped: u64,
}

/// Owns Domain::Console.
#[derive(Debug, Default)]
pub struct ConsoleAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for ConsoleAgent {
    type Model = ConsoleModel;

    const DOMAINS: &'static [Domain] = &[Domain::Console];
    const NAME: &'static str = "mjx-wk-console";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 2 — docs/tasks/T-204-console.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 2 — docs/tasks/T-204-console.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 2 — docs/tasks/T-204-console.md")
    }
}
