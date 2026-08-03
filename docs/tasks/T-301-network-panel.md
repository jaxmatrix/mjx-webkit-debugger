# T — Network panel

Phase: 3  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 3+ task

## Goal

Requests, timing, bodies, and interception. Done when a page load renders a correct waterfall and any request's body can be viewed on demand.

## Seam

`NetworkModel`, `NetworkRequest`, `Timing`, `WebSocketChannel`, and the `NetworkTable` widget.

## Owns

- `crates/mjx-wk-network/src/lib.rs`
- `crates/mjx-wk-ui/src/network_table.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

`fixtures/network-load.jsonl` — a full lifecycle, a WebSocket, and a failed load.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

A request is folded from five events and any of them may be the last seen if the page navigates mid-flight, so render a half-known request without complaint. **WebKit has no `Fetch` domain** — interception is the `Network.addInterception` family. Bodies are fetched on demand; holding every body would dwarf everything else.
