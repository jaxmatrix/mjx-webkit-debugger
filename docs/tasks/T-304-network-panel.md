# T-304 — Network panel — table, waterfall, viewers

**Phase** 3 · **Milestone** v0.3 — Apple platforms + Network
**Blocked by** T-301 · **Parallel-safe with** every other v0.3+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-304-network-panel`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

See the requests. Done when a page load renders a correct waterfall and any request's headers and
body can be inspected.

## Seam

`NetworkTable` and `WaterfallBar` in `mjx-wk-ui`.

## Owns

- `crates/mjx-wk-ui/src/network_table.rs`

## Must not touch

- every other file in `crates/mjx-wk-ui/src/`
- everything under `crates/mjx-wk-network/`

## Fixtures

`fixtures/network-load.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- the table virtualises past a few hundred rows;
- waterfall segments line up against a shared time axis;
- selecting a request shows headers, payload, response and a preview;
- a body is fetched **on selection**, not eagerly — holding every body would dwarf everything else
  the debugger keeps;
- filtering by type and by URL substring works while requests are still arriving.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Cached and service-worker responses have no network timing. Render them as such rather than as a zero-length bar, which reads as "instant" rather than "never happened".
