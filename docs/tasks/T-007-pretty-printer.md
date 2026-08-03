# T-007 — Pretty-printer and position map

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-007-pretty-printer`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Reformat minified JavaScript and CSS, and map every position both ways. Done when a breakpoint set
on a pretty-printed line lands on the right place in the original.

## Seam

`PrettyPrinter` and `PrettyPrinted`.

## Owns

- `crates/mjx-wk-source/src/pretty.rs`
- `crates/mjx-wk-source/tests/pretty.rs`
- golden pairs under `crates/mjx-wk-source/tests/golden/`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

Golden input/output pairs; no protocol trace needed.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- output is semantically identical to input — nothing reordered, nothing dropped;
- string and template-literal contents are **byte-identical**, including newlines inside them;
- `to_original(to_pretty(p)) == p` for every statement start in the golden files;
- a file that is already formatted is left alone.

## Notes

The mapping is the point, not the formatting. Output nobody can set a breakpoint in is worse than
no pretty-printer at all, because it looks like it works.
