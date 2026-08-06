//! Debug-build guard: the egui thread must never await or block.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use std::cell::Cell;

thread_local! {
    /// Set for the duration of [`crate::app::App`]'s frame work.
    static IN_UI_FRAME: Cell<bool> = const { Cell::new(false) };
}

/// Marks the current thread as inside a UI frame until dropped.
#[derive(Debug)]
pub struct UiFrameGuard {
    _private: (),
}

impl UiFrameGuard {
    /// Enter a UI frame. Panics in debug builds if already inside one.
    pub fn enter() -> Self {
        IN_UI_FRAME.with(|flag| {
            debug_assert!(
                !flag.get(),
                "nested UiFrameGuard — a UI frame re-entered itself"
            );
            flag.set(true);
        });
        Self { _private: () }
    }
}

impl Drop for UiFrameGuard {
    fn drop(&mut self) {
        IN_UI_FRAME.with(|flag| flag.set(false));
    }
}

/// Whether the calling thread is currently painting a UI frame.
pub fn in_ui_frame() -> bool {
    IN_UI_FRAME.with(Cell::get)
}

/// Panic in debug builds if called from the UI thread during a frame.
///
/// Call this at the top of any helper that could block or `.await`. The UI
/// path must only `try_send` / `try_recv` on channels.
pub fn ensure_not_ui_thread() {
    // Always evaluate `in_ui_frame` so release builds (RUSTFLAGS=-D warnings)
    // do not see it as dead code; panic only in debug.
    if cfg!(debug_assertions) && in_ui_frame() {
        panic!(
            "UI thread must not await or block during a frame \
             (T-010: actions go through channels; snapshots are Arc clones)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_not_ui_thread_is_quiet_outside_a_frame() {
        ensure_not_ui_thread();
    }

    #[test]
    #[should_panic(expected = "UI thread must not await or block")]
    fn ensure_not_ui_thread_panics_inside_a_frame() {
        let _guard = UiFrameGuard::enter();
        ensure_not_ui_thread();
    }
}
