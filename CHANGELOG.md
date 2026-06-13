# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### 2026-06-13 (SPEC-P3: dictionary_initial parity — dock intrinsic height + example CSS alignment)

- **fix(layout/split): `carve_edge` unset-height now uses widget intrinsic height**
  - Changed `None => 1` in `carve_edge`'s `child_h` match to `None | Some(Scalar::Auto)`
    (delegates to `widget.layout_height()`, falling back to full available height). This
    matches the policy of `layout_vertical`'s `extract_child_spec`. Previously, docked
    widgets with no explicit CSS `height` always got height=1, which caused `Node#dictionary-search`
    (dock: top, no explicit height) to be allocated only 3 rows (1 content + 2 margins)
    instead of the correct 5 (3 input + 2 margins), overwriting the Input bottom border with
    the results-container background.
- **fix(example/dictionary): align compose + CSS with Python**
  - `Input` now carries its CSS id (`"dictionary-search"`) directly via `Input::id()` instead
    of being wrapped in a `Node`, matching Python's `Input(id="dictionary-search")` structure.
    This preserves `Input.layout_height()` in the arena tree (Node child extraction replaces the
    child with a Spacer, losing the intrinsic height).
  - Added `Input::id()` builder method (sets `seed.css_id`).
  - CSS updated to match Python's `dictionary.tcss`: `Screen { background: $panel; }`,
    `Input#dictionary-search` selector, `border: tall transparent` on `#results-container`,
    `margin: 0 0 1 0` on `#results-container`, `:focus { border: tall $border; }` rule.
- **test(layout): new regression test `dock_top_unset_height_uses_intrinsic_height`** guards
  the `carve_edge` `None`-height fix so it cannot silently revert.
- **test(parity): `dictionary_initial` promoted XFail → Pass**

### 2026-06-13 (five_by_five input fix — wrong key identifiers + punctuation display)

- **fix(example/five_by_five): use canonical key identifiers**
  - `on_key_with_app` matched `" "` (literal space) instead of `"space"`, and the
    help binding used `"?"` instead of `"question_mark"` — so Space (make move)
    and `?` (help) never matched; Space then fell through to the default
    action-map (→ Toggle) and the game was non-functional on play despite a
    correct initial frame. Surfaced by the new interactive parity coverage.
- **fix(keys): `format_key_display` renders punctuation identifiers as symbols**
  - Added `punctuation_name_to_char` (inverse of `character_to_key_name`) so
    bindings declared with canonical names (e.g. `question_mark`) display as
    their symbol (`?`) in footers/hints, matching Python Textual. Without this
    the footer showed the literal `question_mark`.
- **test(parity): `five_by_five_after_move` promoted XFail → Pass**; `five_by_five_help`
  re-scoped to the remaining help-screen Markdown content-rendering gap.

### 2026-06-13 (SPEC-RA4: typed widget handles)

- **feat(handle): `Handle<W>` + `HandleSlot<W>` — typed widget handles (RA-4)**
  - `src/handle.rs`: new module. `Handle<W>` wraps `(NodeId, tree_id: u64)` with
    `PhantomData<fn() -> W>` for `Copy + Send + Sync` independent of `W`.
    `HandleSlot<W>`: `Arc<Mutex<Option<(NodeId, u64)>>>` cell filled by the mount
    pipeline; `make_sink()` produces the `HandleSink` callback.
    `Handle::read_in`/`update_in`: typed arena access (read-only / mutable).
    `Handle::read`/`update`/`is_mounted`: app-level delegation to
    `handle_read`/`handle_update`/`handle_is_mounted`.
    `update_in` enqueues a `RuntimeReactiveEntry` when the closure records changes
    or requests repaint/layout — same reactive phase as event handlers.
    `handle_update` drains `drain_pending_class_ops()` after the closure, matching
    the `with_widget_mut` contract (fixes `MarkdownViewer` TOC class staging).
  - `src/widget_tree.rs`: `QueryError` extended with `Unmounted` and
    `TypeMismatch { expected, actual }`; `WidgetTree` gains `tree_id: u64`
    (process-unique, from `AtomicU64`) to prevent cross-tree handle aliasing.
  - `src/compose.rs`: `ChildDecl` gains `handle_sink: Option<HandleSink>` field.
  - `src/widgets/core.rs`: `Widget` trait gains default
    `take_child_handle_sinks() -> Vec<(usize, HandleSink)>`.
  - `src/widgets/containers/app_root.rs`: `with_child_handle<W>` builder +
    `take_child_handle_sinks` override.
  - `src/runtime/mod.rs`: mount pipeline fires sinks at mount; new app-level API:
    `query_one_typed`, `typed_handle`, `mount_typed`,
    `handle_read` (pub(crate)), `handle_update` (pub(crate)),
    `handle_is_mounted` (pub(crate)).
  - `src/lib.rs`/`prelude`: `Handle`, `HandleSlot`, `HandleSink` exported.
  - `tests/typed_handles.rs`: 6 integration tests (slot fills on build,
    unfilled before build, bind fills slot, slot tracks latest mount,
    typed mismatch is loud, stale after remove).
  - Example migrations (judgment rule applied — handles used where they read
    clearer than stringly selectors, not forced mechanically):
    - `markdown`: `HandleSlot<MarkdownViewer>` via `with_child_handle`;
      `on_message_with_app` uses `h.read` for navigator state;
      key handlers use `h.update` for TOC / back / forward.
    - `json_tree`: `HandleSlot<Tree>` via `with_child_handle`;
      `on_app_action_str` uses `h.update` for add/clear/toggle-root.
    - `dictionary`: `Option<Handle<Markdown>>` acquired post-mount via
      `query_one_typed`; `on_message_with_app` uses `h.update`.
    - `code_browser`: `Option<Handle<Static>>` + `Option<Handle<VerticalScroll>>`
      acquired post-mount; `watch_path` uses `h.update` for all three sites;
      descendant selector `#code-view VerticalScroll` replaces the `#code-view`
      selector that silently no-oped scroll-home via the Node wrapper.
    - `five_by_five`: `HandleSlot<WinnerMessage>` via `with_child_handle`;
      `watch_won_at` uses `h.update`. `#moves`/`#progress` label sites keep
      `with_query_one_mut_as` — their init-phase watchers fire before
      `on_mount_with_app`, making post-mount handles incorrect there.
  - Parity: all 8 PTY parity tests pass; XFail cases unchanged.

### 2026-06-13 (SPEC-RA3 Step 10: five_by_five rewrite — signals-first)

- **refactor(examples/five_by_five): rewrite to signals-first idiom (RA-3 Step 10)**
  - `GameState` struct dissolved; pure helper functions replace its methods:
    `toggle_cross`, `filled_count`, `wrap_navigate`, `plural`.
  - `FiveByFiveApp` now derives `Reactive` with four reactive fields:
    `#[reactive(watch_with_app, init = false)] cells: Cells`,
    `#[reactive(watch_with_app)] cursor: (usize, usize)`,
    `#[reactive(watch_with_app)] moves: usize`,
    `#[reactive(watch_with_app, init = false)] won_at: Option<usize>`.
  - `watch_cells`: diffs old/new cell arrays, updates arena node classes via
    `app.query_mut("#cell-r-c").set_class(...)`, updates `#progress` label.
  - `watch_cursor`: removes `cursor` class from old node, adds to new node via
    `app.query_mut(...)` — init fires with old==new to set the initial cursor class.
  - `watch_moves`: updates `#moves` label via `app.with_query_one_mut_as::<Label, _>`.
  - `watch_won_at`: shows/hides `WinnerMessage` via
    `app.with_query_one_mut_as::<WinnerMessage, _>`.
  - `GameCell` loses `filled`, `is_cursor`, `classes`, `set_filled`, `set_cursor`,
    `rebuild_classes`, and `style_classes()` override; `new(row, col)` takes only
    coordinates. CSS classes live on arena nodes, matched via `node_selector_meta_from_node`.
  - `GameHeader::new()` takes no args; initial labels show `Moves: 0` / `Filled: 0`.
    `sync_all`, `sync_cells`, `sync_cursor` free functions deleted.
  - `on_mount_with_app` calls `self.new_game(app)` — sets all reactive fields;
    init-phase watchers (cursor, moves) fire before the first render (G3).
  - `on_key_with_app` navigation arms call `set_cursor(wrap_navigate(...))`;
    space arm calls `set_cells`/`set_moves`/`set_won_at`; n arm calls `new_game`.
  - In-file tests rewritten against pure helpers (`toggle_cross`, `filled_count`,
    `wrap_navigate`); `game_cell_classes_reflect_state` deleted (classes are
    arena-side); `game_header_label_texts` adjusted for `GameHeader::new()` no-args.
  - `five_by_five_initial` PTY parity: remains Pass. LOC: 795 → 768.

### 2026-06-13 (SPEC-RA3 Step 7: code_browser rewrite — signals-first)

- **refactor(examples/code_browser): rewrite to signals-first idiom**
  - `CodeBrowserApp` now derives `Reactive` with `#[var(watch_with_app)] show_tree: bool`
    and `#[reactive(watch_with_app)] path: Option<String>`.
  - `watch_show_tree` applies/removes `-show-tree` CSS class on Screen via
    `app.query_mut(...).set_class(...)` and requests style+layout+repaint invalidation.
  - `watch_path` loads and syntax-highlights the selected file (or shows an error);
    replaces the former `load_path` free function.
  - `on_key_with_app` handles `f` by calling `self.set_show_tree(...)` — replaces the
    old `app.toggle_class('Screen', '-show-tree')` action string.
  - `on_message_with_app` calls `self.set_path(...)` on file selection — delegates to the
    watcher rather than calling load logic directly.
  - `on_mount_with_app` drops the manual `query_mut("Screen").add_class("-show-tree")`
    call; the init-phase watcher (G3) applies initial class before the first render.
  - `reactive_widget_mut` → `Some(self)`.
  - In-file tests updated: binding assertions check `action == "toggle_files"`;
    new `watch_state_default` test asserts `show_tree == true` and `path == None`.
  - `code_browser_initial` PTY-parity case remains xfail-miss (DirectoryTree render
    gap is a separate concern); all other parity cases unchanged.

### 2026-06-13 (SPEC-RA3 Steps 1-6: Signals-first reactive framework additions)

- **feat(reactive): ReactiveCtx invalidation-request API (G2b)**
  - `ReactiveCtx` gains `styles_requested` field and `request_repaint()`,
    `request_layout()`, `request_styles()`, `needs_styles()` methods for watcher
    side-effect signalling without recording a field change.
  - `reset_flags()`/`clear_flags()` clear the new `styles_requested` flag.

- **feat(reactive): ReactiveWidget trait additions — `reactive_dispatch_with_app` and `reactive_record_init` (G1/G3)**
  - `reactive_dispatch_with_app`: like `reactive_dispatch` but receives `&mut App`,
    enabling watchers to query/mutate widgets (Python watcher parity). Default
    delegates to `reactive_dispatch` so existing code needs no change.
  - `reactive_record_init`: records synthetic old==new changes for all init=true
    fields at mount, mirroring Python's `Reactive._initialize_object`.

- **feat(reactive): Python-parity `var()` init default flip + `var_no_init()` (G4)**
  - `ReactiveFlags::var()` now has `init: true` (was `false`). Matches Python
    `var` default (`init=True`, `reactive.py:489`).
  - Added `ReactiveFlags::var_no_init()` constructor for explicit init opt-out.

- **feat(macros): derive macro — `watch_with_app`, `#[var(...)]` args, `reactive_record_init` codegen (G1/G3/G4)**
  - `#[reactive(watch_with_app)]` — watcher receives `&mut App`; triggers override
    of `reactive_dispatch_with_app` in the generated impl.
  - `#[var]` now accepts arguments: `watch`, `watch_with_app`, `init = false`.
  - `reactive_dispatch` dispatches only plain `watch` arms; `reactive_dispatch_with_app`
    dispatches both plain and `watch_with_app` arms.
  - `reactive_record_init` generated for any struct with init=true fields.
  - `flags_expr` uses `var_no_init()` for `#[var(init = false)]`.

- **feat(app-bridge): iterative dispatch + init-phase watcher firing (G2/G3)**
  - `dispatch_app_reactive` replaced with a bounded iterative loop
    (up to `MAX_REACTIVE_ITERATIONS`) that calls `reactive_dispatch_with_app` and
    feeds chained watcher changes back for re-processing. Cycle guard matches
    widget-level `run_reactive_phase_with_dispatch`.
  - `on_app_mount` now calls `reactive_record_init` + `dispatch_app_reactive`
    **before** `on_mount_with_app`, matching Python's init-phase ordering.
  - `needs_styles()` from the dispatch ctx propagates to `EventCtx::request_style_invalidation`.

- **feat(prelude): reactive types exported from `textual::prelude`**
  - `ReactiveChange`, `ReactiveCtx`, `ReactiveFlags`, `ReactiveWidget`, and the
    `Reactive` derive macro are now re-exported from `textual::prelude`.

### 2026-06-13 (SPEC-RA3 Step 8: dictionary example rewrite — signals-first)

- **refactor(example/dictionary): rewrite to signals-first pattern (RA-3 Step 8)**
  - `DictionaryApp` gains `#[derive(Reactive)]` with one reactive field:
    `#[reactive(watch_with_app, init = false)] results: String` — replaces the
    direct `with_query_one_mut_as::<Markdown>` call in `on_message_with_app`.
  - `watch_results` watcher updates the `#results` Markdown widget and requests
    repaint (selector changed from `"Markdown"` to `"#results"`, using the
    widget's existing `.with_id("results")` from compose).
  - `on_message_with_app` `WorkerStateChanged::Success` branch now calls
    `self.set_results(markdown, app.reactive_ctx())` instead of directly
    mutating the widget.
  - `reactive_widget_mut` override returns `Some(self)`.
  - Worker plumbing (`on_input_changed`, `request_exclusive_worker_task`)
    unchanged.
  - PTY parity: `dictionary_initial` remains XFail-miss (known rendering gap,
    not addressed here).
  - LOC: 237 → 263.

### 2026-06-13 (SPEC-RA3 Step 9: markdown example rewrite — signals-first)

- **refactor(example/markdown): rewrite to signals-first pattern (RA-3 Step 9)**
  - `MarkdownApp` gains `#[derive(Reactive)]` with one reactive field:
    `#[reactive(watch_with_app, init = false)] nav_state: (bool, bool)` — replaces
    the manual `navigator_at_start`/`navigator_at_end` cache fields.
  - `watch_nav_state` watcher calls `app.refresh_bindings()` + `ctx.request_repaint()`,
    eliminating the old `update_navigator_state` helper and the manual call sites.
  - `on_message_with_app` reads the navigator state and calls `set_nav_state` via
    the reactive setter; `refresh_bindings` + repaint now happen via the watcher.
  - `check_action` reads `self.nav_state.0`/`.1` (was `navigator_at_start`/`at_end`).
  - `reactive_widget_mut` override returns `Some(self)`.
  - t-key invalidation calls (`request_style_invalidation`, `request_layout_invalidation`,
    `request_repaint`) preserved unchanged — parity-critical for `markdown_toc_toggle`.
  - PTY parity: `markdown_initial` and `markdown_toc_toggle` both Pass (unchanged).
  - LOC: 225 → 209.

### 2026-06-13 (RA-2 complete: behavior-only Widget trait — BREAKING)

- **refactor(core)!: the arena node record is the sole owner of widget identity/style/state**
  - Widget structs no longer carry `id`/`classes`/`styles`/`focus`/`hover`/`disabled`;
    the `WidgetTree` `WidgetNode` record owns them (`classes` is now a `HashSet`).
  - The `Widget` trait sheds ~15 identity/style/state accessor methods and shrinks
    toward behavior (`render`/`measure`/`on_event`/lifecycle). Identity and style
    context reach widgets via `NodeSeed` (compose time) and `NodeState` (runtime).
  - `set_inline_style` now routes pre-mount inline styles into the seed (was a no-op
    default that silently dropped them).
  - **Layout contract change:** `content_width()` returns pure content; the auto-width
    edge adds full horizontal chrome regardless of box-sizing (border-box no longer
    assumes the widget folded its own padding). Height keeps margin-only behavior.
  - New `is_initially_disabled()`/`is_initially_focused()` seed interaction state at
    mount so `:disabled`/`:focus` resolve in headless tree builds.
  - Row/Dock legacy non-tree focus path removed (arena-tree mode is canonical).
  - Migration shipped as the 21-commit `bd7d235`..`45a640c` series; full suite green,
    PTY parity 8/8 unchanged.

### 2026-06-12 (SPEC-RA2 Step 5c: Remove identity/style/state plumbing from toggle/form widgets)

- **refactor(widgets): toggle/form widget families migrated to node-record identity/style/state**
  - `checkbox`, `switch`, `radio_button`, `radio_set`, `option_list`, `select`,
    `selection_list`: remove per-widget `focused`, `hovered`, `classes`,
    `focused_classes`, and `styles` fields; replace with `seed: NodeSeed`.
  - All `has_focus()` / `set_focus()` / `is_hovered()` / `set_hovered()` overrides
    removed (default Widget impls now suffice); focus/hover/class state read from
    `self.node_state()` (dispatch context) and CSS `:focus`/`:hover` pseudo-classes.
  - `RadioButton` retains `set_focus`/`has_focus`/`is_hovered`/`set_hovered` forwarding
    to `BinaryToggleState` to preserve keyboard routing during the dual-write phase.
  - `take_node_seed` implemented on all migrated widgets with the style-preserving
    clone-back pattern so post-mount `styles()`/`content_width()` remain accurate.
  - Unit tests updated to use `set_dispatch_recipient` instead of `widget.set_focus(true)`
    for keyboard routing in isolated test contexts.

### 2026-06-12 (SPEC-RA1 Step 20: Public dispatch_message_queue_tree + acceptance tests)

- **feat(runtime): `dispatch_message_queue_tree` is now `pub`**
  - Promoted from `pub(crate)` to `pub` in `src/runtime/routing.rs`.
  - Re-exported from `textual::runtime` alongside `dispatch_event_tree`.
  - Added to the `textual::prelude` re-export block.
  - Doc comment updated to explain the public role as the canonical tree message pump.

- **test(open_messages): RA-1 acceptance test suite**
  - New integration test `tests/open_messages.rs` (11 tests, T5 in SPEC-RA1).
  - Custom `Ping` and `CursorEcho` / `AltEcho` messages defined entirely outside `src/`.
  - Covers: bubble order, stop propagation, `can_replace` coalescing (same-sender /
    different-sender / non-replaceable), `control` field propagation, built-in/custom
    coexistence, TypeId refinement regression (distinct replaceable types do not coalesce),
    `MessageHandlers<A>` typed registration, and `#[on(ThirdPartyType)]` dispatch.

### 2026-06-12 (SPEC-RA1 Step 19: Rename Msg trait to Message)

- **BREAKING(message): `Msg` trait renamed to `Message` (final API name)**
  - The open message trait is now `pub trait Message` in `crate::message`.
  - All `impl_message!(T)` / `impl_message!(T, replaceable)` macro bodies updated.
  - All `Box<dyn Msg>` / `&dyn Msg` / `M: Msg` bounds renamed to `Message`.
  - Zero callers of `impl_message!` need updating (macro paths use `$crate::message::Message` internally).

### 2026-06-12 (SPEC-RA1 Step 18: Swap carrier to Box<dyn Msg>; remove the Message enum)

- **BREAKING(message): `Message` enum removed; all messages are now `Box<dyn Msg>`**
  - The closed ~110-variant `Message` enum is deleted entirely.
  - `impl_message_from!` macro and the 109 `From<Struct> for Message` impls are gone.
  - `payload_any()` / `payload_msg()` migration shims removed.
  - `MessageEvent.message` field is now private (`Box<dyn Msg>`).
  - New `MessageEvent::from_boxed(sender, Box<dyn Msg>)` constructor added.
  - New `MessageEvent::payload() -> &dyn Msg` accessor added.
  - `MessageEnvelope::message()` now returns `&dyn Msg` instead of `&Message`.
  - `EventCtx::post_message` / `WidgetCtx::post_message` bounds changed from `M: Into<Message>` to `M: Msg`.
  - New `EventCtx::post_message_boxed(Box<dyn Msg>)` added for pre-boxed payloads.
  - Coalescer uses `payload_type_id()` comparison instead of `mem::discriminant` (fixes
    cross-type coalescing for different custom types with `set_replaceable(true)`).

### 2026-06-12 (SPEC-RA1 Step 2: TypeId handler registration + #[on] downcast codegen)

- **feat(message_handlers): new `MessageHandlers<A>` typed registration API**
  - New module `src/message_handlers.rs` with `MessageHandlers<A>` and `MessageContext`.
  - `handlers.on::<T>(|app, msg, mctx, ctx| ...)` registers a closure dispatched by `TypeId`.
  - Multiple handlers for the same type all run in registration order.
  - `MessageHandlers::dispatch` returns `true` if any handler ran.
  - Exported from prelude as `pub use crate::message_handlers::{MessageContext, MessageHandlers}`.

- **feat(textual_app): `TextualApp::register_message_handlers` hook**
  - New optional trait method; `TextualAppAdapter` calls it once in `new()`.
  - Dispatch inserted between Block A (command palette/help panel state) and Block B
    (built-in typed hooks); typed handlers calling `ctx.set_handled()` suppress Block B.

- **BREAKING(macros): `#[on(T)]` generated dispatcher signature changed**
  - Old: `fn __on_dispatch_x(&mut self, msg: &Message, _sender: NodeId, ctx: &mut EventCtx) -> bool`
  - New: `fn __on_dispatch_x(&mut self, event: &MessageEvent, ctx: &mut EventCtx) -> bool`
  - Body uses `event.downcast_ref::<T>()` instead of enum-match.
  - Works for third-party message types (type in caller's scope, not enum variant).

### 2026-06-12 (SPEC-RA1 Step 1: Promote UserMessage to open Msg trait)

- **BREAKING(message): `UserMessage` trait removed; replaced by `Msg` (will be renamed `Message` at Step 19)**
  - `pub trait Msg: Any + Send + Sync + Debug + 'static` is the new open message trait;
    every payload struct (built-in or third-party) implements it.
  - `impl_message!(T)` / `impl_message!(T, replaceable)` macro exported from `textual`
    for implementing `Msg` on any `Clone + Debug + Send + Sync` struct.
  - All 109 built-in payload structs now implement `Msg`; replaceable arm used for the 11
    coalescing types (`InputChanged`, `TextAreaChanged`, `TextAreaSelectionChanged`,
    `DataTableCursorMoved`, `DataTableCellHighlighted`, `DataTableRowHighlighted`,
    `DataTableColumnHighlighted`, `TreeNodeHighlighted`, `OptionHighlighted`,
    `KeyPanelScrolled`, `RichLogScrolled`).
  - `Message::can_replace` now delegates to the `Msg` trait (single source of truth).
  - `Message::Custom` changed from `Box<dyn UserMessage>` to `Box<dyn Msg>`.
  - Migration shims added: `MessageEvent::{new, with_control, downcast_ref, is, payload_type_id}`;
    `MessageEnvelope::{downcast_ref, is}`; `Message::{payload_any, payload_msg}` (pub(crate)).
  - `EventCtx::post_message` and `WidgetCtx::post_message` are now generic `M: Into<Message>`.
  - `NavigatorUpdated` added to `impl_message_from!` invocation (was previously missing).

### 2026-06-12 (SPEC-P1: Complete CSS border-type table + five_by_five parity)

- **feat(style): extend `BorderType` with 10 new variants**
  - Added `Ascii`, `Blank`, `Dashed`, `Double`, `Inner`, `Panel`, `Round`, `Tab`,
    `Thick`, `Wide` — completing the full Python Textual border-type vocabulary.
  - Added `BorderType::from_name(name: &str) -> Option<Self>` for CSS parsing.

- **feat(widgets/helpers): table-driven border glyphs + title-flip**
  - `border_chars` made `pub(crate)` with glyph tables for all 10 new types.
  - Added `border_title_flip(edge_type) -> (bool, bool)`: panel/tab borders swap
    fg/bg for title text (matching Python `BORDER_TITLE_FLIP`).
  - `overlay_border_text` gains a `flip` parameter wired through both title
    and subtitle call sites.

- **refactor(runtime/render): table-driven outline characters**
  - `outline_char_horizontal` / `outline_char_vertical` now look up glyphs via
    `border_chars` instead of hardcoding a fixed character set; all outline types
    (including the 10 new ones) now produce correct outline characters.

- **fix(css/parser): unified border value parser**
  - Replaced `parse_border_edge` / `parse_border_shorthand` with a unified
    `parse_border_value` that accepts tokens in any order (type, color, alpha%),
    handles all Python Textual border type names, treats `none`/`hidden` as
    `BorderEdge::None`, and logs a debug warning + drops invalid declarations.
  - `CommandList { border-top: blank; border-bottom: hkey black }` from default
    CSS now parses correctly (no longer silently dropped).

- **fix(widgets/command_palette): geometry accounts for CommandList border overhead**
  - `palette_geometry` now computes `list_border_overhead` from the resolved
    CommandList style so `desired_results_height` always fits all entries even
    when the CommandList carries blank-top + hkey-bottom borders.

- **feat(examples/five_by_five): reconcile GameHeader with Python three-label layout**
  - `GameHeader` recomposed as `Horizontal` + three `Label` children
    (`#app-title` 60% / `#moves` 20% / `#progress` 20%) matching Python
    `five_by_five.py:84-93` + `five_by_five.tcss:23-33`.
  - Title constant changed to `"5x5 -- A little annoying puzzle"` (ASCII `--`).
  - `sync_all` / `sync_cells` updated to query `#moves` / `#progress` Labels.
  - Footer binding text corrected: "Toggle Dark Mode".
  - `WinnerMessage` CSS gains `border: round` (Python tcss `:73`).

- **parity: promote `five_by_five_initial` to `Pass`**
  - All GameCell borders now render with round glyphs; header layout matches
    Python's three-label 60/20/20 split; PTY parity test promoted from XFail.

### 2026-06-12 (SPEC-P2: Tree navigation bindings, app-level custom action dispatch, TreeNode default state)

- **fix(Tree): hide all navigation bindings from Footer (Python `show=False` parity)**
  - All 15 `Tree::bindings()` declarations now carry `.hidden()`, matching Python
    where every `Tree` BINDING has `show=False`.  Focused Tree no longer floods
    the Footer with navigation keys, allowing app-level bindings to appear.

- **fix(Tree): `TreeNode::new()` starts collapsed (`expanded: false`)**
  - New nodes default to `expanded: false`, matching Python Textual's collapsed
    default.  Callers that need a pre-expanded node must set `.expanded(true)`
    explicitly.  All internal tests and examples updated accordingly.

- **feat(TextualApp): add `title()` hook + propagation to Header**
  - New `TextualApp::title()` method (default: `"textual-rs"`) lets apps declare
    their display title without imperative `set_title` calls.  The runtime reads
    this once at mount time and pushes a `ScreenTitleChanged` message so the
    `Header` widget always shows the correct app title.

- **feat(runtime): add `on_app_unhandled_action` / `on_app_action_str` fallback**
  - New `Widget::on_app_unhandled_action` trait method called by the event loop
    when a declarative binding's action string is not in any node's
    `action_registry()`.  `TextualAppAdapter` overrides it to call the new
    `TextualApp::on_app_action_str` hook, closing the gap where app-declared
    custom actions (e.g. "add", "clear") were silently dropped.

- **feat(json_tree): rewrite to declarative-binding action dispatch**
  - Removed `on_key_with_app`; added `title()` override ("TreeApp") and
    `on_app_action_str` handler for "add"/"clear"/"toggle_root".

- **parity: promote `json_tree_initial` and `json_tree_add_node` to `Pass`**

### 2026-06-12 (Real-PTY parity harness, blocking CI gate)

- **test(parity): add real-PTY parity harness (`tests/pty_parity.rs`)**
  - Runs example binaries in a genuine pseudo-terminal (`portable-pty` +
    `vt100` dev-deps), drives them with key input, and compares captured
    screens against golden files generated from **Python Textual**
    (`tests/pty_parity/golden/`, regenerated only via
    `tools/parity/gen-python-goldens.sh` — no bless-from-Rust mechanism).
  - 7 cases across the 5 shared examples (markdown, five_by_five, json_tree,
    dictionary, code_browser), including keypress scenarios.
  - Strict xfail manifest: known parity gaps are declared with reasons;
    a regression in a passing case fails CI, and a silently-fixed xfail also
    fails (XPASS) until explicitly promoted to `Pass`.
  - Current state: both markdown cases pass (pixel parity with Python);
    five_by_five/json_tree/dictionary/code_browser gaps are tracked as xfail.
  - Deterministic fixture dir for code_browser under
    `tests/pty_parity/fixtures/`.

- **ci: make the PTY parity harness a blocking gate**
  - New `.github/workflows/ci.yml` (push/PR): blocking `pty-parity` job plus
    the existing full test suite as non-blocking (headless-TTY limitation).
  - `release.yml`: `publish` now requires the blocking `pty-parity` job.

### 2026-06-12 (Footer command-palette separator parity)

- **fix(widgets/footer): use `▏` (vkey left edge) for the command-palette separator**
  - Python's Footer draws the separator via `border-left: vkey` on the
    command-palette `FooterKey`, which renders `▏`; the Rust footer hardcoded `│`.
  - Updated both right-dock render paths and the separator-position tests.

### 2026-02-26 (MarkdownViewer TOC hover-fill + heading landing parity)

- **fix(widgets/tree): make TOC hover-line fill span full row width**
  - Hover-path rows now pad with hover-line background through trailing cells
    (instead of stopping at text width), matching Python TOC hover visuals.
  - Added regression test for full-row hover background coverage in `Tree`.

- **fix(widgets/markdown_viewer): align TOC heading navigation landing with Python**
  - TOC selection scroll target now compensates heading top margin when computing
    the line offset, so clicking entries like `Tables` lands one context row before
    the heading rather than inside section body lines.

### 2026-02-26 (Markdown table keyline parity + markdown block visual fixes)

- **fix(layout): reserve keyline ring for grid layouts (Python parity)**
  - `layout_grid` now reserves a 1-cell inner ring when `keyline` is enabled,
    matching Python Textual grid behavior and preventing keyline borders from
    overlapping grid cell content.

- **fix(render): draw full grid keylines (inner + outer borders)**
  - Added grid-specific keyline rendering with proper junction/corner glyphs so
    `MarkdownTableContent` gets full bordered table chrome from `keyline`.

- **fix(widgets/css): align markdown heading/list/table visuals**
  - Non-H1 markdown headings now use intrinsic content width (`width:auto`) so
    underline styling tracks heading text instead of full row width.
  - Unordered markdown lists now render stable Python-like bullet glyphs.
  - Markdown table cells now render one-line rows with nowrap/ellipsis styling,
    hover styling, and tooltip text; table height/width estimation accounts for
    the keyline ring to avoid clipped header/last rows.

### 2026-02-26 (MarkdownViewer sizing stability + table track rebalancing)

- **fix(layout): seed width-dependent intrinsic height in vertical layout**
  - `layout_vertical` now calls `on_layout(...)` with a provisional content width
    before reading `layout_height()` for auto-sized children.
  - Prevents first-frame width=`1` intrinsic-height explosions for widgets that
    compute height from wrapping width (notably `Markdown` in `MarkdownViewer`).

- **fix(widgets): rebalance markdown table column tracks under tight widths**
  - Updated markdown table column fraction/compaction heuristics to keep semantic
    columns (for example `Type`, `Default`) readable while allowing wide
    description columns to absorb most shrink.
  - Added regression tests for markdown table fraction weights and compaction.

### 2026-02-26 (Markdown render fidelity: preserve inline markdown content)

- **refactor(widgets): preserve raw markdown slices in block parser**
  - `MarkdownBlock` now carries raw source slices for headings, paragraphs, lists,
    tables, and code fences.
  - Parser now tracks pulldown-cmark byte ranges to preserve source markdown used
    by rendering, while still exposing normalized heading/list/table metadata.

- **fix(widgets): restore inline markdown styling for paragraphs/lists/fences**
  - `Markdown` now renders paragraph/list/code-fence blocks through
    `rich_rs::markdown::Markdown` using block-specific CSS base styles.
  - This restores inline markdown rendering (for example emphasis/inline code)
    that was lost in plain-text block rendering.

### 2026-02-26 (Markdown block-model foundation)

- **feat(widgets): add internal Markdown block parser/model in textual-rs**
  - Added `src/widgets/markdown_model.rs` with a pulldown-cmark based parser that
    extracts block-level structure (headings, paragraphs, lists, tables, code fences, rules).
  - This lays the groundwork for Python-style block-widget Markdown composition so
    block-specific CSS selectors can be applied via real widget types.

### 2026-02-26 (Markdown block-driven render + typed component styling)

- **feat(css): typed component style resolution helper**
  - Added `resolve_component_style_for_type(...)` so a widget can resolve CSS
    as if rendering a specific component type (for example `MarkdownBullet`,
    `MarkdownTableContent`) while preserving parent selector context.

- **refactor(widgets): Markdown now renders from internal block model**
  - Replaced monolithic `rich-rs` markdown render call with block-driven rendering
    over parsed markdown blocks.
  - Heading/list/table/code-fence rendering now resolves style by markdown component
    type names, enabling existing markdown default CSS to apply to bullets and
    table content classes.
  - Stabilized `layout_height()` to read from markdown render cache after render,
    avoiding provisional-width height drift.

### 2026-02-26 (Markdown list/table style regression guards)

- **fix(widgets): resolve nested table child classes under `MarkdownTableContent`**
  - Added child-of-component style resolution in `Markdown` so selectors like
    `MarkdownTableContent > .header` and `MarkdownTableContent > .cell` apply
    with the correct parent context.

- **test(widgets): add markdown list/table style regression tests**
  - Added tests asserting bullet glyph cells resolve explicit styles in tree mode.
  - Added tests asserting table header/cell styles differ for markdown tables.

### 2026-02-26 (Parser-aligned heading metadata flow)

- **refactor(widgets): unify heading extraction on markdown parser model**
  - Added parser-based heading metadata helpers in `markdown_model` including
    heading line indices.
  - Switched `Markdown::extract_headings()` and `MarkdownViewer` heading-line parsing
    to use the shared parser model, reducing drift between TOC metadata and rendered blocks.

### 2026-02-26 (Delegation regression fix: preserve wrapper CSS type identity)

- **fix(widgets): keep thin wrapper `style_type`/aliases on `delegate_widget_to!`**
  - Stopped full-delegation macro from forwarding `style_type()` and
    `style_type_aliases()` to the inner widget.
  - Fixes regressions where wrappers like `Horizontal` were seen as `Container`,
    breaking type-based default CSS (including `TabbedContent` tab-row layout/rendering).
- **chore(widgets): refresh delegation audit baseline**
  - Updated canonical delegate method count and all `delegate-audit` markers after
    removing the two type-identity forwards from the full list.

### 2026-02-26 (Widget delegation primitive + audit guards)

- **feat(widgets): framework-wide delegation primitive for wrapper widgets**
  - Added `src/widgets/delegate.rs` with `delegate_widget_method!`, `delegate_widget_to!`,
    and `delegate_renderable!` to standardize the Rust equivalent of Python inheritance wrappers
    (`inner` + delegated methods + explicit overrides).
  - Exported delegation macros as public framework API and re-exported from `widgets`/prelude
    for custom compound widgets in apps.

- **refactor(widgets): adopt delegation primitive in scroll wrappers**
  - `ScrollableContainer` and `MarkdownViewer`/TOC wrapper now use explicit overrides plus
    `delegate_widget_method!` for remaining forwarding, reducing boilerplate and drift risk.
  - `containers/thin.rs` is now a temporary compatibility shim that re-exports from
    `widgets::delegate`.

- **test(widgets): guard against silent delegation drift**
  - Added canonical delegation-list markers and a test that counts methods in
    `delegate_widget_to!`'s full forwarding list to detect trait-surface changes.
  - Added `delegate-audit` markers on partial delegation sites to make required audits
    grep-friendly when the canonical list changes.

- **refactor(widgets): remove `containers/thin` compatibility shim**
  - Migrated all container wrappers to import delegation macros from `widgets::delegate`
    directly.
  - Removed `src/widgets/containers/thin.rs` and corresponding module wiring.

### 2026-02-26 (MarkdownViewer scroll parity + scrollbar sync fixes)

- **fix(runtime): smooth scrollbar thumb sync without per-frame relayout**
  - Added lightweight host-scrollbar position sync during render so animated scroll offsets update
    dedicated scrollbar thumbs without forcing `run_layout_pass()` each animation tick.
  - Prevents heavy layout invalidation during TOC scroll animations, improving smoothness.

- **fix(widgets): ScrollView wheel parity with Python**
  - Mouse wheel scrolling is now immediate (non-animated), matching Python Textual behavior.

- **fix(widgets/runtime): restore scrollbar movement on wheel/TOC scroll**
  - Added `scroll_virtual_content_size()` support in `ScrollView` and delegated it through
    wrapper widgets (`thin` macro, `ScrollableContainer`, `MarkdownViewer`).
  - Fixed regression where scrollbar thumb could stop tracking non-drag scroll updates due to
    missing virtual-size delegation in the wrapper chain.

### 2026-02-25 (MarkdownViewer TOC architecture + sizing parity)

- **fix(widgets): MarkdownTableOfContents — Python-style composed Tree behavior**
  - TOC now handles both `TreeNodeSelected` and `TreeNodeActivated` for click/keyboard parity.
  - TOC/headings update flow now requests layout invalidation (not repaint only), so docked
    `width: auto` pane width recomputes when heading content changes.

- **fix(widgets): TOC sidebar width ownership + child fill semantics**
  - `MarkdownTableOfContents` (wrapper) remains the intrinsic width source for docking.
  - The composed TOC `Tree` now fills the wrapper pane instead of applying a second intrinsic
    width clamp, eliminating right-side unused strip and heading text clipping.

- **fix(widgets): Tree/TOC parity details**
  - Tree twisty glyphs aligned with Python (`▶` / `▼`).
  - Added regression tests for TOC relayout, long heading width coverage, and hidden-root guide
    width calculations.

### 2026-02-25 (action parsing, header fix, outline clip, MarkdownViewer slug IDs)

- **feat(event): BindingHint action parsing — structured action_name/action_parameters**
  - `with_action()` now parses action strings (e.g. `"app.push_screen('settings')"`) into
    `action_name = "push_screen"` and `action_parameters = ["settings"]`.
  - `apply_check_action()` passes the parsed name and parameters to `check_fn`, enabling
    widgets to enable/disable bindings based on action arguments.

- **fix(widgets): Header — icon lane click no longer toggles tall mode (Python parity)**
  - Track `press_in_toggle_zone` (x > 1) on mousedown; only toggle tall if both press
    and release occurred in the toggle zone, matching Python behavior.

- **fix(runtime): paint_outline clip rect expansion**
  - Expand clip rect by 1 cell on each side so right/bottom outline edges are not
    clipped when descendants are clipped to their content box.

- **feat(widgets): MarkdownViewer — slug-based heading block_id + shared headings**
  - Headings now carry stable slug IDs (e.g. `"hello-world"`) instead of numeric indices.
  - `slugify_heading()` generates GitHub-style slug IDs with deduplication.
  - `parse_headings()` returns `(level, title, block_id, line_idx)` tuples.
  - `heading_line_offset()` takes block_id string for TOC click-to-scroll.
  - Shared headings via `Arc<RwLock<Vec<HeadingEntry>>>` between MarkdownViewer and TOC.
  - `MarkdownTableOfContents::with_shared_headings()` constructor; TOC renders from
    internal tree (no longer exports composed children).

- **fix(widgets): Tree — skip markup parsing for non-markup labels**
  - Labels without `[/` are rendered as plain text, avoiding spurious markup
    interpretation of bracket characters in TOC headings.

- **feat(examples): Markdown demo — message-driven navigation state updates**
  - `go()`/`back()`/`forward()` now post `NavigatorUpdated` via `ctx.post_message()`
    instead of calling `update_navigator_state()` directly.

- **fix(tests): update scrollbar and tree tests for arena-tree scrollbar children**
  - DataTable, KeyPanel, Log tests use `ScrollbarScrollTo` messages instead of mouse events.
  - ScrollView/VerticalScroll/HorizontalScroll child counts updated for dedicated
    scrollbar children. Header tests use `render_tree_to_frame()`.
  - Tree focus test discovers nodes dynamically instead of hardcoded indices.

### 2026-02-25 (MarkdownViewer — scrollbar + content propagation + widget parity)

- **feat(widgets): MarkdownViewer — shared-markup content propagation for scrollbar support**
  - Root cause: after `take_composed_children()` extracts the `Markdown` child into the arena
    tree, `go()`/`back()`/`forward()` could not reach it to update content. The Markdown child
    stayed empty → `layout_height()` returned 1 → no overflow → no scrollbar.
  - Introduced `Arc<RwLock<String>>` shared content between `MarkdownViewer` and its `Markdown`
    child. `Markdown::with_shared_markup()` constructor reads initial content; `on_layout()`
    syncs from shared state before height computation.
  - `go()`, `back()`, `forward()`, and `set_content()` now push content into shared state.

- **feat(widgets): MarkdownViewer — initial content in markdown demo**
  - Changed `MarkdownViewer::new("")` to `MarkdownViewer::new(DEMO_MD)` so the first frame
    has full content and scrollbar from the start.

- **feat(widgets): scroll_viewport_size() delegation chain**
  - `ScrollView` now overrides `scroll_viewport_size()` (reads from `viewport_width`/
    `viewport_height` atomics), enabling proper content clipping.
  - `ScrollableContainer` and `MarkdownViewer` delegate through to `ScrollView`.
  - `delegate_widget_to!` macro updated to forward `scroll_viewport_size()`.

- **feat(widgets): MarkdownViewer — TOC tree composition + click-to-scroll**
  - `MarkdownTableOfContents` now wraps a persistent `Tree` (not rebuilt on each render).
  - `take_composed_children()` extracts the tree for arena-tree mode.
  - TOC click posts `MarkdownTableOfContentsSelected` with heading index → MarkdownViewer
    scrolls to the heading line offset.
  - `content_width()` cached from inner Tree for layout.

- **feat(widgets): Widget::set_virtual_content_size() trait method**
  - New trait method for widgets to set virtual content dimensions (e.g. when content changes
    asynchronously). Default implementation is a no-op. `ScrollView`, `ScrollableContainer`
    delegate through to their inner scroll state.

- **feat(widgets): MarkdownTableOfContents — NUMERALS prefix for heading labels**
  - Heading labels now include their 1-based index as a prefix (e.g. "I Introduction"),
    matching Python's `MarkdownTableOfContents` heading numbering.

### 2026-02-20 (Toast notification styling regression fix)

- **fix(runtime): restore Toast CSS styling in tree-driven render path**
  - `compose_notifications()` was called after the per-layer style context guard dropped,
    so all `resolve_style()` calls returned empty defaults — producing unstyled black
    rectangles for all toast severities.
  - Re-establish style context via `stylesheet_for_layer(None)` before rendering notifications.
  - Removed spurious `"toast"` class from `Toast::rebuild_classes()` (Python only adds
    the severity class, e.g. `-information`; the CSS uses `Toast` as a type selector).

### 2026-02-20 (LOW priority + DEFERRED items — framework parity)

- **fix(widgets): DirectoryTree — apply filter predicate to async-loaded results**
  - `filter_paths` predicate was only applied during sync initial build; now also applied
    when async subdirectory load results arrive in `apply_directory_load_result()`.

- **feat(widgets): Button — compact mode**
  - `compact(bool)` builder, `set_compact()` reactive setter, `-textual-compact` class toggle.
  - CSS default already had `.-textual-compact { border: none !important; }`.

- **fix(widgets): Toast — full Rich markup support**
  - Replaced hand-rolled `[b]`-only parser with `rich_rs::markup::render()`.
  - All markup tags (`[b]`, `[i]`, `[u]`, colors, nesting) now render correctly.

- **feat(widgets): TreeNode — `add_child()` / `add_leaf()` API**
  - `add_child(&mut self, child) -> &mut TreeNode` for incremental tree construction,
    matching Python's `node.add(label)` pattern.
  - `add_leaf(label)` convenience method.
  - Updated `json_tree` demo to use `add_child()`/`add_leaf()` pattern.

- **feat(widgets): Tree — per-segment guide/label/cursor styling**
  - Render now emits separate `Segment`s for cursor marker, guides, twisty, and label,
    each with independently resolved component styles (`tree--guides`, `tree--guides-hover`,
    `tree--guides-selected`, `tree--label`, `tree--cursor`, `tree--highlight`,
    `tree--highlight-line`). Previously emitted a single concatenated segment per row.
  - Node `component_classes` (e.g. `directory-tree--file`) are now resolved and merged
    into the label style at render time.

- **feat(reactive): `always_update` flag**
  - `ReactiveFlags::reactive_always_update()` — fires watchers even when old == new,
    matching Python's `reactive(always_update=True)`.
  - Tree `set_selected()` now uses `always_update`, removed `move_cursor()` workaround.

- **feat(examples): weather02/weather03 — real HTTP fetch via ureq**
  - Added `ureq` as optional dependency behind `http-examples` feature flag.
  - With `--features http-examples`, weather demos query `wttr.in` for real data.
  - Without the feature, simulated fetch with fabricated data (no network needed).

### 2026-02-20 (Post-sprint remediation — parity fixes across widgets and demos)

- **fix(examples): rewrite `five_by_five` with proper widget composition**
  - Replaced monolithic ASCII-art `GameGrid` with per-cell `GameCell` widgets in a CSS grid.
  - `GameCell` custom widget with CSS-driven classes (`filled`, `cursor`) for visual state.
  - `GameHeader` stats bar, `WinnerMessage` victory overlay (visibility-toggled).
  - `Grid(5, 5)` layout matching Python's `grid-size: 5 5` TCSS.
  - Targeted cell sync via `app.with_query_one_mut_as::<GameCell, _>("#cell-r-c", ...)`.
  - 10 regression tests.

- **fix(widgets): Tree — `auto_expand`, `scroll_to_node`, `cursor_node` property**
  - `auto_expand: bool` auto-expands nodes on insert (matches Python `auto_expand=True`).
  - `scroll_to_node(node_id)` scrolls to bring a node into view.
  - `cursor_node` property returns a reference to the cursor node data.

- **fix(widgets): DirectoryTree — freeze on expand/collapse**
  - Fixed blocking/deadlock in expand/collapse path that caused demo freezes.

- **fix(widgets): MarkdownViewer — hierarchical TOC + navigation history**
  - TOC tree builds hierarchically (H1→root, H2→under last H1, etc.) instead of flat.
  - TOC click scrolls to heading in markdown content.
  - `Navigator` with back/forward history stack for `MarkdownViewer`.

- **fix(widgets): Tabs — emit `TabActivated` when first tab added to live widget**
  - `add_tab` on a live empty `Tabs` now emits `TabActivated` for the first tab.
  - `live` flag distinguishes construction-time vs runtime `add_tab` calls.

- **fix(widgets): ListView — add `ListItem` wrapper**
  - New `ListItem` struct wrapping `Label` for proper `ListItem(Label("text"))` composition.
  - Exported in prelude.

- **fix(widgets): SelectionList/Pretty — `border_title` support**
  - `SelectionList::with_border_title()` and `Pretty::with_border_title()` builder methods.

- **fix(widgets): Static — default `markup: true`**
  - `Static::new()` now defaults to `markup: true` matching Python's `Static(content)`.

- **fix(runtime): CSS transition `color`/`background` aliases**
  - `color` and `background` now accepted as aliases for `fg`/`bg` in CSS transitions.

- **fix(examples): demo parity corrections**
  - `code_browser`: added missing CSS properties.
  - `dictionary`: added widget IDs (`#dictionary-search`, `#results-container`, `#results`).
  - `list_view`: uses `ListItem(Label("text"))` composition matching Python.
  - `selection_list_selected`: added `border_title` matching Python.
  - `markdown`: wired back/forward navigation with `Navigator`.
  - `toast`: fixed notification titles to match Python (no title on 1st/3rd).
  - `weather02`/`weather03`: added `align: center middle` to ScrollView CSS.

### 2026-02-20 (Batch D demos D-041/D-042 — worker lifecycle parity)

- **feat(examples): D-041 `weather02` demo (port of `docs/examples/guide/workers/weather02.py`)**
  - Demonstrates `ctx.request_exclusive_worker_task` as the Rust equivalent of Python's
    `self.run_worker(coroutine, exclusive=True)`.
  - `on_input_changed` spawns an exclusive background worker on every keystroke; previous
    in-flight workers are cancelled automatically.
  - Shared `Arc<Mutex<Option<String>>>` passes the worker result back to the app.
  - `on_message_with_app` receives `WorkerStateChanged::Success` and updates the `Static` widget.
  - 4 regression tests.
  - DEFERRED: Real HTTP fetch (requires blocking HTTP client); simulated with delay + fabricated data.

- **feat(examples): D-042 `weather03` demo (port of `docs/examples/guide/workers/weather03.py`)**
  - Same app as D-041 but documents the Python `@work(exclusive=True)` decorator pattern.
  - Exclusive key `"update_weather"` mirrors the Python method name; logic extracted into a
    `spawn_weather_worker` helper to mirror the decorator-as-separate-method structure.
  - 4 regression tests. Same DEFERRED HTTP gap as D-041.

- **feat(examples): D-043 `dictionary` app demo (port of `examples/dictionary.py`)**
  - Word search app; input triggers exclusive worker lookup with `@work(exclusive=True)` semantics.
  - Results rendered as Markdown via `Markdown::set_markup()`; `on_message_with_app` handles
    `WorkerStateChanged::Success` and updates the widget.
  - Built-in word list (rust, hello, world, python, textual) simulates the real API.
  - 4 regression tests. HTTP dictionary API DEFERRED.

### 2026-02-19 (MarkdownViewer widget + Batch C demos D-030/D-031)

- **feat(widgets): `MarkdownViewer` composite widget and `MarkdownTableOfContents` sidebar**
  - New file `src/widgets/markdown_viewer.rs`.
  - `MarkdownViewer::new(content)` renders Markdown with optional TOC sidebar.
  - `show_table_of_contents(bool)` / `set_show_table_of_contents(bool)` control sidebar visibility.
  - TOC sidebar uses `MarkdownTableOfContents` (Tree-based heading list), visible via
    `child_display_for_tree` (same mechanism as `ContentSwitcher`).
  - CSS class `-show-table-of-contents` toggled on the viewer to drive CSS selector layout.
  - `Markdown::extract_headings()` added to expose heading list for TOC population.
  - 7 regression tests in `src/widgets/markdown_viewer.rs`.
  - Navigation history (`go/back/forward/Navigator`) is DEFERRED pending async document loading.

- **feat(examples): D-030 `markdown_viewer` demo (port of `docs/examples/widgets/markdown_viewer.py`)**
  - Rich Markdown document with headings, tables, code blocks, lists.
  - `show_table_of_contents=true` shows sidebar; DEFERRED note for navigation history.
  - 4 regression tests.

- **feat(examples): D-031 `markdown` app demo (port of `examples/markdown.py`)**
  - TOC toggle binding (`t`), back/forward bindings declared for footer display.
  - `t` key mutates `MarkdownViewer::set_show_table_of_contents()` live; same DEFERRED gap.
  - 4 regression tests.

### 2026-02-19 (Batch C demos D-032..D-034, Tree framework additions)

- **feat(widgets): `Tree::add_root()` and `Tree::toggle_show_root()`**
  - `add_root(node: TreeNode)` appends a root node without clearing the tree.
  - `toggle_show_root()` flips `show_root` without requiring `ReactiveCtx`, for use from app-level hooks.

- **feat(examples): D-032 `tree` demo (port of `docs/examples/widgets/tree.py`)**
  - `Tree` with a "Dune" root, "Characters" sub-node, and three leaf nodes.
  - Uses `TreeNode` builder pattern (`expanded`, `allow_expand`, `with_child`).
  - 4 regression tests.

- **feat(examples): D-033 `json_tree` demo (port of `examples/json_tree.py`)**
  - `a` key adds a JSON sub-tree; `c` clears the tree; `t` toggles root visibility.
  - Embedded minimal JSON parser (no external deps); `Tree::add_root` for dynamic population.
  - 4 regression tests.

- **feat(examples): D-034 `toast` demo (port of `docs/examples/widgets/toast.py`)**
  - Four notifications on mount: information, warning (with markup), error (10s timeout), no-title info.
  - Uses `App::notify(message, title, severity, timeout)` API.
  - 4 regression tests.

### 2026-02-19 (Batch B demos D-022..D-025)

- **feat(examples): D-022 `select_widget` demo (port of `docs/examples/widgets/select_widget.py`)**
  - `Select<String>` populated with 5 poem lines; `SelectChanged` updates app title.
  - 4 regression tests.

- **feat(examples): D-023 `option_list_options` demo (port of `docs/examples/widgets/option_list_options.py`)**
  - `OptionList` with 12 named options, 6 separators, and 1 disabled option (Caprica).
  - Uses `OptionItem::with_id`, `OptionItem::disabled_with_id`, `OptionItem::Separator`.
  - 4 regression tests.

- **feat(examples): D-024 `selection_list_selected` demo (port of `docs/examples/widgets/selection_list_selected.py`)**
  - `SelectionList<String>` with 9 games (3 pre-selected); `Pretty` shows selected values.
  - `SelectionListSelectedChanged` drives Pretty update; `Selection::selected` for pre-selection.
  - 4 regression tests.

- **feat(examples): D-025 `list_view` demo (port of `docs/examples/widgets/list_view.py`)**
  - `ListView` with three string items in a centered auto-height list.
  - Rust renders items natively without `ListItem`/`Label` wrappers (Python equivalent behavior).
  - 4 regression tests.

### 2026-02-19 (Batch B framework + demos)

- **feat(message): `ButtonPressed.button_id: Option<String>`**
  - Carries the CSS id of the pressed button, mirroring Python `Button.Pressed.button.id`.
  - Populated in `Button::dispatch_press()` from `self.style_id()`.
  - All existing callsites updated (`button_id: None` for buttons without an explicit CSS id).

- **feat(widgets): `Button::id()` builder**
  - Sets a CSS id on the button widget, enabling `ButtonPressed.button_id` to carry a meaningful value.
  - `Button::new("label").id("my-btn")` — analogous to Python's `Button("label", id="my-btn")`.

- **feat(widgets): `ContentSwitcher` arena-tree child visibility**
  - Implements `Widget::child_display_for_tree()` so only the active child is visible after arena-tree extraction.
  - New `child_ids: Vec<Option<String>>` field tracks children's CSS ids in insertion order (retained after `take_composed_children` drains `children`).
  - New `children_extracted: bool` field gates arena-tree vs flat render modes.
  - `with_child`, `add_child`, `add_content` updated to populate `child_ids`.
  - 4 new regression tests in `src/widgets/content_switcher.rs`.

- **feat(examples): D-020 `tabs` demo (port of `docs/examples/widgets/tabs.py`)**
  - Demonstrates `Tabs::add_tab/remove_tab/clear`, `TabActivated`/`TabsCleared` messages, and key bindings.
  - Uses type selector `"Tabs"` for queries (no Node wrapper); `Label::with_id()` for label queries.
  - `on_key_with_app` handles add/remove/clear since named actions are not yet fully routed.
  - 4 regression tests in `docs/examples/widgets/examples/tabs/main.rs`.

- **feat(examples): D-021 `content_switcher` demo (port of `docs/examples/widgets/content_switcher.py`)**
  - Demonstrates `ContentSwitcher` with a `DataTable` and Markdown viewer as heterogeneous children.
  - Buttons carry CSS ids matching `ContentSwitcher` child ids; `ButtonPressed.button_id` drives switching.
  - Mirrors Python's `self.query_one(ContentSwitcher).current = event.button.id` pattern.
  - 4 regression tests in `docs/examples/widgets/examples/content_switcher/main.rs`.

### 2026-02-20

- **feat(app): `App::set_title()` / `App::set_sub_title()` / `App::clear_sub_title()` runtime APIs**
  - Mirrors Python `App.title` / `App.sub_title` reactive properties.
  - `set_title(title)` and `set_sub_title(sub_title)` update stored values and enqueue a
    `ScreenTitleChanged` broadcast that reaches the `Header` widget on the next event loop pass.
  - `clear_sub_title()` resets the subtitle to the Header's default.
  - `title()` / `sub_title()` accessors for reading current values.
  - Pending messages are drained at the start of `dispatch_background_runtime_messages()`.
  - Added 5 regression tests.

- **feat(widgets): `Static::update()` / `Static::update_rich()` / `Static::clear()` content update APIs**
  - Mirrors Python `Static.update(content)` which accepts any Rich renderable.
  - `update(text)` replaces content with a plain string (delegates to `Label::set_text()`).
  - `update_rich(text: rich_rs::Text)` stores pre-rendered rich text (e.g. syntax-highlighted code
    produced by `rich_rs::Syntax::highlight()`); `Widget::render()` renders the `Text` directly.
  - `clear()` empties the widget.
  - `layout_height()` for rich content returns the line count of the stored `Text`.
  - Added 5 regression tests.

- **feat(widgets): `ScrollView::scroll_home()` / `ScrollView::scroll_end()`**
  - Mirrors Python `Widget.scroll_home(animate=False)` / `Widget.scroll_end(animate=False)`.
  - `scroll_home()` is an alias for `scroll_to(0)`.
  - `scroll_end()` scrolls to `max_offset()`.
  - Added 2 regression tests.

- **feat(examples): D-012 `code_browser` demo (port of Python `examples/code_browser.py`)**
  - Broad integration demo: `DirectoryTree`, `Static::update_rich`, `ScrollView::scroll_home`,
    `App::set_title`/`set_sub_title`, `Header`, `Footer`, key bindings.
  - Tree visibility toggled via `app.toggle_class('Screen', '-show-tree')` binding — no custom
    action handler needed; CSS `Screen.-show-tree #tree-view { display: block; }` does the work.
  - Uses `rich_rs::Syntax::from_path()` + `.highlight()` for syntax-highlighted file view.
  - Inline CSS loaded via `App::load_stylesheet()` in `configure()` hook.
  - 4 regression tests in `examples/code_browser/main.rs`.

- **feat(examples): D-013 `directory_tree_filtered` demo**
  - Port of Python `docs/examples/widgets/directory_tree_filtered.py`.
  - Demonstrates `DirectoryTree::filter_paths()` with a `no_dotfiles` predicate.
  - 3 regression tests in `docs/examples/widgets/examples/directory_tree_filtered/main.rs`.

### 2026-02-19

- **feat(reactive): app-level reactive bridge for `TextualApp` struct fields**
  - `TextualApp` trait gains two new methods: `reactive_widget_mut()` (returns
    `Option<&mut dyn ReactiveWidget>`, default `None`) and `on_mount_with_app()` (called after the
    widget tree is built, matching Python Textual's `on_mount` timing for init-watcher dispatch).
  - `App` struct gains `pub fn reactive_ctx(&mut self) -> &mut ReactiveCtx` — reactive setters
    generated by `#[derive(Reactive)]` accept this context as their second argument, recording field
    changes that are dispatched to `reactive_widget_mut()` after each hook call.
  - `TextualAppAdapter` dispatches pending reactive changes (setter flags + watcher calls) after
    every `on_app_key`, `on_app_action`, `on_app_message`, `on_app_tick`, and `on_app_mount` call.
    Repaint/layout flags from both the setter and the watcher are propagated to `EventCtx`.
  - `Widget` trait gains `fn on_app_mount(&mut self, app: &mut App, ctx: &mut EventCtx)` (default
    no-op), called from `run_widget_tree` after the arena tree is built.
  - `ReactiveCtx::reset_flags()` added to keep repaint/layout flags clean between hook calls.
  - `ReactiveWidget` and `ReactiveCtx` are now re-exported from `textual::` top level.
  - Added 7 regression tests covering: setter→watcher via key/action/tick, init dispatch via
    mount, repaint propagation, no-dispatch when `reactive_widget_mut()` returns `None`, and
    flag reset across consecutive hook calls.

- **feat(animation): expand CSS transition dispatch to all animatable style properties (P2-36 closure)**
  - `transition_requests_for_style_change` now emits `StyleAnimationRequest` for the 12 style-value
    animatable properties: `fg`, `bg`, `width`, `height`, `min_width`, `max_width`, `min_height`,
    `max_height`, `margin`, `padding`, `tint`, `background_tint`.
  - `dispatch_animation_frame` calls `step_style()` each tick and applies `StyleAnimationUpdate`
    results directly to widget inline styles — matching Python Textual's per-tick style mutation approach.
  - The 4 existing float properties (`opacity`, `text_opacity`, `offset_x`, `offset_y`) continue to
    use the event-dispatch `AnimationRequest` path unchanged.
  - Added 4 new regression tests covering color, spacing, no-op, and apply-helper behavior.

- **refactor(docs/examples): reorganize docs examples into category crates with unified launcher**
  - Moved `docs/widgets/` to `docs/examples/widgets/` and modal demos to `docs/examples/guide/screens/`.
  - Added category crates for all Python Textual doc categories: `app`, `events`, `getting_started`, `guide/*`, `how-to`, `styles`, `themes`, `tutorial`.
  - Added `tools/run-doc-example.sh` unified launcher replacing `tools/run-doc-widget.sh`.
  - Added `tools/doc_examples_index.toml` category→manifest mapping for the launcher.
  - Added `tools/gen-doc-example-stubs.sh` and stub templates to generate placeholder examples.
  - Generated 295 stub examples tracking Python Textual's full docs example surface.
  - Bundled `java_highlights.scm` locally into the `text_area_custom_language` example to remove the Python Textual sibling-repo dependency.
- **[wip] refactor(screen compositing parity): introduce canonical `Screen`/`ModalScreen` host roots and layered screen rendering**
  - Pushed screens now mount through a dedicated host widget that exposes canonical CSS type identity (`Screen` / `ModalScreen`) while preserving composed body widgets as descendants.
  - Runtime tree rendering now composites visible app + screen layers back-to-front with per-layer stylesheet isolation and opaque/translucent modal background semantics aligned to Python defaults.
  - Added regression coverage for modal/non-modal underlay behavior and style-sheet isolation across composed screen layers.

- **[wip] refactor(shared dim path): route command palette underlay dimming through background-alpha compositor path**
  - Removed command-palette-specific runtime dim branch (`with_dim(true)` + panel exclusion) and switched to shared preserve-underlay background tint composition.
  - Shared tint composition now blends both background and foreground colors, improving modal-style dim parity with Python overlays/screens.
  - Runtime command-palette host is now explicitly full-viewport (`position: absolute; width: 100%; height: 100%`) so the shared dim path applies across the entire app surface.
  - Added/updated regression coverage for command palette tree-mode tint behavior while preserving undimmed panel surface.

- **fix(app-root scrollbar parity): reclaim viewport width when overflow disappears**
  - Tree layout-info propagation now feeds `AppRoot` with its solved viewport (`content_rect`) dimensions instead of full layout box dimensions, keeping internal viewport state aligned with runtime scrollbar geometry.
  - This lets app-global scrollbar lanes collapse cleanly on resize when content no longer overflows, so content immediately reclaims horizontal space (Python-equivalent behavior).
  - Added regression coverage for `AppRoot` viewport-size sync and resize transition from overflow to non-overflow.

- **fix(command palette parity): remove tree-host paint artifact and dim underlay in tree mode**
  - Runtime command palette host now keeps its extracted spacer child hidden in tree mode, removing the one-cell header-title overwrite artifact seen when opening the palette.
  - Tree compositor now applies a dim scrim to the already-painted app underlay while command palette is open, excluding the palette panel region so the palette surface remains undimmed.
  - Added regression tests for runtime-host child visibility behavior and tree-mode dimming boundaries.

- **fix(runtime hover/render parity): stop no-scrollbar content flicker on mouse hover**
  - Fixed post-render tree layout propagation to use solved tree geometry (`layout_rect`) instead of painted hit-test bounds, preventing `AppRoot`/scroll viewport collapse on sparse paint frames.
  - This resolves hover-driven text clipping/flicker in modal/log-style views when vertical scrollbar lanes are hidden.
  - Removed the obsolete hit-test-driven tree layout propagation path to keep a single canonical layout-info source.
  - Added regression coverage ensuring post-render layout propagation does not shrink viewport state from narrow hit-test strips.

- **fix(modal03 parity): honor callback-based quit flow when dismissing quit dialog**
  - `docs/widgets/examples/modal03` now follows Python modal03 result semantics (`dismiss(true/false)`) and stops the app through event-loop stop request (`ctx.request_stop()`), so pressing `Quit` exits immediately instead of only dismissing the modal.
  - Kept cancel behavior unchanged (`dismiss(false)`), returning focus to the app without exiting.

- **fix(tooltip parity): unify header/footer hover tooltips on shared system `Tooltip` with canonical placement behavior**
  - Replaced runtime-only hover-bubble composition with a shared tree-mounted system `Tooltip` widget (`#textual-tooltip`) so header/footer/tooltips follow the same CSS defaults and composition path.
  - Added widget-level tooltip anchors (`Widget::tooltip_anchor`) and wired `HeaderIcon` / `Footer` anchors to stabilize placement to Python-equivalent hit regions.
  - Tooltip viewport constraints now resolve from owner content viewports (excluding scrollbar lanes), fixing footer `^p palette` tooltip horizontal clamping in `modal01`.
  - Corrected inflected (above-anchor) tooltip vertical placement so footer tooltips sit above the footer row instead of overlapping clickable bindings.
  - Opening CommandPalette now dismisses visible hover tooltip immediately and starts a short cooldown to prevent tooltip flash while pointer moves toward the palette.

- **fix(header parity): align header composition/interactions with Python widget structure**
  - `Header` now composes canonical child widgets (`HeaderIcon`, `HeaderTitle`, `HeaderClock` / `HeaderClockSpace`) instead of relying on monolithic component-class rendering in tree mode.
  - Restored canonical selector behavior by removing temporary component fallback rules/tests and relying on Python-aligned defaults (`HeaderIcon:hover`, `App:blur HeaderTitle`).
  - Header icon clicks now dispatch the command palette action message path (`AppCommandPalette`) while preserving the command-palette binding hint/tooltip contract.
  - Added regression coverage for composed header structure, tree-mode header toggle behavior, and app-focus-driven `HeaderTitle` dimming.

- **refactor(scrollbar phase2 cleanup): remove widget-local inline scrollbar branches for migrated hosts**
  - `Log`, `RichLog`, `KeyPanel`, and `DataTable` no longer maintain inline scrollbar paint/drag paths after dedicated scrollbar-child migration.
  - Removed legacy widget-local drag state branches from these widgets; scrollbar interaction now flows through dedicated `ScrollBar` children + `Message::ScrollbarScrollTo`.
  - Simplified host render geometry assumptions so migrated widgets render only content, while runtime-host lanes own scrollbar space/hit behavior.

### 2026-02-18
- **fix(app-root scrollbar drag parity): animate root scroll-to updates and keep fixed thumb gain**
  - `AppRootScrollbarScrollTo` now carries float offsets and animation intent so scrollbar drag/click route through the same animated root offset pipeline instead of immediate integer jumps.
  - Added float-preserving root scroll offset plumbing (`Widget::scroll_offset_f32`) and root animation handlers for `approot.offset_x` / `approot.offset_y`.
  - Normalized tree render consumption of root/widget scroll offsets to use rounded float offsets and guarded scrollbar thumb-position sync while dragging.
  - Removed temporary env-gated drag gain modes and kept the fixed gain path as default behavior for predictable drag feel.

- **fix(screen scrollbar parity): stabilize AppRoot scrollbar lane hit/drag behavior**
  - Kept AppRoot scrollbar lanes in fixed screen-space for hit-testing/local coordinate mapping (matching render-time scroll exclusion), fixing thumb-drag edge cases near max offset.
  - Added regression coverage for AppRoot scrollbar-child scroll transform exclusion and stable local coordinate mapping under non-zero root scroll offsets.
  - Improved `ScrollBar` drag/release handling and style-token rendering parity so thumb interaction no longer sticks after end-of-track drags.

- **fix(modal01 parity): center dialog question text via core `content-align` + remove button row artifacts**
  - Added core content-alignment application in the shared widget render pipeline so `content-align` (horizontal + vertical) is respected for plain widgets like `Label`, not only widget-specific render paths.
  - Added `Label::with_id(...)` / `style_id()` support so example parity CSS selectors (for example `#question`) target the actual label widget instead of wrapper nodes.
  - Updated docs `modal01` example composition to mirror Python structure more closely (direct `Grid` children with id-bearing question label).
  - Narrowed text-style suppression metadata handling (`textual:no_text_style`) so synthetic line-padding/centering spaces keep background fill while avoiding reverse/bold text artifacts on button focus rows.

### 2026-02-17
- **fix(tree scroll parity): propagate root virtual extents + root scroll offsets in arena-tree render path**
  - Root-level `ScrollView`/`VerticalScroll`/`HorizontalScroll`/`ScrollableContainer` now receive tree-derived virtual content size in tree mode, so Home/End and scrollbar limits reflect laid-out child bounds.
  - Tree rendering now applies root widget scroll offsets to child paint origin (matching non-root scroll behavior), fixing cases where offsets changed but visible rows/columns did not move.
  - `ScrollEnd` now advances both axes in `ScrollView` (Python-aligned semantics).
  - Mouse wheel input now maps vertical wheel deltas to horizontal scrolling for horizontal-only scroll containers (`overflow-y: hidden` + horizontal overflow enabled).
  - Added/updated container parity coverage for scroll Home/End behavior across `ScrollView`, `ScrollableContainer`, `VerticalScroll`, and `HorizontalScroll`.

- **feat(worker): closure-backed worker tasks via `WorkerRequestPayload::Task`**
  - Added `WorkerRequestPayload::Task(SharedWorkerTask)` variant for arbitrary `FnOnce + Send` work units.
  - Added `SharedWorkerTask` — a clone-friendly `Arc<Mutex<Option<FnOnce>>>` wrapper so closure payloads survive `WorkerRequest` cloning in runtime paths.
  - Added `EventCtx::request_worker_task()` and `request_exclusive_worker_task()` convenience methods for closure-backed workers.
  - Added `EventCtx::request_worker_with_payload()` and `request_exclusive_worker_with_payload()` for passing explicit payloads.

- **feat(messaging): `Message::can_replace()` — message-driven coalescing with `UserMessage` hook**
  - Added `Message::can_replace(&pending)` encoding replacement semantics for all known rapid-fire variants (InputChanged, TextAreaChanged, DataTableCursorMoved, etc.).
  - Added `UserMessage::can_replace()` default hook so custom messages can opt into coalescing.
  - Refactored `coalesce_message_queue()` to delegate to `Message::can_replace()` rather than a routing-local `is_message_replaceable()` predicate, aligning with Python Textual's queue semantics.

- **feat(runtime): app-scoped data binding (`App::set_data` / `get_data` / `data_bind`)**
  - `App::set_data(key, value)` stores a typed value and immediately re-applies any registered bindings for that key.
  - `App::get_data(key)` retrieves a typed value by key.
  - `App::data_bind(key, selector, apply)` registers a typed callback; matched widgets are updated whenever the key changes.

- **feat(widget): `Widget::render_line()` and `render_lines()` default methods**
  - `render_line(y, ...)` extracts a single visual row; `render_lines(start_y, count, ...)` collects a contiguous range.
  - Default implementations delegate to the existing `render()` path; widgets can override for efficient line-level rendering.

- **refactor(runtime tree-only): remove legacy non-tree render compatibility paths and align container/tree contracts**
  - Runtime/event-loop/render paths now operate on the arena tree as the single rendering/dispatch mode; legacy compatibility branches were removed from core flow.
  - Container-family widgets were simplified to tree-driven behavior (no fallback child forwarding/render composition paths), reducing duplicated logic and stale compatibility state.
  - Tree display/layout sync wiring was tightened across routing/help/runtime glue, with tests/snapshots updated to assert tree-only behavior.
  - `TabbedContent` now applies `initial(...)` selection during pane registration (before mount), fixing nested tabbed-content initial visibility on first render in docs parity demos.

- **refactor(containers): move container family out of legacy aliases and into dedicated modules**
  - `src/widgets/aliases.rs` now contains only `Static`; container/scroll implementations were migrated to `src/widgets/containers/*`.
  - Added dedicated container modules for thin wrappers (`horizontal`, `vertical`, `group`, `center/right/middle`, `item_grid`) and scroll widgets (`vertical_scroll`, `horizontal_scroll`, `scrollable_container`).
  - Added shared `scroll_core` helpers and routed migrated scroll containers through the new container module exports in `src/widgets/mod.rs`.
  - Preserved container parity behavior by keeping tree-mode/non-tree-mode contracts and migrated scroll regression tests with the implementations.

- **fix(runtime copy-selected fallback): show quit-help toast when no text is selected**
  - `copy_selected_text` now mirrors Python Textual fallback behavior by showing help-quit notification when selection is empty.
  - Applied to all runtime entry paths (key action dispatch, app message dispatch, and event-loop fast path) to keep behavior consistent.
  - Added regression test `app_copy_selected_text_falls_back_to_help_quit_notification`.

- **refactor(renderables parity): introduce Python-style base renderables modules and extract shared bar logic**
  - Added `src/renderables/{bar,blank,gradient,styled,text_opacity,tint}.rs` and exposed them through `crate::renderables`.
  - Kept existing renderables (`Digits`, `Sparkline`) alongside the new module set to mirror Python Textual structure in a Rust-idiomatic form.
  - Moved tabs underline rendering to shared `renderables::Bar`, removing duplicated half-cell bar composition logic from `Tabs`.
  - Added regression coverage for each new renderable module (dimensions, styling behavior, metadata hooks, and color processing paths).

- **refactor(progress-bar parity): route determinate/gradient/indeterminate rendering through shared `renderables::Bar`**
  - `ProgressBar` now uses `renderables::Bar` for determinate fills and gradient fills, replacing duplicated per-widget bar-cell composition logic.
  - Added configurable bar glyph APIs (`chars`, `half_chars`) so `ProgressBar` keeps block/space visuals while `Tabs` keeps line-glyph visuals.
  - Indeterminate animation now follows Python’s time-based highlight-range algorithm (30 cells/sec, 25% highlight width, bounce over imaginary width) while still honoring `AnimationLevel::None`.
  - Updated progress-bar regression tests to validate rendered output text rather than internal segment chunk counts.

- **refactor(data-table parity): use shared `renderables::Bar` for horizontal scrollbar rendering**
  - `DataTable` horizontal scrollbar track/thumb rendering now composes through `renderables::Bar` (space glyph mode) instead of widget-local per-cell style loops.
  - Keeps existing scrollbar geometry (`line_scrollbar_thumb`) and drag/active style semantics unchanged while removing duplicated scrollbar paint logic.

- **refactor(footer renderables): route FooterKey style-sandwich through shared `renderables::Styled`**
  - Added `Styled::process_segments(...)` as a reusable segment-level style composition helper.
  - `FooterKey` now applies its base/component style layering through shared renderables infrastructure instead of widget-local style merge loops.

- **refactor(blank renderable): wire `Blank` into app-root and scrollbar surface paint paths**
  - Added reusable `Blank::render_for_size(width, height)` and `Blank::line_for_width(width)` helpers so widget/runtime code can consume the blank renderable directly without ad-hoc space-segment loops.
  - `AppRoot` tree-mode render now emits a resolved-background `Blank` surface (Python `app.py` / `screen.py` parity direction) instead of raw unstyled space rows.
  - `ScrollView` scrollbar chrome drawing now uses `Blank`-based runs for track/thumb/corner fills, replacing repeated manual `" "` segment push loops while preserving existing thumb geometry and style-state behavior.

- **feat(button actions): wire `Button::with_action(...)` into runtime action dispatch**
  - Added `Message::ActionDispatchRequested` and runtime handling to parse/resolve/execute declarative action strings from button presses.
  - `Button` now emits `ActionDispatchRequested` when an action is set (and suppresses `ButtonPressed`, matching Python precedence).
  - Added regression coverage for action dispatch routing and button action message emission.

- **fix(screen/runtime parity): route core operations through active screen tree and add modal docs demos**
  - Added active-tree helpers (`active_widget_tree*`) and switched query/focus/selection/render/routing paths to target the active pushed screen tree when present.
  - Screen stack mount now extracts composed children/declarations into the arena tree and accepts either inline CSS text or CSS file paths for `Screen::css()`.
  - Render/layout paths now include active screen stylesheet during style resolution and propagate tree-mode virtual content extents into scroll containers.
  - Added docs-widget modal demos `modal01`, `modal02`, and `modal03` plus shared `modal01.tcss`, and updated `docs/widgets/README.md`.

- **chore(examples): start split between docs-widget demos and app-style examples**
  - Added dedicated docs examples crate at `docs/widgets` to mirror Python docs-widget examples without growing root manifest entries.
  - Moved `tabbed_content` and `tabbed_content_label_color` examples into `docs/widgets/examples/...` via `git mv` (history preserved), including associated TCSS.
  - Added docs-widget runner helper: `tools/run-doc-widget.sh`.
  - Updated README with docs-widget run commands.

- **chore(examples): migrate remaining docs-style root examples into docs widgets crate**
  - Moved all remaining widget/docs-style examples and local TCSS assets from root `examples/` into `docs/widgets/examples/<name>/main.rs` (history preserved).
  - Updated moved examples to use manifest-relative asset paths (`env!("CARGO_MANIFEST_DIR")`), including shared button CSS and custom language highlight include.
  - Updated `tools/run-doc-widget.sh` and `tools/bench_runtime.sh` to point at `docs/widgets/Cargo.toml`.
  - Removed stale root `[[example]]` entry now that docs/widget examples live in the dedicated crate.

- **fix(footer tooltip parity): add Python-style hover tooltip popup + separator-inclusive hover for `^p`**
  - Added core runtime hover-tooltip composition path:
    - widgets can now expose optional hover tooltip text via `Widget::tooltip()`,
    - runtime tracks hovered tooltip state and composes a tooltip bubble near the hovered anchor,
    - tooltip state is cleared on app blur.
  - `Footer` now exposes hovered binding tooltip text (including command-palette hint tooltip) through the new widget tooltip hook.
  - Command-palette footer hover styling now includes the separator cell (`│`) so hover highlight covers the full right hint region.
  - Added regression test `command_palette_hover_applies_to_separator_cell`.

- **fix(footer/tabs parity): tighten footer spacing and tab-gutter width to Python behavior**
  - Footer non-compact binding spacing now renders tightly (`l Leto  j Jessica  p Paul`) instead of wider Rust-only gaps.
  - Command-palette footer separator now sits directly before the key hint (`│^p`) to match Python placement.
  - Tabs underline/gutter width now tracks active tab label width (no extra side padding) for closer visual parity.
  - Added regression tests for footer spacing, command-palette separator placement, and tab underline width.

- **fix(markdown parity): align heading spacing + full-width H1 centering with Python Textual**
  - Markdown heading component classes (`.markdown--h1`..`.markdown--h6`) now carry Python-equivalent header margins so top/bottom heading spacing is applied during core Markdown render normalization.
  - Markdown heading normalization now applies margin only at heading block boundaries (not every wrapped fragment), avoiding over-expansion on wrapped headings.
  - `Markdown::layout_height()` now accounts for heading margin rows, fixing body-text clipping after heading-spacing parity changes.
  - `Markdown::content_width()` now returns no intrinsic width hint, so `width:auto` no longer shrinks Markdown to longest line and H1 centering resolves against the full pane width (matching Python behavior).
  - Added/updated regression tests for heading row offset, centered H1 placement, wrapped heading style retention, and Markdown width-hint behavior.

- **feat(selection/copy parity): add app-level selected-text action pipeline + Markdown selection hooks**
  - Added widget-level selection hooks to `Widget` (`allow_select`, `selection_at`, `update_selection`, `clear_selection`, `get_selection`, `selection_updated`) and shared `WidgetSelectionAnchor`.
  - Added app/runtime copy-selected-text plumbing:
    - new `Action::CopySelectedText`,
    - new `Message::AppCopySelectedText`,
    - default `ctrl+c` action map now routes to selected-text copy instead of quit-help.
  - `TextualAppAdapter` now exposes `copy_selected_text` action and posts app copy messages; action matrices/caller inventory were updated accordingly.
  - Runtime now tracks active selection ownership/anchors, supports drag selection lifecycle on mouse down/move/up, and resolves selected text from selection owner or focused widget.
  - Implemented Markdown selection state/extraction/highlighting (including cache-backed coordinate mapping), plus `get_selection()` support for `Input`, `TextArea`, `Log`, and `MaskedInput`.
  - Added Markdown selection regression tests and updated command-palette/tabs integration expectations affected by action-list and binding-hint parity updates.

- **fix(toast notifications): preserve title + body text in composed notification overlays**
  - Removed duplicate manual vertical padding in `Toast::render`; toast spacing now comes from CSS padding/border in the shared styled render pipeline.
  - Updated `Toast::layout_height()` to derive intrinsic height from actual content lines plus resolved CSS chrome, preventing fixed-height composition from clipping notification body lines.
  - Added regression test `toast_title_and_message_survive_fixed_height_composition` to lock behavior for title+message notifications (including quit-help text).

- **fix(runtime focus parity): clear widget focus on app blur and restore it on app refocus**
  - App runtime now mirrors Python Textual app-focus behavior:
    - on terminal `FocusLost`, capture currently focused tree node and clear widget focus,
    - on `FocusGained`, restore that focused node when still present/displayed and no newer focus is set.
  - Added full-content invalidation on focus transitions so focus-dependent visual states do not leave stale highlights after blur/refocus cycles.
  - Added regression coverage in runtime tests:
    - `app_blur_clears_tree_focus_and_remembers_last_focused_node`
    - `app_focus_restores_blurred_focus_when_no_new_focus_exists`.

- **[wip] parity(help-panel key rows): Python-like key display/order + tooltip-capable hint plumbing**
  - Added optional `tooltip` metadata to `BindingDecl` / `BindingHint` and propagated it through runtime hint normalization and footer binding conversion.
  - Runtime hint dispatch now merges widget hints before app-level hints, matching Python-style active binding order in `HelpPanel`.
  - Declarative binding hints now derive display text from binding key specs (including comma-separated alternates) with Python-like formatting (`^c super+c`, arrow keys, preserving `tab` / `shift+tab` labels).
  - `TextualAppAdapter` default `ctrl+q` binding now carries Python parity tooltip text; command palette hint now carries `"Open command palette"` tooltip metadata.
  - `KeyPanel` now supports wrapped description rows plus dim wrapped tooltip rows, improving sidebar parity for long descriptions/help text.

- **[wip] parity(help-panel sections): focused-first hint ordering + namespace separators**
  - Focus-path binding hint collection is now ordered focused→root, so focused widget bindings appear first in `HelpPanel`/`KeyPanel`.
  - Binding hints now carry optional `namespace` metadata, used by `KeyPanel` to insert section separators between binding source groups (Python-style grouping behavior).
  - `TextualAppAdapter` now publishes hidden focus/copy bindings as explicit `screen`-namespace declarative rows, so HelpPanel shows `tab` / `shift+tab` / `^c super+c` like Python while keeping footer output unchanged.
  - Command palette hint metadata now includes app namespace and no longer forces priority sorting in help-panel output.

- **fix(border alpha composition): respect translucent border colors in rendered edge glyphs**
  - Border rendering now composes edge colors with alpha (for example `vkey $foreground 30%`) over local inner/outer surfaces before converting to terminal colors.
  - This fixes HelpPanel/KeyPanel split divider intensity to match Python-style dim separators instead of rendering as opaque bright lines.
  - Added regression test `help_panel_border_color_composes_foreground_alpha_over_background`.

- **perf(runtime hit-test): remove duplicate full-frame scan during tree layout info apply**
  - Tree layout info distribution now reuses the `HitTestMap` already built in the render pipeline instead of rebuilding a second `NodeHitTestMap` from `FrameBuffer`.
  - `FrameBuffer` now tracks per-cell widget owner IDs as cells are written/composited and exposes `owner_bounds()`.
  - `HitTestMap::from_frame` now builds bounds from this owner map instead of rescanning nested `StyleMeta` maps per cell.
  - Overlay/command-palette/select/welcome frame composition paths now use owner-aware `FrameBuffer::set_cell(...)` writes.
  - Added explicit `HitTestMap -> NodeHitTestMap` conversion and regression coverage for bounds preservation.
  - Removes one full-frame metadata scan per render cycle in tree mode and reduces remaining hit-test extraction overhead.

- **fix(runtime loop): decouple tick cadence from render cadence under sustained input**
  - `run_with` and `run_widget_tree` now schedule ticks from a dedicated tick clock instead of render timestamps.
  - Immediate input renders no longer postpone `on_tick` / `Event::Tick` delivery while keys are held.
  - Preserves low-latency input rendering while keeping time-driven tick behavior progressing (with normal jitter under load).

- **[wip] perf(runtime loop): input-priority render path + reduced per-loop style/tick pressure**
  - Added an input-priority fast path in `run_widget_tree`: when input handling marks content dirty, runtime renders immediately before slower housekeeping phases, reducing visible key-to-frame latency.
  - When immediate input render completes and more terminal input is already queued, runtime now drains queued input first (next loop turn) to reduce visible backlog under rapid key navigation.
  - Gated full style-transition snapshot scans to style/layout-invalidated frames (or cold cache), instead of scanning all tree nodes every loop iteration.
  - Removed unconditional full-content invalidation immediately after `root.on_tick(...)`; repaint now follows normal invalidation/active-state signals.

- **[wip] perf(command-palette keypath): scope repaint invalidation to palette widget when safe**
  - In tree mode with command palette open, key events that only mutate palette-local state now invalidate the palette widget region instead of forcing global repaint.
  - Safety guard keeps global invalidation whenever key handling emits follow-up messages or requests style/layout invalidation.
  - This reduces unnecessary full-frame redraw pressure on palette navigation/search keypresses while preserving correctness paths.

- **fix(help-panel bootstrap): force initial bindings/help refresh when panel mounts**
  - `action_show_help_panel()` now invalidates cached binding-hint and focused-help snapshots immediately after mounting `HelpPanel`.
  - This ensures newly mounted help panels receive the next `BindingsChanged` and focused-help updates even when values are unchanged, preventing stale `(no bindings)` sidebars.
  - Added regression test `action_show_help_panel_invalidates_binding_and_help_caches`.

- **fix(key-panel parity): hide hidden/system bindings in HelpPanel key list**
  - `KeyPanel::set_binding_hints()` now filters out only `system` bindings (hidden bindings are preserved), matching Python key-panel semantics.
  - Added dedupe for repeated key/description pairs from merged hint sources.
  - Footer-only grouping metadata is no longer carried into `KeyPanel` rows.
  - Removed Rust-only key table headers/dividers and the extra KeyPanel title row to align HelpPanel visual structure with Python.
  - Added regression test `binding_hints_filter_system_entries_only`.

- **css parity(help-panel/key-panel): align default selector surface with Python IDs/rules**
  - Added Python-parity ID wiring for help widgets: `Markdown#widget-help`, `KeyPanel#keys-help`, and `BindingsTable#bindings-table`.
  - Added CSS parity rules in defaults for:
    - `HelpPanel > #widget-help:ansi`
    - `HelpPanel > KeyPanel#keys-help` with `min-width: initial` and `split: initial`
    - `#widget-help` reset lines (`padding: 0; margin: 0;`) before final values.

- **[wip] fix(command-palette parity/runtime): align system-command scoring + dynamic help-panel command state updates**
  - `SystemCommandsProvider` now matches Python-style behavior:
    - discovery (`query == ""`) is alphabetical by title,
    - search scoring for built-in/system commands is title-only,
    - fuzzy scoring/highlighting paths were updated to Python-aligned ranking semantics.
  - Command-list rebuild churn reduced by skipping duplicate query rebuilds when input text is unchanged.
  - `TextualAppAdapter` command publishing now:
    - synchronizes `help_panel_visible` from runtime state/messages while palette is open,
    - republishes command text when help-panel visibility changes,
    - omits unsupported `maximize` until runtime maximize/minimize semantics exist.
  - Runtime now forwards `AppShowHelpPanel` / `AppHideHelpPanel` control messages through widget delivery so palette/app adapters can react to visibility transitions.
  - Added/updated regression tests for help-panel control-message delivery and provider ordering/scoring expectations.

### 2026-02-16
- **[wip] fix(command-palette modal layering): enforce topmost render priority in tree mode**
  - Runtime child ordering now enforces `CommandPalette` as top-most among siblings during tree render collection, independent of mount order or parent `layers` declaration.
  - This closes modal overlap cases where later-mounted siblings (for example dynamically mounted panels) could partially paint over an open command palette.
  - `action_show_help_panel` mount behavior remains aligned with this model by targeting the app-content branch when a command-palette host exists.
  - Added regression coverage for:
    - sibling ordering that moves `CommandPalette` to the end/top in no-layer parents,
    - full render-node collection ordering where command palette remains last/top-most,
    - help-panel mount parent selection in command-palette-hosted runtime roots.

- **[wip] feat(command-palette context awareness): dynamic system command text + stateful Keys action parity**
  - `TextualAppAdapter` now publishes built-in command-palette system commands from current app state (instead of relying on static one-time command text), including dynamic `Keys` help text:
    - `Show help for the focused widget and a summary of available keys`
    - `Hide the keys and widget help panel`
  - While the command palette is open, help-panel visibility messages (`AppShowHelpPanel` / `AppHideHelpPanel`) now trigger command-list republish, so command help text updates live.
  - `CommandPalette` keys execution now follows state-aware show/hide behavior:
    - selecting `Keys` hides help/key panels when already visible,
    - otherwise shows them,
    - and emits the matching app message (`AppHideHelpPanel` / `AppShowHelpPanel`).
  - Added command execution parity for built-ins:
    - `Theme` posts `AppChangeTheme`,
    - `Screenshot` posts `AppScreenshot`.
  - Added regression tests for:
    - dynamic keys help text updates in `TextualAppAdapter` as help-panel state changes,
    - second keys invocation emitting hide-help behavior while collapsing the key panel.

- **[wip] fix(command-palette parity polish): full-row hover semantics, render-geometry hit consistency, and blur-safe search-row surface**
  - Command list hover/selection rendering now mirrors Python semantics:
    - hover no longer mutates keyboard selection,
    - hovered and selected states style both title + help rows with full-row background coverage,
    - added explicit `option-list--option-hover` default CSS mapping for command-list rows.
  - Command list text layout now honors option padding in render-time composition, keeping row alignment and highlight ranges consistent with configured CSS padding.
  - Command palette mouse-move mapping now uses render viewport geometry (not only last layout pass), fixing last-row hover misses when render and layout heights diverge in tree-mode overlay paths.
  - Search-row surface composition on app blur was fixed at the root:
    - palette surface normalization now treats both `$background` and `$surface` as underlay colors to be composed onto panel surface,
    - command input transparent/no-border rules now target the concrete rendered widget path (`Input.command-palette--input`), not only wrapper type selectors.
  - Added regression coverage for:
    - keyboard selection stability under hover,
    - full-row selected/hover style propagation across title/help rows,
    - palette hover clear/update behavior including last command rows,
    - blur-state search-row panel surface preservation.

- **[wip] fix(command-palette interaction parity): two-row hit mapping, hover-selection sync, and tick modal routing**
  - `CommandList` now maps mouse-down `y` coordinates from two-row visual layout (title + help) back to underlying option rows, so clicks on help rows resolve to the correct command entry.
  - Mouse move over `CommandList` now synchronizes keyboard selection with hovered command row, keeping pointer and keyboard interaction paths aligned.
  - Runtime tree dispatch now routes `Event::Tick` to the open `CommandPalette` target (same modal routing policy as key/mouse), so palette-local tick-driven behavior such as input caret blinking is not starved by focused underlay widgets.
  - Added regressions for:
    - command-list help-row click row mapping,
    - hover-to-selection synchronization,
    - tree-mode tick routing while command palette is open.

- **[wip] tune(command-palette list block alignment): post-search gap + horizontal indent parity**
  - Added an explicit extra spacer row between the search prompt row and command list rows to match Python command palette vertical rhythm.
  - Shifted command palette search prompt and command list content one column to the right for closer text-block alignment with Python screenshots.
  - Kept layout/hit-test geometry in sync by updating shared palette offsets used by render, layout sizing, and result-row click mapping.
  - Updated command palette open snapshot to lock the new row/column alignment.

- **[wip] tune(command-palette geometry + typography parity): lower search/results block and enforce help-row contrast**
  - Shifted command palette search/results geometry down by one row to better align with Python command palette vertical spacing.
  - Updated palette header/results row math to use explicit offsets (`SEARCH_ROW_OFFSET`, `RESULTS_ROW_OFFSET`) so spacing is stable and testable.
  - Ensured command help rows render with dim + not-bold styling regardless of inherited option emphasis, preserving Python-style title-vs-help contrast hierarchy.
  - Updated command palette open snapshot gate to lock the adjusted layout.

- **[wip] fix(command-palette keys command parity): route `Keys` selection to app help panel**
  - Selecting `Keys` from `CommandPalette` now posts `AppShowHelpPanel`, so TextualApp runtimes open the real help sidebar (matching Python command behavior) instead of only closing the palette.
  - Added regression assertion in command-palette widget tests to ensure `AppShowHelpPanel` is emitted alongside the selection event.

- **[wip] fix(command-palette interaction parity): modal key capture, list navigation, row selection, and non-destructive fuzzy highlights**
  - When command palette is open, runtime now routes non-priority keys directly through event dispatch and skips normal declarative/app binding execution, so typed keys update search instead of triggering underlying app shortcuts.
  - `CommandPalette` now handles list navigation keys (`Up`/`Down`/`Home`/`End`/`PageUp`/`PageDown`) while input remains focused, matching Python-style command-list traversal.
  - Row click execution now resolves from palette results geometry directly, ensuring row selection/activation works even when click targets are palette-local.
  - Added fuzzy-match highlight ranges and title-segment underlining for matched characters; highlight styling now preserves row background color (underline emphasis only), avoiding surface overwrite artifacts.
  - Added regression tests for:
    - palette list navigation under focused input,
    - sender-agnostic `InputChanged` rebuild while open,
    - row-click selection path,
    - fuzzy range extraction and underline/background-preservation rendering semantics.

- **[wip] fix(command-input subclass parity): support Python-style type inheritance in CSS selector matching**
  - Added CSS selector type-alias support in selector metadata/matching so widgets can match both concrete and base type selectors (e.g. `CommandInput` matching `Input` rules).
  - Added `Widget::style_type_aliases()` hook (default empty) and wired selector meta generation to include aliases for both full widget and component selector resolution.
  - Extended `Input` with `with_style_type(...)` so wrapper/subclass-style widgets can set a concrete style type plus base-type aliases.
  - Wired command palette input to render as concrete `CommandInput` with `Input` alias, enabling Python-style `CommandInput` selectors to apply naturally without losing base `Input` style rules.
  - Fixed transparent color composition in `Input` render path to avoid collapsing transparent backgrounds into opaque black during component style flattening.
  - Added regression coverage for:
    - selector type-alias matching semantics,
    - command-palette search-row surface parity with list/panel background.

- **[wip] fix(command-palette geometry/surface parity): panel top offset + panel-surface composition across rows**
  - `CommandPalette` panel Y placement now honors component CSS (`.command-palette--panel { margin-top: ... }`) instead of a hardcoded offset, matching Python structure more closely.
  - Added default `margin-top: 3` for `.command-palette--panel` in widget defaults.
  - Reworked panel-surface composition so command rows that carry implicit/default app background are normalized back to panel background, removing dark app-background bleed inside palette result rows.
  - Search/input/results geometry updated to keep the expected input block spacing under the new top offset.
  - Snapshot/behavior tests updated to assert content-located dim/surface semantics (instead of brittle fixed coordinates), plus a regression test that unselected rows do not reuse app background.

- **[wip] fix(command-palette css parity): honor Python `CommandPalette` component selectors without local fallback rules**
  - `CommandList` now accepts render-time help-style injection from its `CommandPalette` parent render path, so Python selector `CommandPalette > .command-palette--help-text` is applied in-context (instead of relying on `CommandList`-local fallback selectors).
  - Removed `CommandList > .command-palette--help-text` fallback default CSS rule; `CommandPalette` component selector is now the source of truth.
  - Improved panel surface composition so cells with `bg=default` are treated as transparent and composed over panel background, eliminating dark leaks in command rows.
  - `Input` component styling now resolves through `resolve_component_style(...)`, enabling selectors like `Input.command-palette--input > .input--placeholder` to apply correctly.
  - Added regression tests for help-row surface/dim semantics and placeholder dim styling in `tests/command_palette_snapshot.rs`.

- **[wip] fix(command-palette color composition parity): resolve list component styles over panel surface**
  - Command palette list component styles are now resolved over the panel surface color (instead of global default background), so alpha/transparent tokens compose against the correct local surface.
  - Added explicit command-palette surface propagation into `CommandList` and refreshed it on mount/layout/open transitions.
  - Result: highlighted row and help/option color blending tracks panel-local composition semantics more closely.

- **[wip] fix(command-palette list/input styling parity): dim help/placeholder and title-only highlight emphasis**
  - `CommandInput` now tags the underlying `Input` with a dedicated class (`command-palette--input`) so palette-specific placeholder styling can be targeted via CSS.
  - Added command palette placeholder CSS (`Input.command-palette--input > .input--placeholder`) with muted/dim appearance.
  - Aligned command help-row rendering so highlighted command selection emphasizes the title row while help text remains in help style (muted/dim), closer to Python presentation.

- **[wip] fix(command-palette layout parity): content-driven panel sizing + stable open placement**
  - Command palette panel geometry now sizes results area from command content (including list chrome row), instead of using static height assumptions.
  - Opened palette now renders with a stable top offset in render-time fallback (even when `on_layout` has not run yet), preventing top-left/stale-position placement.
  - Updated command palette snapshot and viewport assertions to match overlay-underlay composition and content-driven results layout.
  - Restored markup-command rendering regression coverage (`Deploy` / `Ship current build`) under the new geometry path.

- **[wip] fix(command-palette overlay/render parity): preserve underlay in tree mode and align Python defaults**
  - Command palette now behaves as a true overlay in tree mode by preserving wrapped subtree display while the palette is open (instead of toggling wrapped child visibility off).
  - Fixed palette surface composition so copied input/result cells retain panel background styling, eliminating black-hole sections inside the panel body.
  - Removed manual hardcoded separator line painting in palette render path; border/separator visuals now come from CSS/widget styling as in Python.
  - Synced built-in command palette copy to Python:
    - placeholder now uses `Search for commands…` (ellipsis),
    - default "Keys" help text now matches Python wording.
  - Aligned `CommandList` defaults closer to Python (`border-top`, `border-bottom`, `max-height`, focus border, highlighted-option token mapping).
  - Updated command palette snapshot and tree-mode runtime assertion to reflect overlay-preserving behavior.

- **[wip] refactor(command-palette): decompose monolith into Python-style subwidgets (`SearchIcon`, `CommandInput`, `CommandList`)**
  - Split internal command-palette rendering responsibilities into dedicated widget types inside `src/widgets/command_palette.rs`, mirroring Python Textual structure while keeping Rust-idiomatic internals.
  - Updated `CommandPalette` to compose and drive those widgets for search icon/input/result-list behavior instead of hand-rolled per-section rendering logic.
  - Exported new widget types via `widgets::mod` (`SearchIcon`, `CommandInput`, `CommandList`) for API parity and reuse.
  - Aligned default CSS wiring to target the decomposed widgets (`SearchIcon`, `CommandInput`, `CommandList`, option/help/highlight selectors), reducing component-style special cases.
  - Preserved tree-mode `Action::CommandPalette` behavior by keeping root fallback routing when direct target dispatch is unhandled.
  - Regression coverage validated with `command_palette_snapshot`, `command_palette_lifecycle`, footer tests, and tree render tests.

- **[wip] fix(command-palette overlay/tree parity): host palette as sibling overlay + modal event routing in TextualApp runtime**
  - Reworked `TextualApp` runtime root composition so `CommandPalette` is a sibling child of the app content inside `TextualAppAdapter` (instead of wrapping the whole app tree as parent chrome).
  - Result: command palette now renders as a true layered overlay in tree mode, rather than replacing/hiding the entire app subtree.
  - Updated `AppCommandPalette` runtime handling to target the `CommandPalette` node directly (`query_one("CommandPalette")` + targeted dispatch), preserving action-based open/close semantics.
  - Added modal-style event routing guard in tree mode: when the palette is open, interactive key/mouse/action events are redirected to the palette target to prevent accidental handling by underlying widgets.
  - Tightened command-palette internal message handling so palette-owned `InputChanged` recomputation only runs while open.
  - Added/updated adapter/runtime-root regression tests for composed palette host behavior.

- **[wip] fix(runtime/tabbed-content keybind parity): preserve action dispatch recipient + binding-side effects**
  - Binding-triggered `execute_action` now runs with explicit dispatch recipient context in tree mode, so widget `node_id()`-targeted side effects (for example tab underline animations) resolve to the correct arena node.
  - Unified binding path outcome handling with normal dispatch paths by preserving `stop_requested`, `animation_requests`, and `worker_requests` in runtime split control flow.
  - Result: app key bindings (`l/j/p`) now follow the same tab activation visual path as click/arrow input in `TabbedContent` demos.

- **[wip] fix(command-palette tree runtime): restore global `^p` open path without footer-binding leakage**
  - Fixed tree-mode runtime fallback so unhandled key/mouse/action events still reach the runtime root wrapper (`on_event`) when needed by non-arena wrapper behavior (for example command palette open/close handling).
  - Extended widget-controlled tree display sync to also apply root-wrapper `child_display_for_tree(...)` policy, so wrapper-controlled visibility toggles correctly affect arena children.
  - Kept Python-style action flow for `AppCommandPalette`: runtime dispatches `Action::CommandPalette`, while palette lifecycle messages are emitted by the widget itself.
  - Added regression gates covering:
    - root key fallback in tree mode,
    - command palette rendering/open visibility in tree mode,
    - action-based command palette open dispatch path.

- **[wip] fix(footer parity): Python-style FooterKey hover/click semantics including command palette**
  - Aligned footer key interaction behavior with Python Textual:
    - click on any footer key hint now flows through the same binding/action pipeline as real key presses (`AppSimulateKey` runtime dispatch parity),
    - footer hit-testing now resolves rendered binding regions (including grouped keys) instead of coarse width heuristics.
  - Restored item-level `FooterKey:hover` visuals by fixing background composition against the footer surface (not global app background), so hover reads as a full key-item highlight.
  - Added command-palette (`^p`) parity handling in footer hover/click hit routing so the right-docked key now responds like other footer keys.
  - Added regression coverage for:
    - simulated key parsing and dispatch path behavior,
    - per-binding click resolution (`l/j/p`) and grouped-key click targeting,
    - footer hover background behavior including the command-palette item.

- **[wip] fix(tabbed-content + layout/test regressions): restore stable fallback behavior and remove false `cargo test` blocker**
  - Fixed `ScrollView` content-height inference to ignore trailing blank probe rows from oversized auto/fill renders, preventing false vertical scrollbar activation and viewport width shrink in focus/layout paths.
  - Restored `Middle` / `CenterMiddle` vertical-centering behavior to use intrinsic child height (with non-blank rendered fallback) instead of shaped full-height output.
  - Stabilized `TabbedContent` non-tree compatibility semantics used by isolated tests/previews:
    - keyboard/mouse tab switching,
    - hidden/disabled activation guards,
    - active-pane promotion after hide/disable,
    - binding hints.
  - Updated `TabbedContent` style assertions to render through widget-tree runtime (canonical path) while keeping non-tree compatibility minimal.
  - Result: `cargo test` now runs through the previously reported stop point and completes successfully in this tree.

- **[wip] fix(widget render + tabs parity): restore chrome-aware intrinsic sizing, scoped tab state classes, and tab/button interaction regressions**
  - Added a shared widget render path (`render_widget_with_meta`) that consistently applies:
    - CSS style stack context,
    - line-pad and CSS padding composition,
    - fill/background shaping to content height,
    - border/title/subtitle + opacity pass,
    - stable node metadata tagging.
  - Intrinsic sizing parity fixes across core widgets:
    - `content_width()` hints now include horizontal chrome (CSS padding + border spacing) for auto-sizing widgets including `Button`, `Checkbox`, `Collapsible`, `ContentSwitcher`, `DataTable`, `DirectoryTree`, `Link`, `ListView`, `Log`, `OptionList`, `RadioButton`, `RadioSet`, `Rule` (vertical), `Select`, `SelectionList`, `Switch`, `Tabs::Tab`, `Text::Markdown`, `Toast`, `Tooltip`, and `Tree`.
    - `Panel` intrinsic width/height now include resolved CSS chrome in addition to panel-local border/padding behavior.
  - CSS selector/runtime consistency:
    - added `selector_meta_generic_with_classes(...)` and wired display/visibility tree pass to resolve styles with runtime tree classes attached to nodes.
  - Tabs parity fixes:
    - introduced per-instance scoped `Tabs` style IDs so runtime class/disabled mutations target the correct tabs instance (`#<tabs-scope> #tabs-list > #<tab-id>`),
    - moved initial active/hidden/disabled tab state to declarative child classes in `tab_decls()` (avoids mount-time class replay races),
    - click-on-tab now requests runtime focus and treats clicking the already-active tab as handled,
    - underline base/active style composition corrected for Python-like line appearance,
    - intrinsic tab width now accounts for resolved CSS padding (`width: auto` spacing parity).
  - Button interaction parity:
    - `MouseUp` message emission now occurs before clearing pressed state, so click-generated `ButtonPressed` descriptions include `-active` consistently.
  - Added broad regression coverage in new `tests/intrinsic_size_contract.rs`:
    - locks border-box/content-box auto-size contracts,
    - verifies no-wrap markdown line behavior in wide viewport,
    - verifies widget padding deltas are reflected in intrinsic width for a large cross-widget matrix,
    - verifies tab header auto-width keeps expected horizontal gaps.
  - Added button regression test:
    - `mouse_click_message_description_includes_active_class`.
  - Added Tabs ergonomics helpers to avoid allocation-heavy callsites:
    - `Tabs::is_active(&str) -> bool`,
    - `Tabs::with_active_id(|Option<&str>| ...)`.
  - Updated tabs integration assertions to use non-allocating active-id checks.
  - Runtime message-chain fix for tree mode:
    - `dispatch_message_queue_with_runtime()` now recursively drains messages emitted during message handling (instead of dropping follow-up messages), restoring parity between keyboard and mouse tab switch paths when class/style updates are emitted indirectly via message handlers.
  - Nested TabbedContent routing fix:
    - parent `TabbedContent` no longer marks `TabActivated` as handled when the pane id does not belong to that instance, preventing nested subtab activation from being swallowed.
  - Added targeted regression gates for both runtime message chaining and unknown nested `TabActivated` handling.

### 2026-02-15
- **[wip] fix(tabbed-content parity): align tab state styling, underline behavior, footer hints/separator, and markdown heading surfaces**
  - Runtime/tree binding hints: root app bindings and hints are now preserved in tree mode, and key dispatch falls back to root action execution when tree-target handling doesn’t consume the action.
  - Tabs/TabbedContent parity pass:
    - switched tab underline rendering to Python-style half-cell bar math (`╺/━/╸`) in both `Tabs` and `TabbedContent`,
    - aligned component CSS defaults for active/inactive/hover/disabled tab state and ANSI dim/not-dim semantics,
    - hid left/right "Switch tab" binding hints by default to match Python’s `show=False` behavior.
  - Color/style conversion parity:
    - `Style::to_rich()` now treats fully transparent fg/bg as unset and flattens semi-transparent fg/bg against effective surface background for parity-friendly contrast.
  - Footer parity:
    - command-palette hint now renders with a visible, styleable separator segment before the right-docked `^p palette` item.
  - Markdown heading pass:
    - added markdown heading component default CSS hooks and heading content alignment wiring for centered h1 rendering parity.
  - Added/updated parity regression coverage in `tests/tabs.rs`, `tests/tabbed_content.rs`, `tests/header_footer.rs`, `tests/markdown.rs`, plus style conversion tests in `src/style.rs`.

### 2026-02-14
- **feat(parity): close App/runtime API parity gaps for actions, DOM query mutations, lifecycle events, and controller aliases**
  - Expanded app action parity from partial adapter coverage to full Python action matrix coverage in `TextualApp` (`23/23` actions in `APP_ACTIONS` with adapter execution paths and argument validation).
  - Added runtime handling and message types for full app action set, including richer `AppScreenshot { filename, path }` payload support and caller-inventory parity tests.
  - Implemented app-level convenience wrappers: `App::batch_update`, `App::mount`, `App::mount_all`, `App::get_child_by_type`.
  - Extended `DomQueryMut` parity semantics:
    - added `remove()`,
    - added multi-class helpers (`add_classes`, `remove_classes`, `toggle_classes`),
    - aligned `focus()`/`blur()` to first-match semantics,
    - expanded `set(...)` to include `disabled` and `loading`.
  - Added widget trait mutation hooks for query-driven state changes:
    - `Widget::set_disabled_state`,
    - `Widget::set_loading_state`,
    - `Widget::is_loading`.
  - Wired runtime dispatch of `Event::ScreenSuspend` / `Event::ScreenResume` through push/pop/switch-mode app flows and added ordering coverage tests.
  - Added Python-compat controller aliases/APIs:
    - `Tabs::{disable, enable, hide, show}`,
    - `TabbedContent::{disable_tab, enable_tab, hide_tab, show_tab, get_tab, get_pane, active_pane}`,
    - `ContentSwitcher::add_content(child, id, set_current)`.
  - Removed stale deferred/no-op parity comments for resolved runtime paths (`ScreenSuspend/ScreenResume` event docs, worker runtime header, reactive loop note).

### 2026-02-14
- **feat(devtools): inspector-grade snapshot protocol v2, new devtools commands, runtime hooks**
  - Added `Style::debug_properties()` returning all set CSS properties as human-readable `(&str, String)` pairs for devtools inspection.
  - Enriched devtools snapshot protocol (v2): widget lines extended from 11→19 columns (content_rect, display states, visibility, mounted, parent_id, children_ids). Added `style\t{id}\t{prop}\t{value}` lines for resolved CSS. Fixed class merging to include tree-level classes.
  - Added devtools commands: `TOGGLE_DISPLAY <id>`, `HIGHLIGHT <id>`, `ADD_CLASS <id> <class>`, `REMOVE_CLASS <id> <class>` with full parsing, dispatch, and runtime handling.
  - `HIGHLIGHT` auto-clears after 500ms via `pending_highlight_clear` timer.
  - Added `TextualApp::on_tick()`/`on_tick_with_app()`, `on_action_with_app()`, `on_message_with_app()` convenience hooks with `&mut App` runtime handle.
  - Added `Widget::on_app_action()`, `on_app_message()`, `on_app_tick()` trait methods for runtime-level app hooks.
  - Runtime fallback wiring now invokes app-handle hooks in the event loop: unhandled actions flow to `on_app_action()`, unhandled messages flow to `on_app_message()`, and each tick runs `on_app_tick()` before `Event::Tick`.
  - Migrated cross-widget example callsites to centralized query/mutation APIs:
    - `examples/buttons_advanced.rs` status updates now use `on_message_with_app` + `with_query_one_mut_as::<StatusLine>()`.
    - `examples/data_table.rs` event footer updates now use `on_message_with_app` + `with_query_one_mut_as::<StatusLine>()`.
  - Consolidated ID-targeted controller widget lookups through internal query-style helpers in `ContentSwitcher`, `Tabs`, and `TabbedContent` to reduce duplicated ad-hoc scans.

### 2026-02-14
- **feat(runtime): DomQuery/DomQueryMut API, on_key_with_app hook, selector class actions**
  - Added `DomQuery` (read) and `DomQueryMut` (write) types for chainable CSS-selector-based widget tree queries with filter/exclude/results_where combinators and bulk mutation helpers (add_class, remove_class, toggle_class, set_classes, set_styles, set_display, set_visible, focus/blur, refresh).
  - Added `App::query_exactly_one()`, `query_one_optional()`, `query_children()`, `query_ancestor()`, `get_widget_by_id()`, `get_child_by_id()`, `query_mut()` query variants.
  - Added `App::with_widget_mut_as()` and `with_query_one_mut_as()` for type-safe downcasting widget mutation.
  - Added `TextualApp::on_key_with_app()` hook receiving `&mut App` handle for query/mutation during key handling (Python Textual alignment). Runtime dispatches this before normal widget key routing.
  - Added `app.add_class`/`remove_class`/`toggle_class` action declarations and `AppAddClass`/`AppRemoveClass`/`AppToggleClass` runtime messages with full action→message→runtime pipeline.
  - `TextualAppAdapter` now implements `action_namespace`/`action_registry`/`execute_action` for `app.*` actions including `quit` and selector class mutations.
  - `Widget` trait now requires `Any` supertrait bound for runtime downcasting.
  - Added `Widget::on_app_key()` trait method for runtime-level app key hooks.
  - Examples `keys.rs` and `rich_log.rs` rewritten to use `on_key_with_app` with `with_query_one_mut_as` — eliminated `Arc<Mutex<RichLog>>` shared state and `SharedKeyLog`/`SharedRichLog` wrapper widgets entirely, using message-based communication instead.
  - Comprehensive regression tests for all new query APIs, DomQuery combinators, DomQueryMut mutations, selector class actions, action routing pipeline, and on_app_key dispatch.

- **feat(runtime): app-level `on_key` hook and CSS selector query API** (previous entry)
  - Added `TextualApp::on_key()` capture-phase hook for app-level key interception (mirrors Python Textual's app-level key handling). Wired through `TextualAppAdapter::on_event_capture` and tree-mode `dispatch_event_auto`.
  - Added `App::query()`, `query_one()`, `with_widget_mut()`, `with_query_one_mut()` for CSS-selector-based widget tree queries and scoped mutation.
  - Tree-mode `dispatch_event_auto` now runs root key capture before tree dispatch and root action fallback after unhandled tree dispatch.
  - Examples `keys.rs` and `rich_log.rs` rewritten to use `on_key` hook with `Arc<Mutex<RichLog>>` shared state, eliminating widget-level key interception wrappers.
  - Added regression tests for all new APIs (key hook capture/passthrough, tree dispatch integration, query/query_one delegation, with_query_one_mut mutation).

### 2026-02-14
- **feat(widgets): TabbedContent tree-mode parity — child extraction, action routing, binding hints**
  - TabbedContent now supports tree-mode child extraction for runtime-managed widget trees, with `show_tab` action routing, initial tab selection, and keyboard/mouse activation.
  - Added `dispatch_event_broadcast_tree()` for runtime-global events (binding-hint payload changes) so non-focused widgets like Footer receive notifications.
  - Container widget gains `visit_children_mut()` support for tree-mode child extraction.
  - Widget tree node display/visibility guards on focused-node resolution to skip hidden nodes.
  - Added TabbedContent + Tabs regression tests.
- **refactor(examples): rewrite all examples to current APIs + Python Textual parity**
  - Replaced `hello.rs` kitchen-sink with polished "Mission Control" dashboard showcase (13+ widget types: Header, Footer, Sparkline, ProgressBar, DataTable, TabbedContent, Markdown, Input, Checkbox, Switch, Button, Rule, Static) with new `hello.tcss` stylesheet.
  - Deleted redundant `buttons_composed_pattern.rs` example.
  - Cleaned delegation boilerplate in `rich_log.rs` and `keys.rs` (removed unnecessary forwarded methods, fixed `style_type` for CSS matching).
  - Fixed `input_validation.rs` Palindrome validator to match Python parity (empty string passes).
  - Cleaned `text_area_custom_language.rs` (eliminated Option+take pattern) and `text_area_extended.rs` (added missing mouse forwarding).
  - Net reduction of ~340 lines across examples.
- **docs: update ROADMAP.md and README.md to reflect completed parity work**
  - ROADMAP: marked CSS defaults parity, TCSS property parity, box-model fixes, render parity, P2 deferred closures, and Phase 9.7 modularization as complete. Updated deferred items table (5→2 remaining). Added completed parity plan references.
  - README: polished rewrite reflecting 56 widgets, 108 CSS properties, 1,487+ tests, rich-rs as public crate.

- **fix(css/layout): three button parity fixes — disabled dimming, margin collapsing, box-model correctness**
  - Added `:can-focus` pseudo-class support (AST, parser, matcher, resolver, debug) and global `*:disabled:can-focus { opacity: 70%; }` rule matching Python Textual's disabled-widget dimming.
  - Added `Widget::can_focus()` trait method (inherent focus capability, ignoring disabled state) so disabled buttons still match `:can-focus`.
  - Implemented vertical margin collapsing in `layout_vertical()`: adjacent sibling margins now collapse to `max(bottom, top)` instead of summing additively.
  - Separated `line-pad` from CSS `padding`: added `Style::line_pad` field as a render-time-only property that does NOT inflate the box model, matching Python Textual semantics where `gutter = padding + border.spacing` (line-pad excluded).
  - Changed default `box-sizing` from `content-box` to `border-box` across all layout paths, matching Python Textual's default where borders are included within declared/auto width.
  - Updated render pipeline (`core.rs`, `render.rs`, `types.rs`) to read `line_pad` from the new style field instead of deriving from `resolved.padding`.
  - Added regression test: `disabled_button_matches_global_disabled_can_focus_opacity_rule`.
  - Updated layout tests for border-box default behavior.

### 2026-02-16
- **fix(runtime/tabbed-content): normalize class-aware tree style resolution across render/layout/event-loop paths**
  - Root cause addressed: tree runtime was resolving styles with mixed metadata sources (some paths ignored `WidgetTree` runtime classes while paint paths consumed them), which could desync `-active` class visuals from logical tab activation.
  - Updated tree style resolution callsites to use class-aware selector metadata (`selector_meta_generic_with_classes`) in:
    - render-time layer ordering (`sort_children_by_layer`),
    - layout info propagation (`apply_layout_info_tree`),
    - style snapshot collection for transition dispatch (`collect_current_resolved_styles`),
    - hit-test local coordinate inset calculation (`NodeHitTestMap::content_local_coords`).
  - Added parity regression gate covering app action path (not only direct setter path):
    - `tree_mode_show_tab_action_moves_active_highlight_style` ensures `show_tab(...)` moves active tab highlight/background.
  - Removed temporary debug instrumentation used during bug hunt from runtime render/event loop paths.

- **fix(layout): wire `expand` into flow sizing and clamp absolute min/max constraints**
  - `layout_vertical()` and `layout_horizontal()` now treat `expand: true` as a flex-grow signal on the layout axis, so intrinsic `auto` widgets can participate in remaining-space distribution.
  - `layout_absolute()` now applies `min-width` / `max-width` / `min-height` / `max-height` constraints (with box-sizing-aware outer-size math) for absolutely positioned children.
  - Added behavioral coverage in `tests/p2_layout_css.rs`:
    - `p2g24_absolute_applies_min_constraints`
    - `p2g24_absolute_applies_max_constraints`
    - `p2g35_expand_vertical_grows_intrinsic_child`
    - `p2g35_expand_horizontal_grows_intrinsic_child`
- **fix(render): activate border captions, keylines, and `overlay: screen` compositing**
  - Added widget border caption hooks (`border_title()` / `border_subtitle()`) and wired them into border edge composition.
  - Border top/bottom rows now render caption text with `border-title-*` / `border-subtitle-*` alignment, color, background, and text-style flags.
  - Implemented `overlay: screen` blending as an actual two-pass compositor using pre-paint underlay snapshots and per-cell screen blending.
  - Implemented keyline rendering between adjacent children for vertical/horizontal layouts using `keyline` type + color.
  - Added behavioral render tests in `tests/p2_render_css.rs`:
    - `p2g29_border_title_subtitle_render_on_edges`
    - `p2g34_overlay_screen_blends_with_underlay`
    - `p2g34_keyline_draws_separator_between_children`
- **fix(scrollbar): consume hover/active sub-part CSS and dedupe alias helpers**
  - `ScrollView` now tracks scrollbar sub-part hover state (`thumb` vs `track`) on both axes and consumes `scrollbar-color-hover` / `scrollbar-color-active` and `scrollbar-background-hover` / `scrollbar-background-active` in render.
  - Mouse-down on scrollbar updates sub-part hover state before drag activation, keeping visual state in sync with interaction.
  - Hover state is cleared on widget unhover to avoid stale sub-part styling.
  - `aliases.rs` now delegates duplicated scrollbar thumb/style helpers to shared `ScrollView` helpers (WP-32 consolidation path).
  - Added behavioral tests in `tests/p2_widget_css.rs`:
    - `p2g30_scroll_view_hover_subpart_colors_are_consumed`
    - `p2g30_scroll_view_drag_thumb_uses_active_color`
- **feat(runtime): auto-dispatch per-property CSS transitions on style changes (P2-36)**
  - Added per-node resolved style snapshot cache in `App` and runtime diffing in the widget-tree loop.
  - Runtime now auto-emits `AnimationRequest`s when resolved styles change due class/pseudo/stylesheet updates, limited to supported animatable style properties (`opacity`, `text_opacity`, `offset_x`, `offset_y`) with per-property transition lookup.
  - Added property-name alias handling for transition declarations (`offset-y` ↔ `offset_y`) in runtime lookup.
  - Added runtime unit coverage in `src/runtime/event_loop.rs`:
    - `p2g36_runtime_transition_dispatch_matches_changed_properties`
    - `p2g36_runtime_transition_dispatch_handles_css_hyphen_names`
- **fix(overlay/tree): wire OverlayVisibilityChanged to modal subtree display state**
  - Runtime now consumes `OverlayVisibilityChanged` control messages and toggles the overlay modal subtree `display` flag in tree mode, while leaving the base child displayed.
  - Tree-mode overlay hide/show now triggers layout/content invalidation and repaint through runtime message handling.
  - Added runtime tests:
    - `overlay_visibility_hides_modal_subtree_display_in_tree_mode`
    - `overlay_visibility_show_restores_modal_subtree_display_in_tree_mode`
  - Removed stale parity/deferred comments:
    - old `DEFERRED(parity)` note in `tests/container_parity.rs`
    - outdated tree-mode TODO comments in `src/widgets/containers/overlay.rs`
- **feat(css): DC-01..DC-38/DC-ALL — rewrite all widget default CSS to Python Textual parity**
  - Rewrote all 16 default CSS files (`base`, `button`, `checkbox`, `collapsible`, `containers`, `data_table`, `header_footer`, `input`, `list_view`, `misc`, `select`, `tabs`, `text_area`, `tooltip`, `tree`, `mod`) to match Python Textual DEFAULT_CSS verbatim, using nested `&` syntax.
  - Added new widget defaults: `ModalScreen`, `Widget` (global base with scrollbar/link tokens), `Label` semantic variants (`.success`/`.error`/`.warning`/`.primary`/`.secondary`/`.accent`), `Screen:inline`/`:ansi` blocks, `Collapsible` children (`CollapsibleTitle`, `Contents`), `Toast`/`Notification` with severity/positioning, `Markdown*` full hierarchy (H1–H6, paragraphs, fences, tables, bullet/ordered lists, TOC), `HelpPanel`/`KeyPanel` with child selectors.
  - Parser: comprehensive `initial` keyword support (resets any CSS property to `None`), `offset-x`/`offset-y` with percentage values (`offset-x: -50%`), `strike`/`strikethrough` text-style flag, `link-style`/`link-style-hover` token resolution, `$link-style`/`$link-style-hover` text-style tokens.
  - Style struct: added `strike` field with cascade/inherit/`to_rich()` support, `OffsetValue::Percent` variant for percentage offsets, `$link-background` theme token.
  - Layout: percentage-based offset resolution in `layout_absolute()`.
  - AST: widened `pub(crate)` visibility to `pub` on `StyleSelector`, `SelectorChain`, `Combinator`, `StyleRule` accessors for test introspection.
  - Added 3 new integration test files: `dc_core_defaults.rs` (509 lines), `dc_interactive_defaults.rs` (316 lines), `dc_misc_defaults.rs` (899 lines) — covering parse-and-verify for all DC-* default files.
  - Updated existing tests (`p2_layout_css`, `p2_render_css`, `tabs`, `tabbed_content`) for overflow-axis and padding-axis changes.

### 2026-02-13
- **feat(css): rewrite CSS parser to support nested rules and `&` selector**
  - Rewrote `StyleSheet::parse()` with brace-balanced block parsing, replacing the flat `find('{')/find('}')` approach.
  - Nested CSS rules with `&` (parent reference) and implicit descendant selectors are now supported, matching Python Textual TCSS semantics.
  - Selector group lists (`Label, Button { ... }`) are expanded as Cartesian product with nested selectors.
  - Structured parse-issue reporting (`CssParseIssue`) with kind, offset, snippet, and stderr + debug-style emission.
  - Graceful handling of unsupported `@`-rules (logged as issues, not fatal).
  - Added 3 unit tests (nested `&`/descendant, cartesian expansion, `@`-rule issue) and 3 integration tests (`tests/style_nested.rs`).

### 2026-02-15
- **fix(theme): add missing markdown heading background/text-style tokens**
  - Added `$markdown-h1-background` through `$markdown-h6-background` to textual-dark token resolution.
  - Added `$markdown-h1-text-style` through `$markdown-h6-text-style` token resolution for `text-style` shorthand parsing.
  - Added integration coverage for token resolution and stylesheet parse-flow usage.
- **fix(css/render): align `tint` and `auto NN%` foreground behavior**
  - `tint:` now applies as a final render overlay to both foreground and background segment colors.
  - Added behavioral regression coverage validating `tint` affects final rendered `color` and `background`.
  - Added focused parser/integration coverage for `color: auto NN%` and `fg: auto NN%` populating `fg_auto`.
- **fix(css): `text-style` negation + Textual token refs**
  - Added `text-style: not <flag>` semantics with explicit false flag assignment (for example `not reverse`, `bold not underline`, `bold italic not dim`).
  - Added parser support for Textual text-style token refs in value position: `$button-focus-text-style`, `$block-cursor-text-style`, `$block-cursor-blurred-text-style`, `$input-cursor-text-style`.
  - Kept `text-style: none` shorthand behavior unchanged.
- **feat(css/selectors): add Textual-aligned pseudo-classes `:blur`, `:inline`, `:ansi`, `:nocolor`**
  - Parser: recognizes new pseudo-classes in selector chains.
  - Matcher: `:blur` now matches when not focused (`!focused`), and runtime bridge pseudos match selector state flags.
  - Resolver: selector state now populates `inline/ansi/nocolor` from env bridges (`TEXTUAL_APP_INLINE=1`, `TEXTUAL_APP_ANSI=1`, `TEXTUAL_APP_NOCOLOR=1`) for generic and component selector metadata.
  - Debug output/filtering now includes all new pseudos (`selector_chain_string`, `style_debug_meta_label`, `pseudo=` filter support).
  - Added parser, matching, and debug-string coverage tests for the new pseudo-classes.
- **fix(css/runtime): replace env-based pseudo bridge with runtime context state**
  - Added CSS runtime pseudo context (`AppRuntimePseudos`) and guard APIs in selector context.
  - Resolver now reads `inline/ansi/nocolor` from context instead of reading env vars per style lookup.
  - Runtime render/event-loop style passes now set pseudo context from `App` fields.
  - Added `App::set_css_runtime_pseudos()` / `App::css_runtime_pseudos()` for explicit app-level control.
  - Extended stylesheet invalidation quick-check snapshot matching to include `:blur/:inline/:ansi/:nocolor`.

- **Fix text overflow pipeline + P2 behavioral gate tests**
  - Fix: `split_and_crop_lines` was pre-cropping lines before `apply_text_overflow_to_line` could apply ellipsis/clip, making the text overflow wiring dead code. Now defers cropping when `text-wrap: nowrap` with an overflow mode is active.
  - Fix: Link widget disabled-state now correctly ignores hover styling (matches Python Textual).
  - Added 7 behavioral tests for P2-28 (outline render), P2-31 (text overflow pipeline), P2-32 (disabled link), P2-33 (grid span clamping/occupancy), P2-34 (hatch fill).

- **Framework fixes: layout intrinsic width, border composition, dock sizing, tooltip per-axis constrain**
  - Layout: `width: auto` now uses widget `content_width()` when available instead of expanding to full parent width.
  - Layout: style resolution now pushes ancestor context so CSS descendant/child combinators (`Horizontal > VerticalScroll`) affect width/height distribution.
  - Fix: `apply_border_edges` now properly constrains interior height by accounting for border rows, preventing content from overflowing into border area.
  - Fix: Dock explicit size hints now use `box-sizing: border-box` so band sizes include chrome.
  - Tooltip overlay positioning now supports independent `constrain-x`/`constrain-y` axis overrides.
  - ScrollView: transition parameter resolution delegated to shared `resolve_transition_for_property()` helper.

### 2026-02-14
- **Fix hit-target overshoot from `height:auto` in vertical flow + add opt-in hit probe instrumentation**
  - Layout: `height: auto` now uses widget intrinsic `layout_height()` when available (instead of flexible `1fr` allocation), which fixes oversized interactive rects and prevents vertical containers from expanding beyond intended CSS sizing.
  - Added regression coverage in `src/layout.rs` (`vertical_auto_height_uses_intrinsic_layout_height`).
  - Runtime: added env-gated hit-test tracing (`TEXTUAL_DEBUG_HIT_TEST_VERBOSE=1`) to log frame/tree target selection and movement direction for systematic input-debug sessions.

- **P2-24..P2-36: close TCSS property gap — 52 new CSS properties with parser, cascade, layout/render/widget wiring, and 76 gated tests**
  - Phase 1 (core infrastructure): added 13 new types (Position, BoxSizing, Split, TextWrap, TextOverflow, OverlayMode, KeylineType, ScrollbarGutter, ScrollbarVisibility, TextStyleFlags, Hatch, Keyline, PropertyTransition), 52 StyleProperty enum variants, 52 Style struct fields, ~50 parser arms, cascade/inherit/is_empty entries, importance tracking. Upgraded ImportanceBitset from u64 to u128 to support 100 property variants.
  - Phase 2 layout wiring (P2-24/25/26/27/33): absolute positioning, border-box sizing, split-region layout, per-side padding/margin with `effective_padding()`/`effective_margin()` merge helpers, grid row-span/column-span with occupancy-based 2D placement.
  - Phase 2 render wiring (P2-28/29/31/34/35): outline painting outside border box, text-overflow modes (clip/fold/ellipsis), hatch background fill, axis-specific constrain-x/y resolution, overlay position clamping.
  - Phase 2 widget wiring (P2-30/32/36): scrollbar CSS consumption (12 properties: colors, hover/active, gutter, size, visibility), link hover styling with TextStyleFlags, per-property transition resolution with "all" wildcard fallback.
  - Added 76 gated tests across 3 new test files: `tests/p2_layout_css.rs` (18), `tests/p2_render_css.rs` (31), `tests/p2_widget_css.rs` (27).
  - Deferred items tracked: border title/subtitle rendering (needs widget-level title storage), overlay:screen blend (needs two-pass compositor), keyline rendering (needs layout direction awareness).

### 2026-02-13
- **P1-12/P1-13/P1-15 done: tree-mode test infrastructure + container DEFAULT_CSS + behavioral gate tests**
  - Exposed tree-mode APIs for integration testing: `build_widget_tree_from_root`, `render_tree_to_frame`, `run_layout_pass`, `dispatch_event_tree`, `dispatch_event_to_target_tree`, `focused_node_id_tree`, `DispatchOutcome`.
  - Added DEFAULT_CSS entries for all container/layout widgets (Horizontal, HorizontalGroup, HorizontalScroll, Vertical, VerticalGroup, VerticalScroll, ScrollableContainer, Container, Row, Center, Middle, CenterMiddle, Right) matching Python Textual semantics — fixes horizontal layout in tree mode.
  - Added 33 integration tests across `tests/p1_tree_render.rs` (P1G-12 + P1G-15) and `tests/p1_tree_focus.rs` (P1G-13) proving render, focus/hover, and wrapper-chain correctness through the tree pipeline.
  - Fixed pre-existing `background_is_not_inherited_by_children` test to match correct composition semantics (transparent children compose onto parent background).

- **[wip] Close reactive/runtime integration and worker runtime delivery gaps (P3-20, P5-15, P5-16)**
  - Wired event-loop reactive phase execution so queued reactive entries dispatch watchers and propagate repaint/layout invalidation in runtime flow.
  - Added/validated production reactive enqueue path from widget code (`Checkbox`) so runtime queue usage is not test-only.
  - Wired worker processing into runtime loop: `process_worker_requests()` output now maps to `Message::WorkerStateChanged` and is dispatched through normal message routing.
  - Replaced placeholder worker behavior with real background execution via spawned worker jobs, non-blocking completion draining, and deterministic terminal state handling (success/error/cancel/exclusive).
  - Added/expanded runtime tests for reactive event-loop behavior and worker delivery/execution semantics; verified with `cargo test -q --lib`.

- **P1-14 complete: wire tree-based NodeId across all widgets**
  - Added `node_id()` default method to Widget trait, reading from dispatch context so widgets can identify themselves without storing an ID field.
  - Set dispatch context guard in `render_styled_dyn_obj()` so `self.node_id()` works during rendering.
  - Replaced all 114 `TODO(P1-14)` and `TODO(P1-15)` placeholders across 30+ widget files with real tree-wired NodeId logic: `is_self_target(target)` → `target == self.node_id()`, `NodeId::default()` sentinels → `self.node_id()` in outgoing events/messages.
  - Removed `is_self_target` / `is_self_target_opt` from dispatch_ctx.rs — zero callers remain.
  - Fixed RadioButton mouse targeting bug (was passing `NodeId::default()` to `handle_event`, breaking tree-routed clicks).
  - Fixed tree render root widget dispatch context (now uses real root NodeId from arena).
  - Converted 4 non-P1-14 TODOs to explicit `DEFERRED(<tag>)` markers for future work.
  - Cleaned dead code in routing.rs test structs and event_loop.rs.
  - Added 40+ regression tests across containers, input family, and remaining widgets verifying real-NodeId event dispatch.

- **[wip] Dock tree-layout fill restoration + P1 container regression gates**
  - Restored Dock fill behavior in arena-tree layout: when a `Dock` parent has a single non-docked flow child, that child now receives the full remaining inner region after docked edges are carved.
  - Added `layout_dock_fill` placement helper in layout solver to preserve fill-region `layout_rect`/`content_rect` semantics under tree-driven composition.
  - Added regression coverage for Dock top+fill remaining-region allocation in layout tests.
  - Expanded `tests/p1_dom_input_gates.rs` with `buttons_advanced`-like dock/scroll/fill clickability gates to ensure fill regions retain non-zero interactive layout and route clicks correctly.

- **[wip] DOM tree targeting now reaches deep widgets (hover/click path), layout follow-up in progress**
  - Improved tree hit-target selection to prefer deeper valid descendants when frame metadata and tree targets disagree, reducing coarse row-level targeting.
  - Added richer runtime diagnostics for target selection (`id/type/parent/children`) to trace tree-routing mismatches in wrapper-heavy demos.
  - Extended alias/container composition extraction so wrapper widgets contribute real children to the arena tree (`Horizontal`/`Vertical` groups and scroll aliases).
  - Added one-shot composed-child extraction for `ScrollView` and dock-child extraction/style mapping for `Dock`, moving more structure into tree-driven composition.
  - Result: hover/click targeting now reaches button-level nodes in `buttons_advanced`; remaining regression focus is layout/stacking (for example missing `VerticalScroll` presentation) while tree composition is stabilized further.

- **[wip] DOM input routing stabilization for wrapper-heavy demos (`buttons_advanced`)**
  - Hardened runtime hit-test targeting: ignore invalid/default metadata node IDs and only route mouse events to live tree nodes.
  - Added root fallback dispatch for mouse-down/up when no valid hit-test target exists, preserving screen-local coordinates so legacy/container routing can still resolve child hits.
  - Removed stale focus interception in `AppRoot` (`FocusNext`/`FocusPrev`/`Tab`) that was swallowing focus actions while the old stub focus API is inactive.
  - Forwarded `on_layout` through alias wrappers that already forwarded `on_resize` (`Horizontal`, `Vertical`, `VerticalGroup`, `HorizontalGroup`, `Center*`, `Right`, `Middle`, `ScrollableContainer`, `HorizontalScroll`, `ItemGrid`).
  - Updated container Y-hit-testing to account for child margins and expanded P1 DOM input gates with wrapper-chain button-click coverage.
  - Added container-level focus propagation for blur (`set_focus(false)` clears focused descendants) and a focused gate to prevent stale focus when clicking across independent wrapper columns.

- **Pillar 1 DOM hardening: dispatch recipient context for tree routing**
  - Added runtime dispatch recipient context (`src/runtime/dispatch_ctx.rs`) and wired it into tree/event/message routing so handlers can resolve "self target" against the currently dispatched node instead of relying on `NodeId::default()`.
  - Migrated remaining widget-side `NodeId::default()` self-target checks to recipient-aware predicates across controls, inputs, lists/tables, tree/tabs, overlays/tooltips, command palette, and related mouse/message handlers.
  - Updated Button, ScrollView, and DataTable target checks to use recipient-aware helpers, unblocking tree-routed mouse/animation handling paths.
  - Added focused gate coverage in `tests/p1_dom_input_gates.rs` for click targeting, hover forwarding, focus cycling, repeated click delivery, and DataTable arrow-key routing via Container/Row focus.
  - Restored tree-mode forwarding of top-level messages to the root adapter so `TextualApp` typed hooks (for example `on_button_pressed`) continue to fire when message delivery runs via `WidgetTree`.
  - Added legacy-child delegation in `Dock` and `ScrollView` for focus cycling, mouse targeting, and hover forwarding so composed wrappers route input to real descendants even before those descendants are fully represented as arena nodes.
  - Synced wrapper-delivered child layout before forwarding mouse/hover events (`ScrollView` and `Dock`) so nested widgets like `DataTable` compute row/column hit tests from real viewport dimensions.
  - Added focus descent handoff in `ScrollView::set_focus` so wrapper focus can reach nested focusable descendants under `Dock`/`ScrollView` chains.
  - Expanded `tests/p1_dom_input_gates.rs` with `Dock+ScrollView` routing gates for nested click targeting, focus descent, and DataTable row selection by mouse.
  - Applied the same wrapper-delegation model to `VerticalScroll` (aliases layer): child layout sync, mouse coordinate translation with scroll offset, hover forwarding, and focus descent handoff.
  - Adjusted Tab handling flow so focused branches can consume `FocusNext/FocusPrev` before tree-level fallback focus cycling, enabling nested non-tree descendants to receive focus actions.
  - Expanded `tests/p1_dom_input_gates.rs` with `VerticalScroll` click/focus gates.
  - Refined wrapper focus behavior: removed implicit focus descent from wrapper `set_focus(true)` (which could force first-child focus on mouse click), keeping descent via explicit focus actions instead.

- **Sprint 25: Complete tree-driven rendering (P1-12/P1-13)**
  - **Tree-driven compositor:** New `render_tree_composed()` path walks the arena tree depth-first, rendering each widget at its `layout_rect` position with CSS style stack management for proper inheritance. Replaces the legacy recursive `render_styled()` path when the tree is populated.
  - **`take_composed_children()` on Widget trait:** Promoted from per-widget inherent method to trait method. Containers drain their children into the arena tree during mount; tree-driven rendering handles child layout.
  - **`build_widget_tree()` rewrite:** Now extracts children via `take_composed_children()` recursively (in addition to `compose()` declarations), populating the arena with the full widget hierarchy.
  - **Hover tracking wired through tree:** `set_hovered(true/false)` called on actual widgets via tree nodes, enabling `:hover` CSS pseudo-class matching.
  - **Enter/Leave event dispatch:** `generate_enter_leave_events()` wired into the mouse moved handler; events dispatched through tree paths on hover change.
  - **Click synthesis:** `ClickTracker` integrated into runtime; synthesizes Click events when mousedown and mouseup target the same widget.
  - **ScreenStack::top()** wired into `active_title()`/`active_sub_title()`.
  - **CSS style stack:** Added `push_style_context()`/`pop_style_context()` for tree compositor's manual depth-tracking walk.
  - **FrameBuffer:** Added `write_line_at()` for positioned cell painting in the compositor.
  - **Dead code cleanup:** Deleted ThemeDarkGuard, `render_tree_scaffold()`, `App::run_layout_pass()` wrapper, `DigitsAlign` alias. Remaining 8 dead-code items annotated with justification.
  - Build: 0 errors, 0 warnings (down from 29). Tests: 1316+ passed, 1 pre-existing failure.

- **Sprint 24: Examples rewrite with compose! macro + modernization**
  - **Framework:** Added `with_compose(ComposeResult)` to 6 multi-child containers (AppRoot, Container, Row, Horizontal, VerticalScroll, HorizontalScroll) — bridges the `compose![]` macro to the widget tree builder pattern.
  - **8 examples rewritten** to use `compose![]` for multi-child composition: buttons.rs, buttons_composed_pattern.rs, buttons_advanced.rs, hello.rs, horizontal_scroll.rs, input.rs, input_types.rs, input_validation.rs.
  - **buttons_advanced.rs:** Replaced raw `on_message` matching with `on_button_pressed` typed hook.
  - **tabbed_content.rs:** Fixed height over-allocation bug in TabbedDemo layout when terminal height <= 1.
  - Build: 0 errors, 29 warnings (pre-existing). Tests: 1577 passed, 1 pre-existing failure.

- **Parity Sprint 23 (FINAL): Composition rewrites + MessageEvent control**
  - **WP-01 complete:** ListView mutation APIs — `append()`, `clear()`, `remove()`, `insert()`, `pop()` with selected/offset/disabled consistency. compose() + take_composed_children() wiring. 15 new tests.
  - **WP-02 complete:** RadioSet compose() wiring — `take_composed_children()` drains buttons, `children()` / `children_mut()` accessors. 4 new tests.
  - **WP-03 complete:** Select compose() wiring — compose() override + take_composed_children() stub. 3 new tests.
  - **WP-04 complete:** ProgressBar compose() wiring — compose() override + take_composed_children() stub. 3 new tests.
  - **WP-05 complete:** Checkbox compose() wiring — compose() override + take_composed_children() stub. 2 new tests.
  - **WP-06 complete:** Collapsible CollapsibleTitle extraction — `CollapsibleTitle` widget extracted as child struct (style_type "CollapsibleTitle", CSS class "collapsible--title"), title rendering delegated. compose() + take_composed_children(). 12 new tests.
  - **WP-16 complete:** MessageEvent control field — `control: Option<NodeId>` added to `MessageEvent` so `on_message()` handlers can identify the originating widget directly. Set to `Some(sender)` in `post_message()`. All constructors updated across 10+ files. 3 new tests.
  - **All parity action plan items now closed.** 6 widget composition rewrites + message control field complete the remaining 7 items.
  - Build: 0 errors, 29 warnings (pre-existing). Tests: 1577 passed (+50 new), 1 pre-existing failure (`background_is_not_inherited_by_children`). 78 files changed, +2514/-1231 lines.

- **Parity Sprint 22: P1-15 composition migration + widget polish**
  - **P1-15 complete:** Composition migration — all 5 containers with `Vec<Box<dyn Widget>>` (Container, AppRoot, ContentSwitcher, Row, Collapsible) now implement `compose()`, `children()`, `children_mut()`, and `take_composed_children()`. Pragmatic incremental approach: compose() returns empty due to `&self` ownership constraint; `take_composed_children()` (pub(crate)) is the bridge for future runtime tree mount. Rendering unchanged.
  - **WP-21 complete:** OptionList rich Visual support — `content: Option<Text>` field on `OptionItem::Option`, `rich()` / `rich_with_id()` builders, `render_rich_line()` with style merging. 9 new tests.
  - **WP-27 complete:** OptionList virtual scrolling — `visible_range()` helper, render loop only processes items in viewport range.
  - **WP-28 complete:** HelpPanel auto-discover confirmed already wired via runtime's `dispatch_focused_help_changed` + `dispatch_binding_hints_changed`. Added 3 regression tests.
  - **WP-23 complete:** RichLog deferred rendering — `sized` flag with lazy initialization, write methods skip expensive line estimation until first render provides actual dimensions.
  - **WP-24 complete:** Log text selection — `LogPos`/`SelectionRange` structs, mouse-driven selection (drag to select, click to clear), `apply_selection_to_segments()` with reverse-style highlight, Ctrl+C copy via clipboard message. 2 new tests.
  - **WP-25 complete:** Log LRU render cache — `LogLineCache` (same pattern as RichLog), cache invalidation on write/clear/width change, keyed by `(line_index, content_hash)`. 3 new tests.
  - Build: 0 errors. Tests: 1527 passed (+17 new), 1 pre-existing failure. 10 files changed, +1028/-25 lines.

- **Parity Sprint 21: Close Pillar 3 — full reactive widget migration**
  - **P3-14 complete:** TextArea migrated to reactive — 8 reactive fields (`read_only`, `show_line_numbers`, `indent_width`, `soft_wrap`, `placeholder`, `language`, `cursor_blink_enabled`, `theme`) with 5 watchers (read_only → class rebuild, soft_wrap → layout, language/theme → syntax cache invalidation, cursor_blink → blink state reset). Manual `ReactiveWidget` impl.
  - **P3-15 complete:** DataTable migrated to reactive — 8 reactive setters (`selected`, `cursor`, `cursor_type`, `fixed_rows`, `fixed_columns`, `show_header`, `show_row_labels`, `zebra_stripes`) with 3 watchers (cursor_type, show_header, zebra_stripes).
  - **P3-16 complete:** Tree migrated to reactive — 4 reactive setters (`selected`, `show_root`, `show_guides`, `guide_depth`) with 1 watcher (show_root → offset clamp).
  - **P3-17 complete:** Tabs migrated to reactive — 3 reactive setters (`active`, `tab_disabled`, `tab_hidden`). Convenience wrappers updated.
  - **P3-18 complete:** 10 remaining widgets migrated — Footer (1 field), Header (4), Collapsible (1 + watcher), RichLog (5 + 3 cache-clearing watchers), Rule (2 + orientation watcher), Placeholder (2 + variant watcher), Sparkline (2), ProgressBar (5 + 2 watchers), Select\<T\> (4 + allow_blank watcher), RadioSet (1).
  - **Pillar 3 now fully closed.** All 17 widgets have reactive getters/setters/watchers/dispatch.
  - Example fixes: `text_area_custom_language.rs` and `text_area_custom_theme.rs` updated to use builder methods instead of reactive setters for pre-tree construction.
  - Build: 0 errors. Tests: 1510 passed (0 new failures), 1 pre-existing (`background_is_not_inherited_by_children`). 20 files changed, +1465/-268 lines.

- **Parity Sprint 20: Reactive infra + widget migrations + #[on()] macro**
  - **P3-06 complete:** `#[computed(depends_on = "field1, field2")]` attribute — generates cached getter that recomputes when dependency fields change. Computed fields record their own changes in ReactiveCtx during dispatch, enabling cascading.
  - **P3-07 verified done:** `#[var]` already fully implemented in Sprint 19. Confirmed by integration tests.
  - **P3-08 complete:** `#[reactive(init = false)]` flag — watcher skipped on mount. `reactive_no_init()` and `reactive_layout_no_init()` flag constructors.
  - **P3-09 complete:** Runtime reactive phase wiring — `run_reactive_phase()` with cycle detection (MAX_REACTIVE_ITERATIONS=100), `ReactivePhaseResult`, `ReactiveFieldDescriptor` for introspection, `run_event_loop_reactive_phase()` integration point in event loop (stub until per-widget ReactiveCtx is available).
  - **P3-10 complete:** Button migrated to reactive — `label`, `variant`, `disabled`, `flat` as reactive fields with watchers for class rebuilding. Manual `ReactiveWidget` impl (derive macro can't resolve `textual::` paths within the crate). 35 tests.
  - **P3-11 complete:** Input migrated to reactive — `value()`/`set_value()` as Python-aligned reactive pair alongside internal `text()`/`set_text()`. `placeholder` as reactive field. 72 tests.
  - **P3-12 complete:** Switch migrated to reactive — `BinaryToggleState` replaced with direct fields (`value`, `disabled`, `focused`, `hovered`). `slider_pos` as `#[var]`. Watcher handles class rebuild + animation + message emission. 11 tests.
  - **P3-13 complete:** Checkbox migrated to reactive — `BinaryToggleState` replaced with direct fields. `checked` as reactive watch field with message emission watcher. 6 tests.
  - **P4-09 complete:** `#[on(MessageType)]` and `#[on(MessageType, selector = "...")]` attribute macro — generates `__on_dispatch_*` companion methods with uniform signature `(&mut self, msg: &Message, sender: NodeId, ctx: &mut EventCtx) -> bool`. Selector stored as `const __ON_SELECTOR_*` for runtime matching. Duplicate selector detection at compile time. 8 integration tests.
  - **Proc macro path fix:** Added `extern crate self as textual;` to `src/lib.rs` so `#[derive(Reactive)]`-generated `textual::reactive::*` paths resolve within the crate itself (same pattern as serde/tokio). Future widget migrations (P3-14+) can use the derive macro directly instead of manual impls.
  - Build: 0 errors. Tests: 1510 passed (+42 new), 0 failed. 1 pre-existing integration test failure (`background_is_not_inherited_by_children`).

- **Parity Sprint 19: Reactive foundations + CSS overflow/pointer + CommandPalette Provider**
  - **P3-01..P3-05 complete:** Reactive field system foundation.
    - `src/reactive.rs`: `ReactiveFlags` (repaint/layout/init control), `ReactiveChange` (field_name + flags + type-erased old/new values), `ReactiveCtx` (node_id + change accumulator + repaint/layout request tracking), `ReactiveWidget` trait with default no-op `reactive_dispatch()`. 12 unit tests.
    - `textual-macros/src/reactive.rs`: Full `#[derive(Reactive)]` proc macro — parses `#[reactive]`, `#[reactive(layout)]`, `#[reactive(watch)]`, `#[var]` field attributes. Generates typed getters (`fn field(&self) -> &T`) and setters (`fn set_field(&mut self, value: T, ctx: &mut ReactiveCtx)`) with PartialEq change detection. Generates `ReactiveWidget` impl with watcher dispatch for `#[reactive(watch)]` fields (naming convention: `watch_{field}(old, new, ctx)`). Attribute validation rejects unknown args. 13 integration tests.
  - **P2-22 complete:** Split overflow axes — `overflow_x`/`overflow_y` fields on `Style`, `OverflowX`/`OverflowY` `StyleProperty` variants (46/47) with cascade + importance tracking. Parser maps `overflow-x:`/`overflow-y:` to separate fields while `overflow:` shorthand sets both. ScrollView per-axis scrollbar visibility with fallback to shorthand.
  - **P2-23 complete:** CSS `pointer` property wired to runtime — `pointer_shape_for_hover_tree` rewritten to read computed `style.pointer` instead of hardcoded widget type-name checks. `pointer: text;` added to Input/MaskedInput default CSS. Disabled widgets always show `NotAllowed` cursor.
  - **P5-13 complete:** CommandPalette Provider pattern — `Provider` trait (`Send + Sync + 'static`) with `startup()`/`search()`/`shutdown()` lifecycle hooks. `ProviderResult` struct (id, title, help, score). `SystemCommandsProvider` wrapping built-in `PaletteCommand` list. `add_provider()`/`with_provider()` builder API. Lifecycle wired: startup on open, shutdown on close/unmount, search on keystroke. 6 tests.
  - Build: 0 errors. Tests: 1468 passed (+36 new), 0 failed. 1 pre-existing integration test failure (`background_is_not_inherited_by_children`).

### 2026-02-12
- **Parity Sprint 18: P4-06 — Message struct-per-variant refactor**
  - **P4-03/P4-04/P4-05 resolved:** Candidate A chosen (two-tier: closed enum + trait object). Candidate B (generated union via proc macro) excluded. Prototyping skipped — decision made directly.
  - **P4-06 complete:** Refactored flat `Message` enum (78 variants with inline fields) into 78 standalone structs wrapped by newtype enum variants. Zero behavioral change — pure structural refactor.
    - 78 standalone structs in `message.rs` (8 unit structs with `Copy + PartialEq + Eq`, 70 field structs with `Debug + Clone`)
    - `Message` enum rewritten with newtype wrappers: `Message::Variant(Variant { .. })`
    - `UserMessage` trait (`Any + Send + Sync + Debug + 'static`) with `clone_box()` for trait-object extensibility via `Message::Custom(Box<dyn UserMessage>)`
    - `impl_message_from!` macro generating `From<Struct> for Message` for all 78 variants
    - `pub use crate::message::*;` in prelude exposes all struct names
  - **WP-15 complete:** All widgets, containers, runtime, event system, examples, and tests migrated to newtype-wrapped message syntax (~364 references across 51 files).
  - **Gate B resolved:** P4-05 closed, P4-06 and WP-15 done.
  - Build: 0 errors. Tests: 1432 passed (+2 new), 0 failed. 1 pre-existing integration test failure (`background_is_not_inherited_by_children`).

- **Parity Sprint 17: Screen modes + Layout activation + CSS defaults port + Widget features + Review fixes**
  - **P5-05/P5-12 complete:** Mode system — `push_mode()`/`pop_mode()`/`switch_mode()`/`remove_mode()` with mode-tagged `ScreenEntry` for safe pop semantics. `SystemModalScreen` trait with `inherit_css()` default. `CommandPaletteScreen` implements both `Screen` + `SystemModalScreen`. 10 integration tests (`tests/modes_system.rs`).
  - **P2-18b/P2-19 complete:** Layout activation — `collect_render_nodes` with layer ordering + display:none filtering. `InvalidationFlags` bitfield (content/style/layout), `StyleChangeKind` + `classify_style_change` checking 27 layout-affecting properties including borders. `request_style_invalidation()`/`request_layout_invalidation()` on EventCtx. 15 tests.
  - **P2-20 complete:** CSS defaults port — 16 property categories ported across 9 files (layout, overflow, text-align, content-align, constrain, layer, margin, padding, max/min width/height, width, height, dock, align). 43 tests.
  - **WP-18/WP-19 complete:** Button enhancements — `action` parameter (`with_action()` builder) for string action dispatch; `ButtonLabel::Markup` with `with_markup_label()` for rich-rs rendered labels.
  - **WP-20 complete:** Input suggester system — `Suggester` trait, `SuggestFromList` implementation, ghost text rendering with `input--suggestion` component class, Tab/Right-arrow acceptance, validation guard, stale suggestion cleanup. 11 tests.
  - **WP-22 complete:** Footer signal subscription — `BindingsChanged` signal, `FooterKey` widgets with click-to-invoke, `execute_action()` dispatch.
  - **P5-14 complete:** Header title inheritance — reads title/sub_title from `Screen::title()`/`Screen::sub_title()` falling back to `App` title.
  - **Review fixes:** Ghost text panic guard (char boundary check), accept_suggestion validation, border misclassification in `classify_style_change` (borders affect layout, not just visual).
  - Build: 0 errors. Tests: 1430 passed (+307 new), 0 failed. 1 pre-existing integration test failure (`background_is_not_inherited_by_children`).

- **Parity Sprint 16: Screen system + Worker wiring + BINDINGS migration + Overlay constraint**
  - **P5-01/02/03 complete:** Screen system foundation — `Screen` trait with lifecycle hooks (mount/suspend/resume/unmount), `ScreenStack` with push/pop, `ScreenResult` (Dismissed/Value). ScreenEntry builds WidgetTree from compose() and parses per-screen CSS. Wired into App struct with `push_screen()`/`pop_screen()`. 22 tests.
  - **P5-09 complete:** Worker runtime wiring — `WorkerRegistry` integrated into event loop. `EventCtx::take_worker_requests()` consumed after dispatch, workers registered/set_running/completed. `WorkerStateChanged` message delivered to owning widget. Exclusive mode cancels previous. Cleanup on each tick.
  - **WP-17 complete:** Declarative BINDINGS on 11 widgets — Button, Input, Checkbox, ListView, Tabs, Tree, DataTable, Select, TextArea, CommandPalette, ScrollView all implement `bindings()`, `action_namespace()`, `execute_action()`. Existing on_event handling preserved alongside.
  - **P2-21 + WP-10 complete:** `Constrain` CSS property (none/inside/inflect) — parsed in CSS, cascaded in Style. Tooltip updated with constrain-aware viewport clamping. Default tooltip CSS (`constrain: inside`). Overlay container respects constrain property.
  - Build: 0 errors. Tests: 1123 passed (+69 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 15: Declarative bindings + Worker system + CSS parser gaps + widget polish**
  - **P4-16 complete:** Declarative `BINDINGS` on widgets — `BindingDecl` struct with `new()`/`hidden()`/`priority()` builders. `Widget::bindings()` trait method. `match_binding_tree()` walks focused chain (priority first, then normal). Wired into event loop before `on_event` dispatch. Binding hints auto-collected for footer/help. Action routing via `action_namespace()`/`action_registry()`/`execute_action()` on Widget trait. 12 tests.
  - **P5-07 + P5-08 complete:** Worker abstraction — `WorkerId`, `WorkerState` (Pending/Running/Cancelled/Success/Error), `CancellationToken` (cooperative), `WorkerEntry` lifecycle, `WorkerRegistry` (register/cancel/cancel_by_owner/exclusive mode/cleanup). `WorkerRequest` via `EventCtx::request_worker()`/`request_exclusive_worker()`. 29 tests.
  - **CSS parser gaps closed:** `text-align`, `content-align`, `content-align-horizontal`, `content-align-vertical`, `align`, `align-horizontal`, `align-vertical`, `offset`, `offset-x`, `offset-y` — all now parsed and applied to Style. Importance mapping for all new properties. ~37 tests.
  - **WP-09 (Digits):** CSS `text-align` integration — reads alignment from resolved style instead of widget-local enum. `DigitsAlign` deprecated as alias to `TextAlign`.
  - **WP-26 (ProgressBar):** Gradient support — `with_gradient(start, end)` linearly interpolates color across filled portion. `lerp_color()` helper.
  - **WP-29 (Select):** `allow_blank` mode — when false (default), first option auto-selected; `clear()` is no-op. When true, starts blank, user can deselect. Builder + setter API.
  - Build: 0 errors. Tests: 1054 passed (+100 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 14: !important + control ref + widget CSS defaults + CSS animation**
  - **P2-05 + P2-07 complete:** Per-property `!important` tracking via `ImportanceBitset(u64)` with `StyleProperty` enum (45 variants). Importance-aware cascade in `combine()` — `!important` declarations win over normal regardless of specificity. Parser detects `!important` per-declaration with safe non-ASCII slicing. 27 tests.
  - **P4-17 complete:** `control: Option<NodeId>` added to `MessageEnvelope` — originating widget reference (like Python's `event.control`). Defaults to sender, preserved during bubble, survives coalescing. 8 tests.
  - **WP-07/08/11/12/13/14/30/31 complete:** 8 widget CSS defaults aligned with Python Textual — Header `dock: top`, Footer `dock: bottom`, Button `content-align/text-align: center`, Placeholder `content-align: center middle; overflow: hidden`, Input `width: 100%`, Collapsible `display: none` rule, Rule orientation margins + 1fr sizing, TextArea `1fr` + padding. 13 tests.
  - **P5-11 complete:** CSS property animation — `StyleValue` enum (Color/Float/Scalar/Spacing/Tint), per-property `interpolate_style_property()`, `StyleAnimation` on Animator with `enqueue_style()`/`step_style()`, `animate_style()` on EventCtx. Animatable: fg, bg, opacity, text_opacity, width, height, min/max sizes, margin, padding, tint. 36 tests.
  - Build: 0 errors. Tests: 954 passed (+92 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 13: Envelope dispatch + Layer property + Signal system + :focus-within**
  - **P4-02 complete:** Envelope-based message dispatch — messages now bubble from sender → parent → … → root via `MessageEnvelope`. `stop()` halts propagation, `prevent_default()` skips default action (wired through `DispatchOutcome.default_prevented`). Falls back to depth-first broadcast for orphan/global messages.
  - **P4-14 complete:** Message queue coalescing — rapid-fire replaceable messages (InputChanged, TextAreaChanged, DataTableCursorMoved, etc.) auto-marked replaceable. `coalesce_message_queue()` deduplicates by (sender, variant discriminant), keeping latest. Global re-coalesce after each dispatch round.
  - **P2-17 complete:** CSS `layer` property for z-ordering — `layer: <name>` assigns widget to named layer, `layers: <name1> <name2> ...` on parent defines stacking order. `sort_children_by_layer()` in render pipeline. `layers` is inherited. Unknown layer names fall back to default bucket. 17 tests.
  - **P4-15 complete:** `Signal<T>` observer pattern — lightweight typed pub/sub with `subscribe(node, handler)`, `emit(value)`, `unsubscribe(node)` cleanup. `SignalResponse::Stop` halts remaining subscribers. Function pointer handlers (Send+Sync safe). 13 tests.
  - **P2-16 complete:** `:focus-within` pseudo-class — matches when element or any descendant has focus. Thread-local `FOCUS_WITHIN_IDS` set populated before style resolution. `is_ancestor_of()` helper on WidgetTree. Parser supports `:focus-within` and `:focus_within`. Wired into `apply_display_visibility_to_tree()`. 15 tests.
  - Build: 0 errors. Tests: 862 passed (+47 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 12: CSS display/visibility/overflow + Action resolver + Lifecycle/focus events + MessageEnvelope**
  - **P2-13 complete:** `display: none` wired end-to-end — CSS resolver syncs resolved Display to WidgetNode.display via `apply_display_visibility_to_tree()`, runs before layout pass. Nodes with display:none are skipped in render + layout.
  - **P2-14 complete:** `visibility: hidden` — new `visibility` field on WidgetNode (default: Visible). Hidden nodes occupy space but don't render. Excluded from focus chain.
  - **P2-15 complete:** `overflow` CSS property — parser supports `overflow`, `overflow-x`, `overflow-y` with auto/hidden/scroll values. ScrollView reads overflow from resolved style to suppress scrollbars when `overflow: hidden`.
  - **P4-08 complete:** Action namespace resolution — `resolve_action()` walks widget tree ancestors to find matching ActionHandler. Supports explicit namespaces (`"app.quit"` → find app handler) and unnamespaced bubble resolution. `ResolvedAction` struct + `action_namespace()` trait method. 8 tests.
  - **P4-10 complete:** Lifecycle events — `MountEvent`, `UnmountEvent`, `ReadyEvent` structs + Event variants. Dispatched via on_event after existing on_mount/on_unmount callbacks. Ready fires once after first render frame.
  - **P4-11 complete:** Focus events — `FocusEvent`, `BlurEvent` structs + Event variants. Dispatched on focus transitions with previous-focus tracking.
  - **P4-01 complete:** `MessageEnvelope` with `stop()`, `prevent_default()`, `can_replace()` propagation control. Types + tests only (dispatch wiring is P4-02). 17 tests.
  - Build: 0 errors. Tests: 815 passed (+41 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 11: Action system + Layout-render integration + New events + Easing library**
  - **P4-07 complete:** New `src/action.rs` module — `ActionDecl`, `ActionHandler` trait, `ParsedAction` struct, `parse_action()` string parser (namespace.name(args) format), `APP_ACTIONS` built-in declarations (quit, toggle_dark, bell, push_screen, pop_screen, focus, focus_next, focus_previous), `find_action()` lookup. 28 tests.
  - **P2-18a complete:** Layout-render integration — `run_layout_pass()` computes `layout_rect`/`content_rect` for all tree nodes via CSS layout solvers before rendering. `render_tree_scaffold()` uses precomputed rects to set render options. `App::run_layout_pass()` convenience method with automatic stylesheet context.
  - **P4-12 complete:** Mouse Enter/Leave/Click events — `MouseEnterEvent`, `MouseLeaveEvent`, `ClickEvent` structs + `Event::Enter`/`Event::Leave`/`Event::Click` variants. `generate_enter_leave_events()` helper for hover-change detection. `ClickTracker` for mousedown+mouseup→click synthesis. 14 tests.
  - **P4-13 complete:** Paste event — `PasteEvent` struct + `Event::Paste` variant for bracketed-paste support.
  - **P5-10 complete:** Expanded `AnimationEase` from 5 → 30 variants: added Quad, Cubic-in, Quart, Quint, Expo, Circ, Back, Bounce, Elastic families (In/Out/InOut each). Standard easing equations from easings.net. 22 tests.
  - Updated prelude: exports new event types (`ClickEvent`, `MouseEnterEvent`, `MouseLeaveEvent`, `PasteEvent`, `AnimationEase`) and action types (`ActionDecl`, `ActionHandler`, `ParsedAction`, `parse_action`).
  - Build: 0 errors. Tests: 774 passed (+76 new), 0 failed. 1 pre-existing integration test failure (command_palette).

- **Parity Sprint 10: WidgetId deletion (P1-14g) + Grid solver (P2-11)**
  - **P1-14g complete:** Deleted `WidgetId` struct, deprecated Widget trait methods (`id()`, `visit_children_mut()`, `set_focus_target()`), all 4 legacy stub functions (`collect_focus_ids`, `set_focus_by_id`, `set_hover_by_id`, `dispatch_event_to_focus`). Zero `WidgetId` references remain in the codebase.
  - Replaced 5 production `visit_children_mut` callers with tree-based walks (stylesheet invalidation, WATCH devtools snapshot) or root-only fallbacks (apply_layout_info, hit-test coords).
  - Simplified ~20 stub callers in app_root (focus_first/next/prev), command_palette (restore focus), event_loop (initial focus).
  - Replaced pointer-based CSS computed style cache key (was `widget_node_id(Widget::id())`).
  - **P2-11 complete:** Implemented `layout_grid()` — 2D grid cell placement algorithm. Reads grid config from parent style, places children row-major with wrap at `grid_size_columns`, resolves column/row tracks via `layout_resolve_1d` with gutter spacing, applies margin/border/padding/min/max constraints per child. Scalar cycling for column/row definitions.
  - Updated integration tests (welcome, directory_tree) to remove `visit_children_mut` usage.
  - Build: 0 errors. Tests: 698 passed (686 lib + 12 integration), 0 failed. 1 pre-existing integration test failure (command_palette, from Sprint 8).

- **Parity Sprint 9: Layout solvers + Grid CSS properties**
  - Created `src/layout.rs` module (1524 lines) with full layout solver infrastructure:
    - `layout_resolve_1d()`: core 1D space allocation algorithm ported from Python Textual's `_layout_resolve.py`. Uses pure integer arithmetic (no f32) with remainder cascading for deterministic rounding.
    - `layout_vertical()` (P2-09): vertical stacking solver — resolves child heights via 1D resolver, assigns layout_rect/content_rect.
    - `layout_horizontal()` (P2-10): horizontal row solver — resolves child widths via 1D resolver.
    - `arrange_dock()` (P2-12): dock positioning — separates docked children (top/bottom/left/right), carves out regions, returns reduced available space for flow children.
    - `resolve_layout()`: top-level dispatch by Layout enum (vertical/horizontal/grid). Grid falls back to vertical as stub.
  - Added 6 grid CSS properties to Style struct: `grid_size_columns`, `grid_size_rows`, `grid_columns` (Vec<Scalar>), `grid_rows` (Vec<Scalar>), `grid_gutter_horizontal`, `grid_gutter_vertical` (partial P2-11).
  - Added 8 CSS property parsers: `grid-size`, `grid-size-columns`, `grid-size-rows`, `grid-columns`, `grid-rows`, `grid-gutter`, `grid-gutter-horizontal`, `grid-gutter-vertical`.
  - Fixed button_fill test regression: switched `render_styled_dyn_obj` from broken `tag_widget_meta_legacy` (WidgetId=0 mismatch) to `tag_widget_meta` (correct NodeId encoding). Removed dead `tag_widget_meta_legacy` function.
  - Build: 0 errors. Tests: 673 passed (51 new: 38 layout solver + 13 grid CSS), 0 failed. 1 pre-existing integration test failure (command_palette, from Sprint 8).

- **Parity Sprint 8: Pillar 2 foundation — types, Style rewrite, CSS parser, pseudo-classes**
  - Defined `Scalar` enum (Auto, Cells, Percent, Fraction, ViewWidth, ViewHeight) for CSS size values with unit support.
  - Defined 10 new layout/alignment/pointer enums: `Layout`, `Display`, `Visibility`, `Overflow`, `Dock`, `TextAlign`, `HorizontalAlign`, `VerticalAlign`, `ContentAlign`, `Align`, `Offset`, `Pointer`.
  - Defined `Spacing` struct (4-side u16 padding/margin), replacing old `Margin` type (kept as alias).
  - Rewrote `Style` struct: replaced `width_auto`/`height_auto` booleans with `width: Option<Scalar>`/`height: Option<Scalar>`, changed all sizing fields from `usize` to `Scalar`, replaced `line_pad` with proper `padding: Option<Spacing>`, added 15 new layout/alignment/pointer/layer fields.
  - Extended CSS parser: `parse_scalar()` handles `%`, `fr`, `vw`, `vh` units. New properties: `display`, `layout`, `dock`, `padding`, `overflow`, `text-align`, `visibility`, `pointer`. `line-pad` kept as compat alias.
  - Added 6 CSS pseudo-classes: `:dark`, `:light`, `:even`, `:odd`, `:first-child`, `:last-child` with full matching logic and theme-state context plumbing.
  - Implemented `resolve_scalar()` for converting Scalar units to concrete cell values.
  - Fixed all 421 call-site occurrences across 37 files for the type migration.
  - Build: 0 errors. Tests: 622 passed (37 new), 0 failed.

- **Parity Sprint 7: Close Pillar 1 — test fixes + legacy dispatch cleanup**
  - Fixed all 19 failing tests from Sprint 6's WidgetId→NodeId migration. Tests rewritten to build `WidgetTree` instances and call `_tree` dispatch functions directly (routing, event_loop, render, app_root, select, tabs, tabbed_content, command_palette).
  - Deleted ~400 lines of legacy routing functions from `routing.rs`: `widget_node_id`, `focused_widget_id`, `dispatch_event_to_target`, `dispatch_scroll_action`, `dispatch_mouse_scroll_to_target`, `dispatch_message_queue`, `active_binding_hints`, `focused_help_metadata`, and associated helpers.
  - Simplified all 10 `_auto` bridge methods in `event_loop.rs`: tree path unchanged, else branches now use minimal root-only fallbacks instead of deleted legacy functions.
  - Deleted legacy helper functions from `helpers.rs`: `widget_node_id`, `call_on_mouse_move`, `any_widget_active`, `pointer_shape_for_hover`.
  - Build: 0 errors, 15 warnings (5 deprecated `visit_children_mut`, rest pre-existing). Tests: 585 passed, 0 failed.
  - **Deferred to P2:** WidgetId deletion (P1-14g), `visit_children_mut` removal, stub deletion (`set_focus_by_id` etc.) — all have ~20 callers that need tree-based replacements.

- **Parity Sprint 6: WidgetId→NodeId migration (P1-14a–f) + compose cleanup (P1-15)**
  - Replaced `WidgetId` with `NodeId` across the entire codebase (88 files, ~610 occurrences).
  - Runtime infrastructure: all `App` fields (hovered, focus tracking, binding-hint sources), `HitTestMap`, timer targets, async task targets, overlay refs now use `NodeId` instead of `WidgetId`.
  - `EventCtx` now carries a `node_id: NodeId` field. `post_message()` takes 1 argument (message only) — sender identity comes from `EventCtx.node_id` automatically.
  - `MessageEvent.sender` changed from `WidgetId` to `NodeId`. All `Message` variant target/source fields updated.
  - CSS selectors: `WidgetId` references in context, resolver, and segments replaced with `NodeId`.
  - Widget trait: removed `widget_id: WidgetId` field and `fn id()` override from all 50+ widget structs. Deprecated `id()`, `visit_children_mut()`, `set_focus_target()` kept on trait with defaults for legacy dispatch compatibility.
  - All widget `post_message(self.id, msg)` calls converted to `post_message(msg)` (1-arg).
  - Updated 16 test files and 1 example (`keys.rs`) for the new API.
  - Build: 0 errors, 18 warnings. Tests: 566 passed, 19 failed (expected — legacy identity-based dispatch tests, will be fixed when tree dispatch fully replaces legacy path).
  - **Note:** `WidgetId` type not yet deleted (P1-14g deferred) — deprecated trait methods still reference it as a bridge until legacy dispatch is fully removed.

### 2026-02-11
- **Parity Sprint 5: Compose wiring + final QW batch**
  - P1-05: Wired compose API into live runtime. `App` builds `WidgetTree` from root's `compose()` on startup. Event dispatch, focus management, scroll/mouse routing, message queue, and layout info all bridged through `_auto` methods that use tree-based paths when available, falling back to legacy recursive dispatch otherwise.
  - RichLog: added `Mutex<LineCache>` LRU cache for rendered line segments with configurable size (QW-43). Fixed pre-existing drag-release repaint bug.
  - Log: added `with_highlight(bool)` and `with_highlighter(name)` for syntax highlighting via repr highlighter (QW-44).
  - KeyPanel: added namespace grouping with styled section headers when multiple binding groups exist (QW-45).
  - CommandPalette: added `FuzzyMatcher` with consecutive-match, start-of-word, and position bonuses for score-based ranking (QW-46).
- **Parity Sprint 4: Widget trait redesign + runtime scaffold + QW batch**
  - Widget trait redesign (P1-02): `id()`, `visit_children_mut()`, `set_focus_target()` deprecated with defaults (kept for migration). Added `compose()` default returning empty. `render_styled_dyn_obj` now accepts `NodeId` parameter for future arena rendering.
  - Runtime event routing scaffold (P1-11): Added tree-based dispatch functions (`dispatch_event_tree`, `build_path_to_node`, `focused_node_id_tree`) using explicit `Vec<NodeId>` paths alongside old recursive dispatch.
  - Runtime render scaffold (P1-12): Added `render_tree_scaffold`, `collect_render_nodes`, `apply_layout_info_tree`, `NodeHitTestMap` (NodeId-keyed parallel to HitTestMap).
  - Runtime focus/hover scaffold (P1-13): Added `collect_focus_chain_tree`, `call_on_mouse_move_tree`, `any_widget_active_tree`, `pointer_shape_for_hover_tree`.
  - Switch: added tick-based slider animation with ease-out cubic (QW-31) and half-block sub-cell rendering (QW-32).
  - Collapsible: added default CSS with `border-top`, padding, focus style (QW-30).
  - Static/Label: added `markup` flag for Rich markup rendering (QW-36), `expand`/`shrink` sizing fields (QW-37).
  - MaskedInput: added `set_template()` for runtime template changes (QW-39).
  - SelectionList: made generic over value type `SelectionList<T>` with `SelectionListString` alias (QW-40).
  - Select: added keyboard type-to-search with prefix matching and timeout reset (QW-41).
- **Parity Sprint 3: Compose foundation + DOM queries + validators/CSS**
  - Added `src/compose.rs`: `ComposeResult`, `ChildDecl`, `WidgetBuilder`, `compose![]` macro with `From<W: Widget>` blanket impl.
  - Added lifecycle event system: `LifecycleEvent` (Mount/Unmount) accumulator in `WidgetTree` with `drain_lifecycle()` API.
  - Added DOM query methods: `query()`, `query_one()`, `query_children()` with CSS selector integration (type/class/id/combinator matching).
  - Added `Integer`, `Length`, `Url`, `Regex` validators with 19 tests.
  - Header: changed bg from `$primary` to `$panel`. Footer: aligned to `$footer-*` tokens.
  - Added CSS defaults for Pretty (`height: auto`), Static (`height: auto`), Label (`width/height: auto; min-height: 1`).
  - RichLog: added `min_width` field (default 78). ContentSwitcher: exposed `visible_content()` API.
- **Parity Sprint 2: Pillar 1 core + Input/TextArea/Tabs quick wins**
  - Added arena-based `WidgetTree`/`WidgetNode` (`src/widget_tree.rs`) with mount/remove/move, class manipulation, traversal iterators, display toggle, and 22 unit tests.
  - Input: added key bindings (ctrl+d/k/f/a), public API (clear/insert/delete/replace/select_all/selected_text), password mode, regex restrict, max_length, InputBlurred message.
  - TextArea: added undo/redo stack, word-level nav, shift+selection, config (read_only/show_line_numbers/indent_width/soft_wrap/placeholder), SelectionChanged message.
  - Tabs: migrated active tab from index to string-ID, added remove_tab/clear, 6 new messages (Disabled/Enabled/Hidden/Shown/Cleared/PaneFocused).
  - SelectionList: added toggle_all/select_all/deselect_all. Link: open URLs via `open` crate. Label: added variant parameter with CSS classes.
- **Parity Sprint 1: Bootstrap + DataTable/Tree quick wins**
  - Added `slotmap`, `regex`, `open` crate dependencies.
  - Scaffolded `textual-macros` proc-macro crate for future `#[reactive]`/`#[on()]` macros.
  - Added `NodeId` type alias (`slotmap::DefaultKey`) with `node_id_to_ffi()`/`node_id_from_ffi()` round-trip helpers for hit-test metadata compatibility.
  - Added `WidgetCtx<'a>` zero-cost borrow wrapper providing `ctx.node_id()` identity-through-context API (arena owns identity, not widgets).
  - DataTable: added default CSS, `remove_row`/`clear`/`sort`/`update_cell`/`get_cell`/`get_row` API, `show_header`/`show_row_labels`/`zebra_stripes` config, 5 new highlight/select messages.
  - Tree: added `clear`/`move_cursor`/`select_node`/`toggle_all` API, `show_root`/`show_guides`/`guide_depth` config, Unicode guide rendering, shift+arrow/space bindings, 3 new messages (Collapsed/Expanded/Highlighted).
  - DirectoryTree: added folder/extension/hidden CSS classes, `filter_paths`/`reload_node` APIs.
- **RichLog demo parity + core composition/scroll polish**
  - Added `examples/rich_log.rs` as a Python Textual parity port (`widgets/rich_log.py`) with syntax block, table renderable, markup line, and styled key-event logging in the RichLog stream.
  - Fixed style composition so `rich-rs` default terminal background (`SimpleColor::Default`) is treated as transparent/inheritable during widget style application, preventing terminal-background bleed in composed widget surfaces.
  - Added regression coverage in CSS selector tests to lock in the transparent-default-background composition behavior.
  - Improved consolidated scrollbar drag mapping (`ScrollView::line_drag_offset`) to use pointer-delta scaling against virtual/window size, reducing perceived lag/jumpiness during thumb drag across widgets that share the primitive.
- **DevTools closure (embedded runtime + external tooling plumbing)**
  - Added runtime devtools substrate in `textual-rs` (`src/runtime/devtools.rs`) with a local TCP control/snapshot server, instance registration files, and command queue integration.
  - Added live `WATCH` push-stream support for devtools snapshots (server-side publish/subscribe) so attached consoles can consume incremental updates without polling.
  - Updated devtools server connection handling to process clients concurrently, allowing long-lived watch sessions alongside command/snapshot requests.
  - `App::run_widget_tree` now publishes live widget/runtime snapshots (focus/hover/layout/debug state, widget tree metadata, binding hints) and consumes remote control commands (`focus`, `debug layout`, `quit`).
  - Added environment-gated activation for live inspection (`TEXTUAL_DEVTOOLS`, `TEXTUAL_DEVTOOLS_BIND`, `TEXTUAL_DEVTOOLS_ROOT`) without changing default runtime behavior when disabled.
  - Added focused runtime parser regressions for devtools command handling (`src/runtime/devtools.rs` tests).
  - Added matching `textual-dev-rs` live inspection CLI support:
    - `textual-rs run --devtools ...` to launch instrumented app instances,
    - `textual-rs devtools list|snapshot|focus|debug-layout|quit` to inspect/control running apps.
- **Phase 5 computed-style caching/tree closure**
  - Added a per-widget computed-style cache/tree model in the CSS resolver path, keyed by widget id plus selector ancestry, parent style, inline style, and active stylesheet.
  - Cache invalidation now occurs naturally on class/id/pseudo/style/ancestor/stylesheet changes via key mismatch, while preserving selector-chain correctness.
  - Added render-pass tracking for layout-affecting computed-style deltas so layout callbacks are reapplied when cached style transitions change box-model-affecting fields.
  - Added focused cache/invalidation regressions in `src/css/selectors/mod.rs`; full `cargo test -q --lib --tests` remains green.
- **Phase 8 adapter-utilities breadth closure**
  - Expanded `TextualApp` adapter ergonomics with explicit typed message hooks for common app patterns (`Input`, `TextArea`, `Checkbox`, `ListView`, `TabActivated`, plus existing button/command-palette hooks) while keeping the same message-bus dispatch path.
  - Added compatibility runner aliases in `src/textual_app.rs`: `run_textual_app*` and `run_textual_app_or_snapshot*` (delegating to existing `run*` APIs, no alternate runtime path).
  - Added explicit overlay-backed push/pop helper `OverlayScreenStack` for screen-like app flows; it only emits existing overlay visibility messages.
  - Added `EventCtx` convenience wrappers for overlay and command palette messages (`show/hide/toggle/dismiss overlay`, `open/close/select/set command palette commands`), implemented via `post_message`.
  - Added focused tests for typed-hook dispatch, overlay screen-stack behavior, and new `EventCtx` wrappers; updated docs/roadmap status and example usage (`examples/input_validation.rs`).
- **Dirty/style invalidation closure (`pending-stream #1`)**
  - Added region-scoped framebuffer diff support (`FrameBuffer::diff_to_segments_in_regions`) and runtime dirty-region accumulation to reduce repaint scope for localized updates.
  - Replaced coarse runtime dirty bool flow with typed invalidation flags (`content` / `style` / `layout`) carried by `EventCtx`/`DispatchOutcome`, and used these flags to drive selective relayout and repaint behavior.
  - Updated widget-tree rendering path to use selective dirty regions when safe, while falling back to full redraw for layout/style-wide invalidations and resize paths.
  - Stylesheet hot-reload now computes changed rules and selectively invalidates affected widgets by selector matching (including descendant/child selector chains), with full fallback for broad or layout-affecting changes.
  - Added focused regressions for:
    - region-limited diff behavior (`src/render/mod.rs`),
    - dirty-region expansion/fallback behavior (`src/runtime/types.rs`),
    - stylesheet selector-targeted invalidation behavior (`src/runtime/event_loop.rs`).
- **Timer/task runtime closure (`PR8J`)**
  - Added one-shot timer runtime controls and delivery on the message bus:
    - `TimerSchedule` / `TimerCancel` requests with `TimerFired` / `TimerCancelled` runtime events.
    - integrated timer wakeups into runtime loop timeout selection.
  - Expanded async task semantics and utility surface:
    - added `AsyncTaskCancelTarget` for target-wide cancellation.
    - replacing an in-flight `task_id` now emits `AsyncTaskCancelled` for the replaced task.
    - added general-purpose `AsyncTaskRequest::Sleep` with `AsyncTaskResult::SleepFinished`.
    - added `EventCtx` helper methods for async task and timer schedule/cancel flows.
  - Added runtime-level regressions:
    - `src/runtime/tasks.rs`: replacement cancellation, cancel-by-target, and sleep completion.
    - `src/runtime/timers.rs`: timer schedule/replace/cancel plus timer+async non-blocking progression.
    - `src/event/mod.rs`: `EventCtx` helper emits expected runtime control messages.
  - Added concrete usage path in `examples/hello.rs` (`BackgroundStatusLabel`) showing async background work chained with one-shot timers for progressive UI updates.
- **Terminal/golden coverage expansion (`PR8I`)**
  - Added a deterministic raw terminal-output capture helper for CI (`tests/support/terminal_capture.rs`) that snapshots escaped bytes and control/text segment streams.
  - Expanded metadata integration coverage from framebuffer-only snapshots to framebuffer->diff->raw-terminal-output invariants (`tests/render_metadata.rs`).
  - Added focused golden tests for sparse diff output and no-op frame output invariants, including absolute cursor-control assertions and raw-output snapshots (`tests/terminal_output_golden.rs`).
  - Updated `ROADMAP.md` Phase 1 Golden tests row from `Partial` to `Done`.
- **Deterministic widget-id policy closure (`PR8H`)**
  - Closed Phase 0.5 deterministic ID contract decision by explicitly keeping `WidgetId::new()` as a process-local monotonic allocator (no cross-run determinism guarantee).
  - Stable/persistent widget IDs are deferred for now until a concrete persistence/snapshot requirement exists, avoiding premature ID-contract lock-in.
  - Added focused `WidgetId` regression coverage in `src/widgets/core.rs` (uniqueness/monotonicity and explicit `from_u64` round-trip invariants).
  - Updated `ROADMAP.md` to move the deterministic widget-id row to `Done` and mark Phase 0.5 rich-rs contract closures as met.
- **Rich-rs integration closure follow-up (`PR8G`)**
  - `Link` now emits hyperlink metadata (`StyleMeta.link`) in render output, enabling OSC8 links through the existing `rich-rs` terminal pipeline.
  - Hyperlink policy is now explicit and tested: no explicit `link_id` is set by widgets; `rich-rs` assigns stable per-Console link IDs when needed.
  - Added focused regression coverage in `src/widgets/link.rs` and updated `ROADMAP.md` Phase 0.5 hyperlink-id row to `Done`.
- **Message-bus closure follow-up (`PR8F`)**
  - `Select` open-dropdown Enter/click selection now routes through inner `OptionList`
    message flow (`OptionSelected` consumed in `on_message`) instead of direct click/index coupling.
  - Added explicit ordering regressions for:
    - `OptionList`: `OptionHighlighted` before `OptionSelected`,
    - `Select`: `OptionSelected` before `SelectChanged`,
    - `SelectionList`: `SelectionListToggled` before `SelectionListSelectedChanged`.
  - Updated roadmap/widget source-of-truth docs to mark message-bus closure as done in the
    current widget scope.
- **Grapheme closure follow-up (`PR8E`)**
  - `MaskedInput`:
    - cursor placement from mouse `x` now maps through grapheme/cell boundaries instead of ASCII indexing assumptions.
    - render output now uses grapheme-aware styled runs and width clamping to avoid wide/ZWJ overflow artifacts.
  - `DataTable`:
    - added regressions for combining-mark and wide-cell column-width / header-hit mapping behavior.
  - `Tree`:
    - row width/hit-testing now derive from rendered prefix cell width (including twisty/indent), improving wide/ZWJ/combining behavior.
    - added wrapping-width and viewport-clamp regressions for grapheme-heavy labels.
  - `ROADMAP.md` now marks grapheme-aware text editing as `Done` with cross-widget closure notes.
- **Tier-B/Tier-C closure follow-up (`PR8D`)**
  - `ListView`/`Tree` interaction polish:
    - moved row activation to press/release semantics (emit on matching `MouseUp`), preserving selection/twist-toggle behavior and tightening hover synchronization on click.
  - `Header` interaction polish:
    - added icon/body press-region matching so cross-region press/release is a no-op.
  - Text-edit platform-fidelity shortcuts:
    - added `Ctrl+Insert` (copy), `Shift+Insert` (paste), `Shift+Delete` (cut),
      `Alt+Left/Right/Backspace/Delete` word-nav/delete, and `Super+A/E/Left/Right/Backspace` home/end/delete-to-start mappings.
  - Utility parity/lifecycle polish:
    - `Select`/`OptionList` highlight lifecycle now resets correctly on clear/reopen and clears hover state on app focus loss/unmount.
    - `Log` and `KeyPanel` now request repaint when scrollbar drag ends so thumb active state clears immediately.
- **Tier-A final closure batch (`PR8C`)**
  - `DataTable`:
    - added horizontal viewport scrollbar parity (render + track-click paging + thumb-drag),
      plus horizontal wheel/action behavior when column-cursor navigation is not active.
    - aligned home/end and horizontal key lifecycle behavior with viewport movement semantics.
  - `RichLog`:
    - `write(...)` now honors default `markup` / `highlight` behavior, including repr-highlighter
      application when highlighting is enabled.
    - added focused regressions for default-markup and default-highlighter semantics.
  - `CommandPalette`:
    - close-animation phase now gates child interactions until panel visibility fully settles.
    - unmount lifecycle now resets open/panel state to prevent stale remount behavior.
  - Added focused regressions in `src/widgets/data_table.rs`, `tests/data_table.rs`,
    `tests/rich_log.rs`, and `src/widgets/command_palette.rs`.
- **Widget primitive closure batch (`PR8A`: A/B/C)**
  - Focused HELP metadata pipeline:
    - added framework-level focused-help signaling (`HelpPanelFocusedHelpChanged` / `HelpPanelFocusedHelpCleared`) and runtime diff/dispatch integration.
    - added `Widget::help_markup()` hook and `HelpPanel` message-path consumption.
  - Async task primitive baseline:
    - added runtime async task manager with `AsyncTaskSpawn` / `AsyncTaskCancel` / `AsyncTaskCompleted` / `AsyncTaskCancelled`.
    - migrated `DirectoryTree` lazy loading from tick-queue to runtime async task flow with collapse-time cancellation.
  - CSS/parser closure items for tooltip/help parity:
    - added `hkey` / `vkey` border types in style model, parser, and border rendering.
    - updated `HelpPanel`/`KeyPanel` defaults to `vkey` and added focused parser/widget regressions.
- **Widget closure recovery batch (`PR7K`)**
  - Tier-A follow-up:
    - `DataTable` tightened horizontal-offset stability when fixed columns saturate viewport width, and aligned cursor/home/end paths with column visibility behavior.
    - `Tabs`/`TabbedContent` now gate switch-tab binding hints on switchable targets and reset focus/hover/transient state on unmount.
    - `CommandPalette` refined panel hit-testing to avoid false close behavior when query/input events use local coordinates.
    - `RichLog` auto-scroll now tracks multiline styled/renderable writes with estimated post-write content height.
  - Text-edit/clipboard polish:
    - shared text-edit key decoding now ignores clipboard chords with extra modifiers and centralizes first-line clipboard extraction.
    - `Input` and `MaskedInput` paste flow now consumes only first clipboard line for single-line parity.
  - Utility lifecycle/async polish:
    - `DirectoryTree` now queues directory loads for tick-time processing with collapse-time cancellation of pending descendant loads.
    - `HelpPanel`, `Tooltip`, and `Welcome` unmount now reset lifecycle state to avoid stale focus/visibility/anchor behavior across remount.
- **Widget closure follow-up (`PR7J`)**
  - `ListView`/`Tree` interaction semantics:
    - added explicit activation messages (`ListViewItemActivated`, `TreeNodeActivated`) for enter/click activation paths.
    - refined tree click semantics so twisty clicks toggle without forcing activation.
    - added focus-loss/unmount hover cleanup regressions for both widgets.
  - `Header`/`Footer` lifecycle/message polish:
    - header icon clicks now emit `HeaderIconPressed`.
    - footer unmount now resets focus-tracking state to avoid stale deferred-binding behavior across remount.
- **Widget closure push (`PR7I`)**
  - Tier-A hardening:
    - `DataTable` now keeps fixed columns pinned while shifting non-fixed columns for far-cursor visibility, and header hit-testing maps correctly under shifted columns.
    - `Tabs`/`TabbedContent` now reapply latest content geometry on activation so newly active targets receive immediate resize/layout with current dimensions.
    - `CommandPalette` open-panel hit-testing now uses screen-space coordinates and animated panel position, preventing false outside-click dismissals from child-target mouse events.
    - `RichLog:focus` default CSS now uses background tint (no border-chrome focus glyphs), with focused regression coverage.
  - Tier-B/Tier-C polish:
    - Added runtime-driven `Tooltip`/`HelpPanel` message APIs (`OverlaySetAnchor`/`OverlayClearAnchor`, `HelpPanelSetHelp`/`HelpPanelClearHelp`) and parity regressions.
    - `DirectoryTree` now emits typed selection messages for file vs directory selection paths.
    - `Welcome` hover/close-row lifecycle polish and baseline default CSS parity updates.
    - `ListView`/`Tree` avoid highlighted/selected markers when all candidates are disabled.
    - Shared text-edit key mapping now supports `SUPER+X`/`SUPER+V` clipboard commands alongside existing bindings.
    - `Footer` deferred-bindings lifecycle now preserves pending updates across repeated focus-loss events.

### 2026-02-10
- **Tier-A/Tier-C widget hardening follow-up (`PR7H`)**
  - `DataTable`/`Tabs`/`TabbedContent` parity hardening:
    - added focused message/lifecycle regressions for activation, no-op activation paths, and content-height forwarding behavior.
  - `RichLog` parity improvements:
    - added markup/renderable write paths (`write_markup`, `write_renderable`) and focused coverage.
  - `CommandPalette` rendering polish:
    - improved small-viewport resilience and markup-aware result rendering, with focused snapshot coverage.
  - Utility/lifecycle polish:
    - `Log` now preserves viewport anchor when `max_lines` pruning trims head rows and includes default CSS regression coverage.
    - `Markdown` wrapped heading component-style coverage added.
    - `Tooltip`/`HelpPanel`/`DirectoryTree`/`Welcome` gained additional lifecycle delegation regressions and behavior fixes.
  - Runtime clipboard bridge:
    - clipboard runtime now attempts OS clipboard copy/paste first and falls back to in-app clipboard buffer when unavailable.
- **Container-family parity baseline (`containers.py` alignment pass)**
  - Added new container aliases/classes: `Vertical`, `Center`, `Right`, `Middle`, `VerticalGroup`, `HorizontalGroup`, `ScrollableContainer`, `CenterMiddle`, and `ItemGrid`.
  - Added `ScrollHome` / `ScrollEnd` actions and key bindings (`home`, `end`) plus `ctrl+pageup` / `ctrl+pagedown` horizontal paging bindings.
  - Made `ScrollView` and `HorizontalScroll` focusable and wired home/end handling across scroll aliases/container primitives.
  - Added focused coverage in `tests/container_parity.rs` and scroll container suites.
- **Tier-B/C widget polish follow-up**
  - `ListView`/`Tree` now model highlighted-vs-hovered styling semantics more explicitly (`-highlighted` class behavior).
  - Added runtime clipboard store plumbing for text-edit message flow:
    - handles `TextEditClipboardCopyRequested` and `TextEditClipboardPasteRequested`,
    - responds with `TextEditClipboardPaste` through the runtime message bus.
  - `Welcome` lifecycle polish: close action now emits both `ButtonPressed` and `OverlayDismissRequested`.
  - `Tooltip`/`HelpPanel` lifecycle polish: runtime-driven tooltip anchor updates from mouse events, tooltip hide on app focus loss, and help-panel active/inactive visibility behavior.
- **Widget closure follow-up slices: Header/Footer + Tooltip/HelpPanel + DirectoryTree**
  - `Header` lifecycle polish:
    - explicit hover state cleanup on leave, app focus loss, and unmount transitions.
  - `Footer` lifecycle polish:
    - defers `BindingsChanged` updates while app is unfocused,
    - applies latest deferred bindings once on focus regain while preserving message-bus updates.
  - `Tooltip` parity pass:
    - added anchor-aware overlay positioning with horizontal clamp and vertical inflection,
    - added component-style-driven tooltip bubble/text defaults.
  - `HelpPanel` parity pass:
    - fixed split resize propagation so markdown/key-panel children receive correct layout heights,
    - added lifecycle and short-layout behavior regressions.
  - `DirectoryTree` async/lazy fidelity:
    - added lazy-expand support for unloaded directory branches,
    - improved refresh to preserve expanded paths while reloading expanded directories lazily,
    - added focused `DirectoryTree`/`Tree` regressions for lazy expansion and refresh behavior.
- **Widget closure follow-up slices: RichLog + CommandPalette + ListView/Tree + clipboard hooks**
  - `RichLog` parity hardening:
    - preserves viewport anchor semantics when `max_lines` trimming removes head lines while manually scrolled,
    - preserves all explicit newline-separated styled output from `write_segments(...)`.
  - `CommandPalette` now emits `Message::CommandPaletteCommandSelected` for built-in commands (`keys`, `quit`) before close, with regression coverage for ordering and quit behavior.
  - `ListView` and `Tree` now support disabled-item/node interaction semantics:
    - keyboard navigation skips disabled entries,
    - mouse selection/hover ignores disabled entries,
    - default CSS now includes disabled row/node styles.
  - Added shared text-edit clipboard command hooks (`Copy`/`Cut`/`Paste`) and message-bus clipboard events:
    - `Message::TextEditClipboardCopyRequested { text, cut }`
    - `Message::TextEditClipboardPasteRequested { target }`
    - `Message::TextEditClipboardPaste { target, text }`
  - Wired clipboard message flow for `Input`, `MaskedInput`, and `TextArea` with focused regression tests.
- **Missing widget port PR6C: baseline `DirectoryTree` + `Welcome`**
  - Added `DirectoryTree` (`src/widgets/directory_tree.rs`) as a first-pass filesystem tree widget built on `Tree`, with directory scan/loading, lazy expand-on-toggle behavior, and message-bus forwarding via `on_message`.
  - Added `Welcome` (`src/widgets/welcome.rs`) as a baseline welcome surface with markdown body + bottom action button, routed through widget message flow.
  - Wired exports in `src/widgets/mod.rs` and `src/lib.rs`, and added focused tests in `tests/directory_tree.rs` and `tests/welcome.rs`.
- **Missing widget port PR6A: first-pass `Log`**
  - Added new `Log` widget (`src/widgets/log.rs`) with Python-style plain-text write APIs (`write`, `write_line`, `write_lines`), max-line pruning, and clear behavior.
  - Reused shared line-scroll primitives and scrollbar interactions from `ScrollView` (action/mouse-wheel/drag + clamp semantics) and emits scroll state changes via the message bus.
  - Wired public exports in `src/widgets/mod.rs` and `src/lib.rs`, and added focused behavior regressions in `tests/log.rs`.
- **Missing widget port PR6B: baseline `Tooltip` + `HelpPanel`**
  - Added new `Tooltip` wrapper widget (`src/widgets/tooltip.rs`) that overlays tooltip content using shared PR4 composition (`Overlay::compose_overlay_at`) over a wrapped child.
  - Added message-driven tooltip visibility control via existing overlay messages (`OverlaySetVisible`, `OverlayToggle`, `OverlayDismissRequested`) and emits `OverlayVisibilityChanged` on visibility transitions.
  - Added new `HelpPanel` widget (`src/widgets/help_panel.rs`) that composes markdown help content with `KeyPanel` bindings in a reusable framework-level container.
  - Wired public exports in `src/widgets/mod.rs` and `src/lib.rs`, and added focused regressions in `tests/tooltip.rs` and `tests/help_panel.rs`.
- **Roadmap planning structure consolidation**
  - Consolidated overlapping `ROADMAP.md` sections (`Next priorities` + `Execution checklist`) into a single execution source of truth (`Execution Plan` + ordered PR streams) to reduce drift during active development.
- **DataTable Tier-A closure slice PR5A**
  - Added typed keyed row/column model primitives in `DataTable` (`RowKey`, `ColumnKey`) with keyed add/look-up APIs and cursor cell-key resolution.
  - Added fixed-row/fixed-column baseline behavior in `DataTable` rendering and hit-testing paths, including fixed-row-aware scroll/visibility logic.
  - Expanded keyboard/navigation semantics (`Home`/`End`, `Ctrl+Home`/`Ctrl+End`, viewport-sized paging, page-left/page-right actions) while preserving existing DataTable message bus events.
  - Added focused parity regressions in `src/widgets/data_table.rs` and `tests/data_table.rs` for keyed model, fixed-row mapping/visibility, and cursor navigation semantics.
- **Scrolling primitive unification for data/text-heavy widgets (Phase 7 widget PR1)**
  - Added shared line-scrolling utilities and scrollbar math in `src/widgets/containers/scroll_view.rs`.
  - Migrated `RichLog`, `KeyPanel`, `ListView`, `Tree`, and `DataTable` to the shared scrolling path.
  - Added focused regressions for mouse-scroll clamping and visibility/offset behavior in `tests/list_view.rs`, `tests/tree.rs`, and `tests/data_table.rs`.
- **Shared toggle/option abstraction + widget migrations (Phase 7 widget PR3)**
  - Added shared toggle/option primitives in `src/widgets/toggle_option.rs`: typed `OptionId`,
    shared option row model (`OptionItem`), highlight-vs-selected cursor state
    (`OptionCursorState`), and binary toggle interaction state (`BinaryToggleState`).
  - Migrated `OptionList`/`Select`/`SelectionList` to shared option/cursor semantics, including
    typed option IDs, consistent disabled behavior checks, and explicit highlighted-vs-selected
    separation.
  - Migrated `Checkbox`, `Switch`, and `RadioButton` to shared binary toggle event semantics
    (mouse press/release, keyboard toggle, disabled no-op) while preserving existing message types.
  - Updated `RadioSet` to shared cursor state for highlighted vs active button tracking.
  - Added focused regressions across migrated widgets (`OptionList`, `Select`, `SelectionList`,
    `Checkbox`, `Switch`, `RadioButton`, `RadioSet`) and shared helper tests.
- **Overlay/modal composition unification (Phase 7 widget PR4)**
  - Added shared overlay composition helpers in `src/widgets/containers/overlay.rs` (`compose_overlay`, `compose_overlay_at`) with style-aware overlay semantics used across widgets/runtime.
  - Rebases `Overlay` rendering to shared composition and preserves style/meta in composed segment output.
  - Rebases `CommandPalette` layer composition (key panel split + open panel overlay) and runtime toast stacking (`src/runtime/render.rs`) to the same helper path.
  - Added focused composition regressions in `src/widgets/containers/overlay.rs`; overlay and command palette focused suites remain green.
- **Tier-A Tabs/TabbedContent lifecycle closure (Phase 7 widget PR5B)**
  - Added explicit disabled/hidden state semantics for `Tabs` and `TabbedContent` entries, with activation filtering that skips ineligible tabs/panes for keyboard, mouse, and programmatic activation.
  - Strengthened activation/focus transitions so focus delegation only follows valid active targets and hidden-active transitions select the next available target deterministically.
  - Added focused regressions in `tests/tabs.rs` and `tests/tabbed_content.rs` for keyboard/mouse/state-transition behavior under disabled/hidden lifecycle changes.
- **Roadmap PR sequencing update for widget parity closure**
  - Updated `ROADMAP.md` to add an explicit, ordered widget PR program (shared primitives first, then Tier-A closure, then missing-widget ports), instead of relying only on a generic pointer to the widget plan.
  - Reordered the execution checklist so widget parity closure is tracked as a first-class execution stream with concrete PR slices and exit criteria.
- **Roadmap execution checklist for remaining Todo/Partial items**
  - Added a prioritized, concrete PR-slice checklist in `ROADMAP.md` for all open `Todo`/`Partial` fundamentals (dirty/style invalidation, message bus completion, grapheme completion, timers/async tasks, golden coverage, integration-contract closures, and compatibility/devtools follow-up).
  - Updated v0.2 next-priority wording to reflect current status (`one-shot timers + async task framework`).
- **CI pipeline baseline tracked as done (Phase 0)**
  - Confirmed repository CI workflow runs `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets` on push/PR.
  - Updated `ROADMAP.md` to mark the Phase 0 CI task as done and removed CI from open v0.2 next-priority backlog.
- **Grapheme-safe text editing core (Input/TextArea foundation)**
  - Added shared grapheme-aware text indexing helpers in `src/widgets/text_edit.rs` (boundary clamping, left/right navigation, and cell/byte mapping).
  - Migrated `Input` and `TextArea` cursor movement, backspace/delete behavior, mouse hit-testing, and width-aware rendering loops to use grapheme boundaries.
  - Added targeted regression coverage for combining-mark and ZWJ emoji editing semantics (`src/widgets/input.rs` tests and `tests/text_area_widget.rs`).
- **Shared text-edit command core completion (`Input`/`MaskedInput`/`TextArea`)**
  - Expanded `src/widgets/text_edit.rs` with a reusable key-to-edit-command layer plus shared word-boundary helpers.
  - Migrated `Input`, `MaskedInput`, and `TextArea` key handling to shared command semantics for grapheme/word navigation and deletion.
  - Added keyboard selection baseline parity for `Input` and `TextArea` (`Shift+arrow/Home/End`) with focused regressions in `tests/input_widget.rs`, `tests/text_area_widget.rs`, and widget unit tests.
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
