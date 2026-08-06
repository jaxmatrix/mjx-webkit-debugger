//! ConsoleAgent — attach, message fold, repeats, evaluate routing.
//!
//! **Owned by `docs/tasks/T-204-console.md`.**

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_console::{
    ConsoleAgent, EvalTarget, MESSAGE_CAPACITY, MessageLevel, MessageSource, evaluate,
};
use mjx_wk_dialect::{DialectKind, NormalizedFrame, WebKitDialect};
use mjx_wk_protocol::{Frame, TargetType};
use mjx_wk_session::{DomainAgent, Session, SessionHandle};
use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};
use serde_json::Value;

fn page_target() -> Target {
    Target {
        key: TargetKey("test/page".into()),
        name: "fixture".into(),
        url: "https://example.test/".into(),
        kind: TargetType::WebPage,
        dialect: DialectKind::WebKitRwi,
        origin: TransportOrigin::Replay {
            fixture: "inline".into(),
        },
    }
}

async fn attach(trace: &str) -> SessionHandle {
    let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
    Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach")
}

fn event_frame(method: &str, params: Value) -> NormalizedFrame {
    NormalizedFrame {
        frame: Frame::Event {
            method: method.to_owned(),
            params,
        },
        target: None,
    }
}

fn attach_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/attach.jsonl")
}

/// Pull demuxed `Console.*` events out of the recorded attach trace.
fn console_events_from_attach_fixture() -> Vec<NormalizedFrame> {
    let text = std::fs::read_to_string(attach_fixture_path()).expect("read attach.jsonl");
    let mut events = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).expect("jsonl row");
        let frame = &row["frame"];
        if frame["method"] != "Target.dispatchMessageFromTarget" {
            continue;
        }
        let message = frame["params"]["message"]
            .as_str()
            .expect("dispatch message string");
        let inner: Value = serde_json::from_str(message).expect("inner frame");
        let Some(method) = inner.get("method").and_then(Value::as_str) else {
            continue;
        };
        if !method.starts_with("Console.") {
            continue;
        }
        let params = inner.get("params").cloned().unwrap_or(Value::Null);
        events.push(event_frame(method, params));
    }
    events
}

#[tokio::test]
async fn attach_enables_console_and_snapshot_is_empty() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Console.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut agent = ConsoleAgent::new();
    agent.attach(&session).await.expect("Console.enable");
    let snap = agent.snapshot();
    assert!(snap.messages.is_empty());
    assert_eq!(snap.dropped, 0);
}

#[tokio::test]
async fn message_added_folds_text_level_and_object_ids() {
    let mut agent = ConsoleAgent::new();
    let params = serde_json::json!({
        "message": {
            "source": "console-api",
            "level": "warning",
            "text": "obj",
            "type": "log",
            "repeatCount": 1,
            "parameters": [
                {"type": "string", "value": "hello"},
                {
                    "type": "object",
                    "className": "Object",
                    "description": "Object",
                    "objectId": "0.1"
                }
            ]
        }
    });
    agent
        .on_event(&event_frame("Console.messageAdded", params))
        .await
        .expect("fold");
    let snap = agent.snapshot();
    assert_eq!(snap.messages.len(), 1);
    let m = &snap.messages[0];
    assert_eq!(m.source, MessageSource::ConsoleApi);
    assert_eq!(m.level, MessageLevel::Warning);
    assert_eq!(m.text, "hello Object");
    assert_eq!(m.argument_object_ids, vec!["0.1".to_owned()]);
    assert_eq!(m.repeat_count, 1);
}

#[tokio::test]
async fn message_repeat_count_updated_folds_rather_than_flooding() {
    let mut agent = ConsoleAgent::new();
    let added = serde_json::json!({
        "message": {
            "source": "console-api",
            "level": "log",
            "text": "tick",
            "type": "log",
            "repeatCount": 1
        }
    });
    agent
        .on_event(&event_frame("Console.messageAdded", added))
        .await
        .unwrap();
    agent
        .on_event(&event_frame(
            "Console.messageRepeatCountUpdated",
            serde_json::json!({ "count": 17 }),
        ))
        .await
        .unwrap();
    let snap = agent.snapshot();
    assert_eq!(snap.messages.len(), 1);
    assert_eq!(snap.messages[0].repeat_count, 17);
}

#[tokio::test]
async fn messages_cleared_empties_the_log() {
    let mut agent = ConsoleAgent::new();
    agent
        .on_event(&event_frame(
            "Console.messageAdded",
            serde_json::json!({
                "message": {
                    "source": "network",
                    "level": "error",
                    "text": "404",
                    "type": "log"
                }
            }),
        ))
        .await
        .unwrap();
    agent
        .on_event(&event_frame(
            "Console.messagesCleared",
            serde_json::json!({ "reason": "console-api" }),
        ))
        .await
        .unwrap();
    assert!(agent.snapshot().messages.is_empty());
}

#[tokio::test]
async fn attach_fixture_console_messages_fold() {
    let events = console_events_from_attach_fixture();
    assert!(
        events
            .iter()
            .any(|e| e.frame.method() == Some("Console.messageAdded")),
        "attach.jsonl must carry Console.messageAdded"
    );

    let mut agent = ConsoleAgent::new();
    for event in &events {
        agent.on_event(event).await.expect("fold fixture event");
    }
    let snap = agent.snapshot();
    assert!(
        snap.messages.len() >= 2,
        "expected fixture console.log rows, got {}",
        snap.messages.len()
    );
    assert!(
        snap.messages
            .iter()
            .any(|m| m.text.contains("fixture ready") || m.text.contains("56")),
        "missing 'fixture ready' log: {:?}",
        snap.messages.iter().map(|m| &m.text).collect::<Vec<_>>()
    );
    assert!(
        snap.messages
            .iter()
            .any(|m| m.level == MessageLevel::Error && m.source == MessageSource::Network),
        "missing favicon 404 network error"
    );
    assert!(snap.messages.len() <= MESSAGE_CAPACITY);
}

#[tokio::test]
async fn evaluate_while_running_uses_runtime_evaluate() {
    // Session allocates id 1 = Inspector.enable, 2 = Runtime.evaluate.
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Runtime.evaluate","params":{"expression":"1+1","objectGroup":"console","includeCommandLineAPI":true,"generatePreview":true,"saveResult":true}}}
{"dir":"recv","frame":{"id":2,"result":{"result":{"type":"number","value":2,"description":"2"},"wasThrown":false}}}
"#;
    let session = attach(trace).await;
    let evaluation = evaluate(&session, "1+1", EvalTarget::Runtime)
        .await
        .expect("Runtime.evaluate");
    assert_eq!(evaluation.text, "2");
    assert!(!evaluation.was_thrown);
    assert!(evaluation.object_id.is_none());
}

#[tokio::test]
async fn evaluate_while_paused_uses_evaluate_on_call_frame() {
    // Session allocates id 1 = Inspector.enable, 2 = Debugger.evaluateOnCallFrame.
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.evaluateOnCallFrame","params":{"callFrameId":"frame-0","expression":"localVar","objectGroup":"console","includeCommandLineAPI":true,"generatePreview":true,"saveResult":true}}}
{"dir":"recv","frame":{"id":2,"result":{"result":{"type":"number","value":56,"description":"56"},"wasThrown":false}}}
"#;
    let session = attach(trace).await;
    let evaluation = evaluate(
        &session,
        "localVar",
        EvalTarget::CallFrame {
            call_frame_id: "frame-0",
        },
    )
    .await
    .expect("evaluateOnCallFrame");
    assert_eq!(evaluation.text, "56");
    assert!(!evaluation.was_thrown);
}

#[tokio::test]
async fn evaluate_and_record_appends_input_and_output_rows() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Runtime.evaluate","params":{"expression":"\"hi\"","objectGroup":"console","includeCommandLineAPI":true,"generatePreview":true,"saveResult":true}}}
{"dir":"recv","frame":{"id":2,"result":{"result":{"type":"string","value":"hi"},"wasThrown":false}}}
"#;
    let session = attach(trace).await;
    let mut agent = ConsoleAgent::new();
    agent
        .evaluate_and_record(&session, "\"hi\"", EvalTarget::Runtime)
        .await
        .expect("eval");
    let snap = agent.snapshot();
    assert_eq!(snap.messages.len(), 2);
    assert_eq!(snap.messages[0].text, "> \"hi\"");
    assert_eq!(snap.messages[1].text, "hi");
}
