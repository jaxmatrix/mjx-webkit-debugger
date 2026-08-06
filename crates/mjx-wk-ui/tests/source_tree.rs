//! Source tree widget tests — `docs/tasks/T-009-source-tree.md`.
//!
//! Inventory (T-004) may still be a stub, so trees are hand-built.

use mjx_wk_source::{SourceId, SourceTreeNode};
use mjx_wk_ui::source_tree::{SourceTree, group_key};

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
