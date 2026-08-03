# T-406 — AppleRsdTransport (iOS 17+)

**Phase** 4 · **Milestone** v0.4 — Chromium + Android
**Blocked by** T-303 (the spike) · **Parallel-safe with** every other v0.4+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-406-apple-rsd-transport`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

**Scope this only after T-303 answers.** If iOS 17+ requires a RemoteXPC/RSD tunnel for
`com.apple.webinspector`, build it. If the spike shows classic lockdown still works, close this
ticket as unnecessary and note why.

Written now so the work is visible; deliberately left unscoped so nobody estimates a week of tunnel
plumbing that may not be needed.

## Seam

`AppleRsdTransport`, behind the same `Transport` trait as every other backend.

## Owns

- `crates/mjx-wk-transport/src/apple/rsd.rs` (new, if needed)

## Must not touch

- every other file in `crates/mjx-wk-transport/src/`

## Fixtures

A recorded session from an iOS 17+ device.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Determined by T-303. At minimum: a tunnel is established without requiring root, targets are
listed, and frames flow — or this ticket is closed with the spike's evidence attached.


## Notes

`pymobiledevice3` establishes a userspace tunnel with a pure-Python network stack and no root; if a
tunnel is needed, that is the reference implementation to read.
