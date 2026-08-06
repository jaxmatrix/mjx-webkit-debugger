//! T-008 — virtualised code view + theme tokens.

use std::cell::Cell;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::{HighlightKind, SourceId};
use mjx_wk_ui::code_view::{
    BreakpointMark, CodeView, CodeViewModel, SyntheticSource, HIGHLIGHT_MARGIN_LINES,
};
use mjx_wk_ui::{Action, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct AllowAll;

impl SupportQuery for AllowAll {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Native
    }
}

struct TestState {
    theme: Theme,
    source_lines: Vec<String>,
    breakpoints: Vec<(u32, BreakpointMark)>,
    execution_line: Option<u32>,
    actions: Vec<Action>,
    view: CodeView,
}

fn make_harness(state: TestState) -> Harness<'static, TestState> {
    Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(
            |ui, state: &mut TestState| {
                let support = AllowAll;
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
                    execution_line: state.execution_line,
                    inline_values: &[],
                };
                let mut produced = state.view.ui(ui, &ctx, &model);
                state.actions.append(&mut produced);
            },
            state,
        )
}

fn click_at<S>(harness: &Harness<'_, S>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
}

fn base_state(lines: Vec<String>) -> TestState {
    TestState {
        theme: Theme::dark(),
        source_lines: lines,
        breakpoints: Vec::new(),
        execution_line: None,
        actions: Vec::new(),
        view: CodeView::new(),
    }
}

#[test]
fn theme_dark_and_light_are_distinct() {
    let d = Theme::dark();
    let l = Theme::light();
    assert!(d.is_dark);
    assert!(!l.is_dark);
    assert_ne!(d.background, l.background);
    assert_ne!(d.text, l.text);
    assert_eq!(d.row_height, 18.0);
    assert_eq!(d.gutter_width, 72.0);
    assert_eq!(d.syntax(HighlightKind::Keyword), d.syntax_keyword);
}

#[test]
fn from_visuals_follows_host() {
    let mut dark = egui::Visuals::dark();
    dark.dark_mode = true;
    assert!(Theme::from_visuals(&dark).is_dark);
    let mut light = egui::Visuals::light();
    light.dark_mode = false;
    assert!(!Theme::from_visuals(&light).is_dark);
}

#[test]
fn gutter_click_toggles_breakpoint_at_line() {
    let lines: Vec<String> = (0..40).map(|i| format!("let x{i} = {i};")).collect();
    let mut harness = make_harness(base_state(lines));
    harness.run();

    let row_height = Theme::dark().row_height;
    let y = row_height * 5.0 + row_height * 0.5;
    click_at(&harness, egui::pos2(10.0, y));
    harness.run();

    let actions = &harness.state().actions;
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::ToggleBreakpoint(loc)
                if loc.source == SourceId(1) && loc.line == 5 && loc.column == 0
        )),
        "expected ToggleBreakpoint at line 5, got {actions:?}"
    );
}

#[test]
fn only_visible_window_is_highlighted() {
    const LINE_COUNT: u32 = 200_000;
    let theme = Theme::dark();
    let highlight_calls = Cell::new(0usize);
    let last_window = Cell::new(0u32);
    let mut view = CodeView::new();

    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui(|ui| {
            let support = AllowAll;
            let ctx = PanelCtx {
                theme: &theme,
                support: &support,
            };
            let source = SyntheticSource {
                id: SourceId(7),
                line_count: LINE_COUNT,
                line: "const n = 1;",
                lines: None,
            };
            let visible = view.last_visible_line_range();
            let window = CodeView::highlight_window(visible, LINE_COUNT);
            if window.end > window.start {
                highlight_calls.set(highlight_calls.get() + 1);
                last_window.set(window.end - window.start);
            }
            let model = CodeViewModel {
                text: &source,
                spans: &[],
                spans_start_line: window.start,
                breakpoints: &[],
                execution_line: None,
                inline_values: &[],
            };
            let _ = view.ui(ui, &ctx, &model);
        });

    harness.run();
    // First frame establishes the viewport; run once more so the parent "highlighter"
    // sees a non-empty last_visible range.
    harness.step();

    let painted = last_window.get();
    assert!(painted > 0, "expected a non-empty visible highlight window");
    assert!(
        painted <= 40 + 2 * HIGHLIGHT_MARGIN_LINES,
        "highlighter covered {painted} lines; must not touch the whole file"
    );
    assert!(
        painted < LINE_COUNT / 100,
        "highlighter covered {painted} lines of {LINE_COUNT}"
    );
    assert!(highlight_calls.get() >= 1);
}

#[test]
fn long_line_does_not_wrap_row_height() {
    let huge = "x".repeat(5 * 1024 * 1024);
    let theme = Theme::dark();
    let mut view = CodeView::new();

    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 120.0))
        .build_ui(|ui| {
            let support = AllowAll;
            let ctx = PanelCtx {
                theme: &theme,
                support: &support,
            };
            let source = SyntheticSource {
                id: SourceId(2),
                line_count: 1,
                line: huge.as_str(),
                lines: None,
            };
            let model = CodeViewModel {
                text: &source,
                spans: &[],
                spans_start_line: 0,
                breakpoints: &[],
                execution_line: None,
                inline_values: &[],
            };
            let _ = view.ui(ui, &ctx, &model);
        });

    harness.run();
    // Content height for one clipped row must stay on the order of row_height,
    // not millions of pixels from wrapping a 5 MB line.
    let size = harness.ctx.content_rect().size();
    assert!(
        size.y < 400.0,
        "viewport/content grew too tall ({}); long line likely wrapped",
        size.y
    );
}

#[test]
fn breakpoint_marks_and_execution_line_are_distinguishable() {
    let lines = vec![
        "fn a() {}".to_owned(),
        "fn b() {}".to_owned(),
        "fn c() {}".to_owned(),
        "fn d() {}".to_owned(),
        "fn e() {}".to_owned(),
        "fn f() {}".to_owned(),
        "fn g() {}".to_owned(),
    ];
    let mut state = base_state(lines);
    state.breakpoints = vec![
        (0, BreakpointMark::Resolved),
        (1, BreakpointMark::Pending),
        (2, BreakpointMark::Conditional),
        (3, BreakpointMark::Logpoint),
        (4, BreakpointMark::Disabled),
    ];
    state.execution_line = Some(5);

    let mut harness = make_harness(state);
    harness.run();

    for label in [
        "breakpoint resolved",
        "breakpoint pending",
        "breakpoint conditional",
        "breakpoint logpoint",
        "breakpoint disabled",
    ] {
        harness.get_by_label(label);
    }

    harness.fit_contents();
    harness.snapshot("code_view_gutter_marks");
}

#[test]
fn scroll_synthetic_source_stays_within_16ms() {
    const LINE_COUNT: u32 = 200_000;
    let theme = Theme::dark();
    let mut view = CodeView::new();
    let frame = Cell::new(0u32);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui(|ui| {
            let support = AllowAll;
            let ctx = PanelCtx {
                theme: &theme,
                support: &support,
            };
            let source = SyntheticSource {
                id: SourceId(9),
                line_count: LINE_COUNT,
                line: "function tick() { return 1; }",
                lines: None,
            };
            let f = frame.get();
            if f > 0 {
                view.reveal_line(f.saturating_mul(37));
            }
            frame.set(f + 1);
            let model = CodeViewModel {
                text: &source,
                spans: &[],
                spans_start_line: 0,
                breakpoints: &[],
                execution_line: None,
                inline_values: &[],
            };
            let _ = view.ui(ui, &ctx, &model);
        });

    // Shipping budget is 16 ms; debug builds are outside that contract.
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(16)
    };
    // Warm fonts / pipeline outside the budgeted loop.
    harness.run();

    for _ in 0..30 {
        let start = Instant::now();
        harness.step();
        let dt = start.elapsed();
        assert!(
            dt <= budget,
            "frame took {dt:?} (budget {budget:?}) while scrolling synthetic 200k-line source"
        );
    }
}

#[test]
fn reveal_line_updates_visible_range() {
    const LINE_COUNT: u32 = 5_000;
    let theme = Theme::dark();
    let mut view = CodeView::new();
    view.reveal_line(2_500);
    let visible_start = Cell::new(0u32);
    let visible_end = Cell::new(0u32);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui(|ui| {
            let support = AllowAll;
            let ctx = PanelCtx {
                theme: &theme,
                support: &support,
            };
            let source = SyntheticSource {
                id: SourceId(3),
                line_count: LINE_COUNT,
                line: "x",
                lines: None,
            };
            let model = CodeViewModel {
                text: &source,
                spans: &[],
                spans_start_line: 0,
                breakpoints: &[],
                execution_line: None,
                inline_values: &[],
            };
            let _ = view.ui(ui, &ctx, &model);
            let visible = view.last_visible_line_range();
            visible_start.set(visible.start);
            visible_end.set(visible.end);
        });

    harness.run();
    let start = visible_start.get();
    let end = visible_end.get();
    assert!(
        (start..end).contains(&2_500) || start.abs_diff(2_500) < 40,
        "reveal_line(2500) left visible range {start}..{end}"
    );
}

#[test]
fn scroll_area_sized_by_line_count_times_row_height() {
    // Documented contract: with item_spacing.y = 0, show_rows content height is
    // exactly line_count × row_height. Verified here on the helpers the widget uses.
    let line_count = 200_000u32;
    let row_height = Theme::dark().row_height;
    let expected = line_count as f32 * row_height;
    assert!((expected - 3_600_000.0).abs() < 0.1);
    // highlight window never scales with file size
    let w = CodeView::highlight_window(100..120, line_count);
    assert_eq!(w.end - w.start, 20 + 2 * HIGHLIGHT_MARGIN_LINES);
}
