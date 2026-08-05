//! Event fan-out — slow subscribers lag; typed subscribe matches methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_protocol::TargetType;
use mjx_wk_protocol::generated::debugger;
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
async fn a_slow_subscriber_lags_rather_than_stalling_the_socket() {
    // After the lagging subscriber exists, flood enough resumed events to
    // overflow the broadcast buffer. The session task must still complete
    // subsequent calls — lag drops, it never blocks the recv loop.
    let mut lines = vec![
        r#"{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}"#.to_owned(),
        r#"{"dir":"recv","frame":{"id":1,"result":{}}}"#.to_owned(),
        r#"{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}"#.to_owned(),
    ];
    for _ in 0..400 {
        lines
            .push(r#"{"dir":"recv","frame":{"method":"Debugger.resumed","params":{}}}"#.to_owned());
    }
    lines.push(r#"{"dir":"recv","frame":{"id":2,"result":{}}}"#.to_owned());
    lines.push(
        r#"{"dir":"send","frame":{"id":3,"method":"Debugger.disable","params":{}}}"#.to_owned(),
    );
    lines.push(r#"{"dir":"recv","frame":{"id":3,"result":{}}}"#.to_owned());
    let trace = lines.join("\n");

    let session = attach(&trace).await;
    let _lagging = session.subscribe::<debugger::events::Resumed>();

    session
        .call(debugger::commands::Enable {})
        .await
        .expect("enable must complete despite an unread subscriber");
    session
        .call(debugger::commands::Disable {})
        .await
        .expect("later call must not stall behind a slow subscriber");
}

#[tokio::test]
async fn typed_subscribe_receives_matching_events() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"method":"Debugger.resumed","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut sub = session.subscribe::<debugger::events::Resumed>();
    let (enable, ev) = tokio::join!(session.call(debugger::commands::Enable {}), sub.next());
    enable.expect("Debugger.enable");
    let ev = ev.expect("subscription closed");
    assert_eq!(ev, debugger::events::Resumed {});
}
