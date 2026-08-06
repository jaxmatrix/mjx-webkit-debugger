//! The breakpoint model.
//!
//! Line kinds and the store shell: **`docs/tasks/T-201-breakpoints.md`.**
//! Non-line kinds (DOM / event / URL / symbolic): **`docs/tasks/T-206-dom-debugger-breakpoints.md`.**

use std::collections::HashMap;

use mjx_wk_source::{NodeId, SourceId, SourceLocation};
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
/// Non-line kinds (T-206) live in their own lists — they have no source line and
/// must not pollute the gutter index.
///
/// Cloned into each [`crate::DebugModel`] snapshot: breakpoints are few, and a
/// pointer-sized `Arc` of the model is what the UI thread reads.
#[derive(Debug, Default, Clone)]
pub struct BreakpointStore {
    all: Vec<Breakpoint>,
    by_source: HashMap<SourceId, Vec<Breakpoint>>,
    dom: Vec<DomBreakpoint>,
    event: Vec<EventBreakpoint>,
    url: Vec<UrlBreakpoint>,
    symbolic: Vec<SymbolicBreakpoint>,
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

    /// Every line breakpoint, for the breakpoint list panel.
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

    // --- Non-line kinds (T-206) -------------------------------------------

    /// DOM change breakpoints.
    pub fn dom(&self) -> &[DomBreakpoint] {
        &self.dom
    }

    /// Event listener / timer / animation-frame breakpoints.
    pub fn event(&self) -> &[EventBreakpoint] {
        &self.event
    }

    /// XHR/fetch URL breakpoints.
    pub fn url(&self) -> &[UrlBreakpoint] {
        &self.url
    }

    /// Function-name (symbolic) breakpoints.
    pub fn symbolic(&self) -> &[SymbolicBreakpoint] {
        &self.symbolic
    }

    /// Remember a DOM breakpoint locally. Returns `false` on duplicate.
    pub fn insert_dom(&mut self, bp: DomBreakpoint) -> bool {
        if self.dom.iter().any(|existing| existing == &bp) {
            return false;
        }
        self.dom.push(bp);
        true
    }

    /// Drop a DOM breakpoint after a successful `removeDOMBreakpoint`.
    pub fn remove_dom(&mut self, bp: &DomBreakpoint) -> bool {
        let before = self.dom.len();
        self.dom.retain(|existing| existing != bp);
        self.dom.len() != before
    }

    /// Drop every DOM breakpoint on a node that no longer exists.
    ///
    /// A `node-removed` pause makes the debuggee's `nodeId` useless; leaving the
    /// entry would show a dangling row that can never fire again.
    pub fn cleanup_dom_node(&mut self, node: NodeId) -> usize {
        let before = self.dom.len();
        self.dom.retain(|bp| bp.node != node);
        before - self.dom.len()
    }

    /// Remember an event breakpoint locally. Returns `false` on duplicate.
    pub fn insert_event(&mut self, bp: EventBreakpoint) -> bool {
        if self.event.iter().any(|existing| existing == &bp) {
            return false;
        }
        self.event.push(bp);
        true
    }

    /// Drop an event breakpoint locally.
    pub fn remove_event(&mut self, bp: &EventBreakpoint) -> bool {
        let before = self.event.len();
        self.event.retain(|existing| existing != bp);
        self.event.len() != before
    }

    /// Remember a URL breakpoint locally. Returns `false` on duplicate.
    pub fn insert_url(&mut self, bp: UrlBreakpoint) -> bool {
        if self.url.iter().any(|existing| existing == &bp) {
            return false;
        }
        self.url.push(bp);
        true
    }

    /// Drop a URL breakpoint locally.
    pub fn remove_url(&mut self, bp: &UrlBreakpoint) -> bool {
        let before = self.url.len();
        self.url.retain(|existing| existing != bp);
        self.url.len() != before
    }

    /// Remember a symbolic breakpoint locally. Returns `false` on duplicate.
    pub fn insert_symbolic(&mut self, bp: SymbolicBreakpoint) -> bool {
        if self.symbolic.iter().any(|existing| existing == &bp) {
            return false;
        }
        self.symbolic.push(bp);
        true
    }

    /// Drop a symbolic breakpoint locally.
    pub fn remove_symbolic(&mut self, bp: &SymbolicBreakpoint) -> bool {
        let before = self.symbolic.len();
        self.symbolic.retain(|existing| existing != bp);
        self.symbolic.len() != before
    }

    /// First symbolic breakpoint that matches `name`, honouring regex/case.
    pub fn matching_symbolic(&self, name: &str) -> Option<&SymbolicBreakpoint> {
        self.symbolic.iter().find(|bp| bp.matches(name))
    }
}

/// Pause when the DOM changes. `DOMDebugger.setDOMBreakpoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct DomBreakpoint {
    pub node: NodeId,
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

impl DomBreakpointKind {
    /// Wire string for `DOMDebugger.setDOMBreakpoint` / pause `data.type`.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::SubtreeModified => "subtree-modified",
            Self::AttributeModified => "attribute-modified",
            Self::NodeRemoved => "node-removed",
        }
    }

    /// Parse a pause-data / protocol type string.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "subtree-modified" => Some(Self::SubtreeModified),
            "attribute-modified" => Some(Self::AttributeModified),
            "node-removed" => Some(Self::NodeRemoved),
            _ => None,
        }
    }
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

impl SymbolicBreakpoint {
    /// Whether `name` matches this breakpoint's symbol, regex, and case options.
    ///
    /// Mirrors WebKit's `SymbolicBreakpoint::matches`: exact or regex, optional
    /// case folding. An invalid regex never matches (and never panics).
    pub fn matches(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        if self.is_regex {
            let mut builder = regex::RegexBuilder::new(&self.symbol);
            builder.case_insensitive(!self.case_sensitive);
            match builder.build() {
                Ok(re) => re.is_match(name),
                Err(_) => false,
            }
        } else if self.case_sensitive {
            name == self.symbol
        } else {
            name.eq_ignore_ascii_case(&self.symbol)
        }
    }
}

/// Human-readable "why did it stop?" for an instrumentation pause.
///
/// The UI shows this string the moment execution stops — it is the only answer
/// that matters then. Pulls type / event / URL / symbol out of pause `data`
/// when present; otherwise falls back to the protocol reason label.
pub fn instrumentation_detail(
    reason_label: &str,
    data: Option<&serde_json::Value>,
    store: &BreakpointStore,
) -> String {
    let data = data.unwrap_or(&serde_json::Value::Null);
    match reason_label {
        "DOM" => {
            let kind = data.get("type").and_then(|v| v.as_str()).unwrap_or("DOM");
            match data.get("nodeId").and_then(|v| v.as_i64()) {
                Some(id) => format!("DOM {kind} on node {id}"),
                None => format!("DOM {kind}"),
            }
        }
        "Listener" | "AnimationFrame" | "Interval" | "Timeout" => {
            if let Some(name) = data.get("eventName").and_then(|v| v.as_str()) {
                format!("{reason_label} {name}")
            } else {
                reason_label.to_owned()
            }
        }
        "URL" => {
            if let Some(pattern) = data
                .get("breakpointURL")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                format!("URL matching {pattern}")
            } else if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
                format!("URL {url}")
            } else {
                "URL".into()
            }
        }
        "FunctionCall" => {
            if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                if store.matching_symbolic(name).is_some() {
                    return format!("Symbolic {name}");
                }
                format!("FunctionCall {name}")
            } else {
                "FunctionCall".into()
            }
        }
        other => other.to_owned(),
    }
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

    #[test]
    fn non_line_kinds_set_and_remove_cleanly() {
        let mut store = BreakpointStore::new();
        let dom = DomBreakpoint {
            node: NodeId(7),
            kind: DomBreakpointKind::SubtreeModified,
        };
        assert!(store.insert_dom(dom.clone()));
        assert!(!store.insert_dom(dom.clone()));
        assert_eq!(store.dom().len(), 1);
        assert!(store.remove_dom(&dom));
        assert!(store.dom().is_empty());

        let event = EventBreakpoint {
            category: "listener".into(),
            name: Some("click".into()),
        };
        assert!(store.insert_event(event.clone()));
        assert!(store.remove_event(&event));

        let url = UrlBreakpoint {
            pattern: "api/".into(),
            is_regex: false,
        };
        assert!(store.insert_url(url.clone()));
        assert!(store.remove_url(&url));

        let sym = SymbolicBreakpoint {
            symbol: "computeTotal".into(),
            case_sensitive: true,
            is_regex: false,
        };
        assert!(store.insert_symbolic(sym.clone()));
        assert!(store.remove_symbolic(&sym));
        assert!(store.symbolic().is_empty());
        // Line APIs untouched.
        assert!(store.all().is_empty());
    }

    #[test]
    fn cleanup_dom_node_drops_all_kinds_for_that_node() {
        let mut store = BreakpointStore::new();
        store.insert_dom(DomBreakpoint {
            node: NodeId(1),
            kind: DomBreakpointKind::SubtreeModified,
        });
        store.insert_dom(DomBreakpoint {
            node: NodeId(1),
            kind: DomBreakpointKind::NodeRemoved,
        });
        store.insert_dom(DomBreakpoint {
            node: NodeId(2),
            kind: DomBreakpointKind::AttributeModified,
        });
        assert_eq!(store.cleanup_dom_node(NodeId(1)), 2);
        assert_eq!(store.dom().len(), 1);
        assert_eq!(store.dom()[0].node, NodeId(2));
    }

    #[test]
    fn symbolic_honours_regex_and_case_options() {
        let exact = SymbolicBreakpoint {
            symbol: "Foo".into(),
            case_sensitive: true,
            is_regex: false,
        };
        assert!(exact.matches("Foo"));
        assert!(!exact.matches("foo"));

        let folded = SymbolicBreakpoint {
            symbol: "Foo".into(),
            case_sensitive: false,
            is_regex: false,
        };
        assert!(folded.matches("foo"));
        assert!(folded.matches("FOO"));

        let re = SymbolicBreakpoint {
            symbol: r"^on\w+$".into(),
            case_sensitive: true,
            is_regex: true,
        };
        assert!(re.matches("onClick"));
        assert!(!re.matches("click"));

        let re_ci = SymbolicBreakpoint {
            symbol: r"^foo$".into(),
            case_sensitive: false,
            is_regex: true,
        };
        assert!(re_ci.matches("FOO"));
    }

    #[test]
    fn instrumentation_detail_names_which_breakpoint_fired() {
        let mut store = BreakpointStore::new();
        store.insert_symbolic(SymbolicBreakpoint {
            symbol: "computeTotal".into(),
            case_sensitive: true,
            is_regex: false,
        });
        assert_eq!(
            instrumentation_detail(
                "DOM",
                Some(&serde_json::json!({"type": "node-removed", "nodeId": 42})),
                &store
            ),
            "DOM node-removed on node 42"
        );
        assert_eq!(
            instrumentation_detail(
                "Listener",
                Some(&serde_json::json!({"eventName": "click"})),
                &store
            ),
            "Listener click"
        );
        assert_eq!(
            instrumentation_detail(
                "URL",
                Some(&serde_json::json!({"breakpointURL": "api/", "url": "https://x/api/1"})),
                &store
            ),
            "URL matching api/"
        );
        assert_eq!(
            instrumentation_detail(
                "FunctionCall",
                Some(&serde_json::json!({"name": "computeTotal"})),
                &store
            ),
            "Symbolic computeTotal"
        );
    }
}
