//! Non-line breakpoints — `docs/tasks/T-206-dom-debugger-breakpoints.md`.
//!
//! Covers DOM / event / URL / symbolic set+remove, instrumentation pause
//! detail, node-removed cleanup, fixture pins, and CDP support gating.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_debug::{
    DOM_DEBUGGER_MEMBERS, DebugAgent, DomBreakpoint, DomBreakpointKind, EventBreakpoint,
    PauseReason, SymbolicBreakpoint, UrlBreakpoint,
};
use mjx_wk_dialect::{CdpDialect, Dialect, DialectKind, NormalizedFrame, Support, WebKitDialect};
use mjx_wk_protocol::{Domain, Frame, TargetType};
use mjx_wk_session::{DomainAgent, Session, SessionHandle};
use mjx_wk_source::NodeId;
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

fn event_frame(method: &str, params: Value) -> NormalizedFrame {
    NormalizedFrame {
        frame: Frame::Event {
            method: method.into(),
            params,
        },
        target: None,
    }
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/breakpoint-hit.jsonl")
}

fn unwrap_inner(frame: &Value) -> Value {
    if let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) {
        return serde_json::from_str(message).expect("inner Target message JSON");
    }
    frame.clone()
}

const BASE_ENABLE: &str = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
{"dir":"send","frame":{"id":3,"method":"Debugger.setBreakpointsActive","params":{"active":true}}}
{"dir":"recv","frame":{"id":3,"result":{}}}
"#;

#[tokio::test]
async fn set_and_remove_each_non_line_kind() {
    let trace = format!(
        r#"{BASE_ENABLE}
{{"dir":"send","frame":{{"id":4,"method":"DOMDebugger.setDOMBreakpoint","params":{{"nodeId":42,"type":"subtree-modified"}}}}}}
{{"dir":"recv","frame":{{"id":4,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":5,"method":"DOMDebugger.removeDOMBreakpoint","params":{{"nodeId":42,"type":"subtree-modified"}}}}}}
{{"dir":"recv","frame":{{"id":5,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":6,"method":"DOMDebugger.setEventBreakpoint","params":{{"breakpointType":"listener","eventName":"click"}}}}}}
{{"dir":"recv","frame":{{"id":6,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":7,"method":"DOMDebugger.removeEventBreakpoint","params":{{"breakpointType":"listener","eventName":"click"}}}}}}
{{"dir":"recv","frame":{{"id":7,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":8,"method":"DOMDebugger.setURLBreakpoint","params":{{"url":"api/","isRegex":false}}}}}}
{{"dir":"recv","frame":{{"id":8,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":9,"method":"DOMDebugger.removeURLBreakpoint","params":{{"url":"api/","isRegex":false}}}}}}
{{"dir":"recv","frame":{{"id":9,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":10,"method":"Debugger.addSymbolicBreakpoint","params":{{"symbol":"computeTotal","caseSensitive":true,"isRegex":false}}}}}}
{{"dir":"recv","frame":{{"id":10,"result":{{}}}}}}
{{"dir":"send","frame":{{"id":11,"method":"Debugger.removeSymbolicBreakpoint","params":{{"symbol":"computeTotal","caseSensitive":true,"isRegex":false}}}}}}
{{"dir":"recv","frame":{{"id":11,"result":{{}}}}}}
"#
    );
    let session = attach(&trace).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.expect("attach");

    let dom = DomBreakpoint {
        node: NodeId(42),
        kind: DomBreakpointKind::SubtreeModified,
    };
    agent
        .set_dom_breakpoint(&session, dom.clone())
        .await
        .expect("set DOM");
    assert_eq!(agent.snapshot().breakpoints.dom().len(), 1);
    agent
        .remove_dom_breakpoint(&session, &dom)
        .await
        .expect("remove DOM");
    assert!(agent.snapshot().breakpoints.dom().is_empty());

    let event = EventBreakpoint {
        category: "listener".into(),
        name: Some("click".into()),
    };
    agent
        .set_event_breakpoint(&session, event.clone())
        .await
        .expect("set event");
    agent
        .remove_event_breakpoint(&session, &event)
        .await
        .expect("remove event");
    assert!(agent.snapshot().breakpoints.event().is_empty());

    let url = UrlBreakpoint {
        pattern: "api/".into(),
        is_regex: false,
    };
    agent
        .set_url_breakpoint(&session, url.clone())
        .await
        .expect("set URL");
    agent
        .remove_url_breakpoint(&session, &url)
        .await
        .expect("remove URL");
    assert!(agent.snapshot().breakpoints.url().is_empty());

    let sym = SymbolicBreakpoint {
        symbol: "computeTotal".into(),
        case_sensitive: true,
        is_regex: false,
    };
    agent
        .add_symbolic_breakpoint(&session, sym.clone())
        .await
        .expect("add symbolic");
    agent
        .remove_symbolic_breakpoint(&session, &sym)
        .await
        .expect("remove symbolic");
    assert!(agent.snapshot().breakpoints.symbolic().is_empty());
}

#[tokio::test]
async fn pause_reports_instrumentation_with_which_breakpoint() {
    let session = attach(BASE_ENABLE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.unwrap();

    // Seed store so symbolic matching can name the function.
    agent.breakpoints_mut().insert_symbolic(SymbolicBreakpoint {
        symbol: "computeTotal".into(),
        case_sensitive: true,
        is_regex: false,
    });
    agent.breakpoints_mut().insert_dom(DomBreakpoint {
        node: NodeId(9),
        kind: DomBreakpointKind::NodeRemoved,
    });

    agent
        .on_event(&event_frame(
            "Debugger.paused",
            serde_json::json!({
                "callFrames": [],
                "reason": "DOM",
                "data": {"type": "subtree-modified", "nodeId": 9}
            }),
        ))
        .await
        .unwrap();
    let paused = agent.snapshot().paused.clone().expect("paused");
    assert_eq!(
        paused.reason,
        PauseReason::Instrumentation {
            detail: "DOM subtree-modified on node 9".into()
        }
    );

    agent
        .on_event(&event_frame(
            "Debugger.paused",
            serde_json::json!({
                "callFrames": [],
                "reason": "Listener",
                "data": {"eventName": "click"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        agent.snapshot().paused.as_ref().unwrap().reason,
        PauseReason::Instrumentation {
            detail: "Listener click".into()
        }
    );

    agent
        .on_event(&event_frame(
            "Debugger.paused",
            serde_json::json!({
                "callFrames": [],
                "reason": "URL",
                "data": {"breakpointURL": "api/", "url": "https://example/api/1"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        agent.snapshot().paused.as_ref().unwrap().reason,
        PauseReason::Instrumentation {
            detail: "URL matching api/".into()
        }
    );

    // WebKit fires symbolic as FunctionCall + data.name.
    agent
        .on_event(&event_frame(
            "Debugger.paused",
            serde_json::json!({
                "callFrames": [],
                "reason": "FunctionCall",
                "data": {"name": "computeTotal"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        agent.snapshot().paused.as_ref().unwrap().reason,
        PauseReason::Instrumentation {
            detail: "Symbolic computeTotal".into()
        }
    );
}

#[tokio::test]
async fn node_removed_cleans_dangling_dom_breakpoint() {
    let session = attach(BASE_ENABLE).await;
    let mut agent = DebugAgent::new();
    agent.attach(&session).await.unwrap();
    agent.breakpoints_mut().insert_dom(DomBreakpoint {
        node: NodeId(42),
        kind: DomBreakpointKind::NodeRemoved,
    });
    agent.breakpoints_mut().insert_dom(DomBreakpoint {
        node: NodeId(42),
        kind: DomBreakpointKind::SubtreeModified,
    });
    agent.breakpoints_mut().insert_dom(DomBreakpoint {
        node: NodeId(7),
        kind: DomBreakpointKind::AttributeModified,
    });

    agent
        .on_event(&event_frame(
            "Debugger.paused",
            serde_json::json!({
                "callFrames": [],
                "reason": "DOM",
                "data": {"type": "node-removed", "nodeId": 42}
            }),
        ))
        .await
        .unwrap();

    let snap = agent.snapshot();
    assert_eq!(snap.breakpoints.dom().len(), 1);
    assert_eq!(snap.breakpoints.dom()[0].node, NodeId(7));
    assert!(matches!(
        snap.paused.as_ref().unwrap().reason,
        PauseReason::Instrumentation { .. }
    ));
}

#[tokio::test]
async fn fixture_includes_dom_and_event_breakpoint_traffic() {
    let text = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let mut saw_dom_set = false;
    let mut saw_event_set = false;
    let mut saw_dom_pause = false;
    let mut saw_listener_pause = false;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(line).unwrap();
        let Some(frame) = entry.get("frame") else {
            continue;
        };
        let inner = unwrap_inner(frame);
        let method = inner.get("method").and_then(Value::as_str).unwrap_or("");
        match method {
            "DOMDebugger.setDOMBreakpoint" => saw_dom_set = true,
            "DOMDebugger.setEventBreakpoint" => saw_event_set = true,
            "Debugger.paused" => {
                let reason = inner
                    .pointer("/params/reason")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if reason == "DOM" {
                    saw_dom_pause = true;
                }
                if reason == "Listener" {
                    saw_listener_pause = true;
                }
            }
            _ => {}
        }
    }

    assert!(saw_dom_set, "fixture must set a DOM breakpoint");
    assert!(saw_event_set, "fixture must set an event breakpoint");
    assert!(saw_dom_pause, "fixture must pause for a DOM breakpoint");
    assert!(
        saw_listener_pause,
        "fixture must pause for an event breakpoint"
    );
}

#[tokio::test]
async fn fixture_dom_event_pauses_drive_agent_instrumentation() {
    let text = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let mut agent = DebugAgent::new();
    // Store entries so symbolic-style naming still works if present; DOM/event
    // detail comes from pause data alone.
    agent.breakpoints_mut().insert_dom(DomBreakpoint {
        node: NodeId(1),
        kind: DomBreakpointKind::SubtreeModified,
    });

    let mut instrumentation_hits = 0u32;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(line).unwrap();
        let Some(frame) = entry.get("frame") else {
            continue;
        };
        let inner = unwrap_inner(frame);
        let Some(method) = inner.get("method").and_then(Value::as_str) else {
            continue;
        };
        if method != "Debugger.paused" {
            continue;
        }
        let params = inner.get("params").cloned().unwrap_or(Value::Null);
        let reason = params.get("reason").and_then(Value::as_str).unwrap_or("");
        if reason != "DOM" && reason != "Listener" {
            continue;
        }
        agent.on_event(&event_frame(method, params)).await.unwrap();
        let paused = agent.snapshot().paused.clone().expect("paused");
        assert!(
            matches!(paused.reason, PauseReason::Instrumentation { .. }),
            "expected Instrumentation, got {:?}",
            paused.reason
        );
        instrumentation_hits += 1;
    }
    assert!(
        instrumentation_hits >= 2,
        "fixture must drive at least DOM + Listener instrumentation pauses"
    );
}

#[tokio::test]
async fn webkit_offers_dom_debugger_and_symbolic() {
    let session = attach(
        r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#,
    )
    .await;
    for &(domain, member) in DOM_DEBUGGER_MEMBERS {
        assert!(
            session.supports(domain, member).is_available(),
            "{}.{member} should be available on WebKit",
            domain.as_str()
        );
        assert!(DebugAgent::non_line_disabled_reason(&session, domain, member).is_none());
    }
}

#[test]
fn cdp_disables_symbolic_with_reason_keeps_dom_debugger() {
    let cdp = CdpDialect;
    assert_eq!(
        cdp.supports(Domain::DomDebugger, "setDOMBreakpoint"),
        Support::Native,
        "DOM change breakpoints exist on both engines"
    );
    assert_eq!(
        cdp.supports(Domain::DomDebugger, "setEventBreakpoint"),
        Support::Native
    );
    assert_eq!(
        cdp.supports(Domain::DomDebugger, "setURLBreakpoint"),
        Support::Native
    );
    assert_eq!(
        cdp.supports(Domain::Debugger, "addSymbolicBreakpoint"),
        Support::Unsupported,
        "panel must grey symbolic breakpoints on CDP"
    );
    assert!(!Support::Unsupported.is_available());
}
