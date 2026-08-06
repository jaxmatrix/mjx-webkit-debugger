//! The file tree.
//!
//! **Owned by `docs/tasks/T-009-source-tree.md`.**
//!
//! Grouped by origin, then by path. Must stay usable with ten thousand
//! sources — a large site has that many — so rows are virtualised and the tree
//! is built when the inventory changes, not per frame.

use std::collections::HashSet;

use crate::{Action, PanelCtx};
use mjx_wk_source::{SourceId, SourceTreeNode};

/// Separator used inside [`GroupKey`] so labels that contain `/` stay unambiguous.
const KEY_SEP: char = '\0';

/// Stable identity for a group across inventory rebuilds: the path of flattened
/// group labels from the root down to this node.
type GroupKey = String;

/// A grouped, virtualised list of every known source.
///
/// Expansion state lives here, not in the model, so a page that loads a new
/// script does not collapse the folder the user just opened.
#[derive(Debug, Default)]
pub struct SourceTree {
    expanded: HashSet<GroupKey>,
    /// Flat visible rows for the current frame. Capacity is retained across
    /// frames so a large tree does not allocate on the steady path.
    rows: Vec<FlatRow>,
    /// Group keys present in the model this frame (expanded or not). Used to
    /// drop stale expansion entries without walking the widget again.
    live_keys: HashSet<GroupKey>,
}

/// One row in the virtualised list.
#[allow(dead_code)] // fields are painted by the virtualised ui commit
#[derive(Debug, Clone)]
enum FlatRow {
    Group {
        key: GroupKey,
        label: String,
        depth: u16,
        is_expanded: bool,
    },
    Leaf {
        id: SourceId,
        label: String,
        depth: u16,
    },
}

impl SourceTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the group at `key` is expanded.
    ///
    /// `key` is the same path string the widget uses internally: flattened
    /// labels joined by a NUL byte. Tests build keys with [`group_key`].
    pub fn is_expanded(&self, key: &str) -> bool {
        self.expanded.contains(key)
    }

    /// Force a group's expansion state. Useful from tests and from app code
    /// that wants to reveal a path without simulating clicks.
    pub fn set_expanded(&mut self, key: impl Into<String>, expanded: bool) {
        let key = key.into();
        if expanded {
            self.expanded.insert(key);
        } else {
            self.expanded.remove(&key);
        }
    }

    /// How many rows are currently visible given the expansion state.
    ///
    /// Does not allocate beyond the reused buffer. Intended for tests that
    /// assert virtualisation bounds without painting.
    pub fn visible_row_count(&mut self, tree: &SourceTreeNode) -> usize {
        self.rebuild_rows(tree);
        self.rows.len()
    }

    /// Draw the tree.
    ///
    /// Expansion state lives here, not in the model, so it survives the
    /// inventory changing underneath — a page that loads a script must not
    /// collapse the folder the user just opened.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        tree: &SourceTreeNode,
        selected: Option<SourceId>,
    ) -> Vec<Action> {
        // Row model first; painting lands in the next commit.
        let _ = (ui, ctx, selected);
        self.rebuild_rows(tree);
        Vec::new()
    }

    fn rebuild_rows(&mut self, tree: &SourceTreeNode) {
        self.rows.clear();
        self.live_keys.clear();
        collect_rows(
            tree,
            "",
            0,
            &self.expanded,
            &mut self.rows,
            &mut self.live_keys,
        );
        self.expanded.retain(|k| self.live_keys.contains(k));
    }
}

/// Build the group key for a path of flattened labels.
///
/// Public so tests can assert expansion survival without depending on the
/// private separator.
pub fn group_key(labels: &[&str]) -> String {
    let mut out = String::new();
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            out.push(KEY_SEP);
        }
        out.push_str(label);
    }
    out
}

/// Collapse a chain of single-child groups into one display label.
///
/// `Group{a, [Group{b, [Group{c, kids}]}]}` becomes label `"a/b/c"` over `kids`.
/// A group whose sole child is a leaf is left alone — the folder is still a
/// meaningful nesting level once siblings appear, and collapsing it would hide
/// the path segment from the tree.
pub fn collapse_single_child_groups<'a>(
    label: &str,
    children: &'a [SourceTreeNode],
) -> (String, &'a [SourceTreeNode]) {
    let mut display = label.to_owned();
    let mut rest = children;
    while let [SourceTreeNode::Group {
        label: inner,
        children: next,
    }] = rest
    {
        display.push('/');
        display.push_str(inner);
        rest = next.as_slice();
    }
    (display, rest)
}

fn extend_key(prefix: &str, label: &str) -> GroupKey {
    if prefix.is_empty() {
        label.to_owned()
    } else {
        let mut key = String::with_capacity(prefix.len() + 1 + label.len());
        key.push_str(prefix);
        key.push(KEY_SEP);
        key.push_str(label);
        key
    }
}

fn collect_rows(
    node: &SourceTreeNode,
    parent_key: &str,
    depth: u16,
    expanded: &HashSet<GroupKey>,
    rows: &mut Vec<FlatRow>,
    live_keys: &mut HashSet<GroupKey>,
) {
    match node {
        SourceTreeNode::Group { label, children } => {
            let (display, children) = collapse_single_child_groups(label, children);
            let key = extend_key(parent_key, &display);
            let is_expanded = expanded.contains(&key);
            live_keys.insert(key.clone());
            rows.push(FlatRow::Group {
                key: key.clone(),
                label: display,
                depth,
                is_expanded,
            });
            if is_expanded {
                for child in children {
                    collect_rows(child, &key, depth + 1, expanded, rows, live_keys);
                }
            } else {
                // Still register descendant keys so collapsing a parent does
                // not forget which nested folders were open.
                register_group_keys(children, &key, live_keys);
            }
        }
        SourceTreeNode::Leaf { id, label } => {
            rows.push(FlatRow::Leaf {
                id: *id,
                label: label.clone(),
                depth,
            });
        }
    }
}

fn register_group_keys(nodes: &[SourceTreeNode], parent_key: &str, live_keys: &mut HashSet<GroupKey>) {
    for node in nodes {
        if let SourceTreeNode::Group { label, children } = node {
            let (display, children) = collapse_single_child_groups(label, children);
            let key = extend_key(parent_key, &display);
            live_keys.insert(key.clone());
            register_group_keys(children, &key, live_keys);
        }
    }
}

#[cfg(test)]
mod collapse_tests {
    use super::*;
    use mjx_wk_source::SourceId;

    #[test]
    fn single_child_group_chain_collapses() {
        let tree = SourceTreeNode::Group {
            label: "a".into(),
            children: vec![SourceTreeNode::Group {
                label: "b".into(),
                children: vec![SourceTreeNode::Group {
                    label: "c".into(),
                    children: vec![SourceTreeNode::Leaf {
                        id: SourceId(1),
                        label: "f.js".into(),
                    }],
                }],
            }],
        };
        let SourceTreeNode::Group { label, children } = &tree else {
            panic!("expected group");
        };
        let (display, rest) = collapse_single_child_groups(label, children);
        assert_eq!(display, "a/b/c");
        assert_eq!(
            rest,
            [SourceTreeNode::Leaf {
                id: SourceId(1),
                label: "f.js".into(),
            }]
        );
    }

    #[test]
    fn group_with_multiple_children_is_not_collapsed() {
        let children = vec![
            SourceTreeNode::Leaf {
                id: SourceId(1),
                label: "a.js".into(),
            },
            SourceTreeNode::Leaf {
                id: SourceId(2),
                label: "b.js".into(),
            },
        ];
        let (display, rest) = collapse_single_child_groups("origin", &children);
        assert_eq!(display, "origin");
        assert_eq!(rest.len(), 2);
    }
}
