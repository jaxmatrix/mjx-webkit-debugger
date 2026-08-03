# Fixtures

Recorded protocol traces, replayed through `ReplayTransport` so every test runs with **no WebKit
running**. One JSON object per line:

```json
{"t":1043,"dir":"send","frame":{"id":1,"method":"Debugger.enable","params":{}}}
{"t":2871,"dir":"recv","frame":{"id":1,"result":{}}}
```

Frames are *inspector protocol* frames, not the socket envelope — the envelope is the transport's
business, and a fixture must stay valid if it ever changes.

## Recording

```sh
python3 -m http.server 8731 --directory fixtures/page &
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
  --enable-developer-extras=true http://127.0.0.1:8731/index.html &
cargo run -p xtask -- record --scenario attach --out fixtures/attach.jsonl
```

`--enable-developer-extras=true` is not optional: without it the inspector server listens and never
registers a target.

## Status

**No live traces have been captured yet.** The inspector server handshake is unresolved — see
`docs/tasks/T-000-inspector-handshake.md` and trap 1 in `docs/PROTOCOL-NOTES.md`. Until it is,
`ReplayTransport` is exercised by traces written inline in its own tests.

Hand-authoring the corpus was deliberately **not** done. A fabricated trace would let every task's
tests pass against a protocol nobody has observed, which is worse than having no fixtures: it looks
like coverage.

## `page/`

The debuggee used for recording. Deliberately small and deterministic — a named function called on
a timer so a breakpoint hits without anyone clicking, a stylesheet, local storage, and a `fetch`,
so each scenario has something real to record.
