# T-604 — Styles and Computed panels with live editing

**Phase** 6 · **Milestone** v0.6 — Elements + Styles
**Blocked by** T-602 · **Parallel-safe with** every other v0.6+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-604-styles-panel`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Show and edit the cascade. Done when a declaration can be edited, added, or disabled and the page
updates live.

## Seam

`StylesPanel` and `ComputedPanel`.

## Owns

- `crates/mjx-wk-ui/src/styles.rs`

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`
- everything under `crates/mjx-wk-css/`

## Fixtures

`fixtures/dom-css.jsonl` plus `egui_kittest` snapshots.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- overridden declarations render struck through, with the winning rule identifiable;
- user-agent rules are visually distinct and not editable;
- editing a value applies via `setStyleText` and reverts cleanly on invalid input;
- adding a declaration to a rule works, as does adding a new rule;
- `:hover` and friends can be forced via `forcePseudoState`;
- the Computed panel lists every property with the rule that won.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Editing a stylesheet the debuggee served is a real mutation of the page. Make failure visible: a rejected value must not silently appear to have applied.
