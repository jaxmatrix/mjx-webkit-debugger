# mjx-webkit-debugger

A native debugger for WebKit programs. Attaches to an already-running WebKit application over the
Remote Inspector protocol and gives a Chrome-DevTools-grade experience — browse every source file,
set breakpoints, pause, inspect state — **without embedding a webview itself**.

> **Status: Phase 1a.** The workspace, the frozen interfaces, the generated protocol bindings, and
> the offline test harness are in place. The application does not yet attach to anything: the
> feature work is decomposed into [`docs/tasks/`](docs/tasks/) and is being filled in. See
> [`PLAN.md`](PLAN.md) for what ships when. This section will say something different when v0.1.0
> lands, and not before.

## Why not just use Web Inspector?

Because a debugger that shares a process model with its debuggee goes down with it. Every
webview-based tool — including Safari's own inspector and anything built on Tauri or Electron —
runs inside or alongside the engine it inspects. When the page wedges the compositor, exhausts
memory, or spins the main thread, the tools stop responding at exactly the moment you need them.

This one renders through `wgpu` and links no browser engine at all. CI enforces that:

```sh
cargo run -p xtask -- verify-no-webview
```

## What it talks to

The WebKit Remote Inspector protocol, generated from WebKit's own descriptions — 26 domains, 239
commands, 110 events at the pinned ref (`webkitgtk-2.52.3`).

| Debuggee | Engine | Transport | Status |
|---|---|---|---|
| WebKitGTK / WPE apps, MiniBrowser, Linux Tauri | WebKit | TCP inspector server | Phase 1 |
| Playwright WebKit, Bun | WebKit | TCP inspector server | Phase 1 |
| macOS apps, Safari, macOS Tauri | WKWebView | `webinspectord` | Phase 3 |
| iOS apps and Safari | WKWebView | usbmux → lockdown | Phase 3 |
| **Windows Tauri, WebView2** | **Chromium** | **CDP** | Phase 4 |
| **Android WebView** | **Chromium** | **`adb forward` → CDP** | Phase 4 |

The last two are not an afterthought: Tauri uses a different engine on every platform, so debugging
one Tauri app everywhere means speaking both protocols. See [`docs/TRANSPORTS.md`](docs/TRANSPORTS.md).

## Getting started

Start the debuggee with its inspector server enabled:

```sh
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 ./your-app
```

Then:

```sh
cargo run --release -p mjx-webkit-debugger -- list             # what is inspectable?
cargo run --release -p mjx-webkit-debugger -- attach 127.0.0.1:2999
```

There is also a replay mode that drives the whole UI from a recorded trace with no debuggee at all,
which is how it is developed and demonstrated offline:

```sh
cargo run -p mjx-webkit-debugger -- replay fixtures/attach.jsonl
```

## Known limitations

Stated plainly, because a user who cannot tell whether a feature is missing or merely broken has
been failed twice. [`docs/CHROME-PARITY.md`](docs/CHROME-PARITY.md) tracks the full matrix.

- **Not yet functional.** Phase 1a froze the interfaces; the bodies are being filled in.
- **Restart frame** and **Trusted Type breakpoints** have no WebKit equivalent and will not be
  implemented.
- **Lighthouse, Recorder, WebAudio, WebAuthn, Sensors, and Autofill** panels are out of scope.
- **Apple platforms need the debuggee to opt in.** `WKWebView.isInspectable = true` (iOS 16.4+ /
  macOS 13.3+), and on macOS the debuggee also needs `com.apple.security.get-task-allow`. Neither
  is something a debugger can arrange for you.
- **iOS 17+ may need an RSD tunnel.** Apple moved several developer services to RemoteXPC; whether
  `com.apple.webinspector` is among them is unverified and is a Phase 3 spike.
- Over CDP, WebKit-only features degrade honestly rather than silently: breakpoint probes,
  microtask pausing, and the `Canvas`, `Audit`, `Memory`, and `Recording` domains report as
  unsupported and their controls are disabled.

## Building

```sh
cargo build --workspace
cargo test  --workspace
```

Requires Rust 1.92+ (egui 0.35). No system webview, no C toolchain beyond what `wgpu` needs.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Work is decomposed into independently-runnable tasks in
[`docs/tasks/`](docs/tasks/), each with disjoint file ownership and fixture-backed tests, so several
people can build different parts at once without colliding.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

This project contains no code from WebKit or Chromium. The generated protocol bindings are produced
from WebKit's machine-readable protocol descriptions, which are consulted locally and not
redistributed here.
