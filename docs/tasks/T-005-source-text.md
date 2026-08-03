# T-005 — Source text and line index

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-005-source-text`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Make "what is on line N?" O(1) on a file of any size. Done when a 5 MB bundle indexes and randomly
accesses within the frame budget.

The hard case is not a 200-line module — it is a 5 MB minified bundle on one line, or a
200 000-line vendor file the user scrolls. The code view asks "what is on line 48 217?" sixty times
a second, so that lookup must be O(1) and must not allocate.

## Seam

`SourceText` and `LineIndex` in `mjx-wk-source`. **Not** `SourceStore` — that is T-011.

## Owns

- `crates/mjx-wk-source/src/text.rs`
- `crates/mjx-wk-source/benches/text.rs`

## Must not touch

- `crates/mjx-wk-source/src/store.rs` (T-011)
- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/large-bundle.jsonl` — a multi-megabyte `getScriptSource` reply, and the bench input.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `\n`, `\r\n`, and a final line with no terminator all index correctly;
- a trailing newline does **not** produce a phantom last line;
- `offset_of` treats columns as **UTF-16 code units**, verified with an emoji and a non-Latin
  identifier;
- `looks_minified` keys on mean line length, not file size;
- **bench:** indexing 5 MB stays within the budget in `CLAUDE.md`.

## Notes

Columns are UTF-16 on the wire — JavaScript's string model. Treating them as bytes puts breakpoints
on the wrong character in any file containing an emoji. This is the single most common source of
subtle debugger bugs, and it is invisible until someone debugs a file with a non-ASCII identifier.
