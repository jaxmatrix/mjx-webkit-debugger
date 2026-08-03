# T-014 — Perf bench harness

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-014-perf-benches`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Make the performance budgets real. `CLAUDE.md` commits to four, "asserted by benches in CI" —
nothing currently asserts them, so they are aspirations. Done when a regression past any of them
fails a build.

| Budget | Why |
|---|---|
| attach → source tree visible < 300 ms | the first thing anyone does |
| 5 MB minified bundle scrolls at 60 fps | the case that breaks naive editors |
| pause → first variable row < 100 ms | a debugger that stutters at a breakpoint is unusable |
| no UI frame > 16 ms | the non-negotiable one |

## Seam

No seam. A harness the other tickets' benches plug into.

## Owns

- `crates/*/benches/`
- `xtask/src/bench.rs` if a runner is needed
- the bench job in `.github/workflows/ci.yml`

## Must not touch

- any `src/` file — this ticket adds benches, it does not change behaviour

## Fixtures

`fixtures/large-bundle.jsonl` for the source budgets; a synthetic source for the frame budget.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- each budget is a named bench that **fails** rather than merely reporting;
- thresholds live in one place, not scattered across bench files;
- CI runs them on a consistent runner, and a slow runner does not produce false failures — prefer
  operation counts or allocation counts where wall-clock is too noisy;
- `CLAUDE.md`'s budget table links to the benches that enforce it.

## Notes

Measure what the budget actually claims. "60 fps while scrolling" is a frame-time distribution, not
a mean — assert the 99th percentile, or a single 300 ms hitch passes.
