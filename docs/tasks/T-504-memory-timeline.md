# T-504 — Memory timeline and the performance panel shell

**Phase** 5 · **Milestone** v0.5 — Profiling
**Blocked by** T-501 · **Parallel-safe with** every other v0.5+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-504-memory-timeline`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Tie the profiling views together. Done when a single panel hosts the timeline, flame graph, memory
graph and heap views against one shared time axis, with one set of recording controls.

## Seam

`MemoryTimeline` and the `TimelineRuler` widget; the panel that composes v0.5's views.

## Owns

- `crates/mjx-wk-profile/src/memory.rs` (new)
- `crates/mjx-wk-ui/src/timeline_ruler.rs` (new)

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/timeline-record.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- memory renders by category over time;
- the ruler is shared: zooming or selecting a range applies to every view at once;
- recording controls start and stop the selected instruments together;
- selecting a range filters the flame graph to it.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

The shared axis is what makes the panel more than four separate tools in one window. Build the ruler as the owner of the range, and have the views read it.
