# T-701 — Storage, IndexedDB, cookies and workers

**Phase** 7 · **Milestone** v0.7 — Storage, graphics, audits
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.7 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-701-storage-model`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Model what the page has stored. Done when storage areas, databases, cookies and workers are all
enumerable and mutable.

## Seam

`StorageModel`, `DomStorageArea`, `IndexedDbDatabase`, `ObjectStore`, `Cookie`, and `impl DomainAgent for StorageAgent`.

## Owns

- `crates/mjx-wk-storage/src/lib.rs`
- `crates/mjx-wk-storage/tests/storage.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/storage.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- local and session storage enumerate per origin and update live via the four `DOMStorage` events;
- IndexedDB databases, object stores and indexes enumerate, and entries page rather than loading
  whole;
- cookies read and write through **`Page`**, not a `Storage` domain;
- workers and service workers are listed with their target ids.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**WebKit has no `Storage` domain.** Cookies are `Page.getCookies`/`setCookie`/`deleteCookie`. Chrome moved them years ago, so CDP habits mislead here.
