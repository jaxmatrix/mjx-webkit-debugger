# T-603 — DOM tree widget, element picker, and overlays

**Phase** 6 · **Milestone** v0.6 — Elements + Styles
**Blocked by** T-601 · **Parallel-safe with** every other v0.6+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-603-dom-widget`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Browse and select elements. Done when the tree renders a large document smoothly, the picker
selects by clicking in the page, and overlays show layout.

## Seam

`DomTreeView` and `OverlayConfig` handling.

## Owns

- `crates/mjx-wk-ui/src/dom_tree.rs`

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`
- everything under `crates/mjx-wk-dom/`

## Fixtures

`fixtures/dom-css.jsonl` plus `egui_kittest` snapshots.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- rows virtualise; a node with 10 000 children does not lay them all out;
- hovering a row highlights the node in the page, and clearing the hover clears the highlight;
- the picker arms via `setInspectModeEnabled` and selects on click;
- grid and flex overlays toggle via `showGridOverlay`/`showFlexOverlay`;
- editing as HTML round-trips through `setOuterHTML`.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

A highlight left behind when the debugger loses focus is a page the user cannot see properly. Clear overlays on detach as well as on hover-out.
