# T-010 — App shell, dock, and wiring

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing (seams only) · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-010-app-shell`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Assemble the application: window, dock, target picker, and the two-thread split. Done when
`mjx-webkit-debugger replay fixtures/attach.jsonl` opens a window showing a real source tree and
code view, with no debuggee running.

## Seam

`App`, `Startup`, `run`, `attach::list`, and `AgentRegistry::register`.

## Owns

- `crates/mjx-webkit-debugger/src/`
- `crates/mjx-wk-session/src/agent.rs` — the `AgentRegistry` impl only

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
- the UI thread never awaits — assert with a debug-build check that panics if it does;
- actions produced in frame N are dispatched **before** snapshots are read in frame N+1, so a click
  is acted on one frame earlier;
- a panel whose members are unsupported renders disabled **with a reason**, not hidden;
- `list` prints targets and exits without opening a window.

## Notes

Can start immediately against `todo!()` bodies and goes green last. Replay mode is not a test
fixture that happens to be useful — it is how the UI is developed and demonstrated before any
transport works.
