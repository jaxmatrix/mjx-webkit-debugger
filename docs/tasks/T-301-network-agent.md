# T-301 — Network domain agent and request lifecycle

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.3+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-301-network-agent`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Fold network events into a coherent request model. Done when a page load produces one accurate
record per request, including ones interrupted mid-flight.

A request is folded across five events: `requestWillBeSent` → `responseReceived` → `dataReceived`* →
`loadingFinished` | `loadingFailed`.

## Seam

`NetworkModel`, `NetworkRequest`, `Timing`, `RequestOutcome`, `ResponseSource`, and `impl DomainAgent for NetworkAgent`.

## Owns

- `crates/mjx-wk-network/src/lib.rs`
- `crates/mjx-wk-network/tests/lifecycle.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/network-load.jsonl` — full lifecycle, a WebSocket, and a failed load.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a request interrupted by navigation renders from what is known, without error;
- `requestServedFromMemoryCache` produces a record with no network timing;
- a failed load records whether it was cancelled or errored;
- `preserve_log` keeps records across navigation when set;
- timing segments are monotonic and never negative.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Any of the five events may be the last one seen, so every field after the first is optional. The UI
must render a half-known request without complaint — this is the normal case on a page that
navigates, not an edge case.
