//! Shared snapshot the session task publishes and the UI reads via [`arc_swap`].
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use std::sync::Arc;

use arc_swap::ArcSwap;
use mjx_wk_source::{SourceId, SourceText, SourceTreeNode};

/// Immutable UI-facing session state for one frame.
#[derive(Debug, Clone)]
pub struct ShellSnapshot {
    /// Short status for the toolbar.
    pub status: String,
    /// Longer notes shown when the tree is empty or attach is deferred.
    pub notes: Vec<String>,
    /// Names of agents that attached (empty until L4 agents register).
    pub active_agents: Vec<&'static str>,
    /// Whether a live [`mjx_wk_session::SessionHandle`] is connected.
    pub connected: bool,
    /// Inventory display tree (rebuilt on the session side, never on the UI).
    pub tree: SourceTreeNode,
    /// Currently open source, if any.
    pub selected: Option<SourceId>,
    /// Cached text for [`Self::selected`], ready for the code view.
    pub selected_text: Option<Arc<SourceText>>,
}

impl Default for ShellSnapshot {
    fn default() -> Self {
        Self {
            status: "starting…".into(),
            notes: Vec::new(),
            active_agents: Vec::new(),
            connected: false,
            tree: empty_tree(),
            selected: None,
            selected_text: None,
        }
    }
}

pub fn empty_tree() -> SourceTreeNode {
    SourceTreeNode::Group {
        label: "Sources".into(),
        children: Vec::new(),
    }
}

/// Pointer-stable handle the UI clones each frame.
pub type SharedSnapshot = Arc<ArcSwap<ShellSnapshot>>;

pub fn new_shared_snapshot() -> SharedSnapshot {
    Arc::new(ArcSwap::from_pointee(ShellSnapshot::default()))
}

pub fn publish(shared: &SharedSnapshot, snapshot: ShellSnapshot) {
    shared.store(Arc::new(snapshot));
}

/// Update a snapshot in place via a closure (load → mutate → store).
pub fn update(shared: &SharedSnapshot, f: impl FnOnce(&mut ShellSnapshot)) {
    let mut next = (**shared.load()).clone();
    f(&mut next);
    publish(shared, next);
}
