# T — Timeline, profiles, and heap

Phase: 5  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 5+ task

## Goal

Find out where the time and memory went. Done when a recording produces a navigable timeline, a flame graph, and a heap snapshot with retaining paths.

## Seam

`ProfileModel`, `TimelineRecord`, `FlameFrame`, `HeapNode`, and the `FlameGraphView` widget.

## Owns

- `crates/mjx-wk-profile/src/lib.rs`
- `crates/mjx-wk-ui/src/flame.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/timeline-record.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

**WebKit has no `Profiler` domain.** Five domains start and stop independently, which is why the model tracks instruments rather than one recording flag. The retaining path is the answer to "why has this not been collected" — the only question a heap snapshot is really asked.
