//! Background session task: attach or replay without blocking the UI thread.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_protocol::TargetType;
use mjx_wk_protocol::generated::debugger;
use mjx_wk_protocol::generated::page;
use mjx_wk_session::{AgentRegistry, Session, SessionHandle};
use mjx_wk_source::{SourceId, SourceInventory, SourceStore, SourceText};
use mjx_wk_transport::{
    Discovery, ReplayTransport, Target, TargetKey, TcpInspectorServer, TransportOrigin,
};
use mjx_wk_ui::Action;
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
    Replay { fixture: PathBuf },
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

    // Always seed from the fixture so the tree is populated even when
    // multiplexed Session::attach cannot drive the trace.
    let seed_note = match fixture_seed::seed_inventory_from_fixture(&fixture, &mut inventory) {
        Ok((scripts, tree)) => format!(
            "Seeded inventory from fixture: {scripts} scriptParsed, resource tree={tree}."
        ),
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

    let transport = match ReplayTransport::from_file(&fixture) {
        Ok(t) => t,
        Err(err) => {
            snapshot::update(&snapshot, |s| {
                s.status = "replay load failed".into();
                s.notes.push(format!("{err:#}"));
            });
            drain_until_shutdown(rx).await;
            return;
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
            let registry = AgentRegistry::new();
            // Enable domains the inventory needs; ignore failures on replay.
            let _ = session.call(debugger::commands::Enable {}).await;
            let _ = session.call(page::commands::Enable {}).await;
            if let Ok(tree) = session.call(page::commands::GetResourceTree {}).await {
                inventory.on_resource_tree(&tree.frame_tree);
            }

            snapshot::publish(
                &snapshot,
                ShellSnapshot {
                    status: format!("replay attached: {path_display}"),
                    notes: vec![seed_note],
                    active_agents: registry.active(),
                    connected: session.is_connected(),
                    tree: inventory.tree(),
                    selected: None,
                    selected_text: None,
                },
            );

            let state = HostState {
                inventory,
                store,
                session: Some(session),
                _registry: registry,
                selected: None,
            };
            // Drain scriptParsed on the same task as actions.
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
                             Wave-2 page-target routing for Inspector.enable; the tree below is \
                             seeded from the fixture."
                        ),
                    ],
                    connected: false,
                    tree: inventory.tree(),
                    selected: None,
                    selected_text: None,
                    active_agents: Vec::new(),
                },
            );
            let state = HostState {
                inventory,
                store,
                session: None,
                _registry: AgentRegistry::new(),
                selected: None,
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
            s.status = format!("target index {index} out of range ({} found)", targets.len());
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
            let registry = AgentRegistry::new();

            let _ = session.call(debugger::commands::Enable {}).await;
            let _ = session.call(page::commands::Enable {}).await;
            if let Ok(tree) = session.call(page::commands::GetResourceTree {}).await {
                inventory.on_resource_tree(&tree.frame_tree);
            }

            snapshot::publish(
                &snapshot,
                ShellSnapshot {
                    status: format!("attached to {} ({})", target.name, target.url),
                    notes: Vec::new(),
                    active_agents: registry.active(),
                    connected: true,
                    tree: inventory.tree(),
                    selected: None,
                    selected_text: None,
                },
            );

            let state = HostState {
                inventory,
                store,
                session: Some(session),
                _registry: registry,
                selected: None,
            };
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

struct HostState {
    inventory: SourceInventory,
    store: SourceStore,
    session: Option<SessionHandle>,
    _registry: AgentRegistry,
    selected: Option<SourceId>,
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
                snapshot::update(snapshot, |s| {
                    s.tree = state.inventory.tree();
                });
            }
        }
    }
}

async fn handle_action(
    action: Action,
    snapshot: &SharedSnapshot,
    state: &mut HostState,
) {
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
        other => {
            tracing::debug!(?other, "action ignored in scaffold host");
            snapshot::update(snapshot, |s| {
                s.status = format!("action: {other:?}");
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
    // Offline replay fallback: read fixtures/page/* by URL.
    let body = fixture_seed::load_local_fixture_text(&entry.url)?;
    let text = Arc::new(SourceText::new(id, body));
    Some(text)
}
