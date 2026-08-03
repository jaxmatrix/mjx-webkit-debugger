# T — App shell, dock, and wiring

Phase: 1  ·  Depends on: none (seams only)  ·  Parallel-safe with: all others in Phase 1

## Goal

Assemble the application: window, dock, target picker, and the two-thread split. Done when `mjx-webkit-debugger replay fixtures/attach.jsonl` opens a window showing a real source tree and code view, with no debuggee.

## Seam

`App`, `Startup`, `run`, `attach::list`, and `AgentRegistry::register`.

## Owns

- `crates/mjx-webkit-debugger/src/`
- `crates/mjx-wk-session/src/agent.rs` (the `AgentRegistry` impl only)

## Must not touch

- everything under `crates/mjx-wk-ui/src/` and `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/attach.jsonl`, driven end to end through replay mode.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `replay` mode renders a populated source tree and code view;
- the UI thread never awaits — asserted by a debug-build check that panics if it does;
- actions produced in frame N are dispatched before snapshots are read in frame N+1;
- a panel whose members are unsupported renders disabled **with a reason**, not hidden;
- `list` prints targets and exits without opening a window.

## Notes

Can start immediately against `todo!()` bodies and go green last. Replay mode is not a test fixture that happens to be useful — it is how the UI is developed and demonstrated before any transport works.
