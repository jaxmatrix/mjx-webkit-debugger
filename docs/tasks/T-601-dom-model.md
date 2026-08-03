# T-601 — DOM tree model with incremental mutation

**Phase** 6 · **Milestone** v0.6 — Elements + Styles
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.6+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-601-dom-model`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Track the live document. Done when the tree stays correct through arbitrary mutation without ever
being rebuilt from scratch.

`DOM` is the largest domain in the protocol at 78 members.

## Seam

`DomNode`, `DomModel`, and `impl DomainAgent for DomAgent`.

## Owns

- `crates/mjx-wk-dom/src/lib.rs`
- `crates/mjx-wk-dom/tests/mutation.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/dom-css.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `getDocument` then `requestChildNodes`/`setChildNodes` builds the tree lazily;
- all six mutation events apply **incrementally**: `childNodeInserted`, `childNodeRemoved`,
  `attributeModified`, `attributeRemoved`, `characterDataModified`, `childNodeCountUpdated`;
- `children: None` (not requested) stays distinct from `Some(vec![])` (genuinely none);
- shadow roots and pseudo-elements are represented;
- `documentUpdated` resets cleanly without leaking node ids.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Rebuilding on each change loses the user's expansion state and scroll position. On a page that
mutates continuously — which is most of them — that makes the panel unusable, and it is the single
thing most likely to be got wrong here.
