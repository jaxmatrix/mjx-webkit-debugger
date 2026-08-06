//! Bench owned by `docs/tasks/T-005-source-text.md`.
//!
//! Asserts that indexing a multi-megabyte source stays inside the attach-side
//! budget from `CLAUDE.md` (source tree visible < 300 ms). Random line access
//! must also stay well under a single frame (16 ms) for a full window of rows.

use std::hint::black_box;
use std::time::{Duration, Instant};

use mjx_wk_source::{LineIndex, SourceId, SourceText};

/// Fraction of the attach budget we allow for indexing alone.
const INDEX_BUDGET: Duration = Duration::from_millis(100);

/// Sixty line lookups must finish in one frame — the virtualised code view's
/// steady-state cost for "what is on line N?".
const LOOKUP_WINDOW: usize = 60;
const LOOKUP_BUDGET: Duration = Duration::from_millis(16);

fn main() {
    let text = load_bench_text();
    assert!(
        text.len() >= 2_000_000,
        "bench input must be multi-MB, got {} bytes",
        text.len()
    );

    // Warm once so the timed run is not dominated by first-touch page faults.
    black_box(LineIndex::build(black_box(&text)));

    let started = Instant::now();
    let index = LineIndex::build(black_box(&text));
    let index_elapsed = started.elapsed();
    eprintln!(
        "line_index_build: {index_elapsed:?} for {} bytes / {} lines",
        text.len(),
        index.line_count()
    );
    assert!(
        index_elapsed <= INDEX_BUDGET,
        "indexing took {index_elapsed:?}, budget is {INDEX_BUDGET:?}"
    );

    let line_count = index.line_count().max(1);
    let started = Instant::now();
    for i in 0..LOOKUP_WINDOW {
        let line = ((i as u32).wrapping_mul(7919)) % line_count;
        black_box(index.line_range(black_box(line)));
    }
    let lookup_elapsed = started.elapsed();
    eprintln!("line_range x{LOOKUP_WINDOW}: {lookup_elapsed:?}");
    assert!(
        lookup_elapsed <= LOOKUP_BUDGET,
        "lookups took {lookup_elapsed:?}, budget is {LOOKUP_BUDGET:?}"
    );

    // SourceText path includes Arc wrapping; still must fit the same index budget.
    let started = Instant::now();
    let src = SourceText::new(SourceId(1), text);
    let wrap_elapsed = started.elapsed();
    eprintln!("source_text_new: {wrap_elapsed:?}");
    assert!(
        wrap_elapsed <= INDEX_BUDGET,
        "SourceText::new took {wrap_elapsed:?}, budget is {INDEX_BUDGET:?}"
    );
    black_box(src.line(0));
}

fn load_bench_text() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/large-bundle.jsonl"
    );
    match std::fs::read_to_string(path) {
        Ok(raw) => extract_script_source(&raw).unwrap_or_else(synthetic_five_mb),
        Err(_) => synthetic_five_mb(),
    }
}

fn extract_script_source(jsonl: &str) -> Option<String> {
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(message) = v.pointer("/frame/params/message").and_then(|m| m.as_str()) else {
            continue;
        };
        let Ok(inner) = serde_json::from_str::<serde_json::Value>(message) else {
            continue;
        };
        if let Some(src) = inner
            .pointer("/result/scriptSource")
            .and_then(|s| s.as_str())
        {
            return Some(src.to_owned());
        }
    }
    None
}

/// Fallback when the fixture is absent — still exercises the 5 MB path.
fn synthetic_five_mb() -> String {
    let line = "x".repeat(4095) + "\n";
    line.repeat(1280) // ~5.2 MB
}
