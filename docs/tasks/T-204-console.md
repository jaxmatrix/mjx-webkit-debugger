# T-204 — Console panel and evaluation

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-204-console`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Messages in, expressions out. Done when evaluation sees local scope while paused, and a page
logging in a render loop does not push everything else off screen.

## Seam

`ConsoleModel`, `ConsoleMessage`, `MessageLevel`, `MessageSource`, and the `ConsoleView` widget.

## Owns

- `crates/mjx-wk-console/src/lib.rs`
- `crates/mjx-wk-ui/src/console_view.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/attach.jsonl`, which carries the fixture page's `console.log`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- while paused, evaluation routes to `Debugger.evaluateOnCallFrame` for the **selected** frame;
- while running, it routes to `Runtime.evaluate`;
- object arguments stay expandable, reusing T-203's tree;
- repeats fold via `messageRepeatCountUpdated` rather than flooding;
- the log is bounded and reports how many messages were dropped.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Sending everything to `Runtime.evaluate` makes the console useless at exactly the moment it matters
— you stop at a breakpoint and cannot see the local variable you stopped for. Bounding the log and
**saying** what was dropped beats silently losing history.
