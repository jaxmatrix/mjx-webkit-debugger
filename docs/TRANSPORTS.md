# Transports — reaching a debuggee

Six backends behind two traits (`Transport`, `Discovery`). Everything above `mjx-wk-transport` is
written against those and knows nothing about sockets, USB, or XPC.

## Why this matters more than it looks

**Tauri uses a different engine on every platform.** So "debug my Tauri app" is not one problem:

| Platform | Engine | Protocol | Backend | Phase |
|---|---|---|---|---|
| Linux | WebKitGTK | WebKit RWI | `TcpInspectorServer` | 1 |
| macOS | WKWebView | WebKit RWI | `AppleLocalTransport` | 3 |
| iOS | WKWebView | WebKit RWI | `AppleUsbTransport` | 3 |
| **Windows** | **WebView2** | **CDP** | **`CdpTransport`** | 4 |
| **Android** | **System WebView** | **CDP** | **`AndroidAdbTransport`** | 4 |

Chromium support is therefore a requirement, not a nicety — and it also gives the project a second
engine to validate the whole application against.

## The seam is one JSON string in each direction

That placement is deliberate. Apple's transports do not carry JSON on the wire at all; they carry
**binary plists**, with the WebKit frame inside one:

```text
{ __selector: "_rpc_forwardSocketData:",
  __argument: { WIRConnectionIdentifierKey: …,
                WIRApplicationIdentifierKey: …,
                WIRPageIdentifierKey: …,
                WIRSenderKey: …,
                WIRSocketDataKey: <the JSON frame, as bytes> } }
```

Because `Transport` is "send me a frame, hand me back a frame", all of that lives inside the Apple
backend and **no other crate changes when it lands**.

---

## `TcpInspectorServer` — Phase 1

Reaches anything honouring `WEBKIT_INSPECTOR_SERVER`: WebKitGTK, WPE, WinCairo, Playwright's
WebKit, and Bun.

```sh
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 ./your-app
```

**Not HTTP and not a WebSocket** — see `PROTOCOL-NOTES.md` trap 1. On WebKitGTK/WPE the socket
speaks GLib `SocketConnection` (GVariant bodies): `SetupInspectorClient` → `DidSetupInspectorClient`
→ `SetTargetList`, then `Setup` / `SendMessageToBackend` / `SendMessageToFrontend` multiplexing by
`connectionID`/`targetID`.

The debuggee must have developer extras enabled (`--enable-developer-extras=true` for MiniBrowser,
`webkit_settings_set_enable_developer_extras()` for an application), or the server listens and
never registers a target.

**Status: handshake complete** (T-000). `cargo run -p xtask -- record --list-only` lists targets
against MiniBrowser 2.52.3. T-001 still has to teach `mjx-wk-transport` the same glib framing.

## `ReplayTransport` — Phase 1

A recorded trace, replayed offline. Implements the same trait as a real socket, so every task is
testable with no browser, no port, and no platform. A send the trace does not cover is a **failure**,
which is what stops a fixture test passing while exercising nothing.

## `AppleLocalTransport` — Phase 3 (macOS)

Talks to `webinspectord` over its local Mach service. Frames are wrapped in the plist RPC above.

Handshake: `_rpc_reportIdentifier:` → `_rpc_getConnectedApplications:` → `_rpc_forwardGetListing:`
→ `_rpc_forwardSocketSetup:` → `_rpc_forwardSocketData:` in both directions.

**Prerequisites the debugger cannot arrange for you**, and must therefore report clearly rather
than as a generic connection failure (`TransportError::NotInspectable`):

- the `WKWebView` must have `isInspectable = true` (macOS 13.3+ / Safari 16.4+);
- the debuggee must carry the `com.apple.security.get-task-allow` entitlement, which debug builds
  have and release builds do not.

## `AppleUsbTransport` — Phase 3 (iOS ≤ 16)

`usbmuxd` → `lockdownd` → the `com.apple.webinspector` service, then the same plist RPC. Works from
Linux, macOS, or Windows hosts, since usbmuxd is available on all three.

Same `isInspectable` prerequisite (iOS 16.4+).

## `AppleRsdTransport` — Phase 4, gated on a spike

**Unverified, and deliberately not scoped yet.** iOS 17 moved several developer services to
RemoteXPC/RSD, which needs a tunnel rather than a plain lockdown `StartService`. Public sources
confirm the move for `debugserver`, `instruments.server`, and XCUITest infrastructure; **whether
`com.apple.webinspector` is affected is not established.**

Phase 3 includes a spike task to settle it on a real device. Scoping this backend before that
answer exists would be guessing at a week of work.

## `AndroidAdbTransport` — Phase 4

Android System WebView is Chromium and speaks CDP over an abstract unix socket:

```sh
adb shell cat /proc/net/unix | grep webview_devtools_remote   # find the pid
adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>
```

Discovery is then ordinary CDP `/json/list` on the forwarded port. Pairs with `CdpDialect`.

The app must have called `WebView.setWebContentsDebuggingEnabled(true)` — again, a prerequisite to
report clearly, not to work around.

## `CdpTransport` — Phase 4

WebView2, Chrome, and Edge. `/json/list` for discovery, one WebSocket per target, `sessionId`
instead of `Target.*` wrapping. Pairs with `CdpDialect`, whose capability table already records
what does and does not survive the crossing — see `mjx_wk_dialect::cdp`.

---

## Choosing a backend

`TargetDescriptor::origin` records which backend found a target, and `Transport::dialect` reports
which protocol it speaks, so the session picks the dialect rather than the user being asked. A
target list can mix backends: a Linux machine with an Android phone attached shows both.
