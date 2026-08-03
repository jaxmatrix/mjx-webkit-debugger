# T-202 — Pause state, call stack, and stepping

Phase: 2  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 2+ task

## Goal

Show where execution stopped and let the user move it. Done when the call stack renders with async frames, stepping works in all six modes, and blackboxed frames are hidden by default.

## Seam

`PauseState`, `CallFrame`, `Scope`, `StepKind`, `PauseConfig`, and the `CallStackList` widget.

## Owns

- `crates/mjx-wk-debug/src/pause.rs`
- `crates/mjx-wk-ui/src/call_stack.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/breakpoint-hit.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

**`PauseState::invalidate` must be called on `Debugger.resumed` and `Debugger.globalObjectCleared`.** Every `objectId` dies on resume, and stale ones produce confusing protocol errors when the user expands a row. WebKit adds `stepNext` and `continueUntilNextRunLoop`; the latter is the cleanest way past a promise chain.
