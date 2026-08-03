# T-701 — Storage and application

Phase: 7  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 7+ task

## Goal

Inspect and edit what the page has stored. Done when storage areas, IndexedDB stores, and cookies are all browsable and editable.

## Seam

`StorageModel`, `DomStorageArea`, `IndexedDbDatabase`, `Cookie`, and the `StorageTable` widget.

## Owns

- `crates/mjx-wk-storage/src/lib.rs`
- `crates/mjx-wk-ui/src/storage_table.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/storage.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

**WebKit has no `Storage` domain** — cookies are on `Page`. Chrome moved them years ago, so this is another place CDP muscle memory misleads.
