//! Background session task: attach or replay without blocking the UI thread.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md` (Phase 2 shell wiring).**

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use mjx_wk_console::{ConsoleAgent, EvalTarget, evaluate};
use mjx_wk_debug::{
    BreakpointSpec, BreakpointUrl, DebugAgent, DebugModel, StepKind as DebugStepKind, ValueNodeId,
    ValuePreview, ValueTree, values::PAGE_SIZE,
};
use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_protocol::TargetType;
use mjx_wk_protocol::generated::debugger;
use mjx_wk_protocol::generated::page;
use mjx_wk_protocol::generated::runtime;
use mjx_wk_session::{AgentRegistry, AgentSnapshot, Session, SessionHandle};
use mjx_wk_source::{SourceId, SourceInventory, SourceLocation, SourceStore, SourceText};
use mjx_wk_transport::{
    Discovery, ReplayTransport, Target, TargetKey, TcpInspectorServer, TransportOrigin,
};
use mjx_wk_ui::{Action, StepKind};
use tokio::sync::mpsc;

use crate::fixture_seed;
use crate::snapshot::{self, SharedSnapshot, ShellSnapshot};
use crate::ui_thread;

/// Messages the UI may enqueue for the session task.
#[derive(Debug)]
pub enum SessionCommand {
    /// Forward a user action.
    Action(Action),
    /// Shut down the session task.
    Shutdown,
}

/// How the host should open a session.
#[derive(Debug, Clone)]
pub enum HostStartup {
    Replay {
        fixture: PathBuf,
    },
    Attach {
        address: String,
        target: Option<usize>,
    },
    /// No transport yet — picker mode.
    Idle,
}

/// Spawn the tokio runtime on a dedicated OS thread and return the action sink.
pub fn spawn(
    startup: HostStartup,
    snapshot: SharedSnapshot,
) -> anyhow::Result<mpsc::UnboundedSender<SessionCommand>> {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::Builder::new()
        .name("mjx-session".into())
        .spawn(move || {
            ui_thread::ensure_not_ui_thread();
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("mjx-session-worker")
                .build()
            {
                Ok(runtime) => runtime.block_on(run_host(startup, snapshot, rx)),
                Err(err) => {
                    tracing::error!(error = %err, "failed to build session tokio runtime");
                    snapshot::publish(
                        &snapshot,
                        ShellSnapshot {
                            status: "session runtime failed".into(),
                            notes: vec![format!("{err:#}")],
                            ..ShellSnapshot::default()
                        },
                    );
                }
            }
        })
        .map_err(|err| anyhow::anyhow!("spawn session thread: {err}"))?;
    Ok(tx)
}

async fn run_host(
    startup: HostStartup,
    snapshot: SharedSnapshot,
    mut rx: mpsc::UnboundedReceiver<SessionCommand>,
) {
    match startup {
        HostStartup::Idle => {
            snapshot::publish(
                &snapshot,
                ShellSnapshot {
                    status: "no target — pick one or pass `attach` / `replay`".into(),
                    notes: vec![
                        "Use `list` / `attach` from the CLI, or `replay fixtures/attach.jsonl`."
                            .into(),
                    ],
                    ..ShellSnapshot::default()
                },
            );
            drain_until_shutdown(&mut rx).await;
        }
        HostStartup::Replay { fixture } => run_replay(fixture, snapshot, &mut rx).await,
        HostStartup::Attach { address, target } => {
            run_attach(address, target, snapshot, &mut rx).await;
        }
    }
}

async fn run_replay(
    fixture: PathBuf,
    snapshot: SharedSnapshot,
    rx: &mut mpsc::UnboundedReceiver<SessionCommand>,
) {
    let path_display = fixture.display().to_string();
    let mut inventory = SourceInventory::new();
    let store = SourceStore::new(64 * 1024 * 1024);

    let seed_note = match fixture_seed::seed_inventory_from_fixture(&fixture, &mut inventory) {
        Ok((scripts, tree)) => {
            format!("Seeded inventory from fixture: {scripts} scriptParsed, resource tree={tree}.")
        }
        Err(err) => format!("Fixture seed failed: {err:#}"),
    };

    snapshot::publish(
        &snapshot,
        ShellSnapshot {
            status: format!("loading replay {path_display}"),
            notes: vec![seed_note.clone()],
            tree: inventory.tree(),
            ..ShellSnapshot::default()
        },
    );

    let transport = if fixture_seed::is_multiplexed_fixture(&fixture) {
        match fixture_seed::flatten_multiplexed_trace(&fixture) {
            Ok(flat) => match ReplayTransport::from_str(&flat, format!("{path_display} (flat)")) {
                Ok(t) => t,
                Err(err) => {
                    snapshot::update(&snapshot, |s| {
                        s.status = "replay load failed".into();
                        s.notes.push(format!("{err:#}"));
                    });
                    drain_until_shutdown(rx).await;
                    return;
                }
            },
            Err(err) => {
                snapshot::update(&snapshot, |s| {
                    s.status = "replay flatten failed".into();
                    s.notes.push(format!("{err:#}"));
                });
                drain_until_shutdown(rx).await;
                return;
            }
        }
    } else {
        match ReplayTransport::from_file(&fixture) {
            Ok(t) => t,
            Err(err) => {
                snapshot::update(&snapshot, |s| {
                    s.status = "replay load failed".into();
                    s.notes.push(format!("{err:#}"));
                });
                drain_until_shutdown(rx).await;
                return;
            }
        }
    };

    let target = Target {
        key: TargetKey("replay/0".into()),
        name: fixture
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("replay")
            .to_owned(),
        url: path_display.clone(),
        kind: TargetType::WebPage,
        dialect: DialectKind::WebKitRwi,
        origin: TransportOrigin::Replay {
            fixture: path_display.clone(),
        },
    };

    match Session::attach(Box::new(transport), Box::new(WebKitDialect), target).await {
        Ok(session) => {
            let mut registry = AgentRegistry::new();
            let (debug, console) = register_phase2_agents(&mut registry, &session).await;

            let _ = session.call(page::commands::Enable {}).await;
            if let Ok(tree) = session.call(page::commands::GetResourceTree {}).await {
                inventory.on_resource_tree(&tree.frame_tree);
            }

            let mut state = HostState {
                inventory,
                store,
                session: Some(session.clone()),
                registry,
                selected: None,
                debug,
                console,
            };

            // Drive breakpoint-hit style fixtures far enough for pause + console.
            drive_replay_fixture(&fixture, &session, &mut state).await;

            publish_host(
                &snapshot,
                &state,
                &format!("replay attached: {path_display}"),
                vec![
                    seed_note,
                    "DebugAgent + ConsoleAgent registered via AgentRegistry.".into(),
                ],
            );

            drain_with_events(rx, &snapshot, state).await;
        }
        Err(err) => {
            snapshot::publish(
                &snapshot,
                ShellSnapshot {
                    status: format!("replay (offline seed): {path_display}"),
                    notes: vec![
                        seed_note,
                        format!(
                            "Session::attach failed ({err}). Multiplexed WebKitGTK traces need \
                             page-target routing; the tree below is seeded from the fixture."
                        ),
                    ],
                    connected: false,
                    tree: inventory.tree(),
                    selected: None,
                    selected_text: None,
                    active_agents: Vec::new(),
                    session: None,
                    debug: None,
                    console: None,
                },
            );
            let state = HostState {
                inventory,
                store,
                session: None,
                registry: AgentRegistry::new(),
                selected: None,
                debug: None,
                console: None,
            };
            drain_actions(rx, &snapshot, state).await;
        }
    }
}

async fn run_attach(
    address: String,
    target_index: Option<usize>,
    snapshot: SharedSnapshot,
    rx: &mut mpsc::UnboundedReceiver<SessionCommand>,
) {
    snapshot::publish(
        &snapshot,
        ShellSnapshot {
            status: format!("discovering targets at {address}"),
            ..ShellSnapshot::default()
        },
    );

    let server = TcpInspectorServer::new(address.clone());
    let targets = match server.list().await {
        Ok(t) => t,
        Err(err) => {
            snapshot::update(&snapshot, |s| {
                s.status = "discovery failed".into();
                s.notes = vec![format!("{err:#}")];
            });
            drain_until_shutdown(rx).await;
            return;
        }
    };

    if targets.is_empty() {
        snapshot::update(&snapshot, |s| {
            s.status = format!("no targets at {address}");
            s.notes = vec![
                "Start the debuggee with WEBKIT_INSPECTOR_SERVER set and developer extras on."
                    .into(),
            ];
        });
        drain_until_shutdown(rx).await;
        return;
    }

    let index = target_index.unwrap_or(0);
    let Some(target) = targets.get(index).cloned() else {
        snapshot::update(&snapshot, |s| {
            s.status = format!(
                "target index {index} out of range ({} found)",
                targets.len()
            );
            s.notes = targets
                .iter()
                .enumerate()
                .map(|(i, t)| format!("[{i}] {}  {}", t.name, t.url))
                .collect();
        });
        drain_until_shutdown(rx).await;
        return;
    };

    let transport = match server.attach(&target.key).await {
        Ok(t) => t,
        Err(err) => {
            snapshot::update(&snapshot, |s| {
                s.status = "attach failed".into();
                s.notes = vec![format!("{err:#}")];
            });
            drain_until_shutdown(rx).await;
            return;
        }
    };

    match Session::attach(Box::new(transport), Box::new(WebKitDialect), target.clone()).await {
        Ok(session) => {
            let mut inventory = SourceInventory::new();
            let store = SourceStore::new(64 * 1024 * 1024);
            let mut registry = AgentRegistry::new();
            let (debug, console) = register_phase2_agents(&mut registry, &session).await;

            let _ = session.call(page::commands::Enable {}).await;
            if let Ok(tree) = session.call(page::commands::GetResourceTree {}).await {
                inventory.on_resource_tree(&tree.frame_tree);
            }

            let state = HostState {
                inventory,
                store,
                session: Some(session),
                registry,
                selected: None,
                debug,
                console,
            };
            publish_host(
                &snapshot,
                &state,
                &format!("attached to {} ({})", target.name, target.url),
                Vec::new(),
            );
            drain_with_events(rx, &snapshot, state).await;
        }
        Err(err) => {
            snapshot::update(&snapshot, |s| {
                s.status = "session attach failed".into();
                s.notes = vec![format!("{err:#}")];
            });
            drain_until_shutdown(rx).await;
        }
    }
}

async fn register_phase2_agents(
    registry: &mut AgentRegistry,
    session: &SessionHandle,
) -> (
    Option<AgentSnapshot<DebugModel>>,
    Option<AgentSnapshot<mjx_wk_console::ConsoleModel>>,
) {
    // Console.enable precedes Debugger.enable in WebKitGTK recordings.
    // ReplayTransport drops skipped sends when searching forward, so registering
    // Debug first would consume-past and erase Console.enable from the trace.
    let console = match registry.register(ConsoleAgent::default(), session).await {
        Ok(snap) => snap,
        Err(err) => {
            tracing::warn!(error = %err, "ConsoleAgent register failed");
            None
        }
    };
    let debug = match registry.register(DebugAgent::default(), session).await {
        Ok(snap) => snap,
        Err(err) => {
            tracing::warn!(error = %err, "DebugAgent register failed");
            None
        }
    };
    (debug, console)
}

/// Advance a flattened breakpoint-hit (or similar) fixture to the paused state.
async fn drive_replay_fixture(
    fixture: &std::path::Path,
    session: &SessionHandle,
    state: &mut HostState,
) {
    let name = fixture
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !name.contains("breakpoint") {
        return;
    }

    // Match the recorded setBreakpointByUrl; keep a local copy on the published
    // AgentSnapshot so the gutter shows the mark even though the agent task
    // owns its private store.
    let source = state
        .inventory
        .by_script_id("2")
        .or_else(|| state.inventory.by_script_id("6"))
        .unwrap_or(SourceId(0));
    let spec = BreakpointSpec::at(SourceLocation {
        source,
        line: 3,
        column: 0,
    });

    match session
        .call(debugger::commands::SetBreakpointByUrl {
            line_number: 3,
            url: None,
            url_regex: Some(r".*app\.js".into()),
            column_number: Some(0),
            options: None,
        })
        .await
    {
        Ok(ret) => {
            if let Some(snap) = &state.debug {
                let mut model = (**snap.load()).clone();
                let index = model.breakpoints.insert(spec);
                let id = mjx_wk_debug::BreakpointId(ret.breakpoint_id);
                model.breakpoints.set_id(index, id.clone());
                if let Some(loc) = ret.locations.first() {
                    let actual = SourceLocation {
                        source,
                        line: loc.line_number.max(0) as u32,
                        column: loc.column_number.unwrap_or(0).max(0) as u32,
                    };
                    model.breakpoints.resolve(&id, actual);
                }
                snap.store(Arc::new(model));
            }
        }
        Err(err) => tracing::debug!(error = %err, "replay setBreakpointByUrl skipped"),
    }

    let _ = session
        .call(page::commands::Reload {
            ignore_cache: None,
            revalidate_all_resources: None,
        })
        .await;

    // Give agent tasks a moment to fold paused / console events.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

struct HostState {
    inventory: SourceInventory,
    store: SourceStore,
    session: Option<SessionHandle>,
    /// Kept so agent tasks stay alive for the session lifetime.
    #[allow(dead_code)]
    registry: AgentRegistry,
    selected: Option<SourceId>,
    debug: Option<AgentSnapshot<DebugModel>>,
    console: Option<AgentSnapshot<mjx_wk_console::ConsoleModel>>,
}

fn publish_host(snapshot: &SharedSnapshot, state: &HostState, status: &str, notes: Vec<String>) {
    snapshot::publish(
        snapshot,
        ShellSnapshot {
            status: status.into(),
            notes,
            active_agents: state.registry.active(),
            connected: state.session.as_ref().is_some_and(|s| s.is_connected()),
            tree: state.inventory.tree(),
            selected: state.selected,
            selected_text: None,
            session: state.session.clone(),
            debug: state.debug.clone(),
            console: state.console.clone(),
        },
    );
}

async fn drain_until_shutdown(rx: &mut mpsc::UnboundedReceiver<SessionCommand>) {
    while let Some(cmd) = rx.recv().await {
        if matches!(cmd, SessionCommand::Shutdown) {
            break;
        }
    }
}

async fn drain_actions(
    rx: &mut mpsc::UnboundedReceiver<SessionCommand>,
    snapshot: &SharedSnapshot,
    mut state: HostState,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            SessionCommand::Shutdown => break,
            SessionCommand::Action(action) => {
                handle_action(action, snapshot, &mut state).await;
            }
        }
    }
}

async fn drain_with_events(
    rx: &mut mpsc::UnboundedReceiver<SessionCommand>,
    snapshot: &SharedSnapshot,
    mut state: HostState,
) {
    let Some(session) = state.session.clone() else {
        drain_actions(rx, snapshot, state).await;
        return;
    };

    let mut scripts = session.subscribe::<debugger::events::ScriptParsed>();
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    None | Some(SessionCommand::Shutdown) => break,
                    Some(SessionCommand::Action(action)) => {
                        handle_action(action, snapshot, &mut state).await;
                    }
                }
            }
            event = scripts.next() => {
                let Some(event) = event else { continue; };
                state.inventory.on_script_parsed(&event);
                // Align agent script ids with inventory when possible by
                // republishing tree; agent tracks its own map from the same events.
                snapshot::update(snapshot, |s| {
                    s.tree = state.inventory.tree();
                    s.debug = state.debug.clone();
                    s.console = state.console.clone();
                    s.session = state.session.clone();
                    s.active_agents = state.registry.active();
                });
            }
        }
    }
}

async fn handle_action(action: Action, snapshot: &SharedSnapshot, state: &mut HostState) {
    match action {
        Action::OpenSource(id, line) => {
            state.selected = Some(id);
            let text = resolve_text(id, state).await;
            snapshot::update(snapshot, |s| {
                s.selected = Some(id);
                s.selected_text = text;
                s.status = match line {
                    Some(l) => format!("open source {id} @ line {l}"),
                    None => format!("open source {id}"),
                };
                s.tree = state.inventory.tree();
                s.debug = state.debug.clone();
                s.console = state.console.clone();
                s.session = state.session.clone();
            });
        }
        Action::RequestSource(id) => {
            let text = resolve_text(id, state).await;
            snapshot::update(snapshot, |s| {
                if state.selected == Some(id) {
                    s.selected_text = text;
                }
                s.status = format!("request source {id}");
            });
        }
        Action::ToggleBreakpoint(loc) => {
            toggle_breakpoint(loc, snapshot, state).await;
        }
        Action::RemoveBreakpoint(loc) => {
            remove_breakpoint(loc, snapshot, state).await;
        }
        Action::SetBreakpointsActive(active) => {
            if let Some(session) = &state.session {
                let _ = session
                    .call(debugger::commands::SetBreakpointsActive { active })
                    .await;
            }
            if let Some(snap) = &state.debug {
                let mut model = (**snap.load()).clone();
                model.breakpoints_active = active;
                snap.store(Arc::new(model));
            }
            snapshot::update(snapshot, |s| {
                s.status = format!("breakpoints active={active}");
                s.debug = state.debug.clone();
            });
        }
        Action::Step(kind) => {
            step(kind, snapshot, state).await;
        }
        Action::Resume => {
            if let Some(session) = &state.session {
                let _ = session.call(debugger::commands::Resume {}).await;
            }
            snapshot::update(snapshot, |s| {
                s.status = "resume".into();
                s.debug = state.debug.clone();
            });
        }
        Action::Pause => {
            if let Some(session) = &state.session {
                let _ = session.call(debugger::commands::Pause {}).await;
            }
            snapshot::update(snapshot, |s| {
                s.status = "pause".into();
                s.debug = state.debug.clone();
            });
        }
        Action::SelectFrame(index) => {
            if let Some(snap) = &state.debug {
                let mut model = (**snap.load()).clone();
                if let Some(paused) = model.paused.as_mut()
                    && paused.select_frame(index)
                {
                    let loc = paused.current_frame().map(|f| f.location);
                    snap.store(Arc::new(model));
                    if let Some(loc) = loc {
                        state.selected = Some(loc.source);
                        let text = resolve_text(loc.source, state).await;
                        snapshot::update(snapshot, |s| {
                            s.selected = Some(loc.source);
                            s.selected_text = text;
                            s.status = format!("frame {index}");
                            s.debug = state.debug.clone();
                            s.console = state.console.clone();
                            s.session = state.session.clone();
                            s.tree = state.inventory.tree();
                        });
                        return;
                    }
                }
            }
            snapshot::update(snapshot, |s| {
                s.status = format!("select frame {index}");
                s.debug = state.debug.clone();
            });
        }
        Action::ExpandValue { node, start, count } => {
            expand_value(ValueNodeId(node), start, count, snapshot, state).await;
        }
        Action::Evaluate(expr) => {
            evaluate_expr(&expr, snapshot, state).await;
        }
        Action::AddWatch(expr) => {
            if let Some(snap) = &state.debug {
                let mut model = (**snap.load()).clone();
                model.watches.push(expr.clone());
                snap.store(Arc::new(model));
            }
            snapshot::update(snapshot, |s| {
                s.status = format!("watch {expr}");
                s.debug = state.debug.clone();
            });
        }
        Action::RemoveWatch(index) => {
            if let Some(snap) = &state.debug {
                let mut model = (**snap.load()).clone();
                if index < model.watches.len() {
                    model.watches.remove(index);
                }
                snap.store(Arc::new(model));
            }
            snapshot::update(snapshot, |s| {
                s.status = format!("remove watch {index}");
                s.debug = state.debug.clone();
            });
        }
        other => {
            tracing::debug!(?other, "action not handled by phase-2 host");
            snapshot::update(snapshot, |s| {
                s.status = format!("action: {other:?}");
                s.debug = state.debug.clone();
                s.console = state.console.clone();
            });
        }
    }
}

async fn toggle_breakpoint(loc: SourceLocation, snapshot: &SharedSnapshot, state: &mut HostState) {
    let Some(session) = state.session.clone() else {
        snapshot::update(snapshot, |s| {
            s.status = "toggle breakpoint: no session".into()
        });
        return;
    };
    let Some(snap) = state.debug.clone() else {
        snapshot::update(snapshot, |s| {
            s.status = "toggle breakpoint: debugger unavailable".into();
        });
        return;
    };

    let model = snap.load();
    if let Some(existing) = model
        .breakpoints
        .in_source(loc.source)
        .iter()
        .find(|bp| bp.spec.location.line == loc.line)
    {
        if let Some(id) = existing.id.clone() {
            let _ = session
                .call(debugger::commands::RemoveBreakpoint {
                    breakpoint_id: id.0,
                })
                .await;
        }
        let mut next = (**model).clone();
        next.breakpoints = remove_line_bp(&next.breakpoints, loc);
        snap.store(Arc::new(next));
        snapshot::update(snapshot, |s| {
            s.status = format!("removed breakpoint {}:{}", loc.source, loc.line);
            s.debug = state.debug.clone();
        });
        return;
    }
    drop(model);

    let url = state
        .inventory
        .get(loc.source)
        .map(|e| e.url.clone())
        .unwrap_or_default();
    let url = if url.is_empty() {
        BreakpointUrl::Regex(r".*".into())
    } else {
        BreakpointUrl::Exact(url)
    };
    let (url_exact, url_regex) = match &url {
        BreakpointUrl::Exact(u) => (Some(u.clone()), None),
        BreakpointUrl::Regex(r) => (None, Some(r.clone())),
    };
    let spec = BreakpointSpec::at(loc);

    match session
        .call(debugger::commands::SetBreakpointByUrl {
            line_number: i64::from(loc.line),
            url: url_exact,
            url_regex,
            column_number: Some(i64::from(loc.column)),
            options: None,
        })
        .await
    {
        Ok(ret) => {
            let mut next = (**snap.load()).clone();
            let index = next.breakpoints.insert(spec);
            let id = mjx_wk_debug::BreakpointId(ret.breakpoint_id);
            next.breakpoints.set_id(index, id.clone());
            if let Some(ploc) = ret.locations.first() {
                let actual = SourceLocation {
                    source: loc.source,
                    line: ploc.line_number.max(0) as u32,
                    column: ploc.column_number.unwrap_or(0).max(0) as u32,
                };
                next.breakpoints.resolve(&id, actual);
            }
            snap.store(Arc::new(next));
            snapshot::update(snapshot, |s| {
                s.status = format!("breakpoint {}:{}", loc.source, loc.line);
                s.debug = state.debug.clone();
            });
        }
        Err(err) => {
            snapshot::update(snapshot, |s| {
                s.status = format!("setBreakpointByUrl failed: {err}");
            });
        }
    }
}

fn remove_line_bp(
    store: &mjx_wk_debug::BreakpointStore,
    loc: SourceLocation,
) -> mjx_wk_debug::BreakpointStore {
    let mut next = mjx_wk_debug::BreakpointStore::new();
    for bp in store.all() {
        if bp.spec.location.source == loc.source && bp.spec.location.line == loc.line {
            continue;
        }
        let index = next.insert(bp.spec.clone());
        if let Some(id) = bp.id.clone() {
            next.set_id(index, id);
        }
        match &bp.state {
            mjx_wk_debug::BreakpointState::Resolved { actual } => {
                if let Some(id) = next.all().get(index).and_then(|b| b.id.clone()) {
                    next.resolve(&id, *actual);
                }
            }
            mjx_wk_debug::BreakpointState::Failed { reason } => {
                next.fail(index, reason.clone());
            }
            mjx_wk_debug::BreakpointState::Disabled => {}
            mjx_wk_debug::BreakpointState::Pending => {}
        }
    }
    next
}

async fn remove_breakpoint(loc: SourceLocation, snapshot: &SharedSnapshot, state: &mut HostState) {
    toggle_breakpoint(loc, snapshot, state).await;
}

async fn step(kind: StepKind, snapshot: &SharedSnapshot, state: &mut HostState) {
    let Some(session) = &state.session else {
        return;
    };
    let result = match kind {
        StepKind::Over => session
            .call(debugger::commands::StepOver {})
            .await
            .map(|_| ()),
        StepKind::Into => session
            .call(debugger::commands::StepInto {})
            .await
            .map(|_| ()),
        StepKind::Out => session
            .call(debugger::commands::StepOut {})
            .await
            .map(|_| ()),
        StepKind::Next => session
            .call(debugger::commands::StepNext {})
            .await
            .map(|_| ()),
        StepKind::UntilNextRunLoop => session
            .call(debugger::commands::ContinueUntilNextRunLoop {})
            .await
            .map(|_| ()),
    };
    let _ = DebugStepKind::Over; // keep mapping documented
    snapshot::update(snapshot, |s| {
        s.status = match result {
            Ok(()) => format!("step {kind:?}"),
            Err(err) => format!("step failed: {err}"),
        };
        s.debug = state.debug.clone();
    });
}

async fn expand_value(
    node: ValueNodeId,
    start: u32,
    count: u32,
    snapshot: &SharedSnapshot,
    state: &mut HostState,
) {
    let Some(session) = state.session.clone() else {
        return;
    };
    let Some(snap) = state.debug.clone() else {
        return;
    };

    let mut model = (**snap.load()).clone();
    let Some(paused) = model.paused.as_mut() else {
        snapshot::update(snapshot, |s| s.status = "expand: not paused".into());
        return;
    };
    let Some(frame) = paused.call_frames.get_mut(paused.selected_frame) else {
        return;
    };

    // Ensure the selected frame has a ValueTree with scope roots.
    if frame.scopes.iter().all(|s| s.values.is_none()) {
        let mut tree = ValueTree::new();
        for scope in &frame.scopes {
            let name = scope
                .name
                .clone()
                .unwrap_or_else(|| format!("{:?}", scope.kind));
            let preview = ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: name.clone(),
                has_children: scope.object_id.is_some(),
            };
            tree.push_root(name, scope.object_id.clone(), preview);
        }
        if let Some(this_id) = frame.this_object_id.clone() {
            tree.push_root(
                "this",
                Some(this_id),
                ValuePreview {
                    type_name: "object".into(),
                    subtype: None,
                    description: "this".into(),
                    has_children: true,
                },
            );
        }
        if let Some(scope) = frame.scopes.first_mut() {
            scope.values = Some(tree);
        }
    }

    let Some(tree) = frame.scopes.iter_mut().find_map(|s| s.values.as_mut()) else {
        return;
    };

    let Some(object_id) = tree.get(node).and_then(|n| n.object_id.clone()) else {
        // Accessor invoke path is left for a later increment.
        snapshot::update(snapshot, |s| {
            s.status = format!("expand {node:?}: no objectId");
            s.debug = state.debug.clone();
        });
        return;
    };

    let page = if count == 0 { PAGE_SIZE } else { count };
    match session
        .call(runtime::commands::GetProperties {
            object_id: object_id.clone(),
            own_properties: Some(true),
            fetch_start: Some(i64::from(start)),
            fetch_count: Some(i64::from(page)),
            generate_preview: Some(true),
        })
        .await
    {
        Ok(ret) => {
            tree.apply_properties(
                node,
                start,
                page,
                &ret.properties,
                ret.internal_properties.as_deref().unwrap_or(&[]),
                Some(&object_id),
            );
            snap.store(Arc::new(model));
            snapshot::update(snapshot, |s| {
                s.status = format!("expanded {node:?}");
                s.debug = state.debug.clone();
            });
        }
        Err(err) => {
            snapshot::update(snapshot, |s| {
                s.status = format!("getProperties failed: {err}");
            });
        }
    }
}

async fn evaluate_expr(expr: &str, snapshot: &SharedSnapshot, state: &mut HostState) {
    let Some(session) = state.session.clone() else {
        return;
    };
    let target = state
        .debug
        .as_ref()
        .and_then(|d| d.load().paused.clone())
        .and_then(|p| p.current_frame().map(|f| f.id.clone()));
    let eval_target = match target.as_deref() {
        Some(id) => EvalTarget::CallFrame { call_frame_id: id },
        None => EvalTarget::Runtime,
    };

    match evaluate(&session, expr, eval_target).await {
        Ok(evaluation) => {
            if let Some(snap) = &state.console {
                let mut model = (**snap.load()).clone();
                model.record_evaluation(expr, &evaluation);
                snap.store(Arc::new(model));
            }
            snapshot::update(snapshot, |s| {
                s.status = format!(
                    "eval {}",
                    if evaluation.was_thrown { "threw" } else { "ok" }
                );
                s.console = state.console.clone();
            });
        }
        Err(err) => {
            snapshot::update(snapshot, |s| {
                s.status = format!("evaluate failed: {err}");
            });
        }
    }
}

async fn resolve_text(id: SourceId, state: &mut HostState) -> Option<Arc<SourceText>> {
    if let Some(cached) = state.store.cached(id) {
        return Some(cached);
    }
    let entry = state.inventory.get(id)?.clone();
    if let Some(session) = &state.session
        && let Ok(text) = state.store.text(session, &entry).await
    {
        return Some(text);
    }
    let body = fixture_seed::load_local_fixture_text(&entry.url)?;
    let text = Arc::new(SourceText::new(id, body));
    Some(text)
}
