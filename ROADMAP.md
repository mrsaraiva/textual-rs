# textual-rs Roadmap

This roadmap defines a **Rust Textual** project built on top of `rich-rs`.
It is intentionally separate from `rich-rs` (which targets Python Rich parity).

The goal here is a framework capable of powering real applications, eventually enabling a practical port of Textual apps to Rust.

> **Note:** Phases 0–5 and 7 were largely completed during an intensive push to get the
> Textual button demo (`examples/buttons.rs`) working end-to-end. Implementing that single
> demo drove progress across the entire stack — driver, layout, styling, events, and widgets
> — because every layer had to actually work together. The roadmap below reflects that reality.

---

## Phase 0: Project scaffolding

| Status | Task | Notes |
|--------|------|-------|
| Done | Create crate layout | `textual` crate with `src/` modules and `examples/` |
| Todo | Add CI (fmt, clippy, tests) | Keep toolchain stable |
| Done | Add snapshot testing harness | SVG demo snapshot harness + shared helper (`tests/snapshots.rs`) |
| Done | Add minimal example app | Multiple examples: `buttons`, `hello`, stylesheet hot-reload, etc. |
| Done | Async runtime decision | **Tokio** (aligns with Python Textual's asyncio-first model) |

---

## Phase 0.5: Rich-rs integration contract

**Goal:** codify how `textual-rs` uses `rich-rs` so we don't accidentally bypass the rendering pipeline or lose metadata.

| Status | Task | Notes |
|--------|------|-------|
| Done | Choose render boundary | `rich-rs` Console renders segments; `FrameBuffer` diffs; `Console::print_segments` writes output |
| Done | Define handler metadata schema | `MetaValue::Int` keyed as `textual:widget_id` for hit-testing and event routing |
| Done | Preserve metadata through rendering | Metadata survives clipping/diffing; verified by `tests/render_metadata.rs` |
| Todo | Hyperlink id policy | For OSC8: rely on `rich-rs` per-Console URL→id registry when `link_id` is omitted |
| Todo | Deterministic ids (open) | Widget IDs are random (`WidgetId::new()`); consider hash-based IDs if needed for persistence/snapshots |
| Todo | Integration golden tests | Metadata-specific golden tests (current snapshots cover rendering but not metadata assertions) |

---

## Phase 1: Terminal driver + frame rendering

**Goal:** render a stable frame to the terminal and update it efficiently.

| Status | Task | Notes |
|--------|------|-------|
| Done | Terminal driver (crossterm) | Raw mode, alt-screen, mouse capture, cursor hiding, Kitty pointer-shape protocol (OSC 22) |
| Done | Screen buffer type | `FrameBuffer` — grid of cells with char + style + metadata |
| Done | Frame diff algorithm | `diff_to_segments` produces minimal repaint with cursor-move controls |
| Done | Deterministic renderer | Same tree produces same frame; verified by snapshot tests |
| Done | Synchronized output | DECSET 2026 bracketing + line-wrap disable to prevent resize tearing |
| Partial | Golden tests | SVG snapshots exist; no raw TTY capture tests yet |

Deliverable: ~~an app that can render a full-screen view and update it on a timer without flicker/garbling.~~ **Done.**

---

## Phase 2: Widget tree + lifecycle

**Goal:** establish the core "UI tree" model.

| Status | Task | Notes |
|--------|------|-------|
| Done | Widget base trait | `Widget` trait: render, layout, event, style, focus, hover, active hooks |
| Done | Mount/unmount lifecycle | `on_mount`/`on_unmount`, `visit_children_mut` for tree traversal |
| Done | Composition helpers | Vertical, Horizontal, Dock, Frame, Constrained, ScrollView, VerticalScroll, Grid |
| Done | Per-widget styles API | `WidgetStyles` for inline overrides; `style_classes()` for CSS class resolution |
| Partial | Invalidation model | **MVP dirty flag**: render on invalidation (input/hover/style reload/active), not every tick; still re-renders whole frame when dirty (no dirty regions / selective relayout yet) |

Deliverable: ~~compose a view with multiple widgets and update state to trigger re-render.~~ **Done.**

---

## Phase 3: Events + input + focus

**Goal:** interactive apps with keyboard/mouse events and focus management.

| Status | Task | Notes |
|--------|------|-------|
| Done | Event types | Key, MouseDown, MouseUp, Tick, Resize, Action |
| Done | Event routing | Capture phase (`on_event_capture`) + bubble phase (`on_event`) |
| Done | Focus system | Tab/Shift-Tab traversal, `focusable()`, focus-on-click, focus chain logging |
| Done | Key bindings | `ActionMap` with default bindings (arrows, hjkl, space/enter, tab, page up/down) |
| Done | Resize handling | `on_resize` propagated to tree; framebuffer reset + sync output to prevent tearing |
| Done | Mouse hover + pointer | Hit-testing via framebuffer metadata; hover state propagation; Kitty pointer shape feedback |

Deliverable: ~~focusable button-like widget + key bindings + mouse click.~~ **Done.**

---

## Phase 4: Layout engine (MVP)

**Goal:** reliable sizing/positioning for complex UIs.

| Status | Task | Notes |
|--------|------|-------|
| Done | Box model | Padding, border (all edges, shorthand, `tall`/`block`/`none`), margin, line-pad |
| Done | Layout primitives | Vertical, Horizontal, Dock, Grid, Row with fixed-width support |
| Done | Clipping + regions | Render-only visible area + scroll regions |
| Done | Scroll containers | Vertical + horizontal scrolling (`ScrollView`, `VerticalScroll`) |
| Done | Layout constraints | min/max width/height, `width: auto`, `height: auto` |

Deliverable: ~~sidebar + main view + footer layout with scrolling content.~~ **Done.**

---

## Phase 5: Styling system (MVP)

**Goal:** expressive styling that scales beyond hard-coded colors.

| Status | Task | Notes |
|--------|------|-------|
| Done | Typed style props | Color, bg, bold, dim, italic, underline, border, margin, tint, background-tint, text-style |
| Done | Inline style API | `Style` struct + per-widget `WidgetStyles` overrides |
| Done | Selector model | By type, class, pseudo-class (`:hover`, `:focus`, `:active`, `:disabled`) |
| Done | Style inheritance | Parent → child propagation for inheritable properties |
| Done | Stylesheet parser | `StyleSheet::parse` with property/value parsing and theme token resolution |
| Done | Selector combinators | Descendant, direct child (`>`), grouping (`,`) |
| Done | Specificity + cascade | Specificity scoring; rules cascade in declaration order |
| Done | Stylesheet hot reload | File watch with configurable interval |
| Done | Theme tokens | `$surface`, `$primary`, lighten/darken/muted derivations aligned with Textual |
| Done | Built-in widget defaults | Default stylesheet for Button (all variants, all pseudo-states) and VerticalScroll |
| Todo | Computed styles | No full "resolve inherited + cascaded → computed" pipeline yet |
| Partial | Style invalidation | Stylesheet watch reload marks the app dirty; still no selective style updates / dirty regions |

Deliverable: ~~style a UI via a stylesheet-like source and hot-reload it.~~ **Done.**

---

## Phase 6: Async + animations + timers

**Goal:** "reactive UI" feel: background tasks, spinners, animations, transitions.

| Status | Task | Notes |
|--------|------|-------|
| Partial | Tick system | 100ms tick loop with `on_tick` propagated through widget tree; used for button active-effect timer |
| Partial | Message bus | `Message` / `MessageEvent` + runtime message queue + bubble delivery via `Widget::on_message`. `Input` / `Button` / `Checkbox` / `DataTable` emit messages; some widgets still use direct callbacks. |
| Todo | One-shot timers | No timer API beyond the tick counter |
| Todo | Animation framework | No easing, transitions, or frame-scheduled animations |
| Todo | Async tasks | `run_widget_tree` is async but no `spawn`/`select!` patterns for background work |

Deliverable: progress/spinner + animated UI element without blocking input.

---

## Phase 7: Core widget catalog

**Goal:** enough built-in widgets to build real apps.

Historically we've marked widgets as "Done" once they existed and could support demos.
Going forward we distinguish:

- **Exists (MVP):** functional, typically ASCII-first, enough to run demos.
- **First-class:** behaves and *feels* like Textual, and is implemented in a way that advances core framework fundamentals (v0.2 goals).

### First-class definition

A widget is considered **first-class** when it meets *all* of the following:

- **Behavior parity:** correct focus/hover/active/disabled semantics, plus keyboard and mouse behavior that matches Textual where applicable (including click-cancel / capture semantics).
- **Styling parity:** uses the CSS engine (type/class/pseudo selectors), has reasonable built-in default CSS, and supports theme tokens and pseudo-states (`:hover`, `:focus`, `:active`, `:disabled`).
- **Layout parity:** respects the box model (margin/border/line-pad), provides sensible intrinsic sizing (`content_width()` / `layout_height()` where applicable), and uses `on_layout()` for state that depends on content-box size.
- **No demo-only hacks:** no "do it in render()" state mutation tricks; behavior should be driven by events/layout and reusable outside the demo.
- **Tested:** has behavior tests (not just snapshots) for its core interaction rules and invariants.

### Widget status (MVP → first-class)

| Widget | Exists (MVP) | First-class | Notes |
|--------|--------------|-------------|-------|
| Label / Static | Done | Partial | Rendering/wrapping exists; styling/default CSS parity still light |
| Button | Done | Done | Press/cancel semantics, pseudo-states, default CSS, variants |
| DataTable | Done | Done | Hit-testing, hover/selection semantics, cached widths, offset/state correctness |
| Input | Done | Todo | Currently ASCII-first; needs mouse/cursor semantics + messages + styling parity |
| Checkbox | Done | Todo | Needs mouse parity, pseudo-states, and default CSS parity |
| ListView | Done | Todo | Needs mouse selection/hover + styling + scroll behavior parity |
| Tabs | Done | Todo | Needs styling parity + focus/child lifecycle polish + messages |
| Tree | Done | Todo | Needs mouse parity + styling + better scroll-into-view behavior |
| Markdown | Done | Partial | Renders via rich-rs; widget semantics/styling parity TBD |
| Modal / overlay | Done | Partial | Exists; needs focus-trap semantics + message-based dismissal |
| Spacer | Done | Partial | Exists; styling semantics minimal by design |

### Acceptance criteria (per widget)

These criteria intentionally overlap with v0.2 goals (message bus, invalidation, timers/animations, broader tests).

#### Input (first-class)

- Emits messages instead of requiring direct callbacks:
  - `InputChanged` (value changed), `InputSubmitted` (enter), and optionally `CursorMoved`.
- Mouse behavior:
  - Click positions cursor; drag selects (or at least lays groundwork for selection).
  - Clicking outside cancels selection / deactivates appropriately.
- Keyboard behavior:
  - Standard editing keys; consistent handling of Home/End, word navigation (optional), delete/backspace.
- Styling:
  - `:focus`, `:disabled`, placeholder style (dim/fg token), and default CSS for borders/padding.
- Tests:
  - Cursor movement rules, edit operations, placeholder visibility, and message emission.

#### Checkbox (first-class)

- Toggle via mouse click and keyboard activation when focused.
- Styling:
  - `:focus`, `:hover`, `:active`, `:disabled`; default CSS that matches Textual feel.
- Emits `CheckboxChanged { checked }` (message bus).
- Tests:
  - Toggle semantics (mouse + keyboard) and disabled behavior.

#### ListView (first-class)

- Mouse selection:
  - Click selects; hover highlights; wheel scrolls; click-drag does not spuriously activate.
- Keyboard navigation:
  - Up/down/page navigation; selection is kept visible; focus styling.
- Styling:
  - Distinct selected/hover styles driven by pseudo-classes or classes; default CSS.
- Emits `SelectionChanged` (message bus).
- Tests:
  - Ensure-visible logic, mouse hit-testing selection, and stable behavior with empty lists.

#### Tabs (first-class)

- Keyboard + mouse interaction:
  - Arrow keys/hjkl to change; clicking a tab header activates it.
- Focus semantics:
  - Focus is correctly delegated to active child; switching tabs updates focus predictably.
- Styling:
  - Default CSS for tab bar + active tab; hover/active feedback.
- Emits `TabActivated { index, title }` (message bus).
- Tests:
  - Focus delegation and activation semantics.

#### Tree (first-class)

- Mouse interaction:
  - Click selects; click expand/collapse affordance toggles; hover highlights.
- Keyboard interaction:
  - Left/right to collapse/expand; ensure-visible keeps selection within view.
- Styling:
  - Default CSS for selected/hover/focus; indentation + affordance styling via segments.
- Emits `NodeSelected` / `NodeToggled` (message bus).
- Tests:
  - Visible-index mapping correctness and toggle semantics.

---

## Phase 8: Compatibility layer (optional)

**Goal:** make ports of Python Textual apps less painful.

| Status | Task | Notes |
|--------|------|-------|
| Partial | Textual-like naming | CSS class conventions (`-style-default`, `-primary`, etc.) and property names mirror Textual where practical |
| Todo | API mapping notes | Document conceptual mapping between Python Textual and textual-rs |
| Todo | Adapter utilities | Shortcuts for common Textual app patterns |

---

## Phase 9: Debug + developer experience

**Goal:** make it easy to understand what's happening inside the framework.

| Status | Task | Notes |
|--------|------|-------|
| Done | File-based debug tracing | `TEXTUAL_DEBUG_INPUT_FILE`, `TEXTUAL_DEBUG_LAYOUT_FILE`, `TEXTUAL_DEBUG_STYLE_FILE`, `TEXTUAL_DEBUG_RENDER_FILE` |
| Done | Layout debug overlay | `DebugLayout` mode renders widget bounds and sizes |
| Done | Widget/CSS module organization | Widgets live in `src/widgets/` (per-widget modules + core), CSS engine lives in `src/css/` |
| Todo | DevTools panel | In-app inspector (like Textual's DevTools) |

---

## Phase 9.5: Input diagnostics + key model parity (`textual keys` harness)

**Goal:** add a first-class key/input diagnostics app (similar in purpose to Python Textual's `textual keys`) while closing core input-model gaps in `textual-rs`.

This work is intentionally treated as **fundamentals**, not a one-off demo:
- It de-risks terminal/tmux/OS input differences early.
- It creates a stable reference harness for future widget/debugging work.
- It forces key semantics to be represented explicitly in the framework API.

### Scope and sequence

| Status | Step | Notes |
|--------|------|-------|
| Done | Define canonical key model in `textual-rs` | `KeyEventData` wraps crossterm `KeyEvent` via `Deref`; adds `key`, `character`, `is_printable` fields |
| Done | Add key normalization + alias helpers | `src/keys/mod.rs`: `normalize_key_code`, `key_to_identifier`, `format_key_display`, lazy `aliases()`, symbol table, name replacements |
| Done | Runtime conversion path | `Event::Key` now holds `KeyEventData`; runtime converts at crossterm boundary; `KeyBind::from_event` updated; all widgets unchanged via `Deref` |
| Done | Shared driver protocol uplift | Tri-state `KeyboardProtocol` (Off/Auto/On) in `richtui-crossterm`; Kitty mode 1 (DISAMBIGUATE_ESCAPE_CODES); terminal heuristic detection; `TEXTUAL_KEYBOARD_PROTOCOL` env var |
| Done | Build key diagnostics harness | Canonical `keys` preview now runs via `textual-dev-rs` (`cargo run -- keys`); `textual-rs/examples/keys.rs` remains available for direct library demo runs |
| Done | Add key diagnostics tests | 48 unit tests + 2 doc-tests + 74 integration tests (`tests/key_diagnostics.rs`): normalization, aliases, display, identifiers, Deref, edge cases, ActionMap |
| Done | Document compatibility limits | Module-level docs in `src/keys/mod.rs`: normalization rules, alias table, Kitty protocol modes, terminal compatibility matrix (tmux, screen, macOS Terminal, PuTTY, SSH) |
| Done | Visual parity pass for preview UI | Preview now matches Python `textual_dev` keys layout/behavior target (single-pane layout, header/body/action bar, styled log, scroll behavior) |
| Done | Binding panel fundamentals (for apps that use it) | `KeyPanel`/`BindingsTable` now include styled table rendering, corrected sizing math, and scrollbar interactions (wheel/action/track/drag) with dedicated tests |

### Acceptance criteria

- A `keys` harness exists and is usable as a manual QA app for input support across environments.
- Key presses expose both:
  - **Raw view:** native `crossterm` code/modifiers/kind.
  - **Canonical view:** normalized key identity + derived properties.
- Alias behavior is deterministic (e.g. `tab` and `ctrl+i` relationship is represented).
- Shift/ctrl/alt modifier handling is visible and test-covered.
- Mouse diagnostics include button/position/modifiers/scroll deltas and reflect routing decisions.
- Shared driver can be configured to enable/disable enhanced keyboard protocol; app behavior remains stable when unavailable.
- No demo-only hacks: the harness consumes framework primitives that other widgets/apps can reuse.
- Visual target is explicit: parity is against Python's keys preview UI currently used for QA screenshots (not against Textual's standalone `KeyPanel` widget).

### Visual parity and follow-up plan (updated)

1. **Phase 1: keys demo 1:1 pass (completed)**
   - Right-side binding panel removed from the preview target.
   - Python preview structure/copy matched (`Textual Keys`, instruction panel, bottom `Clear`/`Quit` bar).
   - Demo styling tuned for parity (header, log styling, action bar, scrollbar, interactions).
2. **Phase 2: reusable preview scaffold fundamentals (pending)**
   - Introduce a reusable preview shell composition for title/content/action-bar.
   - Migrate `keys` and at least one additional preview demo to this shell.
3. **Phase 3: styling fidelity fundamentals (pending)**
   - Add missing style engine capabilities needed for parity (component-scoped styles, selector expressiveness, border/divider nuance).
4. **Phase 4: visual regression discipline (pending)**
   - Add snapshot/image-based parity checks for `keys` and extend to other high-value demos.

### Implementation notes (for cross-session continuity)

- Keep this incremental and bisectable:
  1. Canonical model + helpers.
  2. Runtime wiring.
  3. Harness UI.
  4. Driver protocol uplift.
  5. Tests + docs.
- Prefer additive APIs first; delay breaking cleanup until harness proves behavior.
- Use the harness as the source of truth during manual QA (inside/outside tmux, multiple terminals).
- Keep debug channels aligned with harness output so logs and UI corroborate each other.

---

## Definition of Done (v0.1) — Achieved

- [x] A stable full-screen app loop (alt-screen + diff) with no flicker/garble.
- [x] Widget tree with focus, input events, and a small widget set.
- [x] Layout + styling MVP sufficient to build a multi-pane interactive app.
- [x] Snapshot tests that prevent regressions.

## Next priorities (v0.2)

- Widget uplift: MVP → first-class (Input, ListView, Tabs, Tree, Checkbox)
  - Treat demos as integration tests that drive fundamentals (message bus, invalidation, timers/animations, and higher-quality behavioral tests).
- Input diagnostics + key model parity (`textual keys` harness)
  - Canonical key semantics, driver protocol, and preview parity pass are done.
  - Next: complete scaffold/style/regression fundamentals listed in Phase 9.5.
- Dirty invalidation — avoid full re-render every tick. (**MVP done**; next: selective relayout / dirty regions)
- Message bus — decouple widget events from direct callbacks.
- One-shot timers + animation framework.
- CI pipeline (fmt, clippy, tests).
- Expand test coverage beyond snapshot smoke tests.
