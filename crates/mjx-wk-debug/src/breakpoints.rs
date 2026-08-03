//! The breakpoint model.
//!
//! **Owned by `docs/tasks/T-201-breakpoints.md`.**

use mjx_wk_source::SourceLocation;
use serde::{Deserialize, Serialize};

/// The debuggee's identifier for a set breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BreakpointId(pub String);

/// What a breakpoint does when it is hit, beyond pausing.
///
/// WebKit-only. Chrome has just the logpoint, which is [`BreakpointActionKind::Log`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakpointActionKind {
    /// Write a message to the console.
    Log,
    /// Run an expression for its side effects.
    Evaluate,
    /// Sample an expression and show the values inline in the gutter, without
    /// stopping. WebKit's best debugging feature and one Chrome has no answer
    /// to.
    Probe,
    /// Play a sound. Genuinely useful for a breakpoint in a hot path you do not
    /// want to stop at.
    Sound,
}

/// One action attached to a breakpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakpointAction {
    pub kind: BreakpointActionKind,
    /// The expression or message. Unused for [`BreakpointActionKind::Sound`].
    pub data: Option<String>,
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakpointSpec {
    /// Where. Set by URL on the wire so it survives a reload.
    pub location: SourceLocation,
    /// Pause only when this evaluates truthy.
    pub condition: Option<String>,
    /// Skip this many hits before pausing.
    pub ignore_count: u32,
    /// Actions to run on hit.
    pub actions: Vec<BreakpointAction>,
    /// Run the actions and keep going, rather than pausing. This is what turns
    /// a breakpoint into a logpoint or a probe.
    pub auto_continue: bool,
    /// Whether the breakpoint is armed.
    pub enabled: bool,
}

impl BreakpointSpec {
    /// An ordinary pause-here breakpoint.
    pub fn at(location: SourceLocation) -> Self {
        Self {
            location,
            condition: None,
            ignore_count: 0,
            actions: Vec::new(),
            auto_continue: false,
            enabled: true,
        }
    }

    /// Whether this behaves as a logpoint: it runs something and continues.
    pub fn is_logpoint(&self) -> bool {
        self.auto_continue && !self.actions.is_empty()
    }
}

/// What the debuggee did with a breakpoint.
///
/// The distinction between requested and resolved is user-visible: Chrome
/// renders an unresolved breakpoint hollow, and so must we. A breakpoint that
/// silently never binds is a debugging session wasted.
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointState {
    /// Sent, but no script matching the URL has parsed yet. Normal before the
    /// page loads, and permanent if the URL is wrong.
    Pending,
    /// Bound. The location may differ from the one requested — a breakpoint on
    /// a blank line moves to the next statement.
    Resolved { actual: SourceLocation },
    /// The debuggee refused it.
    Failed { reason: String },
    /// Present but not armed.
    Disabled,
}

/// A breakpoint and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Breakpoint {
    pub id: Option<BreakpointId>,
    pub spec: BreakpointSpec,
    pub state: BreakpointState,
    /// How many times it has been hit this session.
    pub hit_count: u32,
}

/// Every breakpoint, of every kind.
#[derive(Debug, Default)]
pub struct BreakpointStore {
    _private: (),
}

impl BreakpointStore {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Breakpoints in one source, for the gutter. Called every frame; must not
    /// allocate or lock.
    pub fn in_source(&self, _source: mjx_wk_source::SourceId) -> &[Breakpoint] {
        todo!("T-201")
    }

    /// Every breakpoint, for the breakpoint list panel.
    pub fn all(&self) -> &[Breakpoint] {
        todo!("T-201")
    }

    /// Add one, returning its index. Does not talk to the debuggee: the agent
    /// sends `Debugger.setBreakpointByUrl` and fills the id in later.
    pub fn insert(&mut self, _spec: BreakpointSpec) -> usize {
        todo!("T-201")
    }

    /// Apply a `Debugger.breakpointResolved`.
    pub fn resolve(&mut self, _id: &BreakpointId, _actual: SourceLocation) {
        todo!("T-201")
    }
}

/// Pause when the DOM changes. `DOMDebugger.setDOMBreakpoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct DomBreakpoint {
    pub node: mjx_wk_source::NodeId,
    pub kind: DomBreakpointKind,
}

/// Chrome offers exactly these three, and so does WebKit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomBreakpointKind {
    /// A child was added or removed.
    SubtreeModified,
    /// An attribute changed.
    AttributeModified,
    /// The node itself was removed.
    NodeRemoved,
}

/// Pause when an event fires. `DOMDebugger.setEventBreakpoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBreakpoint {
    /// e.g. `"listener"`, `"timer"`, `"animation-frame"`.
    pub category: String,
    /// A specific event name, or `None` for the whole category.
    pub name: Option<String>,
}

/// Pause on a network request whose URL matches. `DOMDebugger.setURLBreakpoint`.
///
/// Chrome calls this an XHR/fetch breakpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlBreakpoint {
    /// A substring, or a regex when `is_regex`.
    pub pattern: String,
    pub is_regex: bool,
}

/// Pause when a named function is called. `Debugger.addSymbolicBreakpoint`.
///
/// Chrome's equivalent is the `debug(fn)` console command.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolicBreakpoint {
    pub symbol: String,
    pub case_sensitive: bool,
    pub is_regex: bool,
}
