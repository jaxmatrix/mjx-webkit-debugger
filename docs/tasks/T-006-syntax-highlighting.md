# T-006 — Tree-sitter highlighting

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-006-syntax-highlighting`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Colour the visible window of a source, incrementally, without ever parsing more than needed. Done
when scrolling a large file stays within the frame budget and a syntax error mid-file does not blank
the rest.

## Seam

`Highlighter`, `HighlightSpan`, `HighlightKind`, `TreeSitterHighlighter`.

## Owns

- `crates/mjx-wk-source/src/highlight.rs`
- `crates/mjx-wk-source/tests/highlight.rs`
- `crates/mjx-wk-source/tests/golden/`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

Golden span files under `tests/golden/`, plus the JavaScript and CSS in `fixtures/page/`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- spans are byte ranges **within their line**, not the file;
- a file with a syntax error still highlights what parses;
- a language with no grammar renders as plain text rather than failing;
- repeated calls for the same window do not re-parse;
- **bench:** highlighting a 100-line window of a 5 MB file stays under 2 ms.

## Notes

Never highlight the whole file — on a 5 MB bundle that costs seconds and is discarded the moment
the user scrolls. `HighlightKind` is semantic; colours come from `Theme`, so this module never
mentions one.
