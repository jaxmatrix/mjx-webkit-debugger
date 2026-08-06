# T-011 — Source store — fetch, cache, request dedup

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-011-source-store`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Fetch source text without ever blocking the UI, and never fetch the same thing twice. Done when the
tree and the editor can ask for a 5 MB bundle at the same moment and only one request goes out.

Two protocol paths, chosen by `SourceKind`:

- scripts → `Debugger.getScriptSource { scriptId }`
- documents and stylesheets → `Page.getResourceContent { frameId, url }`, which may return base64

## Seam

`SourceStore` in `mjx-wk-source`. Split out of T-005, which owns `SourceText`/`LineIndex`.

`SourceStore::text` takes `&SourceEntry` (not a bare `SourceId`): the fetch path must choose
`Debugger.getScriptSource` vs `Page.getResourceContent` from `kind` / `script_id` / `frame` /
`url`. The cache stays keyed by `entry.id`.

## Owns

- `crates/mjx-wk-source/src/store.rs`
- `crates/mjx-wk-source/tests/store.rs`

## Must not touch

- `crates/mjx-wk-source/src/text.rs` (T-005)
- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/large-bundle.jsonl` and `fixtures/attach.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- two concurrent `text()` calls for the same id issue **one** request;
- the cache evicts by **bytes**, not entry count — entry sizes here differ by four orders of
  magnitude;
- `cached()` never blocks, so the UI thread may call it;
- base64 content from `getResourceContent` is decoded;
- binary content reports `SourceError::NotText` rather than rendering as mojibake;
- `clear()` on navigation drops everything — script ids are reissued, so stale text would otherwise
  be served under a new script's id.

## Notes

Neither reply may be awaited on the UI thread. A 5 MB response is parsed and line-indexed on the
session side before the UI is handed a pointer to it.
