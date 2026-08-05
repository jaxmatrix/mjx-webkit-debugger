//! Target demux — dispatchMessageFromTarget attribution and routed calls.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use mjx_wk_dialect::{DialectKind, TargetId, WebKitDialect};
use mjx_wk_protocol::generated::debugger;
use mjx_wk_protocol::{Domain, TargetType};
use mjx_wk_session::Session;
use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};

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

async fn attach(trace: &str) -> mjx_wk_session::SessionHandle {
    let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
    Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach")
}

fn join_trace(lines: &[&str]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    out.push('\n');
    out
}

#[tokio::test]
async fn dispatch_message_from_target_reaches_subscribers_with_attribution() {
    let message = serde_json::to_string(r#"{"method":"Debugger.resumed","params":{}}"#).unwrap();
    let dispatch = format!(
        r#"{{"dir":"recv","frame":{{"method":"Target.dispatchMessageFromTarget","params":{{"targetId":"worker-1","message":{message}}}}}}}"#
    );
    let trace = join_trace(&[
        r#"{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}"#,
        r#"{"dir":"recv","frame":{"id":1,"result":{}}}"#,
        r#"{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}"#,
        r#"{"dir":"recv","frame":{"method":"Target.targetCreated","params":{"targetInfo":{"targetId":"worker-1","type":"worker"}}}}"#,
        &dispatch,
        r#"{"dir":"recv","frame":{"id":2,"result":{}}}"#,
    ]);

    let session = attach(&trace).await;
    let mut sub = session.subscribe_domain(Domain::Debugger);

    // Drive the trace forward only after the subscriber exists, so the demuxed
    // event is not broadcast into an empty fan-out. Poll call and subscribe
    // concurrently — awaiting subscribe first would deadlock (no send, no event).
    let (enable, frame) = tokio::join!(session.call(debugger::commands::Enable {}), sub.next());
    enable.expect("Debugger.enable");
    let frame = frame.expect("domain subscription closed early");

    assert_eq!(frame.target, Some(TargetId("worker-1".into())));
    assert_eq!(frame.frame.method(), Some("Debugger.resumed"));
    assert_eq!(session.sub_targets(), vec![TargetId("worker-1".into())]);
}

#[tokio::test]
async fn call_on_routes_through_target_send_message_to_target() {
    // Outer send is Target.sendMessageToTarget; outer Empty ack must not
    // complete the call — the real reply is the unwrapped inner response.
    //
    // Request ids in this trace are chosen to match what the session allocates
    // (1=Inspector.enable, 2=Debugger.enable, 3=routed resume). Replay does
    // not rewrite ids inside the dispatch `message` string, so the inner reply
    // id must already equal the session's correlation id.
    let inner_req =
        serde_json::to_string(r#"{"id":3,"method":"Debugger.resume","params":{}}"#).unwrap();
    let inner_reply = serde_json::to_string(r#"{"id":3,"result":{}}"#).unwrap();
    let send_to_target = format!(
        r#"{{"dir":"send","frame":{{"id":3,"method":"Target.sendMessageToTarget","params":{{"targetId":"w1","message":{inner_req}}}}}}}"#
    );
    let dispatch = format!(
        r#"{{"dir":"recv","frame":{{"method":"Target.dispatchMessageFromTarget","params":{{"targetId":"w1","message":{inner_reply}}}}}}}"#
    );
    let trace = join_trace(&[
        r#"{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}"#,
        r#"{"dir":"recv","frame":{"id":1,"result":{}}}"#,
        r#"{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}"#,
        r#"{"dir":"recv","frame":{"method":"Target.targetCreated","params":{"targetInfo":{"targetId":"w1","type":"worker"}}}}"#,
        r#"{"dir":"recv","frame":{"id":2,"result":{}}}"#,
        &send_to_target,
        r#"{"dir":"recv","frame":{"id":3,"result":{}}}"#,
        &dispatch,
    ]);

    let session = attach(&trace).await;
    session
        .call(debugger::commands::Enable {})
        .await
        .expect("Debugger.enable");
    assert_eq!(session.sub_targets(), vec![TargetId("w1".into())]);

    session
        .call_on(&TargetId("w1".into()), debugger::commands::Resume {})
        .await
        .expect("routed resume");
}
