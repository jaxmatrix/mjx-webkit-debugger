# T-601 — DOM tree

Phase: 6  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 6+ task

## Goal

Browse and edit the live document. Done when the tree stays correct through mutation without losing the user's expansion state, and the element picker works.

## Seam

`DomModel`, `DomNode`, `OverlayConfig`, and the `DomTreeView` widget.

## Owns

- `crates/mjx-wk-dom/src/lib.rs`
- `crates/mjx-wk-ui/src/dom_tree.rs`

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

The largest domain in the protocol, 78 members. The tree arrives in pieces and mutates under you via six separate events; **rebuilding on each change loses expansion and scroll state**, so apply every one incrementally. `children: None` (not requested) is distinct from `Some(vec![])` (genuinely none).
