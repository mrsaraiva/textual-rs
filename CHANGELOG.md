# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### 2026-02-05
- Improved scroll interaction fundamentals:
  - Added deterministic scroll action routing (focused target first, then hovered target, then global fallback) to reduce split-view ambiguity.
  - Added `Shift + mouse wheel` remapping for horizontal scrolling in scrollable containers, while keeping native horizontal-wheel support.
  - Added/expanded scroll diagnostics logs and introduced `examples/horizontal_scroll.rs` for manual QA of vertical/horizontal scroll behavior and clamping.
  - Fixed container event-forwarding gaps so wrapped scrollables (e.g. `ScrollView` inside `Panel`) reliably receive action and mouse-scroll input.
- Implemented dirty-flag rendering: the runtime now only re-renders when something actually changes (input, hover, style reload, active-state transitions), instead of every tick. Added `EventCtx::request_repaint()` so widgets can explicitly request a repaint. `dispatch_event()` returns a `DispatchOutcome` and `poll_stylesheet()` returns a `bool` to propagate dirty signals.
- Modularized the codebase: split the monolithic `controls.rs` into one file per widget (`button.rs`, `list_view.rs`, `data_table.rs`, `tree.rs`, `tabs.rs`, `checkbox.rs`, `spacer.rs`, `input.rs`), renamed `src/widget/` to `src/widgets/`, and extracted CSS styling into a dedicated `src/css/` module. Re-exported `ButtonVariant` in the public prelude.
- Switched terminal driver to the shared `richtui-crossterm` `TerminalDriver` and removed the legacy driver module. Updated `rich-rs` dependency to use the published crate (v1.0.2) instead of a local path.
- Mirrored Python Textual's Input demos as three separate Rust examples (`input`, `input_types`, `input_validation`) and advanced Input fundamentals for parity:
  - Correct default layout height so multiple Inputs stack correctly under `Container`.
  - Cursor renders over placeholder text when focused and empty.
  - Cursor blink matches Textual (toggle every 0.5s using `Instant`).
  - Fixed initial Tab cycling so focus traversal starts from the true focused widget.
  - Added default invalid Input styling (red border) and a small `Pretty` widget used by the validation demo.
- Added `TextArea` widget + demo, and advanced TextArea fundamentals via additional demo ports:
  - New examples mirroring Python Textual: `text_area_example`, `text_area_selection`, `text_area_extended`, `text_area_custom_theme`, `text_area_custom_language`.
  - Selection model + public selection API (`TextAreaSelection` / `TextAreaCursor`) including end-of-line selection rendering; added keyboard selection expansion (`Shift+arrows/Home/End`) and improved gutter behavior past EOF.
  - Focus awareness: new `Event::AppFocus(bool)` and CSS `:focus` gating so focus visuals/carets hide when the terminal window loses focus; added current-line highlight styling for TextArea.
  - Extensibility: `TextArea::on_key` hook (prevent default) plus helpers (`insert`, `move_cursor_relative`).
  - Theming + syntax highlighting: `TextAreaTheme`, theme registration, language registration, and tree-sitter highlighting (built-in Python + demo-registered Java), with cache invalidation so highlighting applies on first render.
  - Fixed deletion on terminals that send Backspace as `KeyCode::Char('\u{7f}')` / `KeyCode::Char('\u{08}')`.
- Introduced an initial message bus: `EventCtx::post_message()` collects `MessageEvent`s during event dispatch; the runtime delivers them via bubbling `Widget::on_message` handlers. `Input` and `Checkbox` now emit Textual-like messages (`InputChanged` / `InputSubmitted` / `CheckboxChanged`). Updated the `input_validation` example to consume `InputChanged` instead of a direct callback.
- Migrated the `buttons` demo to use `ButtonPressed` messages instead of direct callbacks, and added `DataTable` messages (`DataTableCursorMoved`, `DataTableHeaderSelected`, `DataTableCellActivated`) with a status line in the `data_table` demo.
- Fixed message delivery regressions for deep widget trees by routing queued messages deterministically through the widget tree (`Widget::on_message`), which restores status/event updates in demos like `buttons`.

### 2026-02-04
- Added button pressed visual effect with `:active` CSS rules (border inversion + background tint). Mouse presses track actual button state via new `MouseUp` event; keyboard activations use a brief timer.
- Added a lightweight unit test to guard synchronized output bracketing behavior.
- Implemented Kitty pointer-shape protocol (OSC 22) for hover cursor feedback, with best-effort terminal detection and `TEXTUAL_POINTER_SHAPES` override. OSC sequences are written through `Console` (shared with the render pipeline) to prevent interleaving on stdout. Added `mouse_interactive()` widget trait so non-focusable widgets like disabled buttons still show hover cursors.
- Prevented resize tearing / corruption by:
  - Bracketing frame writes with synchronized output (DECSET 2026). Disable with `TEXTUAL_SYNC_OUTPUT=0`.
  - Disabling terminal line wrap while running in alt-screen mode (restored on exit).
- Added in-demo status line wiring and event reporting for the buttons demo.
- Fixed selector matching bugs (direct child combinator semantics) so width rules like `Horizontal > VerticalScroll { width: 24; }` apply correctly.
- Added focused debug tracing via env vars (`TEXTUAL_DEBUG_INPUT_FILE`, `TEXTUAL_DEBUG_LAYOUT_FILE`, `TEXTUAL_DEBUG_STYLE_FILE`, `TEXTUAL_DEBUG_RENDER_FILE`).
- Added demo SVG snapshot harness and shared demo snapshot helper for reuse across examples.
- Added pseudo-classes and interaction styling (hover/focus) with themed base tokens.
- Improved Button rendering parity (centering, intrinsic sizing via `width:auto`, line padding, border shorthands, and bleed fixes).

### 2026-02-03
- Added margin/padding/border subset and an initial Button demo.
- Refactored widgets out of a monolithic `src/widget/mod.rs` into submodules (containers/layout/helpers/selectors/controls/text).
- Expanded widget catalog: Label/Static, Button, Input, Checkbox, ListView, DataTable, Tree, Tabs, Markdown, Modal/overlay.
- Added stylesheet hot-reload (file watch) and examples.
- Added selector combinators (descendant/child), grouping, specificity, and inheritance rules.
- Introduced styling MVP: typed style props, theme tokens, selector model, stylesheet parsing.
- Added focus + event routing MVP (tab traversal, key bindings, action map).
- Added ScrollView (vertical) + nested clipping refinements and horizontal scrolling.
- Implemented early layout primitives (row/column/dock/grid-ish), debug overlays, and clipping regions.
- Added terminal runtime loop foundations with resize hooks and event dispatch scaffolding.
- Documented the rich-rs integration contract and rendering metadata expectations.
