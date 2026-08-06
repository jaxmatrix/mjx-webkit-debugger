//! SourceMapResolver — data: maps, relative URL resolve, multi-loc to_generated,
//! silent load failure, authored sources marked `is_original`.
//!
//! **Owned by `docs/tasks/T-205-source-maps.md`.**

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_protocol::TargetType;
use mjx_wk_session::Session;
use mjx_wk_source::maps::resolve_source_map_url;
use mjx_wk_source::{SourceId, SourceLocation, SourceMapResolver};
use mjx_wk_transport::{Target, TargetKey, Transport, TransportError, TransportOrigin};
use serde_json::{Value, json};
use sourcemap::SourceMapBuilder;
use tokio::sync::Mutex;

fn page_target_at(url: &str) -> Target {
    Target {
        key: TargetKey("test/page".into()),
        name: "fixture".into(),
        url: url.into(),
        kind: TargetType::WebPage,
        dialect: DialectKind::WebKitRwi,
        origin: TransportOrigin::Replay {
            fixture: "inline".into(),
        },
    }
}

fn attach_trace() -> &'static str {
    r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#
}

async fn attach_replay(url: &str) -> mjx_wk_session::SessionHandle {
    let transport = mjx_wk_transport::ReplayTransport::from_str(attach_trace(), "inline")
        .expect("trace parses");
    Session::attach(
        Box::new(transport),
        Box::new(WebKitDialect),
        page_target_at(url),
    )
    .await
    .expect("attach")
}

/// Build a map where one authored line is inlined at two generated columns.
fn multi_inline_map() -> sourcemap::SourceMap {
    let mut b = SourceMapBuilder::new(Some("bundle.js"));
    let src = b.add_source("src/shared.js");
    b.set_source_contents(
        src,
        Some("export function add(a, b) {\n  return a + b;\n}\n"),
    );
    // generated col 0 and col 30 both map back to authored line 0 col 0
    b.add(0, 0, 0, 0, Some("src/shared.js"), None, false);
    b.add(0, 30, 0, 0, Some("src/shared.js"), None, false);
    b.into_sourcemap()
}

fn multi_inline_map_json() -> String {
    let sm = multi_inline_map();
    let mut out = Vec::new();
    sm.to_writer(&mut out).expect("encode map");
    String::from_utf8(out).expect("utf8 map")
}

fn multi_inline_data_url() -> String {
    multi_inline_map().to_data_url().expect("data url")
}

fn bytes_to_data_url(json: &str) -> String {
    sourcemap::SourceMap::from_slice(json.as_bytes())
        .expect("fixture map parses")
        .to_data_url()
        .expect("data url")
}

fn mapped_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/page/mapped")
}

#[test]
fn relative_source_map_url_resolves_against_script_url() {
    assert_eq!(
        resolve_source_map_url("https://example.test/js/bundle.js", "bundle.js.map"),
        "https://example.test/js/bundle.js.map"
    );
    assert_eq!(
        resolve_source_map_url("https://example.test/js/bundle.js", "../maps/x.js.map"),
        "https://example.test/maps/x.js.map"
    );
    let data = "data:application/json;base64,e30=";
    assert_eq!(
        resolve_source_map_url("https://example.test/a.js", data),
        data
    );
}

#[tokio::test]
async fn data_uri_map_loads_without_fetch() {
    let session = attach_replay("https://example.test/").await;
    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(1);
    let url = multi_inline_data_url();

    resolver
        .load(&session, generated, &url)
        .await
        .expect("data: load is Ok");

    assert!(resolver.has_map(generated));

    let orig = resolver
        .to_original(SourceLocation {
            source: generated,
            line: 0,
            column: 0,
        })
        .expect("token at 0:0");
    assert_ne!(orig.source, generated);
    assert_eq!(orig.line, 0);
}

#[tokio::test]
async fn to_generated_returns_every_inline_of_an_authored_line() {
    let session = attach_replay("https://example.test/").await;
    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(7);
    resolver
        .load(&session, generated, &multi_inline_data_url())
        .await
        .expect("load");

    let authored = resolver
        .to_original(SourceLocation::line_start(generated, 0))
        .expect("original");
    let gens = resolver.to_generated(SourceLocation::line_start(authored.source, authored.line));
    assert!(
        gens.len() >= 2,
        "expected several generated sites for one authored line, got {gens:?}"
    );
    assert!(gens.iter().all(|g| g.source == generated));
    let cols: Vec<u32> = gens.iter().map(|g| g.column).collect();
    assert!(cols.contains(&0), "missing col 0 in {cols:?}");
    assert!(cols.contains(&30), "missing col 30 in {cols:?}");
}

#[tokio::test]
async fn authored_sources_are_marked_is_original() {
    let session = attach_replay("https://example.test/").await;
    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(3);
    resolver
        .load(&session, generated, &multi_inline_data_url())
        .await
        .expect("load");

    let entries = resolver.original_entries(generated);
    assert!(!entries.is_empty(), "map should expose authored sources");
    assert!(
        entries.iter().all(|e| e.is_original),
        "every authored entry must be is_original: {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e.display_name() == "shared.js"),
        "expected shared.js in {entries:?}"
    );
}

#[tokio::test]
async fn map_load_failure_is_silent() {
    let session = attach_replay("https://example.test/").await;
    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(9);

    // Nonsense data: URI — decode fails.
    let result = resolver
        .load(
            &session,
            generated,
            "data:application/json;base64,%%%not-valid%%%",
        )
        .await;
    assert!(
        result.is_ok(),
        "failed map must not surface Err: {result:?}"
    );
    assert!(!resolver.has_map(generated));

    // Remote URL with no Network.loadResource in the replay — fetch fails.
    let result = resolver
        .load(&session, generated, "https://example.test/missing.js.map")
        .await;
    assert!(result.is_ok());
    assert!(!resolver.has_map(generated));
}

/// Transport that serves `Page.getResourceTree` + `Network.loadResource` for one map URL.
#[derive(Debug)]
struct MapFetchTransport {
    map_url: String,
    map_body: String,
    status: i64,
    load_calls: Arc<AtomicUsize>,
    pending: Arc<Mutex<Vec<String>>>,
    closed: bool,
}

#[async_trait]
impl Transport for MapFetchTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost("closed".into()));
        }
        let sent: Value = serde_json::from_str(&text)
            .map_err(|e| TransportError::Malformed(format!("outgoing: {e}")))?;
        let id = sent
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| TransportError::Malformed("missing id".into()))?;
        let method = sent
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| TransportError::Malformed("missing method".into()))?;

        let reply = match method {
            "Inspector.enable" => json!({"id": id, "result": {}}),
            "Page.getResourceTree" => json!({
                "id": id,
                "result": {
                    "frameTree": {
                        "frame": {
                            "id": "frame-1",
                            "loaderId": "loader",
                            "url": "https://example.test/mapped/index.html",
                            "securityOrigin": "https://example.test",
                            "mimeType": "text/html"
                        },
                        "resources": []
                    }
                }
            }),
            "Network.loadResource" => {
                self.load_calls.fetch_add(1, Ordering::SeqCst);
                let url = sent["params"]["url"].as_str().unwrap_or("");
                if url != self.map_url {
                    return Err(TransportError::Malformed(format!(
                        "unexpected loadResource url {url}"
                    )));
                }
                json!({
                    "id": id,
                    "result": {
                        "content": self.map_body,
                        "mimeType": "application/json",
                        "status": self.status,
                    }
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
            .push(serde_json::to_string(&reply).expect("reply"));
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
async fn relative_url_absolutized_then_fetched() {
    let map_json = multi_inline_map_json();
    let script = "https://example.test/js/bundle.js";
    let absolute = resolve_source_map_url(script, "bundle.js.map");
    assert_eq!(absolute, "https://example.test/js/bundle.js.map");

    let load_calls = Arc::new(AtomicUsize::new(0));
    let transport = MapFetchTransport {
        map_url: absolute.clone(),
        map_body: map_json,
        status: 200,
        load_calls: Arc::clone(&load_calls),
        pending: Arc::new(Mutex::new(Vec::new())),
        closed: false,
    };
    let session = Session::attach(
        Box::new(transport),
        Box::new(WebKitDialect),
        page_target_at("https://example.test/"),
    )
    .await
    .expect("attach");

    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(5);
    resolver
        .load(&session, generated, &absolute)
        .await
        .expect("load");

    assert_eq!(load_calls.load(Ordering::SeqCst), 1);
    assert!(resolver.has_map(generated));
    assert!(
        resolver
            .to_original(SourceLocation::line_start(generated, 0))
            .is_some()
    );
}

#[tokio::test]
async fn http_error_status_is_silent_failure() {
    let transport = MapFetchTransport {
        map_url: "https://example.test/gone.js.map".into(),
        map_body: String::new(),
        status: 404,
        load_calls: Arc::new(AtomicUsize::new(0)),
        pending: Arc::new(Mutex::new(Vec::new())),
        closed: false,
    };
    let session = Session::attach(
        Box::new(transport),
        Box::new(WebKitDialect),
        page_target_at("https://example.test/"),
    )
    .await
    .expect("attach");

    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(4);
    let result = resolver
        .load(&session, generated, "https://example.test/gone.js.map")
        .await;
    assert!(result.is_ok());
    assert!(!resolver.has_map(generated));
}

#[tokio::test]
async fn fixture_page_map_round_trip() {
    let map_path = mapped_fixture_dir().join("bundle.js.map");
    let map_json = std::fs::read_to_string(&map_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", map_path.display()));

    let session = attach_replay("https://example.test/mapped/").await;
    let mut resolver = SourceMapResolver::new();
    let generated = SourceId(11);

    // data: wrap of the on-disk fixture — no network needed in CI.
    let data_url = bytes_to_data_url(&map_json);
    resolver
        .load(&session, generated, &data_url)
        .await
        .expect("fixture map loads");

    assert!(resolver.has_map(generated));
    let entries = resolver.original_entries(generated);
    assert!(entries.iter().all(|e| e.is_original));
    assert!(
        entries.iter().any(|e| e.url.contains("shared.js")),
        "fixture sources: {entries:?}"
    );

    let authored = resolver
        .to_original(SourceLocation {
            source: generated,
            line: 0,
            column: 0,
        })
        .expect("0:0 maps to authored");
    let gens = resolver.to_generated(SourceLocation::line_start(authored.source, 0));
    assert!(
        gens.len() >= 2,
        "fixture map inlines shared.js at several generated sites: {gens:?}"
    );
}
