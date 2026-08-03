# T-008 — Theme tokens and the virtualised code view

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-008-code-view`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

The source editor: virtualised, highlighted, breakpoint-aware — the most demanding widget in the
application. Done when a 5 MB bundle scrolls at 60 fps and the gutter renders all five breakpoint
states plus the execution line, each visually distinct.

## Seam

`CodeView`, `CodeViewModel`, `BreakpointMark`, and `Theme::dark`/`Theme::light`.

## Owns

- `crates/mjx-wk-ui/src/code_view.rs`
- `crates/mjx-wk-ui/src/theme.rs`
- `crates/mjx-wk-ui/tests/code_view.rs`

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`

## Fixtures

`egui_kittest` snapshots, plus a synthetic 200 000-line source built in-test.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- only visible rows plus a margin are laid out — assert by counting highlighter calls;
- the scroll area is sized `line_count × row_height` **without measuring text**;
- a single 5 MB line clips horizontally and does **not** wrap — wrapping makes a row millions of
  pixels tall;
- a gutter click emits `Action::ToggleBreakpoint` at the right line;
- all five `BreakpointMark` states are distinguishable in the snapshot;
- the execution line is distinct from every breakpoint state in both shape and colour;
- **bench:** no frame exceeds 16 ms while scrolling the synthetic source.

## Notes

Token values are specified in `DESIGN.md`, which owns the visual contract — **if you change a
token, update that file in the same commit.** The widget holds no session and may not await.
