# T — Console panel and evaluation

Phase: 2  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 2+ task

## Goal

Messages in, expressions out. Done when evaluation sees local scope while paused, and a page logging in a render loop does not push everything else off screen.

## Seam

`ConsoleModel`, `ConsoleMessage`, and the `ConsoleView` widget.

## Owns

- `crates/mjx-wk-console/src/lib.rs`
- `crates/mjx-wk-ui/src/console_view.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/attach.jsonl`, which carries the fixture page's `console.log`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

**Route to `Debugger.evaluateOnCallFrame` when paused**, `Runtime.evaluate` otherwise. Sending everything to `Runtime.evaluate` makes the console useless at exactly the moment it matters. The log is bounded; report what was dropped rather than silently losing it.
