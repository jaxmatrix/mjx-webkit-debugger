# T-000 — Complete the inspector server handshake

Phase: 1  ·  Depends on: none  ·  **Blocks: T-001, T-002, and every live fixture**

## Goal

Make a bare TCP client obtain a target list from a running WebKitGTK debuggee. Done when a short
program connects to `WEBKIT_INSPECTOR_SERVER`, performs whatever handshake is required, and prints
a `SetTargetList` containing the page — reproducibly, from a cold start.

This is a **spike**: the deliverable is knowledge plus the smallest code that demonstrates it. It
exists because the handshake is genuinely not known, and guessing would poison everything built on
top of it.

## What is already established

Verified against WebKitGTK 2.52.3, and recorded in `docs/PROTOCOL-NOTES.md` trap 1:

- The server is **not HTTP**. `GET / HTTP/1.1` gets the connection closed.
- Framing is a 4-byte **big-endian** length then JSON (`htonl`, in
  `RemoteInspectorMessageParser.cpp`). Little-endian causes a silent close.
- The client vocabulary, from `RemoteInspectorClient.cpp` at the pinned ref, is
  `SetupInspectorClient`, `Setup`, `SendMessageToBackend`, `FrontendDidClose`; the server sends
  `BackendCommands`, `SetTargetList`, `SendMessageToFrontend`.
- The debuggee must be started with developer extras enabled
  (`--enable-developer-extras=true` for MiniBrowser).

## What is not

**Sending `SetupInspectorClient` with correct framing produces no reply**, though the socket stays
open and the server keeps listening. Something further is required.

Leads, roughly in order of promise:

1. Read `Source/JavaScriptCore/inspector/remote/socket/RemoteInspectorSocketEndpoint.cpp` and
   `RemoteInspectorConnectionClient.cpp` at the pinned ref. The server may only push a target list
   once a *debuggable* connection has registered, which is a different connection from ours.
2. Check whether the debuggee's own `RemoteInspector` connects to the server lazily — possibly only
   when a page is marked inspectable, or on some first-inspection trigger.
3. Compare against a real client: run Epiphany or another WebKitGTK browser, navigate it to
   `inspector://127.0.0.1:2999`, and capture the exchange with `tcpdump -i lo -A port 2999` or
   `strace`. **This is the highest-signal experiment** — it shows exactly what a working client
   sends, in order.
4. Check whether `WEBKIT_INSPECTOR_SERVER` expects the *client* to listen and the debuggee to
   connect out, rather than the reverse.

## Owns

- `xtask/src/record.rs` (the `Client` handshake only)
- `docs/PROTOCOL-NOTES.md` (trap 1)
- a scratch reproduction under `scripts/`

## Must not touch

Anything under `crates/`. Once the handshake is known, T-001 implements it there.

## Done criteria

```sh
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
  --enable-developer-extras=true http://127.0.0.1:8731/index.html &
cargo run -p xtask -- record --list-only
```

prints at least one target. Then `docs/PROTOCOL-NOTES.md` trap 1 is updated to state the handshake
as fact rather than as an open question, and this file is closed.

## Notes

Serve the fixture page with `python3 -m http.server 8731 --directory fixtures/page`.

Do not settle for "it worked once". The handshake must survive a cold start, because every fixture
recording and every user's first attach depends on it.
