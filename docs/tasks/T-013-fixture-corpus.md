# T-013 — Record the fixture corpus

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** T-000 · **Parallel-safe with** T-001, T-002 (which consume it)

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-013-fixture-corpus`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Record every protocol trace the test suite replays, against the local MiniBrowser. Done when
`cargo test --workspace` exercises real recorded traffic rather than traces written inline in tests.

**This is the integration contract for the whole project.** Every ticket's tests are fixture-backed,
and a fixture-backed test needs no browser, no port, no platform.

Nine traces, from `fixtures/page/`:

| Fixture | Pins |
|---|---|
| `socket-handshake.jsonl` | the raw socket envelope → T-001 |
| `attach.jsonl` | handshake + the `scriptParsed` flood |
| `breakpoint-hit.jsonl` | setBreakpointByUrl → reload → paused → getProperties → resume |
| `target-multiplexed.jsonl` | a session using `Target.sendMessageToTarget` |
| `large-bundle.jsonl` | a multi-MB `getScriptSource` → the bench input |
| `network-load.jsonl` | full request lifecycle + a WebSocket + a failed load |
| `dom-css.jsonl` | `getDocument` → `getMatchedStylesForNode` → `getComputedStyleForNode` |
| `timeline-record.jsonl` | `Timeline` + a `ScriptProfiler` run |
| `storage.jsonl` | DOMStorage + IndexedDB + cookies |

## Seam

No seam. `xtask record`'s scenarios, which already exist, plus whatever the handshake needs.

## Owns

- `fixtures/*.jsonl`
- `fixtures/page/` if a scenario needs more to exercise
- `xtask/src/record.rs` — scenario definitions

## Must not touch

- `crates/mjx-wk-transport/src/replay.rs` — the replay side is complete

## Fixtures

This ticket *produces* them.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- each trace replays through `ReplayTransport` without an unmatched send;
- `target-multiplexed.jsonl` genuinely contains `Target.*` wrapping — if the fixture page does not
  produce a multi-process target, find one that does, do not fake it;
- traces are committed and regenerable: re-running `record` produces an equivalent trace.

## Notes

**Do not hand-author these.** A fabricated trace lets every task's tests pass against a protocol
nobody has observed, which is worse than having no fixtures because it looks like coverage. That is
why this is its own ticket rather than a side effect of T-000.
