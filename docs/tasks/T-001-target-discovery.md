# T-001 — Socket protocol framing and target discovery

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** T-000 · **Parallel-safe with** T-003…T-015

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-001-target-discovery`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Speak the inspector server's length-prefixed JSON protocol well enough to list what can be attached
to. Done when `TcpInspectorServer::list()` returns every inspectable target from a running debuggee,
and the framing survives a message split arbitrarily across reads.

## Seam

`encode_message`, `decode_messages`, `SocketEvent`, `SocketTarget`, and `impl Discovery for TcpInspectorServer` — all already declared in `mjx-wk-transport`.

## Owns

- `crates/mjx-wk-transport/src/discovery.rs`
- `crates/mjx-wk-transport/tests/discovery.rs`

## Must not touch

- `crates/mjx-wk-transport/src/tcp.rs` (T-002) — except its `Discovery` impl, which is yours
- `crates/mjx-wk-transport/src/replay.rs` — already complete
- everything outside `crates/mjx-wk-transport/`

## Fixtures

`fixtures/socket-handshake.jsonl` — the raw socket exchange, recorded by T-013. Distinct from the
protocol traces: this one pins the **envelope**, not the frames inside it.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a `SetTargetList` with three targets yields three descriptors, in page order;
- an empty target list yields an empty vec and **no error** — a debuggee started without developer
  extras is a normal situation, not a failure;
- a message fed one byte at a time decodes exactly once, on the last byte;
- two messages arriving in one read both decode;
- a length beyond a sane bound is rejected rather than allocated.

## Notes

Big-endian framing — see `docs/PROTOCOL-NOTES.md` trap 1. `BackendCommands` is tens of kilobytes
and **always** arrives split; a parser that assumes one message per read passes every unit test and
fails against a real debuggee.
