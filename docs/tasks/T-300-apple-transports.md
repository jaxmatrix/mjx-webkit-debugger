# T-300 — macOS and iOS transports, plus the iOS 17 spike

Phase: 3  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 3+ task

## Goal

Attach to WKWebView debuggees. Done when a Tauri app on macOS and an app on an iOS 16 device both appear in the target picker and can be debugged.

## Seam

`AppleLocalTransport`, `AppleUsbTransport`, and their `Discovery` implementations.

## Owns

- `crates/mjx-wk-transport/src/apple/`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

Recorded plist RPC exchanges from each platform.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

Wire format is **binary plist**, not JSON; the inspector frame rides in `WIRSocketDataKey`. Handshake: `_rpc_reportIdentifier:` → `_rpc_getConnectedApplications:` → `_rpc_forwardGetListing:` → `_rpc_forwardSocketSetup:` → `_rpc_forwardSocketData:`. Report the two prerequisites as `TransportError::NotInspectable` with instructions: `isInspectable = true`, and `com.apple.security.get-task-allow` on macOS. **Includes the iOS 17 RSD spike** — whether `com.apple.webinspector` still works over classic lockdown is unverified; settle it before scoping `AppleRsdTransport`.
