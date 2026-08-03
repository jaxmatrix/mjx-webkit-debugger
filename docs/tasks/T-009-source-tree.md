# T-009 — Source tree widget

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-009-source-tree`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Navigate a page's sources. Done when ten thousand sources browse smoothly and the tree survives the
inventory changing underneath it.

## Seam

`SourceTree` in `mjx-wk-ui`. Search is T-012.

## Owns

- `crates/mjx-wk-ui/src/source_tree.rs`
- `crates/mjx-wk-ui/tests/source_tree.rs`

## Must not touch

- `crates/mjx-wk-ui/src/search.rs` (T-012)
- `crates/mjx-wk-ui/src/code_view.rs` and `theme.rs` (T-008)

## Fixtures

`fixtures/attach.jsonl` for the inventory, plus a synthetic ten-thousand-source tree.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- expansion state survives the inventory changing — a page loading a script must not collapse the
  folder the user just opened;
- rows are virtualised past a few hundred entries;
- selecting a leaf emits `Action::OpenSource`;
- a group with one child does not render as a pointless nesting level.

## Notes

Expansion state lives in the widget, not the model. That is what makes the previous point possible.
