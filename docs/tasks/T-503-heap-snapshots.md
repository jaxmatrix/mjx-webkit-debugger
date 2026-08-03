# T-503 — Heap snapshots and retaining paths

**Phase** 5 · **Milestone** v0.5 — Profiling
**Blocked by** T-501 · **Parallel-safe with** every other v0.5+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-503-heap-snapshots`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Answer "why has this not been collected?". Done when a snapshot can be taken, browsed by class,
and any object's retaining path shown.

## Seam

`HeapNode`, snapshot parsing, and the heap view.

## Owns

- `crates/mjx-wk-profile/src/heap.rs` (new)
- `crates/mjx-wk-ui/src/heap_view.rs` (new)

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

A recorded `Heap.snapshot` reply — snapshots are large, so record one and keep it.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a snapshot parses without loading the whole graph into the UI at once;
- objects group by class with counts and retained size;
- the retaining path to a selected object is computed and displayed;
- `Heap.garbageCollected` events annotate the timeline.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

The retaining path is the entire point. A snapshot browser without one is a class histogram, which nobody needs a debugger for.
