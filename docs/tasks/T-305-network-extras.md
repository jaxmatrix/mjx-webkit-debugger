# T-305 — WebSocket frames, interception, and HAR export

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** T-301, T-304 · **Parallel-safe with** every other v0.3+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-305-network-extras`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

The rest of the network story. Done when WebSocket traffic is inspectable, a request can be
overridden or blocked, and a session can be exported as HAR.

## Seam

`WebSocketChannel`, `WsFrame`, the interception API, and HAR serialisation.

## Owns

- `crates/mjx-wk-network/src/websocket.rs`, `intercept.rs`, `har.rs` (new)

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/network-load.jsonl`, which includes a WebSocket.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- WS frames render with direction, opcode and payload, and a binary payload does not corrupt the
  view;
- `Network.addInterception` blocks, rewrites and stubs a request;
- exported HAR validates against the 1.2 schema and re-imports into Chrome DevTools.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

**WebKit has no `Fetch` domain.** Interception is `Network.addInterception` plus
`interceptContinue` / `interceptWithRequest` / `interceptWithResponse` / `interceptRequestWithError`.
Code written from CDP habits will look for `Fetch` and not find it.
