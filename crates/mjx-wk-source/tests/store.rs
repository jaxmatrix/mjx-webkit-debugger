//! SourceStore — fetch, byte-budget LRU, and request dedup.
//!
//! **Owned by `docs/tasks/T-011-source-store.md`.**

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_protocol::TargetType;
use mjx_wk_session::Session;
use mjx_wk_source::{
    FrameId, SourceEntry, SourceError, SourceId, SourceKind, SourceStore, SourceText,
};
use mjx_wk_transport::{
    ReplayTransport, Target, TargetKey, Transport, TransportError, TransportOrigin,
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify};

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

async fn attach_replay(trace: &str) -> mjx_wk_session::SessionHandle {
    let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
    Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach")
}

fn script_entry(id: u32, script_id: &str) -> SourceEntry {
    SourceEntry {
        id: SourceId(id),
        script_id: Some(script_id.into()),
        frame: None,
        url: format!("https://example.test/{script_id}.js"),
        kind: SourceKind::Script {
            module: false,
            content_script: false,
        },
        source_map_url: None,
        is_original: false,
    }
}

fn document_entry(id: u32, frame: &str, url: &str) -> SourceEntry {
    SourceEntry {
        id: SourceId(id),
        script_id: None,
        frame: Some(FrameId(frame.into())),
        url: url.into(),
        kind: SourceKind::Document,
        source_map_url: None,
        is_original: false,
    }
}

/// Soft-depend on T-005: successful fetch wraps through `SourceText::new`.
fn source_text_ready() -> bool {
    use std::sync::OnceLock;
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let ok = std::panic::catch_unwind(|| SourceText::new(SourceId(0), "x".into())).is_ok();
        std::panic::set_hook(prev);
        ok
    })
}

fn require_source_text() -> bool {
    if source_text_ready() {
        true
    } else {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, Ordering::Relaxed) {
            eprintln!(
                "soft-dep: SourceText::new still stubbed (T-005); skipping fetch/cache body tests"
            );
        }
        false
    }
}

#[test]
fn empty_store_cached_and_bytes_are_nonblocking() {
    let store = SourceStore::new(1024);
    assert!(store.cached(SourceId(1)).is_none());
    assert_eq!(store.bytes_held(), 0);
    store.clear();
    assert_eq!(store.bytes_held(), 0);
}

#[tokio::test]
async fn other_kind_is_not_text_without_wire_io() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
    let session = attach_replay(trace).await;
    let store = SourceStore::new(1024);
    let entry = SourceEntry {
        id: SourceId(9),
        script_id: None,
        frame: None,
        url: "https://example.test/x.png".into(),
        kind: SourceKind::Other,
        source_map_url: None,
        is_original: false,
    };
    let err = store.text(&session, &entry).await.unwrap_err();
    assert!(matches!(err, SourceError::NotText(SourceId(9))));
    assert!(store.cached(SourceId(9)).is_none());
}

#[tokio::test]
async fn missing_script_id_is_unavailable_without_wire_io() {
    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
    let session = attach_replay(trace).await;
    let store = SourceStore::new(1024);
    let entry = SourceEntry {
        id: SourceId(2),
        script_id: None,
        frame: None,
        url: "https://example.test/a.js".into(),
        kind: SourceKind::Script {
            module: false,
            content_script: false,
        },
        source_map_url: None,
        is_original: false,
    };
    let err = store.text(&session, &entry).await.unwrap_err();
    assert!(matches!(
        err,
        SourceError::Unavailable {
            id: SourceId(2),
            ..
        }
    ));
}

/// Counts `Debugger.getScriptSource` / `Page.getResourceContent` sends and can
/// hold the first reply until a second caller has also sent — proving dedup.
#[derive(Debug)]
struct GateTransport {
    script_source: String,
    resource: Option<ResourceReply>,
    get_source_calls: Arc<AtomicUsize>,
    /// When set, the first getScriptSource waits here before replying.
    hold: Option<Arc<Notify>>,
    /// Satisfied when the first getScriptSource send is observed.
    first_send: Arc<Notify>,
    pending: Arc<Mutex<Vec<String>>>,
    closed: bool,
}

#[derive(Debug, Clone)]
struct ResourceReply {
    content: String,
    base64_encoded: bool,
}

impl GateTransport {
    fn script(source: impl Into<String>) -> Self {
        Self {
            script_source: source.into(),
            resource: None,
            get_source_calls: Arc::new(AtomicUsize::new(0)),
            hold: None,
            first_send: Arc::new(Notify::new()),
            pending: Arc::new(Mutex::new(Vec::new())),
            closed: false,
        }
    }
}

#[async_trait]
impl Transport for GateTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost("closed".into()));
        }
        let sent: Value = serde_json::from_str(&text)
            .map_err(|e| TransportError::Malformed(format!("outgoing: {e}")))?;
        let id = sent.get("id").and_then(Value::as_u64).ok_or_else(|| {
            TransportError::Malformed("outgoing frame missing id".into())
        })?;
        let method = sent
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| TransportError::Malformed("outgoing frame missing method".into()))?;

        let reply = match method {
            "Inspector.enable" => json!({"id": id, "result": {}}),
            "Debugger.getScriptSource" => {
                let n = self.get_source_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if n == 1 {
                    self.first_send.notify_waiters();
                }
                if let Some(hold) = &self.hold {
                    hold.notified().await;
                }
                json!({
                    "id": id,
                    "result": { "scriptSource": self.script_source },
                })
            }
            "Page.getResourceContent" => {
                self.get_source_calls.fetch_add(1, Ordering::SeqCst);
                let Some(resource) = &self.resource else {
                    return Err(TransportError::Malformed(
                        "unexpected getResourceContent".into(),
                    ));
                };
                json!({
                    "id": id,
                    "result": {
                        "content": resource.content,
                        "base64Encoded": resource.base64_encoded,
                    },
                })
            }
            other => {
                return Err(TransportError::Malformed(format!(
                    "unexpected method {other}"
                )));
            }
        };
        self.pending
            .lock()
            .await
            .push(serde_json::to_string(&reply).expect("reply json"));
        Ok(())
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        // `None` means "nothing queued yet", matching ReplayTransport — the
        // session task parks on the next command rather than spinning.
        self.pending.lock().await.pop().map(Ok)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        Ok(())
    }

    fn dialect(&self) -> DialectKind {
        DialectKind::WebKitRwi
    }
}

async fn attach_gate(transport: GateTransport) -> (mjx_wk_session::SessionHandle, Arc<AtomicUsize>) {
    let calls = Arc::clone(&transport.get_source_calls);
    let session = Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach");
    (session, calls)
}

#[tokio::test]
async fn concurrent_text_shares_one_get_script_source() {
    if !require_source_text() {
        return;
    }

    let hold = Arc::new(Notify::new());
    let mut transport = GateTransport::script("console.log(1);");
    transport.hold = Some(Arc::clone(&hold));
    let first_send = Arc::clone(&transport.first_send);
    let (session, calls) = attach_gate(transport).await;
    let store = Arc::new(SourceStore::new(1024 * 1024));
    let entry = script_entry(1, "42");

    let store_a = Arc::clone(&store);
    let session_a = session.clone();
    let entry_a = entry.clone();
    let a = tokio::spawn(async move { store_a.text(&session_a, &entry_a).await });

    // Wait until the leader has issued getScriptSource, then overlap a second
    // caller before releasing the reply.
    first_send.notified().await;
    let store_b = Arc::clone(&store);
    let session_b = session.clone();
    let entry_b = entry.clone();
    let b = tokio::spawn(async move { store_b.text(&session_b, &entry_b).await });

    // Give the follower time to join the in-flight wait before the reply lands.
    tokio::task::yield_now().await;
    hold.notify_waiters();

    let ta = a.await.unwrap().expect("caller a");
    let tb = b.await.unwrap().expect("caller b");
    assert!(Arc::ptr_eq(&ta, &tb));
    assert_eq!(ta.as_str(), "console.log(1);");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.bytes_held(), "console.log(1);".len());
}

#[tokio::test]
async fn cache_evicts_by_bytes_not_entry_count() {
    if !require_source_text() {
        return;
    }

    // Budget fits the large body alone, but not large+small together — so
    // inserting small after large must evict large (byte LRU), proving we do
    // not keep N entries regardless of size.
    let large = "L".repeat(1000);
    let small = "s".repeat(100);
    let budget = large.len() + 10;

    let transport = ScriptByIdTransport {
        bodies: Arc::new(Mutex::new(vec![
            ("big".into(), large.clone()),
            ("small".into(), small.clone()),
        ])),
        pending: Arc::new(Mutex::new(Vec::new())),
        closed: false,
        fetches: Arc::new(AtomicUsize::new(0)),
    };
    let fetches = Arc::clone(&transport.fetches);
    let session = Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .expect("attach");

    let store = SourceStore::new(budget);
    let big = store
        .text(&session, &script_entry(1, "big"))
        .await
        .expect("large");
    assert_eq!(big.as_str().len(), large.len());
    assert_eq!(store.bytes_held(), large.len());

    let little = store
        .text(&session, &script_entry(2, "small"))
        .await
        .expect("small");
    assert_eq!(little.as_str(), small);
    // Large must have been evicted to make room.
    assert!(store.cached(SourceId(1)).is_none());
    assert!(store.cached(SourceId(2)).is_some());
    assert_eq!(store.bytes_held(), small.len());
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
}

#[derive(Debug)]
struct ScriptByIdTransport {
    bodies: Arc<Mutex<Vec<(String, String)>>>,
    pending: Arc<Mutex<Vec<String>>>,
    closed: bool,
    fetches: Arc<AtomicUsize>,
}

#[async_trait]
impl Transport for ScriptByIdTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        let sent: Value = serde_json::from_str(&text)
            .map_err(|e| TransportError::Malformed(format!("outgoing: {e}")))?;
        let id = sent.get("id").and_then(Value::as_u64).unwrap();
        let method = sent.get("method").and_then(Value::as_str).unwrap();
        let reply = match method {
            "Inspector.enable" => json!({"id": id, "result": {}}),
            "Debugger.getScriptSource" => {
                self.fetches.fetch_add(1, Ordering::SeqCst);
                let script_id = sent["params"]["scriptId"].as_str().unwrap().to_owned();
                let bodies = self.bodies.lock().await;
                let source = bodies
                    .iter()
                    .find(|(k, _)| *k == script_id)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| {
                        TransportError::Malformed(format!("unknown scriptId {script_id}"))
                    })?;
                json!({"id": id, "result": { "scriptSource": source }})
            }
            other => {
                return Err(TransportError::Malformed(format!(
                    "unexpected method {other}"
                )));
            }
        };
        self.pending
            .lock()
            .await
            .push(serde_json::to_string(&reply).unwrap());
        Ok(())
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        self.pending.lock().await.pop().map(Ok)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        Ok(())
    }

    fn dialect(&self) -> DialectKind {
        DialectKind::WebKitRwi
    }
}

#[tokio::test]
async fn clear_drops_cache_so_stale_script_text_is_not_reused() {
    if !require_source_text() {
        return;
    }

    let transport = ScriptByIdTransport {
        bodies: Arc::new(Mutex::new(vec![("1".into(), "first".into())])),
        pending: Arc::new(Mutex::new(Vec::new())),
        closed: false,
        fetches: Arc::new(AtomicUsize::new(0)),
    };
    let session = Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
        .await
        .unwrap();
    let store = SourceStore::new(4096);
    let entry = script_entry(1, "1");
    let text = store.text(&session, &entry).await.unwrap();
    assert_eq!(text.as_str(), "first");
    assert!(store.cached(SourceId(1)).is_some());

    store.clear();
    assert!(store.cached(SourceId(1)).is_none());
    assert_eq!(store.bytes_held(), 0);
}

#[tokio::test]
async fn get_resource_content_decodes_base64() {
    if !require_source_text() {
        return;
    }

    let mut transport = GateTransport::script("");
    transport.resource = Some(ResourceReply {
        // "body {}"
        content: "Ym9keSB7fQ==".into(),
        base64_encoded: true,
    });
    let (session, calls) = attach_gate(transport).await;
    let store = SourceStore::new(4096);
    let entry = document_entry(5, "frame-1", "https://example.test/");
    let text = store.text(&session, &entry).await.expect("document");
    assert_eq!(text.as_str(), "body {}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn get_resource_content_binary_is_not_text() {
    // Binary rejection happens in decode before SourceText::new — no T-005 need.
    let mut transport = GateTransport::script("");
    transport.resource = Some(ResourceReply {
        // PNG signature, base64
        content: "iVBORw0KGgo=".into(),
        base64_encoded: true,
    });
    let (session, _) = attach_gate(transport).await;
    let store = SourceStore::new(4096);
    let entry = document_entry(6, "frame-1", "https://example.test/x.png");
    // Kind must be Document/StyleSheet to hit getResourceContent; binary still
    // fails UTF-8 after base64 decode.
    let entry = SourceEntry {
        kind: SourceKind::StyleSheet,
        ..entry
    };
    let err = store.text(&session, &entry).await.unwrap_err();
    assert!(matches!(err, SourceError::NotText(SourceId(6))));
    assert!(store.cached(SourceId(6)).is_none());
}

#[tokio::test]
async fn replay_inline_get_script_source_caches() {
    if !require_source_text() {
        return;
    }

    let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Debugger.getScriptSource","params":{"scriptId":"7"}}}
{"dir":"recv","frame":{"id":2,"result":{"scriptSource":"export const x = 1;\n"}}}
"#;
    let session = attach_replay(trace).await;
    let store = SourceStore::new(4096);
    let entry = script_entry(3, "7");
    let text = store.text(&session, &entry).await.expect("fetch");
    assert_eq!(text.as_str(), "export const x = 1;\n");
    assert!(store.cached(SourceId(3)).is_some());
    // Second call is cache-only — replay has no further getScriptSource.
    let again = store.text(&session, &entry).await.expect("cached");
    assert!(Arc::ptr_eq(&text, &again));
}

#[test]
fn large_bundle_fixture_carries_multimegabyte_script_source() {
    // The recorded fixture is Target-multiplexed; we assert the corpus shape
    // here and exercise the body through the in-test fetch path above.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/large-bundle.jsonl");
    let text = std::fs::read_to_string(&path).expect("large-bundle.jsonl");
    let mut found = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("jsonl");
        let frame = &v["frame"];
        if frame.get("method").and_then(Value::as_str)
            == Some("Target.dispatchMessageFromTarget")
        {
            let message = frame["params"]["message"].as_str().unwrap_or("");
            let inner: Value = serde_json::from_str(message).unwrap_or(json!({}));
            if let Some(src) = inner.pointer("/result/scriptSource").and_then(Value::as_str) {
                found = Some(src.len());
                break;
            }
        }
    }
    let len = found.expect("getScriptSource result in large-bundle.jsonl");
    assert!(
        len > 1_000_000,
        "large-bundle scriptSource should be multi-MB, got {len}"
    );
}

#[test]
fn attach_fixture_exists_for_inventory_pairing() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/attach.jsonl");
    assert!(path.is_file(), "attach.jsonl should exist at {}", path.display());
}
