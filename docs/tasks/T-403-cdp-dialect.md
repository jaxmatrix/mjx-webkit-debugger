# T-403 — CdpDialect — encode and decode

**Phase** 4 · **Milestone** v0.4 — Chromium + Android
**Blocked by** v0.1 complete · **Parallel-safe with** T-405 and every v0.4+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-403-cdp-dialect`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Translate between WebKit vocabulary and the Chrome DevTools Protocol. Done when a session driven
entirely through `CdpDialect` reaches the same model state as the WebKit one, for the v0.1 and v0.2
feature set.

## Seam

`CdpDialect::encode` and `::decode`. **The capability table is already frozen** — implement against it; changing it is a seam-change PR.

## Owns

- `crates/mjx-wk-dialect/src/cdp.rs` — `encode`/`decode` only
- `crates/mjx-wk-dialect/tests/cdp.rs`

## Must not touch

- `crates/mjx-wk-dialect/src/webkit.rs` — complete
- the capability tables in `cdp.rs`
- everything outside `crates/mjx-wk-dialect/`

## Fixtures

`fixtures/cdp-attach.jsonl`, recorded from Chromium and replayed with `ReplayTransport::with_dialect`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `Target.*` wrapping ⇄ `sessionId` round-trips;
- `Console.messageAdded` is synthesised from `Runtime.consoleAPICalled` and `Log.entryAdded`;
- paginated `getProperties` is emulated by client-side slicing, and a caller cannot tell;
- a member the table marks `Unsupported` returns `DialectError::Unsupported` **before** hitting the
  wire;
- every translation is covered both directions.


## Notes

Everything above this crate speaks WebKit's vocabulary and must not learn which engine it is
talking to. If a translation is impossible without leaking that upward, the seam is wrong — raise it
rather than working around it.
