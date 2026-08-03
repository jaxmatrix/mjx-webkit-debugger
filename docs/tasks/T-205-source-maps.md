# T-205 — Source maps

Phase: 2  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 2+ task

## Goal

Debug the code that was written, not the code that shipped. Done when a breakpoint set in an authored file binds in the generated one, and a pause in generated code displays in the authored file.

## Seam

`SourceMapResolver`.

## Owns

- `crates/mjx-wk-source/src/maps.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

A fixture page built with a bundler, producing a real map.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

`sourceMapURL` may be a `data:` URI carrying the whole map, or relative to the script. A map that fails to load is **not an error the user should see** — the generated source is still perfectly debuggable. `to_generated` returns several locations: one authored line can be inlined in many places, and a breakpoint must be set at each.
