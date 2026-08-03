//! Storage, IndexedDB, and cookies.
//!
//! **Phase 7 — owned by `docs/tasks/T-704-storage-panel.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use crate::{Action, PanelCtx};

/// Storage, IndexedDB, and cookies.
#[derive(Debug, Default)]
pub struct StorageTable {
    _private: (),
}

impl StorageTable {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw, and report what the user did.
    pub fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        todo!("T-704-storage-panel.md")
    }
}
