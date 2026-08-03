//! The scope and variable tree.
//!
//! **Phase 2 — owned by `docs/tasks/T-203-variable-tree.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The scope and variable tree.
#[derive(Debug, Default)]
pub struct VariablesTree {
    _private: (),
}

impl VariablesTree {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-203-variable-tree.md")
    }
}
