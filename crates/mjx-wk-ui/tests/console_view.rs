//! ConsoleView — disabled-with-reason, message paint, Evaluate action.
//!
//! **Owned by `docs/tasks/T-204-console.md`.**

#![allow(clippy::unwrap_used, clippy::expect_used)]

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use mjx_wk_console::{ConsoleMessage, ConsoleModel, MessageLevel, MessageSource};
use mjx_wk_dialect::{CdpDialect, Dialect, Support, WebKitDialect};
use mjx_wk_protocol::Domain;
use mjx_wk_ui::console_view::ConsoleView;
use mjx_wk_ui::{Action, Panel, PanelCtx, SupportQuery, Theme};

#[derive(Debug)]
struct DialectSupport {
    dialect: Box<dyn Dialect>,
}

impl SupportQuery for DialectSupport {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        self.dialect.supports(domain, member)
    }
}

#[derive(Debug)]
struct AllUnsupported;

impl SupportQuery for AllUnsupported {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Unsupported
    }
}

#[derive(Debug)]
struct AllNative;

impl SupportQuery for AllNative {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Native
    }
}

struct ViewState {
    view: ConsoleView,
    theme: Theme,
    support: Box<dyn SupportQuery>,
    model: ConsoleModel,
    actions: Vec<Action>,
}

fn sample_message(text: &str, level: MessageLevel) -> ConsoleMessage {
    ConsoleMessage {
        source: MessageSource::ConsoleApi,
        level,
        text: text.to_owned(),
        argument_object_ids: Vec::new(),
        location: None,
        repeat_count: 1,
    }
}

#[test]
fn webkit_and_cdp_both_report_console_enable_available() {
    assert!(
        WebKitDialect
            .supports(Domain::Console, "enable")
            .is_available()
    );
    // CDP emulates Console via Runtime.consoleAPICalled / Log.entryAdded —
    // Emulated still counts as available so the panel is not greyed out.
    assert_eq!(
        CdpDialect.supports(Domain::Console, "enable"),
        Support::Emulated
    );
    assert!(
        CdpDialect
            .supports(Domain::Console, "enable")
            .is_available()
    );
}

#[test]
fn panel_requires_console_enable() {
    let view = ConsoleView::new();
    assert_eq!(view.requires(), &[(Domain::Console, "enable")]);
}

#[test]
fn unsupported_renders_disabled_with_reason() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 240.0))
        .build_ui_state(
            |ui, state: &mut ViewState| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: state.support.as_ref(),
                };
                let actions = state.view.ui(ui, &ctx, &state.model);
                state.actions.extend(actions);
            },
            ViewState {
                view: ConsoleView::new(),
                theme: Theme::dark(),
                support: Box::new(AllUnsupported),
                model: ConsoleModel::new(),
                actions: Vec::new(),
            },
        );

    harness.run();
    let root = format!("{:?}", harness);
    assert!(
        root.contains("Unavailable")
            || harness.query_all_by_label_contains("Unavailable").count() > 0
            || harness.query_all_by_label_contains("not supported").count() > 0,
        "disabled panel must show a reason, got {root}"
    );
    assert!(
        harness.state().actions.is_empty(),
        "disabled console must not emit Evaluate"
    );
}

#[test]
fn cdp_dialect_support_keeps_panel_enabled() {
    let support = DialectSupport {
        dialect: Box::new(CdpDialect),
    };
    let view = ConsoleView::new();
    let theme = Theme::dark();
    let ctx = PanelCtx {
        theme: &theme,
        support: &support,
    };
    assert!(
        view.unavailable_reason(&ctx).is_none(),
        "CDP emulated Console must not disable the panel"
    );
}

#[test]
fn webkit_dialect_support_keeps_panel_enabled() {
    let support = DialectSupport {
        dialect: Box::new(WebKitDialect),
    };
    let view = ConsoleView::new();
    let theme = Theme::dark();
    let ctx = PanelCtx {
        theme: &theme,
        support: &support,
    };
    assert!(view.unavailable_reason(&ctx).is_none());
}

#[test]
fn dropped_count_is_announced() {
    let mut model = ConsoleModel::new();
    model.dropped = 4;
    model
        .messages
        .push(sample_message("still here", MessageLevel::Log));

    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 240.0))
        .build_ui_state(
            |ui, state: &mut ViewState| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: state.support.as_ref(),
                };
                let _ = state.view.ui(ui, &ctx, &state.model);
            },
            ViewState {
                view: ConsoleView::new(),
                theme: Theme::dark(),
                support: Box::new(AllNative),
                model,
                actions: Vec::new(),
            },
        );

    harness.run();
    assert!(
        harness.query_all_by_label_contains("dropped").count() > 0,
        "must report how many messages were dropped"
    );
}

#[test]
fn enter_in_prompt_emits_evaluate() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 240.0))
        .build_ui_state(
            |ui, state: &mut ViewState| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: state.support.as_ref(),
                };
                let actions = state.view.ui(ui, &ctx, &state.model);
                state.actions.extend(actions);
            },
            ViewState {
                view: ConsoleView::new(),
                theme: Theme::dark(),
                support: Box::new(AllNative),
                model: ConsoleModel::new(),
                actions: Vec::new(),
            },
        );

    let edit = harness.get_by_role(Role::TextInput);
    edit.focus();
    harness.run();
    harness.get_by_role(Role::TextInput).type_text("1+1");
    harness.run();
    // Press Enter while the field has focus.
    harness.get_by_role(Role::TextInput).focus();
    harness.key_press(egui::Key::Enter);
    harness.run();

    assert!(
        harness
            .state()
            .actions
            .iter()
            .any(|a| matches!(a, Action::Evaluate(s) if s == "1+1")),
        "expected Evaluate(1+1), got {:?}",
        harness.state().actions
    );
}

#[test]
fn repeat_badge_and_message_text_render() {
    let mut model = ConsoleModel::new();
    let mut msg = sample_message("tick", MessageLevel::Log);
    msg.repeat_count = 9;
    model.messages.push(msg);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 240.0))
        .build_ui_state(
            |ui, state: &mut ViewState| {
                let ctx = PanelCtx {
                    theme: &state.theme,
                    support: state.support.as_ref(),
                };
                let _ = state.view.ui(ui, &ctx, &state.model);
            },
            ViewState {
                view: ConsoleView::new(),
                theme: Theme::dark(),
                support: Box::new(AllNative),
                model,
                actions: Vec::new(),
            },
        );

    harness.run();
    assert!(harness.query_all_by_label_contains("tick").count() > 0);
    assert!(harness.query_all_by_label_contains("×9").count() > 0);
}
