# T-206 — DOM, event, URL and symbolic breakpoints

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-206-dom-debugger-breakpoints`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

The breakpoint kinds that are not tied to a source line. Done when each fires and reports why
execution stopped.

| Kind | Member | Chrome calls it |
|---|---|---|
| subtree / attribute / node-removal | `DOMDebugger.setDOMBreakpoint` | DOM change breakpoints |
| event listener, by name or category | `DOMDebugger.setEventBreakpoint` | Event listener breakpoints |
| request URL substring or regex | `DOMDebugger.setURLBreakpoint` | XHR/fetch breakpoints |
| function by name | `Debugger.addSymbolicBreakpoint` | `debug(fn)` |

## Seam

`DomBreakpoint`, `EventBreakpoint`, `UrlBreakpoint`, `SymbolicBreakpoint`, and their handling in `DebugAgent`.

## Owns

- `crates/mjx-wk-debug/src/breakpoints.rs` — the non-line kinds only
- `crates/mjx-wk-debug/tests/dom_debugger.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/breakpoint-hit.jsonl`, extended with a DOM and an event breakpoint.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- each kind sets and removes cleanly;
- a pause reports `PauseReason::Instrumentation` carrying which breakpoint fired — "why did it
  stop?" is the only question that matters at that moment;
- a symbolic breakpoint honours its regex and case options;
- a DOM breakpoint on a removed node is cleaned up rather than left dangling.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Coordinate with T-201 on `crates/mjx-wk-debug/src/breakpoints.rs`: T-201 owns the line-breakpoint
types and the store, this ticket owns the four non-line types. If that split proves awkward in
practice, say so in the PR rather than reaching into the other ticket's types.
