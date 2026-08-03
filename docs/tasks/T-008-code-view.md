# T-008 — The virtualised code view

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

The source editor: virtualised, highlighted, breakpoint-aware. Done when a 5 MB bundle scrolls at 60 fps and the gutter renders all five breakpoint states plus the execution line.

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

- only visible rows plus a margin are laid out, asserted by counting highlighter calls;
- the scroll area is sized `line_count × row_height` without measuring text;
- a single 5 MB line clips horizontally and does **not** wrap;
- gutter click emits `Action::ToggleBreakpoint` at the right line;
- all five `BreakpointMark` states are visually distinct in the snapshot;
- the execution line is distinct from every breakpoint state;
- **bench:** no frame exceeds 16 ms while scrolling the synthetic source.

## Notes

Token values are specified in `DESIGN.md`, which owns the visual contract — if you change a token, update that file in the same commit. The widget holds no session and may not await.
