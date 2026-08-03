# T-015 — Packaging and release

**Phase** 1 · **Milestone** v0.1 — Source browser
**Blocked by** T-010 · **Parallel-safe with** every other v0.1 ticket

## Before you start

Read [`AGENTS.md`](../../AGENTS.md), then [`docs/SEAMS.md`](../SEAMS.md) for the frozen interfaces
and the relevant traps in [`docs/PROTOCOL-NOTES.md`](../PROTOCOL-NOTES.md).

**Do not invent interfaces.** Every seam already exists as compiling Rust with `todo!()` bodies;
this ticket fills bodies. If a seam is genuinely wrong, that is a separate seam-change PR, merged
first.

Branch `t-015-packaging`. **Commit atomically** — one reviewable idea per commit, each green on its own;
expect several commits from this ticket, not one. No `Co-Authored-By` trailer. See *Atomic commits*
in [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Goal

Make v0.1 something a person can install. Done when a tagged commit produces downloadable binaries
for Linux, macOS and Windows, and the README tells someone how to get one.

## Seam

No seam. Build and release configuration.

## Owns

- `.github/workflows/release.yml`
- `dist-workspace.toml` or equivalent
- the *Getting started* section of `README.md`

## Must not touch

- everything under `crates/`

## Fixtures

None.

## Done criteria

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- a tag produces binaries for linux-x86_64, linux-aarch64, macos-universal, windows-x86_64;
- checksums are published alongside;
- the Linux build documents its runtime requirements (`wgpu` needs a working GPU stack; name the
  packages);
- `README.md` no longer says the app does not yet attach to anything — **only once that is true**;
- `cargo run -p xtask -- verify-no-webview` runs in the release job too, so a shipped binary can
  never contain a webview.

## Notes

The claim on the tin is that this is a debugger that cannot be taken down by its debuggee. A
release that quietly links a system webview would falsify that, which is why the invariant check
belongs in the release job and not only in CI.
