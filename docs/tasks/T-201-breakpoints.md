# T-201 — Breakpoint store and the Debugger agent

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-201-breakpoints`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Set, resolve, and track breakpoints. Done when a breakpoint set by URL survives a reload, resolves
to its actual location, and hits — including as a logpoint or probe that never stops.

Covers line, conditional, logpoint and probe breakpoints via `Debugger.setBreakpointByUrl`.
`DOMDebugger` kinds are T-206; the editing UI is T-207.

## Seam

`BreakpointStore`, `BreakpointSpec`, `BreakpointState`, `BreakpointAction`, and `impl DomainAgent for DebugAgent`.

## Owns

- `crates/mjx-wk-debug/src/breakpoints.rs`
- `crates/mjx-wk-debug/src/lib.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/breakpoint-hit.jsonl` — set by URL, reload, resolve, hit, resume.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a breakpoint set before the script parses reports `Pending`, then `Resolved` on
  `breakpointResolved`;
- the **actual** location is recorded when it differs from the requested one;
- a breakpoint survives `Page.reload` without being re-sent;
- `autoContinue` with a `Log` action never pauses;
- `ignore_count` skips exactly that many hits;
- `detach` releases every remote object group we opened.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**Set by URL, never by script id** — that is what makes breakpoints survive reloads.
`breakpointResolved` reports the *actual* line, which may differ: a breakpoint on a blank line moves
to the next statement, and the UI must show requested and resolved differently.

`options.actions` with `autoContinue` is what turns a breakpoint into a logpoint or probe. WebKit
has four action kinds where Chrome has one — see `docs/PROTOCOL-NOTES.md`.
