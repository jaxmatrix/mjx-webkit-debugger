# T-407 — Cross-engine validation pass

**Phase** 4 · **Milestone** v0.4 — Chromium + Android
**Blocked by** T-403, T-404 · **Parallel-safe with** every other v0.4+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-407-cross-engine-validation`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Prove the abstraction holds. Done when the v0.1 and v0.2 test suites pass against a Chromium
debuggee as well as a WebKit one, and every difference is either translated or honestly reported as
unsupported.

## Seam

No new seam. A test harness that runs existing suites under both dialects.

## Owns

- `crates/mjx-wk-session/tests/cross_engine.rs`
- the CI matrix entry that runs it

## Must not touch

- everything under `crates/mjx-wk-dialect/src/`

## Fixtures

`fixtures/attach.jsonl` and `fixtures/cdp-attach.jsonl` — the same scenario, both engines.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- the source-browser suite passes under both dialects;
- the debugger suite passes under both, except members the table marks `Unsupported`;
- every such exception is **asserted** to be unsupported rather than skipped, so a regression that
  silently drops a feature fails;
- `docs/CHROME-PARITY.md` is updated with anything the pass discovers.


## Notes

The value here is finding where the WebKit-vocabulary abstraction leaks. Expect to find some; the
outcome is either a translation or an honest entry in the parity table, never a special case above
the dialect layer.
