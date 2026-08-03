# T-012 — Search — index and bar

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-012-search`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Find a string across every source. Done when local results appear as you type and remote results
merge in without moving what you are about to click.

Two strategies, and the choice matters. `Page.searchInResources` searches everything the debuggee
knows without transferring it — right for a large site. Local search over the cache is instant and
works offline — right for what is already open. Do both: local first for immediate feedback, remote
to fill in the rest.

## Seam

`SearchIndex`, `SearchQuery`, `SearchHit` in `mjx-wk-source`; `SearchBar` in `mjx-wk-ui`.

## Owns

- `crates/mjx-wk-source/src/search.rs`
- `crates/mjx-wk-ui/src/search.rs`
- `crates/mjx-wk-ui/tests/search.rs`

## Must not touch

- `crates/mjx-wk-ui/src/source_tree.rs` (T-009)
- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/attach.jsonl`, plus cached sources built in-test.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- local results appear without awaiting anything;
- merging remote results does not reorder those already shown;
- a hit in a minified source **truncates its line** rather than rendering megabytes;
- regex and case-sensitive modes both work, and an invalid regex is reported rather than panicking.

## Notes

A "line" in a minified bundle can be the whole file. Every display path must assume that.
