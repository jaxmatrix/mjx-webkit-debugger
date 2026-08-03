# T-205 — Source maps

**Phase** 2 · **Milestone** v0.2 — Debugger
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.2 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-205-source-maps`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Debug the code that was written, not the code that shipped. Done when a breakpoint set in an
authored file binds in the generated one, and a pause in generated code displays in the authored
file.

## Seam

`SourceMapResolver` in `mjx-wk-source`.

## Owns

- `crates/mjx-wk-source/src/maps.rs`
- `crates/mjx-wk-source/tests/maps.rs`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

A fixture page built with a bundler, producing a real map alongside `fixtures/page/`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a `data:` URI map loads without a fetch;
- a relative `sourceMapURL` resolves against the script's URL;
- `to_generated` returns **several** locations where one authored line is inlined in many places,
  and a breakpoint is set at each;
- a map that fails to load is not surfaced as an error — the generated source is still perfectly
  debuggable;
- authored sources appear in the tree marked `is_original`.


## Notes

The seam was frozen in Phase 1a precisely so the code view and breakpoint model could be written
against `SourceLocation` indirection from the start. Retrofitting a mapping step under a UI that
assumed generated positions means touching every panel.
