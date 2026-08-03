//! The source editor.
//!
//! **Owned by `docs/tasks/T-008-code-view.md`.** The single most demanding
//! widget in the application.
//!
//! # Virtualisation is the whole design
//!
//! A 5 MB bundle can be 200 000 lines, or one line 5 MB long. Both must scroll
//! at 60 fps. So:
//!
//! - lay out only the visible rows plus a small margin;
//! - ask the highlighter only for that window;
//! - never measure the whole text to size the scroll area — use
//!   `line_count * row_height`, which is why the font is monospace and the row
//!   height fixed;
//! - clip a very long line horizontally rather than wrapping it, or a minified
//!   file becomes one row several million pixels tall.
//!
//! # The gutter carries five states
//!
//! Empty, resolved, pending, conditional, logpoint — plus the execution-line
//! marker, which is not a breakpoint and must not look like one. `DESIGN.md`
//! specifies each.

use crate::{Action, PanelCtx};

/// A virtualised, syntax-highlighted, breakpoint-aware source view.
#[derive(Debug, Default)]
pub struct CodeView {
    _private: (),
}

impl CodeView {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Draw the visible window of a source.
    ///
    /// `model` carries the text, its highlight spans, the breakpoints in it,
    /// and the paused location if execution stopped here.
    pub fn ui(
        &mut self,
        _ui: &mut egui::Ui,
        _ctx: &PanelCtx<'_>,
        _model: &CodeViewModel<'_>,
    ) -> Vec<Action> {
        todo!("T-008")
    }

    /// Scroll so a line is visible, centring it if it is far off screen.
    pub fn reveal_line(&mut self, _line: u32) {
        todo!("T-008")
    }
}

/// What the code view needs for one frame. Borrowed, never owned: this is
/// rebuilt every frame and must not allocate.
#[derive(Debug)]
pub struct CodeViewModel<'a> {
    pub text: &'a mjx_wk_source::SourceText,
    /// Spans for the visible window only.
    pub spans: &'a [Vec<mjx_wk_source::HighlightSpan>],
    /// Which lines carry a breakpoint, and in what state.
    pub breakpoints: &'a [(u32, BreakpointMark)],
    /// Where execution is stopped, if it is stopped here.
    pub execution_line: Option<u32>,
    /// Probe values to render inline at the end of a line — WebKit's live
    /// gutter values, which Chrome has no equivalent for.
    pub inline_values: &'a [(u32, String)],
}

/// How a breakpoint should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointMark {
    Resolved,
    /// Set, but no matching script has parsed. Hollow, as in Chrome.
    Pending,
    Conditional,
    Logpoint,
    Disabled,
}
