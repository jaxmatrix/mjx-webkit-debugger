# Design System

Conventions for the debugger's interface. Read this before adding a widget, panel, or colour. The
rule of thumb: **one token per concern, density over decoration, never block a frame.** If you
reach for a raw colour, a one-off row height, or a bespoke tree — stop, there is already a
primitive for it.

This file owns the visual and interaction contract. Read [`CLAUDE.md`](CLAUDE.md) for architecture,
threading, and protocol rules, and [`docs/SEAMS.md`](docs/SEAMS.md) for the interfaces.

This doc holds two kinds of content, maintained differently:

- **Principles** are durable. They hold as widgets come and go.
- **Named contracts** — tokens, states, the keymap — are the design system's current API,
  maintained *with* the code. If you change a token in `crates/mjx-wk-ui/src/theme.rs`, update its
  entry here **in the same commit**. A stale name in this file is a bug, exactly like a stale type.

## Principles

1. **Familiar before clever.** This is a debugger. People arrive with a decade of Chrome DevTools
   muscle memory and a bug to find. Keys, layout, and the meaning of a gutter marker match Chrome
   unless there is a reason they cannot. Novelty here is a tax on someone having a bad day.
2. **Density is a feature.** A debugger shows dense structured data — stacks, scopes, requests,
   rules. Rows are compact, padding is small, and nothing is nested in a rounded box for
   decoration. Group with alignment and a single hairline.
3. **Never block a frame.** No interaction may await anything. A widget returns an `Action` and
   renders the state it has. If data has not arrived, show that it has not — never freeze.
4. **State is legible at a glance.** A user must be able to tell a resolved breakpoint from a
   pending one, a cached response from a network one, an active declaration from an overridden
   one, without reading text or hovering. That is what the state colours below are for; they are
   load-bearing, not decorative.
5. **Unavailable is not invisible.** A panel whose protocol members the debuggee lacks renders
   **disabled with a reason**. Hiding it leaves a user hunting for a tab that is not there.
6. **Monospace everywhere it matters.** Code, values, headers, URLs, stack frames. Fixed row
   heights are also what make virtualisation possible — see below.

## Layout

DevTools-parity, in a dock (`egui_dock`) so panels can be rearranged and torn off.

```
┌──────────────────────────────────────────────────────────────┐
│ toolbar: target ▾   ⏸ ▶ ⤼ ⤵ ⤴   search            theme ▾   │
├───────────────┬──────────────────────────┬───────────────────┤
│ Sources       │  CodeView                │ Debugger          │
│  (tree)       │   gutter │ text          │  Call stack       │
│               │                          │  Scopes           │
│               │                          │  Watch            │
│               │                          │  Breakpoints      │
├───────────────┴──────────────────────────┴───────────────────┤
│ Console                                                      │
└──────────────────────────────────────────────────────────────┘
```

Later phases add tabs (Network, Performance, Elements, Application), never new chrome.

## Tokens

Defined in `crates/mjx-wk-ui/src/theme.rs` as `Theme`. **Widgets never write a literal colour.**

### Surfaces

| Token | Role |
|---|---|
| `background` | the window |
| `panel` | a docked panel's body |
| `gutter` | the code view's left margin |
| `hairline` | the single-pixel rule that separates regions — the only border we draw |

### Text

| Token | Role |
|---|---|
| `text` | primary |
| `text_dim` | secondary: line numbers, inactive rows, inherited rules |
| `accent` | selection and focus |

### Syntax

`syntax_keyword`, `syntax_string`, `syntax_number`, `syntax_comment`, `syntax_function`,
`syntax_type`, `syntax_property`, `syntax_tag`. Mapped from `mjx_wk_source::HighlightKind` by
`Theme::syntax`; a widget never matches on `HighlightKind` itself.

### Debugger states

These carry meaning and must stay visually distinct at a glance:

| Token | Meaning |
|---|---|
| `breakpoint_resolved` | bound to real code; it will hit |
| `breakpoint_pending` | set, but no matching script has parsed — may never hit |
| `breakpoint_conditional` | has a condition |
| `breakpoint_logpoint` | logs or probes and continues; never stops |
| `execution_line` | where execution is stopped — **not a breakpoint, must not look like one** |

### Metrics

`row_height`, `gutter_width`, `indent_width`, `monospace_size`. Row height is **fixed**: the code
view sizes its scroll area as `line_count × row_height` rather than measuring text, which is what
lets a 200 000-line file scroll without laying it out.

## The code view

The most demanding widget in the application; `crates/mjx-wk-ui/src/code_view.rs`.

- **Virtualised.** Only visible rows plus a small margin are laid out or highlighted.
- **Long lines clip, they do not wrap.** A minified bundle is one line several megabytes long;
  wrapping it produces a row millions of pixels tall.
- **Gutter click toggles a breakpoint.** Right-click opens condition / logpoint / probe. Drag a
  breakpoint to move it. This is Chrome's behaviour and there is no reason to differ.
- **Breakpoint marks** use the five states above. A pending breakpoint is drawn hollow.
- **The execution line** is a full-width background wash plus an arrow in the gutter — distinct in
  both shape and colour from any breakpoint.
- **Inline probe values** render right-aligned at the end of their line in `text_dim`. This is
  WebKit's live-value feature and Chrome has no equivalent; it is one of the few places we
  deliberately show something DevTools users will not recognise, so it is styled as annotation
  rather than as content.

## Trees and tables

One tree primitive and one table primitive, used by the source tree, the variable tree, the DOM
tree, the network table, and the storage tables.

- Rows are `row_height` tall and virtualised past a few hundred entries.
- Indentation is `indent_width` per level, with no connecting lines.
- A disclosure triangle appears only where there is something to disclose. **A node whose children
  have not been fetched yet still shows one** — the user should not have to know that expanding
  triggers a request.
- Lazy rows that need paging end with a "Show more" row rather than loading everything: a scope can
  hold fifty thousand properties, and `Runtime.getProperties` is paginated for that reason.

## Keymap

Chrome-compatible. Deviating from these costs a user more than any feature gains them.

| Key | Action |
|---|---|
| `F8` / `Ctrl-\` | resume / pause |
| `F10` | step over |
| `F11` | step into |
| `Shift-F11` | step out |
| `Ctrl-P` | open source by name |
| `Ctrl-Shift-F` | search all sources |
| `Ctrl-F` | search in this source |
| `Ctrl-G` | go to line |
| `Ctrl-B` | toggle breakpoint on the current line |
| `Esc` | toggle the console drawer |

WebKit-only actions get keys Chrome does not use, so nothing familiar is displaced:
`F9` steps to the next run loop, `Shift-F9` toggles pause-on-microtasks.

## Light and dark

Both are first-class and are one code path: every widget reads tokens, so a theme swap needs no
widget changes. Dark is the default, as in every other developer tool. `Theme::from_visuals`
follows the host's preference on first run.
