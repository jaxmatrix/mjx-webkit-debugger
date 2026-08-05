# Fixtures

Recorded protocol traces, replayed through `ReplayTransport` so every test runs with **no WebKit
running**. One JSON object per line.

## Trace kinds

### RWI traces (`attach.jsonl`, …)

Inspector-protocol frames as they leave the glib envelope. On WebKitGTK 2.52 every non-`Target.*`
domain command is multiplexed:

```json
{"t":1043,"dir":"send","frame":{"id":2,"method":"Target.sendMessageToTarget","params":{"targetId":"page-8","message":"{\"id\":1,\"method\":\"Debugger.enable\",\"params\":{}}"}}}
{"t":2871,"dir":"recv","frame":{"id":2,"result":{}}}
{"t":3122,"dir":"recv","frame":{"method":"Target.dispatchMessageFromTarget","params":{"targetId":"page-8","message":"{\"result\":{},\"id\":1}"}}}
```

The inner `message` is a **JSON string** (trap 2 in `docs/PROTOCOL-NOTES.md`). `mjx-wk-dialect`
unwraps it; fixtures keep the wire form.

### Envelope trace (`socket-handshake.jsonl`)

Raw glib `SocketConnection` messages for T-001 — name + GVariant type + body hex, **not** RWI
frames:

```json
{"t":0,"dir":"send","name":"SetupInspectorClient","type":"(ay)","body_hex":"…"}
{"t":1,"dir":"recv","name":"DidSetupInspectorClient","type":"(ay)","body_hex":"…"}
{"t":2,"dir":"recv","name":"SetTargetList","type":"(ta(tsssb))","body_hex":"…"}
```

## Recording

```sh
# Static page + WebSocket echo (needed for network-load)
python3 -m http.server 8731 --directory fixtures/page &
python3 fixtures/page/ws_echo.py &

WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
  --enable-developer-extras=true http://127.0.0.1:8731/index.html &

# Optional: dump the SetTargetList rows (there is no server-served targets HTML page)
cargo run -p xtask -- record --scenario attach --save-targets-page

# One scenario per fixture
for s in socket-handshake attach breakpoint-hit network-load dom-css \
         storage timeline-record large-bundle target-multiplexed; do
  cargo run -p xtask -- record --scenario "$s" --out "fixtures/${s}.jsonl"
done

# Confirm every RWI send replays
cargo run -p xtask -- verify-fixtures
```

`--enable-developer-extras=true` is not optional: without it the inspector server listens and never
registers a target.

`--save-targets-page` writes `fixtures/targets-page.json` — a dump of the handshake's
`SetTargetList` rows. WebKitGTK does **not** serve an inspectable-targets HTML page (see
`docs/PROTOCOL-NOTES.md` trap 1); this flag replaces the old assumption that one existed.

Regenerate `large-bundle.js` if you need a different size:

```sh
python3 fixtures/page/generate-large-bundle.py
```

## Corpus (nine traces)

| Fixture | Pins |
|---|---|
| `socket-handshake.jsonl` | glib `SetupInspectorClient` → `DidSetupInspectorClient` → `SetTargetList` |
| `attach.jsonl` | enable + `Page.getResourceTree` + `scriptParsed` via Target multiplexing |
| `breakpoint-hit.jsonl` | `setBreakpointByUrl` → reload → `paused` → `getProperties` → resume |
| `target-multiplexed.jsonl` | `Target.sendMessageToTarget` / `dispatchMessageFromTarget` (string inner frames) + worker |
| `large-bundle.jsonl` | multi-MB `Debugger.getScriptSource` of `large-bundle.js` |
| `network-load.jsonl` | request lifecycle + WebSocket + failed (`missing-404.json`) load |
| `dom-css.jsonl` | `getDocument` → `getMatchedStylesForNode` → `getComputedStyleForNode` |
| `timeline-record.jsonl` | `Timeline` + `ScriptProfiler` start/stop |
| `storage.jsonl` | DOMStorage + IndexedDB + cookies |

**Do not hand-author these.** Re-run `record` against MiniBrowser when WebKit moves.

## `page/`

The debuggee used for recording: timer breakpoint target, stylesheet, storage seeds, fetch +
WebSocket + deliberate 404, multi-MB script host (`large.html`), and dedicated-worker host
(`worker-host.html`).
