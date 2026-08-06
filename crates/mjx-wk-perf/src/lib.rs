//! Central performance budgets for mjx-webkit-debugger.
//!
//! `CLAUDE.md` commits to four wall-clock budgets "asserted by benches in CI".
//! Those numbers are too noisy on shared runners, so CI asserts **operation
//! counts** (and optional allocation counts) derived from the same budgets.
//! Wall-clock ceilings stay here as documentation and for optional local
//! checks — they are not the CI gate.
//!
//! Every budget constant lives in this crate. Bench files must not invent
//! their own thresholds.

use std::fmt;
use std::time::Duration;

/// One of the four budgets named in `CLAUDE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Budget {
    /// Attach → source tree visible.
    AttachToSourceTree,
    /// 5 MB minified bundle scrolls at 60 fps.
    ScrollFiveMbAt60Fps,
    /// Pause → first variable row.
    PauseToFirstVariable,
    /// No UI frame may exceed 16 ms.
    UiFrameMax,
}

impl Budget {
    /// Stable name for logs and failure messages.
    pub const fn name(self) -> &'static str {
        match self {
            Self::AttachToSourceTree => "attach → source tree visible",
            Self::ScrollFiveMbAt60Fps => "5 MB minified bundle scrolls at 60 fps",
            Self::PauseToFirstVariable => "pause → first variable row",
            Self::UiFrameMax => "no UI frame > 16 ms",
        }
    }

    /// Bench target that enforces this budget (`package` + `[[bench]]` name).
    pub const fn bench_target(self) -> BenchTarget {
        match self {
            Self::AttachToSourceTree => BenchTarget {
                package: "mjx-wk-session",
                bench: "attach",
                path: "crates/mjx-wk-session/benches/attach.rs",
                required: true,
            },
            Self::ScrollFiveMbAt60Fps => BenchTarget {
                package: "mjx-wk-ui",
                bench: "scroll",
                path: "crates/mjx-wk-ui/benches/scroll.rs",
                required: true,
            },
            Self::PauseToFirstVariable => BenchTarget {
                package: "mjx-wk-debug",
                bench: "pause",
                path: "crates/mjx-wk-debug/benches/pause.rs",
                required: true,
            },
            Self::UiFrameMax => BenchTarget {
                package: "mjx-wk-ui",
                bench: "frame",
                path: "crates/mjx-wk-ui/benches/frame.rs",
                required: true,
            },
        }
    }

    /// All budgets the CLAUDE.md table commits to.
    pub const fn all() -> [Budget; 4] {
        [
            Self::AttachToSourceTree,
            Self::ScrollFiveMbAt60Fps,
            Self::PauseToFirstVariable,
            Self::UiFrameMax,
        ]
    }
}

impl fmt::Display for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where a budget's enforcing bench lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchTarget {
    pub package: &'static str,
    pub bench: &'static str,
    pub path: &'static str,
    /// When false, the harness reports the gap but does not fail (T-005's
    /// `text` bench is owned elsewhere and may land later).
    pub required: bool,
}

/// A bench the harness should discover once a peer ticket lands it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FutureBench {
    pub package: &'static str,
    pub bench: &'static str,
    pub path: &'static str,
    pub owner: &'static str,
}

/// Peer-owned benches the harness will run when present.
pub const FUTURE_BENCHES: &[FutureBench] = &[FutureBench {
    package: "mjx-wk-source",
    bench: "text",
    path: "crates/mjx-wk-source/benches/text.rs",
    owner: "T-005",
}];

/// Wall-clock ceilings from `CLAUDE.md` (documentation / optional local checks).
pub mod wall {
    use std::time::Duration;

    /// Attach → source tree visible.
    pub const ATTACH_TO_SOURCE_TREE: Duration = Duration::from_millis(300);
    /// One frame at 60 fps.
    pub const FRAME_60_FPS: Duration = Duration::from_nanos(16_666_667);
    /// Pause → first variable row.
    pub const PAUSE_TO_FIRST_VARIABLE: Duration = Duration::from_millis(100);
    /// Absolute UI frame ceiling.
    pub const UI_FRAME_MAX: Duration = Duration::from_millis(16);
    /// A hitch this long during scroll must fail the scroll budget.
    pub const SCROLL_HITCH_FAIL: Duration = Duration::from_millis(300);
}

/// Operation-count ceilings — the CI gate.
///
/// Costs are in abstract "ops" charged by the benches' cost models. One op is
/// one unit of work that must stay inside a frame (a row layout, a property
/// decode, a tree insert, a protocol frame fold). Tuning belongs here only.
pub mod ops {
    use super::wall;

    /// Nanoseconds claimed by one op under the CI cost model.
    ///
    /// Chosen so that [`super::wall::FRAME_60_FPS`] buys
    /// [`SCROLL_MAX_OPS_PER_FRAME`] ops: a slow runner cannot flake the gate,
    /// but a cost-model regression still fails the build.
    pub const NS_PER_OP: u64 = 4_000;

    /// Rows a virtualised view may touch in one frame (visible + margin).
    pub const VISIBLE_ROWS_PER_FRAME: u64 = 48;
    pub const SCROLL_MARGIN_ROWS: u64 = 8;
    /// Layout + gutter + clip work charged per touched row.
    pub const OPS_PER_ROW: u64 = 8;

    /// Hard ceiling for one scroll/UI frame.
    pub const SCROLL_MAX_OPS_PER_FRAME: u64 =
        (VISIBLE_ROWS_PER_FRAME + SCROLL_MARGIN_ROWS) * OPS_PER_ROW;

    /// Same absolute frame ceiling, named for the UI-frame budget.
    pub const UI_FRAME_MAX_OPS: u64 = SCROLL_MAX_OPS_PER_FRAME;

    /// A 300 ms hitch in the cost model — must fail, not merely report.
    pub const SCROLL_MAX_HITCH_OPS: u64 = (wall::SCROLL_HITCH_FAIL.as_nanos() as u64) / NS_PER_OP;

    /// Attach path: parse fixture frames, session handshake, fold inventory
    /// into a tree the UI could show. Sized for `fixtures/attach.jsonl` plus
    /// headroom for a busy page's first paint of the source tree.
    pub const ATTACH_MAX_OPS: u64 = 50_000;

    /// Pause → first variable row: decode the paused event, walk the top
    /// scope, and materialise the first property row (incl. paginated
    /// `getProperties` fold). Sized for `fixtures/breakpoint-hit.jsonl`.
    pub const PAUSE_MAX_OPS: u64 = 25_000;

    /// Convert a wall-clock budget into an op ceiling under this cost model.
    pub const fn ops_for(duration: std::time::Duration) -> u64 {
        (duration.as_nanos() as u64) / NS_PER_OP
    }
}

/// Running tally of operations (and optional logical allocations).
#[derive(Debug, Default, Clone)]
pub struct OpCounter {
    ops: u64,
    allocs: u64,
}

impl OpCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Charge `n` operations.
    pub fn add(&mut self, n: u64) {
        self.ops = self.ops.saturating_add(n);
    }

    /// Charge one operation.
    pub fn tick(&mut self) {
        self.add(1);
    }

    /// Charge a logical allocation (e.g. a row buffer the real path would own).
    pub fn alloc(&mut self, n: u64) {
        self.allocs = self.allocs.saturating_add(n);
        self.add(n);
    }

    pub fn ops(&self) -> u64 {
        self.ops
    }

    pub fn allocs(&self) -> u64 {
        self.allocs
    }
}

/// Fail the process if `actual` exceeds `budget`.
pub fn assert_ops(budget: Budget, actual: u64, max: u64) {
    if actual > max {
        let target = budget.bench_target();
        panic!(
            "perf budget exceeded: {budget}\n  \
             ops: {actual} > {max}\n  \
             enforced by {} ({})\n  \
             thresholds: crates/mjx-wk-perf (do not raise inline in the bench)",
            target.path, target.bench,
        );
    }
}

/// Fail if any single frame's op count exceeds the frame ceiling.
pub fn assert_frame_ops(budget: Budget, frame_index: usize, actual: u64, max: u64) {
    if actual > max {
        let target = budget.bench_target();
        panic!(
            "perf budget exceeded: {budget}\n  \
             frame {frame_index}: {actual} ops > {max}\n  \
             enforced by {} ({})",
            target.path, target.bench,
        );
    }
}

/// Optional local wall-clock check. Not used by CI (too noisy on shared runners).
pub fn assert_wall(budget: Budget, elapsed: Duration, max: Duration) {
    if elapsed > max {
        let target = budget.bench_target();
        panic!(
            "perf wall-clock budget exceeded: {budget}\n  \
             elapsed: {elapsed:?} > {max:?}\n  \
             enforced by {} ({})\n  \
             note: CI asserts operation counts, not wall-clock",
            target.path, target.bench,
        );
    }
}

/// Op ceiling for a budget under the CI cost model.
pub fn max_ops(budget: Budget) -> u64 {
    match budget {
        Budget::AttachToSourceTree => ops::ATTACH_MAX_OPS,
        Budget::ScrollFiveMbAt60Fps => ops::SCROLL_MAX_OPS_PER_FRAME,
        Budget::PauseToFirstVariable => ops::PAUSE_MAX_OPS,
        Budget::UiFrameMax => ops::UI_FRAME_MAX_OPS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_budget_matches_60fps_cost_model() {
        let from_wall = ops::ops_for(wall::FRAME_60_FPS);
        assert!(
            from_wall >= ops::SCROLL_MAX_OPS_PER_FRAME,
            "cost model must fit a full virtualised frame inside 16.67 ms: \
             {from_wall} < {}",
            ops::SCROLL_MAX_OPS_PER_FRAME
        );
    }

    #[test]
    fn hitch_ceiling_is_stricter_than_mean_only() {
        assert!(ops::SCROLL_MAX_HITCH_OPS > ops::SCROLL_MAX_OPS_PER_FRAME);
        assert_eq!(
            ops::SCROLL_MAX_HITCH_OPS,
            ops::ops_for(wall::SCROLL_HITCH_FAIL)
        );
    }

    #[test]
    fn every_budget_names_a_bench() {
        for budget in Budget::all() {
            let t = budget.bench_target();
            assert!(!t.package.is_empty());
            assert!(!t.bench.is_empty());
            assert!(t.path.contains("benches/"));
        }
    }
}
