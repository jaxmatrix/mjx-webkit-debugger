# T-203 — Lazy, paginated variable tree

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-203-variable-tree`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Inspect values in a paused frame without ever fetching more than is on screen. Done when a scope
with fifty thousand properties opens instantly and pages as the user scrolls.

## Seam

`ValueTree`, `ValueNode`, `ValuePreview`, `ValueNodeId`, and the `VariablesTree` widget. Watch expressions render here too.

## Owns

- `crates/mjx-wk-debug/src/values.rs`
- `crates/mjx-wk-ui/src/variables.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/breakpoint-hit.jsonl`, which includes a `getProperties` walk.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- nothing is fetched until a row is expanded;
- expanding pages via `fetchStart`/`fetchCount`, with a "show more" row for the remainder;
- a getter renders as `(…)` and is invoked only on explicit request — invoking it can have side
  effects, so the user must opt in;
- the whole tree is dropped on resume;
- watch expressions re-evaluate on every pause and step.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**`Runtime.getProperties` is paginated on WebKit** — `fetchStart`/`fetchCount`, which CDP does not
have. Ignoring that is how a debugger hangs on a large array. Previews come free from
`generatePreview`: the debuggee builds them while answering.
