# Chrome DevTools parity

Every Chrome DevTools feature, mapped onto the WebKit domain that implements it. This is the
backlog for every phase after the first, and the source of the honest limitations in `README.md`.

Legend: **direct** — a WebKit member does the same job · **ours** — we implement it client-side ·
**better** — WebKit offers more than Chrome · **gap** — no equivalent, will not be implemented ·
**scope** — deliberately out of scope.

## Sources / debugger — Phases 1–2

| Chrome feature | WebKit | Status | Phase |
|---|---|---|---|
| File tree, open by name | `Debugger.scriptParsed` ⊕ `Page.getResourceTree` | direct | 1 |
| Search in file / all files | `Page.searchInResource`, `searchInResources` | direct | 1 |
| Pretty-print (`{}`) | — | ours | 1 |
| Syntax highlighting, code folding | — | ours (tree-sitter) | 1 |
| Line-of-code breakpoint | `Debugger.setBreakpointByUrl` | direct | 2 |
| Conditional breakpoint | `options.condition` | direct | 2 |
| Logpoint | `options.actions` = `Log` + `autoContinue` | **better** — also `Evaluate`, `Probe`, `Sound` | 2 |
| DOM change breakpoints (subtree / attribute / removal) | `DOMDebugger.setDOMBreakpoint` | direct | 2 |
| XHR/fetch breakpoints | `DOMDebugger.setURLBreakpoint` | direct | 2 |
| Event listener breakpoints | `DOMDebugger.setEventBreakpoint` | direct | 2 |
| Exception breakpoints (caught / uncaught) | `Debugger.setPauseOnExceptions` | direct | 2 |
| Function breakpoint (`debug(fn)`) | `Debugger.addSymbolicBreakpoint` | direct | 2 |
| Step over / into / out | `Debugger.stepOver` / `stepInto` / `stepOut` | direct | 2 |
| Continue to here | `Debugger.continueToLocation` | direct | 2 |
| — | `Debugger.stepNext`, `continueUntilNextRunLoop` | **better** | 2 |
| — | `setPauseOnMicrotasks`, `setPauseOnAssertions` | **better** | 2 |
| Call stack, async frames | `Debugger.paused.asyncStackTrace` | direct | 2 |
| Scope pane, editing | `Debugger.paused.callFrames[].scopeChain` | direct | 2 |
| Inline values beside declarations | probe actions + `Runtime` type profiler | **better** | 2 |
| Watch expressions | `Debugger.evaluateOnCallFrame` | direct | 2 |
| Blackboxing / ignore list | `Debugger.setShouldBlackboxURL` | direct | 2 |
| Source maps | `scriptParsed.sourceMapURL` | ours | 2 |
| Copy stack trace | — | ours | 2 |
| **Restart frame** | — | **gap** | — |
| **Trusted Type breakpoints** | — | **gap** | — |
| Live edit, workspaces, snippets | — | **scope** | — |

## Console — Phase 2

| Chrome feature | WebKit | Status |
|---|---|---|
| Message log, levels, grouping | `Console.messageAdded` | direct |
| Repeat collapsing | `Console.messageRepeatCountUpdated` | direct |
| Evaluate, with autocompletion | `Runtime.evaluate` / `Debugger.evaluateOnCallFrame` | direct |
| Object expansion in output | `Runtime.getProperties` | direct |
| Logging channel levels | `Console.setLoggingChannelLevel` | **better** |
| AI assistance | — | **scope** |

## Network — Phase 3

| Chrome feature | WebKit | Status |
|---|---|---|
| Request table, waterfall, timing | `Network` lifecycle events | direct |
| Headers, payload, response, preview | `Network.getResponseBody` | direct |
| WebSocket frames | `Network.webSocketFrame*` | direct |
| Request blocking / override | `Network.addInterception` family (**not** `Fetch`) | direct |
| Throttling | `Network.setEmulatedConditions` | direct* |
| HAR export | — | ours |
| Extra HTTP headers | `Network.setExtraHTTPHeaders` | direct |
| Certificate details | `Network.getSerializedCertificate` | direct |

\* not exposed by the WebKitGTK build tested; degrades honestly.

## Performance & Memory — Phase 5

| Chrome feature | WebKit | Status |
|---|---|---|
| Performance recording | `Timeline.start` / `stop`, `eventRecorded` | direct |
| JS sampling profile, flame chart | `ScriptProfiler` | direct |
| Per-thread CPU | `CPUProfiler` | direct |
| Memory timeline by category | `Memory` | direct |
| Heap snapshot, retaining paths | `Heap.snapshot` | direct |
| GC events | `Heap.garbageCollected` | direct |
| Core Web Vitals, Insights, Lighthouse | — | **scope** |

## Elements & Styles — Phase 6

| Chrome feature | WebKit | Status |
|---|---|---|
| DOM tree, edit as HTML, attributes | `DOM` (78 members) | direct |
| Element picker, highlight overlays | `DOM.setInspectModeEnabled`, `highlightNode` | direct |
| Grid / flex overlays | `DOM.showGridOverlay`, `showFlexOverlay` | direct |
| Matched, inherited, pseudo rules | `CSS.getMatchedStylesForNode` | direct |
| Computed panel | `CSS.getComputedStyleForNode` | direct |
| Live style editing | `CSS.setStyleText`, `setRuleSelector`, `addRule` | direct |
| Force element state (`:hover`) | `CSS.forcePseudoState` | direct |
| Fonts panel | `CSS.getFontDataForNode` | direct |
| Event listeners | `DOM.getEventListenersForNode` | direct |
| Accessibility pane | `DOM.getAccessibilityPropertiesForNode` | direct |
| Layers | `LayerTree` + `reasonsForCompositingLayer` | direct |
| Animations | `Animation` (18 members) | direct |
| Rendering overlays (paint flashing, rulers) | `Page.setShowPaintRects`, `setShowRulers` | direct |
| CSS overview, Changes tab | — | **scope** |

## Application — Phase 7

| Chrome feature | WebKit | Status |
|---|---|---|
| Local / session storage | `DOMStorage` | direct |
| IndexedDB | `IndexedDB` | direct |
| Cookies | `Page.getCookies` (**not** a `Storage` domain) | direct |
| Service workers, workers | `ServiceWorker`, `Worker` | direct |
| Manifest, background services | — | **scope** |

## WebKit-only, no Chrome counterpart — Phase 7

| Feature | Domain |
|---|---|
| Canvas inspection, recording, live shader editing | `Canvas` (28), `Recording` |
| Scriptable audits inside the debuggee | `Audit` |
| Media statistics | `DOM.getMediaStats` |
| Browser extension inventory | `Browser` |

## Panels we will not build

Lighthouse, Recorder, Issues, WebAudio, WebAuthn, Sensors, Autofill, Developer Resources, Protocol
Monitor. Each depends on Chromium-specific instrumentation with no WebKit counterpart, or is a
product rather than a debugger feature.
