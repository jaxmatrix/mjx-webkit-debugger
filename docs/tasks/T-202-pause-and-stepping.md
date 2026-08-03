# T-202 — Pause state, call stack, and stepping

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-202-pause-and-stepping`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Show where execution stopped and let the user move it. Done when the call stack renders with async
frames, all six stepping modes work, and blackboxed frames are hidden by default.

## Seam

`PauseState`, `CallFrame`, `Scope`, `StepKind`, `PauseConfig`, `ExceptionPause`, and the `CallStackList` widget.

## Owns

- `crates/mjx-wk-debug/src/pause.rs`
- `crates/mjx-wk-ui/src/call_stack.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/breakpoint-hit.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `PauseState::invalidate` runs on **both** `Debugger.resumed` and `Debugger.globalObjectCleared`;
- selecting a frame re-targets scope display and evaluation;
- async frames render distinctly from synchronous ones;
- blackboxed frames are collapsed behind a "show N more" row;
- `stepNext` and `continueUntilNextRunLoop` are offered when supported and hidden when not.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**Every `objectId` dies on resume.** Failing to invalidate produces stale rows that error when
expanded — the bug looks like a protocol fault and is actually a lifecycle one.

WebKit adds `stepNext` (finer than step-over) and `continueUntilNextRunLoop` (the cleanest way past
a chain of promise callbacks). Neither exists over CDP.
