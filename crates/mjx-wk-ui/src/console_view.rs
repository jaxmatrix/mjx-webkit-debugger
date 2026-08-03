//! The console log and its prompt.
//!
//! **Phase 2 — owned by `docs/tasks/T-204-console.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The console log and its prompt.
#[derive(Debug, Default)]
pub struct ConsoleView {
    _private: (),
}

impl ConsoleView {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-204-console.md")
    }
}
