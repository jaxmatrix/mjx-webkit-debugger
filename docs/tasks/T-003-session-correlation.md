# T-003 — Session correlation, event fan-out, and Target.* demux

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-003-session-correlation`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Turn a byte stream into typed request/response and subscribable events. Done when
`SessionHandle::call` returns a typed reply, subscribers receive events, and a multi-process page is
indistinguishable from a simple one.

## Seam

`Session::attach`, all of `SessionHandle`, `Subscription`, and event routing in `AgentRegistry`.

## Owns

- `crates/mjx-wk-session/src/lib.rs`
- `crates/mjx-wk-session/tests/`

## Must not touch

- `crates/mjx-wk-session/src/gating.rs` — `Capabilities` is already complete
- `crates/mjx-wk-dialect/` — `WebKitDialect` already does the multiplexing

## Fixtures

`fixtures/attach.jsonl` and `fixtures/target-multiplexed.jsonl`, replayed through
`ReplayTransport`. **No live debuggee** — this ticket needs no browser and is not blocked by T-000.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a reply reaches the caller that sent the request, ids correlated;
- an unsolicited reply is logged and dropped, never panicked on — the debuggee is untrusted;
- a slow subscriber lags rather than stalling the socket;
- a `Target.dispatchMessageFromTarget` reaches subscribers attributed to its target;
- `call` on an unsupported member returns `SessionError::Unsupported` **without touching the wire**;
- a `MethodNotFound` teaches `Capabilities`, and the second call short-circuits.

## Notes

The session is the only thing that awaits the transport. Time spent in an agent's fold is time the
socket is not being read — see the threading model in `CLAUDE.md`.
