//! Source tree widget tests — `docs/tasks/T-009-source-tree.md`.
//!
//! Inventory (T-004) may still be a stub, so trees are hand-built. Theme
//! (T-008) may still be a stub, so tokens are filled in explicitly here.

use egui::Color32;
use egui_kittest::{
    Harness,
    kittest::Queryable,
};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::{SourceId, SourceTreeNode};
use mjx_wk_ui::source_tree::{SourceTree, group_key};
use mjx_wk_ui::{Action, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct AlwaysSupported;

impl SupportQuery for AlwaysSupported {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Native
    }
}

/// Theme values for tests. `Theme::dark()` belongs to T-008 and may still panic.
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

fn sample_tree() -> SourceTreeNode {
    SourceTreeNode::Group {
        label: "https://example.com".into(),
        children: vec![
            SourceTreeNode::Group {
                label: "js".into(),
                children: vec![
                    SourceTreeNode::Leaf {
                        id: SourceId(1),
                        label: "app.js".into(),
                    },
                    SourceTreeNode::Leaf {
                        id: SourceId(2),
                        label: "vendor.js".into(),
                    },
                ],
            },
            SourceTreeNode::Leaf {
                id: SourceId(3),
                label: "index.html".into(),
            },
        ],
    }
}

/// Origin → single folder → file: the folder chain must collapse so the user
/// does not click through a pointless nesting level.
fn single_child_chain() -> SourceTreeNode {
    SourceTreeNode::Group {
        label: "https://example.com".into(),
        children: vec![SourceTreeNode::Group {
            label: "assets".into(),
            children: vec![SourceTreeNode::Group {
                label: "js".into(),
                children: vec![SourceTreeNode::Leaf {
                    id: SourceId(10),
                    label: "main.js".into(),
                }],
            }],
        }],
    }
}

fn tree_with_extra_sibling(base: SourceTreeNode) -> SourceTreeNode {
    match base {
        SourceTreeNode::Group { label, mut children } => {
            children.push(SourceTreeNode::Leaf {
                id: SourceId(99),
                label: "late.js".into(),
            });
            SourceTreeNode::Group { label, children }
        }
        leaf => leaf,
    }
}

struct TreeUiState {
    widget: SourceTree,
    theme: Theme,
    support: AlwaysSupported,
    tree: SourceTreeNode,
    selected: Option<SourceId>,
    actions: Vec<Action>,
}

#[test]
fn collapse_joins_single_child_group_path() {
    let tree = single_child_chain();
    let mut widget = SourceTree::new();
    let key = group_key(&["https://example.com/assets/js"]);
    widget.set_expanded(key, true);
    assert_eq!(widget.visible_row_count(&tree), 2); // collapsed group + leaf
}

#[test]
fn expansion_survives_inventory_churn() {
    let mut tree = sample_tree();
    let mut widget = SourceTree::new();
    let origin = group_key(&["https://example.com"]);
    let js = group_key(&["https://example.com", "js"]);
    widget.set_expanded(&origin, true);
    widget.set_expanded(&js, true);

    // origin + js + app.js + vendor.js + index.html
    assert_eq!(widget.visible_row_count(&tree), 5);
    assert!(widget.is_expanded(&origin));
    assert!(widget.is_expanded(&js));

    // A page loads another script under the same origin — expansion must hold.
    tree = tree_with_extra_sibling(tree);
    assert_eq!(widget.visible_row_count(&tree), 6);
    assert!(
        widget.is_expanded(&origin),
        "origin must stay open after inventory grows"
    );
    assert!(
        widget.is_expanded(&js),
        "js folder must stay open after inventory grows"
    );
}

#[test]
fn selecting_a_leaf_emits_open_source() {
    let mut widget = SourceTree::new();
    widget.set_expanded(group_key(&["https://example.com"]), true);
    widget.set_expanded(group_key(&["https://example.com", "js"]), true);

    let state = TreeUiState {
        widget,
        theme: test_theme(),
        support: AlwaysSupported,
        tree: sample_tree(),
        selected: None,
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 480.0))
        .build_ui_state(
            |ui, state| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: &state.support,
                };
                // Accumulate: `Harness::run` may paint idle frames after the
                // click frame, and those must not wipe the action we care about.
                let frame = state.widget.ui(ui, &ctx, &state.tree, state.selected);
                state.actions.extend(frame);
            },
            state,
        );

    harness.run();
    harness.state_mut().actions.clear();
    harness.get_by_label("app.js").click();
    harness.step();

    assert_eq!(
        harness.state().actions,
        vec![Action::OpenSource(SourceId(1), None)],
        "clicking a leaf must emit OpenSource"
    );
}

#[test]
fn ten_thousand_sources_virtualise() {
    const N: usize = 10_000;
    let children: Vec<_> = (0..N)
        .map(|i| SourceTreeNode::Leaf {
            id: SourceId(i as u32),
            label: format!("file-{i}.js"),
        })
        .collect();
    let tree = SourceTreeNode::Group {
        label: "https://big.example".into(),
        children,
    };

    let mut widget = SourceTree::new();
    widget.set_expanded(group_key(&["https://big.example"]), true);
    assert_eq!(widget.visible_row_count(&tree), N + 1);

    let viewport_h = 360.0_f32;
    let state = TreeUiState {
        widget,
        theme: test_theme(),
        support: AlwaysSupported,
        tree,
        selected: Some(SourceId(0)),
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, viewport_h))
        .build_ui_state(
            |ui, state| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: &state.support,
                };
                state.actions = state.widget.ui(ui, &ctx, &state.tree, state.selected);
            },
            state,
        );
    harness.run();

    let painted = harness.query_all_by_label_contains("file-").count();
    assert!(
        painted < 500,
        "expected virtualised paint, got {painted} file labels"
    );
    assert!(
        painted > 0,
        "at least the first window of files must be painted"
    );

    assert!(harness.query_by_label("file-0.js").is_some());
    assert!(
        harness
            .query_by_label(&format!("file-{}.js", N - 1))
            .is_none(),
        "last file must not be laid out while scrolled to the top"
    );
}

#[test]
fn clicking_group_toggles_expansion() {
    let state = TreeUiState {
        widget: SourceTree::new(),
        theme: test_theme(),
        support: AlwaysSupported,
        tree: sample_tree(),
        selected: None,
        actions: Vec::new(),
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 480.0))
        .build_ui_state(
            |ui, state| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: &state.support,
                };
                state.actions = state.widget.ui(ui, &ctx, &state.tree, state.selected);
            },
            state,
        );

    harness.run();
    // Collapsed: only the origin row is visible.
    assert!(harness.query_by_label("app.js").is_none());
    harness
        .get_by_label("▸ https://example.com")
        .click();
    harness.run();
    assert!(harness.state().widget.is_expanded(&group_key(&["https://example.com"])));
    assert!(harness.query_by_label("index.html").is_some());
}
