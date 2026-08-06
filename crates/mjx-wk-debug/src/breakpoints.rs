//! The breakpoint model.
//!
//! **Owned by `docs/tasks/T-201-breakpoints.md`.**

use std::collections::HashMap;

use mjx_wk_source::{SourceId, SourceLocation};
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
///
/// Line breakpoints live in [`Self::all`]; the per-source index is a denormalised
/// view so [`Self::in_source`] is a slice return with no allocation — the gutter
/// calls it every frame.
///
/// Cloned into each [`crate::DebugModel`] snapshot: breakpoints are few, and a
/// pointer-sized `Arc` of the model is what the UI thread reads.
#[derive(Debug, Default, Clone)]
pub struct BreakpointStore {
    all: Vec<Breakpoint>,
    by_source: HashMap<SourceId, Vec<Breakpoint>>,
}

impl BreakpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Breakpoints in one source, for the gutter. Called every frame; must not
    /// allocate or lock.
    pub fn in_source(&self, source: SourceId) -> &[Breakpoint] {
        self.by_source
            .get(&source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every breakpoint, for the breakpoint list panel.
    pub fn all(&self) -> &[Breakpoint] {
        &self.all
    }

    /// Add one, returning its index. Does not talk to the debuggee: the agent
    /// sends `Debugger.setBreakpointByUrl` and fills the id in later.
    pub fn insert(&mut self, spec: BreakpointSpec) -> usize {
        let state = if spec.enabled {
            BreakpointState::Pending
        } else {
            BreakpointState::Disabled
        };
        let source = spec.location.source;
        let bp = Breakpoint {
            id: None,
            spec,
            state,
            hit_count: 0,
        };
        let index = self.all.len();
        self.by_source.entry(source).or_default().push(bp.clone());
        self.all.push(bp);
        index
    }

    /// Apply a `Debugger.breakpointResolved`.
    pub fn resolve(&mut self, id: &BreakpointId, actual: SourceLocation) {
        let Some(index) = self.find_index(id) else {
            return;
        };
        let Some(bp) = self.all.get_mut(index) else {
            return;
        };
        bp.state = BreakpointState::Resolved { actual };
        self.reindex();
    }

    /// Record the debuggee's id after `setBreakpointByUrl` succeeds.
    pub fn set_id(&mut self, index: usize, id: BreakpointId) {
        let Some(bp) = self.all.get_mut(index) else {
            return;
        };
        bp.id = Some(id);
        self.reindex();
    }

    /// Mark a breakpoint as refused by the debuggee.
    pub fn fail(&mut self, index: usize, reason: impl Into<String>) {
        let Some(bp) = self.all.get_mut(index) else {
            return;
        };
        bp.state = BreakpointState::Failed {
            reason: reason.into(),
        };
        self.reindex();
    }

    /// Count a hit for a breakpoint that paused execution.
    pub fn record_hit(&mut self, id: &BreakpointId) {
        let Some(index) = self.find_index(id) else {
            return;
        };
        if let Some(bp) = self.all.get_mut(index) {
            bp.hit_count = bp.hit_count.saturating_add(1);
            self.reindex();
        }
    }

    /// Look up by debuggee id.
    pub fn find_index(&self, id: &BreakpointId) -> Option<usize> {
        self.all.iter().position(|bp| bp.id.as_ref() == Some(id))
    }

    /// Rebuild the per-source view after a mutation that may change which
    /// source a breakpoint belongs to (requested → resolved).
    fn reindex(&mut self) {
        self.by_source.clear();
        for bp in &self.all {
            let source = match &bp.state {
                BreakpointState::Resolved { actual } => actual.source,
                _ => bp.spec.location.source,
            };
            self.by_source.entry(source).or_default().push(bp.clone());
        }
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

/// Pause when a network request whose URL matches. `DOMDebugger.setURLBreakpoint`.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn loc(source: u32, line: u32, column: u32) -> SourceLocation {
        SourceLocation {
            source: SourceId(source),
            line,
            column,
        }
    }

    #[test]
    fn insert_starts_pending_and_indexes_by_source() {
        let mut store = BreakpointStore::new();
        let index = store.insert(BreakpointSpec::at(loc(1, 3, 0)));
        assert_eq!(index, 0);
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].state, BreakpointState::Pending);
        assert_eq!(store.in_source(SourceId(1)).len(), 1);
        assert!(store.in_source(SourceId(2)).is_empty());
    }

    #[test]
    fn resolve_records_actual_location_when_it_differs() {
        let mut store = BreakpointStore::new();
        let index = store.insert(BreakpointSpec::at(loc(1, 3, 0)));
        store.set_id(index, BreakpointId("/.*app\\.js/:3:0".into()));
        store.resolve(
            &BreakpointId("/.*app\\.js/:3:0".into()),
            loc(1, 3, 2), // blank-line slide: column 0 → 2
        );
        let bp = &store.all()[0];
        assert_eq!(bp.spec.location, loc(1, 3, 0));
        assert_eq!(
            bp.state,
            BreakpointState::Resolved {
                actual: loc(1, 3, 2)
            }
        );
    }

    #[test]
    fn disabled_spec_inserts_as_disabled() {
        let mut store = BreakpointStore::new();
        let mut spec = BreakpointSpec::at(loc(1, 0, 0));
        spec.enabled = false;
        store.insert(spec);
        assert_eq!(store.all()[0].state, BreakpointState::Disabled);
    }

    #[test]
    fn logpoint_is_auto_continue_with_actions() {
        let mut spec = BreakpointSpec::at(loc(1, 0, 0));
        assert!(!spec.is_logpoint());
        spec.auto_continue = true;
        spec.actions.push(BreakpointAction {
            kind: BreakpointActionKind::Log,
            data: Some("hit".into()),
        });
        assert!(spec.is_logpoint());
    }

    #[test]
    fn record_hit_increments_count() {
        let mut store = BreakpointStore::new();
        let index = store.insert(BreakpointSpec::at(loc(1, 3, 0)));
        let id = BreakpointId("bp-1".into());
        store.set_id(index, id.clone());
        store.record_hit(&id);
        store.record_hit(&id);
        assert_eq!(store.all()[0].hit_count, 2);
    }

    #[test]
    fn in_source_returns_empty_slice_without_allocating_entry() {
        let store = BreakpointStore::new();
        // Two calls must both be empty; the map must not grow on a miss.
        assert!(store.in_source(SourceId(99)).is_empty());
        assert!(store.in_source(SourceId(99)).is_empty());
        assert!(store.by_source.is_empty());
    }
}
