# T-702 — Canvas contexts, recordings, and shader editing

**Phase** 7 · **Milestone** v0.7 — Storage, graphics, audits
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.7 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-702-canvas-inspection`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

WebKit's canvas tooling, which Chromium has no equivalent for. Done when canvas contexts are
listed, a frame can be recorded and stepped, and shader source can be edited live.

## Seam

`CanvasContext`, `ShaderProgram`, `CanvasRecording`, and the `Canvas`/`Recording` half of `GraphicsAgent`.

## Owns

- `crates/mjx-wk-graphics/src/canvas.rs` (new)
- `crates/mjx-wk-ui/src/canvas_view.rs` (new)

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

A recorded `Canvas` session — the fixture page needs a WebGL canvas adding.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- 2d, WebGL, WebGL2 and WebGPU contexts are listed with their backing node and memory;
- a recording captures a frame and can be stepped call by call;
- vertex and fragment source can be fetched, edited, and applied via `updateShader`;
- a program can be disabled and highlighted;
- over CDP the whole panel reports unsupported, which the dialect table already encodes.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

`Canvas` is 28 members. Live shader editing is the standout feature and the reason this is worth building despite having no Chromium counterpart.
