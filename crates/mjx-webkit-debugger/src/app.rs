//! The application: window, dock, and the wiring between them and the session.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md` (Phase 2 shell wiring).**
//!
//! # The split that keeps it fast
//!
//! ```text
//!   main thread                        session thread (tokio)
//!   ───────────                        ──────────────────────
//!   eframe event loop                  owns the Transport
//!   Panel::ui  ──► Vec<Action> ──────► drains, sends commands
//!   reads Arc snapshots  ◄──────────── agents publish snapshots
//! ```
//!
//! The main thread never awaits and never locks anything the session thread
//! holds for long. Snapshots cross as `Arc` clones through `ArcSwap`, so
//! reading one is a pointer copy however large the state behind it is.
//!
//! # Frame order (load-bearing)
//!
//! Actions produced in frame N are dispatched **before** snapshots are read in
//! frame N+1, so a click is acted on one frame earlier than if we read first.

use std::path::PathBuf;

use anyhow::Result;
use egui_dock::{DockArea, DockState, NodeIndex, Style as DockStyle, TabViewer};
use mjx_wk_debug::{BreakpointState, DebugModel};
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::highlight::TreeSitterHighlighter;
use mjx_wk_source::{Highlighter, SourceTreeNode};
use mjx_wk_ui::call_stack::CallStackList;
use mjx_wk_ui::code_view::{BreakpointMark, CodeView, CodeViewModel};
use mjx_wk_ui::console_view::ConsoleView;
use mjx_wk_ui::source_tree::SourceTree;
use mjx_wk_ui::variables::{VariablesModel, VariablesTree};
use mjx_wk_ui::{Action, Panel, PanelCtx, PanelId, SupportQuery, Theme};
use tokio::sync::mpsc;

use crate::session_host::{self, HostStartup, SessionCommand};
use crate::snapshot::{SharedSnapshot, ShellSnapshot};
use crate::support::{DetachedSupport, SessionSupport};
use crate::ui_thread::UiFrameGuard;

/// How the application starts.
#[derive(Debug)]
pub enum Startup {
    /// Open with the target picker.
    Picker,
    /// Attach immediately.
    Attach {
        address: String,
        target: Option<usize>,
    },
    /// Drive the UI from a recorded trace, with no debuggee.
    Replay { fixture: PathBuf },
}

/// Dock tabs for the Phase 2 shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    Sources,
    Code,
    CallStack,
    Variables,
    Console,
    /// Demonstrates "disabled with a reason" for an unsupported panel.
    Network,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::Sources => "Sources",
            Tab::Code => "Code",
            Tab::CallStack => "Call stack",
            Tab::Variables => "Variables",
            Tab::Console => "Console",
            Tab::Network => "Network",
        }
    }
}

/// The eframe application.
pub struct App {
    dock: DockState<Tab>,
    /// Actions produced last frame; drained at the start of this frame.
    pending_actions: Vec<Action>,
    /// UI → session (never blocks: try_send only).
    session_tx: mpsc::UnboundedSender<SessionCommand>,
    snapshot: SharedSnapshot,
    theme: Theme,
    source_tree: SourceTree,
    code_view: CodeView,
    call_stack: CallStackList,
    variables: VariablesTree,
    console: ConsoleView,
    highlighter: TreeSitterHighlighter,
    /// Cached highlight lines for the last painted window.
    highlight_spans: Vec<Vec<mjx_wk_source::HighlightSpan>>,
    highlight_start: u32,
    /// Scratch buffer for breakpoint gutter marks (rebuilt each Code frame).
    bp_marks: Vec<(u32, BreakpointMark)>,
    network_panel: NetworkDisabledPanel,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("pending_actions", &self.pending_actions.len())
            .finish_non_exhaustive()
    }
}

/// Open the window and run until it closes.
pub fn run(startup: Startup) -> Result<()> {
    let host = match &startup {
        Startup::Picker => HostStartup::Idle,
        Startup::Attach { address, target } => HostStartup::Attach {
            address: address.clone(),
            target: *target,
        },
        Startup::Replay { fixture } => HostStartup::Replay {
            fixture: fixture.clone(),
        },
    };

    let snapshot = crate::snapshot::new_shared_snapshot();
    let session_tx = session_host::spawn(host, snapshot.clone())?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mjx-webkit-debugger")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    let session_tx_for_exit = session_tx.clone();
    eframe::run_native(
        "mjx-webkit-debugger",
        native_options,
        Box::new(move |_cc| Ok(Box::new(App::new(session_tx, snapshot)))),
    )
    .map_err(|err| anyhow::anyhow!("eframe: {err}"))?;

    let _ = session_tx_for_exit.send(SessionCommand::Shutdown);
    Ok(())
}

impl App {
    fn new(session_tx: mpsc::UnboundedSender<SessionCommand>, snapshot: SharedSnapshot) -> Self {
        let mut dock = DockState::new(vec![Tab::Sources]);
        {
            let surface = dock.main_surface_mut();
            let [_sources, code] = surface.split_right(NodeIndex::root(), 0.22, vec![Tab::Code]);
            let [code, right] = surface.split_right(code, 0.62, vec![Tab::CallStack]);
            let [_stack, _vars] = surface.split_below(right, 0.45, vec![Tab::Variables]);
            let [code, _console] = surface.split_below(code, 0.72, vec![Tab::Console]);
            surface[code].append_tab(Tab::Network);
        }

        Self {
            dock,
            pending_actions: Vec::new(),
            session_tx,
            snapshot,
            theme: Theme::dark(),
            source_tree: SourceTree::new(),
            code_view: CodeView::new(),
            call_stack: CallStackList::new(),
            variables: VariablesTree::new(),
            console: ConsoleView::new(),
            highlighter: TreeSitterHighlighter::new(),
            highlight_spans: Vec::new(),
            highlight_start: 0,
            bp_marks: Vec::new(),
            network_panel: NetworkDisabledPanel::new(),
        }
    }

    /// Drain last frame's actions to the session task (non-blocking).
    fn dispatch_pending_actions(&mut self) {
        for action in self.pending_actions.drain(..) {
            match self.session_tx.send(SessionCommand::Action(action)) {
                Ok(()) => {}
                Err(mpsc::error::SendError(SessionCommand::Action(action))) => {
                    tracing::warn!(?action, "session task gone; dropping action");
                }
                Err(mpsc::error::SendError(_)) => {
                    tracing::warn!("session task gone");
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _guard = UiFrameGuard::enter();

        // 1. Actions from frame N, before reading snapshots for frame N+1.
        self.dispatch_pending_actions();

        // 2. Snapshot is a pointer load — never awaits.
        let snap = self.snapshot.load_full();

        egui::containers::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("mjx-webkit-debugger");
                ui.separator();
                ui.label(snap.status.as_str());
                if snap.connected {
                    ui.colored_label(egui::Color32::from_rgb(80, 200, 120), "connected");
                }
                if !snap.active_agents.is_empty() {
                    ui.separator();
                    ui.label(format!("agents: {}", snap.active_agents.join(", ")));
                }
            });
        });

        // 3. Dock: panels return Actions collected for the next frame.
        let detached = DetachedSupport::not_attached();
        let session_support = snap.session.as_ref().map(SessionSupport::new);
        let support: &dyn SupportQuery = match &session_support {
            Some(s) => s,
            None => &detached,
        };

        let debug_model = snap.debug.as_ref().map(|d| d.load_full());
        let console_model = snap.console.as_ref().map(|c| c.load_full());

        let mut viewer = ShellTabViewer {
            snap: &snap,
            support,
            theme: &self.theme,
            source_tree: &mut self.source_tree,
            code_view: &mut self.code_view,
            call_stack: &mut self.call_stack,
            variables: &mut self.variables,
            console: &mut self.console,
            highlighter: &mut self.highlighter,
            highlight_spans: &mut self.highlight_spans,
            highlight_start: &mut self.highlight_start,
            bp_marks: &mut self.bp_marks,
            debug: debug_model.as_deref(),
            console_model: console_model.as_deref(),
            network: &mut self.network_panel,
            actions: Vec::new(),
        };

        DockArea::new(&mut self.dock)
            .style(DockStyle::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut viewer);

        self.pending_actions.append(&mut viewer.actions);

        // Keep painting while the session host updates the snapshot.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn on_exit(&mut self) {
        let _ = self.session_tx.send(SessionCommand::Shutdown);
    }
}

struct ShellTabViewer<'a> {
    snap: &'a ShellSnapshot,
    support: &'a dyn SupportQuery,
    theme: &'a Theme,
    source_tree: &'a mut SourceTree,
    code_view: &'a mut CodeView,
    call_stack: &'a mut CallStackList,
    variables: &'a mut VariablesTree,
    console: &'a mut ConsoleView,
    highlighter: &'a mut TreeSitterHighlighter,
    highlight_spans: &'a mut Vec<Vec<mjx_wk_source::HighlightSpan>>,
    highlight_start: &'a mut u32,
    bp_marks: &'a mut Vec<(u32, BreakpointMark)>,
    debug: Option<&'a DebugModel>,
    console_model: Option<&'a mjx_wk_console::ConsoleModel>,
    network: &'a mut NetworkDisabledPanel,
    actions: Vec<Action>,
}

impl TabViewer for ShellTabViewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let ctx = PanelCtx {
            theme: self.theme,
            support: self.support,
        };
        match *tab {
            Tab::Sources => {
                let tree = &self.snap.tree;
                if is_empty_tree(tree) {
                    ui.label("No sources yet.");
                    for note in &self.snap.notes {
                        ui.label(note);
                    }
                } else {
                    self.actions
                        .extend(self.source_tree.ui(ui, &ctx, tree, self.snap.selected));
                }
            }
            Tab::Code => match self.snap.selected_text.as_deref() {
                Some(text) => {
                    let visible = self.code_view.last_visible_line_range();
                    let window = CodeView::highlight_window(visible, text.index().line_count());
                    *self.highlight_spans = self.highlighter.spans(text, window.clone());
                    *self.highlight_start = window.start;

                    self.bp_marks.clear();
                    if let Some(debug) = self.debug {
                        for bp in debug.breakpoints.in_source(text.id()) {
                            let line = match &bp.state {
                                BreakpointState::Resolved { actual } => actual.line,
                                _ => bp.spec.location.line,
                            };
                            self.bp_marks.push((line, mark_for(bp)));
                        }
                    }

                    let execution_line = self.debug.and_then(|d| {
                        d.paused.as_ref().and_then(|p| {
                            p.current_frame().and_then(|f| {
                                (f.location.source == text.id()).then_some(f.location.line)
                            })
                        })
                    });

                    let model = CodeViewModel {
                        text,
                        spans: self.highlight_spans.as_slice(),
                        spans_start_line: *self.highlight_start,
                        breakpoints: self.bp_marks.as_slice(),
                        execution_line,
                        inline_values: &[],
                    };
                    self.actions.extend(self.code_view.ui(ui, &ctx, &model));
                }
                None => {
                    ui.heading("Code");
                    if self.snap.selected.is_some() {
                        ui.label("Loading source text…");
                    } else {
                        ui.label("Select a source in the tree.");
                    }
                    for note in &self.snap.notes {
                        ui.small(note);
                    }
                }
            },
            Tab::CallStack => {
                if let Some(debug) = self.debug {
                    if let Some(reason) = &debug.disabled_reason {
                        ui.add_enabled_ui(false, |ui| {
                            ui.heading("Call stack");
                            ui.label(reason);
                        });
                    } else {
                        self.actions
                            .extend(self.call_stack.ui(ui, &ctx, debug.paused.as_ref()));
                    }
                } else {
                    ui.add_enabled_ui(false, |ui| {
                        ui.heading("Call stack");
                        ui.label(
                            "Unavailable: Debugger domain is not attached on this target \
                             (not attached, wrong target kind, or dialect gap).",
                        );
                    });
                }
            }
            Tab::Variables => {
                if let Some(debug) = self.debug {
                    if let Some(reason) = &debug.disabled_reason {
                        ui.add_enabled_ui(false, |ui| {
                            ui.heading("Variables");
                            ui.label(reason);
                        });
                    } else {
                        let values = debug.paused.as_ref().and_then(|p| {
                            p.current_frame()
                                .and_then(|f| f.scopes.iter().find_map(|s| s.values.as_ref()))
                        });
                        let model = VariablesModel {
                            values,
                            watches: debug.watches.as_slice(),
                        };
                        self.actions.extend(self.variables.ui(ui, &ctx, &model));
                    }
                } else {
                    ui.add_enabled_ui(false, |ui| {
                        ui.heading("Variables");
                        ui.label(
                            "Unavailable: Runtime.getProperties is not supported on this target \
                             (not attached, wrong target kind, or dialect gap).",
                        );
                    });
                }
            }
            Tab::Console => match self.console_model {
                Some(model) => {
                    self.actions.extend(self.console.ui(ui, &ctx, model));
                }
                None => {
                    ui.add_enabled_ui(false, |ui| {
                        ui.heading("Console");
                        ui.label(
                            "Unavailable: `Console.enable` is not supported on this target \
                             (not attached, wrong target kind, or dialect gap).",
                        );
                        ui.separator();
                        ui.label("The panel stays visible so its absence is never a mystery.");
                    });
                }
            },
            Tab::Network => {
                self.actions
                    .extend(render_disabled_panel(ui, self.network, self.support));
            }
        }
    }
}

fn mark_for(bp: &mjx_wk_debug::Breakpoint) -> BreakpointMark {
    if !bp.spec.enabled {
        return BreakpointMark::Disabled;
    }
    if bp.spec.is_logpoint() {
        return BreakpointMark::Logpoint;
    }
    if bp.spec.condition.is_some() {
        return BreakpointMark::Conditional;
    }
    match &bp.state {
        BreakpointState::Pending => BreakpointMark::Pending,
        BreakpointState::Resolved { .. } => BreakpointMark::Resolved,
        BreakpointState::Failed { .. } => BreakpointMark::Pending,
        BreakpointState::Disabled => BreakpointMark::Disabled,
    }
}

fn is_empty_tree(tree: &SourceTreeNode) -> bool {
    match tree {
        SourceTreeNode::Group { children, .. } => children.is_empty(),
        SourceTreeNode::Leaf { .. } => false,
    }
}

/// Render a panel disabled with a reason when any required member is unsupported.
fn render_disabled_panel(
    ui: &mut egui::Ui,
    panel: &mut NetworkDisabledPanel,
    support: &dyn SupportQuery,
) -> Vec<Action> {
    let mut reason: Option<(Domain, &'static str, Support)> = None;
    for &(domain, member) in panel.requires() {
        let s = support.supports(domain, member);
        if !s.is_available() {
            reason = Some((domain, member, s));
            break;
        }
    }

    if let Some((domain, member, _)) = reason {
        ui.add_enabled_ui(false, |ui| {
            ui.heading(panel.title());
            ui.label(format!(
                "Unavailable: `{domain}.{member}` is not supported on this target \
                 (not attached, wrong target kind, or dialect gap)."
            ));
            ui.separator();
            ui.label("The panel stays visible so its absence is never a mystery.");
        });
        Vec::new()
    } else {
        ui.label("Network panel ready (Wave 3).");
        Vec::new()
    }
}

/// Stand-in Network panel that declares a requirement so the disabled path runs.
#[derive(Debug, Default)]
struct NetworkDisabledPanel;

impl NetworkDisabledPanel {
    fn new() -> Self {
        Self
    }
}

impl Panel for NetworkDisabledPanel {
    fn id(&self) -> PanelId {
        PanelId("network")
    }

    fn title(&self) -> &str {
        "Network"
    }

    fn requires(&self) -> &[(Domain, &'static str)] {
        &[(Domain::Network, "enable")]
    }

    fn ui(&mut self, _ui: &mut egui::Ui, _ctx: &PanelCtx<'_>) -> Vec<Action> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot;

    #[test]
    fn actions_drain_before_snapshot_read_order_is_documented() {
        let snapshot = snapshot::new_shared_snapshot();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new(tx, snapshot);
        app.pending_actions
            .push(Action::OpenSource(mjx_wk_source::SourceId(1), Some(10)));
        app.dispatch_pending_actions();
        assert!(app.pending_actions.is_empty());
        let cmd = rx.try_recv().expect("action forwarded");
        assert!(matches!(
            cmd,
            SessionCommand::Action(Action::OpenSource(mjx_wk_source::SourceId(1), Some(10)))
        ));
    }

    #[test]
    fn run_rejects_nothing_about_startup_variants() {
        let _ = Startup::Picker;
        let _ = Startup::Attach {
            address: "127.0.0.1:2999".into(),
            target: Some(0),
        };
        let _ = Startup::Replay {
            fixture: PathBuf::from("fixtures/attach.jsonl"),
        };
    }

    #[test]
    fn unsupported_network_panel_reports_unavailable_reason() {
        let panel = NetworkDisabledPanel::new();
        let support = DetachedSupport::not_attached();
        let blocked = panel
            .requires()
            .iter()
            .find(|&&(domain, member)| !support.supports(domain, member).is_available());
        assert!(
            blocked.is_some(),
            "DetachedSupport must disable Network so the panel stays visible with a reason"
        );
    }
}
