//! L4 — breakpoints, pausing, and inspecting state.
//!
//! **Phase 2.** The seam is frozen in Phase 1a so the code view can be built
//! against it: a gutter that knows about [`BreakpointState`] from the start does
//! not need rewriting when breakpoints arrive.
//!
//! # WebKit is richer than Chrome here
//!
//! Chrome's logpoint is one thing. WebKit has [`BreakpointAction`]s — log,
//! evaluate, sound, and **probe**, which samples an expression every time the
//! line runs and shows the values inline without ever stopping. There is also
//! `setPauseOnMicrotasks`, `setPauseOnAssertions`, and symbolic (function-name)
//! breakpoints. None of these exist over CDP, which is why
//! [`Support`](mjx_wk_dialect::Support) is consulted before offering them.
//!
//! # Two rules that are easy to get wrong and hard to notice
//!
//! 1. **Remote object handles die on resume.** Every `objectId` in a paused
//!    scope becomes invalid the moment execution continues. The variable tree
//!    must be dropped on `Debugger.resumed` and on
//!    `Debugger.globalObjectCleared`, or the UI shows stale rows that error
//!    when expanded. See [`pause::PauseState::invalidate`].
//! 2. **Breakpoints are set by URL, not by script id.** That is what makes them
//!    survive a reload. The debuggee answers with `breakpointResolved` giving
//!    the *actual* location, which may not be the line asked for — a breakpoint
//!    on a blank line moves to the next statement. The UI must show requested
//!    and resolved differently, which is Chrome's grey-versus-blue distinction.

pub mod breakpoints;
pub mod pause;
pub mod values;

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

pub use breakpoints::{
    Breakpoint, BreakpointAction, BreakpointActionKind, BreakpointId, BreakpointSpec,
    BreakpointState, BreakpointStore, DomBreakpoint, EventBreakpoint, SymbolicBreakpoint,
    UrlBreakpoint,
};
pub use pause::{CallFrame, PauseConfig, PauseReason, PauseState, Scope, ScopeKind, StepKind};
pub use values::{ValueNode, ValueNodeId, ValuePreview, ValueTree};

/// Everything the debugger panel displays.
#[derive(Debug, Default)]
pub struct DebugModel {
    /// Breakpoints, whether or not they have resolved.
    pub breakpoints: BreakpointStore,
    /// The current pause, if execution is stopped.
    pub paused: Option<PauseState>,
    /// Whether breakpoints are armed at all — Chrome's "deactivate breakpoints".
    pub breakpoints_active: bool,
    /// What causes a pause besides breakpoints.
    pub pause_config: PauseConfig,
    /// URL patterns excluded from stepping and pausing.
    pub blackboxed: Vec<String>,
    /// Watch expressions, re-evaluated on every pause and step.
    pub watches: Vec<String>,
}

/// Owns `Debugger` and `DOMDebugger`.
#[derive(Debug, Default)]
pub struct DebugAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for DebugAgent {
    type Model = DebugModel;

    const DOMAINS: &'static [Domain] = &[Domain::Debugger, Domain::DomDebugger];
    const NAME: &'static str = "debug";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("T-201 — fixtures/breakpoint-hit.jsonl pins this")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("T-201")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("T-201")
    }

    async fn detach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        // Remote objects pin JavaScript values in the debuggee's heap. Leaving
        // them behind leaks memory in the program under test, which is exactly
        // the thing someone using a debugger is likely to be measuring.
        todo!("T-201 — Runtime.releaseObjectGroup for every group we opened")
    }
}
