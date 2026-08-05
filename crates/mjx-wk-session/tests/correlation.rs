//! Correlation and attach — done-criteria for request/response matching.
//!
//! Traces are inline `ReplayTransport::from_str` strings. The regenerable
//! corpus under `fixtures/` is owned by T-013 and is not required here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use mjx_wk_dialect::{DialectKind, WebKitDialect};
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

#[tokio::test]
async fn a_reply_reaches_the_caller_that_sent_the_request() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    session
        .call(debugger::commands::Enable {})
        .await
        .expect("Debugger.enable");
    assert!(session.is_connected());
    assert_eq!(session.target().url, "https://example.test/");
}

#[tokio::test]
async fn an_unsolicited_reply_is_logged_and_dropped() {
    // A response for an id nobody allocated must not panic the session task.
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"recv","frame":{"id":999,"result":{"surprise":true}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    session
        .call(debugger::commands::Enable {})
        .await
        .expect("still correlates after unsolicited drop");
}

#[tokio::test]
async fn attach_sends_only_inspector_enable() {
    // Trace covers Inspector.enable alone. A session that also enabled
    // Debugger/Network/... would fail replay with "not in trace".
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
    let session = attach(trace).await;
    assert!(session.is_connected());
    assert!(session.supports(Domain::Inspector, "enable").is_available());
}
