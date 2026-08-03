# T — Breakpoints, including WebKit actions and probes

Phase: 2  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 2+ task

## Goal

Every breakpoint type Chrome has, plus the four WebKit actions it does not. Done when a breakpoint set by URL survives a reload, resolves to its actual location, and a probe shows live values inline without stopping.

## Seam

`BreakpointStore`, `BreakpointSpec`, `BreakpointState`, `BreakpointAction`, the DOM/event/URL/symbolic breakpoint types, and `impl DomainAgent for DebugAgent`.

## Owns

- `crates/mjx-wk-debug/src/breakpoints.rs`
- `crates/mjx-wk-debug/src/lib.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/breakpoint-hit.jsonl` — set by URL, reload, resolve, hit, inspect, resume.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

Set **by URL, never by script id**, so breakpoints survive reloads. `breakpointResolved` reports the *actual* line, which may differ from the one requested; render requested and resolved differently. `options.actions` with `autoContinue` is what makes a logpoint or probe — see `docs/PROTOCOL-NOTES.md`.
