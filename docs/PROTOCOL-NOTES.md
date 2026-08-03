# Protocol notes — WebKit is not Chrome

Verified against **WebKitGTK 2.52.3** (`webkit2gtk-4.1`), pinned ref `webkitgtk-2.52.3`.

If you arrive from the Chrome DevTools Protocol, this is the page to read before writing code. The
differences are structural, not cosmetic, and most of them fail *silently*.

## Ground truth is extractable from the installed library

WebKit compiles its inspector frontend into a GResource bundle linked into the shared library, and
the generated protocol description inside it is the authority on what **that build** speaks:

```sh
gresource extract /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0 \
  /org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js
```

`cargo run -p xtask -- verify-protocol` does exactly this and diffs it against what we generated.
CI fails on drift in the dangerous direction — the debuggee knowing a member we have no types for.

Note it can differ from upstream source: WebKitGTK ships `Security` and `Recording` in
`Source/JavaScriptCore/inspector/protocol/*.json` but does not activate them, so we generate 26
domains and this build exposes 24.

## Surface at the pinned ref

26 domains, 239 commands, 110 events, 66 enums, 182 named types.

| Domain | Members | Domain | Members | Domain | Members |
|---|---|---|---|---|---|
| `DOM` | 78 | `Timeline` | 12 | `DOMDebugger` | 8 |
| `Debugger` | 40 | `Heap` | 12 | `Worker` | 7 |
| `Network` | 37 | `Memory` | 10 | `ScriptProfiler` | 6 |
| `Page` | 31 | `DOMStorage` | 10 | `LayerTree` | 6 |
| `CSS` | 31 | `Target` | 8 | `CPUProfiler` | 6 |
| `Runtime` | 29 | `IndexedDB` | 8 | `Inspector` | 5 |
| `Canvas` | 28 | `Animation` | 18 | `Browser` | 4 |
| `Console` | 15 | `Audit` | 3 | `Recording` / `ServiceWorker` | 2 / 1 |

## The eight traps

### 1. The inspector server is not an HTTP server

There is no `/json/list`, and — verified against WebKitGTK 2.52.3 — **no HTTP endpoint at all**.
Sending `GET / HTTP/1.1` to `WEBKIT_INSPECTOR_SERVER` gets the connection closed.

The *"Inspectable targets"* HTML page that appears in WebKit's binary strings is generated
**client-side**, by an inspecting browser's `inspector://` scheme handler, from a target list it
received over the socket. It is never served to anyone. (An earlier draft of this project's plan
assumed otherwise, on the strength of those strings; it was wrong.)

What listens is a **length-prefixed JSON socket protocol**
(`RemoteInspectorSocketEndpoint`). Each message is a 4-byte **big-endian** length followed by that
many bytes of JSON — `RemoteInspectorMessageParser.cpp` uses `htonl`:

```text
+--------+---------------------------+
|  size  |          payload          | (next message)
| 4 bytes|       `size` bytes        |
+--------+---------------------------+
```

Little-endian is not merely wrong, it is *quietly* wrong: the server reads an enormous length,
rejects it as invalid, and closes without a word.

Client → server, from `RemoteInspectorClient.cpp` at the pinned ref:

```json
{"event": "SetupInspectorClient"}
{"event": "Setup",                 "connectionID": 1, "targetID": 2}
{"event": "SendMessageToBackend",  "connectionID": 1, "targetID": 2, "message": "…"}
{"event": "FrontendDidClose",      "connectionID": 1, "targetID": 2}
```

Server → client:

```json
{"event": "BackendCommands",       "backendCommands": "…"}
{"event": "SetTargetList",         "connectionID": 1, "targetList": [
   {"targetID": 2, "name": "…", "url": "…", "type": "web-page"}]}
{"event": "SendMessageToFrontend", "connectionID": 1, "targetID": 2, "message": "…"}
```

`message` carries the inspector protocol frame **as a string**, which is why the `Transport` seam —
one JSON string in each direction — sits exactly where it does.

Two further facts, both established the hard way:

- **The debuggee must be started with developer extras enabled.** MiniBrowser needs
  `--enable-developer-extras=true`; an application needs
  `webkit_settings_set_enable_developer_extras()`. Without it the server listens and no target is
  ever registered.
- **The handshake is not yet complete.** Sending `SetupInspectorClient` with correct framing to a
  developer-extras-enabled MiniBrowser 2.52.3 produces **no reply**, though the socket stays open.
  Something further is needed before the server emits `SetTargetList`. This is unresolved and is
  `docs/tasks/T-000-inspector-handshake.md`; it blocks live fixture capture. It is recorded as
  unknown rather than guessed at, because a plausible-looking wrong handshake is worse than none.

### 2. `Target.*` wraps frames as JSON *strings*

```json
{"method":"Target.dispatchMessageFromTarget",
 "params":{"targetId":"w1","message":"{\"method\":\"Debugger.paused\",…}"}}
```

The inner frame is a **string**, not a nested object. Handled once in `mjx-wk-dialect`; get it
wrong and everything works against a simple page while every domain breaks on the first real
multi-process site.

### 3. `Runtime.getProperties` is paginated

`getProperties(objectId, ownProperties?, fetchStart?, fetchCount?, generatePreview?)`. CDP has no
`fetchStart`/`fetchCount`. Ignoring them is how a debugger hangs expanding a large array.

### 4. `Page.getResourceContent` is keyed by URL

`getResourceContent(frameId, url)` → `(content, base64Encoded)`. Not by a request id, and the
content may be base64 and need decoding.

### 5. Three domains Chrome has that WebKit does not

| Chrome | WebKit |
|---|---|
| `Fetch` | `Network.addInterception` + `interceptContinue` / `interceptWithRequest` / `interceptWithResponse` / `interceptRequestWithError` |
| `Storage` | cookies live on `Page`: `getCookies`, `setCookie`, `deleteCookie` |
| `Profiler` | `ScriptProfiler` + `CPUProfiler` + `Heap` + `Memory`, started and stopped independently |

### 6. Remote object handles die on resume

Every `objectId` in a paused scope is invalid the instant execution continues. Clear the variable
tree on `Debugger.resumed` **and** on `Debugger.globalObjectCleared`, or the UI shows stale rows
that error when expanded.

### 7. Breakpoints are set by URL

`setBreakpointByUrl(lineNumber, url?, urlRegex?, columnNumber?, options?)` → `(breakpointId,
locations)`. That is what makes them survive a reload. The debuggee then sends `breakpointResolved`
with the **actual** location, which may not be the line requested — a breakpoint on a blank line
moves to the next statement. Render requested and resolved differently.

Columns are **UTF-16 code units**, JavaScript's string model, not bytes. Conflating them puts
breakpoints on the wrong character in any file with an emoji or a non-Latin identifier.

### 8. Domains are gated per target type

`activateDomain("Page", ["web-page"])`. A `service-worker` or bare `javascript` target has no
`Page`, `DOM`, or `CSS`. Never assume a domain is present.

## What WebKit has that Chrome does not

Worth knowing, because these are the places this debugger can be *better* rather than merely equal:

- **Breakpoint actions** — `Log`, `Evaluate`, `Sound`, and **`Probe`**. A probe samples an
  expression every time the line runs and shows the values inline in the gutter without ever
  stopping. Chrome's logpoint is only the first of the four.
- **`setPauseOnMicrotasks`**, **`setPauseOnAssertions`**, **`continueUntilNextRunLoop`** — the last
  is the cleanest way past a chain of promise callbacks.
- **`addSymbolicBreakpoint`** — break on a function by name, with regex and case options.
- **`Runtime` type and control-flow profilers** — `enableTypeProfiler`,
  `getRuntimeTypesForVariablesAtOffsets`, `getBasicBlocks`. Inline type annotations and dead-code
  shading, from JavaScriptCore.
- **`Canvas`** (28 members) — including live shader source editing. No Chromium equivalent.
- **`Audit`** — runs JavaScript assertions inside the debuggee and reports structured results.

## Two upstream quirks the generator handles

- **`"$ref": "boolean"`** appears where `"type": "boolean"` was meant, on
  `Runtime.PropertyDescriptor.isPrivate`. A reference naming a primitive is treated as that
  primitive.
- **Four members are declared twice** under complementary `condition`s — `DOM.highlightNode` and
  `DOM.setInspectModeEnabled`, where the iOS build lacks `showRulers`. They are merged by union of
  fields, with any field absent from one variant made optional. Since an absent optional is not
  serialised, an iOS debuggee never receives a field it does not know.

Five types are self-referential and must be boxed: `Console.StackTrace.parentStackTrace`,
`DOM.Node.contentDocument`, `DOM.Node.templateContent`, `Runtime.RemoteObject.classPrototype`,
`Runtime.StructureDescription.prototypeStructure`. The generator computes this rather than listing
it, so a newly-recursive type is handled instead of producing an `E0072` in generated code.
