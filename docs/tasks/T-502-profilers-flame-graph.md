# T-502 — Script and CPU profilers with a flame graph

**Phase** 5 · **Milestone** v0.5 — Profiling
**Blocked by** T-501 · **Parallel-safe with** every other v0.5+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-502-profilers-flame-graph`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

See where the time went. Done when a sampling profile renders as a flame graph you can drill into,
and frames link back to their source.

## Seam

`FlameFrame`, the `ScriptProfiler`/`CPUProfiler` handling in `ProfileAgent`, and the `FlameGraphView` widget.

## Owns

- `crates/mjx-wk-profile/src/samples.rs` (new)
- `crates/mjx-wk-ui/src/flame.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/timeline-record.jsonl`, which includes a `ScriptProfiler` run.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- self and total sample counts are correct and self ≤ total everywhere;
- the graph virtualises — a deep stack must not lay out every frame;
- clicking a frame opens its source at the right line;
- per-thread CPU data renders alongside without being conflated with JS samples.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Sample counts, not wall-clock, are what the protocol gives. Present them as proportions and label the axis honestly rather than implying millisecond precision that is not there.
