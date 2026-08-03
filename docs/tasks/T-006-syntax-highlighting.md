# T-006 — Tree-sitter highlighting

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

Colour the visible window of a source, incrementally, without ever parsing more than needed. Done when scrolling a large file stays within the frame budget and a syntax error mid-file does not blank the rest.

## Seam

`Highlighter`, `HighlightSpan`, `HighlightKind`, `TreeSitterHighlighter`.

## Owns

- `crates/mjx-wk-source/src/highlight.rs`
- `crates/mjx-wk-source/tests/highlight.rs`
- `crates/mjx-wk-source/tests/golden/`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

Golden span files under `tests/golden/`, plus the JavaScript in `fixtures/page/app.js`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- spans for a window are byte ranges **within their line**, not the file;
- a file with a syntax error still highlights what parses;
- a language with no grammar renders as plain text rather than failing;
- repeated calls for the same window do not re-parse;
- **bench:** highlighting a 100-line window of a 5 MB file stays under 2 ms.

## Notes

Never highlight the whole file. On a 5 MB bundle that costs seconds and is discarded the moment the user scrolls. `HighlightKind` is semantic; colours come from `Theme`, so this module never mentions one.
