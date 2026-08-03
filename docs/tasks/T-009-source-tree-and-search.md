# T — Source tree and search widgets

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

Navigate and search a page's sources. Done when ten thousand sources browse smoothly and search shows local results as you type while remote ones merge in without moving what you are about to click.

## Seam

`SourceTree`, `SearchBar`, and `SearchIndex` in `mjx-wk-source`.

## Owns

- `crates/mjx-wk-ui/src/source_tree.rs`
- `crates/mjx-wk-ui/src/search.rs`
- `crates/mjx-wk-source/src/search.rs`
- `crates/mjx-wk-ui/tests/source_tree.rs`

## Must not touch

- `crates/mjx-wk-ui/src/code_view.rs` and `theme.rs` (T-008)
- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/attach.jsonl` for the inventory, plus a synthetic ten-thousand-source tree.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- expansion state survives the inventory changing underneath;
- rows are virtualised past a few hundred entries;
- local results appear without awaiting anything;
- merging remote results does not reorder those already shown;
- a hit in a minified source truncates its line rather than rendering megabytes.

## Notes

Expansion state lives in the widget, not the model: a page loading a script must not collapse the folder the user just opened.
