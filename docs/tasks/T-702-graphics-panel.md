# T — Canvas, layers, and animations

Phase: 7  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 7+ task

## Goal

The WebKit-only graphics tooling. Done when canvas contexts are listed, a frame can be recorded and stepped, shader source can be edited live, and the layer tree shows compositing reasons.

## Seam

`GraphicsModel`, `CanvasContext`, `ShaderProgram`, `Layer`, `AnimationEntry`.

## Owns

- `crates/mjx-wk-graphics/src/lib.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

A fixture page with a WebGL canvas and a CSS animation.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

Mostly territory Chromium has no equivalent for — `Canvas` alone is 28 members including live shader editing. Over CDP the whole panel reports unsupported, which is already in the dialect's table.
