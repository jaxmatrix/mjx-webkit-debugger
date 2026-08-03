//! The application: window, dock, and the wiring between them and the session.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**
//!
//! # The split that keeps it fast
//!
//! ```text
//!   main thread                        session thread (tokio)
//!   ───────────                        ──────────────────────
//!   eframe event loop                  owns the Transport
//!   Panel::ui  ──► Vec<Action> ──────► drains, sends commands
//!   reads Arc snapshots  ◄──────────── agents publish snapshots
//! ```
//!
//! The main thread never awaits and never locks anything the session thread
//! holds for long. Snapshots cross as `Arc` clones through `ArcSwap`, so
//! reading one is a pointer copy however large the state behind it is.

use std::path::PathBuf;

use anyhow::Result;

/// How the application starts.
///
/// The fields are read by `run`, which is T-010's to write; until then nothing
/// constructs an `App` either.
#[derive(Debug)]
#[allow(dead_code, reason = "consumed by run(), owned by T-010")]
pub enum Startup {
    /// Open with the target picker.
    Picker,
    /// Attach immediately.
    Attach {
        address: String,
        target: Option<usize>,
    },
    /// Drive the UI from a recorded trace, with no debuggee.
    Replay { fixture: PathBuf },
}

/// The eframe application.
#[derive(Debug)]
#[allow(dead_code, reason = "constructed by run(), owned by T-010")]
pub struct App {
    _private: (),
}

/// Open the window and run until it closes.
pub fn run(_startup: Startup) -> Result<()> {
    todo!("T-010")
}

impl eframe::App for App {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Order matters: drain the actions produced last frame *before* reading
        // snapshots, so a click is acted on one frame earlier.
        //
        // Nothing here may await or block. If a handler needs the debuggee, it
        // sends an Action and reads the answer from a later snapshot.
        todo!("T-010")
    }
}
