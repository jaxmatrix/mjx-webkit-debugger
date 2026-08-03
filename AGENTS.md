# AGENTS.md

Vendor-neutral entry point for coding agents working in **mjx-webkit-debugger**.

The full, authoritative guidance lives in:

- [`CLAUDE.md`](CLAUDE.md) — architecture rules, the protocol traps, the threading model and perf
  budgets, the Plan → Plan-Optimization → thorough-atomic process, and commands.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — the test-driven workflow, the fixture tiers, file
  ownership, the seam-change protocol, and git/commit conventions.
- [`DESIGN.md`](DESIGN.md) — the visual and interaction contract. Tokens, gutter states, keymap.
- [`docs/SEAMS.md`](docs/SEAMS.md) — **the frozen interface contract for the whole product.**
- [`PLAN.md`](PLAN.md) — the phased roadmap and current status.

## What this project is

A **native GUI debugger for WebKit programs**. It attaches to an already-running WebKit
application over the Remote Inspector protocol and gives a Chrome-DevTools-grade experience:
browse every source file, set breakpoints, pause, inspect state.

## The short version

- **The debugger is not itself a WebKit program.** No webview, ever, in the shipped dependency
  graph — not Tauri, not wry, not webkit2gtk. A crash or hang in the debuggee must not be able to
  take the debugger with it. Enforced by `cargo run -p xtask -- verify-no-webview`.
- **Everything above `mjx-wk-dialect` speaks WebKit's vocabulary**, including when the wire is
  Chromium's. Translation happens in one place.
- **Layering points downward only.** L0 protocol → L1 dialect/transport → L2 session → L3 source →
  L4 features → L5 UI → L6 app. **The nine L4 feature crates are peers and must never depend on
  one another** — that rule is what keeps phases parallel.
- **The UI thread never awaits, never blocks, never allocates per frame.** Widgets are pure
  functions of a snapshot that return `Action`s.
- **WebKit is not Chrome.** Do not code the protocol from CDP memory; see the traps in `CLAUDE.md`
  and `docs/PROTOCOL-NOTES.md`. `Runtime.getProperties` is paginated, there is no `Fetch` domain,
  there is no `/json/list`, and `Target.*` wraps frames as JSON *strings*.
- **Test-driven & incremental** — write the failing test first, against a fixture; keep every
  increment green. A test that needs a running browser is a test that will not run in CI.
- **Do the work thoroughly and correctly — no monkey-patching.** Optimize the design (memory,
  speed, reliability) *before* coding.
- **Atomic commits, no `Co-Authored-By` / AI-attribution trailers.**
- **Say what is true.** If a feature is not implemented, the README says so rather than the UI
  pretending. `docs/CHROME-PARITY.md` tracks the gaps honestly.

## Working on a task

Implementation work is decomposed into files under [`docs/tasks/`](docs/tasks/). Each names the
seam it implements, the paths it **owns**, the paths it **must not touch**, and the fixtures that
must go green. Pick one up, stay inside its ownership boundary, and do not invent interfaces — if
the seam is wrong, that is a separate change to `docs/SEAMS.md` first.

## `reference/` is git-ignored

`reference/webkit-protocol/` holds WebKit's protocol descriptions, pinned to `webkitgtk-2.52.3`.
It is local-only and **never staged**. The generated Rust under
`crates/mjx-wk-protocol/src/generated/` **is** committed, so a clean clone builds without it. See
[`reference/README.md`](reference/README.md).

## Commands

```sh
cargo build   --workspace
cargo test    --workspace
cargo clippy  --workspace --all-targets -- -D warnings
cargo run -p xtask -- codegen            # regenerate protocol types (needs reference/)
cargo run -p xtask -- verify-protocol    # generated types vs the installed WebKit
cargo run -p xtask -- verify-no-webview  # the architectural rule, enforced
cargo run -p xtask -- record --scenario attach --out fixtures/attach.jsonl
```
