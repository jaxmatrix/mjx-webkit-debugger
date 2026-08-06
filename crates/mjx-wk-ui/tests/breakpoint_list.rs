//! Breakpoint list + gutter context menu — `docs/tasks/T-207-breakpoint-ui.md`.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mjx_wk_debug::{
    Breakpoint, BreakpointAction, BreakpointActionKind, BreakpointSpec, BreakpointState,
};
use mjx_wk_dialect::{CdpDialect, Dialect, Support, WebKitDialect};
use mjx_wk_protocol::Domain;
use mjx_wk_source::{SourceId, SourceLocation};
use mjx_wk_ui::breakpoint_list::{
    BREAKPOINT_LIST_REQUIRES, BreakpointEdit, BreakpointList, BreakpointListModel,
    PROBE_ACTION_PREFIX, decode_breakpoint_action, encode_probe_action, probe_supported,
};
use mjx_wk_ui::code_view::{BreakpointMark, CodeView, CodeViewModel, SyntheticSource};
use mjx_wk_ui::{Action, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct FixedSupport(Support);

impl SupportQuery for FixedSupport {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        if BREAKPOINT_LIST_REQUIRES
            .iter()
            .any(|&(d, m)| d == domain && m == member)
        {
            self.0
        } else if member == "setPauseOnMicrotasks" {
            // Match Native for WebKit-shaped FixedSupport(Native).
            self.0
        } else {
            Support::Native
        }
    }
}

#[derive(Debug)]
struct DialectSupport<D: Dialect + std::fmt::Debug>(D);

impl<D: Dialect + std::fmt::Debug> SupportQuery for DialectSupport<D> {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        self.0.supports(domain, member)
    }
}

fn loc(source: u32, line: u32) -> SourceLocation {
    SourceLocation {
        source: SourceId(source),
        line,
        column: 0,
    }
}

fn bp_at(source: u32, line: u32, state: BreakpointState) -> Breakpoint {
    let mut spec = BreakpointSpec::at(loc(source, line));
    if matches!(state, BreakpointState::Disabled) {
        spec.enabled = false;
    }
    Breakpoint {
        id: None,
        spec,
        state,
        hit_count: 0,
    }
}

struct ListState<S: SupportQuery> {
    widget: BreakpointList,
    theme: Theme,
    support: S,
    breakpoints: Vec<Breakpoint>,
    source_names: Vec<(SourceId, String)>,
    actions: Vec<Action>,
}

fn paint_list<S: SupportQuery>(ui: &mut egui::Ui, state: &mut ListState<S>) {
    let ctx = PanelCtx {
        theme: &state.theme,
        support: &state.support,
    };
    let names: Vec<(SourceId, &str)> = state
        .source_names
        .iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();
    let model = BreakpointListModel {
        breakpoints: &state.breakpoints,
        source_names: &names,
    };
    let frame = state.widget.ui(ui, &ctx, &model);
    state.actions.extend(frame);
}

#[test]
fn list_groups_by_source_and_links_to_line() {
    let state = ListState {
        widget: BreakpointList::new(),
        theme: Theme::dark(),
        support: FixedSupport(Support::Native),
        breakpoints: vec![
            bp_at(1, 2, BreakpointState::Pending),
            bp_at(1, 10, BreakpointState::Resolved { actual: loc(1, 10) }),
            bp_at(2, 0, BreakpointState::Pending),
        ],
        source_names: vec![
            (SourceId(1), "app.js".into()),
            (SourceId(2), "lib.js".into()),
        ],
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 320.0))
        .build_ui_state(paint_list, state);

    harness.run();
    harness.get_by_label("app.js");
    harness.get_by_label("lib.js");
    harness.state_mut().actions.clear();
    harness.get_by_label_contains("breakpoint: 3:0").click();
    harness.step();

    assert!(
        harness
            .state()
            .actions
            .iter()
            .any(|a| matches!(a, Action::OpenSource(SourceId(1), Some(2)))),
        "expected OpenSource to line 2, got {:?}",
        harness.state().actions
    );
}

#[test]
fn disabled_breakpoint_stays_visible() {
    let state = ListState {
        widget: BreakpointList::new(),
        theme: Theme::dark(),
        support: FixedSupport(Support::Native),
        breakpoints: vec![bp_at(1, 4, BreakpointState::Disabled)],
        source_names: vec![(SourceId(1), "app.js".into())],
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 200.0))
        .build_ui_state(paint_list, state);

    harness.run();
    harness.get_by_label("app.js");
    harness.get_by_label_contains("breakpoint disabled:");
    harness.fit_contents();
    harness.snapshot("breakpoint_list_disabled");
}

#[test]
fn unsupported_renders_disabled_with_reason() {
    let state = ListState {
        widget: BreakpointList::new(),
        theme: Theme::dark(),
        support: FixedSupport(Support::Unsupported),
        breakpoints: vec![bp_at(1, 0, BreakpointState::Pending)],
        source_names: vec![],
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 160.0))
        .build_ui_state(paint_list, state);

    harness.run();
    harness.get_by_label_contains("Breakpoints unavailable");
    // First required member in DEBUG_PANEL_REQUIRES is Debugger.enable.
    harness.get_by_label_contains("Debugger.enable");
    assert!(harness.state().actions.is_empty());
}

#[test]
fn webkit_and_cdp_dialects_gate_panel_and_probe() {
    assert_eq!(
        WebKitDialect.supports(Domain::Debugger, "setBreakpointByUrl"),
        Support::Native
    );
    assert_eq!(
        CdpDialect.supports(Domain::Debugger, "setBreakpointByUrl"),
        Support::Native
    );
    assert!(
        WebKitDialect
            .supports(Domain::Debugger, "setPauseOnMicrotasks")
            .is_available()
    );
    assert!(
        !CdpDialect
            .supports(Domain::Debugger, "setPauseOnMicrotasks")
            .is_available()
    );

    // WebKit — panel interactive, Probe offered.
    {
        let theme = Theme::dark();
        let support = DialectSupport(WebKitDialect);
        let ctx = PanelCtx {
            theme: &theme,
            support: &support,
        };
        assert!(probe_supported(&ctx));
        assert!(unavailable_is_none(&ctx));

        let state = ListState {
            widget: BreakpointList::new(),
            theme: Theme::dark(),
            support: DialectSupport(WebKitDialect),
            breakpoints: vec![bp_at(1, 0, BreakpointState::Pending)],
            source_names: vec![(SourceId(1), "a.js".into())],
            actions: Vec::new(),
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(360.0, 200.0))
            .build_ui_state(paint_list, state);
        harness.run();
        harness.get_by_label_contains("breakpoint:");
    }

    // CDP — panel interactive (line BPs exist), Probe not offered.
    {
        let theme = Theme::dark();
        let support = DialectSupport(CdpDialect);
        let ctx = PanelCtx {
            theme: &theme,
            support: &support,
        };
        assert!(!probe_supported(&ctx), "Probe must not be offered over CDP");
        assert!(unavailable_is_none(&ctx));

        let state = ListState {
            widget: BreakpointList::new(),
            theme: Theme::dark(),
            support: DialectSupport(CdpDialect),
            breakpoints: vec![bp_at(1, 0, BreakpointState::Pending)],
            source_names: vec![(SourceId(1), "a.js".into())],
            actions: Vec::new(),
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(360.0, 200.0))
            .build_ui_state(paint_list, state);
        harness.run();
        harness.get_by_label_contains("breakpoint:");
    }
}

fn unavailable_is_none(ctx: &PanelCtx<'_>) -> bool {
    for &(domain, member) in BREAKPOINT_LIST_REQUIRES {
        if matches!(ctx.support.supports(domain, member), Support::Unsupported) {
            return false;
        }
    }
    true
}

#[test]
fn condition_editor_emits_set_breakpoint_condition() {
    let mut widget = BreakpointList::new();
    widget.begin_edit(BreakpointEdit::Condition {
        location: loc(1, 5),
        draft: "x > 1".into(),
    });
    let state = ListState {
        widget,
        theme: Theme::dark(),
        support: FixedSupport(Support::Native),
        breakpoints: vec![bp_at(1, 5, BreakpointState::Pending)],
        source_names: vec![(SourceId(1), "a.js".into())],
        actions: Vec::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 240.0))
        .build_ui_state(paint_list, state);
    harness.run();
    harness.state_mut().actions.clear();
    // Enter while the editor is focused commits (same path as the Set button).
    harness.key_press(egui::Key::Enter);
    harness.step();
    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::SetBreakpointCondition(l, Some(c))
                if l.line == 5 && c == "x > 1"
        )),
        "got {:?}",
        harness.state().actions
    );
}

#[test]
fn probe_encoding_round_trips() {
    let encoded = encode_probe_action("this.x");
    assert!(encoded.starts_with(PROBE_ACTION_PREFIX));
    let (is_probe, data) = decode_breakpoint_action(&encoded);
    assert!(is_probe);
    assert_eq!(data, "this.x");
    let (is_probe, data) = decode_breakpoint_action("hello");
    assert!(!is_probe);
    assert_eq!(data, "hello");
}

#[test]
fn list_shows_logpoint_and_conditional_detail() {
    let mut conditional = bp_at(1, 1, BreakpointState::Pending);
    conditional.spec.condition = Some("n === 0".into());

    let mut logpoint = bp_at(1, 2, BreakpointState::Pending);
    logpoint.spec.auto_continue = true;
    logpoint.spec.actions.push(BreakpointAction {
        kind: BreakpointActionKind::Log,
        data: Some("hit".into()),
    });

    let state = ListState {
        widget: BreakpointList::new(),
        theme: Theme::dark(),
        support: FixedSupport(Support::Native),
        breakpoints: vec![conditional, logpoint],
        source_names: vec![(SourceId(1), "app.js".into())],
        actions: Vec::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 240.0))
        .build_ui_state(paint_list, state);
    harness.run();
    harness.fit_contents();
    harness.snapshot("breakpoint_list_kinds");
}

// ---- CodeView gutter context menu ----

struct CodeState {
    theme: Theme,
    support_native: bool,
    source_lines: Vec<String>,
    breakpoints: Vec<(u32, BreakpointMark)>,
    actions: Vec<Action>,
    view: CodeView,
}

#[derive(Debug)]
struct MenuSupport {
    native_bp: bool,
}

impl SupportQuery for MenuSupport {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        if domain == Domain::Debugger && member == "setBreakpointByUrl" {
            return if self.native_bp {
                Support::Native
            } else {
                Support::Emulated
            };
        }
        if domain == Domain::Debugger && member == "setPauseOnMicrotasks" {
            return if self.native_bp {
                Support::Native
            } else {
                Support::Unsupported
            };
        }
        Support::Native
    }
}

fn paint_code(ui: &mut egui::Ui, state: &mut CodeState) {
    let support = MenuSupport {
        native_bp: state.support_native,
    };
    let ctx = PanelCtx {
        theme: &state.theme,
        support: &support,
    };
    let line_refs: Vec<&str> = state.source_lines.iter().map(String::as_str).collect();
    let source = SyntheticSource {
        id: SourceId(1),
        line_count: line_refs.len() as u32,
        line: "",
        lines: Some(line_refs.as_slice()),
    };
    let model = CodeViewModel {
        text: &source,
        spans: &[],
        spans_start_line: 0,
        breakpoints: &state.breakpoints,
        execution_line: None,
        inline_values: &[],
    };
    let mut produced = state.view.ui(ui, &ctx, &model);
    state.actions.append(&mut produced);
}

fn secondary_click_at(harness: &Harness<'_, CodeState>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

#[test]
fn gutter_context_menu_offers_condition_logpoint_probe_disable() {
    let lines: Vec<String> = (0..20).map(|i| format!("let x{i} = {i};")).collect();
    let state = CodeState {
        theme: Theme::dark(),
        support_native: true,
        source_lines: lines,
        breakpoints: vec![(5, BreakpointMark::Resolved)],
        actions: Vec::new(),
        view: CodeView::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(paint_code, state);

    harness.run();
    let row_height = Theme::dark().row_height;
    let y = row_height * 5.0 + row_height * 0.5;
    secondary_click_at(&harness, egui::pos2(10.0, y));
    harness.step();

    harness.get_by_label("Edit condition…");
    harness.get_by_label("Add logpoint…");
    harness.get_by_label("Add probe…");
    harness.get_by_label("Disable");
}

#[test]
fn gutter_context_menu_hides_probe_on_cdp() {
    let lines: Vec<String> = (0..20).map(|i| format!("let x{i} = {i};")).collect();
    let state = CodeState {
        theme: Theme::dark(),
        support_native: false,
        source_lines: lines,
        breakpoints: vec![(3, BreakpointMark::Resolved)],
        actions: Vec::new(),
        view: CodeView::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(paint_code, state);

    harness.run();
    let row_height = Theme::dark().row_height;
    let y = row_height * 3.0 + row_height * 0.5;
    secondary_click_at(&harness, egui::pos2(10.0, y));
    harness.step();

    harness.get_by_label("Edit condition…");
    harness.get_by_label("Add logpoint…");
    harness.get_by_label("Disable");
    assert!(
        harness.query_by_label("Add probe…").is_none(),
        "Probe must not appear over CDP-shaped support"
    );
}

#[test]
fn list_disable_checkbox_emits_toggle_breakpoint() {
    let state = ListState {
        widget: BreakpointList::new(),
        theme: Theme::dark(),
        support: FixedSupport(Support::Native),
        breakpoints: vec![bp_at(1, 5, BreakpointState::Resolved { actual: loc(1, 5) })],
        source_names: vec![(SourceId(1), "app.js".into())],
        actions: Vec::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 200.0))
        .build_ui_state(paint_list, state);

    harness.run();
    harness.state_mut().actions.clear();
    // Uncheck the enabled checkbox → ToggleBreakpoint (disable without remove).
    harness.get_by_role(Role::CheckBox).click();
    harness.step();
    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::ToggleBreakpoint(l) if l.line == 5
        )),
        "got {:?}",
        harness.state().actions
    );
}

#[test]
fn gutter_condition_menu_opens_editor_and_emits() {
    let lines: Vec<String> = (0..20).map(|i| format!("let x{i} = {i};")).collect();
    let mut view = CodeView::new();
    view.begin_gutter_edit(BreakpointEdit::Condition {
        location: loc(1, 2),
        draft: "n > 0".into(),
    });
    let state = CodeState {
        theme: Theme::dark(),
        support_native: true,
        source_lines: lines,
        breakpoints: vec![],
        actions: Vec::new(),
        view,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(paint_code, state);

    harness.run();
    assert!(harness.state().view.gutter_edit().is_some());
    harness.state_mut().actions.clear();
    harness.key_press(egui::Key::Enter);
    harness.step();
    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::SetBreakpointCondition(l, Some(c)) if l.line == 2 && c == "n > 0"
        )),
        "got {:?}",
        harness.state().actions
    );
}

#[test]
fn gutter_probe_menu_emits_prefixed_action() {
    let lines: Vec<String> = (0..10).map(|i| format!("x{i}")).collect();
    let mut view = CodeView::new();
    view.begin_gutter_edit(BreakpointEdit::Probe {
        location: loc(1, 1),
        draft: "this.v".into(),
    });
    let state = CodeState {
        theme: Theme::dark(),
        support_native: true,
        source_lines: lines,
        breakpoints: vec![],
        actions: Vec::new(),
        view,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(paint_code, state);
    harness.run();
    harness.state_mut().actions.clear();
    harness.key_press(egui::Key::Enter);
    harness.step();
    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::SetBreakpointAction(l, Some(d))
                if l.line == 1 && d == "probe:this.v"
        )),
        "got {:?}",
        harness.state().actions
    );
}
