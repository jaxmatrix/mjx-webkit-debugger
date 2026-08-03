# T-002 — Transport over the inspector socket

Phase: 1  ·  Depends on: T-000, T-001  ·  Parallel-safe with: T-003…T-010

## Goal

Carry inspector protocol frames to and from one target. Done when a session can be attached, frames flow both ways, and closing sends `FrontendDidClose` so the debuggee tears its side down.

## Seam

`TcpInspectorServer::attach` and `impl Transport for TcpTransport`.

## Owns

- `crates/mjx-wk-transport/src/tcp.rs`
- `crates/mjx-wk-transport/tests/tcp.rs`

## Must not touch

- `crates/mjx-wk-transport/src/discovery.rs` (T-001)
- `crates/mjx-wk-transport/src/replay.rs` — already complete; if you need to change it, that is a seam-change PR

## Fixtures

`fixtures/attach.jsonl` — the handshake and the `scriptParsed` flood.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a refused connection reports `TransportError::Connect` naming the endpoint;
- a clean close reads as `recv() -> None`, not an error;
- a 5 MB frame arriving across many reads is delivered once, intact;
- `close()` sends `FrontendDidClose` before dropping the socket.

## Notes

An outgoing frame becomes `SendMessageToBackend`; each `SendMessageToFrontend`'s `message` is a received frame. The `Transport` seam is one JSON string each way precisely so the envelope stops here.
