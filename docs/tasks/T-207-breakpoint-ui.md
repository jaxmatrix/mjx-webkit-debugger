# T-207 — Breakpoint list and the condition/action editor

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** T-201, T-008 · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-207-breakpoint-ui`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

The affordances for creating and managing breakpoints. Done when a user can add a condition,
convert a breakpoint to a logpoint or probe, disable one without losing it, and see every breakpoint
in the page at once.

## Seam

`BreakpointList` in `mjx-wk-ui`, plus the gutter context menu in `CodeView`.

## Owns

- `crates/mjx-wk-ui/src/breakpoint_list.rs` (new; add the `mod` line)
- `crates/mjx-wk-ui/src/code_view.rs` — the context menu only

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`
- everything under `crates/mjx-wk-debug/`

## Fixtures

`egui_kittest` snapshots.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- right-click on the gutter offers condition, logpoint, probe and disable;
- the list groups by source and links back to the line;
- a disabled breakpoint stays in the list, visibly disabled;
- a `Probe` action is offered **only** when `supports(Debugger, "setBreakpointByUrl")` reports
  native — over CDP it is unavailable and must not be offered.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

The five gutter states and their colours are specified in `DESIGN.md`; T-008 renders them, this
ticket makes them settable. If you add a state, update `DESIGN.md` in the same commit.
