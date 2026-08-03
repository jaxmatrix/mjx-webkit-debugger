# T-000 — Complete the inspector server handshake

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** nothing · **Parallel-safe with** **Blocks T-001, T-002 and T-013** — the only real blocker in the project

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-000-inspector-handshake`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Make a bare TCP client obtain a target list from a running WebKitGTK debuggee. Done when a short
program connects to `WEBKIT_INSPECTOR_SERVER`, performs whatever handshake is required, and prints a
`SetTargetList` containing the page — reproducibly, from a cold start.

**This is a spike.** The deliverable is knowledge plus the smallest code that demonstrates it. It
exists because the handshake is genuinely not known, and guessing would poison everything built on
top of it.

### What is already established

Verified against WebKitGTK 2.52.3:

- The server is **not HTTP**. `GET / HTTP/1.1` gets the connection closed.
- Framing is a 4-byte **big-endian** length then JSON (`htonl`, in
  `RemoteInspectorMessageParser.cpp`). Little-endian fails *silently*: the server reads an enormous
  length, calls it invalid, and hangs up.
- Client vocabulary from `RemoteInspectorClient.cpp`: `SetupInspectorClient`, `Setup`,
  `SendMessageToBackend`, `FrontendDidClose`. Server sends `BackendCommands`, `SetTargetList`,
  `SendMessageToFrontend`.
- The debuggee must be started with developer extras enabled.

### What is not

**`SetupInspectorClient` with correct framing produces no reply**, though the socket stays open and
the server keeps listening. Something further is required.

Leads, in order of promise:

1. Read `RemoteInspectorSocketEndpoint.cpp` and `RemoteInspectorConnectionClient.cpp` at the pinned
   ref. The server may only push a target list once a *debuggable* connection has registered —
   a different connection from ours.
2. Check whether the debuggee's own `RemoteInspector` connects lazily.
3. **Highest signal:** point Epiphany at `inspector://127.0.0.1:2999` and capture the exchange with
   `tcpdump -i lo -A port 2999`. That shows exactly what a working client sends, in order.
4. Check whether the *client* is meant to listen and the debuggee to connect out.

## Seam

The `Client` handshake in `xtask/src/record.rs`. No crate seam changes.

## Owns

- `xtask/src/record.rs` — the handshake only
- `docs/PROTOCOL-NOTES.md` — trap 1
- a scratch reproduction under `scripts/`

## Must not touch

Anything under `crates/`. Once the handshake is known, T-001 implements it there.

## Fixtures

None — this ticket is what makes fixtures recordable at all.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

```sh
python3 -m http.server 8731 --directory fixtures/page &
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
  --enable-developer-extras=true http://127.0.0.1:8731/index.html &
cargo run -p xtask -- record --list-only
```

prints at least one target. Then trap 1 of `docs/PROTOCOL-NOTES.md` states the handshake as fact
rather than as an open question.

## Notes

Do not settle for "it worked once". The handshake must survive a cold start, because every fixture
recording and every user's first attach depends on it.
