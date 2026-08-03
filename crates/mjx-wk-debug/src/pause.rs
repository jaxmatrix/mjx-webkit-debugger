//! Paused execution: why, where, and what is in scope.
//!
//! **Owned by `docs/tasks/T-202-pause-and-stepping.md`.**

use mjx_wk_source::SourceLocation;

use crate::values::ValueTree;

/// Why the debuggee stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum PauseReason {
    Breakpoint,
    /// A `debugger;` statement.
    DebuggerStatement,
    Exception {
        caught: bool,
    },
    /// A step finished.
    Step,
    /// The user pressed pause.
    User,
    /// A DOM, event, or URL breakpoint.
    Instrumentation {
        detail: String,
    },
    /// An `assert()` failed — WebKit-only.
    Assertion,
    /// A microtask boundary — WebKit-only.
    Microtask,
    Other(String),
}

/// What kind of scope a frame's variables live in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Local,
    /// A closure over an enclosing function.
    Closure,
    /// A `catch` block's binding.
    Catch,
    /// A `with` block.
    With,
    /// A `let`/`const` block scope.
    Block,
    /// Function parameters.
    FunctionName,
    /// The global lexical environment, distinct from the global object.
    GlobalLexicalEnvironment,
    /// An ES module's top level.
    NestedLexical,
}

/// One scope in a frame's chain.
#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    /// The remote object holding the bindings. **Invalid after resume.**
    pub object_id: Option<String>,
    /// A label for `with` and closure scopes.
    pub name: Option<String>,
    /// Lazily expanded — a global scope has thousands of properties and must
    /// never be fetched just because a pause happened.
    pub values: Option<ValueTree>,
}

/// One frame of the call stack.
#[derive(Debug, Clone)]
pub struct CallFrame {
    /// The debuggee's handle for this frame, needed by
    /// `Debugger.evaluateOnCallFrame`.
    pub id: String,
    /// The function's name, or something readable for anonymous functions.
    pub function_name: String,
    pub location: SourceLocation,
    /// Innermost first, matching the protocol.
    pub scopes: Vec<Scope>,
    /// `this` in this frame.
    pub this_object_id: Option<String>,
    /// Whether this frame is blackboxed and should be hidden by default.
    pub is_blackboxed: bool,
}

/// The state of a stopped debuggee.
#[derive(Debug, Clone)]
pub struct PauseState {
    pub reason: PauseReason,
    /// Innermost first.
    pub call_frames: Vec<CallFrame>,
    /// Frames from the async operation that scheduled this one, if any.
    pub async_stack: Vec<CallFrame>,
    /// Which frame the user has selected. Scopes and evaluation follow it.
    pub selected_frame: usize,
}

impl PauseState {
    /// Drop every remote object handle.
    ///
    /// Called on `Debugger.resumed` and `Debugger.globalObjectCleared`. Every
    /// `objectId` in the frames above is dead the instant execution continues;
    /// keeping them means the next expansion fails with a confusing protocol
    /// error rather than simply showing nothing.
    pub fn invalidate(&mut self) {
        for frame in self
            .call_frames
            .iter_mut()
            .chain(self.async_stack.iter_mut())
        {
            frame.this_object_id = None;
            for scope in &mut frame.scopes {
                scope.object_id = None;
                scope.values = None;
            }
        }
    }

    /// The frame the user is looking at.
    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.call_frames.get(self.selected_frame)
    }
}

/// One stepping action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Run the next statement, not entering calls.
    Over,
    /// Enter the next call.
    Into,
    /// Finish this function.
    Out,
    /// Advance one bytecode-level statement — finer than `Over`, WebKit-only.
    Next,
    /// Run until this location.
    ContinueTo(SourceLocation),
    /// Run until the next event-loop turn. WebKit-only, and the cleanest way
    /// to get past a chain of promise callbacks.
    UntilNextRunLoop,
}

/// What pauses execution besides breakpoints.
#[derive(Debug, Clone, PartialEq)]
pub struct PauseConfig {
    pub exceptions: ExceptionPause,
    /// WebKit-only.
    pub on_microtasks: bool,
    /// WebKit-only.
    pub on_assertions: bool,
    /// Whether a `debugger;` statement stops.
    pub on_debugger_statements: bool,
    /// Whether to stop inside WebKit's own internal scripts. Off, always,
    /// unless you are debugging WebKit.
    pub in_internal_scripts: bool,
}

impl Default for PauseConfig {
    fn default() -> Self {
        Self {
            exceptions: ExceptionPause::Uncaught,
            on_microtasks: false,
            on_assertions: false,
            on_debugger_statements: true,
            in_internal_scripts: false,
        }
    }
}

/// Which exceptions stop execution. Chrome's two independent checkboxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionPause {
    None,
    Uncaught,
    All,
}
