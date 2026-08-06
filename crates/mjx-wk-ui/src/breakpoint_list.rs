//! The breakpoint list and condition / action editors.
//!
//! **Owned by `docs/tasks/T-207-breakpoint-ui.md`.**
//!
//! Groups every line breakpoint by source, links back to the line, and lets the
//! user edit conditions, convert to logpoint / probe, and disable without
//! removing. Probe is offered only on WebKit (see [`probe_supported`]).

use egui::{RichText, ScrollArea, TextEdit, Vec2};

use mjx_wk_debug::{Breakpoint, BreakpointState, DEBUG_PANEL_REQUIRES};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::{SourceId, SourceLocation};

use crate::{Action, PanelCtx};

/// Protocol members this panel needs.
///
/// When any is [`Support::Unsupported`], the panel renders disabled with a
/// reason — never hidden, never silently broken.
pub const BREAKPOINT_LIST_REQUIRES: &[(Domain, &str)] = DEBUG_PANEL_REQUIRES;

/// Prefix on [`Action::SetBreakpointAction`] data that marks a Probe (WebKit).
/// Bare strings are logpoints. The host strips this before talking to the wire.
pub const PROBE_ACTION_PREFIX: &str = "probe:";

/// What the list needs for one frame. Borrowed, never owned.
#[derive(Debug, Clone, Copy)]
pub struct BreakpointListModel<'a> {
    /// Every breakpoint, of every source.
    pub breakpoints: &'a [Breakpoint],
    /// Display labels for sources, looked up by id. Missing ids fall back to
    /// [`SourceId`]'s `Display`.
    pub source_names: &'a [(SourceId, &'a str)],
}

/// Inline editor opened from a row or from the gutter context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakpointEdit {
    Condition {
        location: SourceLocation,
        draft: String,
    },
    Logpoint {
        location: SourceLocation,
        draft: String,
    },
    Probe {
        location: SourceLocation,
        draft: String,
    },
}

/// The breakpoint list panel widget.
#[derive(Debug, Default)]
pub struct BreakpointList {
    /// Open inline editor, if any.
    edit: Option<BreakpointEdit>,
    /// Request focus on the editor TextEdit once after it opens.
    focus_edit: bool,
    /// Flat rows rebuilt each frame (capacity retained).
    rows: Vec<ListRow>,
}

#[derive(Debug, Clone)]
enum ListRow {
    Group {
        label: String,
    },
    Breakpoint {
        location: SourceLocation,
        label: String,
        disabled: bool,
        detail: String,
    },
}

impl BreakpointList {
    pub fn new() -> Self {
        Self::default()
    }

    /// The open editor, if any. Useful from tests.
    pub fn edit(&self) -> Option<&BreakpointEdit> {
        self.edit.as_ref()
    }

    /// Open an editor programmatically (gutter menu / tests).
    pub fn begin_edit(&mut self, edit: BreakpointEdit) {
        self.edit = Some(edit);
        self.focus_edit = true;
    }

    /// How many virtualised rows are visible for the current model.
    pub fn visible_row_count(&mut self, model: &BreakpointListModel<'_>) -> usize {
        self.rebuild_rows(model);
        self.rows.len()
    }

    /// Draw, and report what the user did.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        model: &BreakpointListModel<'_>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let theme = ctx.theme;

        if let Some(reason) = unavailable_reason(ctx) {
            ui.add_enabled_ui(false, |ui| {
                ui.colored_label(theme.text_dim, reason);
            });
            return actions;
        }

        self.rebuild_rows(model);

        if let Some(edit_actions) = self.paint_editor(ui, ctx) {
            actions.extend(edit_actions);
        }

        let row_height = theme.row_height;
        ui.spacing_mut().item_spacing.y = 0.0;

        let mut open_source: Option<(SourceId, u32)> = None;
        let mut toggle: Option<SourceLocation> = None;
        let mut remove: Option<SourceLocation> = None;
        let mut begin_edit: Option<BreakpointEdit> = None;

        let total = self.rows.len();
        ScrollArea::vertical()
            .id_salt("mjx_breakpoint_list")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, total, |ui, row_range| {
                for row_idx in row_range {
                    let Some(row) = self.rows.get(row_idx).cloned() else {
                        break;
                    };
                    match row {
                        ListRow::Group { label } => {
                            ui.label(RichText::new(label).color(theme.text_dim).strong());
                        }
                        ListRow::Breakpoint {
                            location,
                            label,
                            disabled,
                            detail,
                            ..
                        } => {
                            ui.horizontal(|ui| {
                                let enabled = !disabled;
                                let mut checked = enabled;
                                let resp = ui.checkbox(&mut checked, "");
                                resp.clone().on_hover_text(if enabled {
                                    "Disable breakpoint"
                                } else {
                                    "Enable breakpoint"
                                });
                                if resp.changed() {
                                    toggle = Some(location);
                                }

                                let color = if disabled { theme.text_dim } else { theme.text };
                                let text = if detail.is_empty() {
                                    label.clone()
                                } else {
                                    format!("{label}  {detail}")
                                };
                                let access = if disabled {
                                    format!("breakpoint disabled: {label}")
                                } else {
                                    format!("breakpoint: {label}")
                                };
                                let btn = ui.add(
                                    egui::Button::new(RichText::new(text).color(color))
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE)
                                        .min_size(Vec2::new(0.0, row_height)),
                                );
                                btn.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        access.clone(),
                                    )
                                });
                                if btn.clicked() {
                                    open_source = Some((location.source, location.line));
                                }
                                btn.context_menu(|ui| {
                                    if ui.button("Edit condition…").clicked() {
                                        begin_edit = Some(BreakpointEdit::Condition {
                                            location,
                                            draft: String::new(),
                                        });
                                        ui.close();
                                    }
                                    if ui.button("Add logpoint…").clicked() {
                                        begin_edit = Some(BreakpointEdit::Logpoint {
                                            location,
                                            draft: String::new(),
                                        });
                                        ui.close();
                                    }
                                    if probe_supported(ctx) && ui.button("Add probe…").clicked() {
                                        begin_edit = Some(BreakpointEdit::Probe {
                                            location,
                                            draft: String::new(),
                                        });
                                        ui.close();
                                    }
                                    if enabled && ui.button("Disable").clicked() {
                                        toggle = Some(location);
                                        ui.close();
                                    }
                                    if !enabled && ui.button("Enable").clicked() {
                                        toggle = Some(location);
                                        ui.close();
                                    }
                                    if ui.button("Remove").clicked() {
                                        remove = Some(location);
                                        ui.close();
                                    }
                                });
                            });
                        }
                    }
                }
            });

        if let Some(edit) = begin_edit {
            self.edit = Some(edit);
            self.focus_edit = true;
        }
        if let Some((source, line)) = open_source {
            actions.push(Action::OpenSource(source, Some(line)));
        }
        if let Some(loc) = toggle {
            actions.push(Action::ToggleBreakpoint(loc));
        }
        if let Some(loc) = remove {
            actions.push(Action::RemoveBreakpoint(loc));
        }

        actions
    }

    fn paint_editor(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>) -> Option<Vec<Action>> {
        let theme = ctx.theme;
        let Some(edit) = self.edit.as_mut() else {
            return None;
        };

        let (title, hint) = match edit {
            BreakpointEdit::Condition { .. } => ("Condition", "Break when true"),
            BreakpointEdit::Logpoint { .. } => ("Logpoint", "Message to log"),
            BreakpointEdit::Probe { .. } => ("Probe", "Expression to sample"),
        };

        let mut submit = false;
        let mut cancel = false;
        let mut actions = Vec::new();
        let want_focus = self.focus_edit;

        ui.horizontal(|ui| {
            ui.label(RichText::new(title).color(theme.accent));
            let draft = match edit {
                BreakpointEdit::Condition { draft, .. }
                | BreakpointEdit::Logpoint { draft, .. }
                | BreakpointEdit::Probe { draft, .. } => draft,
            };
            let response = ui.add(
                TextEdit::singleline(draft)
                    .hint_text(hint)
                    .desired_width(ui.available_width().max(120.0))
                    .font(egui::TextStyle::Monospace)
                    .id_salt("mjx_bp_edit"),
            );
            if want_focus {
                response.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && (response.has_focus() || response.lost_focus()) {
                submit = true;
            }
            if ui.button("Set").clicked() {
                submit = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
        self.focus_edit = false;
        ui.separator();

        if cancel {
            self.edit = None;
            return Some(actions);
        }
        if submit {
            let finished = self.edit.take();
            if let Some(edit) = finished {
                match edit {
                    BreakpointEdit::Condition { location, draft } => {
                        let value = trim_opt(draft);
                        actions.push(Action::SetBreakpointCondition(location, value));
                    }
                    BreakpointEdit::Logpoint { location, draft } => {
                        let value = trim_opt(draft);
                        actions.push(Action::SetBreakpointAction(location, value));
                    }
                    BreakpointEdit::Probe { location, draft } => {
                        let value = trim_opt(draft).map(|s| format!("{PROBE_ACTION_PREFIX}{s}"));
                        actions.push(Action::SetBreakpointAction(location, value));
                    }
                }
            }
            return Some(actions);
        }
        Some(actions)
    }

    fn rebuild_rows(&mut self, model: &BreakpointListModel<'_>) {
        self.rows.clear();
        if model.breakpoints.is_empty() {
            return;
        }

        // Group by display source (resolved location when bound).
        let mut order: Vec<(SourceId, Vec<usize>)> = Vec::new();
        for (index, bp) in model.breakpoints.iter().enumerate() {
            let source = display_source(bp);
            if let Some((_, idxs)) = order.iter_mut().find(|(s, _)| *s == source) {
                idxs.push(index);
            } else {
                order.push((source, vec![index]));
            }
        }

        for (source, idxs) in order {
            let label = source_label(model, source);
            self.rows.push(ListRow::Group { label });
            for index in idxs {
                let bp = &model.breakpoints[index];
                let location = display_location(bp);
                let disabled = matches!(bp.state, BreakpointState::Disabled) || !bp.spec.enabled;
                let kind = row_kind_label(bp);
                let label = format!("{}:{}{kind}", location.line + 1, location.column,);
                let detail = row_detail(bp);
                self.rows.push(ListRow::Breakpoint {
                    location,
                    label,
                    disabled,
                    detail,
                });
            }
        }
    }
}

fn display_source(bp: &Breakpoint) -> SourceId {
    match &bp.state {
        BreakpointState::Resolved { actual } => actual.source,
        _ => bp.spec.location.source,
    }
}

fn display_location(bp: &Breakpoint) -> SourceLocation {
    match &bp.state {
        BreakpointState::Resolved { actual } => *actual,
        _ => bp.spec.location,
    }
}

fn source_label(model: &BreakpointListModel<'_>, source: SourceId) -> String {
    model
        .source_names
        .iter()
        .find(|(id, _)| *id == source)
        .map(|(_, name)| (*name).to_owned())
        .unwrap_or_else(|| source.to_string())
}

fn row_kind_label(bp: &Breakpoint) -> String {
    if bp.spec.is_logpoint() {
        if bp
            .spec
            .actions
            .iter()
            .any(|a| matches!(a.kind, mjx_wk_debug::BreakpointActionKind::Probe))
        {
            return "  [probe]".into();
        }
        return "  [logpoint]".into();
    }
    if bp.spec.condition.is_some() {
        return "  [conditional]".into();
    }
    match &bp.state {
        BreakpointState::Pending => "  [pending]".into(),
        BreakpointState::Failed { .. } => "  [failed]".into(),
        BreakpointState::Disabled => String::new(),
        BreakpointState::Resolved { .. } => String::new(),
    }
}

fn row_detail(bp: &Breakpoint) -> String {
    if let Some(cond) = &bp.spec.condition {
        return format!("if {cond}");
    }
    if let Some(action) = bp.spec.actions.first() {
        if let Some(data) = &action.data {
            return data.clone();
        }
    }
    String::new()
}

fn trim_opt(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_owned())
    }
}

fn unavailable_reason(ctx: &PanelCtx<'_>) -> Option<String> {
    for &(domain, member) in BREAKPOINT_LIST_REQUIRES {
        match ctx.support.supports(domain, member) {
            Support::Unsupported => {
                return Some(format!(
                    "Breakpoints unavailable: {domain}.{member} is not supported on this target"
                ));
            }
            Support::Native | Support::Emulated => {}
        }
    }
    None
}

/// Whether a Probe action may be offered.
///
/// Requires [`Support::Native`] for `Debugger.setBreakpointByUrl` (task gate)
/// and a WebKit-only Debugger member so CDP — where line breakpoints exist but
/// Probe options do not — never offers it.
pub fn probe_supported(ctx: &PanelCtx<'_>) -> bool {
    matches!(
        ctx.support.supports(Domain::Debugger, "setBreakpointByUrl"),
        Support::Native
    ) && ctx
        .support
        .supports(Domain::Debugger, "setPauseOnMicrotasks")
        .is_available()
}

/// Encode a probe expression for [`Action::SetBreakpointAction`].
pub fn encode_probe_action(expression: &str) -> String {
    format!("{PROBE_ACTION_PREFIX}{expression}")
}

/// Decode a [`Action::SetBreakpointAction`] payload into (is_probe, data).
pub fn decode_breakpoint_action(data: &str) -> (bool, &str) {
    if let Some(rest) = data.strip_prefix(PROBE_ACTION_PREFIX) {
        (true, rest)
    } else {
        (false, data)
    }
}
