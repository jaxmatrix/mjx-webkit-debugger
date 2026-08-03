//! The Styles and Computed panels.
//!
//! **Phase 6 — owned by `docs/tasks/T-604-styles-panel.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The Styles and Computed panels.
#[derive(Debug, Default)]
pub struct StylesPanel {
    _private: (),
}

impl StylesPanel {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-604-styles-panel.md")
    }
}
