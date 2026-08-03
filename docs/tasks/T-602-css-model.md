# T-602 — Matched, inherited and computed styles

**Phase** 6 · **Milestone** v0.6 — Elements + Styles
**Blocked by** v0.1 complete · **Parallel-safe with** every other v0.6+ ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-602-css-model`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Model why a property has the value it has. Done when the full cascade for an element is available,
losers included.

## Seam

`CssModel`, `MatchedStyles`, `CssRule`, `CssProperty`, `RuleOrigin`, and `impl DomainAgent for CssAgent`.

## Owns

- `crates/mjx-wk-css/src/lib.rs`
- `crates/mjx-wk-css/tests/cascade.rs`

## Must not touch

Any other L4 feature crate. **The nine feature crates are peers and must never depend on one
another** — if you need something another has, it belongs in `mjx-wk-source` (L3). See
[`docs/SEAMS.md`](../SEAMS.md).

## Fixtures

`fixtures/dom-css.jsonl`.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- matched rules are ordered most-specific-first, with the matching selector identified;
- inherited rules carry the ancestor they came from;
- pseudo-element rules are grouped by pseudo;
- overridden declarations are marked `Inactive`, not dropped;
- `@media`/`@supports`/`@layer`/`@container` groupings are preserved;
- the inline `style` attribute is separate from author rules.

Plus: the panel renders **disabled, with a reason**, when `SessionHandle::supports` reports its
members unavailable — checked against a CDP-dialect session as well as a WebKit one. Never hidden,
never silently broken.

## Notes

Takes a `mjx_wk_source::NodeId`, **never** a type from `mjx-wk-dom`. Keeping the losers is what makes the panel worth having.
