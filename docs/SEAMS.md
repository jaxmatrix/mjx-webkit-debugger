# SEAMS — the frozen interface contract

Every cross-crate interface in the product, for every phase, including phases that have not been
scoped. **All of it already exists in the workspace as compiling Rust with `todo!()` bodies.**

That is the point. A task fills bodies; it never invents an interface. Ten people can work at once
because the shape of what they are building was agreed before any of them started.

## The rule

> **The code is the seam. This document is the map.**

Signatures live in the crates and are the authority — a doc that repeats them goes stale and starts
lying. This file records **what each seam is for, why it is shaped that way, and what is not
allowed to change without a seam-change PR.**

### Changing a seam

A separate PR touching only the signature and this file, reviewed on its own, merged before any
task depends on it. Slipping an interface change into a feature PR turns ten independent tasks into
ten conflicts. See `CONTRIBUTING.md`.

## Layering

Dependencies point **downward only**. Sideways is forbidden and that is what keeps phases parallel.

```
L0  mjx-wk-protocol    Command, Event, Frame, Domain, generated::*
L1  mjx-wk-dialect     Dialect, DialectKind, Support, TargetId, NormalizedFrame
L1  mjx-wk-transport   Transport, Discovery, TargetKey, TransportOrigin
L2  mjx-wk-session     SessionHandle, DomainAgent, Capabilities, Subscription
L3  mjx-wk-source      SourceId, NodeId, RequestId, FrameId, SourceLocation, SourceKind
L4  mjx-wk-{debug, console, network, dom, css, profile, storage, graphics, audit}
L5  mjx-wk-ui          Panel, Action, PanelCtx, SupportQuery, Theme
L6  mjx-webkit-debugger
```

**The nine L4 crates are peers and must never depend on one another.** If `mjx-wk-css` needs a DOM
node it takes a `mjx_wk_source::NodeId`, never a `&DomTree`. All shared vocabulary lives in L3, and
composition happens only in L6.

---

## L0 · `mjx-wk-protocol` — what a frame is

| Item | Purpose | Frozen because |
|---|---|---|
| `Command` | ties a request type to its method name **and its reply type** | the whole call site is `session.call(SomeCommand { .. })` with the return type inferred; changing it changes every caller |
| `Event` | ties an event type to its method name | same |
| `Frame` | the four wire message shapes | classification is written by hand, not `#[serde(untagged)]`, which would silently pick the first arm that parses |
| `Domain` | 26 domains with exact wire spellings | `CSS` not `Css`, `CPUProfiler` not `CpuProfiler`; get it wrong and methods silently never match |
| `DebuggableType` / `TargetType` | **two** vocabularies, 5 and 7 values | the protocol genuinely has both; `targetTypes` adds `page` and `worker`. Conflating them mis-gates domains on multi-process pages |
| `generated::*` | 27 modules, 239 commands, 110 events | produced by `xtask codegen`, output committed |

Generated layout: types at a module's root (that is how cross-domain `$ref`s address them),
commands and events in their own namespaces (which is what keeps `Animation`'s `TrackingUpdate`
type from colliding with its `trackingUpdate` event).

```
generated::debugger::Location            // a type
generated::debugger::commands::Resume    // a command
generated::debugger::events::Paused      // an event
```

## L1 · `mjx-wk-dialect` — WebKit ⇄ Chromium

| Item | Purpose |
|---|---|
| `Dialect` | `encode` / `decode` / `supports` |
| `Support` | `Native` \| `Emulated` \| `Unsupported` — what a panel consults before offering a control |
| `TargetId` | protocol-level target, distinct from a transport's discovery key |
| `NormalizedFrame` | a WebKit-shaped frame plus which target produced it |

**Everything above this crate speaks WebKit's vocabulary**, deliberately rather than neutrally.
WebKit is primary and in places richer; a neutral third model would be one more thing to learn and
lossy in whichever direction it did not favour.

`WebKitDialect` is implemented (target multiplexing is not a no-op — the inner frame is a JSON
*string*). `CdpDialect`'s capability table is implemented and frozen; its translation is Phase 4.

## L1 · `mjx-wk-transport` — how frames travel

| Item | Purpose |
|---|---|
| `Transport` | `send(String)` / `recv() -> Option<String>` / `close` / `dialect` |
| `Discovery` | `list() -> Vec<TargetDescriptor>` |
| `TargetKey` | opaque; a socket path here, an app/page id pair there |

**The seam is one JSON string in each direction, and that placement is load-bearing.** Apple's
transports carry binary plists, not JSON — the WebKit frame rides inside one as `WIRSocketDataKey`.
Because the seam is "give me a frame, take a frame", plist wrapping lives entirely inside the Apple
backend and no other crate changes when it lands.

`ReplayTransport` is implemented and is the integration contract for every task. See
`docs/TRANSPORTS.md` for the six backends.

## L2 · `mjx-wk-session` — one attached debuggee

| Item | Purpose |
|---|---|
| `SessionHandle` | `call` / `subscribe` / `supports` / `for_target`; cheap, cloneable, `Send` |
| `DomainAgent` | **the extension point for every feature, present and future** |
| `Capabilities` | optimistic with a negative cache — see below |
| `UnsupportedReason` | `Dialect` \| `DebuggeeBuild` \| `TargetKind` |

Capability gating has **two independent axes**, and they fail for different reasons a user needs
told apart: the dialect (a CDP debuggee has no `Canvas` in any version) and the debuggee (this
build, this target kind). WebKit has no "list your capabilities" command, so the model is assume-
then-learn: remember every member the debuggee rejects with `MethodNotFound`, and nothing else —
an argument error says something about one call, not about a member's existence.

## L3 · `mjx-wk-source` — text, and where things are

`SourceId`, `NodeId`, `RequestId`, `FrameId`, `SourceLocation`, `SourceKind`.

**These ids live here so the L4 crates can stay peers.** Every feature needs to name a place: a
breakpoint, a stack frame, a CSS rule, a network initiator, a profiler sample.

`SourceId` is deliberately *not* the debuggee's `scriptId`, which is a string, is valid for one page
lifetime, and does not exist for documents or stylesheets. A dense local id survives reloads —
which is what keeps the user's open tab and breakpoints across a refresh.

Also: `SourceInventory` (merges `scriptParsed` with `getResourceTree`), `SourceText` + `LineIndex`,
`SourceStore` (`text(session, &SourceEntry)` — fetch path needs kind/script_id/frame/url; cache
keyed by `SourceId`), `Highlighter`, `SourceMapResolver`, `PrettyPrinter`, `SearchIndex`.

## L4 · feature crates — one `DomainAgent` and one model each

| Crate | Domains | Phase |
|---|---|---|
| `mjx-wk-debug` | `Debugger`, `DOMDebugger` | 2 |
| `mjx-wk-console` | `Console` | 2 |
| `mjx-wk-network` | `Network` | 3 |
| `mjx-wk-profile` | `Timeline`, `ScriptProfiler`, `CPUProfiler`, `Heap`, `Memory` | 5 |
| `mjx-wk-dom` | `DOM` | 6 |
| `mjx-wk-css` | `CSS` | 6 |
| `mjx-wk-storage` | `DOMStorage`, `IndexedDB`, `Worker`, `ServiceWorker`, `Page` (cookies) | 7 |
| `mjx-wk-graphics` | `Canvas`, `Recording`, `LayerTree`, `Animation` | 7 |
| `mjx-wk-audit` | `Audit`, `Browser`, `Inspector` | 7 |

## L5 · `mjx-wk-ui` — panels and widgets

| Item | Purpose |
|---|---|
| `Panel` | `id` / `title` / `requires` / `ui` |
| `Action` | **the only channel from UI back to the debuggee**; `#[non_exhaustive]`, grows per phase |
| `PanelCtx` | theme and a `SupportQuery`; everything in it is cheap to read |
| `SupportQuery` | a trait, so this crate keeps its promise of not depending on the session |
| `Theme` | tokens; widgets never write a literal colour. `DESIGN.md` owns the contract |

**This crate holds no session.** A widget is a pure function of a snapshot returning `Action`s. It
cannot await, so it cannot drop a frame no matter what the debuggee is doing.

Every widget module — including panels for phases not yet started — is **declared now**, so the
task that adds one creates its file rather than editing a shared module list.

---

## The prediction mechanism

`DomainAgent` + `Panel` is how a phase nobody has scoped already has a shape. Any future feature —
WebAssembly debugging, accessibility auditing, a new device transport — is:

1. one L4 crate implementing `DomainAgent`,
2. one `Panel` implementation over its model,
3. one line in the registry,
4. one `Transport` or `Dialect` implementation if the wire differs.

Nothing already written changes. That is the whole reason the seam was frozen before any feature
was built, and it is the property to protect when reviewing a change that wants to reach sideways.
