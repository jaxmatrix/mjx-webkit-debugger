//! Call stack widget — `docs/tasks/T-202-pause-and-stepping.md`.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use mjx_wk_debug::{CallFrame, PauseReason, PauseState, Scope, ScopeKind};
use mjx_wk_dialect::{CdpDialect, Dialect, Support, WebKitDialect};
use mjx_wk_protocol::Domain;
use mjx_wk_source::{SourceId, SourceLocation};
use mjx_wk_ui::call_stack::CallStackList;
use mjx_wk_ui::{Action, PanelCtx, StepKind, SupportQuery, Theme};

#[derive(Debug)]
struct AlwaysSupported;

impl SupportQuery for AlwaysSupported {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Native
    }
}

#[derive(Debug)]
struct DialectSupport<D: Dialect>(D);

impl<D: Dialect + std::fmt::Debug> SupportQuery for DialectSupport<D> {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        self.0.supports(domain, member)
    }
}

#[derive(Debug)]
struct NoDebugger;

impl SupportQuery for NoDebugger {
    fn supports(&self, domain: Domain, _member: &str) -> Support {
        if domain == Domain::Debugger {
            Support::Unsupported
        } else {
            Support::Native
        }
    }
}

/// CDP-like: ordinary stepping works; WebKit-only members do not.
#[derive(Debug)]
struct CdpStepping;

impl SupportQuery for CdpStepping {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        if domain == Domain::Debugger && matches!(member, "stepNext" | "continueUntilNextRunLoop") {
            return Support::Unsupported;
        }
        // Mirror CdpDialect for continueUntilNextRunLoop / microtasks, and
        // keep the members CallStackList requires available.
        CdpDialect.supports(domain, member)
    }
}

struct UiState<S: SupportQuery> {
    theme: Theme,
    support: S,
    widget: CallStackList,
    paused: Option<PauseState>,
    actions: Vec<Action>,
}

fn frame(name: &str, line: u32, blackboxed: bool) -> CallFrame {
    CallFrame {
        id: format!("id-{name}-{line}"),
        function_name: name.into(),
        location: SourceLocation {
            source: SourceId(1),
            line,
            column: 0,
        },
        scopes: vec![Scope {
            kind: ScopeKind::Local,
            object_id: Some(format!("scope-{name}")),
            name: None,
            values: None,
        }],
        this_object_id: Some(format!("this-{name}")),
        is_blackboxed: blackboxed,
    }
}

fn sample_paused() -> PauseState {
    PauseState {
        reason: PauseReason::Breakpoint,
        call_frames: vec![
            frame("computeTotal", 3, false),
            frame("vendorInner", 10, true),
            frame("vendorOuter", 20, true),
            frame("boot", 0, false),
        ],
        async_stack: vec![frame("setTimeout", 40, false), frame("main", 1, false)],
        selected_frame: 0,
    }
}

fn make_harness<S: SupportQuery + 'static>(state: UiState<S>) -> Harness<'static, UiState<S>> {
    Harness::builder()
        .with_size(egui::vec2(480.0, 420.0))
        .build_ui_state(
            |ui, state: &mut UiState<S>| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: &state.support,
                };
                let frame_actions = state.widget.ui(ui, &ctx, state.paused.as_ref());
                state.actions.extend(frame_actions);
            },
            state,
        )
}

#[test]
fn pause_shows_stop_location_and_call_stack() {
    let state = UiState {
        theme: Theme::dark(),
        support: AlwaysSupported,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness.query_by_label("Paused on breakpoint").is_some());
    assert!(harness.query_by_label("computeTotal  4:1").is_some());
    assert!(harness.query_by_label("boot  1:1").is_some());
}

#[test]
fn selecting_a_frame_emits_select_frame() {
    let state = UiState {
        theme: Theme::dark(),
        support: AlwaysSupported,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();
    harness.state_mut().actions.clear();

    harness.get_by_label("boot  1:1").click();
    harness.step();

    assert!(
        harness
            .state()
            .actions
            .iter()
            .any(|a| matches!(a, Action::SelectFrame(3))),
        "expected SelectFrame(3), got {:?}",
        harness.state().actions
    );
}

#[test]
fn async_frames_render_distinctly() {
    let state = UiState {
        theme: Theme::dark(),
        support: AlwaysSupported,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness.query_by_label("Async call stack").is_some());
    assert!(harness.query_by_label("setTimeout  41:1 (async)").is_some());
    assert!(harness.query_by_label("main  2:1 (async)").is_some());
}

#[test]
fn blackboxed_frames_collapse_behind_show_more() {
    let state = UiState {
        theme: Theme::dark(),
        support: AlwaysSupported,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness.query_by_label("Show 2 more frames").is_some());
    assert!(harness
        .query_by_label("vendorInner  11:1 (blackboxed)")
        .is_none());
    assert!(harness
        .query_by_label("vendorOuter  21:1 (blackboxed)")
        .is_none());

    harness.get_by_label("Show 2 more frames").click();
    harness.run();

    assert!(harness.state().widget.show_blackboxed());
    assert!(harness
        .query_by_label("vendorInner  11:1 (blackboxed)")
        .is_some());
    assert!(harness
        .query_by_label("vendorOuter  21:1 (blackboxed)")
        .is_some());
}

#[test]
fn webkit_offers_step_next_and_run_loop() {
    let state = UiState {
        theme: Theme::dark(),
        support: DialectSupport(WebKitDialect),
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness.query_by_label("Step over").is_some());
    assert!(harness.query_by_label("Step next").is_some());
    assert!(harness.query_by_label("Next run loop").is_some());

    harness.state_mut().actions.clear();
    harness.get_by_label("Step next").click();
    harness.step();
    assert!(harness
        .state()
        .actions
        .iter()
        .any(|a| matches!(a, Action::Step(StepKind::Next))));
}

#[test]
fn cdp_hides_webkit_only_step_modes() {
    let state = UiState {
        theme: Theme::dark(),
        support: CdpStepping,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness.query_by_label("Step over").is_some());
    assert!(harness.query_by_label("Step into").is_some());
    assert!(harness.query_by_label("Step out").is_some());
    assert!(
        harness.query_by_label("Step next").is_none(),
        "stepNext must be hidden over CDP"
    );
    assert!(
        harness.query_by_label("Next run loop").is_none(),
        "continueUntilNextRunLoop must be hidden over CDP"
    );

    // Real dialect table: continueUntilNextRunLoop is listed Unsupported.
    assert_eq!(
        CdpDialect.supports(Domain::Debugger, "continueUntilNextRunLoop"),
        Support::Unsupported
    );
    assert_eq!(
        WebKitDialect.supports(Domain::Debugger, "continueUntilNextRunLoop"),
        Support::Native
    );
}

#[test]
fn unavailable_debugger_renders_disabled_with_reason() {
    let state = UiState {
        theme: Theme::dark(),
        support: NoDebugger,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();

    assert!(harness
        .query_by_label_contains("Call stack unavailable")
        .is_some());
    assert!(harness.query_by_label("Step over").is_none());
    assert!(harness.query_by_label("computeTotal  4:1").is_none());
}

#[test]
fn step_over_emits_action() {
    let state = UiState {
        theme: Theme::dark(),
        support: AlwaysSupported,
        widget: CallStackList::new(),
        paused: Some(sample_paused()),
        actions: Vec::new(),
    };
    let mut harness = make_harness(state);
    harness.run();
    harness.state_mut().actions.clear();

    harness.get_by_label("Step over").click();
    harness.step();

    assert!(harness
        .state()
        .actions
        .iter()
        .any(|a| matches!(a, Action::Step(StepKind::Over))));
}
