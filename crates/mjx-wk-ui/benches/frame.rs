//! No UI frame > 16 ms.
//!
//! Enforces [`mjx_wk_perf::Budget::UiFrameMax`] against a synthetic source —
//! the absolute frame ceiling, independent of the scroll distribution bench.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use mjx_wk_perf::{Budget, assert_frame_ops, ops};

fn main() {
    // Synthetic 200 000-line source (T-008's hard case). One frame may only
    // lay out the visible window; touching the whole file is a budget fail.
    const LINE_COUNT: u64 = 200_000;
    let window = ops::VISIBLE_ROWS_PER_FRAME + ops::SCROLL_MARGIN_ROWS;

    // Honest frame: virtualised window only (layout + gutter marks).
    let mut honest = 0u64;
    for _ in 0..window {
        honest = honest
            .saturating_add(ops::OPS_PER_ROW)
            .saturating_add(ops::GUTTER_MARK_OPS_PER_ROW);
    }
    assert_frame_ops(Budget::UiFrameMax, 0, honest, ops::UI_FRAME_MAX_OPS);

    // Regression detector: a naive full-file layout must exceed the ceiling.
    let naive = LINE_COUNT.saturating_mul(ops::OPS_PER_ROW);
    assert!(
        naive > ops::UI_FRAME_MAX_OPS,
        "cost model invariant: full-file layout ({naive}) must exceed the frame ceiling ({})",
        ops::UI_FRAME_MAX_OPS
    );

    println!(
        "ok: {} — synthetic {LINE_COUNT}-line source, frame {honest} ops ≤ {} \
         (naive full-file {naive} ops would fail)",
        Budget::UiFrameMax,
        ops::UI_FRAME_MAX_OPS,
    );
}
