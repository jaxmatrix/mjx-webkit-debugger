//! Search across sources.
//!
//! **Owned by `docs/tasks/T-009-source-tree-and-search.md`.**

use crate::{Action, PanelCtx};

/// A query box and its results.
#[derive(Debug, Default)]
pub struct SearchBar {
    _private: (),
}

impl SearchBar {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw the box and the hit list.
    ///
    /// Local results appear as the user types; remote results arrive later and
    /// merge in. The list must not jump when they do — a result the user is
    /// about to click must not move.
    pub fn ui(
        &mut self,
        _ui: &mut egui::Ui,
        _ctx: &PanelCtx<'_>,
        _hits: &[mjx_wk_source::SearchHit],
    ) -> Vec<Action> {
        todo!("T-009")
    }
}
