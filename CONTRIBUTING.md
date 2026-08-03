# Contributing to mjx-webkit-debugger

This project is built **deliberately, test-first, and incrementally**, and it is built by many
people (and agents) working in parallel. Both of those shape the workflow below.

## The development loop (TDD)

Every change follows **red → green → refactor**:

1. **Red** — write a failing test first. Prefer a *fixture* test: a recorded protocol trace
   replayed through `ReplayTransport`.
2. **Green** — write the minimum code to make it pass.
3. **Refactor** — clean it up with the tests still green.

Before writing code for a non-trivial piece, do the **Plan → Plan-Optimization** step: decide the
design and *optimize it for memory, speed, and reliability first*. We prefer the correct design
over the merely-working one. See `CLAUDE.md`.

## How parallel work stays parallel

Three mechanisms, and all three matter.

### 1. The seam freeze

`docs/SEAMS.md` holds every cross-crate interface, and all of it exists in the workspace as
compiling Rust with `todo!()` bodies. **A task fills bodies; it never invents an interface.**

If a seam is genuinely wrong, that is a **seam change**: a separate PR touching only
`docs/SEAMS.md` and the signature, reviewed on its own, merged before any task depends on it.
Slipping an interface change into a feature PR is what turns ten independent tasks into ten
conflicts.

### 2. File ownership

Each `docs/tasks/T-*.md` lists the paths it **owns** and the paths it **must not touch**. The lists
are disjoint across tasks that can run at the same time.

Every cross-cutting edit was made up front: the workspace `Cargo.toml`, all seventeen crate
manifests, and every `mod` declaration — including modules for panels that do not exist yet, whose
files are stubs waiting for their task. So **no task ever edits a shared file**, and two tasks that
both pass their tests merge cleanly.

If you find yourself needing to edit a file you do not own, stop. Either the task boundary is
wrong or the change belongs in a seam-change PR.

### 3. Fixtures as the integration contract

A task is done when its fixture tests pass **with no WebKit running**. If two tasks pass their
fixtures, they integrate.

## Test tiers

1. **Unit** — pure logic, no I/O. Parsers, models, translation tables.
2. **Fixture** — a recorded trace replayed through `ReplayTransport`. This is the default tier and
   where most tests belong.
3. **Snapshot** — widget rendering, via `egui_kittest`.
4. **Live** — against a real MiniBrowser. Linux-gated, `#[ignore]` by default, run explicitly.
   Never a gate on a PR from someone without WebKitGTK installed.

**A replayed send that the trace does not cover is a failure, not a no-op.** That is deliberate: it
stops a test passing while exercising nothing. If you need a frame the fixture lacks, re-record it:

```sh
WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser https://example.com &
cargo run -p xtask -- record --scenario attach --out fixtures/attach.jsonl
```

Recording is scripted from a named scenario rather than proxying a human-driven inspector,
precisely so a fixture can be regenerated rather than being a one-off nobody can reproduce.

## Required checks (green before every commit)

```sh
cargo fmt --all
cargo build  --workspace
cargo test   --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p xtask -- verify-no-webview
```

CI runs these on Linux, macOS, and Windows, plus a strict `cargo doc` job. On Linux it also runs
`cargo run -p xtask -- verify-protocol`, which extracts the protocol description from the installed
WebKit and diffs it against what we generated — so a WebKit upgrade is caught at build time rather
than discovered as a runtime error.

## Adding a new feature domain

The shape is fixed, which is the point:

1. A new L4 crate, depending on `mjx-wk-session` and `mjx-wk-source` and **on no other L4 crate**.
2. One `DomainAgent` implementation: `attach` enables its domains, `on_event` folds, `snapshot`
   publishes.
3. One `Panel` implementation in `mjx-wk-ui`, over that model, holding no session.
4. One line registering it in the app.
5. A recorded fixture and tests.

Nothing already written changes. If your feature seems to need a change elsewhere, say so in the
PR description — it usually means the seam missed something, which is worth knowing.

## Code style

- Pure-Rust dependencies. `unsafe` is denied workspace-wide; if genuinely required,
  `#[allow(unsafe_code)]` locally **with a written safety justification**.
- No `unwrap`/`expect`/`panic` on protocol input in library paths — the debuggee is untrusted.
- Respect the layering. Dependencies point downward only; L4 crates are peers.
- Comments explain *why*, not *what*. The protocol has a lot of non-obvious behaviour, and a
  comment recording which trap a line avoids is worth more than one restating the line.

## Git & commit conventions

- **Atomic commits** — one self-contained change per commit. Split unrelated changes.
- **Commit only when green** — a test is committed with or before the code it covers.
- **No `Co-Authored-By` or AI-attribution trailers.** Keep messages plain: imperative subject, body
  explaining *why*. Conventional-commit prefixes are encouraged: `feat(session):`, `fix(dialect):`,
  `chore:`, `docs:`, `test:`, `refactor:`.
- **Branching:** setup commits go directly on `main`. Once task work begins, branch per task
  (`t-004-source-inventory`) and consolidate via a pull request.
- **Never stage `reference/`** — it is git-ignored local-only material. Fixtures go in `fixtures/`.

## Reporting honestly

If a capability is not implemented, `README.md` and `docs/CHROME-PARITY.md` say so. A panel whose
protocol members are unavailable renders **disabled with a reason** — never hidden, never silently
broken. A user who cannot tell whether a feature is missing or just not working has been failed
twice.
