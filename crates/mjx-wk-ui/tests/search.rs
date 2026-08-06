//! SearchBar — done-criteria for T-012.
//!
//! The bar is a pure function of hits: it must not reorder what the caller
//! merged, and clicking a hit must open that source.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use egui::Color32;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::{SearchHit, SearchIndex, SourceId, SourceLocation};
use mjx_wk_ui::search::SearchBar;
use mjx_wk_ui::{Action, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct AllNative;

impl SupportQuery for AllNative {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Native
    }
}

fn test_theme() -> Theme {
    Theme {
        is_dark: true,
        background: Color32::from_rgb(0x1e, 0x1e, 0x1e),
        panel: Color32::from_rgb(0x25, 0x25, 0x26),
        gutter: Color32::from_rgb(0x2d, 0x2d, 0x2d),
        hairline: Color32::from_rgb(0x3c, 0x3c, 0x3c),
        text: Color32::from_rgb(0xdc, 0xdc, 0xdc),
        text_dim: Color32::from_rgb(0x9d, 0x9d, 0x9d),
        accent: Color32::from_rgb(0x0e, 0x63, 0x9c),
        syntax_keyword: Color32::from_rgb(0xc5, 0x86, 0xc0),
        syntax_string: Color32::from_rgb(0xce, 0x91, 0x78),
        syntax_number: Color32::from_rgb(0xb5, 0xce, 0xa8),
        syntax_comment: Color32::from_rgb(0x6a, 0x99, 0x55),
        syntax_function: Color32::from_rgb(0xdc, 0xdc, 0xaa),
        syntax_type: Color32::from_rgb(0x4e, 0xc9, 0xb0),
        syntax_property: Color32::from_rgb(0x9c, 0xdc, 0xfe),
        syntax_tag: Color32::from_rgb(0x56, 0x9c, 0xd6),
        breakpoint_resolved: Color32::from_rgb(0xe5, 0x14, 0x00),
        breakpoint_pending: Color32::from_rgb(0x80, 0x80, 0x80),
        breakpoint_conditional: Color32::from_rgb(0xf5, 0xa6, 0x23),
        breakpoint_logpoint: Color32::from_rgb(0xb1, 0x80, 0xd7),
        execution_line: Color32::from_rgb(0xff, 0xcc, 0x00),
        row_height: 18.0,
        gutter_width: 48.0,
        indent_width: 16.0,
        monospace_size: 13.0,
    }
}

fn hit(source: u32, line: u32, text: &str) -> SearchHit {
    SearchHit {
        location: SourceLocation {
            source: SourceId(source),
            line,
            column: 0,
        },
        line_text: text.into(),
        match_range: 0..text.len().min(4) as u32,
    }
}

struct BarState {
    bar: SearchBar,
    theme: Theme,
    support: AllNative,
    hits: Vec<SearchHit>,
    actions: Vec<Action>,
}

#[test]
fn typing_emits_search_action() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut BarState| {
            let ctx = PanelCtx {
                theme: &state.theme,
                support: &state.support,
            };
            let actions = state.bar.ui(ui, &ctx, &state.hits);
            state.actions.extend(actions);
        },
        BarState {
            bar: SearchBar::new(),
            theme: test_theme(),
            support: AllNative,
            hits: Vec::new(),
            actions: Vec::new(),
        },
    );

    let edit = harness.get_by_role(Role::TextInput);
    edit.focus();
    harness.run();
    harness.get_by_role(Role::TextInput).type_text("foo");
    harness.run();

    let actions = &harness.state().actions;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Search(s) if s == "foo")),
        "expected Search(\"foo\"), got {actions:?}"
    );
    assert_eq!(harness.state().bar.query().text, "foo");
}

#[test]
fn clicking_a_hit_opens_that_source_at_its_line() {
    let hits = vec![
        hit(1, 0, "first match"),
        hit(2, 4, "second match"),
        hit(3, 9, "third match"),
    ];

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut BarState| {
            let ctx = PanelCtx {
                theme: &state.theme,
                support: &state.support,
            };
            let actions = state.bar.ui(ui, &ctx, &state.hits);
            state.actions.extend(actions);
        },
        BarState {
            bar: SearchBar::new(),
            theme: test_theme(),
            support: AllNative,
            hits,
            actions: Vec::new(),
        },
    );
    harness.run();

    harness.get_by_label_contains("source#2:5").click();
    harness.run();

    let actions = &harness.state().actions;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::OpenSource(SourceId(2), Some(4)))),
        "expected OpenSource(2, Some(4)), got {actions:?}"
    );
}

#[test]
fn hit_list_keeps_caller_order_when_remote_appends() {
    let local = vec![hit(1, 0, "alpha"), hit(2, 1, "beta")];
    let remote = vec![hit(2, 1, "beta"), hit(9, 0, "gamma")];
    let merged = SearchIndex::merge_remote(local, remote);
    assert_eq!(merged[0].location.source, SourceId(1));
    assert_eq!(merged[1].location.source, SourceId(2));
    assert_eq!(merged[2].location.source, SourceId(9));

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut BarState| {
            let ctx = PanelCtx {
                theme: &state.theme,
                support: &state.support,
            };
            let _ = state.bar.ui(ui, &ctx, &state.hits);
        },
        BarState {
            bar: SearchBar::new(),
            theme: test_theme(),
            support: AllNative,
            hits: merged,
            actions: Vec::new(),
        },
    );
    harness.run();

    let labels: Vec<String> = harness
        .query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| l.contains("source#"))
        .collect();
    let i1 = labels
        .iter()
        .position(|l| l.contains("source#1:"))
        .expect("source#1");
    let i9 = labels
        .iter()
        .position(|l| l.contains("source#9:"))
        .expect("source#9");
    assert!(
        i1 < i9,
        "local hit must stay above remote append: {labels:?}"
    );
}

#[test]
fn invalid_regex_is_reported_on_the_bar() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut BarState| {
            let ctx = PanelCtx {
                theme: &state.theme,
                support: &state.support,
            };
            let _ = state.bar.ui(ui, &ctx, &[]);
        },
        BarState {
            bar: SearchBar::new(),
            theme: test_theme(),
            support: AllNative,
            hits: Vec::new(),
            actions: Vec::new(),
        },
    );

    harness.get_by_label("Regex").click();
    harness.run();
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::TextInput)
        .type_text("[unterminated");
    harness.run();

    assert!(
        harness.state().bar.regex_error().is_some(),
        "invalid regex must be reported on the bar"
    );
}

#[test]
fn megabyte_line_label_stays_bounded() {
    let huge = "z".repeat(2_000_000);
    let hits = vec![SearchHit {
        location: SourceLocation {
            source: SourceId(1),
            line: 0,
            column: 0,
        },
        line_text: huge,
        match_range: 0..1,
    }];

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut BarState| {
            let ctx = PanelCtx {
                theme: &state.theme,
                support: &state.support,
            };
            let _ = state.bar.ui(ui, &ctx, &state.hits);
        },
        BarState {
            bar: SearchBar::new(),
            theme: test_theme(),
            support: AllNative,
            hits,
            actions: Vec::new(),
        },
    );
    harness.run();

    for node in harness.query_all_by_role(Role::Button) {
        if let Some(label) = node.accesskit_node().label() {
            assert!(
                label.len() < 4_000,
                "hit label must truncate, got {} bytes",
                label.len()
            );
        }
    }
}
