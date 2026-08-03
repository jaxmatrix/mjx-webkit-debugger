# Tasks

47 independently-runnable units of work, **one session each**. Every file is a complete ticket body:
an isolated session should be able to work from it alone, without reading four other things first.

Each names the seam it fills, the paths it **owns**, the paths it **must not touch**, and the
fixtures that must go green. Pick one up, stay inside its ownership boundary, and do not invent
interfaces — if a seam is wrong, that is a separate seam-change PR, merged first. See
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) for why those rules exist.

**Tracked in Linear** as project `MJX-DEVTOOLS`, one issue per file. Linear owns *state*; these
files own *detail*.

## Ids are stable

Ids are assigned once and never reused or renumbered — the crate stubs reference these filenames in
doc comments, so a renumber would invalidate them. That is why the numbering is not sequential:
`T-011` and `T-012` are v0.1 work split out of `T-005` and `T-009` after the fact.

---

## v0.1 — A working source browser (16)

| Task | What | Blocked by |
|---|---|---|
| [T-000](T-000-inspector-handshake.md) | **Complete the inspector server handshake** — spike | — |
| [T-013](T-013-fixture-corpus.md) | Record the fixture corpus | T-000 |
| [T-001](T-001-target-discovery.md) | Socket protocol framing and target discovery | T-000 |
| [T-002](T-002-socket-transport.md) | Transport over the inspector socket | T-000, T-001 |
| [T-003](T-003-session-correlation.md) | Session correlation, event fan-out, `Target.*` demux | — |
| [T-004](T-004-source-inventory.md) | Script and resource inventory | — |
| [T-005](T-005-source-text.md) | Source text and line index | — |
| [T-011](T-011-source-store.md) | Source store — fetch, cache, request dedup | — |
| [T-006](T-006-syntax-highlighting.md) | Tree-sitter highlighting | — |
| [T-007](T-007-pretty-printer.md) | Pretty-printer and position map | — |
| [T-008](T-008-code-view.md) | Theme tokens and the virtualised code view | — |
| [T-009](T-009-source-tree.md) | Source tree widget | — |
| [T-012](T-012-search.md) | Search — index and bar | — |
| [T-010](T-010-app-shell.md) | App shell, dock, and wiring | — |
| [T-014](T-014-perf-benches.md) | Perf bench harness | — |
| [T-015](T-015-packaging.md) | Packaging and release | T-010 |

**T-000 is the only real blocker.** Everything except discovery, transport and fixture capture is
written against seams and fixtures and can start today, in parallel, without a browser.

## v0.2 — A working debugger (7)

| Task | What | Blocked by |
|---|---|---|
| [T-201](T-201-breakpoints.md) | Breakpoint store and the `Debugger` agent | v0.1 |
| [T-206](T-206-dom-debugger-breakpoints.md) | DOM, event, URL and symbolic breakpoints | v0.1 |
| [T-207](T-207-breakpoint-ui.md) | Breakpoint list and condition/action editor | T-201, T-008 |
| [T-202](T-202-pause-and-stepping.md) | Pause state, call stack, and stepping | v0.1 |
| [T-203](T-203-variable-tree.md) | Lazy, paginated variable tree | v0.1 |
| [T-204](T-204-console.md) | Console panel and evaluation | v0.1 |
| [T-205](T-205-source-maps.md) | Source maps | v0.1 |

## v0.3 — Apple platforms and network (6)

| Task | What | Blocked by |
|---|---|---|
| [T-300](T-300-macos-transport.md) | macOS `webinspectord` transport | v0.1 |
| [T-302](T-302-ios-usb-transport.md) | iOS ≤16 transport over usbmux | T-300 |
| [T-303](T-303-ios17-rsd-spike.md) | **Spike:** does iOS 17+ need an RSD tunnel? | — |
| [T-301](T-301-network-agent.md) | `Network` agent and request lifecycle | v0.1 |
| [T-304](T-304-network-panel.md) | Network panel — table, waterfall, viewers | T-301 |
| [T-305](T-305-network-extras.md) | WebSocket frames, interception, HAR export | T-301, T-304 |

## v0.4 — Chromium and Android (5)

| Task | What | Blocked by |
|---|---|---|
| [T-403](T-403-cdp-dialect.md) | `CdpDialect` — encode and decode | v0.1 |
| [T-404](T-404-cdp-transport.md) | `CdpTransport` — WebView2 and Chrome | T-403 |
| [T-405](T-405-android-transport.md) | `AndroidAdbTransport` | T-403, T-404 |
| [T-406](T-406-apple-rsd-transport.md) | `AppleRsdTransport` — *scope only after T-303* | T-303 |
| [T-407](T-407-cross-engine-validation.md) | Cross-engine validation pass | T-403, T-404 |

## v0.5 — Profiling (4)

| Task | What | Blocked by |
|---|---|---|
| [T-501](T-501-timeline.md) | `Timeline` domain and the record tree | v0.1 |
| [T-502](T-502-profilers-flame-graph.md) | Script and CPU profilers with a flame graph | T-501 |
| [T-503](T-503-heap-snapshots.md) | Heap snapshots and retaining paths | T-501 |
| [T-504](T-504-memory-timeline.md) | Memory timeline and the performance panel shell | T-501 |

## v0.6 — Elements and styles (4)

| Task | What | Blocked by |
|---|---|---|
| [T-601](T-601-dom-model.md) | DOM tree model with incremental mutation | v0.1 |
| [T-603](T-603-dom-widget.md) | DOM tree widget, element picker, overlays | T-601 |
| [T-602](T-602-css-model.md) | Matched, inherited and computed styles | v0.1 |
| [T-604](T-604-styles-panel.md) | Styles and Computed panels with live editing | T-602 |

## v0.7 — Storage, graphics, audits (5)

| Task | What | Blocked by |
|---|---|---|
| [T-701](T-701-storage-model.md) | Storage, IndexedDB, cookies and workers | v0.1 |
| [T-704](T-704-storage-panel.md) | Storage panel | T-701 |
| [T-702](T-702-canvas-inspection.md) | Canvas contexts, recordings, shader editing | v0.1 |
| [T-705](T-705-layers-animations.md) | Layer tree and animations | v0.1 |
| [T-703](T-703-audits.md) | Audits | v0.1 |

---

## Template

[`TASK-TEMPLATE.md`](TASK-TEMPLATE.md) — use it when adding a task, and keep the section order,
since these files are copied verbatim into Linear.
