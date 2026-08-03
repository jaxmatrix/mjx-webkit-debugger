# T — Lazy, paginated variable tree

Phase: 2  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 2+ task

## Goal

Inspect values in a paused frame without ever fetching more than is on screen. Done when a scope with fifty thousand properties opens instantly and pages as the user scrolls.

## Seam

`ValueTree`, `ValueNode`, `ValuePreview`, and the `VariablesTree` widget.

## Owns

- `crates/mjx-wk-debug/src/values.rs`
- `crates/mjx-wk-ui/src/variables.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/breakpoint-hit.jsonl`, which includes a `getProperties` walk.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

**`Runtime.getProperties` is paginated on WebKit** — `fetchStart`/`fetchCount`, which CDP lacks. Asking for everything is how a debugger hangs on a large array. A getter is shown as `(…)` and invoked only on request, because invoking it can have side effects.
