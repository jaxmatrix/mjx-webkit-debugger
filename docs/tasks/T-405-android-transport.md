# T-405 — AndroidAdbTransport

**Phase** 4 · **Milestone** v0.4 — Chromium + Android
**Blocked by** T-403, T-404 · **Parallel-safe with** every other v0.4+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-405-android-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Debug an Android System WebView — which means Tauri on Android. Done when an app on a connected
device appears in the target picker.

## Seam

`AndroidAdbTransport` and its `Discovery` impl. Reuses `CdpTransport` once forwarded.

## Owns

- `crates/mjx-wk-transport/src/android.rs` (new)

## Must not touch

- every other file in `crates/mjx-wk-transport/src/`

## Fixtures

A recorded session from a real device.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- devices are enumerated via `adb devices`;
- webview sockets are discovered from `adb shell cat /proc/net/unix`, matching
  `webview_devtools_remote_<pid>`, and the pid is mapped back to a package name;
- `adb forward` is set up and **torn down** on close, so repeated runs do not leak forwards;
- an app without `setWebContentsDebuggingEnabled(true)` reports `NotInspectable` with that remedy.


## Notes

Discovery is the interesting part: the socket name carries a pid, not a package, so the mapping back to something a user recognises has to be done explicitly.
