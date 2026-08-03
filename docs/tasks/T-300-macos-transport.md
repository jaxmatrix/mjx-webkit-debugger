# T-300 — macOS webinspectord transport

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** v0.1 complete · **Parallel-safe with** T-302, T-303, and every v0.3+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-300-macos-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Attach to `WKWebView` debuggees on macOS — which means Tauri apps. Done when a local debug-built
app appears in the target picker and can be debugged.

## Seam

`AppleLocalTransport` and its `Discovery` impl, behind the existing `Transport` trait.

## Owns

- `crates/mjx-wk-transport/src/apple/mod.rs` and `local.rs` (new; add the `mod` line)

## Must not touch

- `crates/mjx-wk-transport/src/tcp.rs`, `discovery.rs`, `replay.rs`
- everything outside `crates/mjx-wk-transport/`

## Fixtures

A recorded plist RPC exchange from a real macOS session.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- the handshake completes: `_rpc_reportIdentifier:` → `_rpc_getConnectedApplications:` →
  `_rpc_forwardGetListing:` → `_rpc_forwardSocketSetup:`;
- frames ride in and out via `_rpc_forwardSocketData:`;
- a debuggee without `isInspectable` reports `TransportError::NotInspectable` **with instructions**,
  never a generic connection failure;
- likewise a debuggee lacking `com.apple.security.get-task-allow`.


## Notes

The wire is **binary plist**, not JSON, with the inspector frame inside `WIRSocketDataKey`. Because
`Transport` is one JSON string each way, all of that stays inside this backend and no other crate
changes. See `docs/TRANSPORTS.md`.

The two prerequisites are not things a debugger can arrange, which is why they get their own error
variant rather than being reported as a failure to connect.
