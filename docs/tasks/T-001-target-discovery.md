# T — Socket protocol framing and target discovery

Phase: 1  ·  Depends on: T-000  ·  Parallel-safe with: T-003…T-010

## Goal

Speak the inspector server's length-prefixed JSON protocol well enough to list what can be attached to. Done when `TcpInspectorServer::list()` returns every inspectable target from a running debuggee, and when the framing survives a message split arbitrarily across reads.

## Seam

`encode_message`, `decode_messages`, `SocketEvent`, `SocketTarget`, and `Discovery for TcpInspectorServer` in `mjx-wk-transport`.

## Owns

- `crates/mjx-wk-transport/src/discovery.rs`
- `crates/mjx-wk-transport/tests/discovery.rs`

## Must not touch

- `crates/mjx-wk-transport/src/tcp.rs` (T-002) — except the `Discovery` impl, which is yours
- everything outside `crates/mjx-wk-transport/`

## Fixtures

`fixtures/socket-handshake.jsonl` — the raw socket exchange, recorded by T-000. Distinct from the protocol traces: this one pins the *envelope*, not the frames inside it.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a `SetTargetList` with three targets yields three descriptors, in order;
- an empty target list yields an empty vec and **no error**;
- a message fed one byte at a time decodes exactly once, on the last byte;
- two messages in one read both decode;
- a length that would exceed a sane bound is rejected rather than allocated.

## Notes

Big-endian framing — see `docs/PROTOCOL-NOTES.md` trap 1. `BackendCommands` is tens of kilobytes and *always* arrives split; a parser that assumes one message per read passes every unit test and fails against a real debuggee.
