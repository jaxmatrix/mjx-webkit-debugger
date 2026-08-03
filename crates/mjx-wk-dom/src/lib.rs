//! L4 — the DOM tree and inspector overlays.
//!
//! **Phase 6.** The largest domain in the protocol: 78 members.
//!
//! # The tree arrives in pieces
//!
//! `DOM.getDocument` returns only the top of it. Children come from
//! `requestChildNodes` and then `setChildNodes`, and the tree mutates under you
//! via `childNodeInserted`, `childNodeRemoved`, `attributeModified`,
//! `characterDataModified`, and `childNodeCountUpdated`. Rebuilding from
//! scratch on each change loses the user's expansion state and scroll
//! position, so every one of those must be applied incrementally.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// One node.
#[derive(Debug, Clone)]
pub struct DomNode {
    pub id: mjx_wk_source::NodeId,
    pub parent: Option<mjx_wk_source::NodeId>,
    /// 1 = element, 3 = text, 8 = comment, 9 = document.
    pub node_type: i64,
    pub name: String,
    pub value: String,
    pub attributes: Vec<(String, String)>,
    /// `None` when children have not been requested yet — distinct from
    /// `Some(vec![])`, which means there genuinely are none.
    pub children: Option<Vec<mjx_wk_source::NodeId>>,
    /// Reported separately, so a collapsed node can show a count.
    pub child_count: u32,
    pub is_shadow_root: bool,
    pub pseudo_type: Option<String>,
}

/// What to draw over the page when highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlayConfig {
    pub show_info: bool,
    pub show_rulers: bool,
    pub show_grid: bool,
    pub show_flex: bool,
}

/// The element panel's state.
#[derive(Debug, Default)]
pub struct DomModel {
    pub root: Option<mjx_wk_source::NodeId>,
    pub selected: Option<mjx_wk_source::NodeId>,
    pub overlay: OverlayConfig,
    /// True while the element picker is armed.
    pub inspect_mode: bool,
}

/// Owns Domain::Dom.
#[derive(Debug, Default)]
pub struct DomAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for DomAgent {
    type Model = DomModel;

    const DOMAINS: &'static [Domain] = &[Domain::Dom];
    const NAME: &'static str = "mjx-wk-dom";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 6 — docs/tasks/T-601-dom-model.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 6 — docs/tasks/T-601-dom-model.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 6 — docs/tasks/T-601-dom-model.md")
    }
}
