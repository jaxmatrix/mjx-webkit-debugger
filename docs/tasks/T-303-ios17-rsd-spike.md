# T-303 — Spike: does com.apple.webinspector need an RSD tunnel on iOS 17+?

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** nothing · **Parallel-safe with** everything

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-303-ios17-rsd-spike`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Settle an open question before anyone scopes work against it. Done when we know, from a real
device, whether `com.apple.webinspector` is reachable over classic lockdown `StartService` on
iOS 17 and 18, or whether it requires a RemoteXPC/RSD tunnel.

**Spike, not a feature.** The deliverable is an answer plus the evidence for it.

## Seam

None. The answer determines whether `AppleRsdTransport` (T-406) is scoped at all.

## Owns

- `docs/TRANSPORTS.md` — the iOS 17 section
- a scratch probe under `scripts/`

## Must not touch

Anything under `crates/`.

## Fixtures

None.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a documented result from a real iOS 17+ device, either "classic lockdown still works" or "a
  tunnel is required, and here is what it takes";
- `docs/TRANSPORTS.md` states it as fact and stops calling it unverified;
- if a tunnel is required, T-406 gets a scope; if not, T-406 is closed as unnecessary.


## Notes

What is established: iOS 17 moved `debugserver`, `instruments.server` and XCUITest infrastructure
to RemoteXPC. **Whether `webinspectord` moved with them is not established** — every public source
found so far is silent on it.

`pymobiledevice3` can establish a userspace tunnel without root and is the fastest way to test both
paths. Answer it empirically; do not infer it from the other services.
