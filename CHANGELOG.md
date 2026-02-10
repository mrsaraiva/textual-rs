# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### 2026-02-10
- **Grapheme-safe text editing core (Input/TextArea foundation)**
  - Added shared grapheme-aware text indexing helpers in `src/widgets/text_edit.rs` (boundary clamping, left/right navigation, and cell/byte mapping).
  - Migrated `Input` and `TextArea` cursor movement, backspace/delete behavior, mouse hit-testing, and width-aware rendering loops to use grapheme boundaries.
  - Added targeted regression coverage for combining-mark and ZWJ emoji editing semantics (`src/widgets/input.rs` tests and `tests/text_area_widget.rs`).
- **Message-bus-only text widget integration (breaking)**
  - Removed callback hooks from text widgets: `Input::on_change`, `TextArea::on_change`, and `TextArea::on_key`.
  - Added `Message::TextAreaChanged { value }` and now emit it on text edits from key-driven interactions.
  - Updated `examples/text_area_extended.rs` to implement key customization via a wrapper widget/event handling, instead of per-widget callback hooks.
- **Message-bus-only `MaskedInput` integration (breaking)**
  - Removed `MaskedInput::on_change`; `MaskedInput` now follows the same message-only integration model as `Input`/`TextArea`.
  - Kept `Message::InputChanged` / `Message::InputSubmitted` as the supported integration surface and added regression coverage for change message emission.
- **Message-bus-only `Button` integration (breaking)**
  - Removed `Button::on_press`; button activation now integrates via `Message::ButtonPressed` only.
  - Added regression coverage for key-triggered button message emission.
- **Message-bus interaction coverage for `Header` + `Placeholder`**
  - Added `Message::HeaderToggled { tall }` when header body clicks toggle tall mode.
  - Added `Message::PlaceholderVariantChanged { variant }` when placeholder clicks rotate variant state.
  - Added widget-level regression tests validating emission and no-op paths.
- **Message-bus interaction coverage for `Footer` + `KeyPanel` + `RichLog`**
  - Added `Message::FooterBindingsUpdated { count }` when footer binding hints update.
  - Added `Message::KeyPanelBindingsUpdated { count }` when key panel binding hints update.
  - Added `Message::KeyPanelScrolled { offset, max_offset }` and `Message::RichLogScrolled { offset, max_offset }` on user-driven scroll state changes.
  - Added targeted widget regression tests for message emission and no-op behavior.
- **Grapheme audit follow-up for text-heavy widgets**
  - Added targeted regression coverage for wide-grapheme behavior across `DataTable` column hit-testing, `Tabs` mouse header hit-testing, `Tree` intrinsic width calculations, and markdown heading component styling with emoji content.
- **Command palette provider plumbing (Phase 9.6)**
  - Added message-driven command updates: `Message::CommandPaletteSetCommands { commands }`.
  - `CommandPalette` now accepts runtime/app command list refreshes through the message bus and rebuilds results immediately.
- **Command palette provider lifecycle parity (Phase 9.6)**
  - Added `TextualApp` provider lifecycle hooks (`command_palette_providers`) with a new `CommandPaletteProvider` trait for startup, command enumeration, selection handling, and shutdown.
  - `TextualApp` adapter now wires provider lifecycle from palette message flow:
    - `CommandPaletteOpened` initializes providers and emits `CommandPaletteSetCommands`.
    - `CommandPaletteCommandSelected` routes selected command IDs to provider handlers.
    - `CommandPaletteClosed` (and unmount) shuts providers down and clears lifecycle state.
  - Added focused lifecycle regression coverage in `src/textual_app.rs` for open/select/close and reopen behavior.
- **Command palette overlay/screen transition parity (Phase 9.6)**
  - `CommandPalette` now captures and clears wrapped-child focus when opening, then restores the prior focus target (with safe fallback) on close.
  - Palette lifecycle now reacts to transition signals: overlay visibility/toggle/dismiss message flow and app focus loss both force-close the palette through the same message-bus path.
  - Added focused regression coverage for focus restoration, transition-triggered close, and command-selection/close message ordering (`src/widgets/command_palette.rs`, `tests/command_palette_lifecycle.rs`).
- **Phase 9.6 binding lifecycle + footer parity pass**
  - Runtime now enriches active binding lifecycle updates with focused-path widget hints (ancestor -> focused widget), then normalizes ordering/dedup for deterministic `BindingsChanged` emissions.
  - Completed app/screen lifecycle parity for bindings: runtime now rebroadcasts `BindingsChanged` when the active binding scope source chain changes (even when hint payload text is unchanged), and no-focus states now retain single-child app/screen scope hints.
  - `Tabs` and `TabbedContent` now expose focused binding hints for tab switching (`←/→`), so Footer/KeyPanel can reflect active tab-navigation affordances.
  - Footer now groups consecutive non-command bindings sharing the same group into compact key clusters with one trailing group label, and compact mode now tightens key/description spacing (including right-docked command-palette separator behavior).
  - Added regression coverage for focused-path binding hint collection, grouped footer rendering, compact spacing behavior, and footer right-docked command-palette slot behavior.
- **Phase 9.6 tab-strip default CSS parity tightening**
  - Tuned `Tabs` and `TabbedContent` defaults to match Python visual rhythm more closely: unfocused active tabs now keep panel rhythm, focused active tabs use block-cursor foreground/background + focus text style, and underline bars get focused-state treatment.
  - Added explicit focused underline component hooks in widget render paths (`-focus` class on underline components) so default CSS can style focus contrast without demo-specific logic.
  - Added targeted regression tests in `tests/tabs.rs` and `tests/tabbed_content.rs` that assert focused active-tab and underline styles from the default stylesheet.
- **Windows safe-borders policy (workaround, explicit opt-in)**
  - Kept Windows safe-borders as a workaround for terminal-specific block-border artifacts, but not enabled globally by default.
  - Standardized `TEXTUAL_WINDOWS_SAFE_BORDERS` parsing to support `on|off|auto` (plus boolean aliases), with `auto` currently resolving conservatively to off.
  - Added parser regression tests and documentation for the opt-in behavior.

### 2026-02-09
- **Tier C widget parity uplift (8 widgets)**
  - **Pretty (breaking):** redesigned to delegate to `rich_rs::Pretty` — now accepts `impl Debug` instead of `Arc<Mutex<Vec<String>>>`. Added `update()` method, static/shared modes.
  - **ProgressBar:** added ETA estimation, percentage display, `show_bar`/`show_percentage`/`show_eta` toggles, `animation_level` awareness. Fixed suffix width bug on narrow layouts.
  - **Digits:** added `DigitsAlign` enum and `text_align` support (left/center/right). Fixed CJK width calculation.
  - **Rule:** added reactive `set_orientation()` and `set_line_style()` setters.
  - **Link:** added `tooltip` field with builder/setter. Added focus/hover/activation tests.
  - **Placeholder:** added `disabled` state with event blocking and CSS opacity. Fixed text variant separator and word wrap.
  - **LoadingIndicator:** added `animation_enabled` flag with static "Loading..." fallback when disabled.
  - **Sparkline:** added edge-case test coverage (NaN, empty data, single value).
  - 135 new unit tests across all 8 widgets.
- **Core modularization (Phase M1 — behavior-preserving)**
  - Split `src/runtime/mod.rs` (2509 lines) into focused submodules: `event_loop.rs`, `routing.rs`, `render.rs`, `helpers.rs`, `types.rs`; `mod.rs` retains `App` struct and orchestration.
  - Split `src/widgets/containers.rs` (2964 lines) into per-widget modules under `src/widgets/containers/`: `container.rs`, `constrained.rs`, `styled.rs`, `node.rs`, `app_root.rs`, `frame.rs`, `panel.rs`, `scroll_view.rs`, `overlay.rs`.
  - Split `src/css/selectors.rs` (1609 lines) into `src/css/selectors/`: `ast.rs`, `parser.rs`, `matching.rs`, `resolver.rs`, `segments.rs`, `context.rs`, `debug.rs`.
  - Split `src/css/defaults.rs` (490 lines) into per-widget CSS fragment modules under `src/css/defaults/` with deterministic aggregator in `mod.rs`.
  - All splits are purely mechanical — no behavior changes, all 309 tests pass.
- **Event loop tick repaint fix**
  - Always repaint after `on_tick` to keep tick-driven widgets (counters, cursors) in sync.

- **App runner API simplification + sync entrypoints (breaking)**
  - Introduced concise runner names in `textual_app`: `run`, `run_with_output`, `run_snapshot`, `run_snapshot_with_output`, plus blocking variants `run_sync`, `run_sync_with_output`, `run_sync_snapshot`, and `run_sync_snapshot_with_output` (`src/textual_app.rs`, `src/lib.rs`).
  - Removed verbose compatibility aliases (`run_textual_app*`) to keep the public API surface minimal during alpha development.
  - Added typed app ergonomics to `TextualApp`: `on_button_pressed(...)` and optional `take_exit_output()` for simple app-result flows without external shared state.
  - Added `Static::class(...)` / `Static::id(...)` sugar to reduce composition boilerplate in examples.
  - Updated button examples accordingly:
    - `examples/buttons.rs` now uses top-down in-`compose` composition (doc-first readability) and sync snapshot runner (no async `main` required).
    - Added `examples/buttons_composed_pattern.rs` preserving the helper/indirection composition pattern as an alternative.
    - Updated `examples/buttons_advanced.rs` to the concise snapshot runner.
- **Examples API migration (didactic ergonomics pass)**
  - Migrated all remaining Rust examples to the new concise app runners and trait flow, removing direct runtime bootstrapping (`App::new` + `run_widget_tree`) from example entrypoints.
  - Standardized examples toward top-down composition in `TextualApp::compose` for readability as learning material, while preserving advanced behaviors (keys diagnostics, tabbed content interactions, validation flows, textarea customizations).
  - Reduced async boilerplate in examples by switching simple/demo entrypoints to sync runners (`run_sync` / `run_sync_snapshot*`) where no explicit async orchestration is needed.
- **Lockfile refresh cleanup**
  - Updated `Cargo.lock` to reflect current dependency graph with local `rich-rs` patching and removed stale registry/unused patch lock metadata.

- **Toast parity + border semantics refactor (no demo hacks)**
  - Refactored `Toast` rendering to stop manually painting a fake left accent strip; toast now renders content-only and relies on the shared widget style/border pipeline for border composition (`src/widgets/toast.rs`).
  - Added first-class `outer` border type support across style model, CSS parser, and border renderer (`src/style.rs`, `src/css/selectors.rs`, `src/widgets/helpers.rs`), then aligned toast defaults with Python (`border-left: outer ...`) in `src/css/defaults.rs`.
  - Preserved Python-like toast placement/stacking behavior improvements in runtime overlay composition, including side margin and toast width clamping (`src/runtime/mod.rs`).
  - Added inline bold support for toast message key hints (`[b]...[/b]`) and switched the app-level quit help toast to `Press [b]ctrl+q[/b] ...` formatting to match Python visual emphasis (`src/widgets/toast.rs`, `src/runtime/mod.rs`).
- **Safety policy hardening**
  - Enforced a crate-wide no-unsafe policy with `unsafe_code = "forbid"` in `Cargo.toml`, so any `unsafe` usage now fails compilation by default.
- **Roadmap prioritization update**
  - Added Phase 9.7 as the next priority in `ROADMAP.md`, formalizing a fundamentals-first modularization pass before further major parity expansion work.
- **App composition API fundamentals (Phase A)**
  - Added trait-based app authoring (`TextualApp`) and `run_textual_app()` runtime helper to reduce example/app boilerplate while preserving low-level `App::run_widget_tree` access.
  - Added app-level lifecycle/message/action hook surface (`on_mount`, `on_message`, `on_action`) via an internal adapter, enabling Python-like minimal app structure in idiomatic Rust.
  - Exported the new API through the crate root and prelude (`src/textual_app.rs`, `src/lib.rs`).
- **Buttons example migration to trait-based app API**
  - Migrated `examples/buttons.rs` to the new `TextualApp` + `run_textual_app()` path, keeping snapshot behavior unchanged while removing runtime setup boilerplate.
  - Migrated `examples/buttons_advanced.rs` to app-level message handling (`TextualApp::on_message`), removing the custom wrapper widget used only to intercept `ButtonPressed`.
- **Optional snapshot integration for trait-based apps**
  - Added `run_textual_app_or_snapshot()` as an opt-in helper for examples/dev binaries; production apps can continue using `run_textual_app()` without snapshot wiring.
  - Added trait hooks `snapshot_css_path()` and `compose_for_snapshot()` with defaults, so examples can keep snapshot output aligned with runtime CSS without repeating boilerplate in `main`.
  - Updated button examples to use the new helper, reducing `main` to a minimal entry path.
- **Buttons parity alignment (`buttons.py`)**
  - Updated `examples/buttons.rs` so button press exits the app and prints the pressed button description to stdout (matching Python example behavior).
  - Kept runtime auto-focus semantics enabled and aligned startup behavior by making `VerticalScroll` focusable; this matches Python’s effective behavior where initial focus lands on the first scrollable container rather than the first button.
- **Scrollable aliases parity: visible scrollbars + focus semantics**
  - Added visible scrollbar rendering to `VerticalScroll` and `HorizontalScroll` (track/thumb sizing and position now mirror `ScrollView` fundamentals instead of scrolling invisibly).
  - `VerticalScroll` is now focusable and tracks focus state, aligning app startup focus behavior with Python when scroll containers are the first focus targets.
- **App-level quit guidance notifications (`Ctrl+C` parity baseline)**
  - Added `Action::HelpQuit` and default `Ctrl+C` binding so applications can show quit guidance as an inherited app behavior rather than per-demo logic.
  - Switched default quit key semantics to `Ctrl+Q` (configurable via existing `set_quit_keys` API), matching Textual-style defaults more closely.
  - Added app-level notification state and runtime toast composition rendered in the bottom-right using existing `Toast` widget styling, with timeout-based expiry and stacking.
  - Refined notification timing to real-time `Duration` (default 5s, matching Python `NOTIFICATION_TIMEOUT`) so toast lifetime no longer depends on render tick cadence.
  - Aligned toast chrome closer to Python defaults: severity left accent border, `max-width: 50%`, horizontal padding (`line-pad: 1`), and explicit vertical padding in the `Toast` widget render/layout path.
  - Adjusted help-toast quit shortcut text to use readable binding strings (`ctrl+q`) while keeping compact key displays (`^q`) in footer/key-hint UI.
  - Removed the hard runtime cap on concurrently displayable toasts; visible count now depends on viewport space and toast expiry, matching Python `ToastRack` behavior more closely.

### 2026-02-08 (batch 10)
- **Style composition fundamentals: transparent widgets inherit parent surface at render time**
  - Kept CSS semantics aligned with Textual by making `bg` non-inherited in style resolution (`src/style.rs`).
  - Fixed render-time segment composition so segments without explicit background are painted with the effective parent surface (or widget `bg` when set), preventing terminal/default background bleed for transparent children like `Static` headers (`src/css/selectors.rs`).
  - Added regression coverage asserting child backgrounds remain transparent at style-resolution level (`tests/style_inheritance.rs`).
- **Style-debug instrumentation: selector/rule provenance for any widget**
  - Generalized style debug logging beyond `VerticalScroll` and width-only traces.
  - Added `TEXTUAL_DEBUG_STYLE_FILTER` support (`type=`, `class=`, `id=`, `pseudo=` or label substring) to target specific widgets/components.
  - Style logs now include rule and resolved summaries with `fg`, `fg_auto`, `bg`, text attributes, opacity, tints, and layout-relevant style fields (`src/css/selectors.rs`).
- **Workspace/dev dependency alignment**
  - Added `[patch.crates-io] rich-rs = { path = "../rich-rs" }` for local development parity and updated lockfile to the local `rich-rs` + dependency refresh (`Cargo.toml`, `Cargo.lock`).

### 2026-02-07 (batch 9)
- **Resize/corruption fundamentals: absolute diff cursoring + hardened redraw path**
  - Switched framebuffer diff emission to absolute cursor positioning (`MoveTo`) instead of relative cursor movement (`CursorDown`/`CarriageReturn`/`CursorForward`) to prevent drift/corruption during aggressive resize bursts (`src/render/mod.rs`).
  - Added runtime one-shot clear-on-resize handling and explicit runtime-mode reassertion around resize/render paths so terminals that reset modes during resize recover cleanly (`src/runtime/mod.rs`).
  - Added optional render-stream diagnostics (`TEXTUAL_DEBUG_RESIZE_TRACE`) including control-head and cursor/overflow stats to support deterministic resize debugging (`src/runtime/mod.rs`).
  - Added regression tests for absolute cursor diff behavior and clear-prepend behavior (`src/render/mod.rs`, `src/runtime/mod.rs` tests).

### 2026-02-07 (batch 8)
- **Buttons demo split: parity demo + advanced event-propagation demo**
  - Converted `examples/buttons.rs` into a clean Python-parity buttons layout demo (no embedded status footer/event wiring).
  - Added `examples/buttons_advanced.rs` preserving the previous event/status behavior for propagation diagnostics.
- **Disabled styling fundamentals: widget-level opacity support**
  - Added first-class `opacity` support to the style model and CSS parser (`src/style.rs`, `src/css/selectors.rs`).
  - Applied widget opacity after border composition in the render pipeline so disabled styling affects the whole widget surface (`src/widgets/core.rs`).
  - Added `Button:disabled { opacity: 70%; }` to align with Textual's disabled widget fade semantics (`src/css/defaults.rs`).
  - Added regression coverage for disabled button dim behavior and opacity composition (`tests/buttons_demo.rs`, `src/css/selectors.rs` tests).
- **Theme/token parity regression coverage**
  - Added `tests/theme_tokens.rs` validating key textual-dark token values used by button variants and semantic text colors.
- **Runtime resize recovery fix (dirty-loop compatibility)**
  - Ensured resize-invalidated frames trigger render under dirty-flag scheduling by honoring `resized_since_last_render` in render gates (`src/runtime/mod.rs`).

### 2026-02-07 (batch 7)
- **Style/color fundamentals: `auto` foreground semantics + `text-opacity` parity**
  - Added first-class auto-foreground semantics in the style engine (`fg: auto <percent>%`) and token-backed auto mappings for `$text`, `$text-muted`, `$text-disabled`, and `$button-color-foreground` (`src/style.rs`, `src/css/selectors.rs`).
  - Resolved `auto` foreground at render time against the effective composed background, matching Textual's contrast behavior instead of pre-baked hardcoded foreground colors (`src/css/selectors.rs`).
  - Added `text-opacity` CSS support (percent and float forms) and applied it during segment composition for both explicit and pre-existing foreground styles (`src/style.rs`, `src/css/selectors.rs`).
  - Corrected composition order so foreground color resolution happens after background tint/tint, ensuring contrast calculations use final background color (`src/css/selectors.rs`).
  - Aligned button disabled semantics with Python defaults: non-flat uses `text-opacity: 60%`, flat uses `fg: auto 50%` (`src/css/defaults.rs`).
  - Added regression tests for auto foreground parsing/resolution, tint-aware contrast behavior, text-opacity parsing/application, and style merge precedence between concrete and auto foregrounds (`src/css/selectors.rs`, `src/style.rs`).

### 2026-02-07 (batch 6)
- **Rendering/style composition fundamentals: transparent segment compositing + row bleed fix**
  - Aligned container defaults with Python Textual by removing opinionated default backgrounds from `VerticalScroll` and `ScrollView` (their defaults now focus on layout/overflow behavior, not paint) (`src/css/defaults.rs`).
  - Fixed framebuffer write composition so transparent segments no longer wipe inherited/default cell style; base theme background is preserved when writing unstyled spaces and transparent segments (`src/render/mod.rs`).
  - Fixed `Row` horizontal composition to avoid carrying the last child background into trailing viewport width (right-side color bleed/leak on wide terminals), using no-bg-safe width normalization (`src/widgets/layout.rs`).
  - Added regression coverage for both fundamentals (`tests/layout_transparency_regression.rs`).

### 2026-02-07 (batch 5)
- **Input-family chrome unification (Input + MaskedInput)**
  - Refactored `Input` and `MaskedInput` to share focus/mouse-active state, cursor blink timing, app-focus handling, and class toggling through `InputChrome` (`src/widgets/input.rs`, `src/widgets/masked_input.rs`, `src/widgets/input_chrome.rs`).
  - Added `MaskedInput` component CSS parity hooks for cursor, selection, and placeholder styling (`src/css/defaults.rs`).
  - Wired `input_chrome` module into widget exports for shared internal reuse (`src/widgets/mod.rs`).

### 2026-02-07 (batch 4)
- **ScrollView fill/background fundamentals + buttons demo parity fix**
  - Added default `ScrollView` background (`bg: $panel`) in built-in CSS so fill-area rows render with panel styling instead of terminal black (`src/css/defaults.rs`).
  - Fixed CSS style application for unstyled segments so widget `bg` / `fg` can still be applied to padded blank lines generated during layout (`src/css/selectors.rs`).
  - Hardened `VerticalScroll` intrinsic-height shaping to avoid truncating rendered content when effective rendered height exceeds reported intrinsic height (`src/widgets/aliases.rs`).
  - Added shared input chrome scaffold module (`src/widgets/input_chrome.rs`) to centralize cursor-blink/focus/class behavior for input-family widgets.
  - Result: in `examples/buttons.rs`, the fill area between buttons and footer is now painted correctly, and scrollbar visibility remains tied to actual overflow.

### 2026-02-07 (batch 3)
- **Port 4 more widgets from Python Textual** (LoadingIndicator, Sparkline, Digits, MaskedInput)
  - Added `LoadingIndicator` widget (`src/widgets/loading_indicator.rs`) — animated cycling gradient dots (5 `●` chars), blocks input events during capture phase, tick-driven animation. 6 unit tests.
  - Added `Sparkline` widget (`src/widgets/sparkline.rs`) — bar chart from numerical data using `▁▂▃▄▅▆▇█` bars, data bucketing with configurable summary function, color gradient between min/max via component classes (`sparkline--min-color`, `sparkline--max-color`). 12 unit tests.
  - Added `Digits` widget (`src/widgets/digits.rs`) — 3×3 Unicode block font for numerical displays, supports digits, hex, operators, currency symbols; auto-selects bold/normal glyph table from CSS.
  - Added `MaskedInput` widget (`src/widgets/masked_input.rs`) — template-based formatted input with character-level validation (alpha, digit, hex, binary, etc.), auto-inserted separators, cursor navigation skipping separators, case forcing (`>`/`<`/`!`), custom blank placeholder via `;`. Reuses `InputChanged`/`InputSubmitted` messages. 23 unit tests.
  - All widgets are first-class: segment-based rendering, CSS component styles, `style_type()`, default CSS rules in `defaults.rs`, full Widget trait, proper event handling.

### 2026-02-07 (batch 2)
- **Port 6 more widgets from Python Textual** (SelectionList, ProgressBar, Collapsible, ContentSwitcher, Link, Toast)
  - Added `SelectionList` widget (`src/widgets/selection_list.rs`) — multi-select checklist wrapping OptionList with per-item toggle checkboxes (`▐X▌`/`▐ ▌`), keyboard/mouse toggling, select/deselect all, emits `SelectionListToggled`/`SelectionListSelectedChanged` messages. 5 unit tests.
  - Added `ProgressBar` widget (`src/widgets/progress_bar.rs`) — determinate/indeterminate progress bar with component classes (`bar--bar`, `bar--complete`, `bar--indeterminate`), bounce animation for indeterminate mode. 9 unit tests.
  - Added `Collapsible` widget (`src/widgets/collapsible.rs`) — expand/collapse container with clickable title bar (▶/▼), keyboard/mouse toggle, full child rendering pipeline, emits `CollapsibleToggled` message.
  - Added `ContentSwitcher` widget (`src/widgets/content_switcher.rs`) — shows one child at a time matched by `style_id()`, delegates lifecycle events to visible child only.
  - Added `Link` widget (`src/widgets/link.rs`) — clickable text opening URLs, activates on click/Enter/Space, emits `LinkClicked` message.
  - Added `Toast` widget (`src/widgets/toast.rs`) — notification with severity levels (Information/Warning/Error), tick-based auto-dismiss timeout, click to dismiss, emits `ToastDismissed` message.
  - All widgets are first-class: segment-based rendering, CSS component styles, `style_type()`, default CSS rules in `defaults.rs`, full Widget trait, proper event handling.

### 2026-02-07
- **Port 7 new widgets from Python Textual**
  - Added `Rule` widget (`src/widgets/rule.rs`) — horizontal/vertical separator with 9 line styles (solid, dashed, double, heavy, thick, ascii, blank, hidden, none).
  - Added `Switch` widget (`src/widgets/switch.rs`) — boolean toggle with slider rendering, keyboard/mouse interaction, emits `SwitchChanged` message.
  - Added `Placeholder` widget (`src/widgets/placeholder.rs`) — layout placeholder with cycling variants (Default/Size/Text) and rotating background colors.
  - Added `RadioButton` widget (`src/widgets/radio_button.rs`) — radio button with circle glyphs (●/○), component styles, emits `RadioButtonChanged` message.
  - Added `RadioSet` widget (`src/widgets/radio_set.rs`) — mutual-exclusion container for radio buttons with keyboard navigation, emits `RadioSetChanged` message.
  - Added `OptionList` widget (`src/widgets/option_list.rs`) — scrollable option list with separators, disabled items, keyboard/mouse navigation, emits `OptionHighlighted`/`OptionSelected` messages.
  - Added `Select<T>` widget (`src/widgets/select.rs`) — generic dropdown select with overlay popup, emits `SelectChanged` message.
  - All widgets are first-class: segment-based rendering, CSS component styles, `style_type()`, default CSS rules, full Widget trait, proper event handling.
  - Added porting guidelines document (`docs/devel/WIDGETS_LEFT_TO_PORT.md`).

- **Phase 9.6 fundamentals: tabbed parity + command palette + markdown heading hooks**
  - Added first-pass `CommandPalette` widget (`src/widgets/command_palette.rs`) and integrated it into the `tabbed_content` demo via framework composition (`examples/tabbed_content.rs`), with open/close, search/filter, selection, and execute/dismiss flow.
  - Added runtime priority action routing so `Ctrl+P` is handled as a high-priority action before raw key dispatch, plus default `Ctrl+P -> Action::CommandPalette` mapping (`src/runtime/mod.rs`), preventing focused input widgets from swallowing command-palette activation.
  - Extended binding/footer pipeline for command-palette hint placement (`^p palette`) using structured `BindingHint` metadata (`show`, grouping, display, priority/system), and kept footer rendering driven by `BindingsChanged`.
  - Added `TabbedContent` + `TabPane` first-class widget fundamentals and examples (`src/widgets/tabbed_content.rs`, `examples/tabbed_content.rs`, `examples/tabbed_content_label_color.rs`), including component-id selector support for `#--content-tab-<id>`.
  - Added markdown heading component-style hooks (`markdown--h1` ... `markdown--h6`) at widget level with default CSS parity tokens (`src/widgets/text.rs`, `src/css/defaults.rs`) so heading styling is framework-driven rather than demo CSS.
  - Added regression coverage for this slice: tabbed behavior tests, footer/binding tests, command-palette lifecycle tests, command-palette open/closed snapshots, and markdown heading style assertion (`tests/tabbed_content.rs`, `tests/header_footer.rs`, `tests/command_palette_snapshot.rs`, `tests/markdown.rs`).

### 2026-02-06
- **Keys preview parity + reusable widget foundations**
  - Added reusable widgets for developer previews and app chrome: `Header`, `Footer`, `RichLog`, `KeyPanel`, and `BindingsTable` (`src/widgets/header.rs`, `src/widgets/footer.rs`, `src/widgets/rich_log.rs`, `src/widgets/key_panel.rs`), with public exports in `src/widgets/mod.rs` and `src/lib.rs`.
  - Added default CSS coverage for the new widgets (`src/css/defaults.rs`) and new scrollbar theme tokens (`scrollbar*`) in `src/style.rs`.
  - Refined `examples/keys.rs` to match Python Textual keys preview behavior/structure and moved demo styling to `examples/keys.tcss`.
  - Improved `KeyPanel` / `BindingsTable` fundamentals: styled table component rendering, corrected intrinsic height math, and full vertical scrollbar interactions (wheel, actions, track click, drag).
- **Input/event/runtime fundamentals for diagnostics tooling**
  - Added `Event::BindingsChanged(Vec<BindingHint>)` and runtime binding-hint aggregation from `ActionMap` + quit keys, with incremental dispatch when hints change.
  - Extended `Action` with human-readable descriptions and `ActionMap::entries()` to support bindings UIs.
  - Added `EventCtx::request_stop()` and stop propagation through dispatch/message queues to support message-driven app shutdown paths.
  - Added configurable quit key APIs (`set_quit_keys`, `clear_quit_keys`) and corresponding runtime tests.
- **Scrolling + scrollbar behavior parity**
  - Upgraded `ScrollView` and `RichLog` scrollbars with proper thumb sizing/positioning, themed track/thumb styles, track-click paging, drag interactions, and clamp behavior improvements.
  - Fixed `Dock` fill rendering order to resolve layout inconsistencies when mixing fill and side/top/bottom regions.
- **Tests and docs**
  - Added widget behavior tests for new components: `tests/header_footer.rs`, `tests/key_panel.rs`, `tests/rich_log.rs`.
  - Updated `ROADMAP.md` Phase 9.5 status to reflect completed visual parity pass and current pending fundamentals.
  - Expanded `tests/key_panel.rs` coverage with sizing, non-overflow action handling, and scrollbar drag behavior checks.
  - Added preview scaffold tests (`tests/preview_root.rs`) and snapshot coverage (`tests/preview_root_snapshot.rs`).
- **Preview scaffold fundamentals**
  - Added reusable preview composition helpers: `preview_root`, `preview_root_with_bottom`, and `preview_root_with_top_bottom` (`src/widgets/preview.rs`).
  - Migrated `examples/keys.rs` and `examples/data_table.rs` to the shared preview scaffold composition path.
- **Phase 9.5 styling + regression completion**
  - Added component-style resolver primitives in the CSS engine (`selector_meta_component_for`, `resolve_component_style`) and wired `Header` + `KeyPanel`/`BindingsTable` to use CSS-driven component styles.
  - Added keys parity snapshot baseline (`tests/keys_preview_snapshot.rs`) and updated `examples/keys.tcss` to style header components through component selectors.
- **Widget uplift: Checkbox/ListView/Tree → first-class**
  - Upgraded `Checkbox` with mouse press/release activation semantics (click-cancel), hover/active/disabled state handling, improved rendering (`☐`/`☑`), and preserved message emission via `CheckboxChanged`.
  - Reworked `ListView` with stable viewport state (`on_layout`), ensure-visible navigation, mouse row selection, hover tracking, wheel scrolling, and selection messages (`ListViewSelectionChanged`).
  - Reworked `Tree` with flattened visible-index mapping, mouse branch-toggle hit testing, keyboard expand/collapse navigation parity, hover-aware row rendering, and emitted messages (`TreeNodeSelected`, `TreeNodeToggled`).
  - Added default CSS rules for `Checkbox`, `ListView`, and `Tree`, including component-level state styling for rows/items.
  - Expanded behavior tests for all three widgets (`tests/checkbox_widget.rs`, `tests/list_view.rs`, `tests/tree.rs`) and refreshed snapshots.
- **Widget + container fundamentals: Tabs/Text/Pretty/Spacer and wrappers**
  - Upgraded `Tabs` with header hit-testing and mouse activation, keyboard activation parity, focus/hover-aware component styling, child `on_layout`/`on_message` forwarding, and `TabActivated` message emission.
  - Added forwarding fundamentals to container wrappers (`Panel`, `Frame`) for `on_layout`, `on_message`, and (for `Frame`) mouse-scroll propagation.
  - Improved text-family and utility widgets: `Label`/`Markdown` intrinsic layout width-aware sizing, richer `Pretty` rendering with multiline fallback and CSS component styles, and `Spacer` intrinsic width hints.
  - Added/expanded tests for tabs and wrapper forwarding (`tests/tabs.rs`, `tests/container_wrappers.rs`, `tests/text_pretty_spacer.rs`).
- **Overlay + input/markdown first-class completion**
  - Upgraded `Overlay`/modal fundamentals with focus-trap event routing, `Esc` dismiss behavior, and message-driven visibility controls (`OverlaySetVisible`, `OverlayToggle`, `OverlayDismissRequested`, `OverlayVisibilityChanged`).
  - Added behavior coverage for overlay interaction semantics (`tests/overlay_widget.rs`), including event trapping and message/escape dismissal.
  - Strengthened `Input` and `Markdown` behavior coverage (message emission tests in `src/widgets/input.rs`; wrap-aware sizing test in `tests/text_pretty_spacer.rs`).
  - Updated `ROADMAP.md` widget status to mark `Input`, `Markdown`, and `Modal/overlay` as first-class.

### 2026-02-05
- **Phase 9.5: Input diagnostics + key model parity**
  - Added canonical key model (`src/keys/mod.rs`): `KeyEventData` wraps crossterm's `KeyEvent` via `Deref` and adds normalized key name, character, printability. Normalization follows Python Textual conventions (alphabetical modifier ordering, shift consumption rules, and non-shift modifier chords not printable).
  - Added key normalization helpers: `key_to_identifier()` (Python-identifier form), `format_key_display()` (human-friendly with Unicode arrows/caret notation), and lazy alias resolution (`tab`↔`ctrl+i`, `enter`↔`ctrl+m`, `escape`↔`ctrl+[`).
  - Added Kitty keyboard protocol support to `richtui-crossterm` driver: tri-state `KeyboardProtocol` enum (Off/Auto/On), terminal auto-detection heuristic (kitty, WezTerm, foot, ghostty), and `TEXTUAL_KEYBOARD_PROTOCOL` env var override for Auto mode.
  - Migrated `Event::Key` from `crossterm::event::KeyEvent` to `KeyEventData`. All widget code continues working via `Deref` (key.code, key.modifiers unchanged). `KeyBind::from_event` updated to accept `&KeyEventData`.
  - Added `examples/keys.rs` diagnostic harness: real-time display of key, mouse, focus, resize, and scroll events with both canonical and raw crossterm data. Similar to Python Textual's `textual keys` command.
  - Added 74 integration tests (`tests/key_diagnostics.rs`) covering round-trip normalization, alias correctness, display formatting, identifier conversion, Deref compatibility, edge cases (media/modifier/lock keys, control chars, key repeat/release), ActionMap integration, and comprehensive symbol roundtrip.
  - Documented terminal compatibility limits (tmux, screen, macOS Terminal, PuTTY, SSH) and Kitty protocol behavior in module docs.
  - New prelude exports: `KeyEventData`, `key_to_identifier`, `format_key_display`.
  - **Breaking:** `TextArea::on_key` callback now takes `KeyEventData` (uses `.clone()` instead of `Copy`).
  - Runtime now defaults shared driver keyboard protocol to `Auto`, so `TEXTUAL_KEYBOARD_PROTOCOL` env overrides and terminal capability detection are effective by default.

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
