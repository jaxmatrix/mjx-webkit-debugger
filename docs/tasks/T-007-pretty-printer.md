# T-007 — Pretty-printer and position map

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: all others in Phase 1

## Goal

Reformat minified JavaScript and CSS, and map every position both ways. Done when a breakpoint set on a pretty-printed line lands on the right place in the original.

## Seam

`PrettyPrinter`, `PrettyPrinted`.

## Owns

- `crates/mjx-wk-source/src/pretty.rs`
- `crates/mjx-wk-source/tests/pretty.rs`

## Must not touch

- every other file in `crates/mjx-wk-source/src/`

## Fixtures

Golden input/output pairs under `tests/golden/`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- output is semantically identical to input — nothing reordered, nothing dropped;
- string and template-literal contents are byte-identical, including newlines inside them;
- `to_original(to_pretty(p)) == p` for every statement start in the golden files;
- a file that is already formatted is left alone.

## Notes

The mapping is the point, not the formatting. Output nobody can set a breakpoint in is worse than no pretty-printer, because it looks like it works.
