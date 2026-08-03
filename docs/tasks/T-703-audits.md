# T-703 — Audits

Phase: 7  ·  Depends on: Phase 1 complete  ·  Parallel-safe with: every other Phase 7+ task

## Goal

Run scriptable assertions inside the debuggee. Done when a suite runs and its results link back to the nodes they point at.

## Seam

`AuditModel`, `AuditSuite`, `AuditResult`.

## Owns

- `crates/mjx-wk-audit/src/lib.rs`

## Must not touch

Any other L4 crate. **The nine feature crates are peers and must never depend on one another** —
if you need something another one has, it belongs in `mjx-wk-source` (L3). See
`docs/SEAMS.md`.

## Fixtures

A fixture suite with a passing and a failing test.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Plus: the panel renders disabled, with a reason, when `SessionHandle::supports` says its members
are unavailable — verified against a CDP-dialect session as well as a WebKit one.

## Notes

WebKit's `Audit` runs JavaScript test functions in the debuggee — closer to a scriptable assertion runner than to Lighthouse, and not a substitute for it.
