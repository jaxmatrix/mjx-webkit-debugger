//! The scope and variable tree.
//!
//! **Phase 2 — owned by `docs/tasks/T-203-variable-tree.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.
//!
//! Expansion state lives here, not in the model, so a fresh `getProperties`
//! page does not collapse the row the user just opened. Fetching is never done
//! on this thread — expanding emits [`Action::ExpandValue`] for the session.

use std::collections::HashSet;

use egui::{ScrollArea, TextEdit, Vec2};

use mjx_wk_debug::{ValueNodeId, ValueTree, values::PAGE_SIZE};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;

use crate::{Action, PanelCtx};

/// Protocol members this panel needs.
pub const VARIABLES_REQUIRES: &[(Domain, &str)] = &[(Domain::Runtime, "getProperties")];

/// Inputs for one frame of the variables panel.
#[derive(Debug, Clone, Copy)]
pub struct VariablesModel<'a> {
    /// Lazily expanded scopes / properties for the selected frame.
    pub values: Option<&'a ValueTree>,
    /// Watch expression source text (owned by `DebugModel`).
    pub watches: &'a [String],
}

/// The scope and variable tree.
#[derive(Debug, Default)]
pub struct VariablesTree {
    expanded: HashSet<ValueNodeId>,
    /// Flat visible rows for the current frame. Capacity is retained.
    rows: Vec<FlatRow>,
    watch_draft: String,
}

/// One virtualised row.
#[derive(Debug, Clone)]
enum FlatRow {
    Value {
        id: ValueNodeId,
        depth: u16,
        label: String,
        disclose: Disclose,
        is_accessor: bool,
    },
    ShowMore {
        parent: ValueNodeId,
        start: u32,
        remaining: u32,
        depth: u16,
    },
    WatchEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disclose {
    None,
    Collapsed,
    Expanded,
}

impl VariablesTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `id` is expanded in the widget.
    pub fn is_expanded(&self, id: ValueNodeId) -> bool {
        self.expanded.contains(&id)
    }

    /// Force expansion state — useful from tests.
    pub fn set_expanded(&mut self, id: ValueNodeId, expanded: bool) {
        if expanded {
            self.expanded.insert(id);
        } else {
            self.expanded.remove(&id);
        }
    }

    /// How many rows are visible given the current expansion state.
    pub fn visible_row_count(&mut self, model: &VariablesModel<'_>) -> usize {
        self.rebuild_rows(model);
        self.rows.len()
    }

    /// Draw, and report what the user did.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        model: &VariablesModel<'_>,
    ) -> Vec<Action> {
        if let Some(reason) = unsupported_reason(ctx) {
            ui.colored_label(ctx.theme.text_dim, reason);
            return Vec::new();
        }

        self.rebuild_rows(model);

        let row_height = ctx.theme.row_height;
        let indent_width = ctx.theme.indent_width;
        let text_color = ctx.theme.text;
        let text_dim = ctx.theme.text_dim;
        let accent = ctx.theme.accent;

        ui.spacing_mut().item_spacing.y = 0.0;

        let mut actions = Vec::new();
        let mut toggled: Vec<ValueNodeId> = Vec::new();
        let mut expand_requests: Vec<(ValueNodeId, u32, u32)> = Vec::new();
        let mut remove_watch: Option<usize> = None;
        let mut add_watch: Option<String> = None;

        let total_rows = self.rows.len();
        ScrollArea::vertical()
            .id_salt("mjx_variables_tree")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                for row_idx in row_range {
                    let Some(row) = self.rows.get(row_idx) else {
                        break;
                    };
                    match row {
                        FlatRow::Value {
                            id,
                            depth,
                            label,
                            disclose,
                            is_accessor,
                        } => {
                            let indent = indent_width * f32::from(*depth);
                            ui.horizontal(|ui| {
                                ui.add_space(indent);
                                match disclose {
                                    Disclose::None => {
                                        ui.add_space(indent_width);
                                    }
                                    Disclose::Collapsed | Disclose::Expanded => {
                                        let marker = if *disclose == Disclose::Expanded {
                                            '▾'
                                        } else {
                                            '▸'
                                        };
                                        let response = ui.add(
                                            egui::Button::new(
                                                egui::RichText::new(marker.to_string())
                                                    .color(text_dim),
                                            )
                                            .fill(egui::Color32::TRANSPARENT)
                                            .stroke(egui::Stroke::NONE)
                                            .min_size(Vec2::new(indent_width, row_height)),
                                        );
                                        if response.clicked() {
                                            toggled.push(*id);
                                        }
                                    }
                                }

                                let color = if *is_accessor { text_dim } else { text_color };
                                let response = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(label.as_str()).color(color),
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(Vec2::new(0.0, row_height)),
                                );
                                if response.clicked() && *is_accessor {
                                    // Opt-in getter invoke — same Action channel as expand.
                                    expand_requests.push((*id, 0, 0));
                                }
                                if response.secondary_clicked()
                                    && let Some(values) = model.values
                                    && let Some(pos) =
                                        values.watch_roots().iter().position(|&w| w == *id)
                                {
                                    remove_watch = Some(pos);
                                }
                            });
                        }
                        FlatRow::ShowMore {
                            parent,
                            start,
                            remaining,
                            depth,
                        } => {
                            let indent = indent_width * f32::from(*depth);
                            ui.horizontal(|ui| {
                                ui.add_space(indent + indent_width);
                                let text = format!("Show more ({remaining})…");
                                let response = ui.add(
                                    egui::Button::new(egui::RichText::new(text).color(accent))
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE)
                                        .min_size(Vec2::new(0.0, row_height)),
                                );
                                if response.clicked() {
                                    expand_requests.push((*parent, *start, PAGE_SIZE));
                                }
                            });
                        }
                        FlatRow::WatchEditor => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("＋").color(text_dim));
                                let edit = TextEdit::singleline(&mut self.watch_draft)
                                    .hint_text("Watch expression")
                                    .desired_width(f32::INFINITY);
                                let response = ui.add(edit);
                                if response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    let expr = self.watch_draft.trim().to_owned();
                                    if !expr.is_empty() {
                                        add_watch = Some(expr);
                                        self.watch_draft.clear();
                                    }
                                }
                            });
                        }
                    }
                }
            });

        for id in toggled {
            let opening = !self.expanded.contains(&id);
            if opening {
                self.expanded.insert(id);
                if let Some(values) = model.values
                    && values.needs_fetch(id)
                {
                    let start = values.get(id).map(|n| n.fetched.end).unwrap_or(0);
                    // Accessors use count=0 as the opt-in invoke signal.
                    let count = if values.get(id).is_some_and(|n| n.is_accessor) {
                        0
                    } else {
                        PAGE_SIZE
                    };
                    expand_requests.push((id, start, count));
                }
            } else {
                self.expanded.remove(&id);
            }
        }

        for (node, start, count) in expand_requests {
            actions.push(Action::ExpandValue {
                node: node.0,
                start,
                count,
            });
        }
        if let Some(expr) = add_watch {
            actions.push(Action::AddWatch(expr));
        }
        if let Some(idx) = remove_watch {
            actions.push(Action::RemoveWatch(idx));
        }

        // Drop expansion entries whose nodes vanished after a resume/clear.
        if let Some(values) = model.values {
            self.expanded.retain(|id| values.get(*id).is_some());
        } else {
            self.expanded.clear();
        }

        actions
    }

    fn rebuild_rows(&mut self, model: &VariablesModel<'_>) {
        self.rows.clear();
        // Always offer the watch editor so the panel is useful while running.
        for (i, expr) in model.watches.iter().enumerate() {
            // Prefer the evaluated watch root when the tree has one.
            if let Some(values) = model.values
                && let Some(&id) = values.watch_roots().get(i)
            {
                collect_value_rows(values, id, 0, &self.expanded, &mut self.rows);
            } else {
                self.rows.push(FlatRow::Value {
                    id: ValueNodeId(u32::MAX - i as u32),
                    depth: 0,
                    label: format!("{expr}: …"),
                    disclose: Disclose::None,
                    is_accessor: false,
                });
            }
        }
        self.rows.push(FlatRow::WatchEditor);

        if let Some(values) = model.values {
            for &id in values.scope_roots() {
                collect_value_rows(values, id, 0, &self.expanded, &mut self.rows);
            }
        }
    }
}

fn collect_value_rows(
    tree: &ValueTree,
    id: ValueNodeId,
    depth: u16,
    expanded: &HashSet<ValueNodeId>,
    rows: &mut Vec<FlatRow>,
) {
    let Some(node) = tree.get(id) else {
        return;
    };
    let can_disclose = node.preview.has_children
        || node.is_accessor
        || node.children.as_ref().is_some_and(|c| !c.is_empty())
        || tree.remaining(id).is_some();
    let is_open = expanded.contains(&id);
    let disclose = if !can_disclose {
        Disclose::None
    } else if is_open {
        Disclose::Expanded
    } else {
        Disclose::Collapsed
    };

    let label = format!("{}: {}", node.name, node.preview.description);
    rows.push(FlatRow::Value {
        id,
        depth,
        label,
        disclose,
        is_accessor: node.is_accessor,
    });

    if !is_open {
        return;
    }

    if let Some(children) = &node.children {
        for &child in children {
            collect_value_rows(tree, child, depth + 1, expanded, rows);
        }
    }
    if let Some(remaining) = tree.remaining(id) {
        rows.push(FlatRow::ShowMore {
            parent: id,
            start: node.fetched.end,
            remaining,
            depth: depth + 1,
        });
    }
}

fn unsupported_reason(ctx: &PanelCtx<'_>) -> Option<String> {
    for &(domain, member) in VARIABLES_REQUIRES {
        match ctx.support.supports(domain, member) {
            Support::Unsupported => {
                return Some(format!(
                    "Variables unavailable: {domain}.{member} is not supported on this target"
                ));
            }
            Support::Native | Support::Emulated => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mjx_wk_debug::{ValuePreview, values::WatchResult, values::WatchValue};
    use mjx_wk_protocol::generated::runtime::{RemoteObject, RemoteObjectType};

    fn remote(desc: &str) -> RemoteObject {
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

    #[test]
    fn show_more_row_appears_when_remaining() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "arr",
            Some("a".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: Some("array".into()),
                description: "Array(200)".into(),
                has_children: true,
            },
        );
        tree.set_known_total(root, 200);
        let page: Vec<_> = (0..PAGE_SIZE)
            .map(|i| {
                use mjx_wk_protocol::generated::runtime::PropertyDescriptor;
                PropertyDescriptor {
                    name: i.to_string(),
                    value: Some(remote(&i.to_string())),
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
            })
            .collect();
        tree.apply_properties(root, 0, PAGE_SIZE, &page, &[], Some("a"));

        let mut widget = VariablesTree::new();
        widget.set_expanded(root, true);
        let model = VariablesModel {
            values: Some(&tree),
            watches: &[],
        };
        let count = widget.visible_row_count(&model);
        // watch editor + root + PAGE_SIZE children + show more
        assert_eq!(count, 1 + 1 + PAGE_SIZE as usize + 1);
        assert!(
            widget
                .rows
                .iter()
                .any(|r| matches!(r, FlatRow::ShowMore { .. }))
        );
    }

    #[test]
    fn watch_reeval_updates_visible_label() {
        let mut tree = ValueTree::new();
        tree.set_watch_roots([WatchResult {
            expression: "x".into(),
            value: WatchValue::Ready(Box::new(remote("1"))),
        }]);
        let mut widget = VariablesTree::new();
        let watches = ["x".to_owned()];
        {
            let model = VariablesModel {
                values: Some(&tree),
                watches: &watches,
            };
            widget.rebuild_rows(&model);
            let label = match &widget.rows[0] {
                FlatRow::Value { label, .. } => label.clone(),
                _ => panic!("expected watch row"),
            };
            assert!(label.contains("1"), "{label}");
        }

        tree.set_watch_roots([WatchResult {
            expression: "x".into(),
            value: WatchValue::Ready(Box::new(remote("99"))),
        }]);
        let model = VariablesModel {
            values: Some(&tree),
            watches: &watches,
        };
        widget.rebuild_rows(&model);
        let label = match &widget.rows[0] {
            FlatRow::Value { label, .. } => label.clone(),
            _ => panic!("expected watch row"),
        };
        assert!(label.contains("99"), "{label}");
    }
}
