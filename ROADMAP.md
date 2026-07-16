# textual-rs Roadmap

This roadmap defines a **Rust Textual** project built on top of [`rich-rs`](https://crates.io/crates/rich-rs)
and [`crossterm`](https://crates.io/crates/crossterm).

The goal here is a framework capable of powering real applications, eventually enabling a practical port of Textual apps to Rust.

> **Note:** Phases 0–5 and 7 were largely completed during an intensive push to get the
> Textual button demo (`examples/buttons.rs`) working end-to-end. Implementing that single
> demo drove progress across the entire stack — driver, layout, styling, events, and widgets
> — because every layer had to actually work together. The roadmap below reflects that reality.

---

## Post-1.0 status and the 1.1.0 milestone

**1.0.0 shipped** (crates.io), followed by three correctness patch releases that
closed every confirmed bug found by porting two real apps (toolong-rs,
textual-dev-rs) and by auditing against Python Textual's own test suite:

- **1.0.1** — 5 port bugs: screen CSS matching, stray `>` type name, class-op
  relayout, unhandled-binding warning, public frame accessors.
- **1.0.2** (+ `textual-macros` 1.0.1) — 10 audit bugs: 4 binding/dispatch, 4
  data/list widgets, 2 input/text.
- **1.0.3** — 3 correctness fixes distilled from design analysis of the harder
  gaps: build-time `on_mount` side-effect drop, cross-tree command aliasing,
  user-`layers` clobbering the system toast layer.

18 confirmed bugs closed, each with a ported regression test. The release
pipeline was hardened to publish `textual-macros` before `textual` automatically.

### 1.1.0 = full Python parity depth + the extension story

Decided scope (maintainer call, 2026-07-16): **full parity**, including the large
structural items. Every fix is parity-gated (visual + pty_parity idle) and carries
a ported regression test. Items marked **[spec]** get a Fable design/spec pass and
maintainer review before implementation; the rest go direct to implementation.

**W1 — Extension story (make third-party widgets/apps first-class)**
- [x] Cross-screen widget access: App-level sync (`query_on`/`with_widget_mut_on`)
  + deferred `ctx.query_one_on` (Gap 7 B1/B2). Design done
  (`docs/devel/DESIGN_mount_and_cross_screen.md`). [M/L]
- [x] Component classes for custom widgets: `COMPONENT_CLASSES` +
  `get_component_rich_style`, so third-party widgets expose restylable internals
  instead of hardcoding RGB. **[spec]** [M]
- [x] Render-time resolved-style / theme-var access + a public timer API seam
  (retire the internal `ctx.event_ctx_mut()` backdoor). [S]

**W2 — Parity foundations**
- [x] Action-parser fidelity: last-dot namespace split, typed-arg eval, malformed
  input errors (unblocks typed `check_action` and the keymap subsystem). [M]
- [x] Validation description-priority ladder: `describe_failure` + `merge` +
  `failure_descriptions`. [M]
- [x] Reactive `always_update` (derive attribute) + Suggester `use_cache`. [S]
- [x] Devtools framework hooks: log sink over the devtools socket + debug-channel
  introspection + `on_tick` for inactive screens (harness work lives in
  `textual-dev-rs`). [M]

**W3 — Structural parity (the large items, all in for 1.1.0)**
- [x] Tree / OptionList stable key identity: `NodeID` / `RowKey` / `ColumnKey`
  registries, `get_*_by_id`, key-based CRUD, typed not-found errors. **[spec]** [L]
- [x] TextArea undo/redo `EditHistory` batching: time-batch, cursor-move
  checkpoint, char-count split, paste isolation (replaces the full-snapshot
  stack; diverges from all 16 Python history tests today). **[spec]** [L]
- [x] Keymap subsystem: `BindingsMap`, `BindingDecl.id`, `apply_keymap`, clash
  detection, `bindings_updated_signal` (user key remapping). **[spec]** [L]
- [x] Wrapped-line navigation under `soft_wrap`: `get_location_above/below`,
  `WrappedDocument` / `DocumentNavigator` parity. **[spec]** [M/L]

**W4 — Behavior flips (maintainer-approved)**
- [x] `Select.allow_blank` default `false` -> `true` (Python parity; breaking
  default, audit examples/apps). [S]
- [ ] Fine-grained widget messages: DataTable (Row/Cell/Column/Header/RowLabel
  Highlighted+Selected), Collapsible (`Expanded`/`Collapsed`), SelectionList
  (`SelectionHighlighted`), so Python-style granular `#[on(...)]` handlers
  dispatch. [M]

**W5 — Document intentional divergences (KNOWN_GAPS, no code)**
- [x] name-convention dispatch (`on_<ns>_<name>`) + `key_<name>` method dispatch;
  message-class MRO inheritance; `BINDINGS` class-hierarchy inheritance /
  `inherit_bindings`; private `_watch`/`_validate`/`_compute` + reactive
  inheritance; the reactive deferred-phase watcher model; theme tokens vs Python
  0.58. All deliberate consequences of Rust's flat-TypeId / compile-time-derive /
  non-subclassing model.

**Spec-first queue (Fable before implementation):** component classes, Tree/
OptionList key identity, TextArea undo batching, keymap subsystem, wrapped-line
navigation. Cross-screen access is already designed. Everything in W2, W4, and the
W1 style/timer seam goes direct to implementation.

---

## Phase 0: Project scaffolding

| Status | Task | Notes |
|--------|------|-------|
| Done | Create crate layout | `textual` crate with `src/` modules and `examples/` |
| Done | Add CI (fmt, clippy, tests) | GitHub Actions workflow runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --all-targets` on push/PR |
| Done | Add snapshot testing harness | SVG demo snapshot harness + shared helper (`tests/snapshots.rs`) |
| Done | Add minimal example app | Multiple examples: `buttons`, `hello`, stylesheet hot-reload, etc. |
| Done | Async runtime decision | **Tokio** core runtime (with sync/blocking convenience runners for simple apps/examples) |

---

## Phase 0.5: Rich-rs integration contract

**Goal:** codify how `textual-rs` uses `rich-rs` so we don't accidentally bypass the rendering pipeline or lose metadata.

| Status | Task | Notes |
|--------|------|-------|
| Done | Choose render boundary | `rich-rs` Console renders segments; `FrameBuffer` diffs; `Console::print_segments` writes output |
| Done | Define handler metadata schema | `MetaValue::Int` keyed as `textual:widget_id` for hit-testing and event routing |
| Done | Preserve metadata through rendering | Metadata survives clipping/diffing; verified by `tests/render_metadata.rs` |
| Done | Hyperlink id policy | Link widget now emits OSC8 metadata via `StyleMeta.link` and relies on `rich-rs` per-Console URL→id registry when `link_id` is omitted (PR8G, 2026-02-11) |
| Done | Deterministic widget-id policy | `WidgetId::new()` remains process-local and monotonic (global atomic allocator); stable/persistent IDs are explicitly deferred for now to avoid locking in a premature persistence contract (PR8H, 2026-02-11) |
| Done | Integration golden tests | Metadata-specific coverage now includes direct assertions and snapshots for metadata preservation across framebuffer + diff (`tests/render_metadata.rs`) |

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
| Done | Golden tests | SVG snapshots plus deterministic raw TTY capture coverage for framebuffer->diff->output invariants (`tests/render_metadata.rs`, `tests/terminal_output_golden.rs`, `tests/support/terminal_capture.rs`) |

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
| Done | Invalidation model | Dirty-region tracking is now wired through runtime + framebuffer region-scoped diffing; runtime tracks layout/style/content invalidation flags and limits repaint scope to affected widget regions where feasible. |

Deliverable: ~~compose a view with multiple widgets and update state to trigger re-render.~~ **Done.**

---

## Phase 3: Events + input + focus

**Goal:** interactive apps with keyboard/mouse events and focus management.

| Status | Task | Notes |
|--------|------|-------|
| Done | Event types | Key, MouseDown, MouseUp, Enter, Leave, Click, Paste, Mount, Unmount, Ready, Focus, Blur, Tick, Resize, Action |
| Done | Event routing | Capture phase (`on_event_capture`) + bubble phase (`on_event`) |
| Done | Focus system | Tab/Shift-Tab traversal, `focusable()`, focus-on-click, focus chain logging |
| Done | Key bindings | `ActionMap` with default bindings (arrows, hjkl, space/enter, tab, page up/down) |
| Done | Resize handling | `on_resize` propagated to tree; framebuffer reset + sync output to prevent tearing |
| Done | Mouse hover + pointer | Hit-testing via framebuffer metadata; hover state propagation; Kitty pointer shape feedback |
| Done | String action system | `ActionDecl` + `ActionHandler` trait + `parse_action()` parser + `APP_ACTIONS` built-in declarations + `resolve_action()` namespace resolution (bubble from focused to root). |
| Done | CSS display/visibility/overflow | `display: none` hides from render+layout, `visibility: hidden` hides from render but preserves layout, `overflow` controls ScrollView scrollbar behavior. |
| Done | Message envelope + dispatch | `MessageEnvelope` with `stop()`, `prevent_default()`, `can_replace()` propagation control. Envelope-based bubble dispatch (sender→root) with stop semantics. Message queue coalescing deduplicates rapid-fire replaceable messages. |
| Done | Signal system | `Signal<T>` lightweight typed pub/sub with subscribe/emit/unsubscribe. Function pointer handlers for Send+Sync safety. |
| Done | CSS layer property | `layer: <name>` + `layers: <name1> <name2>` for z-ordering in render pipeline. Layers inherited, unknown names default bucket. |
| Done | :focus-within pseudo-class | Matches when element or any descendant has focus. Thread-local focus-within set populated before style resolution. |
| Done | Per-property !important | `ImportanceBitset(u64)` tracks importance per CSS property. Importance-aware cascade: `!important` wins over normal regardless of specificity. Parser detects `!important` per-declaration. |
| Done | Message control reference | `control: Option<NodeId>` on `MessageEnvelope` — originating widget reference (Python's `event.control`). |
| Done | Widget CSS defaults batch | Header dock:top, Footer dock:bottom, Button/Placeholder alignment, Input width:100%, Collapsible display:none, Rule 1fr margins, TextArea 1fr+padding. |
| Done | CSS property animation | `StyleValue` enum + per-property interpolation + `StyleAnimation` on Animator + `animate_style()` on EventCtx. Animatable: colors, opacity, scalars, spacing, tint. |
| Done | Declarative BINDINGS | `BindingDecl` struct + `Widget::bindings()` + `match_binding_tree()` focused chain walk (priority then normal) + event loop wiring + action routing via `execute_action()`. |
| Done | Worker system | `WorkerId`, `WorkerState`, `CancellationToken`, `WorkerRegistry` with register/cancel/exclusive/cleanup. `EventCtx::request_worker()` / `request_exclusive_worker()`. |
| Done | CSS parser: align/offset | `text-align`, `content-align`, `content-align-horizontal/vertical`, `align`, `align-horizontal/vertical`, `offset`, `offset-x/y` all parsed and applied. |
| Done | Widget polish batch 2 | Digits CSS text-align integration, ProgressBar gradient, Select allow_blank mode. |
| Done | Screen system | `Screen` trait + `ScreenStack` + push/pop with lifecycle hooks (mount/suspend/resume/unmount). `ScreenResult` for return values. Per-screen WidgetTree + CSS. |
| Done | Worker runtime wiring | `WorkerRegistry` in event loop, `WorkerStateChanged` message delivery, exclusive mode, cleanup. |
| Done | BINDINGS migration | 11 widgets (Button, Input, Checkbox, ListView, Tabs, Tree, DataTable, Select, TextArea, CommandPalette, ScrollView) implement `bindings()` + `execute_action()`. |
| Done | Constrain CSS property | `constrain: none/inside/inflect` for overlay/tooltip positioning. Tooltip viewport clamping. Default tooltip CSS. |

Deliverable: ~~focusable button-like widget + key bindings + mouse click.~~ **Done.**

---

## Phase 4: Layout engine (MVP)

**Goal:** reliable sizing/positioning for complex UIs.

| Status | Task | Notes |
|--------|------|-------|
| Done | Box model | Padding, border (all edges, shorthand, `tall`/`block`/`none`), margin, line-pad (render-only), border-box default, vertical margin collapsing |
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
| Done | Built-in widget defaults | All 16 default CSS files match Python Textual DEFAULT_CSS verbatim (DC-00..DC-38/DC-ALL) |
| Done | Computed styles | Per-widget computed-style cache/tree now memoizes selector/cascade + inline + inheritance across renders; cache keys include ancestor selector stack + parent style and invalidate correctly on style/class/id/pseudo/stylesheet changes, with layout-affecting computed deltas triggering layout application. |
| Done | Style invalidation | Stylesheet hot-reload now computes changed rules and invalidates matching widgets/selectors (including descendant/child selector matches) with selective region redraw where feasible; falls back to full redraw for layout-affecting or broad changes. |

Deliverable: ~~style a UI via a stylesheet-like source and hot-reload it.~~ **Done.**

### Known CSS Engine Gaps

No open CSS engine gaps. All previously tracked items are resolved.

### TCSS Property Parity (Complete)

**P2-24..P2-36 DONE.** 52 CSS properties added (types, parser, cascade,
layout/render/widget wiring, 76 gated tests). Property count: **~108 of ~108.**

**CSS Defaults Parity (Complete).** DC-00 through DC-38/DC-ALL executed — all 16
default CSS files rewritten to match Python Textual DEFAULT_CSS verbatim with `&`
nesting syntax, multi-line formatting, and full token/property coverage. DCE-Core
engine prerequisites (pseudo-classes, color values, text-style parity, markdown
tokens, tint rendering, auto foreground) landed as prerequisite commits.

**Box-model parity fixes (Complete).** `line-pad` separated from CSS `padding` as a
render-time-only property (matching Python semantics). Default `box-sizing` changed
from `content-box` to `border-box` across all layout paths (matching Python Textual).
Vertical margin collapsing implemented (`max(bottom, top)` instead of additive).
`:can-focus` pseudo-class + global disabled dimming rule added.

Full property-by-property gap analysis with 8 priority tiers lives in:
- **Gap details:** `docs/devel/PARITY_ANALYSIS.md` → Appendix D
- **Action items:** `docs/devel/PARITY_ACTION_PLAN.md` → Pillar 2 (P2-24 through P2-36)

#### P2 Deferred Items (parsed + cascaded, rendering not yet active)

All originally-deferred items have been resolved.

#### P2 Previously Deferred, Now Resolved

| Item | Resolution |
|------|-----------|
| Border title/subtitle rendering (P2-29) | Widget `border_title()`/`border_subtitle()` hooks + border edge composition wired |
| `overlay: screen` blend (P2-34) | Two-pass compositor with pre-paint underlay snapshot + per-cell screen blending |
| Keyline rendering (P2-34) | Vertical/horizontal layout keyline separators with type + color |
| Scrollbar hover/active sub-part styling (P2-30) | Sub-part hit-testing (thumb vs track) with per-state CSS color consumption |
| Absolute positioning min/max constraints (P2-24) | `layout_absolute()` now applies min/max width/height constraints |
| `pointer` CSS → runtime wiring (P2-23) | `pointer_shape_for_hover_tree` rewritten to read computed `style.pointer`; `pointer: text` added to Input/MaskedInput defaults; disabled widgets always `NotAllowed` |
| Full transition dispatch (P2-36) | `transition_requests_for_style_change` now emits `StyleAnimationRequest` for all 12 style-value animatable properties (fg, bg, width/height, min/max dimensions, margin, padding, tint, background_tint); `dispatch_animation_frame` calls `step_style()` and applies results directly to widget inline styles each tick. |

---

## Phase 6: Async + animations + timers

**Goal:** "reactive UI" feel: background tasks, spinners, animations, transitions.

| Status | Task | Notes |
|--------|------|-------|
| Done | Tick system | Adaptive tick cadence (idle 100ms / active ~16ms) with `on_tick` propagation and event-loop repaint scheduling |
| Done | Message bus | `Message` / `MessageEvent` + runtime message queue + bubble delivery via `Widget::on_message` are now the widget interaction integration surface. PR8F (2026-02-11) closed remaining `Select` direct-coupling paths by routing open-dropdown selection through `OptionList` message flow (`OptionSelected` -> `SelectChanged`) and added ordering regressions across `OptionList`/`Select`/`SelectionList`. |
| Done | Grapheme-aware text editing model | Shared text-edit command core drives `Input` / `MaskedInput` / `TextArea`, with grapheme-sensitive follow-up closure for `MaskedInput` cursor/render paths plus `DataTable`/`Tree` width-hit-testing and wrapping edge regressions (`ZWJ`, combining marks, wide-cell labels) landed in PR8E (2026-02-11) |
| Done | One-shot timers | Runtime one-shot timers are available via `TimerSchedule` / `TimerCancel` with `TimerFired` / `TimerCancelled` delivery and event-loop timeout integration |
| Done | `set_interval` / `set_timer` callback timers | `App::set_interval` / `set_timer` schedule arbitrary-delay, named, repeating Rust callbacks (Python `MessagePump.set_interval`/`set_timer` parity), returning a `TimerHandle` for stop/pause/resume/reset. Faithful `timer.py` semantics: skip/fast-forward of missed deadlines, bounded repeat, pause re-anchoring, deterministic manual-clock test path. `App::with_app_struct` bridges callbacks back into the app reactive bridge. Clears clock / recompose01 / recompose02 / world_clock01 / structure (rewired off `on_tick` faking). |
| Done | Animation framework | Animator/easing pipeline, runtime animation queue, CSS transition parsing, and widget integrations (tabs/tabbed/scroll/palette) are in place. 30 easing functions (Quad/Cubic/Quart/Quint/Expo/Circ/Back/Bounce/Elastic families). |
| Done | Async tasks | Runtime async task API now covers spawn/cancel/cancel-by-target/completion, replacement cancellation signaling, and broader utility requests beyond directory reads |

Deliverable: progress/spinner + animated UI element without blocking input.

---

## Phase 7: Core widget catalog

**Goal:** enough built-in widgets to build real apps.

Historically we've marked widgets as "Done" once they existed and could support demos.
Going forward we distinguish:

- **Exists (MVP):** functional, typically ASCII-first, enough to run demos.
- **First-class:** behaves and *feels* like Textual, and is implemented in a way that advances core framework fundamentals (v0.2 goals).

Detailed per-widget execution planning is maintained in:
- `docs/devel/WIDGET_PORTING_PLAN.md` (**source of truth for widget-level tracking**)

`ROADMAP.md` intentionally keeps only milestone-level widget status and acceptance targets.

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
| Label / Static | Done | Done | Wrap-aware intrinsic sizing + default CSS parity baseline |
| Button | Done | Done | Press/cancel semantics, pseudo-states, default CSS, variants |
| DataTable | Done | Done | Hit-testing, hover/selection semantics, cached widths, offset/state correctness; PR5A adds keyed row/column APIs, fixed-row/column baseline behavior, and richer keyboard navigation |
| Input | Done | Done | Cursor/mouse groundwork, selection baseline, message emission (`InputChanged`/`InputSubmitted`), validation classes, and placeholder/cursor component styling |
| Checkbox | Done | Done | Mouse + keyboard toggle parity, pseudo-state styling, disabled semantics, and behavior tests |
| ListView | Done | Done | Mouse selection/hover + wheel scroll + ensure-visible keyboard navigation + selection messages/tests |
| Tabs | Done | Done | Header mouse+keyboard activation, focus/child lifecycle polish, CSS component styling, and activation messages/tests |
| Tree | Done | Done | Mouse/keyboard parity, CSS-driven row states, selection/toggle messages, and behavior tests |
| Markdown | Done | Done | Width-aware intrinsic sizing + default CSS baseline + behavior coverage |
| Pretty | Done | Done | Multiline fallback, component styling hooks, and intrinsic sizing |
| Modal / overlay | Done | Done | Focus-trap event routing, `Esc` dismiss semantics, and message-driven visibility control |
| Spacer | Done | Done | Intentional minimal widget with intrinsic width hints and default style semantics |

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
| Done | Textual-like naming | Naming guidance is now documented as source-of-truth in `docs/devel/TEXTUAL_COMPAT_MAPPING.md` (class/component/state naming + migration guidance) |
| Done | API mapping notes | Python Textual ↔ textual-rs concept/API mapping is documented in `docs/devel/TEXTUAL_COMPAT_MAPPING.md` and aligned with current `TextualApp`/message-bus/runtime APIs |
| Done | Adapter utilities | Added explicit adapter-breadth utilities in `src/textual_app.rs` / `src/event/mod.rs`: typed app message hooks for common patterns, compatibility runner aliases (`run_textual_app*`), overlay-backed screen push/pop helper (`OverlayScreenStack`), and message convenience wrappers that stay on the existing message bus |

---

## Phase 9: Debug + developer experience

**Goal:** make it easy to understand what's happening inside the framework.

| Status | Task | Notes |
|--------|------|-------|
| Done | File-based debug tracing | `TEXTUAL_DEBUG_INPUT_FILE`, `TEXTUAL_DEBUG_LAYOUT_FILE`, `TEXTUAL_DEBUG_STYLE_FILE`, `TEXTUAL_DEBUG_RENDER_FILE` |
| Done | Layout debug overlay | `DebugLayout` mode renders widget bounds and sizes |
| Done | Initial widget/CSS module organization | Widgets live in `src/widgets/` and CSS engine lives in `src/css/`; deeper decomposition tracked in Phase 9.7 |
| Done | DevTools panel | Runtime now exposes an embedded live-inspection substrate in `textual-rs` (snapshot + remote commands) and `textual-dev-rs` consumes it through `devtools` commands (`list`, `snapshot`, `focus`, `debug-layout`, `quit`) for Python-like live inspection workflows. Scope boundary remains: `textual-rs` owns embedded inspector/runtime hooks; `textual-dev-rs` owns external CLI tooling. |

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
2. **Phase 2: reusable preview scaffold fundamentals (completed)**
   - Added reusable preview composition helpers (`preview_root*`) for title/content/top/bottom sections.
   - Migrated `keys` and `data_table` demos to the shared scaffold composition.
3. **Phase 3: styling fidelity fundamentals (completed)**
   - Added reusable component-style resolution helpers and applied them in first-class keys widgets (`Header`, `KeyPanel`/`BindingsTable`) with CSS-driven component tokens/states.
4. **Phase 4: visual regression discipline (completed)**
   - Added snapshot coverage for preview scaffold + keys parity surface (`tests/preview_root_snapshot.rs`, `tests/keys_preview_snapshot.rs`).

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

## Phase 9.6: `TabbedContent` + Footer + Command Palette parity

**Goal:** close the remaining parity gap surfaced by the `tabbed_content` Python demos by implementing framework fundamentals (not demo-only styling patches).

This phase is intentionally split by ownership boundary:
- **Widget defaults:** `Tabs` / `TabbedContent` visual and interaction semantics.
- **App/runtime:** binding metadata, footer hint rendering, command palette invocation and UI.
- **Shared styling:** markdown heading parity that affects tabbed demos and other screens.

### Parity gap classification (current)

| Area | Status | Gap |
|------|--------|-----|
| `TabbedContent` visuals | Done | Active/focus/underline rhythm now matches the Python default hierarchy via widget default CSS + component focus hooks |
| Footer bindings | Done | Footer bindings are generated from active binding hints with grouping/compact behavior and right-docked command palette slot parity |
| Command palette | Done | Command palette lifecycle is now screen/overlay-aware, preserves/restores wrapped focus across open/close, and includes provider lifecycle + transition regression coverage |
| Markdown heading style | Done | `Markdown` now applies heading component-style hooks (`markdown--h1` … `markdown--h6`) with Textual-like defaults |

### Scope and sequence

| Status | Step | Notes |
|--------|------|-------|
| Done | Structured binding model | Runtime now carries richer binding metadata in `BindingHint` (`show`, display, grouping, priority/system), app APIs to register visible hints, focused-path collection when widgets are focused, and app/screen lifecycle-aware rebroadcasting when binding scope changes |
| Done | Footer from active bindings | `Footer` consumes `BindingsChanged`, renders showable bindings, groups consecutive non-command bindings by group label, applies compact spacing parity, and keeps command-palette hints docked in the right slot |
| Done | `Tabs`/`TabbedContent` default CSS parity | Tightened default visual rhythm and focus-state parity: calmer unfocused active tabs, block-cursor-focused active tabs, and focused underline bar treatment through component class hooks, with targeted style regression tests |
| Done | Command palette fundamentals | Added `CommandPalette` widget (search + results + execute/dismiss), runtime priority routing for `Action::CommandPalette`, default `ctrl+p` action-map binding, message-driven command-list updates (`CommandPaletteSetCommands`), provider lifecycle hooks (open/select/close startup-shutdown wiring), and overlay/screen transition-aware close + focus restoration behavior |
| Done | Markdown heading parity pass | Added widget-level heading style hooks + default CSS component styles; no demo-level overrides required |
| Done | Regression coverage | Tab activation + footer binding + command-palette lifecycle tests and open/closed palette snapshots are in place |

### Acceptance criteria

- `examples/tabbed_content.rs` and `examples/tabbed_content_label_color.rs` match Python behavior and visual hierarchy without demo-specific logic.
- Footer hints are generated from active bindings (not hardcoded), including `^p palette` when command palette is enabled.
- `ctrl+p` opens command palette in-app; palette supports basic search, selection, execute, and dismiss.
- `TabbedContent #--content-tab-<id>` selectors remain supported and tested.
- Tab strip active/focus/hover visuals are controlled by widget defaults and CSS components, not ad-hoc render branches in demos.
- Snapshot/behavior tests cover at least:
  - tab activation state transitions (keyboard + mouse),
  - footer hint composition from bindings,
  - command palette lifecycle (open, choose command, close).

### Implementation notes (cross-session)

1. Land binding-model uplift first; Footer and command palette should both consume the same source of truth.
2. Keep command palette minimal on first pass (system commands + app commands) and extend providers later.
3. Do not patch parity at demo layer if the behavior is widget/runtime responsibility.
4. Treat `tabbed_content` screenshots as a regression target for this phase.

---

## Phase 9.7: Core modularization

**Goal:** reduce large monolith modules so parity/fundamentals work can continue safely and faster.

Reference plan:
- `docs/devel/MODULARIZATION_PLAN.md`

### Scope (high level)

| Status | Area | Notes |
|--------|------|-------|
| Done | Runtime decomposition | Split `src/runtime/mod.rs` into internal modules by concern (event loop, routing, render, helpers, types) |
| Done | Containers decomposition | Split `src/widgets/containers.rs` into per-widget modules under `src/widgets/containers/` plus shared helpers |
| Done | Default CSS ownership split | Split `src/css/defaults.rs` into per-widget CSS modules under `src/css/defaults/` with deterministic aggregation via `mod.rs` |
| Done | Selector engine decomposition | Split `src/css/selectors.rs` into AST/parser/matching/specificity modules under `src/css/selectors/` |

### Acceptance criteria

- Modularization commits are behavior-preserving by default and remain bisectable.
- Existing demos and focused tests continue to pass through each phase.
- No demo-specific hacks are introduced as part of refactors.
- Breaking API changes are allowed during alpha, but must be intentional and documented in `CHANGELOG.md`.
- Foundation work is documented and cross-session executable via `docs/devel/MODULARIZATION_PLAN.md`.

---

## Phase 10: API re-architecture (RA track)

**Goal:** remove the measured Rust-vs-Textual API friction (2.3–3.2× example LOC vs
Python; per-widget DOM boilerplate; closure-scoped mutation as primary path;
closed-world message enum; inheritance-shaped specialization) by changing where
state lives — framework-owned DOM node record, signals-first state, open
`TypeId`-dispatched messages, typed handles, composition-based specialization.

Detailed execution plan, evidence, acceptance criteria, and success metric live in
`docs/devel/REARCHITECTURE_PLAN.md` (source of truth, `RA-0`..`RA-5`).

| Status | Step | Notes |
|--------|------|-------|
| Done | RA-0 Verification gate | Harness + blocking CI landed (`dfc566e`, `25e58e9`); verified 8/8 green. Gate rule persists: no RA phase marked Done without harness + test + LOC-metric evidence |
| Done | RA-1 Open messages | Landed 2026-06-12 (21 commits, `321586c`..`03891bb`): `Message` enum removed, open trait + `impl_message!`, `TypeId` `MessageHandlers`, `#[on]` downcast codegen, third-party acceptance tests (`tests/open_messages.rs`). Gate evidence: full suite 2,564/0; pty harness 8/8 (5 Pass / 2 XFail unchanged); LOC metric re-measured 2.7x (unchanged — ergonomics targets are RA-2/RA-3) |
| Done | RA-2 Node-record split | Landed 2026-06-13 (`bd7d235`..`45a640c`): arena WidgetNode owns id/classes(HashSet)/styles/geometry/focus/hover/disabled; `Widget` trait shed ~15 identity/style/state methods. Keystone `45a640c` net −78 lines. Gate evidence: full suite green, pty 8/8 (5 Pass / 2 XFail unchanged), LOC 2.71 (flat — internal ownership change; ergonomics land in RA-3/RA-5). Residual: 4 widgets retain runtime-mutated `classes` exposed via `style_classes` (tracked) |
| Done (selective) | RA-3 Signals-first state | Landed 2026-06-13 (`d98a64e`..`HEAD`). Fair-tested: signals helps where Python uses `reactive` (five_by_five −27) and hurts where Python is imperative (dictionary +26, reverted). Applied selectively-correct across all 5 examples. Full suite green, parity 8/8. **LOC ≤1.5× target retracted as unreachable** (inflation = tests + explicit types + stubbed async, untouched by any RA phase). RA-4/RA-5 proceed on ergonomic merit, not LOC — see `REARCHITECTURE_PLAN.md` §5 |
| Done | RA-4 Typed handles | Landed 2026-06-13 (`3c5c4e7`..`5480d14`): `Handle<W>` + `HandleSlot<W>` in `src/handle.rs`; `QueryError::{Unmounted,TypeMismatch}`; `WidgetTree::tree_id` (cross-tree safety); mount-pipeline sink wiring; `AppRoot::with_child_handle`; app-level `query_one_typed`/`typed_handle`/`mount_typed`. 4 examples migrated (judgment rule applied — handles where clearer than selectors); the five_by_five label migration (`b5037ea`) was REVERTED — its proven query-mutate form is kept. RA-4 verification added interactive parity coverage and surfaced a separate pre-existing-or-RA-3 bug: **five_by_five is input-dead app-wide** (tracked as `five_by_five_after_move`/`five_by_five_help` xfails). Gate evidence: full suite green; pty harness 11 cases (7 Pass / 4 XFail). |
| Done | RA-5 Composition-based specialization | Landed 2026-06-13 (`a6b011e`,`a0d8917`): GameCell rewritten via Button containment (owns inner Button, `style_type_aliases`, `ButtonPressed` interception) — no macros/inheritance; delegation macros deprecated. Full suite 2,612/0; pty 11/11. **RA-1..RA-5 COMPLETE.** Final example-LOC mean 3.07× (rose from 2.71× — the ≤1.5× thesis is conclusively retracted; RA wins are architectural, not LOC) |

Acceptance target: mean example LOC ratio vs Python drops from 2.7× to ≤1.5× with
no parity-harness regressions. CSS engine, layout, compositor, and rich-rs
integration are explicitly out of scope (unchanged).

Parity scoreboard (2026-06-13, real-PTY harness, 11 cases incl. interactive):
9 Pass / 2 XFail. Pass: markdown (initial+toc), five_by_five (initial+after_move),
json_tree (initial+add_node+toggle_root), dictionary. XFail: code_browser_initial
(spurious horizontal scrollbar from `host_content_extent` measuring width:auto
children at layout width, not natural content width — fix attempted but introduced
a parallel-test global-state regression in keys_preview; reverted, diagnosis kept),
five_by_five_help (help-screen Markdown content renders empty). Session wins:
five_by_five input fixed (key identifiers), dictionary rendering fixed (dock
intrinsic height).

---

## Definition of Done (v0.1) — Achieved

- [x] A stable full-screen app loop (alt-screen + diff) with no flicker/garble.
- [x] Widget tree with focus, input events, and a small widget set.
- [x] Layout + styling MVP sufficient to build a multi-pane interactive app.
- [x] Snapshot tests that prevent regressions.

## Execution Plan (v0.2, Single Source of Truth)

### Recently Closed Streams
- Widget closure program (Tier A/B/C + primitives + message bus + grapheme) is complete in current scope.
  Source of truth remains `docs/devel/WIDGET_PORTING_PLAN.md`.
- Invalidation model + style invalidation closure: landed.
- One-shot timers + broader async task runtime closure: landed.
- Terminal/golden coverage expansion: landed.
- Rich-rs integration contract closures (hyperlink policy + deterministic widget-id policy decision): landed.
- Phase 8 compatibility/doc ergonomics closure (including adapter utilities breadth): landed.
- DevTools panel + external live-inspection plumbing closure: landed.
- **TCSS Property Parity** (P2-24..P2-36): 52 CSS properties wired, ~108/108 coverage. Landed.
- **CSS Defaults Parity** (DC-00..DC-38/DC-ALL): all 16 default CSS files rewritten to Python parity with `&` nesting, DCE-Core prerequisites, and 3 new integration test files (~1,700 lines). Landed.
- **CSS Engine Parity** (DCE-01..DCE-12): pseudo-classes (`:ansi`/`:inline`/`:blur`/`:nocolor`), transparent/ANSI color parsing, text-style negation, markdown tokens, tint rendering, auto foreground. Landed.
- **Box-model parity**: `line-pad` separated from CSS padding, default `box-sizing` changed to `border-box`, vertical margin collapsing, `:can-focus` pseudo-class + disabled dimming. Landed.
- **Render parity** (P2 deferred closures): border captions, keylines, `overlay: screen` blending, scrollbar hover/active sub-parts, absolute min/max constraints, `expand` in flow sizing. Landed.
- **Per-property `transition`** dispatch: runtime auto-emits AnimationRequests on style changes for animatable properties (`opacity`, `text_opacity`, `offset_x`, `offset_y`). Landed.
- **Full CSS transition property coverage (P2-36)**: all 12 style-value animatable properties now dispatched via `step_style()` — colors (`fg`, `bg`, `tint`, `background_tint`), dimensions (`width`, `height`, `min/max` variants), spacing (`margin`, `padding`). Landed.
- **App-level reactive bridge**: `TextualApp` structs with `#[derive(Reactive)]` fields can now use `app.reactive_ctx()` in hook methods; `reactive_widget_mut()` enables watcher dispatch; `on_mount_with_app()` supports init-phase watcher firing after tree build. Landed.
- **Phase 9.7 modularization**: runtime, containers, default CSS, and selector engine all decomposed. Landed.

### Open Items
- **Example regressions tracked as parity-harness xfails** — the 2026-06-12 audit
  found real-terminal regressions invisible to the (fully green) test suite: missing
  border types (`round`/`double`/`dashed`/`ascii`/`panel`/`thick`/`wide`/`inner`/`blank`
  absent from `parse_border_edge`), `code_browser` DirectoryTree never rendering,
  `json_tree` app-level bindings dead, `dictionary` results container missing.
  The blocking real-PTY parity harness landed (`431e918`, `dfc566e`, `25e58e9`;
  see `tests/pty_parity.rs`); these regressions remain open as its strict xfail
  manifest entries — fixing one without promoting it to a pass case fails CI.
- **Phase 10 API re-architecture (`RA-0`..`RA-5`)** — see Phase 10 table and
  `docs/devel/REARCHITECTURE_PLAN.md`.

### Doc Discipline
- After each merged stream, update `ROADMAP.md` and the relevant source-of-truth docs in the same batch to prevent drift.
- `docs/devel/CSS_PARITY_PLAN.md` and `docs/devel/PARITY_ACTION_PLAN.md` execution is complete; these docs remain as historical references.
