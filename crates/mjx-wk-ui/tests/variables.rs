//! Variables tree widget tests — `docs/tasks/T-203-variable-tree.md`.

use egui::Color32;
use egui_kittest::{Harness, kittest::Queryable};
use mjx_wk_debug::{
    ValuePreview, ValueTree,
    values::{PAGE_SIZE, WatchResult, WatchValue},
};
use mjx_wk_dialect::{CdpDialect, Dialect, Support, WebKitDialect};
use mjx_wk_protocol::Domain;
use mjx_wk_protocol::generated::runtime::{PropertyDescriptor, RemoteObject, RemoteObjectType};
use mjx_wk_ui::variables::{VARIABLES_REQUIRES, VariablesModel, VariablesTree};
use mjx_wk_ui::{Action, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct FixedSupport(Support);

impl SupportQuery for FixedSupport {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        if VARIABLES_REQUIRES
            .iter()
            .any(|&(d, m)| d == domain && m == member)
        {
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

fn test_theme() -> Theme {
    Theme {
        is_dark: true,
        background: Color32::from_rgb(0x1e, 0x1e, 0x1e),
        panel: Color32::from_rgb(0x25, 0x25, 0x26),
        gutter: Color32::from_rgb(0x1e, 0x1e, 0x1e),
        hairline: Color32::from_rgb(0x3c, 0x3c, 0x3c),
        text: Color32::from_rgb(0xcc, 0xcc, 0xcc),
        text_dim: Color32::from_rgb(0x88, 0x88, 0x88),
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
        breakpoint_pending: Color32::from_rgb(0xe5, 0x14, 0x00),
        breakpoint_conditional: Color32::from_rgb(0xe5, 0x14, 0x00),
        breakpoint_logpoint: Color32::from_rgb(0xf5, 0xa6, 0x23),
        execution_line: Color32::from_rgb(0xff, 0xcc, 0x00),
        row_height: 18.0,
        gutter_width: 48.0,
        indent_width: 12.0,
        monospace_size: 12.0,
    }
}

fn remote_number(desc: &str) -> RemoteObject {
    RemoteObject {
        r#type: RemoteObjectType::Number,
        subtype: None,
        class_name: None,
        value: None,
        description: Some(desc.into()),
        object_id: None,
        size: None,
        class_prototype: None,
        preview: None,
    }
}

fn prop(name: &str, desc: &str) -> PropertyDescriptor {
    PropertyDescriptor {
        name: name.into(),
        value: Some(remote_number(desc)),
        writable: Some(true),
        get: None,
        set: None,
        was_thrown: None,
        configurable: Some(true),
        enumerable: Some(true),
        is_own: Some(true),
        symbol: None,
        is_private: None,
        native_getter: None,
    }
}

struct VarsUiState<S: SupportQuery> {
    widget: VariablesTree,
    theme: Theme,
    support: S,
    tree: ValueTree,
    watches: Vec<String>,
    actions: Vec<Action>,
}

fn paint_vars<S: SupportQuery>(ui: &mut egui::Ui, state: &mut VarsUiState<S>) {
    let ctx = PanelCtx {
        theme: &state.theme,
        support: &state.support,
    };
    let model = VariablesModel {
        values: Some(&state.tree),
        watches: &state.watches,
    };
    let frame = state.widget.ui(ui, &ctx, &model);
    state.actions.extend(frame);
}

#[test]
fn expanding_unfetched_row_emits_paginated_expand_value() {
    let mut tree = ValueTree::new();
    let root = tree.push_root(
        "Local",
        Some("scope-1".into()),
        ValuePreview {
            type_name: "object".into(),
            subtype: None,
            description: "Object".into(),
            has_children: true,
        },
    );
    assert!(tree.needs_fetch(root));

    let state = VarsUiState {
        widget: VariablesTree::new(),
        theme: test_theme(),
        support: FixedSupport(Support::Native),
        tree,
        watches: Vec::new(),
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 480.0))
        .build_ui_state(paint_vars, state);

    harness.run();
    harness.state_mut().actions.clear();
    harness.get_by_label("▸").click();
    harness.step();

    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::ExpandValue {
                node,
                start: 0,
                count: PAGE_SIZE
            } if *node == root.0
        )),
        "expected ExpandValue for unfetched root, got {:?}",
        harness.state().actions
    );
}

#[test]
fn show_more_click_emits_next_page() {
    let mut tree = ValueTree::new();
    let root = tree.push_root(
        "obj",
        Some("o".into()),
        ValuePreview {
            type_name: "object".into(),
            subtype: None,
            description: "Object".into(),
            has_children: true,
        },
    );
    // A short-but-full page (fetchCount == returned length) means more remain,
    // without needing 100 painted children for the click target.
    const PAGE: u32 = 5;
    let page: Vec<_> = (0..PAGE).map(|i| prop(&format!("k{i}"), "0")).collect();
    tree.apply_properties(root, 0, PAGE, &page, &[], Some("o"));
    assert_eq!(tree.remaining(root), Some(PAGE_SIZE));

    let mut widget = VariablesTree::new();
    widget.set_expanded(root, true);

    let state = VarsUiState {
        widget,
        theme: test_theme(),
        support: FixedSupport(Support::Native),
        tree,
        watches: Vec::new(),
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 480.0))
        .build_ui_state(paint_vars, state);

    harness.run();
    harness.state_mut().actions.clear();
    harness.get_by_label_contains("Show more").click();
    harness.step();

    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::ExpandValue {
                node,
                start: PAGE,
                count: PAGE_SIZE
            } if *node == root.0
        )),
        "expected next-page ExpandValue, got {:?}",
        harness.state().actions
    );
}

#[test]
fn unsupported_runtime_renders_disabled_with_reason() {
    let state = VarsUiState {
        widget: VariablesTree::new(),
        theme: test_theme(),
        support: FixedSupport(Support::Unsupported),
        tree: ValueTree::new(),
        watches: Vec::new(),
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 200.0))
        .build_ui_state(paint_vars, state);

    harness.run();
    harness.get_by_label_contains("Variables unavailable");
    harness.get_by_label_contains("getProperties");
    assert!(harness.state().actions.is_empty());
}

#[test]
fn webkit_and_cdp_dialects_both_enable_variables() {
    assert_eq!(
        WebKitDialect.supports(Domain::Runtime, "getProperties"),
        Support::Native
    );
    assert_eq!(
        CdpDialect.supports(Domain::Runtime, "getProperties"),
        Support::Native
    );

    // WebKit
    {
        let mut tree = ValueTree::new();
        tree.push_root(
            "Local",
            Some("s".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        let state = VarsUiState {
            widget: VariablesTree::new(),
            theme: test_theme(),
            support: DialectSupport(WebKitDialect),
            tree,
            watches: Vec::new(),
            actions: Vec::new(),
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(320.0, 200.0))
            .build_ui_state(paint_vars, state);
        harness.run();
        harness.get_by_label_contains("Local:");
    }

    // CDP — pagination is emulated client-side, but the member is Native.
    {
        let mut tree = ValueTree::new();
        tree.push_root(
            "Local",
            Some("s".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        let state = VarsUiState {
            widget: VariablesTree::new(),
            theme: test_theme(),
            support: DialectSupport(CdpDialect),
            tree,
            watches: Vec::new(),
            actions: Vec::new(),
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(320.0, 200.0))
            .build_ui_state(paint_vars, state);
        harness.run();
        harness.get_by_label_contains("Local:");
    }
}

#[test]
fn accessor_click_requests_opt_in_invoke() {
    let mut tree = ValueTree::new();
    let root = tree.push_root(
        "obj",
        Some("holder".into()),
        ValuePreview {
            type_name: "object".into(),
            subtype: None,
            description: "Object".into(),
            has_children: true,
        },
    );
    let getter = PropertyDescriptor {
        name: "dangerous".into(),
        value: None,
        writable: None,
        get: Some(RemoteObject {
            r#type: RemoteObjectType::Function,
            subtype: None,
            class_name: None,
            value: None,
            description: Some("function".into()),
            object_id: Some("getter-1".into()),
            size: None,
            class_prototype: None,
            preview: None,
        }),
        set: None,
        was_thrown: None,
        configurable: Some(true),
        enumerable: Some(true),
        is_own: Some(true),
        symbol: None,
        is_private: None,
        native_getter: None,
    };
    tree.apply_properties(root, 0, PAGE_SIZE, &[getter], &[], Some("holder"));
    let child = tree.get(root).unwrap().children.as_ref().unwrap()[0];

    let mut widget = VariablesTree::new();
    widget.set_expanded(root, true);

    let state = VarsUiState {
        widget,
        theme: test_theme(),
        support: FixedSupport(Support::Native),
        tree,
        watches: Vec::new(),
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 480.0))
        .build_ui_state(paint_vars, state);

    harness.run();
    harness.state_mut().actions.clear();
    harness.get_by_label_contains("(...)").click();
    harness.step();

    assert!(
        harness.state().actions.iter().any(|a| matches!(
            a,
            Action::ExpandValue {
                node,
                start: 0,
                count: 0
            } if *node == child.0
        )),
        "expected accessor invoke ExpandValue, got {:?}",
        harness.state().actions
    );
}

#[test]
fn cleared_tree_drops_expansion_state() {
    let mut tree = ValueTree::new();
    let root = tree.push_root(
        "Local",
        Some("s".into()),
        ValuePreview {
            type_name: "object".into(),
            subtype: None,
            description: "Object".into(),
            has_children: true,
        },
    );
    let mut widget = VariablesTree::new();
    widget.set_expanded(root, true);
    assert!(widget.is_expanded(root));

    tree.clear();
    let state = VarsUiState {
        widget,
        theme: test_theme(),
        support: FixedSupport(Support::Native),
        tree,
        watches: Vec::new(),
        actions: Vec::new(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 200.0))
        .build_ui_state(paint_vars, state);
    harness.run();
    assert!(!harness.state().widget.is_expanded(root));
}

#[test]
fn watch_roots_track_reevaluation() {
    let mut tree = ValueTree::new();
    tree.set_watch_roots([WatchResult {
        expression: "a".into(),
        value: WatchValue::Ready(Box::new(remote_number("1"))),
    }]);
    let watches = vec!["a".to_owned()];
    let mut widget = VariablesTree::new();
    {
        let model = VariablesModel {
            values: Some(&tree),
            watches: &watches,
        };
        // watch row + editor
        assert_eq!(widget.visible_row_count(&model), 2);
    }

    tree.set_watch_roots([WatchResult {
        expression: "a".into(),
        value: WatchValue::Ready(Box::new(remote_number("7"))),
    }]);
    let model = VariablesModel {
        values: Some(&tree),
        watches: &watches,
    };
    assert_eq!(widget.visible_row_count(&model), 2);
}
