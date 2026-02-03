# textual-rs Roadmap

This roadmap defines a **Rust Textual** project built on top of `rich-rs`.
It is intentionally separate from `rich-rs` (which targets Python Rich parity).

The goal here is a framework capable of powering real applications, eventually enabling a practical port of Textual apps to Rust.

---

## Phase 0: Project scaffolding

| Status | Task | Notes |
|--------|------|-------|
| Todo | Create crate layout | `textual` crate + optional `textual-macros` later |
| Todo | Add CI (fmt, clippy, tests) | Keep toolchain stable |
| Todo | Add snapshot testing harness | Screen/frame snapshots are the core correctness tool |
| Todo | Add minimal example app | “hello widget tree” |
| Done | Async runtime decision | **Tokio** (aligns with Python Textual’s asyncio-first model) |

---

## Phase 0.5: Rich-rs integration contract (recommended)

**Goal:** codify how `textual-rs` uses `rich-rs` so we don’t accidentally bypass the rendering pipeline or lose metadata.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Choose render boundary | Prefer reusing `rich-rs` `ScreenBuffer` + diffing initially (or build a thin adapter); avoid duplicating proven logic |
| Todo | Define handler metadata schema | Use `StyleMeta.meta` with `MetaValue` for structured handler payloads (not ad-hoc strings) |
| Todo | Preserve metadata through rendering | Ensure wrapping/clipping/diffing never drops `StyleMeta.meta` needed for hit-testing and event routing |
| Todo | Hyperlink id policy | For OSC8: rely on `rich-rs` per-Console URL→id registry when `link_id` is omitted |
| Todo | Deterministic ids (open) | If needed for persistence/snapshots, add a **Textual-level** deterministic (hash-based) id scheme; don’t overload OSC8 ids |
| Todo | Integration golden tests | Frame snapshots that assert metadata + hyperlink correctness under diffing |

---

## Phase 1: Terminal driver + frame rendering

**Goal:** render a stable frame to the terminal and update it efficiently.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Terminal driver (crossterm) | raw mode, alt-screen, cursor, input |
| Todo | Screen buffer type | grid of cells (char + style) |
| Todo | Frame diff algorithm | minimal repaint with cursor controls |
| Todo | Deterministic renderer | “same tree ⇒ same frame” |
| Todo | Golden tests (TTY capture) | avoid regressions; verify scrollback correctness |

Deliverable: an app that can render a full-screen view and update it on a timer without flicker/garbling.

---

## Phase 2: Widget tree + lifecycle

**Goal:** establish the core “UI tree” model.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Widget base trait | render + layout + event hooks |
| Todo | Mount/unmount lifecycle | compose, query children, ids |
| Todo | Invalidation model | mark dirty, re-layout, re-render |
| Todo | Composition helpers | containers, simple stacking |

Deliverable: compose a view with multiple widgets and update state to trigger re-render.

---

## Phase 3: Events + input + focus

**Goal:** interactive apps with keyboard/mouse events and focus management.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Event types | key, mouse, paste, resize |
| In progress | Event routing | bubbling/capture (as needed) |
| In progress | Focus system | focusable widgets, tab order |
| In progress | Key bindings | map keys → actions/commands |
| In progress | Resize handling | recompute layout + rerender |

Deliverable: focusable button-like widget + key bindings + mouse click.

---

## Phase 4: Layout engine (MVP)

**Goal:** reliable sizing/positioning for complex UIs.

Textual’s layout is powerful; start with an MVP that can evolve.

| Status | Task | Notes |
|--------|------|-------|
| In progress | Box model | padding, border, margin (subset) |
| In progress | Layout primitives | vertical/horizontal, dock, grid-ish |
| In progress | Clipping + regions | render-only visible area + scroll regions (MVP) |
| Todo | Scroll containers | vertical scrolling first |

Deliverable: sidebar + main view + footer layout with scrolling content.

---

## Phase 5: Styling system (MVP)

**Goal:** expressive styling that scales beyond hard-coded colors.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Typed style props | color, bg, bold, border, etc. |
| Todo | Selector model | by id/class/type (subset) |
| Todo | Cascading + computed styles | resolve inheritance and overrides |
| Todo | Style invalidation | update styles without rebuilding tree |

Deliverable: style a UI via a stylesheet-like source (format TBD) and hot-reload it (optional).

---

## Phase 6: Async + animations + timers

**Goal:** “reactive UI” feel: background tasks, spinners, animations, transitions.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Message bus | widget messages to app |
| Todo | Timers | repeating + one-shot |
| Todo | Animation ticks | frame scheduling + invalidation |
| Todo | Async tasks | implement on Tokio (`spawn`, `select!`, timers) |

Deliverable: progress/spinner + animated UI element without blocking input.

---

## Phase 7: Core widget catalog

**Goal:** enough built-in widgets to build real apps.

| Status | Task | Notes |
|--------|------|-------|
| Todo | Label / Static | text rendering + wrapping |
| Todo | Button | focus + click + states |
| Todo | Input | text entry, cursor, selection |
| In progress | ListView / DataTable | virtualization later |
| In progress | Modal / overlay | stacking and focus trap |

---

## Phase 8: Compatibility layer (optional)

**Goal:** make ports of Python Textual apps less painful.

| Status | Task | Notes |
|--------|------|-------|
| Todo | API mapping notes | document conceptual mapping |
| Todo | “Textual-like” naming | where it helps portability |
| Todo | Adapter utilities | shortcuts for common patterns |

---

## Definition of Done (v0.1)

- A stable full-screen app loop (alt-screen + diff) with no flicker/garble.
- Widget tree with invalidation, focus, input events, and a small widget set.
- Layout + styling MVP sufficient to build a multi-pane interactive app.
- Snapshot/golden tests that prevent regressions.
