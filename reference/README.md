# reference/ — local-only protocol material

Everything under this directory except this file is **git-ignored and must never
be staged**. It is input to `xtask codegen`, not part of the distribution. The
*generated* Rust under `crates/mjx-wk-protocol/src/generated/` is committed, so a
clean clone builds without any of this present.

## `webkit-protocol/`

The WebKit Remote Inspector protocol descriptions, pinned to the WebKit tag that
matches the WebKitGTK build this repo is developed against.

| | |
|---|---|
| Source | `WebKit/WebKit` → `Source/JavaScriptCore/inspector/protocol/*.json` |
| Pinned ref | `webkitgtk-2.52.3` |
| Files | 27 (26 domains + `GenericTypes.json`) |
| Licence | BSD-2-Clause, Copyright Apple Inc. and others |

Repopulate with:

```sh
REF=webkitgtk-2.52.3
mkdir -p reference/webkit-protocol
curl -sS "https://api.github.com/repos/WebKit/WebKit/contents/Source/JavaScriptCore/inspector/protocol?ref=$REF" \
  | grep -oE '"download_url": "[^"]+"' | sed 's/"download_url": "//;s/"$//' \
  | (cd reference/webkit-protocol && xargs -n1 -P8 curl -sS -O)
```

Then `cargo run -p xtask -- codegen`.

## Why the pinned ref matters

The descriptions above are *upstream source*. What a given WebKit build actually
exposes at runtime can differ — GTK ships `Security` in source but does not
activate it, for instance. The runtime truth is extractable from the installed
library itself:

```sh
gresource extract /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0 \
  /org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js
```

`cargo run -p xtask -- verify-protocol` does exactly that and diffs the two.
CI fails on drift, so a WebKit upgrade is caught at build time rather than
discovered as a runtime error. See `docs/PROTOCOL-NOTES.md`.

## `chromium-protocol/` (Phase 4)

The Chrome DevTools Protocol descriptions (`browser_protocol.pdl`,
`js_protocol.pdl`), used to pin `CdpDialect`. Not needed before Phase 4.
