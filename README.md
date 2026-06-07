# textual-rs

[![Crates.io](https://img.shields.io/crates/v/textual.svg)](https://crates.io/crates/textual)
[![Documentation](https://docs.rs/textual/badge.svg)](https://docs.rs/textual)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A Rust port of [Textual](https://github.com/Textualize/textual) — a **reactive TUI framework** for building rich terminal applications with widgets, CSS styling, layout, and event-driven architecture.

Built on [`rich-rs`](https://crates.io/crates/rich-rs) for terminal rendering primitives and [`crossterm`](https://crates.io/crates/crossterm) for terminal I/O.

> **Attribution.** textual-rs is a derivative work: a Rust port of
> [Textual](https://github.com/Textualize/textual), created by Will McGugan and the
> [Textualize](https://www.textualize.io/) team. All credit for the original framework
> design, API, and concepts goes to them — this project exists only because of their work,
> and aims to bring that experience to Rust. Published on crates.io as the `textual` crate.

## Installing

```toml
[dependencies]
textual = "1.0.0-dev"
```

## Features

- **56 widgets** — buttons, inputs, text areas, data tables, trees, tabs, markdown viewer, select, checkboxes, progress bars, overlays, and more
- **108 CSS properties** — type/id/class/pseudo-class selectors, descendant/child combinators, nested `&` rules, cascade with specificity and `!important`, theme tokens
- **Full layout engine** — vertical, horizontal, grid, dock, and absolute positioning with box model (margin, border, padding), scrolling, min/max constraints, fractional/percentage/viewport units
- **Event system** — capture/bubble phases, focus management, keyboard bindings with action maps, mouse hit-testing, message bus
- **Reactive runtime** — Tokio-based event loop, reactive state with watchers, workers for background tasks, CSS transitions with easing functions
- **Hot-reloadable stylesheets** — external `.tcss` files with `App::watch_stylesheet()`
- **Deterministic rendering** — frame buffer with screen diffing, metadata-safe hit-testing across repaints
- **1,490 tests** — unit, integration, snapshot (via `insta`), and golden-file coverage
- **`unsafe` forbidden** — enforced by lint configuration

## Quick start

```bash
tools/run-doc-example.sh widgets buttons           # Interactive button demo
tools/run-doc-example.sh widgets hello             # Composed widget/layout showcase
tools/run-doc-example.sh widgets data_table        # Data table widget
tools/run-doc-example.sh widgets input             # Input fields
tools/run-doc-example.sh widgets text_area_example # Text editor
```

Widget-focused docs parity examples live in a dedicated crate:

```bash
cargo run --manifest-path docs/examples/widgets/Cargo.toml --example tabbed_content
tools/run-doc-example.sh widgets tabbed_content_label_color
tools/run-doc-example.sh guide/screens modal01
```

## Demo Source Mapping

- Python docs demos (`../textual/docs/examples/**`) map to our docs example lane (`docs/examples/**`).
  - Current crate-backed location: `docs/examples/widgets/examples/**` and `docs/examples/guide/**`.
- Python app demos (`../textual/examples/**`) map to app examples under `examples/**`.

## Widget catalog

**Interactive:** Button, Input, MaskedInput, TextArea, Checkbox, RadioSet, Switch, Select, OptionList, SelectionList, ListView, DataTable, Tree, DirectoryTree, Tabs, TabbedContent, Collapsible, CommandPalette, Link

**Display:** Label/Static, Text, Markdown, Pretty, Digits, ProgressBar, LoadingIndicator, Sparkline, RichLog, Log, Toast, Rule, Spacer, Placeholder, HelpPanel, KeyPanel

**Containers:** Container, ScrollView, Frame, Panel, Overlay, Constrained, Styled, Node

## CSS styling

Stylesheets use Textual's TCSS syntax with nested rules:

```css
Button {
    width: auto;
    min-width: 16;
    line-pad: 1;
    text-align: center;
    content-align: center middle;

    &.-style-flat {
        text-style: bold;
        color: auto 90%;
        background: $surface;
        border: block $surface;

        &:hover {
            background: $primary;
            border: block $primary;
        }
    }
}
```

Supported selectors: type, `#id`, `.class`, pseudo-classes (`:hover`, `:focus`, `:active`, `:disabled`, `:can-focus`, `:dark`, `:light`, `:even`, `:odd`, `:first-child`, `:last-child`, and more), descendant (` `), child (`>`), grouping (`,`), universal (`*`).

Theme tokens (`$primary`, `$surface`, `$error-darken-2`, etc.) resolve against the active theme and support lighten/darken/muted derivations.

## Layout

Five layout modes: **vertical**, **horizontal**, **grid**, **dock**, and **absolute**.

Size units: cells (`20`), auto, percentage (`50%`), fractions (`1fr`), viewport (`100vw`, `50vh`).

Box model with margin collapsing, border-box sizing (default, matching Python Textual), padding, and border. Constraints via `min-width`, `max-width`, `min-height`, `max-height`. Overflow handling with scrollbars (`overflow: auto | hidden | scroll`).

## Architecture

```
Widget tree → rich-rs Segments (with metadata) → FrameBuffer (2D grid) → frame diff → ANSI output
```

- **Event routing:** capture phase (root → focused) then bubble phase (focused → root)
- **Style resolution:** CSS cascade with specificity, inheritance, and `!important`
- **Rendering:** dirty-flag driven — widgets call `ctx.request_repaint()` to trigger re-render

## Build and test

```bash
cargo build                      # Build library
cargo test                       # Run all 1,490 tests
cargo clippy                     # Lint
cargo fmt                        # Format
```

## Debugging

Environment variables for targeted instrumentation:

```bash
TEXTUAL_DEBUG_STYLE_FILE=/tmp/style.log   # Log CSS resolution
TEXTUAL_DEBUG_LAYOUT_FILE=/tmp/layout.log # Log layout calculations
TEXTUAL_DEBUG_INPUT_FILE=/tmp/input.log   # Log input events
TEXTUAL_DEBUG_RENDER_FILE=/tmp/render.log # Log rendering
TEXTUAL_DEBUG_BORDER_FILE=/tmp/border.log # Log border painting
TEXTUAL_DEBUG_FOCUS=1                     # Log focus changes to stderr
```

Filters narrow output: `TEXTUAL_DEBUG_STYLE_FILTER='type=Button,class=error'`

## Python parity

Python Textual is the source of truth for behavior and default styling. The port aligns:

1. **Semantics first** — event/focus/message behavior, layout/box-model rules
2. **Defaults second** — all 16 widget default CSS files match Python Textual verbatim
3. **Visuals third** — render-time composition, border painting, opacity blending

Rust idioms are used where appropriate (ownership, type safety, modular boundaries) while preserving behavioral parity.

## Roadmap

See `ROADMAP.md` for detailed phase tracking. Current status:

- Phases 0–9.6: **Complete** (runtime, widgets, layout, styling, async, animations, debug tooling, input diagnostics, tabbed content parity)
- Phase 9.7: **Active** (core modularization)
- v0.2 execution: widget closure, invalidation improvements, broader test coverage

## License

MIT
