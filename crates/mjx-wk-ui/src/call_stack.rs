//! The call stack, with async frames.
//!
//! **Phase 2 — owned by `docs/tasks/T-202-pause-and-stepping.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// The call stack, with async frames.
#[derive(Debug, Default)]
pub struct CallStackList {
    _private: (),
}

impl CallStackList {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-202-pause-and-stepping.md")
    }
}
