//! The call stack, with async frames.
//!
//! **Phase 2 — owned by `docs/tasks/T-202-pause-and-stepping.md`.**
//!
//! Declared in Phase 1a so that the task adding it creates this file rather
//! than editing a shared module list. See `CONTRIBUTING.md`, *file ownership*.

use egui::{RichText, ScrollArea};

use mjx_wk_debug::{CallFrame, PauseReason, PauseState};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;

use crate::{Action, PanelCtx, StepKind};

/// Protocol members this widget needs to be interactive.
///
/// When any is [`Support::Unsupported`], the widget renders disabled with a
/// reason — never hidden, never silently broken.
pub const CALL_STACK_REQUIRES: &[(Domain, &str)] = &[
    (Domain::Debugger, "paused"),
    (Domain::Debugger, "resume"),
    (Domain::Debugger, "stepOver"),
];

/// The call stack, with async frames.
#[derive(Debug, Default)]
pub struct CallStackList {
    /// When false, blackboxed frames collapse behind a "show N more" row.
    show_blackboxed: bool,
}

impl CallStackList {
    pub fn new() -> Self {
        Self {
            show_blackboxed: false,
        }
    }

    /// Whether blackboxed frames are currently expanded.
    pub fn show_blackboxed(&self) -> bool {
        self.show_blackboxed
    }

    /// Force blackboxed-frame visibility. Useful from tests.
    pub fn set_show_blackboxed(&mut self, show: bool) {
        self.show_blackboxed = show;
    }

    /// Draw, and report what the user did.
    ///
    /// `paused` is `None` when execution is running — the widget still paints
    /// (step controls stay available for pause/resume), but the frame list
    /// explains that nothing is stopped.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        paused: Option<&PauseState>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let theme = ctx.theme;

        if let Some(reason) = unavailable_reason(ctx) {
            ui.add_enabled_ui(false, |ui| {
                ui.colored_label(theme.text_dim, reason);
            });
            return actions;
        }

        self.paint_step_bar(ui, ctx, &mut actions);

        ui.add_space(4.0);
        ui.separator();

        match paused {
            None => {
                ui.colored_label(theme.text_dim, "Not paused");
            }
            Some(state) => {
                self.paint_reason(ui, ctx, state);
                self.paint_frames(ui, ctx, state, &mut actions);
            }
        }

        actions
    }

    fn paint_step_bar(&self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>, actions: &mut Vec<Action>) {
        let theme = ctx.theme;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            if step_button(ui, ctx, "Pause", "pause") {
                actions.push(Action::Pause);
            }
            if step_button(ui, ctx, "Resume", "resume") {
                actions.push(Action::Resume);
            }
            if step_button(ui, ctx, "Step over", "stepOver") {
                actions.push(Action::Step(StepKind::Over));
            }
            if step_button(ui, ctx, "Step into", "stepInto") {
                actions.push(Action::Step(StepKind::Into));
            }
            if step_button(ui, ctx, "Step out", "stepOut") {
                actions.push(Action::Step(StepKind::Out));
            }

            // WebKit-only — offer when supported, hide when not. Never grey
            // them out next to Chrome's three; absence is the signal.
            if member_available(ctx, "stepNext") {
                let resp = ui.add(
                    egui::Button::new(RichText::new("Step next").color(theme.text))
                        .fill(theme.panel),
                );
                if resp
                    .on_hover_text("Finer than step-over (WebKit)")
                    .clicked()
                {
                    actions.push(Action::Step(StepKind::Next));
                }
            }
            if member_available(ctx, "continueUntilNextRunLoop") {
                let resp = ui.add(
                    egui::Button::new(RichText::new("Next run loop").color(theme.text))
                        .fill(theme.panel),
                );
                if resp
                    .on_hover_text("Continue until the next event-loop turn (WebKit)")
                    .clicked()
                {
                    actions.push(Action::Step(StepKind::UntilNextRunLoop));
                }
            }
        });
    }

    fn paint_reason(&self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>, state: &PauseState) {
        let label = match &state.reason {
            PauseReason::Breakpoint => "Paused on breakpoint".to_owned(),
            PauseReason::DebuggerStatement => "Paused on debugger statement".to_owned(),
            PauseReason::Exception { caught: true } => "Paused on caught exception".to_owned(),
            PauseReason::Exception { caught: false } => "Paused on uncaught exception".to_owned(),
            PauseReason::Step => "Paused after step".to_owned(),
            PauseReason::User => "Paused".to_owned(),
            PauseReason::Instrumentation { detail } => format!("Paused on {detail}"),
            PauseReason::Assertion => "Paused on assertion".to_owned(),
            PauseReason::Microtask => "Paused on microtask".to_owned(),
            PauseReason::Other(s) => format!("Paused ({s})"),
        };
        ui.label(RichText::new(label).color(ctx.theme.text_dim));
    }

    fn paint_frames(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        state: &PauseState,
        actions: &mut Vec<Action>,
    ) {
        let theme = ctx.theme;

        ui.spacing_mut().item_spacing.y = 0.0;

        ScrollArea::vertical()
            .id_salt("mjx_call_stack")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut hidden_run = 0usize;
                for (index, frame) in state.call_frames.iter().enumerate() {
                    if frame.is_blackboxed && !self.show_blackboxed {
                        hidden_run += 1;
                        continue;
                    }
                    if hidden_run > 0 {
                        self.paint_show_more(ui, ctx, hidden_run);
                        hidden_run = 0;
                    }
                    let selected = index == state.selected_frame;
                    if paint_frame_row(ui, ctx, frame, selected, FrameKind::Sync) {
                        actions.push(Action::SelectFrame(index));
                    }
                }
                if hidden_run > 0 {
                    self.paint_show_more(ui, ctx, hidden_run);
                }

                if !state.async_stack.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Async call stack")
                            .color(theme.text_dim)
                            .italics(),
                    );
                    for frame in &state.async_stack {
                        // Async frames are historical — selecting them does not
                        // retarget evaluateOnCallFrame.
                        let _ = paint_frame_row(ui, ctx, frame, false, FrameKind::Async);
                    }
                }
            });
    }

    fn paint_show_more(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>, n: usize) {
        let label = if n == 1 {
            "Show 1 more frame".to_owned()
        } else {
            format!("Show {n} more frames")
        };
        if ui
            .add(egui::Button::new(RichText::new(label).color(ctx.theme.accent)).frame(false))
            .clicked()
        {
            self.show_blackboxed = true;
        }
    }
}

#[derive(Clone, Copy)]
enum FrameKind {
    Sync,
    Async,
}

fn unavailable_reason(ctx: &PanelCtx<'_>) -> Option<String> {
    for &(domain, member) in CALL_STACK_REQUIRES {
        if matches!(ctx.support.supports(domain, member), Support::Unsupported) {
            return Some(format!(
                "Call stack unavailable: {}.{member} is not supported on this connection",
                domain.as_str()
            ));
        }
    }
    None
}

fn member_available(ctx: &PanelCtx<'_>, member: &str) -> bool {
    ctx.support
        .supports(Domain::Debugger, member)
        .is_available()
}

fn step_button(ui: &mut egui::Ui, ctx: &PanelCtx<'_>, label: &str, member: &str) -> bool {
    let available = member_available(ctx, member);
    let theme = ctx.theme;
    let resp = ui.add_enabled(
        available,
        egui::Button::new(RichText::new(label).color(theme.text)).fill(theme.panel),
    );
    available && resp.clicked()
}

fn frame_label(frame: &CallFrame, kind: FrameKind) -> String {
    let name = if frame.function_name.is_empty() {
        "(anonymous)"
    } else {
        frame.function_name.as_str()
    };
    // One-based line:column for humans.
    let loc = format!("{}:{}", frame.location.line + 1, frame.location.column + 1);
    match kind {
        FrameKind::Async => format!("{name}  {loc} (async)"),
        FrameKind::Sync if frame.is_blackboxed => format!("{name}  {loc} (blackboxed)"),
        FrameKind::Sync => format!("{name}  {loc}"),
    }
}

fn paint_frame_row(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    frame: &CallFrame,
    selected: bool,
    kind: FrameKind,
) -> bool {
    let theme = ctx.theme;
    let label = frame_label(frame, kind);
    let text_color = match kind {
        FrameKind::Async => theme.text_dim,
        FrameKind::Sync if frame.is_blackboxed => theme.text_dim,
        FrameKind::Sync if selected => theme.accent,
        FrameKind::Sync => theme.text,
    };
    let rich = if selected && matches!(kind, FrameKind::Sync) {
        RichText::new(label).color(text_color).strong()
    } else {
        RichText::new(label).color(text_color)
    };

    match kind {
        FrameKind::Async => {
            // Not selectable — async frames have no live callFrameId.
            ui.add(egui::Label::new(rich));
            false
        }
        FrameKind::Sync => ui
            .add(egui::Button::new(rich).fill(if selected {
                theme.panel
            } else {
                egui::Color32::TRANSPARENT
            }))
            .clicked(),
    }
}
