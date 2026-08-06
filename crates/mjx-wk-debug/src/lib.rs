//! L4 — breakpoints, pausing, and inspecting state.
//!
//! **Phase 2.** The seam is frozen in Phase 1a so the code view can be built
//! against it: a gutter that knows about [`BreakpointState`] from the start does
//! not need rewriting when breakpoints arrive.
//!
//! # WebKit is richer than Chrome here
//!
//! Chrome's logpoint is one thing. WebKit has [`BreakpointAction`]s — log,
//! evaluate, sound, and **probe**, which samples an expression every time the
//! line runs and shows the values inline without ever stopping. There is also
//! `setPauseOnMicrotasks`, `setPauseOnAssertions`, and symbolic (function-name)
//! breakpoints. None of these exist over CDP, which is why
//! [`Support`](mjx_wk_dialect::Support) is consulted before offering them.
//!
//! # Two rules that are easy to get wrong and hard to notice
//!
//! 1. **Remote object handles die on resume.** Every `objectId` in a paused
//!    scope becomes invalid the moment execution continues. The variable tree
//!    must be dropped on `Debugger.resumed` and on
//!    `Debugger.globalObjectCleared`, or the UI shows stale rows that error
//!    when expanded. See [`pause::PauseState::invalidate`].
//! 2. **Breakpoints are set by URL, not by script id.** That is what makes them
//!    survive a reload. The debuggee answers with `breakpointResolved` giving
//!    the *actual* location, which may not be the line asked for — a breakpoint
//!    on a blank line moves to the next statement. The UI must show requested
//!    and resolved differently, which is Chrome's grey-versus-blue distinction.

pub mod breakpoints;
pub mod pause;
pub mod values;

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::{NormalizedFrame, Support};
use mjx_wk_protocol::generated::debugger::{
    self, BreakpointActionType, BreakpointOptions, Location as ProtocolLocation,
};
use mjx_wk_protocol::generated::runtime;
use mjx_wk_protocol::{Domain, Frame};
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};
use mjx_wk_source::{SourceId, SourceLocation};
use tracing::warn;

pub use breakpoints::{
    Breakpoint, BreakpointAction, BreakpointActionKind, BreakpointId, BreakpointSpec,
    BreakpointState, BreakpointStore, DomBreakpoint, EventBreakpoint, SymbolicBreakpoint,
    UrlBreakpoint,
};
pub use pause::{CallFrame, PauseConfig, PauseReason, PauseState, Scope, ScopeKind, StepKind};
pub use values::{ValueNode, ValueNodeId, ValuePreview, ValueTree};

/// Protocol members the debugger panel needs.
///
/// If any is [`Support::Unsupported`], the panel must render **disabled with a
/// reason**, never hidden and never silently broken.
pub const DEBUG_PANEL_REQUIRES: &[(Domain, &str)] = &[
    (Domain::Debugger, "enable"),
    (Domain::Debugger, "setBreakpointByUrl"),
];

/// Everything the debugger panel displays.
#[derive(Debug, Default, Clone)]
pub struct DebugModel {
    /// Breakpoints, whether or not they have resolved.
    pub breakpoints: BreakpointStore,
    /// The current pause, if execution is stopped.
    pub paused: Option<PauseState>,
    /// Whether breakpoints are armed at all — Chrome's "deactivate breakpoints".
    pub breakpoints_active: bool,
    /// What causes a pause besides breakpoints.
    pub pause_config: PauseConfig,
    /// URL patterns excluded from stepping and pausing.
    pub blackboxed: Vec<String>,
    /// Watch expressions, re-evaluated on every pause and step.
    pub watches: Vec<String>,
    /// When `Some`, the panel renders disabled with this explanation.
    pub disabled_reason: Option<String>,
}

/// How a URL is matched when setting a line breakpoint.
///
/// Always a URL (or URL regex) — never a script id. That is what makes
/// breakpoints survive `Page.reload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakpointUrl {
    /// Exact resource URL.
    Exact(String),
    /// Regex over resource URLs (as in the `breakpoint-hit` fixture).
    Regex(String),
}

/// Owns `Debugger` and `DOMDebugger`.
#[derive(Debug, Default)]
pub struct DebugAgent {
    model: DebugModel,
    /// Object groups opened for evaluation / property walks. Released on detach
    /// so we do not pin values in the debuggee's heap after we leave.
    object_groups: Vec<String>,
}

impl DebugAgent {
    pub fn new() -> Self {
        Self::default()
    }

    /// Why the debugger panel should be disabled, if it should.
    ///
    /// Checked against both capability axes via [`SessionHandle::supports`].
    pub fn disabled_reason(session: &SessionHandle) -> Option<String> {
        for &(domain, member) in DEBUG_PANEL_REQUIRES {
            if !session.supports(domain, member).is_available() {
                return Some(format!(
                    "`{}.{member}` is unavailable on this target",
                    domain.as_str()
                ));
            }
        }
        None
    }

    /// Whether a WebKit-only debugger member is offerable here.
    ///
    /// Probe / sound / microtask pause and friends must be greyed on CDP with
    /// a reason, not offered as if they would work.
    pub fn member_support(session: &SessionHandle, member: &str) -> Support {
        session.supports(Domain::Debugger, member)
    }

    /// Remember an object group we opened so [`Self::detach`] can release it.
    pub fn note_object_group(&mut self, group: impl Into<String>) {
        let group = group.into();
        if !self.object_groups.iter().any(|g| g == &group) {
            self.object_groups.push(group);
        }
    }

    /// Direct access to the breakpoint store (tests and the future action path).
    pub fn breakpoints_mut(&mut self) -> &mut BreakpointStore {
        &mut self.model.breakpoints
    }

    /// Set a line breakpoint by URL. Never by script id.
    ///
    /// Inserts into the store as [`BreakpointState::Pending`], sends
    /// `Debugger.setBreakpointByUrl`, then fills the id and resolves any
    /// locations returned immediately. Further matches after a reload arrive as
    /// `breakpointResolved` events — the breakpoint is **not** re-sent.
    pub async fn set_breakpoint_by_url(
        &mut self,
        session: &SessionHandle,
        url: BreakpointUrl,
        spec: BreakpointSpec,
    ) -> Result<usize, SessionError> {
        let index = self.model.breakpoints.insert(spec.clone());
        if !spec.enabled {
            return Ok(index);
        }

        let (url_exact, url_regex) = match &url {
            BreakpointUrl::Exact(u) => (Some(u.clone()), None),
            BreakpointUrl::Regex(r) => (None, Some(r.clone())),
        };

        let options = breakpoint_options(&spec);

        let result = session
            .call(debugger::commands::SetBreakpointByUrl {
                line_number: i64::from(spec.location.line),
                url: url_exact,
                url_regex,
                column_number: Some(i64::from(spec.location.column)),
                options,
            })
            .await;

        match result {
            Ok(ret) => {
                let id = BreakpointId(ret.breakpoint_id);
                self.model.breakpoints.set_id(index, id.clone());
                // Immediate resolve into already-parsed scripts. Keep the
                // requested SourceId — script ids die on reload; ours do not.
                if let Some(loc) = ret.locations.first() {
                    let actual = source_location_from_protocol(spec.location.source, loc);
                    self.model.breakpoints.resolve(&id, actual);
                }
                Ok(index)
            }
            Err(err) => {
                self.model
                    .breakpoints
                    .fail(index, format!("setBreakpointByUrl failed: {err}"));
                Err(err)
            }
        }
    }

    fn handle_breakpoint_resolved(&mut self, params: &serde_json::Value) {
        let Some(id) = params
            .get("breakpointId")
            .and_then(|v| v.as_str())
            .map(|s| BreakpointId(s.to_owned()))
        else {
            warn!("breakpointResolved without breakpointId");
            return;
        };
        let Some(location) = params.get("location") else {
            warn!("breakpointResolved without location");
            return;
        };
        let Ok(loc) = serde_json::from_value::<ProtocolLocation>(location.clone()) else {
            warn!("breakpointResolved location did not decode");
            return;
        };
        let source = self
            .model
            .breakpoints
            .find_index(&id)
            .and_then(|i| self.model.breakpoints.all().get(i))
            .map(|bp| bp.spec.location.source)
            .unwrap_or(SourceId(0));
        let actual = source_location_from_protocol(source, &loc);
        self.model.breakpoints.resolve(&id, actual);
    }

    fn handle_paused(&mut self, params: &serde_json::Value) {
        // Hit counting only here — full PauseState materialisation is T-202.
        if let Some(id) = params
            .pointer("/data/breakpointId")
            .and_then(|v| v.as_str())
        {
            self.model
                .breakpoints
                .record_hit(&BreakpointId(id.to_owned()));
        }
    }

    fn handle_execution_continued(&mut self) {
        // Remote object handles die on resume / globalObjectCleared.
        if let Some(paused) = self.model.paused.as_mut() {
            paused.invalidate();
        }
        self.model.paused = None;
    }
}

#[async_trait]
impl DomainAgent for DebugAgent {
    type Model = DebugModel;

    const DOMAINS: &'static [Domain] = &[Domain::Debugger, Domain::DomDebugger];
    const NAME: &'static str = "debug";

    async fn attach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        if let Some(reason) = Self::disabled_reason(session) {
            self.model.disabled_reason = Some(reason.clone());
            return Err(SessionError::Unsupported {
                domain: Domain::Debugger,
                member: "enable".into(),
                reason: mjx_wk_session::UnsupportedReason::Dialect,
            });
        }
        self.model.disabled_reason = None;

        session.call(debugger::commands::Enable {}).await?;
        session
            .call(debugger::commands::SetBreakpointsActive { active: true })
            .await?;
        self.model.breakpoints_active = true;

        // Default group for future evaluates / property walks (T-203). Tracking
        // it here means detach always has something to release when we used it.
        self.note_object_group("mjx-debug");

        Ok(())
    }

    async fn on_event(&mut self, event: &NormalizedFrame) -> Result<(), SessionError> {
        let Frame::Event { method, params } = &event.frame else {
            return Ok(());
        };
        match method.as_str() {
            "Debugger.breakpointResolved" => self.handle_breakpoint_resolved(params),
            "Debugger.paused" => self.handle_paused(params),
            "Debugger.resumed" | "Debugger.globalObjectCleared" => {
                self.handle_execution_continued();
            }
            _ => {}
        }
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(self.model.clone())
    }

    async fn detach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        // Remote objects pin JavaScript values in the debuggee's heap. Leaving
        // them behind leaks memory in the program under test, which is exactly
        // the thing someone using a debugger is likely to be measuring.
        for group in self.object_groups.drain(..) {
            match session
                .call(runtime::commands::ReleaseObjectGroup {
                    object_group: group.clone(),
                })
                .await
            {
                Ok(_) => {}
                // Session already closed — nothing left to leak into.
                Err(SessionError::Closed) => return Ok(()),
                Err(err) => {
                    warn!(error = %err, object_group = %group, "releaseObjectGroup failed");
                }
            }
        }
        Ok(())
    }
}

fn breakpoint_options(spec: &BreakpointSpec) -> Option<BreakpointOptions> {
    let has_actions = !spec.actions.is_empty();
    let has_condition = spec.condition.is_some();
    let has_ignore = spec.ignore_count > 0;
    let has_auto = spec.auto_continue;
    if !has_actions && !has_condition && !has_ignore && !has_auto {
        return None;
    }
    Some(BreakpointOptions {
        condition: spec.condition.clone(),
        actions: has_actions.then(|| {
            spec.actions
                .iter()
                .map(|a| debugger::BreakpointAction {
                    r#type: match a.kind {
                        BreakpointActionKind::Log => BreakpointActionType::Log,
                        BreakpointActionKind::Evaluate => BreakpointActionType::Evaluate,
                        BreakpointActionKind::Probe => BreakpointActionType::Probe,
                        BreakpointActionKind::Sound => BreakpointActionType::Sound,
                    },
                    data: a.data.clone(),
                    id: None,
                    emulate_user_gesture: None,
                })
                .collect()
        }),
        auto_continue: has_auto.then_some(true),
        ignore_count: has_ignore.then_some(i64::from(spec.ignore_count)),
    })
}

fn source_location_from_protocol(source: SourceId, loc: &ProtocolLocation) -> SourceLocation {
    SourceLocation {
        source,
        line: loc.line_number.max(0) as u32,
        column: loc.column_number.unwrap_or(0).max(0) as u32,
    }
}

#[cfg(test)]
mod options_tests {
    use super::*;
    use mjx_wk_protocol::generated::debugger::BreakpointActionType;

    fn loc(line: u32) -> SourceLocation {
        SourceLocation {
            source: SourceId(1),
            line,
            column: 0,
        }
    }

    #[test]
    fn logpoint_options_include_auto_continue_and_log_action() {
        let mut spec = BreakpointSpec::at(loc(10));
        spec.auto_continue = true;
        spec.actions.push(BreakpointAction {
            kind: BreakpointActionKind::Log,
            data: Some("x=${x}".into()),
        });
        assert!(spec.is_logpoint(), "autoContinue + Log never pauses");

        let options = breakpoint_options(&spec).expect("logpoint must send options");
        assert_eq!(options.auto_continue, Some(true));
        let actions = options.actions.expect("actions");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].r#type, BreakpointActionType::Log);
    }

    #[test]
    fn ignore_count_is_sent_in_options() {
        let mut spec = BreakpointSpec::at(loc(10));
        spec.ignore_count = 3;
        let options = breakpoint_options(&spec).expect("options");
        assert_eq!(options.ignore_count, Some(3));
    }

    #[test]
    fn ordinary_breakpoint_omits_options() {
        let spec = BreakpointSpec::at(loc(3));
        assert!(breakpoint_options(&spec).is_none());
    }
}
