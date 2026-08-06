//! Attach → source tree visible.
//!
//! Enforces [`mjx_wk_perf::Budget::AttachToSourceTree`]. CI asserts operation
//! counts, not wall-clock — see `crates/mjx-wk-perf`.
//!
//! Measures:
//! 1. parsing `fixtures/attach.jsonl`;
//! 2. a real [`Session::attach`] handshake (Inspector.enable);
//! 3. folding script/resource-shaped rows into a synthetic source tree the UI
//!    could show — the inventory/tree widgets are owned by peer tickets, so
//!    this charges the same op model they must stay inside.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_dialect::{DialectKind, WebKitDialect};
use mjx_wk_perf::{Budget, OpCounter, assert_ops, max_ops, ops};
use mjx_wk_protocol::TargetType;
use mjx_wk_session::Session;
use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};
use serde_json::Value;

fn main() {
    let root = repo_root();
    let fixture = root.join("fixtures/attach.jsonl");
    let text = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));

    let mut counter = OpCounter::new();

    // --- 1. Parse every fixture frame (attach I/O fold).
    let mut script_urls = Vec::new();
    let mut resource_urls = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        counter.tick(); // read line
        let entry: Value = serde_json::from_str(line).expect("fixture line JSON");
        counter.tick(); // parse
        collect_source_hints(&entry, &mut script_urls, &mut resource_urls, &mut counter);
    }

    // --- 2. Real session attach on a handshake-compatible slice.
    // `fixtures/attach.jsonl` multiplexes through Target.*; Session::attach
    // sends bare Inspector.enable. Use the same shape the session tests pin.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let handshake = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
"#;
        let transport = ReplayTransport::from_str(handshake, "attach-bench-handshake")
            .expect("handshake trace");
        counter.add(2); // encode + correlate
        let session = Session::attach(
            Box::new(transport),
            Box::new(WebKitDialect),
            page_target("fixtures/attach.jsonl"),
        )
        .await
        .expect("Session::attach");
        counter.tick(); // capabilities ready
        assert!(session.is_connected());
    });

    // --- 3. Source tree visible: materialise virtualised rows from the
    // fixture's script/resource hints (plus a synthetic busy-page pad so the
    // budget covers a realistic first paint, not a one-row page).
    const SYNTHETIC_EXTRA_SOURCES: u64 = 256;
    let mut tree_rows: Vec<String> = Vec::new();
    for url in script_urls.iter().chain(resource_urls.iter()) {
        counter.alloc(1);
        tree_rows.push(origin_path_row(url));
        counter.add(ops::OPS_PER_ROW); // row insert + origin bucket
    }
    for i in 0..SYNTHETIC_EXTRA_SOURCES {
        counter.alloc(1);
        tree_rows.push(format!("https://cdn.example/vendor/chunk-{i}.js"));
        counter.add(ops::OPS_PER_ROW);
    }
    // First paint: only the visible window is laid out.
    let first_paint_rows = ops::VISIBLE_ROWS_PER_FRAME.min(tree_rows.len() as u64);
    counter.add(first_paint_rows * ops::OPS_PER_ROW);

    let budget = Budget::AttachToSourceTree;
    assert_ops(budget, counter.ops(), max_ops(budget));
    println!(
        "ok: {budget} — {} ops ≤ {} (tree rows {}, first paint {})",
        counter.ops(),
        max_ops(budget),
        tree_rows.len(),
        first_paint_rows,
    );
}

fn page_target(fixture: &str) -> Target {
    Target {
        key: TargetKey("bench/attach".into()),
        name: "attach-bench".into(),
        url: "https://example.test/".into(),
        kind: TargetType::WebPage,
        dialect: DialectKind::WebKitRwi,
        origin: TransportOrigin::Replay {
            fixture: fixture.into(),
        },
    }
}

fn collect_source_hints(
    entry: &Value,
    scripts: &mut Vec<String>,
    resources: &mut Vec<String>,
    counter: &mut OpCounter,
) {
    let Some(frame) = entry.get("frame") else {
        return;
    };
    counter.tick();

    if let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) {
        counter.tick(); // unwrap Target.* string
        if let Ok(inner) = serde_json::from_str::<Value>(message) {
            counter.tick();
            note_inner(&inner, scripts, resources, counter);
        }
        return;
    }
    note_inner(frame, scripts, resources, counter);
}

fn note_inner(
    frame: &Value,
    scripts: &mut Vec<String>,
    resources: &mut Vec<String>,
    counter: &mut OpCounter,
) {
    let method = frame.get("method").and_then(Value::as_str).unwrap_or("");
    counter.tick();
    if method == "Debugger.scriptParsed"
        && let Some(url) = frame.pointer("/params/url").and_then(Value::as_str)
    {
        scripts.push(url.to_owned());
        counter.alloc(1);
    }
    if let Some(tree) = frame.pointer("/result/frameTree") {
        walk_resources(tree, resources, counter);
    }
}

fn walk_resources(node: &Value, resources: &mut Vec<String>, counter: &mut OpCounter) {
    counter.tick();
    if let Some(url) = node.pointer("/frame/url").and_then(Value::as_str) {
        resources.push(url.to_owned());
        counter.alloc(1);
    }
    if let Some(arr) = node.get("resources").and_then(Value::as_array) {
        for r in arr {
            counter.tick();
            if let Some(url) = r.get("url").and_then(Value::as_str) {
                resources.push(url.to_owned());
                counter.alloc(1);
            }
        }
    }
    if let Some(children) = node.get("childFrames").and_then(Value::as_array) {
        for child in children {
            walk_resources(child, resources, counter);
        }
    }
}

fn origin_path_row(url: &str) -> String {
    // Cheap stand-in for inventory's origin → path grouping.
    match url.split_once("://") {
        Some((origin, rest)) => format!("{origin}/{rest}"),
        None => url.to_owned(),
    }
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("crates/mjx-wk-session → repo root")
}
