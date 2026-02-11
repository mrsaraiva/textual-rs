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
| Todo | Hyperlink id policy | For OSC8: rely on `rich-rs` per-Console URL→id registry when `link_id` is omitted |
| Todo | Deterministic ids (open) | Widget IDs are random (`WidgetId::new()`); consider hash-based IDs if needed for persistence/snapshots |
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
| Partial | Computed styles | On-demand style resolution (selector match + cascade + inline + inheritance) exists in render path; no cached per-widget computed-style tree yet |
| Partial | Style invalidation | Stylesheet watch reload is applied in the runtime tick loop; app still uses full-frame dirty redraw (no selective style invalidation / dirty regions) |

Deliverable: ~~style a UI via a stylesheet-like source and hot-reload it.~~ **Done.**

---

## Phase 6: Async + animations + timers

**Goal:** "reactive UI" feel: background tasks, spinners, animations, transitions.

| Status | Task | Notes |
|--------|------|-------|
| Done | Tick system | Adaptive tick cadence (idle 100ms / active ~16ms) with `on_tick` propagation and event-loop repaint scheduling |
| Done | Message bus | `Message` / `MessageEvent` + runtime message queue + bubble delivery via `Widget::on_message` are now the widget interaction integration surface. PR8F (2026-02-11) closed remaining `Select` direct-coupling paths by routing open-dropdown selection through `OptionList` message flow (`OptionSelected` -> `SelectChanged`) and added ordering regressions across `OptionList`/`Select`/`SelectionList`. |
| Done | Grapheme-aware text editing model | Shared text-edit command core drives `Input` / `MaskedInput` / `TextArea`, with grapheme-sensitive follow-up closure for `MaskedInput` cursor/render paths plus `DataTable`/`Tree` width-hit-testing and wrapping edge regressions (`ZWJ`, combining marks, wide-cell labels) landed in PR8E (2026-02-11) |
| Todo | One-shot timers | No timer API beyond the tick counter |
| Done | Animation framework | Animator/easing pipeline, runtime animation queue, CSS transition parsing, and widget integrations (tabs/tabbed/scroll/palette) are in place |
| Todo | Async tasks | `run_widget_tree` is async but no `spawn`/`select!` patterns for background work |

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
| Partial | Textual-like naming | CSS class conventions (`-style-default`, `-primary`, etc.) and property names mirror Textual where practical |
| Partial | API mapping notes | App composition/messaging mapping notes exist; broader Python Textual ↔ textual-rs conceptual mapping is still incomplete |
| Partial | Adapter utilities | `TextualApp` trait plus async/sync/snapshot runners cover core app wiring; broader compatibility shortcut layer is still limited |

---

## Phase 9: Debug + developer experience

**Goal:** make it easy to understand what's happening inside the framework.

| Status | Task | Notes |
|--------|------|-------|
| Done | File-based debug tracing | `TEXTUAL_DEBUG_INPUT_FILE`, `TEXTUAL_DEBUG_LAYOUT_FILE`, `TEXTUAL_DEBUG_STYLE_FILE`, `TEXTUAL_DEBUG_RENDER_FILE` |
| Done | Layout debug overlay | `DebugLayout` mode renders widget bounds and sizes |
| Done | Initial widget/CSS module organization | Widgets live in `src/widgets/` and CSS engine lives in `src/css/`; deeper decomposition tracked in Phase 9.7 |
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

## Phase 9.7: Core modularization (next priority)

**Goal:** reduce large monolith modules so parity/fundamentals work can continue safely and faster.

This is a **foundational maintenance phase**, not feature work. It is tracked as the immediate next priority because current monolith hotspots increase regression risk and slow iteration.

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

## Definition of Done (v0.1) — Achieved

- [x] A stable full-screen app loop (alt-screen + diff) with no flicker/garble.
- [x] Widget tree with focus, input events, and a small widget set.
- [x] Layout + styling MVP sufficient to build a multi-pane interactive app.
- [x] Snapshot tests that prevent regressions.

## Execution Plan (v0.2, Single Source of Truth)

- Now: Widget PR6 baseline missing-widget ports are landed for `Log` (PR6A), `Tooltip`/`HelpPanel` (PR6B), and `DirectoryTree`/`Welcome` (PR6C).
- Landed (2026-02-10): widget follow-up slices for `RichLog` + `CommandPalette` interaction hardening, `ListView`/`Tree` disabled-navigation semantics, and shared text-edit clipboard message hooks across `Input`/`MaskedInput`/`TextArea`.
- Landed (2026-02-10): widget follow-up slices for `Header`/`Footer` lifecycle polish, tooltip/help-panel positioning and default CSS parity pass, and `DirectoryTree` lazy-loader fidelity improvements.
- Landed (2026-02-10): container-family parity baseline slice (`PR7F`) with new `Vertical`/`Center`/`Right`/`Middle` aliases, focusable scroll containers, and Home/End + ctrl+PageUp/PageDown scroll bindings.
- Landed (2026-02-10): Tier-B/C polish slice (`PR7G`) with highlighted-vs-selected semantics for `ListView`/`Tree`, runtime clipboard store plumbing, and lifecycle polish for `Welcome`/`Tooltip`/`HelpPanel`.
- Landed (2026-02-10): Tier-A/Tier-C follow-up slice (`PR7H`) with DataTable/Tabs message+lifecycle hardening, RichLog markup/renderable support, CommandPalette small-viewport/markup rendering polish, and additional overlay/widget lifecycle regressions.
- Landed (2026-02-11): widget closure push (`PR7I`) with DataTable fixed-column horizontal-shift behavior + shifted hit-testing, Tabs/TabbedContent activation geometry replay, CommandPalette screen-space hit-testing fix, RichLog focus-style parity update, runtime-driven Tooltip/HelpPanel message APIs, DirectoryTree typed file/dir selection messages, Welcome lifecycle/CSS polish, and Tier-B regressions for disabled highlight handling + Footer/text-edit clipboard shortcut polish.
- Landed (2026-02-11): widget closure follow-up (`PR7J`) with explicit activation-message paths for `ListView`/`Tree`, tree twisty-click toggle-vs-activate parity, header icon press message emission, and footer unmount lifecycle reset for deferred bindings.
- Landed (2026-02-11): widget closure recovery batch (`PR7K`) with DataTable horizontal-offset stability fixes, Tabs/TabbedContent binding-hint + unmount lifecycle polish, CommandPalette local-coordinate hit-test hardening, RichLog multiline auto-scroll estimation, single-line clipboard paste normalization for Input/MaskedInput, and DirectoryTree deferred load queue with collapse-time cancellation.
- Landed (2026-02-11): widget primitive closure batch (`PR8A`: A/B/C) with focused HELP metadata pipeline, runtime async task primitive baseline (`spawn`/`cancel`/completion delivery), DirectoryTree async-task migration, and `hkey`/`vkey` CSS border parser+renderer support used by HelpPanel/KeyPanel parity defaults.
- Landed (2026-02-11): Tier-A final closure batch (`PR8C`) with DataTable horizontal viewport/scrollbar + key-lifecycle parity hardening, RichLog default markup/highlighter semantics, and CommandPalette close-animation interaction gating + unmount lifecycle reset.
- Landed (2026-02-11): Tier-B/Tier-C closure follow-up (`PR8D`) with press/release activation parity for `ListView`/`Tree`, header press-region interaction fidelity, expanded platform text-edit shortcuts, `Select`/`OptionList` highlight lifecycle cleanup, and scrollbar drag-release repaint fixes in `Log`/`KeyPanel`.
- Next (widget-first): close remaining widget parity/hardening slices before non-widget streams, prioritizing Tier-A/B gaps plus container-family parity gaps tracked in `docs/devel/WIDGET_PORTING_PLAN.md`.
- During widget-first execution: land message-bus and grapheme follow-ups as part of each widget PR slice (no callback shims; alpha breakage is acceptable when it improves fundamentals).
- Then: remaining infrastructure closures (dirty/style invalidation, timers/async, golden coverage, integration-contract closures, compatibility/docs).
- Doc checkpoint rule: after every merged widget PR, update both `ROADMAP.md` (milestone/checklist status) and `docs/devel/WIDGET_PORTING_PLAN.md` (widget-level matrix/notes) in the same work batch.

### Widget Reality Snapshot (2026-02-11 audit, post `PR7K`)

High-confidence widget work already landed in recent commits (`112c29a`..`25b2deb`) includes:
- Tier-A hardening for `DataTable`, `Tabs`/`TabbedContent`, `RichLog`, `CommandPalette`.
- Tier-B interaction/message hardening for `ListView`/`Tree`, `Header`/`Footer`, and shared text-edit clipboard flows.
- Tier-C utility lifecycle hardening for `Tooltip`/`HelpPanel`/`DirectoryTree`/`Welcome`.
- No missing widget ports remain; remaining widget work is parity/polish/fundamentals closure.

Still-open widget gaps are now concentrated in:
- `HelpPanel`: focused-widget HELP metadata source parity.
- `DirectoryTree`: deeper async scheduling parity (beyond tick-queued deferred load).
- `Tooltip`: deeper CSS/parser-feature parity.

## Ordered PR Streams (Open Todo/Partial)

This checklist turns remaining `Todo`/`Partial` items into concrete, reviewable PR slices.
Order is prioritized for widget-first execution while keeping fundamentals and regression risk under control.

1. Widget first-class closure program (Phase 7 + widget plan)
   - PR 1: Continue Tier-A closure pass after PR5/PR6 landings (`DataTable`, `Tabs`/`TabbedContent`, `RichLog`, `CommandPalette`), with targeted behavior + styling parity deltas per slice.
     - PR7A (2026-02-10): `RichLog` now preserves scroll-anchor behavior when max-line trimming drops head rows and keeps explicit multi-line styled writes intact; `CommandPalette` now emits `CommandPaletteCommandSelected` for built-ins (`keys`, `quit`) before close, with focused regressions.
     - PR7I (2026-02-11): additional Tier-A closure hardening landed: `DataTable` fixed-column-preserving horizontal shift + shifted header hit-test mapping; `Tabs`/`TabbedContent` activation now replays latest geometry to newly active content; `CommandPalette` click hit-testing corrected to screen-space coordinates and animated panel Y; `RichLog` focus-style parity moved to background-tint with regression coverage.
     - PR7K (2026-02-11): Tier-A recovery polish landed: `DataTable` horizontal-offset stability + home/end cursor visibility alignment, switchable-target binding-hint gating and unmount lifecycle reset for `Tabs`/`TabbedContent`, local-coordinate-safe `CommandPalette` hit-testing, and multiline auto-scroll estimation for `RichLog`.
     - PR8C (2026-02-11): Tier-A final closure landed: `DataTable` now has horizontal viewport scrollbar parity (render/track/drag/action-wheel) with key-lifecycle alignment, `RichLog.write(...)` now honors default markup/highlighter semantics, and `CommandPalette` blocks child interaction while close-animation panel remains visible plus unmount lifecycle reset coverage.
   - PR 2: Tier-B closure slices (`ListView`/`Tree`, text-edit follow-up including clipboard hooks, `Header`/`Footer` lifecycle polish) aligned to Python semantics.
     - PR7B (2026-02-10): `ListView`/`Tree` now support disabled-item/node navigation semantics (keyboard + mouse skip/ignore + disabled classes); shared text-edit clipboard hooks are message-bus-first via `TextEditClipboard*` messages with focused regressions for `Input`/`MaskedInput`/`TextArea`.
     - PR7C (2026-02-10): `Header`/`Footer` lifecycle polish landed: hover state cleanup on focus/unmount for `Header`, and deferred `BindingsChanged` handling in `Footer` while app is unfocused with focused replay on regain.
     - PR7I (2026-02-11): Tier-B follow-up landed: `ListView`/`Tree` no longer render highlighted/selected markers when only disabled rows/nodes are present; `Footer` now preserves deferred binding updates across repeated focus-loss cycles; shared text-edit key mapping now supports `SUPER+X`/`SUPER+V`.
     - PR7J (2026-02-11): deeper Tier-B interaction semantics landed: `ListView` and `Tree` now emit explicit activation messages on enter/click, tree twisty clicks toggle without activation side effects, header icon clicks emit `HeaderIconPressed`, and footer unmount resets deferred-focus tracking state.
     - PR8D (2026-02-11): final Tier-B polish follow-up landed press/release activation lifecycle for `ListView`/`Tree`, header press-region parity, and expanded platform text-edit shortcut fidelity.
   - PR 3: Tier-C/utility closure slices (toggle/list family polish, `Tooltip`/`HelpPanel` positioning/default CSS, `DirectoryTree` async loader fidelity, `Welcome` lifecycle/CSS parity).
     - PR7D (2026-02-10): `Tooltip`/`HelpPanel` parity pass landed with anchor-aware tooltip positioning (clamp + inflection), help-panel split/lifecycle fixes, and default CSS component updates.
     - PR7E (2026-02-10): `DirectoryTree` lazy-loader fidelity landed with expandable-directory support in `Tree`, message-driven lazy child loading, and refresh behavior that preserves expanded paths.
     - PR7I (2026-02-11): utility parity follow-up landed runtime-driven `Tooltip` anchor updates via message bus, `HelpPanel` typed HELP-content message handling, typed `DirectoryTree` file/directory selection messages, and `Welcome` close-row hover lifecycle + baseline default CSS surface polish.
     - PR7K (2026-02-11): utility recovery polish landed tick-queued `DirectoryTree` deferred loading with collapse-time cancellation and unmount-state cleanup for `HelpPanel`/`Tooltip`/`Welcome`.
     - PR8D (2026-02-11): utility/list polish landed `Select`/`OptionList` highlight lifecycle synchronization and scrollbar drag-release repaint parity in `Log`/`KeyPanel`.
   - PR 4: Container-family parity closure (Python `containers.py` alignment: behavior/default CSS/lifecycle for missing or partial container semantics).
     - PR7F (2026-02-10): baseline parity slice landed: new `Vertical`/`Center`/`Right`/`Middle` aliases plus `VerticalGroup`/`HorizontalGroup`/`ScrollableContainer`/`CenterMiddle`/`ItemGrid` compatibility classes, `ScrollView`/`HorizontalScroll` focusability, and key/action parity additions (`ScrollHome`/`ScrollEnd`, `ctrl+pageup/pagedown` horizontal paging bindings) with focused regression tests.
   - PR 5: Tier-B/C polish follow-up.
     - PR7G (2026-02-10): landed highlighted-vs-selected class semantics for `ListView`/`Tree`, app/runtime clipboard request/response plumbing for text-edit widgets, `Welcome` close-lifecycle polish, and runtime-driven tooltip/help-panel lifecycle updates.
   - PR 6: Tier-A/Tier-C hardening follow-up.
     - PR7H (2026-02-10): landed additional parity hardening for `DataTable`/`Tabs`/`TabbedContent` (message/lifecycle regressions), `RichLog` (markup/renderable input paths), `CommandPalette` (small viewport + markup result rendering), and overlay/widget lifecycle delegation regressions.
   - PR 7: Per-slice doc sync checkpoint: update `docs/devel/WIDGET_PORTING_PLAN.md` matrix + relevant `ROADMAP.md` checklist rows in the same commit series.
   - Exit criteria: widget plan matrix has no unowned `Partial` items for the current target tier, and each closed slice has focused behavior tests.

2. Widget-blocking primitive closures (explicit)
   - PR 1: Focused-help metadata pipeline for `HelpPanel` (widget/source -> runtime -> focused help sink).
   - PR 2: Async task primitive baseline (`spawn`/completion/cancel) to support true non-blocking loader parity (`DirectoryTree` and future widgets).
   - PR 3: CSS/parser closure items required for final tooltip/help styling parity.
   - PR8A (2026-02-11): landed all three baseline primitives:
     - focused-help metadata pipeline via `Widget::help_markup` + runtime focused-help dispatch + `HelpPanel` focused-help message handling;
     - runtime async-task baseline with `AsyncTask*` messages plus `DirectoryTree` migration to async load/cancel flow;
     - CSS/parser/style support for `hkey`/`vkey` borders with HelpPanel/KeyPanel default parity updates and parser/widget regressions.
   - Exit criteria: widget plan no longer lists primitive blockers as external dependencies.

3. Message bus completion (Phase 6, widget-coupled)
   - PR 1: Audit remaining widgets with state-changing interactions and add missing `Message` variants as part of widget closure slices.
   - PR 2: Replace remaining direct internal event coupling with message emissions + `on_message` consumers (no compatibility shims).
   - PR 3: Add per-widget regression tests asserting message emission order/content on interaction.
   - PR8F (2026-02-11): closed remaining `Select` direct click/index coupling by routing dropdown selection through inner `OptionList` message emission and `Select::on_message` consumption; added explicit ordering regressions for `OptionList` (`OptionHighlighted` before `OptionSelected`), `Select` (`OptionSelected` before `SelectChanged`), and `SelectionList` (`SelectionListToggled` before `SelectionListSelectedChanged`).
   - Exit criteria: no callback-style integration surfaces for widget interactions; roadmap row can move to `Done`. **Met (PR8F, 2026-02-11).**

4. Grapheme model completion beyond `Input`/`TextArea` (Phase 6, widget-coupled)
   - PR 1: Migrate remaining text-heavy widgets (`MaskedInput`, then `DataTable`/`Tree`/wrapping edge-cases) to shared grapheme-safe helpers.
   - PR 2: Add combining-mark + ZWJ + wide-cell regression tests for editing/hit-testing/truncation.
   - PR8E (2026-02-11): landed `MaskedInput` grapheme-aware cursor-x mapping and width-clamped render runs, plus `DataTable` and `Tree` ZWJ/combining/wide-cell regressions for hit-testing and wrapping width edges.
   - Exit criteria: grapheme row moves from `Partial` to `Done` with cross-widget regression coverage. **Met (PR8E, 2026-02-11).**

5. Dirty invalidation + style invalidation (Phases 2 + 5)
   - PR 1: Add dirty-region tracking in `FrameBuffer`/runtime and repaint only touched regions.
   - PR 2: Add selective relayout invalidation flags (layout/style/content) instead of global dirty redraw.
   - PR 3: Wire stylesheet reload to selective style invalidation (affected subtree/type/class), not full-app redraw.
   - Exit criteria: unchanged screenshots and focused performance tests for reduced redraw area.

6. One-shot timers + async task primitives (Phase 6)
   - PR 1: Introduce one-shot timer API (schedule/cancel) integrated with runtime loop.
   - PR 2: Introduce background-task API (`spawn` + completion message delivery + cancellation semantics).
   - PR 3: Add demo/test proving non-blocking background work + timer-driven UI updates.
   - Exit criteria: timers/tasks rows move to `Done`, with runtime-level tests.

7. Terminal/golden coverage expansion (Phases 0.5 + 1)
   - PR 1: Add metadata golden assertions for framebuffer->diff->output invariants (beyond current snapshot coverage).
   - PR 2: Add raw terminal-output capture test harness for deterministic control-sequence checks.
   - Exit criteria: “Golden tests” row in Phase 1 can move to `Done`.

8. Rich-rs integration contract closures (Phase 0.5)
   - PR 1: Implement/document hyperlink ID policy usage (`StyleMeta.link`/`link_id`) where links are rendered.
   - PR 2: Decide deterministic widget-id policy (explicitly keep runtime IDs non-deterministic, or add optional stable IDs for persistence/snapshots).
   - Exit criteria: both Phase 0.5 `Todo` rows move to `Done` or to explicit `Won't do (for now)` notes.

9. Compatibility/doc ergonomics (Phase 8 + 9)
   - PR 1: Publish Python Textual ↔ textual-rs concept/API mapping doc (source-of-truth).
   - PR 2: Add adapter utilities for common patterns (screen push/pop helpers, message convenience wrappers).
   - PR 3: Scope/implement DevTools panel MVP (tree/focus/style inspection) or explicitly defer with rationale.
   - Exit criteria: Phase 8 rows progress to `Done`; DevTools has concrete status (Done or deferred with decision note).
