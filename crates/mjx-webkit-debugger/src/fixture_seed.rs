//! Seed a [`SourceInventory`] from a recorded `.jsonl` without a live session.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md` (Phase 2 shell wiring).**
//!
//! WebKitGTK multiplex fixtures wrap domain traffic in `Target.*`, so
//! `Session::attach`'s root `Inspector.enable` often cannot drive the trace.
//! Scraping `scriptParsed` / `getResourceTree` out of the fixture still lets
//! replay show a real source tree offline — the same approach T-004's inventory
//! tests use.
//!
//! [`flatten_multiplexed_trace`] rewrites those fixtures into bare domain frames
//! so `Session::attach` + agent registration can replay them end-to-end.

use std::path::Path;

use anyhow::{Context, Result};
use mjx_wk_protocol::generated::debugger::events::ScriptParsed;
use mjx_wk_protocol::generated::page::commands::GetResourceTreeReturns;
use mjx_wk_source::SourceInventory;
use serde_json::{Value, json};

/// Populate `inventory` from `Debugger.scriptParsed` and `Page.getResourceTree`
/// payloads found inside a recorded trace (including Target.*-wrapped ones).
pub fn seed_inventory_from_fixture(
    path: &Path,
    inventory: &mut SourceInventory,
) -> Result<(usize, bool)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading fixture {}", path.display()))?;

    let mut scripts = 0usize;
    let mut saw_tree = false;

    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("{}:{}: jsonl row", path.display(), n + 1))?;
        let frame = &row["frame"];
        for inner in unwrap_frames(frame) {
            if inner.get("method").and_then(Value::as_str) == Some("Debugger.scriptParsed") {
                let event: ScriptParsed = serde_json::from_value(inner["params"].clone())
                    .with_context(|| format!("{}:{}: scriptParsed", path.display(), n + 1))?;
                inventory.on_script_parsed(&event);
                scripts += 1;
            }
            if inner
                .get("result")
                .and_then(|r| r.get("frameTree"))
                .is_some()
            {
                let returns: GetResourceTreeReturns =
                    serde_json::from_value(inner["result"].clone()).with_context(|| {
                        format!("{}:{}: getResourceTree", path.display(), n + 1)
                    })?;
                inventory.on_resource_tree(&returns.frame_tree);
                saw_tree = true;
            }
        }
    }

    Ok((scripts, saw_tree))
}

/// Rewrite a WebKitGTK `Target.*`-multiplexed `.jsonl` into bare domain frames.
///
/// `Session::attach` sends root `Inspector.enable`; agents send bare
/// `Debugger.enable` / `Console.enable`. Real WebKitGTK recordings wrap those
/// in `Target.sendMessageToTarget`. Flattening keeps ReplayTransport's method
/// match working without inventing a new session seam.
pub fn flatten_multiplexed_trace(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading fixture {}", path.display()))?;

    let mut out = String::new();
    let mut skipped_outer_acks: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("{}:{}: jsonl row", path.display(), n + 1))?;
        let dir = row.get("dir").and_then(Value::as_str).unwrap_or("");
        let frame = row.get("frame").cloned().unwrap_or(Value::Null);
        let method = frame.get("method").and_then(Value::as_str).unwrap_or("");

        if method == "Target.sendMessageToTarget" {
            let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) else {
                continue;
            };
            let inner: Value = serde_json::from_str(message)
                .with_context(|| format!("{}:{}: inner send JSON", path.display(), n + 1))?;
            if let Some(outer_id) = frame.get("id").and_then(Value::as_u64) {
                skipped_outer_acks.insert(outer_id);
            }
            let t = row.get("t").cloned().unwrap_or(json!(0));
            out.push_str(&serde_json::to_string(
                &json!({"t": t, "dir": dir, "frame": inner}),
            )?);
            out.push('\n');
            continue;
        }

        if method == "Target.dispatchMessageFromTarget" {
            let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) else {
                continue;
            };
            let inner: Value = serde_json::from_str(message)
                .with_context(|| format!("{}:{}: inner recv JSON", path.display(), n + 1))?;
            let t = row.get("t").cloned().unwrap_or(json!(0));
            out.push_str(&serde_json::to_string(
                &json!({"t": t, "dir": dir, "frame": inner}),
            )?);
            out.push('\n');
            continue;
        }

        if method == "Target.targetCreated" || method.starts_with("Target.") {
            continue;
        }

        // Drop outer empty acks for sendMessageToTarget (id-only result frames).
        if method.is_empty()
            && let Some(id) = frame.get("id").and_then(Value::as_u64)
            && skipped_outer_acks.contains(&id)
        {
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    Ok(out)
}

/// Whether a fixture appears to use WebKitGTK page-target multiplexing.
pub fn is_multiplexed_fixture(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.contains("Target.sendMessageToTarget"))
        .unwrap_or(false)
}

/// Yield the frame itself plus any JSON string carried in
/// `Target.dispatchMessageFromTarget`.
fn unwrap_frames(frame: &Value) -> Vec<Value> {
    let mut out = vec![frame.clone()];
    if frame.get("method").and_then(Value::as_str) == Some("Target.dispatchMessageFromTarget")
        && let Some(message) = frame.pointer("/params/message").and_then(Value::as_str)
        && let Ok(inner) = serde_json::from_str::<Value>(message)
    {
        out.push(inner);
    }
    out
}

/// Best-effort load of a fixture-page file for offline code view when the
/// session cannot fetch (`Page.getResourceContent` / `getScriptSource`).
pub fn load_local_fixture_text(url: &str) -> Option<String> {
    // Recorded attach traces point at the local fixture HTTP server.
    const PREFIX: &str = "http://127.0.0.1:8731/";
    let rel = url.strip_prefix(PREFIX)?;
    let path = Path::new("fixtures/page").join(rel);
    std::fs::read_to_string(path).ok()
}
