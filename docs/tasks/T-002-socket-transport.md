# T-002 — Transport over the inspector socket

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** T-000, T-001 · **Parallel-safe with** T-003…T-015

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-002-socket-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Carry inspector protocol frames to and from one target. Done when a session attaches, frames flow
both ways, and closing sends `FrontendDidClose` so the debuggee tears its side down instead of
leaving the target marked as being inspected.

## Seam

`TcpInspectorServer::attach` and `impl Transport for TcpTransport`.

## Owns

- `crates/mjx-wk-transport/src/tcp.rs`
- `crates/mjx-wk-transport/tests/tcp.rs`

## Must not touch

- `crates/mjx-wk-transport/src/discovery.rs` (T-001)
- `crates/mjx-wk-transport/src/replay.rs` — already complete; changing it is a seam-change PR

## Fixtures

`fixtures/attach.jsonl` — the handshake and the `scriptParsed` flood.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a refused connection reports `TransportError::Connect` **naming the endpoint** — "is the app
  running with inspection enabled?" is the first thing a user needs to know;
- a clean close reads as `recv() -> None`, not an error;
- a 5 MB frame arriving across many reads is delivered once, intact;
- `close()` sends `FrontendDidClose` before dropping the socket.

## Notes

An outgoing frame becomes `SendMessageToBackend`; each `SendMessageToFrontend`'s `message` is a
received frame. The `Transport` seam is one JSON string each way precisely so the envelope stops
here — nothing above this crate learns the wire format.
