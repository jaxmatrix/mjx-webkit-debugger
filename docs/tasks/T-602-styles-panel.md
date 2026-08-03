# T — Styles and Computed panels

Phase: 6  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 6+ task

## Goal

Show why a property has the value it has. Done when matched, inherited, and pseudo rules render in cascade order with overridden declarations struck through, and edits apply live.

## Seam

`CssModel`, `MatchedStyles`, `CssRule`, `CssProperty`, and the `StylesPanel` widget.

## Owns

- `crates/mjx-wk-css/src/lib.rs`
- `crates/mjx-wk-ui/src/styles.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/dom-css.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

Keep the losers, not just the winner: the panel's whole value is showing which rule won and which were overridden. Takes a `mjx_wk_source::NodeId`, **never** a type from `mjx-wk-dom`.
