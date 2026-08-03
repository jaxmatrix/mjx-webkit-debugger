# T — Session correlation, event fan-out, and Target.* demux

Phase: 1  ·  Depends on: none  ·  Parallel-safe with: T-001…T-002, T-004…T-010

## Goal

Turn a byte stream into typed request/response and subscribable events. Done when `SessionHandle::call` returns a typed reply, subscribers receive events, and a multi-process page is indistinguishable from a simple one.

## Seam

`Session::attach`, all of `SessionHandle`, `Subscription`, and `AgentRegistry` event routing.

## Owns

- `crates/mjx-wk-session/src/lib.rs`
- `crates/mjx-wk-session/tests/`

## Must not touch

- `crates/mjx-wk-session/src/gating.rs` — already complete
- `crates/mjx-wk-dialect/` — `WebKitDialect` already does the multiplexing

## Fixtures

`fixtures/attach.jsonl` and `fixtures/target-multiplexed.jsonl`, both replayed through `ReplayTransport`. **No live debuggee.**

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a reply reaches the caller that sent the request, with ids correlated;
- an unsolicited reply is logged and dropped, not panicked on;
- a slow subscriber lags rather than stalling the socket;
- a `Target.dispatchMessageFromTarget` reaches a subscriber attributed to its target;
- `call` on an unsupported member returns `SessionError::Unsupported` **without touching the wire**;
- a `MethodNotFound` teaches `Capabilities`, and the second call short-circuits.

## Notes

The session is the only thing that awaits the transport. Time spent in an agent's fold is time the socket is not read — see the threading model in `CLAUDE.md`.
