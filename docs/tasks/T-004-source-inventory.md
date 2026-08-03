# T — Script and resource inventory

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

Merge the two feeds that describe what the page loaded into one file list. Done when a page's scripts, documents, and stylesheets appear exactly once each, grouped by origin, and survive a reload with their ids intact.

## Seam

`SourceInventory`, `SourceEntry`, `SourceTreeNode` in `mjx-wk-source`.

## Owns

- `crates/mjx-wk-source/src/inventory.rs`
- `crates/mjx-wk-source/tests/inventory.rs`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

`fixtures/attach.jsonl` — the `scriptParsed` flood plus a `getResourceTree` reply covering the same page.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a script named by both feeds appears **once**;
- `SourceId` for a given URL is stable across `on_navigated`;
- an `eval` script with an empty URL still gets a usable `display_name`;
- the tree groups by origin and sorts stably.

## Notes

`Page.getResourceContent` is keyed by URL and needs a `frameId`, so an entry from the resource tree must carry both. Script ids die on navigation but entries must not, or the user loses their open tab and breakpoints on every reload.
