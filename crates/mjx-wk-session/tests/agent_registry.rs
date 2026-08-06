//! AgentRegistry — domain gating, attach skip, and event fan-in.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mjx_wk_dialect::{DialectKind, NormalizedFrame, WebKitDialect};
use mjx_wk_protocol::{Domain, TargetType};
use mjx_wk_session::{AgentRegistry, DomainAgent, Session, SessionError, SessionHandle};
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

async fn attach(trace: &str) -> SessionHandle {
    let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
    Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach")
}

#[derive(Debug)]
struct CountingAgent {
    events: Arc<AtomicUsize>,
}

#[async_trait]
impl DomainAgent for CountingAgent {
    type Model = ();

    const DOMAINS: &'static [Domain] = &[Domain::Debugger];
    const NAME: &'static str = "counting";

    async fn attach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        session
            .call(mjx_wk_protocol::generated::debugger::commands::Enable {})
            .await?;
        Ok(())
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        self.events.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(())
    }
}

#[derive(Debug, Default)]
struct CanvasOnlyAgent;

#[async_trait]
impl DomainAgent for CanvasOnlyAgent {
    type Model = ();

    const DOMAINS: &'static [Domain] = &[Domain::Canvas];
    const NAME: &'static str = "canvas-only";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        panic!("attach must not run when no domains are available");
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(())
    }
}

#[derive(Debug, Default)]
struct FailingAttachAgent;

#[async_trait]
impl DomainAgent for FailingAttachAgent {
    type Model = ();

    const DOMAINS: &'static [Domain] = &[Domain::Runtime];
    const NAME: &'static str = "failing-attach";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        Err(SessionError::Closed)
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(())
    }
}

#[tokio::test]
async fn register_attaches_and_lists_the_agent() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut registry = AgentRegistry::new();
    let events = Arc::new(AtomicUsize::new(0));

    let snap = registry
        .register(
            CountingAgent {
                events: Arc::clone(&events),
            },
            &session,
        )
        .await
        .expect("register")
        .expect("agent attached");

    assert_eq!(registry.active(), vec!["counting"]);
    let _ = snap.load();
}

#[tokio::test]
async fn register_skips_when_no_domain_is_available() {
    // Teach the session that Canvas.enable is absent, then register an agent
    // that only needs Canvas — it must be skipped without calling attach.
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Canvas.enable","params":{}}}
{"dir":"recv","frame":{"id":2,"error":{"code":-32601,"message":"'Canvas.enable' was not found"}}}
"#;
    let session = attach(trace).await;
    let err = session
        .call(mjx_wk_protocol::generated::canvas::commands::Enable {})
        .await;
    assert!(err.is_err());
    assert!(!session.supports(Domain::Canvas, "enable").is_available());

    let mut registry = AgentRegistry::new();
    let snap = registry
        .register(CanvasOnlyAgent, &session)
        .await
        .expect("register returns Ok on skip");
    assert!(snap.is_none());
    assert!(registry.active().is_empty());
}

#[tokio::test]
async fn register_skips_attach_failure_without_failing_the_registry() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut registry = AgentRegistry::new();
    let snap = registry
        .register(FailingAttachAgent, &session)
        .await
        .expect("attach failure is skipped");
    assert!(snap.is_none());
    assert!(registry.active().is_empty());
}

#[tokio::test]
async fn registered_agent_receives_domain_events() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"method":"Debugger.resumed","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    let events = Arc::new(AtomicUsize::new(0));
    let mut registry = AgentRegistry::new();

    let snap = registry
        .register(
            CountingAgent {
                events: Arc::clone(&events),
            },
            &session,
        )
        .await
        .expect("register")
        .expect("agent attached");

    // CountingAgent::attach drives Debugger.enable, which releases the
    // Debugger.resumed event sitting ahead of the enable reply.
    for _ in 0..50 {
        if events.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        events.load(Ordering::SeqCst) >= 1,
        "agent should have folded Debugger.resumed"
    );
    // Successful on_event republishes into the ArcSwap the host holds.
    let _ = snap.load();
}

#[derive(Debug, Default)]
struct CountingModel {
    events: usize,
}

#[derive(Debug)]
struct SnapshotAgent {
    model: CountingModel,
}

#[async_trait]
impl DomainAgent for SnapshotAgent {
    type Model = CountingModel;

    const DOMAINS: &'static [Domain] = &[Domain::Debugger];
    const NAME: &'static str = "snapshot-agent";

    async fn attach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        session
            .call(mjx_wk_protocol::generated::debugger::commands::Enable {})
            .await?;
        Ok(())
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        self.model.events += 1;
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(CountingModel {
            events: self.model.events,
        })
    }
}

#[tokio::test]
async fn register_publishes_model_updates_through_arcswap() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.enable","params":{}}}
{"dir":"recv","frame":{"method":"Debugger.resumed","params":{}}}
{"dir":"recv","frame":{"id":2,"result":{}}}
"#;
    let session = attach(trace).await;
    let mut registry = AgentRegistry::new();
    let snap = registry
        .register(
            SnapshotAgent {
                model: CountingModel::default(),
            },
            &session,
        )
        .await
        .expect("register")
        .expect("agent attached");

    assert_eq!(snap.load().events, 0);

    for _ in 0..50 {
        if snap.load().events > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        snap.load().events >= 1,
        "ArcSwap must republish after on_event"
    );
}
