# T-404 — CdpTransport — WebView2 and Chrome

**Phase** 4 · **Milestone** v0.4 — Chromium + Android
**Blocked by** T-403 · **Parallel-safe with** T-405 and every v0.4+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-404-cdp-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Reach a Chromium debuggee. Done when a WebView2 app and a Chrome instance both appear in the
target picker and can be debugged.

## Seam

`CdpTransport` and its `Discovery` impl.

## Owns

- `crates/mjx-wk-transport/src/cdp.rs` (new; add the `mod` line)

## Must not touch

- every other file in `crates/mjx-wk-transport/src/`

## Fixtures

`fixtures/cdp-attach.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `/json/list` discovery parses every target kind, including workers and iframes;
- one WebSocket carries all targets, demultiplexed by `sessionId`;
- a Chromium that is not listening reports `Connect` naming the endpoint and the
  `--remote-debugging-port` flag it needs;
- `Transport::dialect` reports `ChromeDevToolsProtocol`, so the session picks the right dialect
  without being told.


## Notes

This is where CDP *is* simpler than WebKit: `/json/list` really is a JSON endpoint. Do not carry the WebKit socket-protocol assumptions over.
