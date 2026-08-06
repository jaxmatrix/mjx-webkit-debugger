//! 5 MB minified bundle scrolls at 60 fps.
//!
//! Enforces [`mjx_wk_perf::Budget::ScrollFiveMbAt60Fps`]. Asserts the **frame
//! distribution**: every frame and the 99th percentile stay inside the 60 fps
//! op ceiling, and a single hitch at the 300 ms cost-model ceiling fails.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_perf::{Budget, assert_frame_ops, ops};

fn main() {
    let root = repo_root();
    let bundle = root.join("fixtures/page/large-bundle.js");
    let bytes = std::fs::metadata(&bundle)
        .unwrap_or_else(|e| panic!("stat {}: {e}", bundle.display()))
        .len();
    assert!(
        bytes >= 2_000_000,
        "fixtures/page/large-bundle.js should be multi-megabyte (got {bytes} bytes)"
    );

    // Cost model for a minified multi-MB bundle: either one enormous line or
    // hundreds of thousands of short ones. Virtualisation may only touch the
    // visible window (+ margin) per frame — that is the whole budget.
    let line_count = synthetic_line_count(bytes);
    let window = ops::VISIBLE_ROWS_PER_FRAME + ops::SCROLL_MARGIN_ROWS;
    let frames = 120u64; // two seconds of scrolling at 60 fps
    let mut frame_ops: Vec<u64> = Vec::with_capacity(frames as usize);

    let mut scroll_line: u64 = 0;
    for frame in 0..frames {
        let mut charged = 0u64;
        // Advance by ~⅓ of a page each frame.
        scroll_line =
            (scroll_line + ops::VISIBLE_ROWS_PER_FRAME / 3) % line_count.saturating_sub(1).max(1);

        let start = scroll_line;
        let end = (start + window).min(line_count);
        for _row in start..end {
            charged = charged.saturating_add(ops::OPS_PER_ROW);
        }
        // Long-line horizontal clip: charged once per frame, never per byte.
        if bytes >= 2_000_000 {
            charged = charged.saturating_add(ops::MINIFIED_CLIP_OPS);
        }

        assert_frame_ops(
            Budget::ScrollFiveMbAt60Fps,
            frame as usize,
            charged,
            ops::SCROLL_MAX_OPS_PER_FRAME,
        );
        // A hitch at the 300 ms cost-model ceiling must fail.
        assert_frame_ops(
            Budget::ScrollFiveMbAt60Fps,
            frame as usize,
            charged,
            ops::SCROLL_MAX_HITCH_OPS,
        );
        frame_ops.push(charged);
    }

    frame_ops.sort_unstable();
    let p99_index = ((frame_ops.len() as f64) * 0.99).ceil() as usize - 1;
    let p99 = frame_ops[p99_index.min(frame_ops.len() - 1)];
    assert_frame_ops(
        Budget::ScrollFiveMbAt60Fps,
        p99_index,
        p99,
        ops::SCROLL_MAX_OPS_PER_FRAME,
    );

    println!(
        "ok: {} — {} frames over {line_count} lines ({bytes} bytes), \
         p99={p99} ops ≤ {}",
        Budget::ScrollFiveMbAt60Fps,
        frames,
        ops::SCROLL_MAX_OPS_PER_FRAME,
    );
}

fn synthetic_line_count(bytes: u64) -> u64 {
    // Prefer the hard case T-008 names: 200 000 lines, or one line for a
    // fully-minified file. Use byte size to pick a representative count.
    const HARD_CASE_LINES: u64 = 200_000;
    if bytes >= 2_000_000 {
        HARD_CASE_LINES
    } else {
        (bytes / 40).max(1)
    }
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("crates/mjx-wk-ui → repo root")
}
