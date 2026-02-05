# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### 2026-02-04
- Added a lightweight unit test to guard synchronized output bracketing behavior.
- Implemented Kitty pointer-shape protocol (OSC 22) for hover cursor feedback, with best-effort terminal detection and `TEXTUAL_POINTER_SHAPES` override.
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
