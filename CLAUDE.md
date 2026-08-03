# CLAUDE.md — guidance for AI agents working in mjx-webkit-debugger

This file orients Claude Code (and any coding agent) working in this repo. Humans: see
`README.md`, `PLAN.md`, and `CONTRIBUTING.md`. The frozen interfaces are in `docs/SEAMS.md`.

## What this project is

A **native GUI debugger for WebKit programs**, in Rust, for Windows/macOS/Linux. It attaches over
the WebKit Remote Inspector protocol to an already-running application and gives a
Chrome-DevTools-grade experience: browse every source, set breakpoints, pause, inspect state.

**The debugger is not itself a WebKit program.** That is the central architectural claim, not a
detail. Every webview-based devtool shares a process model with its debuggee, so when the debuggee
wedges the compositor or exhausts memory, the tools go down with it — precisely when they are
needed. This one renders through `egui`/`wgpu` and links no engine. CI enforces it:
`cargo run -p xtask -- verify-no-webview`.

## How we work here (non-negotiable process)

Every unit of work follows: **Plan → Plan-Optimization → thorough atomic implementation.**

1. **Plan** the atomic piece of work.
2. **Plan-Optimization** — *before writing code*, refine the design for **low memory, fast and
   reliable execution, and correctness**. Weigh allocations, copies, cache behaviour, and failure
   modes. **No monkey-patching or shortcuts** — choose the design that is right, not the one that
   merely works.
3. **Thorough atomic implementation** — finish the piece *completely, correctly, with tests*
   before moving on. No half-done atoms, no "fix it later" placeholders in shipped code.
4. **Discussion-first** — begin a session by discussing the plan for what is about to be
   implemented. Extra planning time is welcome.

Work is decomposed into `docs/tasks/T-*.md`. Each names the seam it implements, the files it
**owns**, the files it **must not touch**, and the fixtures that must go green. Stay inside the
ownership boundary. **Do not invent interfaces** — if a seam is wrong, changing `docs/SEAMS.md` is
a separate, serial change that comes first.

## Architecture rules

- **Layering: dependencies point downward only.**

  ```
  L0  mjx-wk-protocol    generated domain types + envelope + Command/Event traits
  L1  mjx-wk-dialect     WebKit RWI ⇄ normalized ⇄ CDP
  L1  mjx-wk-transport   Transport · Discovery · Tcp · Replay
  L2  mjx-wk-session     correlation, event fan-out, Target.* demux, capability gating
  L3  mjx-wk-source      inventory, text, line index, highlighting, source maps
  L4  mjx-wk-{debug,console,network,dom,css,profile,storage,graphics,audit}
  L5  mjx-wk-ui          Panel trait + egui widgets
  L6  mjx-webkit-debugger
  ```

- **The nine L4 crates are peers and must never depend on one another.** This is the rule that
  keeps phases parallel. If `mjx-wk-css` needs a node, it takes a `mjx_wk_source::NodeId`, never a
  `&DomTree` from `mjx-wk-dom`. Shared vocabulary lives in L3.
- **`mjx-wk-ui` never holds a session.** Widgets are pure functions of a snapshot returning
  `Action`s. This is what guarantees the UI thread cannot block.
- **Everything above `mjx-wk-dialect` speaks WebKit's vocabulary**, even when the wire is
  Chromium's. Translation happens in exactly one place.
- **`unsafe_code = "deny"`** workspace-wide. A crate that truly needs it must
  `#[allow(unsafe_code)]` locally **with a written safety justification**.
- **No `unwrap`/`expect`/`panic` on protocol input.** The debuggee is untrusted: it may be the
  buggy program you are debugging. Return typed `thiserror` errors. `anyhow` only in `xtask`,
  tests, and the binary's top level. Clippy warns on `unwrap_used`/`expect_used`; `clippy.toml`
  exempts tests, where a failed unwrap *is* the assertion.

## Protocol rules — WebKit is not Chrome

Do not code this protocol from CDP memory. Verified against WebKitGTK 2.52.3; full notes in
`docs/PROTOCOL-NOTES.md`.

1. **There is no `/json/list`.** Target discovery is an HTML page you scrape; the WebSocket path
   comes out of a button's `onclick` and has the shape `/socket/{connection}/{target}/{page}`.
2. **`Target.*` wraps frames as JSON *strings*.** `Target.sendMessageToTarget` and
   `dispatchMessageFromTarget` carry the real frame as a string inside a JSON object. Handle it in
   `mjx-wk-dialect` or every domain breaks on the first multi-process page.
3. **`Runtime.getProperties` is paginated** (`fetchStart`, `fetchCount`). Asking for everything is
   how a debugger hangs on a large array.
4. **`Page.getResourceContent` is keyed by URL**, not by a request id, and may return base64.
5. **There is no `Fetch` domain** (use `Network.addInterception`), **no `Storage` domain** (cookies
   are on `Page`), and **no `Profiler` domain** (it is `ScriptProfiler` + `CPUProfiler` + `Heap` +
   `Memory`).
6. **Remote object handles die on resume.** Every `objectId` is invalid the moment execution
   continues. Clear the variable tree on `Debugger.resumed` and `Debugger.globalObjectCleared`.
7. **Breakpoints are set by URL, not script id**, so they survive reloads. The debuggee replies
   with `breakpointResolved` giving the *actual* location, which may differ from the one asked
   for. Render requested and resolved differently — Chrome's hollow-versus-filled distinction.
8. **Domains are gated per target type.** A service worker has no `Page`. Never assume.

WebKit also has capabilities Chrome lacks, and they are cheap once the seam exists: breakpoint
**actions** (log / evaluate / **probe** / sound — the probe shows live values inline without
stopping), `setPauseOnMicrotasks`, `setPauseOnAssertions`, `addSymbolicBreakpoint`,
`setShouldBlackboxURL`, and `Canvas` shader inspection.

## Threading model

- **One tokio task owns the transport.** It is the only thing that awaits on it.
- **The egui thread never awaits, never blocks, never allocates per frame.**
- UI → session: an `mpsc` channel of `Action`s. Session → UI: `ArcSwap<Snapshot>` per
  `DomainAgent`, so reading state is a pointer copy however large it is.
- A 5 MB `getScriptSource` reply is parsed and line-indexed on the session side before the UI is
  handed a pointer to it.

## Performance budgets (asserted by benches in CI)

| Budget | Why |
|---|---|
| attach → source tree visible < 300 ms | the first thing anyone does |
| 5 MB minified bundle scrolls at 60 fps | the case that breaks naive editors |
| pause → first variable row < 100 ms | a debugger that stutters at a breakpoint is unusable |
| no UI frame > 16 ms, ever | the non-negotiable one |

## Settled implementation choices

- **UI:** `egui` + `eframe` + `wgpu` + `egui_dock`. Dual MIT/Apache-2.0, no system webview.
- **Highlighting:** `tree-sitter`, incremental, computed for the visible window only.
- **Protocol types:** generated by `xtask` from `reference/webkit-protocol/`, **output committed**,
  never a `build.rs`. Regenerate with `cargo run -p xtask -- codegen`.
- **Fixtures:** recorded traces replayed through `ReplayTransport`. A test that needs a running
  browser is a test that will not run in CI.
- **Errors:** `thiserror` in libraries, `anyhow` in tooling and the binary.

## Commands

```sh
cargo build   --workspace
cargo test    --workspace
cargo clippy  --workspace --all-targets -- -D warnings
cargo fmt --all

cargo run -p xtask -- codegen            # regenerate protocol types (needs reference/)
cargo run -p xtask -- verify-protocol    # generated types vs the installed WebKit
cargo run -p xtask -- verify-no-webview  # the architectural rule, enforced
cargo run -p xtask -- record --scenario attach --out fixtures/attach.jsonl
```

To record against the local browser:

```sh
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser https://example.com &
cargo run -p xtask -- record --scenario attach --save-targets-page
```

## Git / commits

- **Atomic commits.** One reviewable idea per commit, each building and testing green *on its own*.
  A ticket produces four to eight commits, not one, and scaffolding is a sequence rather than a
  single drop. If the subject needs an "and", it is two commits. The full rule, with its worked
  example and size heuristic, is *Atomic commits* in [`CONTRIBUTING.md`](CONTRIBUTING.md) — read it
  before your first commit here.
- **Do NOT add `Co-Authored-By` or any AI-attribution trailer**, in commits or PR bodies.
- **Project-setup commits go on `main`;** once features start, **branch per ticket + open a PR**.
- `reference/` is git-ignored — never stage it. Fixtures belong in `fixtures/`.

## Say what is true

If a capability is not implemented, `README.md` and `docs/CHROME-PARITY.md` say so, rather than the
UI pretending. A panel whose protocol members are unavailable renders **disabled with a reason**,
never hidden and never silently broken.
