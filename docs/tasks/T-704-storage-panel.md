# T-704 — Storage panel

**Phase** 7 · **Milestone** v0.7 — Storage, graphics, audits
**Blocked by** T-701 · **Parallel-safe with** every other v0.7 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-704-storage-panel`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Browse and edit stored data. Done when every storage kind is inspectable and editable from one
panel.

## Seam

`StorageTable` in `mjx-wk-ui`.

## Owns

- `crates/mjx-wk-ui/src/storage_table.rs`

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`
- everything under `crates/mjx-wk-storage/`

## Fixtures

`fixtures/storage.jsonl` plus snapshots.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a key/value pair can be edited, added and deleted, and the change reaches the page;
- IndexedDB entries page as the user scrolls;
- a cookie's flags render and can be changed;
- clearing a storage area asks first — it is destructive and irreversible in the page.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

This panel mutates the debuggee's persistent state. Confirmation on destructive actions is a correctness requirement, not a nicety.
