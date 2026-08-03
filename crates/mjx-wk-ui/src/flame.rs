//! The flame graph and timeline ruler.
//!
//! **Phase 5 — owned by `docs/tasks/T-502-profilers-flame-graph.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The flame graph and timeline ruler.
#[derive(Debug, Default)]
pub struct FlameGraphView {
    _private: (),
}

impl FlameGraphView {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-502-profilers-flame-graph.md")
    }
}
