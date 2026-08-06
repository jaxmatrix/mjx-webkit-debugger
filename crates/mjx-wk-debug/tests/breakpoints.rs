//! DebugAgent — breakpoints against fixtures/breakpoint-hit.jsonl.
//!
//! **Owned by `docs/tasks/T-201-breakpoints.md`.**

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::Duration;

use mjx_wk_debug::{
    BreakpointId, BreakpointSpec, BreakpointState, BreakpointUrl, DEBUG_PANEL_REQUIRES, DebugAgent,
};
use mjx_wk_dialect::{CdpDialect, Dialect, DialectKind, NormalizedFrame, Support, WebKitDialect};
use mjx_wk_protocol::{Domain, Frame, TargetType};
use mjx_wk_session::{AgentRegistry, DomainAgent, Session, SessionHandle};
use mjx_wk_source::{SourceId, SourceLocation};
use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};
use serde_json::Value;

fn page_target(fixture: &str) -> Target {
    Target {
        key: TargetKey("test/page".into()),
        name: "fixture".into(),
        url: "http://127.0.0.1:8731/index.html".into(),
        kind: TargetType::WebPage,
        dialect: DialectKind::WebKitRwi,
        origin: TransportOrigin::Replay {
            fixture: fixture.into(),
        },
    }
}

async fn attach(trace: &str) -> SessionHandle {
    let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
    Session::attach(
        Box::new(transport),
        Box::new(WebKitDialect),
        page_target("inline"),
    )
    .await
    .expect("attach")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/breakpoint-hit.jsonl")
}

/// Unwrap Target.* multiplexing the way WebKitDialect does on the wire.
fn unwrap_inner(frame: &Value) -> Value {
    if let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) {
        return serde_json::from_str(message).expect("inner Target message JSON");
    }
    frame.clone()
}

fn loc(source: u32, line: u32, column: u32) -> SourceLocation {
    SourceLocation {
        source: SourceId(source),
        line,
        column,
    }
}

fn event_frame(method: &str, params: Value) -> NormalizedFrame {
    NormalizedFrame {
        frame: Frame::Event {
            method: method.into(),
            params,
        },
        target: None,
    }
}

const ENABLE_TRACE: &str = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
{"dir":"send","frame":{"id":3,"method":"Debugger.setBreakpointsActive","params":{"active":true}}}
{"dir":"recv","frame":{"id":3,"result":{}}}
"#;

const SET_TRACE: &str = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
{"dir":"send","frame":{"id":3,"method":"Debugger.setBreakpointsActive","params":{"active":true}}}
{"dir":"recv","frame":{"id":3,"result":{}}}
{"dir":"send","frame":{"id":4,"method":"Debugger.setBreakpointByUrl","params":{"lineNumber":3,"urlRegex":".*app\\.js","columnNumber":0}}}
{"dir":"recv","frame":{"id":4,"result":{"breakpointId":"/.*app\\.js/:3:0","locations":[{"scriptId":"2","lineNumber":3,"columnNumber":2}]}}}
"#;

#[tokio::test]
async fn attach_enables_debugger_and_activates_breakpoints() {
    let session = attach(ENABLE_TRACE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.expect("attach");
    let snap = agent.snapshot();
    assert!(snap.breakpoints_active);
    assert!(snap.disabled_reason.is_none());
}

#[tokio::test]
async fn set_by_url_pending_then_resolved_from_reply_locations() {
    let session = attach(SET_TRACE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.expect("attach");

    let index = agent
        .set_breakpoint_by_url(
            &session,
            BreakpointUrl::Regex(r".*app\.js".into()),
            BreakpointSpec::at(loc(1, 3, 0)),
        )
        .await
        .expect("set");

    let snap = agent.snapshot();
    let bp = &snap.breakpoints.all()[index];
    assert_eq!(
        bp.id.as_ref().map(|i| i.0.as_str()),
        Some(r"/.*app\.js/:3:0")
    );
    // Requested column 0; debuggee slid to column 2.
    assert_eq!(bp.spec.location.column, 0);
    assert_eq!(
        bp.state,
        BreakpointState::Resolved {
            actual: loc(1, 3, 2)
        }
    );
}

#[tokio::test]
async fn breakpoint_survives_reload_without_resend() {
    let session = attach(SET_TRACE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.expect("attach");
    agent
        .set_breakpoint_by_url(
            &session,
            BreakpointUrl::Regex(r".*app\.js".into()),
            BreakpointSpec::at(loc(1, 3, 0)),
        )
        .await
        .expect("set once");

    // Reload clears JS state but the logical breakpoint stays — no second set.
    agent
        .on_event(&event_frame(
            "Debugger.globalObjectCleared",
            Value::Object(Default::default()),
        ))
        .await
        .unwrap();
    agent
        .on_event(&event_frame(
            "Debugger.breakpointResolved",
            serde_json::json!({
                "breakpointId": r"/.*app\.js/:3:0",
                "location": {"scriptId": "6", "lineNumber": 3, "columnNumber": 2}
            }),
        ))
        .await
        .unwrap();

    let snap = agent.snapshot();
    assert_eq!(
        snap.breakpoints.all().len(),
        1,
        "must not re-insert on reload"
    );
    assert_eq!(
        snap.breakpoints.all()[0].state,
        BreakpointState::Resolved {
            actual: loc(1, 3, 2)
        }
    );
}

#[tokio::test]
async fn hit_count_tracks_paused_events() {
    let session = attach(SET_TRACE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.unwrap();
    agent
        .set_breakpoint_by_url(
            &session,
            BreakpointUrl::Regex(r".*app\.js".into()),
            BreakpointSpec::at(loc(1, 3, 0)),
        )
        .await
        .unwrap();

    let params = serde_json::json!({
        "callFrames": [],
        "reason": "Breakpoint",
        "data": {"breakpointId": r"/.*app\.js/:3:0"}
    });
    agent
        .on_event(&event_frame("Debugger.paused", params.clone()))
        .await
        .unwrap();
    agent
        .on_event(&event_frame("Debugger.paused", params))
        .await
        .unwrap();
    assert_eq!(agent.snapshot().breakpoints.all()[0].hit_count, 2);
}

#[tokio::test]
async fn detach_releases_object_groups() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
{"dir":"send","frame":{"id":3,"method":"Debugger.setBreakpointsActive","params":{"active":true}}}
{"dir":"recv","frame":{"id":3,"result":{}}}
{"dir":"send","frame":{"id":4,"method":"Runtime.releaseObjectGroup","params":{"objectGroup":"mjx-debug"}}}
{"dir":"recv","frame":{"id":4,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.unwrap();
    agent.detach(&session).await.expect("detach");
}

#[tokio::test]
async fn fixture_breakpoint_hit_events_drive_resolve_and_hits() {
    // Offline fold of the recorded multiplexed fixture — same events the agent
    // sees after dialect unwrap. Pins fixtures/breakpoint-hit.jsonl.
    let text = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let mut agent = DebugAgent::new();
    let index = agent
        .breakpoints_mut()
        .insert(BreakpointSpec::at(loc(1, 3, 0)));

    let mut saw_resolved = false;
    let mut pause_hits = 0u32;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(line).unwrap();
        let Some(frame) = entry.get("frame") else {
            continue;
        };
        let inner = unwrap_inner(frame);

        if let Some(result) = inner.get("result")
            && let Some(id) = result.get("breakpointId").and_then(Value::as_str)
        {
            agent
                .breakpoints_mut()
                .set_id(index, BreakpointId(id.into()));
            if let Some(locs) = result.get("locations").and_then(Value::as_array)
                && let Some(loc_v) = locs.first()
            {
                let line_n = loc_v["lineNumber"].as_u64().unwrap() as u32;
                let column = loc_v["columnNumber"].as_u64().unwrap_or(0) as u32;
                agent
                    .breakpoints_mut()
                    .resolve(&BreakpointId(id.into()), loc(1, line_n, column));
            }
            continue;
        }

        let Some(method) = inner.get("method").and_then(Value::as_str) else {
            continue;
        };
        if !method.starts_with("Debugger.") {
            continue;
        }
        let params = inner.get("params").cloned().unwrap_or(Value::Null);
        agent.on_event(&event_frame(method, params)).await.unwrap();
        if method == "Debugger.breakpointResolved" {
            saw_resolved = true;
        }
        if method == "Debugger.paused" {
            pause_hits += 1;
        }
    }

    assert!(
        saw_resolved,
        "fixture must emit breakpointResolved after reload"
    );
    assert!(pause_hits >= 2, "fixture pauses before and after reload");
    let snap = agent.snapshot();
    let bp = &snap.breakpoints.all()[index];
    assert!(
        matches!(bp.state, BreakpointState::Resolved { .. }),
        "survived reload: {bp:?}"
    );
    assert!(
        bp.hit_count >= 2,
        "hit_count from paused events: {}",
        bp.hit_count
    );
}

#[tokio::test]
async fn registry_publishes_snapshots_for_debug_agent() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
{"dir":"send","frame":{"id":3,"method":"Debugger.setBreakpointsActive","params":{"active":true}}}
{"dir":"recv","frame":{"id":3,"result":{}}}
{"dir":"recv","frame":{"method":"Debugger.breakpointResolved","params":{"breakpointId":"x","location":{"scriptId":"1","lineNumber":0,"columnNumber":0}}}}
"#;
    let session = attach(trace).await;
    let mut registry = AgentRegistry::new();
    let published = registry
        .register(DebugAgent::new(), &session)
        .await
        .expect("register")
        .expect("agent attached");

    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = published.load();
    assert!(registry.active().contains(&"debug"));
}

#[test]
fn panel_requires_lists_debugger_members() {
    assert!(
        DEBUG_PANEL_REQUIRES
            .iter()
            .any(|&(d, m)| d == Domain::Debugger && m == "setBreakpointByUrl")
    );
}

#[tokio::test]
async fn webkit_session_offers_set_breakpoint_by_url() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
    let session = attach(trace).await;
    assert!(
        session
            .supports(Domain::Debugger, "setBreakpointByUrl")
            .is_available()
    );
    assert!(DebugAgent::disabled_reason(&session).is_none());
    assert_eq!(
        DebugAgent::member_support(&session, "setPauseOnMicrotasks"),
        Support::Native
    );
}

#[test]
fn cdp_disables_webkit_only_members_with_reason() {
    // Capability table is live today; encode/decode are Phase 4.
    let cdp = CdpDialect;
    assert_eq!(
        cdp.supports(Domain::Debugger, "setBreakpointByUrl"),
        Support::Native,
        "line breakpoints exist on both engines"
    );
    assert_eq!(
        cdp.supports(Domain::Debugger, "setPauseOnMicrotasks"),
        Support::Unsupported,
        "panel must grey WebKit-only controls on CDP"
    );
    assert_eq!(
        cdp.supports(Domain::Debugger, "playBreakpointActionSound"),
        Support::Unsupported
    );
    assert!(!Support::Unsupported.is_available());
}
