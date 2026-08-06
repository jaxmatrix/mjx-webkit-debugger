//! The console log and its prompt.
//!
//! **Phase 2 — owned by `docs/tasks/T-204-console.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.
//!
//! Object arguments keep their remote ids so a later host can expand them with
//! T-203's value tree. This widget stubs expansion locally (show the id) and
//! does **not** import `mjx-wk-debug` — L4 crates stay peers.

use egui::{Color32, RichText, ScrollArea, TextEdit};

use mjx_wk_console::{ConsoleMessage, ConsoleModel, MessageLevel};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;

use crate::{Action, Panel, PanelCtx, PanelId};

/// Protocol members this panel needs.
pub const REQUIRES: &[(Domain, &str)] = &[(Domain::Console, "enable")];

/// The console log and its prompt.
#[derive(Debug, Default)]
pub struct ConsoleView {
    /// Expression being typed in the prompt.
    prompt: String,
    /// Object ids whose disclosure rows are open this frame.
    expanded: std::collections::HashSet<String>,
}

impl ConsoleView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Why the panel cannot run, if any required member is unavailable.
    pub fn unavailable_reason<'a>(
        &self,
        ctx: &PanelCtx<'a>,
    ) -> Option<(Domain, &'static str, Support)> {
        for &(domain, member) in REQUIRES {
            let support = ctx.support.supports(domain, member);
            if !support.is_available() {
                return Some((domain, member, support));
            }
        }
        None
    }

    /// Draw, and report what the user did.
    ///
    /// Pure function of `model`: expansion of object stubs is the only widget
    /// state, so a page that floods the log cannot collapse what the user just
    /// opened.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        model: &ConsoleModel,
    ) -> Vec<Action> {
        if let Some((domain, member, _)) = self.unavailable_reason(ctx) {
            ui.add_enabled_ui(false, |ui| {
                ui.heading("Console");
                ui.label(format!(
                    "Unavailable: `{domain}.{member}` is not supported on this target \
                     (not attached, wrong target kind, or dialect gap)."
                ));
                ui.separator();
                ui.label("The panel stays visible so its absence is never a mystery.");
            });
            return Vec::new();
        }

        let mut actions = Vec::new();
        let theme = ctx.theme;

        if model.dropped > 0 {
            ui.colored_label(
                theme.breakpoint_conditional,
                format!(
                    "{} older message{} dropped to stay within the log bound.",
                    model.dropped,
                    if model.dropped == 1 { "" } else { "s" }
                ),
            );
        }

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for message in &model.messages {
                    self.paint_message(ui, theme, message);
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("›").color(theme.accent).monospace());
            let response = ui.add(
                TextEdit::singleline(&mut self.prompt)
                    .hint_text("Expression")
                    .desired_width(ui.available_width())
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme.text),
            );
            // Enter while focused (or the instant focus is lost to Enter) submits.
            // Matching only `lost_focus` would miss the common "still focused" case.
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && (response.has_focus() || response.lost_focus()) {
                let expr = self.prompt.trim().to_owned();
                if !expr.is_empty() {
                    actions.push(Action::Evaluate(expr));
                    self.prompt.clear();
                    response.request_focus();
                }
            }
        });

        actions
    }

    fn paint_message(&mut self, ui: &mut egui::Ui, theme: &crate::Theme, message: &ConsoleMessage) {
        let color = level_color(theme, message.level);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(level_label(message.level))
                    .color(color)
                    .monospace()
                    .size(theme.monospace_size * 0.85),
            );
            ui.label(
                RichText::new(&message.text)
                    .color(color)
                    .monospace()
                    .size(theme.monospace_size),
            );
            if message.repeat_count > 1 {
                ui.label(
                    RichText::new(format!("×{}", message.repeat_count))
                        .color(theme.text_dim)
                        .monospace()
                        .size(theme.monospace_size * 0.85),
                );
            }
        });

        // Soft stub: keep object ids expandable without importing mjx-wk-debug.
        // A later host can replace this with T-203's VariablesTree.
        for object_id in &message.argument_object_ids {
            let open = self.expanded.contains(object_id);
            let label = if open {
                format!("▼ Object {object_id}")
            } else {
                format!("▶ Object {object_id}")
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(label).monospace().color(theme.text_dim))
                        .frame(false),
                )
                .clicked()
            {
                if open {
                    self.expanded.remove(object_id);
                } else {
                    self.expanded.insert(object_id.clone());
                }
            }
            if open {
                ui.indent(object_id.as_str(), |ui| {
                    ui.label(
                        RichText::new("(expand with variable tree — T-203)")
                            .italics()
                            .color(theme.text_dim)
                            .small(),
                    );
                });
            }
        }
    }
}

impl Panel for ConsoleView {
    fn id(&self) -> PanelId {
        PanelId("console")
    }

    fn title(&self) -> &str {
        "Console"
    }

    fn requires(&self) -> &[(Domain, &'static str)] {
        REQUIRES
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>) -> Vec<Action> {
        // Panel::ui has no model — paint an empty log. Callers that have a
        // snapshot should use [`ConsoleView::ui`] with the model instead.
        let empty = ConsoleModel::default();
        ConsoleView::ui(self, ui, ctx, &empty)
    }
}

fn level_label(level: MessageLevel) -> &'static str {
    match level {
        MessageLevel::Debug => "debug",
        MessageLevel::Log => "log",
        MessageLevel::Info => "info",
        MessageLevel::Warning => "warn",
        MessageLevel::Error => "error",
    }
}

fn level_color(theme: &crate::Theme, level: MessageLevel) -> Color32 {
    match level {
        MessageLevel::Debug => theme.text_dim,
        MessageLevel::Log | MessageLevel::Info => theme.text,
        MessageLevel::Warning => theme.breakpoint_conditional,
        MessageLevel::Error => theme.breakpoint_resolved,
    }
}
