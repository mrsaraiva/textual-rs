<!--
This project is intentionally split from rich-rs.
rich-rs focuses on Rich (rendering + renderables + Live/Progress).
textual-rs focuses on Textual (reactive TUI framework built on top of rich-rs).
-->

# textual-rs

A Rust implementation of [Textual](https://github.com/Textualize/textual): a **reactive TUI framework** (widget tree, styling, layout, events) for building rich terminal applications.

This repository assumes `rich-rs` exists as the underlying rendering engine (segments/styles, tables/panels, Live/Progress, etc.). `textual-rs` provides the **application framework** on top: widgets, layout, events, and reactivity.

## Goals

- Provide a **Textual-like developer experience** in Rust: reactive widgets, composable layouts, and high-quality terminal rendering.
- Build on `rich-rs` for rendering primitives and terminal controls.
- Prioritize **correctness and determinism** (screen diffing, stable layouts, predictable event routing).
- Support real applications (event loop, input handling, focus, scrolling, animations, async tasks).

## Non-goals (initially)

- 1:1 API compatibility with Python Textual (we may add a compatibility layer later).
- Replacing the Rust TUI ecosystem; `textual-rs` is intentionally opinionated and framework-oriented.
- A full “HTML/CSS in the terminal” clone on day one (we’ll grow the styling and layout systems iteratively).

## Design sketch (working model)

- **Driver**: terminal backend (input + output + raw mode) based on `crossterm`.
- **Renderer**: builds a frame (screen buffer) and applies a minimal diff to the terminal (uses `rich-rs` segments/styles/control codes).
- **App runtime**: event loop + scheduling (timers, animation ticks, background tasks).
- **Widget tree**: mount/unmount lifecycle, render, layout, focus, scrolling, event routing.
- **Styles**: CSS-ish rules → computed styles per widget (subset at first).

## Minimal layout support

`textual-rs` includes a minimal box model via the `Frame` widget: it wraps a single child
with padding and an optional border (Unicode box drawing). Basic layout primitives include
`Container` (vertical), `Row` (horizontal), `Dock` (top/bottom/left/right/fill), and `Grid`
(fixed rows/cols). `ScrollView` provides MVP vertical scrolling within a fixed height. Use
`Constrained` (or `LayoutConstraints`) to apply simple min/max sizing hints. Containers clip
children to the current viewport height (MVP clipping). This is an MVP layout
primitive, not a full styling system.

Current widget catalog (MVP): `Label`, `Button`, `Input`, `Checkbox`, `ListView`, `DataTable`, `Tree`, `Tabs`, `Markdown`, and `Overlay`.

## Styling MVP

Inline styling is available via `Style` and the `Styled` wrapper widget. This is a minimal, typed API
(`fg`, `bg`, `bold`, `dim`, `italic`, `underline`) to enable quick visual iteration before a full
selector/cascade system lands.

Selector styling (MVP) is available via `StyleSheet` with `Type`, `Id`, and `Class` selectors.
Use `Node` to attach ids/classes to existing widgets.

## Rich-rs integration contract

Textual (Python) uses Rich (Python) as a rendering + styling engine. `textual-rs` should treat `rich-rs` the same way.

- **No direct ANSI writes:** widgets should render via `rich-rs` types (`Text`, `Segments`, renderables) and let the backend handle ANSI/control emission.
- **Metadata for event routing:** attach interaction metadata to output via `StyleMeta.meta` using `MetaValue` (structured values), not ad-hoc strings.
  - Convention: handler keys use Textual-style names like `@click`, `@hover`, `@action`.
  - Example schema: `meta["@click"] = MetaValue::Tuple([MetaValue::Str("button_id"), ...])` or `MetaValue::Map({ "id": "...", ... })`.
- **Hyperlinks (OSC 8):** use `StyleMeta.link` (and optional `StyleMeta.link_id`) to generate terminal hyperlinks.
- **ID strategy (important):**
  - `rich-rs` maintains a **per-Console registry** for hyperlink ids. If `link_id` is missing, it generates a stable id for a given URL **within that Console**.
  - Textual-level identity (widgets, nodes, hit-testing) must be **separate** from OSC8 hyperlink ids.
  - If we later need deterministic ids across runs (e.g. for snapshots/serialization), add an explicit Textual-level deterministic id scheme (e.g. hash-based) rather than relying on OSC8 ids.

## Repository status

This is an early draft project skeleton: the initial docs and roadmap exist, but implementation will follow once the core milestones are agreed.

## Roadmap

See `ROADMAP.md`.

## Open questions (to decide early)

- Runtime: **Tokio** (chosen). We may revisit runtime-agnostic execution later if it becomes a strong requirement.
- Styling: adopt a small CSS subset immediately, or start with a typed style system and add CSS later?
- Layout: port Textual’s layout engine vs. implement a simplified initial layout with a migration path?
- Rendering: do we require a persistent `Screen` buffer + diff from day one? (recommended for correctness/performance)
- IDs: do we need deterministic (hash-based) ids for persisted state/snapshots, or are stable per-run ids sufficient?
