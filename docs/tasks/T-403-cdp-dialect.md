# T — CDP dialect, transport, and Android

Phase: 4  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 4+ task

## Goal

Debug Chromium WebViews — which means Tauri on Windows and Android. Done when the same application attaches to a WebView2 app and an Android WebView, with WebKit-only features honestly disabled.

## Seam

`CdpDialect::encode`/`decode`, `CdpTransport`, `AndroidAdbTransport`.

## Owns

- `crates/mjx-wk-dialect/src/cdp.rs`
- `crates/mjx-wk-transport/src/cdp.rs`
- `crates/mjx-wk-transport/src/android.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/cdp-attach.jsonl`, recorded from Chromium, replayed with `ReplayTransport::with_dialect`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

The capability table in `cdp.rs` is already frozen — implement against it, do not edit it without a seam-change PR. Translations: `Target.*` ⇄ `sessionId`, `Console.messageAdded` ⇄ `Runtime.consoleAPICalled` + `Log.entryAdded`, paginated `getProperties` ⇄ client-side slicing, `addInterception` ⇄ `Fetch`, `Timeline` ⇄ `Tracing`. Android: `adb forward tcp:P localabstract:webview_devtools_remote_<pid>`, pid from `/proc/net/unix`.
