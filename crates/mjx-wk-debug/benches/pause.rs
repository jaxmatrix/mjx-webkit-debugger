//! Pause → first variable row.
//!
//! Enforces [`mjx_wk_perf::Budget::PauseToFirstVariable`]. Folds
//! `fixtures/breakpoint-hit.jsonl` through the cost model for decoding a
//! `Debugger.paused` event and materialising the first `Runtime.getProperties`
//! row — without calling Phase-2 `ValueTree` APIs still owned by T-203.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_perf::{Budget, OpCounter, assert_ops, max_ops};
use serde_json::Value;

fn main() {
    let root = repo_root();
    let fixture = root.join("fixtures/breakpoint-hit.jsonl");
    let text = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));

    let mut counter = OpCounter::new();
    let mut paused: Option<Value> = None;
    let mut first_properties: Option<Value> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        counter.tick();
        let entry: Value = serde_json::from_str(line).expect("fixture line JSON");
        counter.tick();
        let Some(frame) = entry.get("frame") else {
            continue;
        };
        counter.tick();

        let inner = unwrap_inner(frame, &mut counter);
        let method = inner
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        counter.tick();

        if method == "Debugger.paused" && paused.is_none() {
            paused = Some(inner.clone());
            counter.alloc(1);
        }
        if first_properties.is_none()
            && inner
                .pointer("/result/properties")
                .and_then(Value::as_array)
                .is_some()
        {
            first_properties = Some(inner.clone());
            counter.alloc(1);
        }
    }

    let paused = paused.expect("fixture must contain Debugger.paused");
    let call_frames = paused
        .pointer("/params/callFrames")
        .and_then(Value::as_array)
        .expect("paused.callFrames");
    counter.add(call_frames.len() as u64); // walk frames

    let top = call_frames.first().expect("at least one call frame");
    let scopes = top
        .get("scopeChain")
        .and_then(Value::as_array)
        .expect("scopeChain");
    counter.add(scopes.len() as u64);

    // First variable row: the first own property from getProperties, or a
    // scope preview row when the top scope is empty.
    let mut first_row: Option<String> = None;
    if let Some(props) = first_properties
        .as_ref()
        .and_then(|v| v.pointer("/result/properties"))
        .and_then(Value::as_array)
    {
        counter.add(props.len() as u64); // paginated fold — WebKit fetchCount
        if let Some(prop) = props.first() {
            let name = prop
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("(anonymous)");
            let preview = prop
                .pointer("/value/type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            first_row = Some(format!("{name}: {preview}"));
            counter.alloc(1);
            counter.add(4); // row materialise
        }
    }
    if first_row.is_none() {
        // Empty closure scope — still must produce a visible row promptly.
        let scope_ty = scopes
            .first()
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("scope");
        first_row = Some(format!("{scope_ty}: …"));
        counter.alloc(1);
        counter.add(4);
    }

    let row = first_row.expect("first variable row");
    let budget = Budget::PauseToFirstVariable;
    assert_ops(budget, counter.ops(), max_ops(budget));
    println!(
        "ok: {budget} — {} ops ≤ {} (first row `{row}`)",
        counter.ops(),
        max_ops(budget),
    );
}

fn unwrap_inner(frame: &Value, counter: &mut OpCounter) -> Value {
    if let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) {
        counter.tick();
        let inner: Value = serde_json::from_str(message).expect("Target.* message JSON");
        counter.tick();
        return inner;
    }
    frame.clone()
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("crates/mjx-wk-debug → repo root")
}
