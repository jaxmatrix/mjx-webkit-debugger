# T-302 — iOS ≤16 transport over usbmux

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** T-300 · **Parallel-safe with** T-303 and every v0.3+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-302-ios-usb-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Attach to a `WKWebView` on a physical iOS 16 device. Done when a connected device's inspectable
pages appear in the target picker from a Linux, macOS or Windows host.

## Seam

`AppleUsbTransport` and its `Discovery` impl. Shares the plist RPC codec with T-300.

## Owns

- `crates/mjx-wk-transport/src/apple/usb.rs` (new)

## Must not touch

- `crates/mjx-wk-transport/src/apple/local.rs` (T-300)
- everything outside `crates/mjx-wk-transport/`

## Fixtures

A recorded exchange from a real device.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- `usbmuxd` device enumeration lists connected devices;
- `lockdownd` starts `com.apple.webinspector`;
- the same plist RPC as T-300 then applies unchanged;
- a device without Developer Mode, or a webview without `isInspectable`, reports
  `NotInspectable` with the specific remedy;
- works from a Linux host — usbmuxd is available on all three platforms and this must not become
  macOS-only by accident.


## Notes

Depends on T-300 only for the plist codec; if that lands as a shared module the two are otherwise
independent. iOS **17+** is deliberately out of scope here — see T-303.
