# T-705 — Layer tree and animations

**Phase** 7 · **Milestone** v0.7 — Storage, graphics, audits
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.7 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-705-layers-animations`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Why did this get its own layer, and what is animating. Done when compositing layers and running
animations are both inspectable.

## Seam

`Layer`, `AnimationEntry`, and the `LayerTree`/`Animation` half of `GraphicsAgent`.

## Owns

- `crates/mjx-wk-graphics/src/layers.rs` (new)
- `crates/mjx-wk-ui/src/layers_view.rs` (new)

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

A recorded `LayerTree` and `Animation` session.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- layers list with bounds, memory, and their backing node;
- **compositing reasons** are shown per layer — that is the question the panel exists to answer;
- animations list with target, duration, iterations and playback rate;
- playback rate can be changed and an animation seeked, so a transition can be inspected mid-flight.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Coordinate with T-702 on `crates/mjx-wk-graphics/src/lib.rs`: that ticket owns the canvas half of `GraphicsModel`, this one the layers and animations half.
