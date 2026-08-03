# T-004 — Script and resource inventory

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-004-source-inventory`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Merge the two feeds that describe what the page loaded into one file list. Done when a page's
scripts, documents and stylesheets appear exactly once each, grouped by origin, and survive a reload
with their ids intact.

Two feeds, arriving at different times in different shapes:

| Feed | Carries | Arrives |
|---|---|---|
| `Debugger.scriptParsed` | scripts, by `scriptId` | streamed, continuously |
| `Page.getResourceTree` | documents, stylesheets, images, by URL | once, on request |

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

- a script named by **both** feeds appears once;
- `SourceId` for a given URL is stable across `on_navigated`;
- an `eval` script with an empty URL still gets a usable `display_name` — a tree of blank rows is
  useless, and these are common;
- the tree groups by origin and sorts stably.

## Notes

`Page.getResourceContent` is keyed by **URL** and needs a `frameId`, so an entry from the resource
tree must carry both. Script ids die on navigation but entries must not, or the user loses their
open tab and their breakpoints on every reload.
