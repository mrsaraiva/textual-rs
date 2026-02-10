# Widget Porting Plan (Source of Truth)

Last updated: 2026-02-10
Status: Active

## Purpose

This document is the canonical plan for widget parity work between `textual-rs` and Python Textual.

Goals:
- Port missing widgets.
- Refactor existing widgets implemented with local shortcuts into shared framework abstractions.
- Align behavior and styling with Python Textual where semantics are framework-level, while keeping Rust-idiomatic design.

## Global Rules

- No demo-specific hacks for framework behavior.
- Prefer shared abstractions over per-widget implementations.
- Widget rendering should be content-oriented; borders/chrome/composition should be handled by shared style/render pipeline where possible.
- Python Textual is the behavioral reference, not a line-by-line implementation constraint.

## First-Class Definition

A widget is first-class only when all are true:
- Full `Widget` trait behavior (render/events/focus/layout/integration).
- Default CSS parity with Python `DEFAULT_CSS` semantics.
- Component/state classes are modeled via selectors rather than ad-hoc branches.
- Messages/events are framework-level and reusable by other widgets/apps.
- No widget-local bypass for concerns already handled by core (style application, scrolling primitives, overlay/modal composition, etc.).
- Regression tests cover core behavior and parity-sensitive interactions.

## Core Abstraction Backlog (Priority)

These fundamentals unblock multiple widgets and should land in small, behavior-focused commits.

### P0 - Scrolling model unification

Current issue:
- Scrolling responsibilities are still split across `ScrollView`, aliases, and widget-local logic.

Targets:
- Establish two reusable primitives:
  - viewport scroll container primitive (child clipping + scrollbar semantics),
  - line-API scroll primitive for log/table/editor-like widgets.
- Rebase `VerticalScroll`, `HorizontalScroll`, `RichLog`, and list/tree/table families on these primitives.

Relevant files:
- `src/widgets/containers/scroll_view.rs`
- `src/widgets/aliases.rs`
- `src/widgets/rich_log.rs`
- `src/widgets/data_table.rs`
- `src/widgets/list_view.rs`
- `src/widgets/tree.rs`
- `src/widgets/key_panel.rs`

### P0 - Shared text editing core completion

Current issue:
- Shared grapheme helpers exist, but `Input`, `MaskedInput`, and `TextArea` still diverge in editing/selection semantics.

Targets:
- Complete reusable text-editing core:
  - buffer model,
  - selection model,
  - cursor/edit command layer,
  - clipboard integration hook.
- Rebase all text-edit widgets onto this core.

Relevant files:
- `src/widgets/text_edit.rs`
- `src/widgets/input.rs`
- `src/widgets/masked_input.rs`
- `src/widgets/text_area.rs`
- `src/widgets/input_chrome.rs`

### P1 - Overlay/modal composition model

Current issue:
- Overlay flows still include widget-local or runtime-local composition patterns.

Targets:
- Establish reusable overlay/modal tree composition pattern used by:
  - command palette,
  - toast rack/holders,
  - tooltip/help overlays.

Relevant files:
- `src/widgets/containers/overlay.rs`
- `src/widgets/command_palette.rs`
- `src/widgets/toast.rs`
- `src/runtime/render.rs`

### P1 - Toggle/list option abstraction

Current issue:
- Checkbox/Radio/SelectionList/OptionList/Select still duplicate option/toggle behavior.

Targets:
- Shared toggle-row and option-list model with:
  - typed option IDs,
  - disabled semantics,
  - highlighted vs selected separation,
  - consistent message semantics.

Relevant files:
- `src/widgets/checkbox.rs`
- `src/widgets/switch.rs`
- `src/widgets/radio_button.rs`
- `src/widgets/radio_set.rs`
- `src/widgets/option_list.rs`
- `src/widgets/selection_list.rs`
- `src/widgets/select.rs`

## Current Widget State Matrix

Status meaning in this matrix:
- `Done`: first-class in current scope.
- `Partial`: implemented and usable, but parity/fundamental gaps remain.
- `Todo`: missing or not yet ported.

### Tier A (highest impact)

- `DataTable` - `Partial`
  - Needs keyed rows/columns, fixed headers/columns, richer cursor modes, and unified scroll primitive parity.
- `Tabs` + `TabbedContent` - `Partial`
  - Needs stronger Tab/Pane model (disabled/hidden lifecycle and activation semantics).
- `RichLog` - `Partial`
  - Needs line-API scroll primitive migration and broader renderable/markup parity.
- `CommandPalette` - `Partial`
  - Provider lifecycle is landed; remaining work is reusable modal/overlay composition parity.

### Tier B

- `ListView` + `Tree` - `Partial`
  - Needs highlighted vs selected cleanup, disabled-navigation semantics, unified scroll behavior.
- `Input` + `MaskedInput` + `TextArea` - `Partial`
  - Grapheme safety improved; shared editing core completion + clipboard/selection parity still open.
- `Header` + `Footer` - `Partial`
  - Significant parity landed (binding lifecycle/footer grouping), but composition/interaction polish remains.

### Tier C / Utility

- `Button` - `Done`
- `Checkbox` - `Partial`
- `Switch` - `Partial`
- `RadioButton` / `RadioSet` - `Partial`
- `Select` / `OptionList` / `SelectionList` - `Partial`
- `ProgressBar`, `LoadingIndicator`, `Sparkline`, `Digits`, `Placeholder`, `Pretty`, `Rule`, `Link` - `Partial`
- `Toast` - `Partial`
- `KeyPanel` - `Partial`
- `Markdown` - `Partial` (implemented; no longer missing)

## Missing Widgets to Port

Python source root: `../textual/src/textual/widgets/`

- `Log` (`_log.py`) - medium; should reuse the line-API scroll primitive.
- `DirectoryTree` (`_directory_tree.py`) - hard; builds on `Tree` + async filesystem model.
- `Tooltip` (`_tooltip.py`) - depends on robust overlay/layer composition.
- `HelpPanel` (`_help_panel.py`) - depends on markdown + overlay behavior.
- `Welcome` (`_welcome.py`) - lower priority/demo-oriented.

Notes:
- `Markdown` is implemented in `textual-rs` and tracked as `Partial`, not missing.
- Missing container-family parity (from Python `containers.py`) should be tracked as container parity work, not widget absence in this list.

## Existing Widgets: Refactor/Parity Queue (Execution Order)

1. Scrolling primitives and migrations
   - Land shared scroll primitives, then migrate `RichLog`, `KeyPanel`, `ListView`, `Tree`, `DataTable`.

2. Text editing core completion
   - Rebase `Input`, `MaskedInput`, and `TextArea` on one editing command model.

3. Toggle/list abstraction
   - Rebase `Select`/`OptionList`/`SelectionList` + switch/radio family on shared option/toggle semantics.

4. Overlay/modal composition unification
   - Rebase `CommandPalette` and toast rack to reusable overlay tree composition.

5. Tier-A parity closure
   - Complete `DataTable` and `Tabs`/`TabbedContent` parity-specific gaps.

6. Missing widget ports
   - Prioritize `Log`, then `Tooltip`/`HelpPanel`, then `DirectoryTree`, then `Welcome`.

## Review Protocol for Any Widget

For each widget (ported or existing), review in this order:
1. Python widget implementation and `DEFAULT_CSS`.
2. Rust widget implementation + default CSS.
3. Classify gaps as:
   - missing core abstraction,
   - widget-local workaround that should move to core,
   - intentional Rust-idiomatic divergence (document rationale).
4. Define acceptance criteria before coding.

## Acceptance Criteria Template

A parity/refactor item is complete when:
- Behavior in target demos/tests matches Python intent.
- Styling is driven by default CSS/tokens/component classes, not hardcoded demo logic.
- Implementation uses shared abstractions where available.
- Text-heavy behavior is grapheme-safe where relevant (movement, selection, deletion, width/alignment).
- Regression tests/snapshots cover changed fundamentals.
- `CHANGELOG.md` is updated for user-facing/foundational changes.

## Cross-cutting Notes (2026-02-10)

- Command palette provider lifecycle is landed; remaining gap is reusable overlay/modal composition parity.
- Footer/binding lifecycle parity has advanced significantly; remaining work is polish and shared abstractions.
- Animation framework is in place; timer/task APIs are still separate roadmap work.
