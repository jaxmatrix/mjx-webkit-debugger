//! The application: window, dock, and the wiring between them and the session.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**
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
use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_source::highlight::TreeSitterHighlighter;
use mjx_wk_source::{Highlighter, SourceTreeNode};
use mjx_wk_ui::code_view::{CodeView, CodeViewModel};
use mjx_wk_ui::source_tree::SourceTree;
use mjx_wk_ui::{Action, Panel, PanelCtx, PanelId, SupportQuery, Theme};
use tokio::sync::mpsc;

use crate::session_host::{self, HostStartup, SessionCommand};
use crate::snapshot::{SharedSnapshot, ShellSnapshot};
use crate::support::DetachedSupport;
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

/// Dock tabs for the v0.1 shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    Sources,
    Code,
    /// Demonstrates "disabled with a reason" for an unsupported panel.
    Network,
    Console,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::Sources => "Sources",
            Tab::Code => "Code",
            Tab::Network => "Network",
            Tab::Console => "Console",
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
    highlighter: TreeSitterHighlighter,
    /// Cached highlight lines for the last painted window.
    highlight_spans: Vec<Vec<mjx_wk_source::HighlightSpan>>,
    highlight_start: u32,
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
            let [_sources, code] = surface.split_right(NodeIndex::root(), 0.28, vec![Tab::Code]);
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
            highlighter: TreeSitterHighlighter::new(),
            highlight_spans: Vec::new(),
            highlight_start: 0,
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

        // egui 0.35: TopBottomPanel → Panel; show_inside → show.
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
        let support = DetachedSupport::not_attached();
        let mut viewer = ShellTabViewer {
            snap: &snap,
            support: &support,
            theme: &self.theme,
            source_tree: &mut self.source_tree,
            code_view: &mut self.code_view,
            highlighter: &mut self.highlighter,
            highlight_spans: &mut self.highlight_spans,
            highlight_start: &mut self.highlight_start,
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
    highlighter: &'a mut TreeSitterHighlighter,
    highlight_spans: &'a mut Vec<Vec<mjx_wk_source::HighlightSpan>>,
    highlight_start: &'a mut u32,
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
                    self.actions.extend(self.source_tree.ui(
                        ui,
                        &ctx,
                        tree,
                        self.snap.selected,
                    ));
                }
            }
            Tab::Code => match self.snap.selected_text.as_deref() {
                Some(text) => {
                    let visible = self.code_view.last_visible_line_range();
                    let window =
                        CodeView::highlight_window(visible, text.index().line_count());
                    *self.highlight_spans = self.highlighter.spans(text, window.clone());
                    *self.highlight_start = window.start;
                    let model = CodeViewModel {
                        text,
                        spans: self.highlight_spans.as_slice(),
                        spans_start_line: *self.highlight_start,
                        breakpoints: &[],
                        execution_line: None,
                        inline_values: &[],
                    };
                    self.actions
                        .extend(self.code_view.ui(ui, &ctx, &model));
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
            Tab::Network => {
                self.actions
                    .extend(render_disabled_panel(ui, self.network, self.support));
            }
            Tab::Console => {
                ui.heading("Console");
                ui.label("Console panel arrives in Phase 2 (T-204).");
                ui.label(self.snap.status.as_str());
            }
        }
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
