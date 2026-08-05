//! Capability gating — MethodNotFound teaches; later calls skip the wire.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mjx_wk_dialect::{DialectKind, Support, WebKitDialect};
use mjx_wk_protocol::generated::debugger;
use mjx_wk_protocol::{Domain, TargetType};
use mjx_wk_session::{Session, SessionError, UnsupportedReason};
use mjx_wk_transport::{
    ReplayTransport, Target, TargetKey, Transport, TransportError, TransportOrigin,
};

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

/// Counts every `send` so gating tests can prove the wire was never touched.
#[derive(Debug)]
struct CountingTransport {
    inner: ReplayTransport,
    sends: Arc<AtomicUsize>,
}

#[async_trait]
impl Transport for CountingTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        self.inner.send(text).await
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        self.inner.recv().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }

    fn dialect(&self) -> DialectKind {
        self.inner.dialect()
    }
}

#[tokio::test]
async fn unsupported_member_returns_without_touching_the_wire() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.setPauseOnMicrotasks","params":{"enabled":true}}}
{"dir":"recv","frame":{"id":2,"error":{"code":-32601,"message":"'Debugger.setPauseOnMicrotasks' was not found"}}}
"#;
    let sends = Arc::new(AtomicUsize::new(0));
    let inner = ReplayTransport::from_str(trace, "gating").unwrap();
    let transport = CountingTransport {
        inner,
        sends: Arc::clone(&sends),
    };
    let session = Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .unwrap();

    // First call hits the wire, learns MethodNotFound.
    let err = session
        .call(debugger::commands::SetPauseOnMicrotasks {
            enabled: true,
            options: None,
        })
        .await
        .expect_err("method missing");
    assert!(matches!(err, SessionError::Protocol(_)));

    let after_first = sends.load(Ordering::SeqCst);

    // Second call must short-circuit with Unsupported and zero additional sends.
    let err = session
        .call(debugger::commands::SetPauseOnMicrotasks {
            enabled: true,
            options: None,
        })
        .await
        .expect_err("known absent");
    match err {
        SessionError::Unsupported {
            domain,
            member,
            reason,
        } => {
            assert_eq!(domain, Domain::Debugger);
            assert_eq!(member, "setPauseOnMicrotasks");
            assert_eq!(reason, UnsupportedReason::DebuggeeBuild);
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(sends.load(Ordering::SeqCst), after_first);
    assert_eq!(
        session.supports(Domain::Debugger, "setPauseOnMicrotasks"),
        Support::Unsupported
    );
}

#[tokio::test]
async fn method_not_found_teaches_capabilities_for_the_next_call() {
    // Same knowledge path as the gating test, asserted from the caller's view:
    // first Protocol, second Unsupported without needing a second trace send.
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.setPauseOnMicrotasks","params":{"enabled":true}}}
{"dir":"recv","frame":{"id":2,"error":{"code":-32601,"message":"not found"}}}
"#;
    let session = attach(trace).await;

    let first = session
        .call(debugger::commands::SetPauseOnMicrotasks {
            enabled: true,
            options: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(first, SessionError::Protocol(ref e) if e.is_unsupported()));

    let second = session
        .call(debugger::commands::SetPauseOnMicrotasks {
            enabled: true,
            options: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(second, SessionError::Unsupported { .. }));
}
