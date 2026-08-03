# T-501 — Timeline domain and the record tree

**Phase** 5 · **Milestone** v0.5 — Profiling
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.5+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-501-timeline`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Record what the engine did, and when. Done when a recording produces a navigable tree of timeline
records anchored to a shared time axis.

## Seam

`Instrument`, `TimelineRecord`, `RecordingSession`, and `impl DomainAgent for ProfileAgent`.

## Owns

- `crates/mjx-wk-profile/src/lib.rs`
- `crates/mjx-wk-profile/src/timeline.rs` (new)

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

- `Timeline.start`/`stop` bracket a recording and `eventRecorded` builds a correct tree;
- nested records nest, and an unterminated record at stop is closed rather than dropped;
- records carry a `SourceLocation` where the protocol supplies one, so they link into the code view;
- instruments start and stop **independently** — the model tracks a set, not a flag.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**WebKit has no `Profiler` domain.** Five domains cover what Chrome puts in two, and they are
independently controllable. That is why `ProfileModel::recording` is a `Vec<Instrument>`.
