# T — Source text, line index, fetch and cache

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

Make "what is on line N?" O(1) on a file of any size, and fetch text without ever blocking the UI. Done when a 5 MB bundle can be indexed and randomly accessed within the frame budget.

## Seam

`SourceText`, `LineIndex`, `SourceStore` in `mjx-wk-source`.

## Owns

- `crates/mjx-wk-source/src/text.rs`
- `crates/mjx-wk-source/src/store.rs`
- `crates/mjx-wk-source/benches/text.rs`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/large-bundle.jsonl` — a multi-megabyte `getScriptSource` reply, and the perf bench input.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `\n`, `\r\n`, and a final line with no terminator all index correctly;
- a trailing newline does **not** produce a phantom last line;
- `offset_of` treats columns as UTF-16 code units, verified with an emoji and a non-Latin identifier;
- two concurrent `text()` calls for the same id issue **one** request;
- the cache evicts by bytes, not entries;
- **bench:** indexing 5 MB stays under the budget in `CLAUDE.md`.

## Notes

Columns are UTF-16 on the wire — JavaScript's string model. Treating them as bytes puts breakpoints on the wrong character in any file with an emoji. This is the single most common source of subtle debugger bugs.
