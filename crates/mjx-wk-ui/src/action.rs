//! The only channel from the UI back to the debuggee.
//!
//! A widget never calls the session. It returns [`Action`]s, which the app
//! drains and forwards. That indirection is what makes widgets testable without
//! a debuggee and what guarantees the UI thread never awaits.
//!
//! The enum is `#[non_exhaustive]` because every phase adds variants to it —
//! it is the one place in the seam designed to grow.

use mjx_wk_source::{NodeId, SourceId, SourceLocation};

/// A stepping action, mirroring `mjx_wk_debug::StepKind`.
///
/// Duplicated rather than imported so that `mjx-wk-ui` does not depend on
/// `mjx-wk-debug` for a four-variant enum; the app maps between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Over,
    Into,
    Out,
    Next,
    UntilNextRunLoop,
}

/// Something the user did that the debuggee needs to hear about.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Action {
    // ---- source (Phase 1) ----
    /// Open a source, optionally scrolling to a line.
    OpenSource(SourceId, Option<u32>),
    /// Fetch text that is not cached yet.
    RequestSource(SourceId),
    /// Toggle pretty-printing for a source.
    TogglePrettyPrint(SourceId),
    /// Run a search.
    Search(String),

    // ---- debugger (Phase 2) ----
    ToggleBreakpoint(SourceLocation),
    SetBreakpointCondition(SourceLocation, Option<String>),
    /// Add a log or probe action to a breakpoint.
    SetBreakpointAction(SourceLocation, Option<String>),
    RemoveBreakpoint(SourceLocation),
    SetBreakpointsActive(bool),
    Step(StepKind),
    ContinueTo(SourceLocation),
    Resume,
    Pause,
    SelectFrame(usize),
    /// Expand a variable row. Carries the slice to fetch, because
    /// `Runtime.getProperties` is paginated on WebKit.
    ExpandValue {
        node: u32,
        start: u32,
        count: u32,
    },
    Evaluate(String),
    AddWatch(String),
    RemoveWatch(usize),
    SetBlackboxed(SourceId, bool),

    // ---- network (Phase 3) ----
    SelectRequest(mjx_wk_source::RequestId),
    RequestBody(mjx_wk_source::RequestId),
    ClearNetworkLog,

    // ---- profiling (Phase 5) ----
    StartRecording,
    StopRecording,
    TakeHeapSnapshot,

    // ---- elements (Phase 6) ----
    SelectNode(NodeId),
    ExpandNode(NodeId),
    HighlightNode(Option<NodeId>),
    SetInspectMode(bool),
    EditStyle {
        node: NodeId,
        property: String,
        value: String,
    },
    ForcePseudoClass {
        node: NodeId,
        class: String,
        enabled: bool,
    },
}
