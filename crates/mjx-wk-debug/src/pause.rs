//! Paused execution: why, where, and what is in scope.
//!
//! **Owned by `docs/tasks/T-202-pause-and-stepping.md`.**

use mjx_wk_protocol::generated::console;
use mjx_wk_protocol::generated::debugger::{
    self, PausedReason, ScopeType, SetPauseOnExceptionsState, events::Paused,
};
use mjx_wk_source::{SourceId, SourceLocation};

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

    /// Whether an incoming event must clear every live `objectId`.
    ///
    /// `Debugger.resumed` and `Debugger.globalObjectCleared` are both fatal to
    /// remote handles — missing either leaves stale rows that look like a
    /// protocol fault when expanded.
    pub fn must_invalidate(qualified_method: &str) -> bool {
        matches!(
            qualified_method,
            "Debugger.resumed" | "Debugger.globalObjectCleared"
        )
    }

    /// The frame the user is looking at.
    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.call_frames.get(self.selected_frame)
    }

    /// Select a sync call-frame for scopes and `evaluateOnCallFrame`.
    ///
    /// Returns `false` when `index` is out of range; selection is unchanged.
    pub fn select_frame(&mut self, index: usize) -> bool {
        if index < self.call_frames.len() {
            self.selected_frame = index;
            true
        } else {
            false
        }
    }

    /// Fold a `Debugger.paused` event into UI-ready state.
    ///
    /// `resolve_script` maps the debuggee's `scriptId` onto a dense
    /// [`SourceId`]. `is_blackboxed` marks frames the call-stack widget should
    /// collapse by default. Neither talks to the wire — the agent supplies
    /// both from the inventory and the blackbox list.
    pub fn from_paused(
        event: &Paused,
        mut resolve_script: impl FnMut(&str) -> Option<SourceId>,
        mut is_blackboxed: impl FnMut(SourceId) -> bool,
    ) -> Self {
        let call_frames: Vec<CallFrame> = event
            .call_frames
            .iter()
            .map(|frame| map_sync_frame(frame, &mut resolve_script, &mut is_blackboxed))
            .collect();

        let mut async_stack = Vec::new();
        if let Some(trace) = event.async_stack_trace.as_ref() {
            flatten_async_stack(
                trace,
                &mut resolve_script,
                &mut is_blackboxed,
                &mut async_stack,
            );
        }

        Self {
            reason: map_reason(event.reason, event.data.as_ref()),
            call_frames,
            async_stack,
            selected_frame: 0,
        }
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

impl StepKind {
    /// Wire member name under `Debugger`, for capability gating.
    pub fn wire_member(self) -> Option<&'static str> {
        Some(match self {
            Self::Over => "stepOver",
            Self::Into => "stepInto",
            Self::Out => "stepOut",
            Self::Next => "stepNext",
            Self::ContinueTo(_) => "continueToLocation",
            Self::UntilNextRunLoop => "continueUntilNextRunLoop",
        })
    }

    /// Members that exist on WebKit RWI and have no CDP counterpart.
    pub fn is_webkit_only(self) -> bool {
        matches!(self, Self::Next | Self::UntilNextRunLoop)
    }
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

impl ExceptionPause {
    /// The `Debugger.setPauseOnExceptions` state value.
    pub fn as_protocol(self) -> SetPauseOnExceptionsState {
        match self {
            Self::None => SetPauseOnExceptionsState::None,
            Self::Uncaught => SetPauseOnExceptionsState::Uncaught,
            Self::All => SetPauseOnExceptionsState::All,
        }
    }
}

fn map_reason(reason: PausedReason, data: Option<&serde_json::Value>) -> PauseReason {
    match reason {
        PausedReason::Breakpoint => PauseReason::Breakpoint,
        PausedReason::DebuggerStatement => PauseReason::DebuggerStatement,
        PausedReason::Exception => {
            // Chrome/WebKit put `uncaught: true` in `data` for uncaught throws.
            let uncaught = data
                .and_then(|d| d.get("uncaught"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            PauseReason::Exception { caught: !uncaught }
        }
        PausedReason::Assert => PauseReason::Assertion,
        PausedReason::Microtask => PauseReason::Microtask,
        PausedReason::PauseOnNextStatement => PauseReason::User,
        PausedReason::FunctionCall => PauseReason::Step,
        PausedReason::Url => PauseReason::Instrumentation {
            detail: "URL".into(),
        },
        PausedReason::Dom => PauseReason::Instrumentation {
            detail: "DOM".into(),
        },
        PausedReason::AnimationFrame => PauseReason::Instrumentation {
            detail: "AnimationFrame".into(),
        },
        PausedReason::Interval => PauseReason::Instrumentation {
            detail: "Interval".into(),
        },
        PausedReason::Listener => PauseReason::Instrumentation {
            detail: "Listener".into(),
        },
        PausedReason::Timeout => PauseReason::Instrumentation {
            detail: "Timeout".into(),
        },
        PausedReason::CspViolation => PauseReason::Instrumentation {
            detail: "CSPViolation".into(),
        },
        PausedReason::BlackboxedScript => PauseReason::Instrumentation {
            detail: "BlackboxedScript".into(),
        },
        PausedReason::Other => PauseReason::Other("other".into()),
    }
}

fn map_scope_kind(ty: ScopeType) -> ScopeKind {
    match ty {
        ScopeType::Global => ScopeKind::Global,
        ScopeType::With => ScopeKind::With,
        ScopeType::Closure => ScopeKind::Closure,
        ScopeType::Catch => ScopeKind::Catch,
        ScopeType::FunctionName => ScopeKind::FunctionName,
        ScopeType::GlobalLexicalEnvironment => ScopeKind::GlobalLexicalEnvironment,
        ScopeType::NestedLexical => ScopeKind::NestedLexical,
    }
}

fn map_location(
    loc: &debugger::Location,
    resolve_script: &mut impl FnMut(&str) -> Option<SourceId>,
) -> SourceLocation {
    let source = resolve_script(&loc.script_id).unwrap_or(SourceId(0));
    SourceLocation {
        source,
        line: saturating_u32(loc.line_number),
        column: saturating_u32(loc.column_number.unwrap_or(0)),
    }
}

fn saturating_u32(n: i64) -> u32 {
    if n <= 0 {
        0
    } else if n >= i64::from(u32::MAX) {
        u32::MAX
    } else {
        n as u32
    }
}

fn display_name(name: &str) -> String {
    if name.is_empty() {
        "(anonymous)".into()
    } else {
        name.to_owned()
    }
}

fn map_sync_frame(
    frame: &debugger::CallFrame,
    resolve_script: &mut impl FnMut(&str) -> Option<SourceId>,
    is_blackboxed: &mut impl FnMut(SourceId) -> bool,
) -> CallFrame {
    let location = map_location(&frame.location, resolve_script);
    let blackboxed = is_blackboxed(location.source);
    CallFrame {
        id: frame.call_frame_id.clone(),
        function_name: display_name(&frame.function_name),
        location,
        scopes: frame
            .scope_chain
            .iter()
            .map(|scope| Scope {
                kind: map_scope_kind(scope.r#type),
                object_id: scope.object.object_id.clone(),
                name: scope.name.clone(),
                values: None,
            })
            .collect(),
        this_object_id: frame.this.object_id.clone(),
        is_blackboxed: blackboxed,
    }
}

fn flatten_async_stack(
    trace: &console::StackTrace,
    resolve_script: &mut impl FnMut(&str) -> Option<SourceId>,
    is_blackboxed: &mut impl FnMut(SourceId) -> bool,
    out: &mut Vec<CallFrame>,
) {
    for (i, frame) in trace.call_frames.iter().enumerate() {
        let source = resolve_script(&frame.script_id).unwrap_or(SourceId(0));
        let location = SourceLocation {
            source,
            line: saturating_u32(frame.line_number),
            column: saturating_u32(frame.column_number),
        };
        let blackboxed = is_blackboxed(source);
        out.push(CallFrame {
            // Async frames have no `callFrameId` — synthesise a stable label
            // so the UI can key rows without pretending evaluation works.
            id: format!("async:{i}:{}:{}", location.line, location.column),
            function_name: display_name(&frame.function_name),
            location,
            scopes: Vec::new(),
            this_object_id: None,
            is_blackboxed: blackboxed,
        });
    }
    if let Some(parent) = trace.parent_stack_trace.as_deref() {
        flatten_async_stack(parent, resolve_script, is_blackboxed, out);
    }
}
