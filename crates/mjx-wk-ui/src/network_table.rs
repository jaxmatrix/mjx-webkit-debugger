//! The request table and waterfall.
//!
//! **Phase 3 — owned by `docs/tasks/T-304-network-panel.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The request table and waterfall.
#[derive(Debug, Default)]
pub struct NetworkTable {
    _private: (),
}

impl NetworkTable {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-304-network-panel.md")
    }
}
