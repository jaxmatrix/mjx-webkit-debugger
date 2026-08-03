# Tasks

Independently-runnable units of work. Each names the seam it fills, the files it **owns**, the
files it **must not touch**, and the fixtures that must go green.

Pick one up, stay inside its ownership boundary, and do not invent interfaces. See
`CONTRIBUTING.md` for why those rules exist and what to do when one gets in the way.

## Phase 1 — a working source browser (v0.1.0)

| Task | What | Status |
|---|---|---|
| [T-000](T-000-inspector-handshake.md) | **Complete the inspector server handshake** | **open — blocks T-001, T-002, and all live fixtures** |
| [T-001](T-001-target-discovery.md) | Socket protocol framing and target discovery | blocked on T-000 |
| [T-002](T-002-socket-transport.md) | `Transport` over the inspector socket | blocked on T-000 |
| [T-003](T-003-session-correlation.md) | Session correlation, event fan-out, `Target.*` demux | ready |
| [T-004](T-004-source-inventory.md) | Script and resource inventory | ready |
| [T-005](T-005-source-text-store.md) | Source text, line index, fetch and cache | ready |
| [T-006](T-006-syntax-highlighting.md) | Tree-sitter highlighting | ready |
| [T-007](T-007-pretty-printer.md) | Pretty-printer and position map | ready |
| [T-008](T-008-code-view.md) | The virtualised code view | ready |
| [T-009](T-009-source-tree-and-search.md) | Source tree and search widgets | ready |
| [T-010](T-010-app-shell.md) | App shell, dock, and wiring | ready |

T-003…T-009 are mutually parallel-safe and none of them needs a live debuggee: they are written
against fixtures and unit tests. T-010 depends on seams only, so it can start immediately and goes
green last.

## Later phases

| Task | Phase | What |
|---|---|---|
| [T-201](T-201-breakpoints.md) | 2 | Breakpoints, including WebKit actions and probes |
| [T-202](T-202-pause-and-stepping.md) | 2 | Pause state, call stack, stepping |
| [T-203](T-203-variable-tree.md) | 2 | Lazy, paginated variable tree |
| [T-204](T-204-console.md) | 2 | Console panel and evaluation |
| [T-205](T-205-source-maps.md) | 2 | Source maps |
| [T-300](T-300-apple-transports.md) | 3 | macOS and iOS ≤16 transports, plus the iOS 17 spike |
| [T-301](T-301-network-panel.md) | 3 | Network panel |
| [T-403](T-403-cdp-dialect.md) | 4 | CDP dialect, transport, and Android |
| [T-501](T-501-profiling.md) | 5 | Timeline, profiles, heap |
| [T-601](T-601-dom-tree.md) | 6 | DOM tree |
| [T-602](T-602-styles-panel.md) | 6 | Styles and Computed |
| [T-701](T-701-storage-panel.md) | 7 | Storage and application |
| [T-702](T-702-graphics-panel.md) | 7 | Canvas, layers, animations |
| [T-703](T-703-audits.md) | 7 | Audits |
