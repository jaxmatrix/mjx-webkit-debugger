## What and why

<!-- What changed, and the reason. Link the task file if there is one. -->

Task: `docs/tasks/T-NNN-...md`

## Checklist

- [ ] Stayed inside the task's **Owns** list; touched nothing in **Must not touch**
- [ ] No interface changed — or, if one did, it is a separate seam-change PR merged first
- [ ] Tests are fixture-backed and pass with **no WebKit running**
- [ ] `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace`
- [ ] `cargo run -p xtask -- verify-no-webview`
- [ ] **Commits are atomic** — each builds and tests green on its own, and no subject needs an "and"
- [ ] No `Co-Authored-By` or AI-attribution trailer in any commit or in this description
- [ ] `reference/` is not staged
- [ ] If a UI token changed, `DESIGN.md` changed in the same commit
- [ ] If a capability is unimplemented, `docs/CHROME-PARITY.md` and `README.md` say so
