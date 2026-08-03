//! The file tree.
//!
//! **Owned by `docs/tasks/T-009-source-tree.md`.**
//!
//! Grouped by origin, then by path. Must stay usable with ten thousand
//! sources — a large site has that many — so rows are virtualised and the tree
//! is built when the inventory changes, not per frame.

use crate::{Action, PanelCtx};

/// A grouped, virtualised list of every known source.
#[derive(Debug, Default)]
pub struct SourceTree {
    _private: (),
}

impl SourceTree {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw the tree.
    ///
    /// Expansion state lives here, not in the model, so it survives the
    /// inventory changing underneath — a page that loads a script must not
    /// collapse the folder the user just opened.
    pub fn ui(
        &mut self,
        _ui: &mut egui::Ui,
        _ctx: &PanelCtx<'_>,
        _tree: &mjx_wk_source::SourceTreeNode,
        _selected: Option<mjx_wk_source::SourceId>,
    ) -> Vec<Action> {
        todo!("T-009")
    }
}
