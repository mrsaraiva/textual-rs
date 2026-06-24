# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### 2026-06-24 (feat(data_table): renderable Content cells + key-function / multi-column sort)

- **`DataTable` cells are now styled `Content`, not plain `String`.** Each cell is a
  `Cell { content: Content, align: TextAlign }` (exported as `DataTableCell` in the prelude),
  rendered through the canonical content subsystem (`Content::render_strips`). Per-cell
  foreground color, italic/bold, arbitrary markup spans, and horizontal justification now fall
  out of the content path for free — replacing the old parallel `cell_justify: Vec<Vec<CellJustify>>`
  emulation vector. This is faithful to Python Textual where a cell may be a `rich.text.Text`
  with its own `style` and `justify`. New cell constructors: `Cell::text`/`markup`/`content`/
  `styled` + `with_align`, plus `From<&str>`/`From<String>`/`From<Content>`.
- **Styled-cell add APIs.** `DataTable::add_row_cells(Vec<C: Into<Cell>>)` and
  `add_row_cells_labeled` accept pre-built styled cells; `add_row_labeled` now takes an
  `Into<Content>` label, so row labels are styled `Content` (Python `add_row(..., label=Text(...))`).
  The existing `add_row`/`add_rows`/`add_columns`/`add_row_with_key` `ToString` paths still work
  (they wrap each value in a plain `Cell::text`). `update_cell_content` replaces a cell with a
  styled one.
- **Key-function and multi-column sort.** New `DataTable::sort_by(columns, reverse, key_fn)` —
  the closure receives the selected columns' plain text and returns a `SortKey` (numeric/string/
  tuple), faithful to Python `sort(*columns, key=…, reverse=…)`. `sort_by_columns(columns, reverse)`
  is the no-key multi-column form. The single-column `sort(column, reverse)` now compares values as
  `SortKey`s, so numeric columns sort numerically (`"10"` after `"2"`) instead of lexicographically.
  New `SortKey` type (numbers via `f64::total_cmp`, strings lexicographic, tuples element-wise).
- **Demos rewired to real fundamentals** (no more "framework gap" approximations):
  `data_table_renderables` builds italic `#03AC13` right-justified `Content` cells;
  `data_table_labels` uses a styled `[#B0FC38 italic]` row label; `data_table_sort` uses real
  key-function sorts (average-of-times-then-last-name, last-name lambda, country plain-text) and a
  real multi-column sort.
- New verification tests in `tests/data_table.rs`: a styled cell renders its `#03AC13` fg + italic;
  numeric-key and key-function single-column sorts order correctly; a custom average-key over
  multiple columns sorts faithfully; multi-column sort; styled row label renders. No styled-parity
  regression (72 PASSING held; pty_parity 186/0).

### 2026-06-24 (fix(color): port LAB conversion exactly to Python's easyrgb f64 form)

- **`rgb_to_lab` / `lab_to_rgb` are now byte-exact to Python Textual.** The RGB↔CIE-L\*a\*b\*
  conversion in `src/style.rs` was the Bruce-Lindbloom `6/29` piecewise form in `f32`; Python
  Textual's `textual/color.py` uses the easyrgb form (`7.787*t + 16/116`, thresholds `0.008856` /
  `0.2068930344`, asymmetric `lab_to_rgb` X/Y/Z constants) in `f64`. The conversion is now a faithful
  `f64` port of the easyrgb form. `lighten_lab` / `darken_lab` take `f64` amounts (so the luminosity
  step `spread/2 = 0.075` feeds the LAB math without `f32` rounding), drop the non-Python pre-conversion
  `L` clamp (Python only `.clamped`s the final RGBA), and truncate channels with `int(c*255)` semantics.
  This removes shade-token drift of up to 42/channel across the `$*-lighten-*` / `$*-darken-*`
  design tokens.
- **`textual-dark` base colors corrected.** The static token table stored `accent`/`warning` as
  `#FEA62B` and `error` as `#B93C5B` — these are Python's *round-tripped* `n==0` shade values, not the
  source colors. The table now stores the source colors (`#FFA62B`, `#FFA62B`, `#BA3C5B`) so the
  lighten/darken shades derive correctly, while the BARE `$accent`/`$warning`/`$error`/… design tokens
  reproduce Python's `color.lighten(0)` LAB round-trip (`#FEA62B`/`#B93C5B`). Tokens derived from
  `accent.hex` in Python's `_generate` (e.g. `footer-key-foreground`) keep the raw source color,
  matching Python. All 78 shade tokens (both the `parse_color_like` static path and the
  `ColorSystem.generate` path in `src/theme.rs`) are now byte-exact to Python's `textual-dark`.
- New regression tests `style::tests::lab_shade_parity_with_python` (41 LAB lighten/darken cases) and
  `style::tests::dark_design_tokens_match_python_generate` (bare + shade + derived tokens) lock the
  parity. No styled-parity regression (72 PASSING held).
### 2026-06-23 (feat(helpers): B-cluster helper APIs — scroll_visible + Welcome arena composition)

- **`App::scroll_visible(node_id: NodeId) -> bool`** — new method that scrolls the nearest
  scrollable ancestor to make a descendant widget visible, mirroring Python's
  `Widget.scroll_visible()` → `Screen.scroll_to_widget()` → `scroll_to_region()` flow.
  Two-phase algorithm: read phase walks the arena ancestor chain looking for the first
  `scroll_viewport_size()`-bearing container, computes the widget's virtual coordinate
  (screen_pos − container_origin + current_offset), applies minimum delta on each axis via
  `min_scroll_delta`, then write phase downcast-dispatches to `ScrollView`,
  `ScrollableContainer`, `HorizontalScroll`, or `VerticalScroll`.  Returns `false` when no
  scrollable ancestor exists or the node is absent.
- **`Welcome` — arena composition.** `Welcome` was using an inline-rendering approach where
  its `Button` and `Markdown` were private fields not mounted in the arena tree.  `Welcome`
  now uses `compose()` to place `Container(id="md") > Markdown` and `Button(id="close")` as
  proper arena children.  `render()` returns empty segments; events and messages route through
  the tree.  `set_close_label(&str)` is a new public method.  `query_one(Button)` / `#close`
  now resolves correctly — enabling `widgets04` label update.
- **Demos rewired:** `actions06`, `actions07` — replaced terminal-width scroll hack with
  `app.scroll_visible(page_id)`; `widgets04` — uses `with_query_one_mut_as::<Button>` on
  `#close` to change the label after mount.
- **Tests:** `tests/welcome.rs` — 3 integration tests (render, compose count, lifecycle);
  internal unit tests in `src/widgets/welcome.rs` for compose ids + message routing;
  `tests/scroll_view.rs` — `app_scroll_visible_returns_false_for_missing_node` verifies
  method contract.
### 2026-06-23 (feat(callthread): `App::call_from_thread` — synchronous UI-thread dispatch)

- **`App::call_from_thread(callback)` — post a callable onto the UI thread from a worker.** New
  primitive mirroring Python Textual's [`App.call_from_thread`](https://textual.textualize.io/api/app/#textual.app.App.call_from_thread).
  A background worker thread (e.g. from `EventCtx::request_worker_task` /
  `request_exclusive_worker_task`) posts a closure onto the app's event loop; the loop runs it with
  exclusive `&mut App` access on the next tick and ships its return value back, blocking the worker
  until it returns. This replaces the previous `Arc<Mutex<Option<…>>>` result-ferrying workaround in
  the threaded-worker demos with a faithful, reusable framework primitive. Unlike Python it is an
  *associated function* (no `&self`): a worker thread does not hold an `App` reference; the app is
  supplied to the callback on the UI thread instead.
  - API: `App::call_from_thread<F, R>(callback) -> Result<R, CallFromThreadError>` where
    `F: FnOnce(&mut App) -> R + Send + 'static`, `R: Send + 'static`; `App::is_ui_thread() -> bool`.
  - Errors (mirroring Python's `RuntimeError` guards): `CallFromThreadError::NotRunning` (no event
    loop), `SameThread` (called on the UI thread — would deadlock), `Disconnected` (app shut down
    before the callable ran). `CallFromThreadError` is re-exported in `textual::prelude`.
  - Runtime wiring: the event loop registers its thread as the UI thread (RAII guard, unregisters and
    drains pending jobs on every exit path) and drains the global call-from-thread queue once per tick
    before processing worker requests.
  - Demos rewired: `docs/examples/guide/workers/examples/weather04` and `weather05` now use
    `App::call_from_thread` to apply fetched weather to the `#weather` `Static`, exactly mirroring
    Python weather05's `self.call_from_thread(weather_widget.update, weather)` (and dropping the
    shared-mutex result buffer entirely).
### 2026-06-23 (fix(progress_bar): gradient color sweep reversed to match Python `_apply_gradient`)

- **`render_determinate_gradient` — gradient direction now matches Python exactly.**
  Python's `_apply_gradient` (in `renderables/bar.py`) applies the gradient
  **reversed**, keyed off the highlighted (text) length — not the absolute cell
  position: `t = (text_length - offset) / (width - 1)`, so the leftmost highlighted
  cell gets the highest t value and the rightmost gets the lowest.  The previous Rust
  implementation used forward `t = x / (width - 1)` keyed off absolute position,
  producing a mirrored sweep.  Fixed by:
  - Pre-counting the total highlighted cells (`highlighted_count`).
  - Computing per-cell `t = (highlighted_count - highlight_offset) / (width - 1)`
    (decreasing left-to-right), matching Python exactly.
  - `get_color` already clamps t to [0, 1] so t > 1 (partially-filled bar) is
    handled correctly.
- Added two regression tests (`gradient_direction_reversed_matches_python`,
  `gradient_direction_partial_fill_reversed`) that assert the per-cell gradient
  color direction matches Python's direction for both 100%- and 50%-filled bars.

### 2026-06-23 (feat(renderwire): B-cluster renderable wiring — gradient, OptionContent, pretty, rich_log)

- **`LinearGradient` — multi-stop gradient in `ProgressBar`.** `ProgressBar` previously collapsed
  the gradient to a 2-stop `(Color, Color)` pair; it now holds a full `LinearGradient` (existing
  renderable at `src/renderables/gradient.rs`). The 12-stop rainbow demo port now uses the real
  `LinearGradient` instead of a 2-color approximation. `LinearGradient::get_color()` is now public
  (renamed from private `sample_color`) matching Python's `Gradient.get_color`. API: `ProgressBar`
  gains `gradient()`, `set_gradient()`, `with_gradient()` returning/taking `Option<LinearGradient>`.
- **`OptionContent` — arbitrary Renderables in OptionList items.** `OptionItem` can now hold
  `OptionContent::Renderable(Arc<dyn Renderable>)` in addition to `OptionContent::Text(Text)`.
  Tables render live at the runtime widget content width (Python `scrollable_content_region.width`
  parity — scrollbar-width-aware). API: `OptionItem::renderable(label, r)`,
  `OptionItem::renderable_with_id(label, r, id)`, `item.content() -> Option<&OptionContent>`,
  `item.text_content() -> Option<&Text>`. `OptionContent` and `OptionId` are re-exported in
  `textual::prelude`. The `option_list_tables` example port now uses `OptionItem::renderable`
  instead of pre-rendering tables to `Text` at hardcoded width 78.
- **`RichLog::write_debug` — real Pretty path.** `write_debug<T: Debug>` previously wrote a plain
  text repr; it now writes a `rich_rs::pretty::Pretty` renderable so debug values appear with proper
  syntax highlighting and Python-repr-style indentation (matching Python's `RichLog.write(value)` via
  `rich.pretty.Pretty`).
- **`pretty.rs` — confirmed already correct.** The existing `Pretty::new(&T)` path captures
  `format!("{:?}", value)` and feeds it to `rich_rs::pretty::Pretty::from_str()`. No changes needed.
### 2026-06-23 (feat(routing): declarative `@on` routing + `prevent` context)

- **Declarative message routing (`@on(Message, selector)`).** New `routing` module with
  `MessageRouter<S>` — the Rust analogue of Python Textual's `@on` decorator (`textual/_on.py`).
  Register handlers with `router.on::<M>(selector, handler)` / `on_any::<M>(handler)`, then
  `router.dispatch(state, event, ctx)` runs every handler whose message type matches *and* whose
  CSS selector matches the message's control (mirroring Python's `_get_dispatch_methods`). Selectors
  support `#id`, `.class`, `Type`, compound terms (`Button#save.primary`, `.toggle.dark`) and
  comma-separated groups (`#quit, #cancel`), parsed by the new `Selector` type. A selector-less
  (`""` / `Selector::any()`) route matches every control. Control identity is supplied by the new
  `Message::control_meta()` trait method + `ControlMeta` struct; `ButtonPressed` implements it
  (`#id` + `Button` type), so `@on(Button.Pressed, "#quit")` routing works end-to-end.
- **`prevent(MessageType)` context.** `EventCtx::prevent::<M>(|ctx| { ... })` (and
  `prevent_types(&[TypeId], ...)`) temporarily suppress a message type from being posted for the
  duration of the closure, mirroring Python's `with self.prevent(M):` (`message_pump.py`
  `_prevent_message_types_stack` + `post_message`'s `_is_prevented` check). Scopes nest (the active
  prevented set is the union of the stack). `is_prevented::<M>()`, `pending_message_count()`, and
  `has_pending_message::<M>()` were added for querying/testing.
- **Demo ports rewired to real features:** `events/on_decorator02` now routes via `MessageRouter`
  (`@on(Button.Pressed, "#bell"/"#toggle-dark"/"#quit")`); `events/on_decorator01` keeps the
  single-handler form and routes the theme toggle through `run_action("app.toggle_dark")`;
  `events/prevent` clears its `Input` inside `ctx.prevent::<InputChanged>()` so the bell-on-change
  handler never fires for a programmatic clear (replacing the structural-only note).
- **Deferred:** `guide/compound/byte03`'s `prevent(BitSwitch.BitChanged)` still uses a bool flag —
  its feedback loop is suppressed across a *later* reactive-update cycle (`Handle::update`'s watcher
  emits through a different `EventCtx`), which an app-side `prevent` scope cannot span until `prevent`
  is threaded through `ReactiveCtx`/the reactive-update pipeline (`DEFERRED(byte03-prevent)`).
### 2026-06-23 (fix(widgets/DirectoryTree): apply `filter_paths` on the async lazy subdir load)

- **`DirectoryTree::filter_paths` now applies on every load path.** The custom path-filter
  predicate is applied not only to the initial synchronous build and direct `read_children`,
  but also to the **async lazy subdirectory load** (`AsyncTaskResult::DirectoryEntries` delivered
  for an expanded subdir). Previously a `DEFERRED` gap meant the filter was bypassed when a subdir
  was expanded, so excluded entries (e.g. dotfiles) leaked into lazily-loaded subtrees. This matches
  Python `DirectoryTree.filter_paths`, whose filter runs inside the single `_load_directory` worker
  used for all loads (initial, lazy expand, reload).
- **Example rewired:** `docs/examples/widgets/directory_tree_filtered` no longer documents a known
  gap; expanding a nested directory keeps dotfiles hidden, matching the top-level behavior of
  Python's `FilteredDirectoryTree`.
- **Verification test:** new `directory_tree_filter_applies_on_async_lazy_subdir_load` expands a
  subdirectory (spawning an async `ReadDirectory`), delivers an async result containing both a kept
  file and a dotfile, and asserts the dotfile never reaches the rendered tree while the kept file does.
### 2026-06-23 (fix(runtime): fire `on_mount()` on arena-tree children)

- **Extracted tree children now receive `on_mount()`.** Children declared via `compose()` /
  `with_child()` are extracted out of their parent widget and re-homed as arena-tree nodes. The
  tree-build path drained the initial `Mount` lifecycle events and discarded them, so those nodes
  never had `on_mount()` called — only the synthetic root and widgets that still hold their children
  as struct fields (which container widgets deliberately skip in tree mode). A widget that populates
  its content in `on_mount` (the `Hello(Static)` wrapper pattern: an inner `Static` updated on mount,
  with `render()` delegated to it) therefore rendered an **empty content box**. `build_widget_tree`
  and `build_widget_tree_from_root` now call the new `WidgetTree::fire_mount_callbacks(root_stub)`,
  which invokes `on_mount()` on every freshly-mounted node (in mount order, skipping the root stub).
  This was **not** a render-delegation bug — content set in `new()` always painted; the gap was the
  missing per-node `on_mount()`. Fixes the `docs/examples/guide/widgets/hello04`, `hello05`, and
  `hello06` demos (multilingual greeting now paints on startup).
- Added `tests/wrapped_static_mount_render.rs` (positive: wrapped-`Static` content set in `on_mount`
  paints; negative control: skipping `fire_mount_callbacks` reproduces the empty box).

### 2026-06-23 (feat(reactive): field-to-field `data_bind` reactive binding)

- **Field-to-field data binding (keystone).** `App::data_bind_reactive::<W, T>(source, source_field,
  target_selector, set_child)` binds a parent/app reactive **field** to a child widget's reactive
  **field** — Python parity with `child.data_bind(App.field)` / `child.data_bind(child=App.field)`.
  Whenever the source reactive changes, the value propagates into every widget matched by
  `target_selector` (via the caller-supplied typed setter, an unconditional set mirroring Python's
  `_Mutated`) and each child's `watch_*` fires. Previously the derive `Reactive` engine had
  compute/watch/recompose/validate/mutate but **no** way to propagate one reactive into another
  widget's reactive; demos faked it with manual `on_tick` fan-out.
- **App-level reactive changes now fire dynamic watchers.** The app-reactive bridge
  (`dispatch_app_reactive`) fires `data_bind` / `watch_reactive` watchers registered against the app
  reactive source (`App::app_reactive_source()`) after the app's own `watch_*`, mirroring Python
  `_check_watchers` firing the "global" `__watchers`. App reactives have no tree node, so bindings
  key off a sentinel app-source `NodeId`.
- **`watch_with_app` child watchers fire during fan-out.** New `App::with_widget_taken_as::<W>()`
  temporarily swaps a child widget out of the tree (children/styles/`NodeId` preserved) so its
  `reactive_dispatch_with_app` can run with `&mut App` — letting a bound child's watcher
  `query_one`/mutate sibling/descendant nodes (e.g. update its `Digits`) without a borrow conflict.
- **Demo ports rewired to real `data_bind`:** `guide/reactivity/world_clock02` (positional
  `data_bind(App.time)`) and `world_clock03` (keyword `data_bind(clock_time=App.time)`, binding a
  source field onto a differently-named target field) now use `App::data_bind_reactive` instead of
  `on_tick_with_app` polling.
- **Deterministic tests.** New behavioral tests assert the binding propagates a value to each bound
  child's reactive and fires its watcher with that value (no wall-clock/time-dependent goldens), plus
  `with_widget_taken_as` round-trips the widget and preserves child nodes.

### 2026-06-23 (feat(timer): `set_interval` / `set_timer` scheduling primitive)

- **Real timer subsystem (keystone).** Apps can now self-schedule arbitrary-delay, named,
  repeating callbacks via `App::set_interval(interval, repeat, pause, callback)` and
  `App::set_timer(delay, callback)` — Python parity with `MessagePump.set_interval` / `set_timer`.
  Previously the only periodic hook was a global per-frame `on_tick`; widgets/apps could not
  register a 1-second `update_clock`-style callback. Returns a `TimerHandle` for
  `stop_timer` / `pause_timer` / `resume_timer` / `reset_timer` (Python `Timer.stop/pause/resume/reset`).
- **Faithful `timer.py` semantics.** Timers schedule against the event loop's frame/timeout so
  callbacks fire at the right wall-clock cadence; the loop timeout is clamped to the soonest timer
  deadline. Repeating timers fast-forward past missed deadlines (Python `skip` — a stalled loop fires
  once, not a backlog burst); bounded `repeat` fires exactly N times then auto-removes; paused timers
  neither advance nor drive the loop timeout and re-anchor on resume.
- **App-struct bridge for timer callbacks.** `App::with_app_struct::<T>()` lets a timer callback
  re-enter the user app struct to mutate reactive fields (Python `self.time = now`), firing the
  watcher/recompose through the app reactive bridge in the same turn. DOM-only callbacks
  (`update_clock`) just query+update a widget directly.
- **Deterministic test path.** The timer runtime is driven by a swappable clock; tests advance a
  manual clock and assert exactly how many times a callback fired after N advances — no wall-clock
  sleeping, no time-dependent goldens. 18 new behavioral tests (runtime + timer module).
- **Demo ports rewired to real `set_interval`** (no more `on_tick` second-boundary faking):
  `widgets/clock`, `guide/reactivity/recompose01`, `recompose02`, `world_clock01`, and
  `guide/core/structure` now register a real 1-second timer instead of detecting second boundaries
  in `on_tick_with_app`.

### 2026-06-23 (feat(action): `@click` action-link routing + `run_action(str)` + namespaced dispatch)

- **`@click` action-link routing (super-keystone).** `[@click=action]` markup is now live, not
  dead metadata. The content renderer bakes the parsed `@click` action string into each rendered
  segment's `StyleMeta` (mirroring Python's `Style._meta`), so it survives blitting into the frame
  buffer. On a click, the runtime reads the `@click` meta at the clicked cell (mirroring Python
  `widget._on_click` → `app._broker_event` → `get_style_at`) and dispatches the named action. Works
  for **any** widget that renders `@click` markup (Label/Static and future), with no per-widget
  wiring. Action argument parsing is parenthesis-aware, so `@click=set('a', 'b')` keeps its full
  value.
- **`App::run_action(str)` + `EventCtx::run_action(str)`** (Python parity: `App.run_action` /
  `Widget.run_action`). Run an action by name from app or widget code instead of inlining the
  mutation; both route through the unified runtime dispatch chain.
- **Namespaced action resolution + `check_action` gating.** A unified runtime dispatcher resolves
  every string action (from `@click`, `run_action`, key bindings, or `ActionDispatchRequested`)
  against the `widget → screen → app` namespace chain, gates the resolved target with
  `Widget::check_action` (new trait method; the app adapter delegates to `TextualApp::check_action`),
  and falls back to the app's custom `action_<name>` hook for unknown actions. The previously
  missing custom-action fallback in the `ActionDispatchRequested` path is now wired.
- **Demo ports rewired to real action dispatch** (no more inlined mutation / "framework gap"
  workarounds): `guide/actions` `actions02` (now uses `App::run_action`), `actions03`/`actions04`
  (real `[@click=app.set_background(...)]` links), `actions05` (widget-scoped `ColorSwitcher` action
  via the namespace chain); `guide/widgets` `hello05` (widget-scoped `[@click='next_word']`).
  `content01` and `hello06` already carried the `@click` markup and now route for real.
- **Verification:** `tests/click_actions_pty.rs` drives the real `actions03` example in a PTY,
  sends an SGR mouse click on a `[@click=...]` span, and asserts the screen background actually
  changed — proving the full input → hit-test → dispatch → mutate chain.

### 2026-06-23 (feat(style): public component-class style API — `get_component_rich_style`)

- **Public component-class style API** (Python parity: `Widget.COMPONENT_CLASSES` /
  `Widget.get_component_styles` / `Widget.get_component_rich_style`). Custom/external widgets can
  now declare and resolve component-class styling from CSS instead of hardcoding colours:
  - `Widget::component_classes() -> &[&'static str]` — declare the widget's component-class names
    (mirrors Python's `COMPONENT_CLASSES`).
  - `Widget::get_component_styles(name) -> Style` — resolve a declared component class to the
    typed CSS `Style` against the live widget context.
  - `Widget::get_component_rich_style(name) -> Option<rich_rs::Style>` — resolve a component class
    to a ready-to-paint Rich style (colours flattened over the effective background).
  - The underlying resolver `css::resolve_component_style` is now `pub` and re-exported in the
    prelude (was `pub(crate)`), so example/3rd-party crates can use it directly.
- **Demo ports rewired to the real API** (no more hardcoded `#A5BAC9` / `#004578` / `darkred`):
  `guide/widgets` `checker02`, `checker03`, and `checker04` now declare `component_classes()` and
  resolve their square colours from the CSS component-class rules via `get_component_rich_style`,
  faithfully reproducing Python's `checker0{2,3,4}.py`. `checker03` continues to match its PTY
  parity golden byte-for-byte.
### 2026-06-23 (feat(content): Content::from_markup_with_vars template-variable substitution)

- **`Content::from_markup_with_vars(markup, &variables)`**: a new constructor that performs
  `string.Template`-style `safe_substitute` over `$name` / `${name}` **before** tag parsing,
  faithfully mirroring Python `Content.from_markup(text, **variables)` (`markup.py` `_to_content`'s
  `process_text`). Substitution applies to **text tokens only** — tag bodies like `[$primary]`
  are left intact so theme tokens still resolve at render time — and follows CPython's default
  `Template` semantics: `$$`→`$` escape, ASCII identifier pattern (`[A-Za-z_][A-Za-z0-9_]*`),
  exact-case dict lookup, and unknown keys left unmodified (no error). Critically, a variable
  *value* that contains markup (`$x` where `x = "[red]BIG[/red]"`) is inserted as **literal text**,
  never re-parsed as a tag — matching Python. Span offsets are tracked post-substitution.
- **`Static::update_content(Content)`**: render a pre-built `Content` (e.g. one with template
  variables already substituted) through the same alignment / background / theme-token / link
  path as plain markup, so the widget displays the content's own spans verbatim. Mirrors Python
  `Static.update(content)` for `Content`/`Visual` values. (`Content` is now `Send + Sync` — its
  lazy `cell_length` cache moved from `OnceCell` to `OnceLock`.)
- **Markup Playground port now faithfully reproduces Python** (`guide/content/playground`): the
  JSON variables panel is parsed (`serde_json`) into a variable map and threaded into
  `Content::from_markup_with_vars`, then rendered via `Static::update_content`, with the
  `Content.spans` list shown in the Spans panel. Previously the port parsed the variables panel
  but never substituted (a confirmed framework gap). Clears the `guide/content/playground` demo.
### 2026-06-23 (feat(theme): named theme catalog/registry + cycling)

- **Named theme registry** (`src/theme.rs`): a faithful port of Python Textual's
  `textual/theme.py` (`Theme`, `BUILTIN_THEMES`) plus the design-token generator from
  `textual/design.py` (`ColorSystem._generate`). All 20 Python built-in themes are ported
  with their exact color values (`textual-dark`, `textual-light`, `nord`, `gruvbox`,
  `tokyo-night`, `catppuccin-*`, `dracula`, `monokai`, `flexoki`, `solarized-{light,dark}`,
  `rose-pine{,-moon,-dawn}`, `atom-one-{dark,light}`, `ansi-{dark,light}`). The generator
  derives every semantic token (`$text-error`, `$primary-muted`, `$surface-active`,
  `$scrollbar`, shades, …) per theme using Python's exact algorithms (LAB lighten/darken,
  truncating channel blends, `tint`/`__add__`). Base/semantic tokens verified hex-for-hex
  against Python; the LAB-derived shade family (`$*-lighten-2/3`, `$*-darken-3`) still
  diverges by up to ~42/channel on some themes — a pre-existing `rgb_to_lab`/`lab_to_rgb`
  inaccuracy (Bruce-Lindbloom vs Python's easyrgb form, f32 vs f64), tracked as a follow-up.
- **App theme API** (`App::register_theme` / `available_themes` / `theme_name` /
  `set_theme_by_name` / `set_theme_cycle` / `cycle_theme` / `action_cycle_theme`): mirrors
  Python `App.register_theme` / `available_themes` / `App.theme = name` / `action_cycle_theme`.
  Activating a non-default named theme swaps the active design-token map so every `$`-token
  resolves from that theme and re-colors the UI; the hand-tuned `textual-dark` static path is
  preserved as the default (zero golden regression). New `cycle_theme` / `set_theme` app
  actions (`AppCycleTheme` / `AppSetTheme` messages) wire CSS/binding action strings to the
  runtime.
- **`todo_app` doc example rewired** to faithfully reproduce Python: Ctrl+T now cycles the
  exact Python named-theme list `[nord, gruvbox, tokyo-night, textual-dark, solarized-light]`
  (was a `toggle_dark` workaround) and applies `nord` on mount. Added the `hatch` Screen rule
  from the Python source.

### 2026-06-23 (fix(layout): exact cumulative-floor fr distribution + per-layer dock isolation)

- **Exact cumulative-floor flow sizing** (`layout_resolve_1d_exact`): the vertical/horizontal
  layouts now size every child — fixed scalars AND `fr` — to exact `f64` cells and floor the
  RUNNING position (`floor(cum + exact) - floor(cum)`), mirroring Python `_resolve.resolve` +
  `layouts/{vertical,horizontal}.py` (`accumulate(...).__floor__()`). Previously each child's
  size was floored independently, losing the fractional remainder: a stack of non-integer
  relative units (`12.5%`/`5w`/`12.5h`/`6.25vw`/`12.5vh`) under-sized, and — worse — the `fr`
  children reserved space against the un-carried INTEGER fixed sizes while the fixed children
  DISPLAYED their carried (often +1) sizes, overflowing the row/column by the accumulated carry.
  New `style::resolve_scalar_exact` returns the exact pre-floor cell count for simple fixed
  scalars; the resolver computes the `fr` unit from the exact remaining space (with the same
  iterative `min_size` clamp as the integer resolver). Integer layouts are a no-op.
- **Per-layer dock isolation**: a `dock`ed widget on a SEPARATE layer (e.g. a `layer: ruler`
  overlay) now OVERLAYS the flow region instead of carving it, matching Python `_arrange.py`
  (layers arranged independently). Applied in BOTH `layout::resolve_layout` (the flow region)
  and `runtime::render::host_content_extent` (the scrollable virtual size) — without the latter
  the overlay dock inflated the virtual extent and triggered a phantom scrollbar lane that
  shifted every relative-unit child by the lane width.
- **Width-aware height remeasure in horizontal layout**: a content-height child (unset OR `auto`
  height) in an `fr`-width row now re-measures its wrapped height at the RESOLVED width (it
  previously used the stale `layout_height()` from whatever width it was last laid out at), so a
  wrapping `Label` in a `width: 1fr` row sizes its box to the correct line count. The Phase 1
  remeasure only covered explicit `auto`; this adds the unset-height + fr-width case.
- Promotes `"width_comparison"`, `"height_comparison"`, and `"text_style"` to the styled
  PASSING set. (styled 54→57)

### 2026-06-23 (fix(text): text-align auto-fg extend, justify spacing, link-color auto contrast)

- **Vertical-extend fill for `color: auto` content widgets**: the BOX vfill discriminator in
  `render_widget_with_meta` checked only `resolved.fg`, so a content widget with `color: auto`
  (e.g. `text_align`'s `Label { color: auto }`) lost its auto-contrast foreground on the blank
  extend rows below its text. It now also checks `resolved.fg_auto` (the linked auto pair), so
  those rows carry the auto-contrast fg in their content area (padding stays bg-only), matching
  Python `widget.render_line` `IndexError` → `Strip.blank(width, visual_style.rich_style)`.
- **`text-align: justify`** is now implemented in `Content::render_strips`: inter-word spacing is
  stretched so each non-final line fills the width (Python `_FormattedLine.to_strip` justify
  branch — round-robin space distribution from the right). The last line of a paragraph stays
  left-aligned (`line_end`), and the stretched pad spaces are foreground-bearing (resolving
  `color: auto` contrast) like Python's `(style + text_style).rich_style`. `wrap_and_format`
  gained a `_marked` variant returning the per-line `line_end` flag.
- **`link-color: auto` contrast against the link background**: the default `$link-color`
  (= `$text` = `auto 87%`) is now treated as auto (new `Style.link_color_auto` /
  `link_color_hover_auto` markers). `Widget.link_style` parity: the link foreground is
  `link_background.get_contrast_text(alpha)` — a contrast resolved against the LINK background,
  not a fixed color resolved against the screen. A bright `link-background: $accent` now yields
  dark link text instead of light. `$link-color`/`$link-color-hover` map to `auto 87%`.
- Promotes `"text_align"` and `"link_background"` to the styled PASSING set. (styled 65→67)
### 2026-06-23 (fix(render): CSS `hatch` compositing parity — defer + content-box scope)

- **`hatch` fill** is now DEFERRED until after a node's children render and SCOPED to the
  node content box (inside any border/padding), matching Python `line_post` / `apply_hatch`.
  Previously the fill ran before children, so the inner content child of a `.class()`/`.id()`
  `Node` wrapper (which carries the border + hatch, with the raw text in an inner child)
  un-hatched the FIRST inner row. Naively deferring past children then over-filled the blank
  padding around a `border_title` on the border row (` cross ` → `╳cross╳`); scoping the fill
  to the content box fixes both. Leaf hatch (e.g. a bare `Label` with `hatch`) is unchanged —
  its content box equals its layout rect.
- **`Node` wrapper** gained `with_border_title()` / `with_border_subtitle()` and now reports
  `border_title()` / `border_subtitle()` so a `.class()`/`.id()` panel renders its title on the
  border (Python `static.border_title = ...`). The `docs/examples/styles/hatch` example sets
  per-panel titles accordingly.
- Promotes `"hatch"` to the styled PASSING set. (styled 65→66)
### 2026-06-23 (fix(scrollbar): scrollbar thickness honors CSS `scrollbar-size`)

- **Host scrollbar widgets now paint at the CSS-resolved thickness.** `apply_host_scrollbar_layout`
  reserved the correct lane width from `scrollbar-size` (e.g. `scrollbar-size: 10 4` → a 4-wide
  vertical lane) but the `ScrollBar` widget still painted glyphs at its hardcoded creation default
  (vertical 2 / horizontal 1), so a 4-wide lane showed only a 2-wide bar. Added `ScrollBar::set_thickness`
  and drive it from `geometry.vertical_lane_width` / `geometry.horizontal_lane_height` during host
  scrollbar layout. Matches Python `ScrollBar.thickness` flowing from `styles.scrollbar_size_*`.
- **Fixed a latent horizontal-scrollbar row-break bug**: the inter-row `Segment::line()` separator was
  bounded by `length` (the track width) instead of the rendered row count, so a horizontal bar emitted
  a spurious trailing line break (and would have dropped breaks if `thickness > length`). Now bounded by
  `lines.len()`. Vertical bars were unaffected (rows == track length there).
- Example fix (`styles/scrollbar_size2`): moved `id()` from a `Node` wrapper onto the
  `ScrollableContainer` host directly, so `#v1 { scrollbar-size: ... }` matches the scrollbar host
  instead of an intermediate wrapper (which fell back to the `Widget` default). Mirrors Python
  `ScrollableContainer(Label(...), id="v1")`.
- Regression tests: `scrollbar_thickness_drives_vertical_glyph_width`,
  `scrollbar_thickness_drives_horizontal_row_count`.

### 2026-06-23 (fix(render): chrome-only container vertical-extend fill is bg-only)

- **Vertical-extend (BOX) fill** in `render_widget_with_meta` now discriminates chrome-only
  containers from content widgets. A bordered/sized `Container` (or any layout container)
  renders no text content — Python's `Widget.render` returns `Blank(background_colors[1])`,
  a background-only visual with no foreground — so its interior extend rows are now **bg-only**
  even though `color` is inherited from `Screen { color: $foreground }` (was bleeding
  `fg=$foreground` onto the container's blank rows). Content widgets (Static/Label) keep the
  foreground-bearing `visual_style` extend (Python `widget.render_line` `IndexError` fallback).
  The `segments_empty` flag is the in-render discriminator (chrome-only containers produce no
  content segments). Matches Python `_styles_cache` / `get_inner_outer` `inner.rich_style`.
- Promotes `"dock_all"` and `"margin_all"` to the styled PASSING set. (styled 61→63)

### 2026-06-23 (feat(links): `links` example parity — id-selector CSS on Static, @click link styling, trailing-newline row count)

- **`Static::id()`** now sets `seed.css_id` directly on the Static node (instead of
  wrapping in a transparent `Node`), so CSS id-selectors (`#custom { link-color: ... }`)
  target the Static widget itself. `css_id_cache` preserves the id across `take_node_seed()`
  for off-tree CSS resolution.
- **`Static::class()`** continues to return a `Node` wrapper, preserving the existing layout
  tree structure that nesting01/02 and other examples depend on.
- **`Static::render()`** now overlays `link-color` / `link-background` CSS tokens onto spans
  carrying `@click` markup (mirrors Python `widget.link_style` applied per-span). Plain
  content spans are never affected.
- **`intrinsic_height()`** counts a trailing `\n` in the text as an extra blank line
  (Python Rich counts it; Rust `str::lines()` previously ignored it).
- **Trailing empty strip fix**: extra `Segment::line()` tokens are emitted for trailing empty
  strips so `split_and_crop_lines` produces the correct blank row count without running the
  vertical-fill path with `fg=#e0e0e0` on those rows.
- Promotes `"links"` to the styled PASSING set. (styled 56→57)
### 2026-06-23 (fix(tint,background_tint): exact Python parity for tint and background-tint CSS properties)

- **fix(scrollbar): bake explicit host `color` into track segments** — `ScrollBarRender::render_bar`
  now accepts an optional `track_fg: Option<Color>`. `ScrollBar::render()` passes `resolved.fg`
  (the host widget's `color`) so track whitespace cells carry explicit fg in their segment style,
  matching Python's `_Styled(scrollbar_render, rich_style)` which applies the host fg to ALL
  rendered segments (including track whitespace). Previously, `apply_style_to_segments` dropped fg
  from whitespace cells via the `has_glyph` guard — scrollbar track cells always appeared `fg=def`.
  Promoted `tint` to `PASSING` (58 total, styled 56→58).

- **fix(example/background_tint): use `Vertical::new().id()` directly instead of `Node` wrapper** —
  Python's `Vertical(Label(...), id="tint1")` sets the CSS id on the Vertical itself, so both
  `Vertical { background: $panel; }` and `#tint1 { background-tint: ...; }` apply to the SAME
  widget node. The Rust example previously used `Node::new(Vertical).id("tint1")`, which split these
  CSS rules across two separate nodes (`background` on inner Vertical, `background-tint` on outer
  Node), preventing the tint from affecting the background. Fixed to `Vertical::new().id("tint1")`.
  Also: Fix 2 (previous session) applied `bg`-only vfill for `fg_auto` extend rows (matching Python's
  `inner.rich_style` for vertical extend beyond content height). Promoted `background_tint` to
  `PASSING`.

### 2026-06-23 (fix(color): exact-0.5 placeholder alpha + Python opacity double-application)

- **Placeholder bg** uses exact float `0.5` alpha (`rgba_f`) instead of `128/255`=0.50196,
  so composited cells match Python's `background: {color} 50%` per-cell RGB. Flips
  `column_span`, `row_span`.
- **Widget opacity** now matches Python's double-application: background is composited once
  in `background_colors` and again in `_apply_opacity` (`parent.blend(parent.blend(bg,o),o)`),
  while fg is blended once; border-fg is pre-blended in `apply_border_edges` to match. Flips
  `opacity`. (styled 53→56)

### 2026-06-23 (fix(color): blend() and lab_to_rgb use truncation to match Python int() semantics)

- **fix(style/blend):** `blend()` channel mix now uses truncating cast (`as u8`) instead of
  `round()`, matching Python's `int(r1 + (r2 - r1) * factor)` semantics.  Dynamic shade
  tokens computed at runtime (via `parse_shade` / muted variants) now produce exact
  Python-parity values.

- **fix(style/lab_to_rgb):** `lab_to_rgb` final quantisation step changed from `round()` to
  truncating `as u8` cast, matching Python's `int(r * 255)` in `lab_to_rgb`.  Fixes
  off-by-one for dynamic darken/lighten calls on any color not covered by the hard-coded
  token map (e.g. `primary-darken-1`, `primary-lighten-1`).
### 2026-06-23 (feat(widgets): @click link visual — apply link-* CSS to markup spans; fix link-color alpha shorthand)

- **feat(text/Label): apply CSS link-* tokens to `[@click=...]` markup spans** —
  `Label::render()` now detects spans whose `raw_tag` starts with `@click=` and
  overlays a link-style derived from `visual_style.link_color`, `link_background`,
  and `link_style`.  Mirrors Python `widget.link_style` applied to segments whose
  meta carries `@click`.  `[link=url]` spans are intentionally excluded (matching
  Python behavior where only action links get link-color styling).

- **fix(css): parse `link-color`/`link-background`/`link-color-hover`/`link-background-hover` with optional `N%` alpha** —
  CSS parser now uses `parse_color_like_with_alpha` for all link-color and
  link-background properties, matching the existing `color`/`background` handling.
  Fixes `link-color: hsl(60,100%,50%) 50%` and similar alpha-qualified colors.

- **parity**: `link_color` promoted to PASSING (53 → 53 styled examples, was 52).
### 2026-06-23 (feat(checkbox): migrate render to Content::render_strips — Phase-D retirement)

- **feat(checkbox): render via Content::render_strips** —
  `Checkbox::render()` now resolves `current_self_style()` +
  `current_ancestor_composited_background()` for correct bg composition, builds
  the `▐X▌ label ` content via `Content::assemble` with per-part styles
  (`side_style`, `button_style`, `label_style` from `resolve_component_style`),
  and calls `Content::render_strips` (no_wrap, height=1, left-align).
  Segments are tagged `textual:no_text_style` so `apply_style_to_segments`
  skips redundant re-application of CSS text attributes already baked in.
  Snapshot updated: cells now carry `textual:no_text_style=true` metadata
  (meta-only change, visual output unchanged).
### 2026-06-23 (feat(toast): migrate render to Content::render_strips — Phase D retirement)

- **feat(toast/Toast): render via Content::render_strips** —
  `Toast::render()` now uses `Content::from_markup` + `Content::assemble` +
  `Content::render_strips` following the established Button/Label pattern.
  `current_self_style()` / `current_ancestor_composited_background()` supply
  the visual style; the `toast--title` component style is resolved via
  `resolve_component_style` and assembled into a styled span at the front of
  the content. Segments are tagged `textual:no_text_style=true` so
  `apply_style_to_segments` skips redundant re-application of text attributes.
  The old `render_markup_line` private method (which used `Text::plain` +
  `adjust_line_length_no_bg`) is retired. Layout helpers (`wrapped_line_count`,
  `content_box_width`, `markup_cell_len`) are unchanged; they are used only for
  `layout_height` / `content_width` computation, not the render path.
### 2026-06-23 (feat(aliases/Static): migrate render to Content::render_strips — Phase-D retirement)

- **feat(aliases/Static): render via Content::render_strips** —
  `Static::render()` now calls `Content::from_markup`/`Content::from_text` +
  `Content::render_strips` directly with `current_self_style()` /
  `current_ancestor_composited_background()`.  The internal `Label` dependency
  for rendering is retired; `Static` now owns its text, markup, wrap, expand,
  shrink and `NodeSeed` fields directly.  Segments are tagged `no_text_style`
  so `apply_style_to_segments` skips redundant re-application of text
  attributes already baked in by render_strips.  The `Rich(Text)` path
  (`update_rich`) continues to delegate to `Text::render` unchanged.
  Layout helpers (`layout_height`, `content_width`, `auto_content_width`) are
  now self-contained — no longer double-counting Label chrome.  All existing
  regression tests pass (styled 52/87 unchanged, pty_parity 186/0 unchanged).

### 2026-06-23 (feat(widgets): migrate Tab, CollapsibleTitle, MarkdownHeadingBlock to Content::render_strips)

- **feat(tabs/Tab): render via Content::render_strips** —
  `Tab::render()` now uses `Content::from_text` + `Content::render_strips` with
  `current_self_style()` / `current_ancestor_composited_background()`.
  Segments are tagged `no_text_style` so `apply_style_to_segments` skips
  redundant re-application of text attributes already baked in by render_strips.

- **feat(collapsible/CollapsibleTitle): render via Content::render_strips** —
  `CollapsibleTitle::render()` migrated from `Text::plain(...).render()` to
  `Content::render_strips` with the same visual-style + bg-composition pattern.

- **feat(text/MarkdownHeadingBlock): render via Content::render_strips** —
  `MarkdownHeadingBlock::render()` migrated from `Text::plain(...).render()` to
  `Content::render_strips`.  Snapshot updated: heading cells now carry
  `textual:no_text_style=true` metadata (meta-only change, visual output
  unchanged).  All CSS text attributes (bold, color) remain correctly baked
  via `current_self_style()`.

### 2026-06-23 (feat(button): migrate render to Content::render_strips — focus band covers line-pad)

- **feat(button): retire custom line-pad render code; use Content::render_strips** —
  `Button::render()` now builds a `Content` from the label (with line-pad spaces
  pre-baked into the content text when the label fits) and renders via
  `Content::render_strips` with the focus-aware `visual_style` (from
  `current_self_style()`), center alignment, and `line_pad=0` (the outer
  line-pad spaces are still applied by `render_widget_with_meta`'s
  `apply_line_pad`).  The pre-baked line-pad spaces are content runs, not
  alignment padding, so the S1 seam fix in `render_strips` bakes the full
  `visual_style` (including `reverse=true` for `:focus`) into them.  Result:
  the focused-button reverse band now covers `" Label "` (label + 1 space on
  each side) matching Python, not just `"Label"` (old behaviour when label
  was narrow enough to fit with line-pad).  The old `no_style_space_segment`
  helper is retired; all segments from `render_strips` are tagged
  `no_text_style` (style baked by render_strips; `apply_style_to_segments`
  skips redundant re-application).

- **chore(snapshots): update frame_layout + keys_preview snapshots** —
  Two snapshot tests updated for the new segment-metadata layout: glyph cells
  from the button now carry `textual:no_text_style=true` (style baked in by
  render_strips instead of applied later by apply_style_to_segments).  Visual
  output is identical; metadata reflects the new render path.

### 2026-06-22 (fix(content): C1 seam fixes — correct whitespace-span style + vertical-fill surface)

- **fix(content/seam1): remove `has_glyph` guard from content runs** —
  `emit_rendered_segments` previously dropped fg and all text attributes
  (underline, reverse, strike, italic, bold, dim) from whitespace-only content
  runs.  Python's `_FormattedLine.to_strip` applies `(style + text_style).rich_style`
  to **every** `Content.render()` run without discrimination: a span covering
  only spaces but styled with `reverse=True` or `underline=True` must preserve
  those attributes.  Fixed by removing the `has_glyph` parameter from
  `make_segment` and unconditionally applying all style attributes.
  Alignment-pad segments (`pad_left`/`pad_right` from `make_bg_segment`)
  remain bg-only — matching Python's `style.background_style.rich_style`.

- **fix(content/seam2): vertical fill rows carry full style, reverse=false** —
  Fill rows (added to reach the requested `height`) previously used
  `make_bg_segment` (bg-only).  Python `Visual.to_strips` uses
  `(style + Style(reverse=False)).rich_style` for fill rows — fg + bg, with
  reverse forced to false.  Fixed by computing `fill_style = visual_style` with
  `reverse = Some(false)` inside `render_strips` and emitting fill rows via
  `make_full_segment`.

- **tests added:** `test_seam1_whitespace_span_reverse_preserved`,
  `test_seam1_whitespace_span_underline_preserved`,
  `test_seam2_vertical_fill_full_style_reverse_false`,
  `test_render_strips_vertical_fill_full_style`,
  `test_render_strips_line_pad_carries_fg`.  Old bg-only fill test and
  line-pad-no-fg test updated to reflect correct Python semantics.

- Full gate: lib compiles, docs/examples build, pty_parity 186/0, full suite
  0 FAILED, visual_parity 52 PASS / 0 REGRESSION.  Documented in
  `docs/devel/CONTENT_LAYER_KEYSTONE.md` § 11.

### 2026-06-22 (refactor(content): audit dead-code post Phase D migration)

- **refactor(content): retire stale doc comments** — the `# ADDITIVE — not yet
  wired into the render path` comment in `Content::render_strips` and the
  module-level Phase C/D integration comment in `src/content/mod.rs` are updated
  to reflect that `Content::render_strips` IS now wired into `Label::render()`
  (Phase D, 94eda62). The remaining `Text::plain` usages in `text.rs` are all
  inside Markdown-internal widgets (heading, paragraph, blockquote, etc.),
  which are not yet migrated; they are explicitly documented as pending.
- **audit (core.rs, segments.rs):** All branches in `render_widget_with_meta`
  fill surfaces and `apply_style_to_segments` `has_glyph` fg-stamp remain
  active and are still exercised by non-migrated widgets (Button, DataTable,
  Input, Tree, etc.). No dead code found; nothing else retired in this pass.
  Full gate: lib compiles, docs/examples build, pty_parity 186/0, full suite
  0 FAILED, visual_parity 52 PASS / 0 REGRESSION.

### 2026-06-22 (feat(content): Label/Static render via Content::render_strips — styled 42→52)

- **feat(Label/Static render pipeline):** `Label::render()` now uses
  `Content::render_strips` instead of `rich-rs render_str` / `Text::plain`.
  - Builds a `Content` (via `from_markup` or `from_text`) and calls
    `render_strips(width, None, &render_style, text_align, "fold", false, 0, resolve_fn)`.
  - Correctly computes the effective background by flattening the widget's own
    `bg` over the **ancestor** composited background
    (`current_ancestor_composited_background()`, which excludes the current
    widget's own style from the composite — matching `apply_style_to_segments`
    post-render behavior).
  - Tags each Content-produced segment with `textual:no_text_style` so
    `apply_style_to_segments` skips re-applying CSS text attributes (bold,
    italic, etc.) that are already baked in by `render_strips`. The background
    and foreground are also already explicit in the segments so they survive the
    pass unchanged; `fg_auto`, tint, and `text_opacity` are still applied.
  - Adds `current_ancestor_composited_background()` CSS helper.
- **parity (styled 42→52):** Flips `text_style_all`, `border01`, `border_title`,
  `box_sizing01`, `dimensions01`, `dimensions02`, `dimensions03`, `outline01`,
  `padding01`, `text_opacity` to PASSING. No regressions in the previous 42.
  Snapshot churn is meta-only (`textual:no_text_style` now appears on Label
  cells); visual content is unchanged.

### 2026-06-22 (deps: bump rich-rs to 1.2.1 — link markup fix flips link examples)

- **deps: rich-rs 1.1.1 → 1.2.1.** Picks up the `[link=url]` markup fix (OSC8 meta
  only, no hardcoded cyan/underline), so link styling now comes from the CSS `link-*`
  tokens as Python does. Flips styled examples `link_background_hover`,
  `link_color_hover`, `link_style`, `link_style_hover` (styled 38→42). No regressions;
  full suite green.

### 2026-06-22 (feat(content): Phase C — Content::render_strips)

- **feat(content/Phase C): `Content::render_strips`** — turns a `Content` into
  fully-styled `Vec<Vec<rich_rs::Segment>>` for a given width / height /
  alignment / overflow / `visual_style` / theme-token resolver.
  - Implements the 3-surface semantic from Python `_FormattedLine.to_strip`
    and `apply_style_to_segments`:
    - **Glyph cells** carry full colour (fg + bg + text attrs from
      `visual_style` combined with span styles).
    - **Content-pad / alignment-pad cells** carry bg only (no fg) — mirrors
      Python `style.background_style`.
    - **Vertical fill rows** (when `height > content_rows`) carry bg only.
  - Calls `wrap_and_format` internally; applies `resolve_styles(resolve_fn)`
    so theme tokens (`$primary`, `auto 20%`) are resolved with live context.
  - `align` (`Left / Center / Right / Justify`) and `line_pad` are fully
    supported.
  - ADDITIVE — not yet wired into the render path.  Migration is Phase D.
  - 14 new unit tests asserting per-cell fg/bg for plain text, markup
    (bold/color/custom resolver), alignment pad, vertical fill, wrapping,
    overflow modes, line_pad, and the `has_glyph` invariant.
  - All gates green: `cargo build`, docs/examples build, lib unit tests
    compile, pty_parity 186/0, full `--tests` suite 0 FAILED, visual_parity
    42 PASS / 0 REGRESSION.

### 2026-06-22 (feat(content): Phase B — wrap_and_format, truncate, pad/align, divide/split)

- **feat(content/Phase B): `Content::wrap_and_format` + manipulation API**
  - `wrap_and_format(width, overflow, no_wrap, line_pad)` — word-wraps a
    `Content` into `Vec<Content>` lines.  Reuses `rich_rs::divide_line` for
    word-boundary break positions; does **Textual-specific** `rstrip()` /
    `truncate(width)` / `pad(line_pad, line_pad)` in `Content` itself, never
    in rich-rs (which remains a faithful Rich port).
    - Non-last wrapped lines are rstripped (the key Textual semantic that was
      previously pushed into rich-rs as a band-aid).
    - `overflow="fold"` → hard-fold long words across lines.
    - `overflow="ellipsis"` → truncate with `…`.
    - `no_wrap=true` → per-logical-line fold or truncate (no word-wrap).
    - `line_pad` → spaces prepended+appended to every output line.
  - `Content::truncate(max_width, ellipsis)` — cell-width truncation with
    optional `…` ellipsis; adjusts spans via `trim_spans`.
  - `Content::pad_left(n)`, `pad_right(n)`, `pad(left, right)` — unstyled
    space padding; left-pad shifts spans (no fg style on pad bytes).
  - `Content::center(width, ellipsis)`, `right_align(width, ellipsis)` —
    alignment helpers that rstrip+truncate before padding.
  - `Content::divide(offsets)` — split at byte offsets, distributing spans.
  - `Content::split_on(sep, allow_blank)` — split on separator string,
    optionally retaining trailing blank piece.
  - `Content::rstrip()`, `rstrip_end(size)`, `right_crop(n)` — trailing
    whitespace removal.
  - 42 new unit tests covering all of the above; all 2049 suite tests green.
  - Phase B is still additive — not wired into the render path.
### 2026-06-22 (fix(scrollbar): gutter reservation separated from widget visibility; CSS class cascade via take_node_seed)

- **fix(scrollbar): `scrollbar-gutter: stable` now reserves the gutter lane without displaying the widget**
  - `apply_host_scrollbar_layout` previously used `geometry.vertical_lane_width > 0` (which is
    true for both stable-gutter reservation AND overflow-driven display) as the scrollbar-widget
    SHOW flag. Changed to `geometry.show_vertical` / `geometry.show_horizontal`, which is `true`
    only when content actually overflows AND visibility is allowed. Python parity:
    `_arrange_scrollbars` uses `show_vertical_scrollbar` (overflow + allowed), while
    `_get_scrollbar_region` handles gutter reservation separately. Styled parity 36→37 PASS
    (`scrollbar_gutter` promoted); 0 regressions; pty_parity 186; full suite green.

- **fix(css): `ScrollableContainer.take_node_seed()` now delegates to inner `ScrollView`**
  - Classes set via `.class("foo")` on `VerticalScroll` / `HorizontalScroll` were silently
    discarded during `tree.mount()` because `ScrollableContainer.take_node_seed()` used the
    Widget trait default (returns empty `NodeSeed`), losing the `ScrollView.seed.classes` that
    `.class()` had populated. Added `take_node_seed`, `style`, and `set_inline_style` to the
    `delegate_widget_method!` list in `ScrollableContainer`. This makes CSS class selectors
    (e.g. `.right { scrollbar-visibility: hidden }`) reach the node's resolved style in
    `apply_host_scrollbar_layout`. Partially advances `scrollbar_visibility` parity (right panel
    now hides scrollbar correctly); left panel still differs due to a pre-existing
    double-subtraction in `ScrollView.render()` that is tracked as a separate issue.
### 2026-06-22 (fix(render): paint_keylines draws full outer boundary + corner junctions)

- **fix(render): `paint_keylines` now draws complete keyline box for Horizontal/Vertical layouts**
  - Previously only drew interior vertical dividers between adjacent children and omitted the
    top/bottom horizontal lines, left/right outer boundary verticals, and corner/T-junction
    characters — producing bare dividers instead of a full box.
  - Fix: for Horizontal layouts, collect outer boundary (x_start, x_end) plus each child's
    right edge as vertical line positions; for Vertical layouts collect outer boundary
    (y_start, y_end) plus each child's bottom edge as horizontal line positions.  Delegate to
    the same junction-aware rasteriser used by the Grid path so corners and T-junctions are
    computed correctly via `keyline_junction_char`.
  - Background-preservation fix: keyline characters now preserve the existing cell background
    from the surface beneath instead of resetting it to "default" — matching Python's canvas
    overlay behaviour which carries only a foreground colour.
  - Styled parity 36→37 PASS (`keyline_horizontal` promoted); 0 regressions; pty_parity 186;
    full suite green.  (`keyline` grid example remains PENDING due to missing column-span/
    row-span layout support — a separate workstream gap.)
### 2026-06-22 (fix(layout): apply CSS `offset` in vertical/horizontal flow layout)

- **fix(layout): flow-positioned widgets now honour `offset` CSS property**
  - `layout_vertical` and `layout_horizontal` previously ignored `offset` entirely;
    only `layout_absolute` applied it. Python's `layouts/vertical.py` and
    `layouts/horizontal.py` store a per-placement offset in `WidgetPlacement` for
    EVERY child, then apply it as a visual shift at render time. The Rust side now
    mirrors this: after computing each child's normal-flow `(layout_x, layout_y)`,
    `style.offset` is read and applied to produce `(visual_x, visual_y)` which is
    stored in `layout_rect`/`content_rect`. The flow cursor (`y` in vertical, `x`
    in horizontal) continues to advance from the unshifted flow position, so offset
    is purely visual and does not perturb sibling layout — matching Python semantics.
  - Percentage offsets resolve against the widget's own layout-box dimensions,
    matching the `layout_absolute` precedent and Python's `ScalarOffset.resolve(size, viewport)`.
  - **Known limitation**: `Rect` coordinates are `u16`; negative offsets that would
    place a widget above/left of the screen origin saturate to 0 instead of going
    off-screen (which would require `i16`/`i32` coordinates in `WidgetNode`). The
    `offset.py` example (`Chani` with offset `0 -3`) therefore remains PENDING; full
    negative-offset clipping needs `widget_tree.rs`/`render.rs` changes (out of scope
    for this cluster). Positive offsets (`Paul` offset `8 2`, `Duncan` offset `4 10`)
    render correctly.
  - 0 regressions; pty_parity 186/0; full suite green.
### 2026-06-22 (fix(layout): carve_edge respects `height: auto` for docked containers)

- **fix(layout): split `None | Some(Scalar::Auto)` in `carve_edge` — docked containers with `height: auto` now size to their content**
  - `carve_edge` in `src/layout/split.rs` (used by both dock and split layout) previously
    treated `height: auto` identically to unset height (`None`), causing a docked Container
    with `height: auto` to consume ALL remaining available height instead of sizing to its
    content. Python parity: `_get_box_model`'s `is_auto_height` branch calls
    `get_content_height()` rather than filling the container.
  - Fix: split the `None | Some(Scalar::Auto)` match arm into two cases. `None` (unset) keeps
    the existing fill-available behaviour. `Some(Scalar::Auto)` now tries `layout_height()` for
    leaf widgets first, then falls back to `measure_intrinsic_content_height` for arena-tree
    containers whose children are drained. This mirrors the pattern used in `layout_vertical.rs`
    for auto-height flow children. 0 regressions; pty_parity 186; full suite green.
  - **Note**: `dock_all` still does not promote to PASSING due to a pre-existing fg-color
    rendering issue (empty container interiors show `fg=#e0e0e0` instead of `fg=def`) that
    is outside the scope of `split.rs`. The layout height fix is correct and complete; the
    rendering issue requires investigation in `src/widgets/core.rs` or
    `src/css/selectors/segments.rs`.
### 2026-06-22 (fix(css): Widget type selector now matches all widgets — Python base-class parity)

- **fix(css/matching): `Widget` CSS type selector matches all widgets (Python MRO parity)**
  - In Python Textual every widget's `_css_type_names` frozenset includes `"Widget"` (via MRO),
    so `Widget { ... }` default/user CSS rules apply to all widgets. In Rust, concrete widgets
    have type names like `"Button"`, `"Label"`, etc. — never `"Widget"` — so the `Widget {}`
    selector matched nothing; all `Widget { scrollbar-*, link-*, ... }` default rules were
    silently dropped, and user CSS like `Screen > Widget { background: green; width: 50% }`
    never matched.
  - Fix: in `StyleSelector::matches`, the literal type name `"Widget"` now skips the type
    check entirely, matching any widget. This mirrors Python's `name in node._css_type_names`
    semantics where Widget is always present.
  - Two regression-test cases added: `widget_selector_matches_any_concrete_type` and
    `widget_selector_with_pseudo_still_filters_by_pseudo`.
- **fix(css/defaults): `Widget {}` rule reordered to appear before `Screen {}` in base.rs**
  - Widget-specific rules (Screen, ModalScreen, etc.) must appear AFTER the Widget base rule
    in source order so they override it at equal specificity (both are type selectors with
    specificity 1). Previously Widget appeared after Screen, so `Widget { background: transparent }`
    would override `Screen { bg: $background }` for Screen itself.
  - `background: transparent` omitted from Widget defaults for now: the Rust rendering code
    uses `parent_style.bg` directly in `apply_border_edges` rather than
    `current_composited_background()`, so injecting `bg: Some(transparent)` on intermediate
    widgets (Grid, Container, etc.) causes border backgrounds to flatten against black instead
    of Screen's `$background`. DEFERRED(render-transparent-bg): fix `apply_border_edges` and
    `apply_style_to_segments` to use `current_composited_background()`.
  - Styled tally unchanged at 36 PASS / 0 regressions; pty_parity 186/0; full suite green.
  - Target examples `width` and `height` remain PENDING: the Rust examples use `Placeholder`
    (which sets an inline background via `Placeholder::apply_bg_color()`) that overrides the
    user CSS `Screen > Widget { background: green }` rule. Width/height would promote once
    the examples are updated to use a plain widget without inline bg.

### 2026-06-22 (fix(layout): apply_parent_align runs for Grid — border_all/outline_all promoted)

- **fix(layout): remove early-return guard that skipped `apply_parent_align` for `Layout::Grid`**
  - `apply_parent_align` had an unconditional `if strategy == Layout::Grid { return; }` guard
    that prevented children placed by `layout_grid` from being aligned within the available
    region. Python's `_arrange.py` applies `_align_size` for ALL layout strategies including
    Grid; the Rust guard was a divergence. Fix: remove the guard and add the
    `apply_parent_align` call after `layout_grid` in the Grid match arm, mirroring the
    vertical/horizontal branches. Also fixes the `_strategy` unused-variable warning introduced
    by removing the guard (parameter renamed to `_strategy`). Styled parity 34→36 PASS
    (`border_all`, `outline_all` promoted); 0 regressions; pty_parity 186; full suite green.
- **example(border_sub_title_align_all): add `.with_border_title()` / `.with_border_subtitle()`
  to all 9 labels** — the example previously had none, so border title/subtitle slots were
  blank. Now populated with the Python-equivalent plain-text strings (rich markup stripped).
  The example remains PENDING due to rich-markup color rendering differences (red/purple title
  colors) which are a known workstream gap, not a layout gap.
### 2026-06-22 (fix(css): scrollbar-size two-value shorthand now parsed correctly)

- **fix(css): `scrollbar-size: H V` two-value shorthand sets H/V axes independently**
  - The `scrollbar-size: H V` shorthand was parsed with `parse::<u16>()` on the whole
    string, which fails silently on `"H V"` (multiple tokens), leaving both axes at
    the default. Now splits on whitespace: two-token form sets
    `scrollbar_size_horizontal = H` and `scrollbar_size_vertical = V` per Python
    `_styles_builder.py:997-1017`; the single-token fallback is preserved for backward
    compatibility. Unit tests added in `parser.rs`.

### 2026-06-17 (fix(color): Color alpha is a float — exact composite parity)

- **fix(color): `Color.a` is now `f32` (Python-faithful), not `u8`**
  - The keystone for the styled-parity rounding cluster. `background: red 10%`
    parsed to alpha `round(0.1*255)=26` and blended with factor `26/255=0.10196`,
    while Python keeps the alpha as the float `0.1`; the factor difference drifts
    a composited channel by ±1 (e.g. bg `#291010` py vs `#2a1010` rust). Changed
    `Color.a` to a fractional `f32` in `[0,1]`, added `rgba_f`/`alpha_u8`, made
    `flatten_over` use the truncated float composite (`under + over.with_alpha(a)`
    == `int(u+(o-u)*a)`), and threaded float alpha through parse (rgba/hsla),
    opacity multiply, tint, gradient/bar/progress lerps, and the link/input alpha
    guards. `Eq`/`Hash` are hand-implemented over the alpha bits so embedding
    structs keep deriving them. Styled parity 31→34 PASS (promoted `align`,
    `background_transparency`, `colors02`), 0 regressions; pty_parity 186; full
    suite green with no snapshot deltas.

### 2026-06-17 (fix(button): focus reverse band covers line-pad spaces)

- **fix(button): apply `line-pad` as styled label spaces so the `:focus` reverse band matches Python**
  - The Button's custom render centered the label with unstyled spaces and ignored
    `line-pad: 1`, so the focused-button `text-style: reverse` band covered only the
    glyphs (`"Default"`) instead of `" Default "` like Python. Now the label is padded
    with `line-pad` styled spaces **when it fits** (plain text unchanged — the pad
    replaces centering spaces 1:1; verified pty 186 + full suite green, no collateral).
    Narrow buttons (label + line-pad > width) keep prior truncation — a tracked edge
    case (no Python reference yet). Caught by the interactive harness; `button_focus`
    remains PENDING only on the residual surface/blend bg delta (color-workstream).

### 2026-06-17 (test(parity): interactive styled-parity harness — focus/hover/active states)

- **test(parity): `visual_parity_interactive.rs` — styled parity for POST-INTERACTION frames**
  - The static styled harness only captures the initial frame, so focus/hover/active
    color states were never checked (e.g. a focused Button's `text-style: reverse`
    band). New harness sends keys, waits for re-stabilization, then compares per-cell
    RGB (incl. the `reverse` attr) vs a Python golden. First case `button_focus`
    (PENDING/tracked) documents the focused-button reverse-band divergence: Python
    bakes the button's `line-pad: 1` spaces into the styled label so the reverse band
    covers them; Rust's custom button render centers with unstyled spaces (band too
    narrow). A naive line-pad fix shifts button centering/truncation (regresses pty
    goldens + narrow key-panel buttons) so it needs a proper button-render rework;
    plus a residual surface/blend bg delta (color-workstream cluster). Harness now
    CATCHES this class of bug instead of it being eyeballed.
### 2026-06-17 (fix(theme): scrollbar token uses truncating float blend)

- **fix(theme): `$scrollbar`/`$scrollbar-hover` match Python's hex exactly**
  - Python bakes these as `(background-darken-1 + primary.with_alpha(0.4/0.5))`,
    a float-factor blend truncated with `int()`. The Rust theme used the rounding
    `blend`, drifting the channel by one (`#003055` vs Python `#003054`). Switched
    to `blend_over_float`. Removes the scrollbar-color divergence shared across
    every tall example (scrollbars/min_height/overflow/…); those examples still
    have unrelated residual diffs. 0 regressions; pty_parity 186; full suite green.

### 2026-06-17 (fix(render): default content-align skips the fg-bearing fill)

- **fix(render): only run the alignment fill for a non-default content-align**
  - Python `_visual_to_strips` guards `Strip.align` with `if content_align !=
    ("left", "top")`. The Rust port ran the fg-bearing align pad for ALL
    content-align values including the default `(left, top)`, so the trailing
    horizontal pad of a content row was colored with `$foreground` instead of the
    background-only `adjust_cell_length`/`inner.rich_style` extend (fg=default).
    Added the `!= (left, top)` guard. Styled parity 30→31 PASS (promoted
    `content_align`), 0 regressions (`content_align_all` still exact); pty_parity
    186; full suite green.

### 2026-06-17 (fix(color): float-faithful auto/contrast compositing)

- **fix(color): composite auto/contrast text with the fractional alpha directly**
  - `Color::blend_over_float` mirrors Python `under.blend(over, factor)` /
    `under + over.with_alpha(factor)`: `int(u + (o - u) * factor)` per channel,
    in float, truncated. The auto/`$text` contrast paths previously did
    `contrast.with_alpha(a).flatten_over(bg)`, which quantizes the alpha to u8
    (`round(a*255)`) and then rounds the composite via integer division — drifting
    the result by one (e.g. 87% contrast text). Switched the auto-color text fill
    (`segments.rs`) and the content-align/vertical-extend fill (`core.rs`) to
    `blend_over_float`. Styled parity 27→30 PASS (promoted `max_height`,
    `max_width`, `min_width`), 0 regressions; pty_parity 186; full suite green.

### 2026-06-17 (fix(render): text-opacity 0% blanks glyphs and drops fg)

- **fix(css): `text-opacity: 0%` produces blank cells with default foreground**
  - Mirror Python `TextOpacity.process_segments`' `opacity == 0` branch: every
    cell becomes `from_color(bgcolor=...)` — the glyph run is replaced by spaces
    of equal cell width and the foreground is dropped entirely (terminal-default),
    rather than recoloring the glyph to match the background. Applies to both
    glyph cells and the fg-bearing vertical-extend fill rows. Makes the 0%-opacity
    rows per-cell exact vs Python. (Non-zero text-opacity rows still diverge on the
    glyph-row horizontal pad — a follow-on surface-precision sub-cluster.) 0
    regressions; pty_parity 186; full suite green.

### 2026-06-17 (fix(css): Label no longer shadows inherited foreground)

- **fix(css): remove `Label { fg: $foreground }` from the base defaults**
  - Python Textual's `Label` (and `Static`/`Widget`) DEFAULT_CSS sets no
    `color`/`fg`; the foreground is supplied solely by `Screen { color:
    $foreground }` inherited down the ancestor cascade (`visual_style`). The Rust
    port had added an explicit `fg: $foreground` on `Label`, which shadowed any
    explicit ancestor `color` — e.g. `Screen { color: black }` left labels at the
    theme `$foreground` instead of black. Removed it; `Style::inherit_from`
    already propagates the ancestor color. Styled parity 24→27 PASS (promoted
    `margin`, `outline`, `padding`), 0 regressions; pty_parity 186; full suite
    green with no snapshot deltas.

### 2026-06-17 (fix(render): vertical-extend fill carries $foreground)

- **fix(render): rows beyond content height inherit the resolved foreground**
  - The keystone fill-split painted the entire `set_shape` extend (both trailing
    horizontal pad AND vertical fill rows) background-only. But Python only leaves
    the horizontal trailing pad fg-default (`_styles_cache` `adjust_cell_length`
    with `inner.rich_style`); the vertical extend rows beyond the cached content
    use `widget.render_line`'s `Strip.blank(width, visual_style.rich_style)`, which
    carries `$foreground` (or `color: auto` contrast). Split the non-aligned fill
    into a bg-only horizontal `adjust_line_length` pad plus fg-bearing vertical
    blank rows (shared `fill_fg_style` with the content-align pad). Styled parity
    19→24 PASS (promoted `content_align_all`, `text_overflow`, `text_wrap`,
    `visibility`, `dimensions04`), 0 regressions; pty_parity holds at 186; full
    suite green with no snapshot deltas.

### 2026-06-17 (fix(render): default-fg keystone — 4-surface fill split)

- **fix(render): split the widget fill style into Python's content/pad/align/box surfaces**
  - `core.rs` previously painted ALL fill (line-extend, h-pad, v-align rows, box/blank)
    with one `fill` style that carried the widget's resolved fg, so fill cells got a
    concrete fg where Python leaves terminal-default — the root of the styled color gap.
    Split it to mirror Python `content.py`/`_styles_cache.py`: glyph text keeps fg
    (inherited `$foreground` via base `Screen { color }`), horizontal content-pad +
    CSS-padding/box fill are background-only (fg=default), vertical content-align rows
    keep fg. Styled parity 13→19 PASS, 0 regressions; pty_parity holds at 186. One
    meta-only snapshot delta (no visual change). Remaining styled divergences
    (`color: auto`, interactive/focus states, blend specifics) are follow-on clusters.

### 2026-06-17 (test(parity): styled harness auto-discovers all styles — full color sizing)

- **test(parity): `visual_parity.rs` now auto-discovers every styles example**
  - Replaced the hand-listed cases with auto-discovery of every `styles/` +
    `guide/styles/` example that has a built Rust binary (**87 discovered**) +
    a `PASSING` allowlist (asserted exact) with the rest reported as the
    color-parity workstream. Full sizing at exact per-cell RGB:
    **13 PASS, 74 PENDING.** The test fails only on a `PASSING` regression;
    `REPORT_ONLY=1` tallies, `REGEN_STYLED=1` regenerates Python goldens (87
    committed as the parity baseline). This is the systematic engine for the
    color-parity workstream (fix a root cluster → re-measure the flip).

### 2026-06-17 (test(parity): widen styled harness to 21 styles — color-parity gap mapped)

- **test(parity): widened the styled harness + measured the real color-parity gap**
  - Extended `visual_parity.rs` to 21 color-focused `styles/` examples with a
    `REPORT_ONLY` measure mode. At exact per-cell RGB: **5 PASS** (background,
    color, color_auto, border, align_all), **16 XFAIL** documented. This quantifies
    that the plain-text harness was masking a broad **color-parity workstream**
    (default-fg emission, tint/opacity blend, outline/scrollbar/hatch color
    application, `color: auto N%`), not two bugs. Note: a naive base
    `color: $foreground` actually *regressed* parity for some examples — the
    default-fg model needs a more careful fix (deferred to the workstream).

### 2026-06-17 (test(parity): T-visual styled-parity harness — color verification)

- **test(parity): styled (per-cell RGB) parity harness (`tests/visual_parity.rs`)**
  - The plain-text `pty_parity` harness can't see color, so `styles/` examples
    "passed" on text while their colors were unverified. New tiered styled harness
    runs BOTH the Rust example and the Python source through one `portable-pty +
    vt100 + COLORTERM=truecolor` path and compares per-cell `(char, fg, bg)` exactly
    — no tmux. Goldens generated from Python (`REGEN_STYLED=1`). First batch:
    `background`, `color`, `color_auto` **styled-verified PASS**; `background_tint`
    (auto-contrast `color: auto N%` + tint-blend rounding) and `colors` (default
    foreground emission) tracked as documented XFAILs.

### 2026-06-17 (fix(style): parse hsl()/hsla() colors)

- **fix(style): `Color::parse` accepts `hsl(h, s%, l%)` and `hsla(h, s%, l%, a)`**
  - Python Textual supports CSS `hsl()`; Rust dropped it (fell back to default).
    Added HSL→RGB conversion. Fixes e.g. `background: hsl(240,100%,50%)` (blue) and
    `color: hsl(...)` — caught by the new styled-parity harness on `styles/background`
    + `styles/color`. +unit test.

### 2026-06-17 (fix(layout): overflow-y containers don't clamp child height — promotes min_height)

- **fix(layout): a vertically-scrollable horizontal container lets children overflow**
  - `layout_horizontal` clamped each child's cross-axis height down to the
    container height unconditionally, so a child taller than the viewport (e.g.
    `min-height` larger than the container) never produced vertical overflow.
    Now, when the parent's resolved `overflow-y` is `auto`/`scroll`, the child
    keeps its resolved height so the content overflows and scrolls (Python
    parity) — mirroring the existing width-axis `allow_h_overflow` handling in
    `layout_vertical`. Gated on overflow so `overflow: hidden` rows still clamp.
    Combined with the plain-container scroll-host work, this promotes
    `docs_min_height` (PTY 185 → 186) and un-ignores the container scrollbar-gutter
    regression test.

### 2026-06-17 (feat(reactive): watch/recompose/validate/mutate + dynamic watch — Python parity)

- **feat(reactive): close the reactivity gaps on the existing reactive system**
  - The `#[derive(Reactive)]` system already generated getters/setters/`watch_<f>`/
    `compute_<f>` + init firing + an app-level dispatch bridge. Added the missing
    Python-parity pieces: `#[reactive(recompose)]` (rebuild the owner subtree on
    change), `#[reactive(validate)]`/`#[var(validate)]` (setter calls
    `validate_<f>` before store), generated `mutate_<f>(ctx)` (in-place mutation,
    fires unconditionally — Python `mutate_reactive`), `#[computed(…, watch)]`
    (computed fields fire watchers), a `recompose` flag threaded end-to-end
    (`ReactiveCtx::request_recompose`, widget- and app-level recompose via
    `App::recompose_app`), and a dynamic-watcher registry
    (`App::watch_reactive(node, field, cb)`). Converted **13/15** reactivity docs
    ports to the real API (dropping their workarounds): validate01, watch01,
    computed01, refresh01/02/03, recompose01/02, set_reactive01/02/03,
    dynamic_watch, world_clock01. (Examples are interactive → not static-scoreboard
    cases; verified via unit + integration tests; PTY 185, 0 regressions across all
    `#[derive(Reactive)]` widgets.) world_clock02/03 deferred — need a reactive
    `data_bind(ChildField=AppField)` primitive (documented gap).

### 2026-06-17 (feat(containers): plain Container/Horizontal/Vertical are scroll hosts)

- **feat(widgets/containers): `overflow-x/y: auto|scroll` on plain containers**
  - Previously only `ScrollView`/`VerticalScroll`/etc. reserved a scrollbar gutter
    and scrolled; a plain `Container`/`Horizontal`/`Vertical` with `overflow: auto`
    did neither. `Container` is now a first-class scroll host (scroll offset/viewport/
    virtual-size, scroll + drag events, content clipping), with scrollbar lanes
    mounted **lazily by the runtime only when resolved overflow is auto/scroll and
    content overflows** — so `overflow: hidden` containers get zero injected nodes
    (no tree perturbation). `ScrollView`'s inner content container suppresses its own
    lanes to avoid double-hosting. +regression tests. PTY 185, 0 regressions.

### 2026-06-17 (feat(widgets/Placeholder): width:auto shrinks to content)

- **feat(widgets/Placeholder): `auto_content_width` shrink-to-content**
  - A `width: auto` Placeholder now reports its label's cell width (Python parity),
    so it shrinks to content instead of flex-filling. (`height: auto` already matched.)

### 2026-06-17 (test(parity): promote 5 link_* examples — PTY 185)

- **test(parity): promote `docs_link_color`, `docs_link_color_hover`,
  `docs_link_background`, `docs_link_background_hover`, `docs_link_style`**
  - Enabled by the `Label` markup-default flip (link markup now renders; link
    colors drop in plain capture so they match Python). PTY 180 → 185.
    (`links` still has a `Static` trailing-newline residual.)

### 2026-06-17 (fix(layout): vw/vh axis, percent truncation, transparent-wrapper child sizing)

- **fix(style): `vw`/`vh` resolve against the correct viewport axis**
  - `resolve_scalar` took a single `viewport_size`, so `ViewWidth` and
    `ViewHeight` both resolved against whichever axis the callsite passed —
    `width: 25vh` became 25% of viewport *width*. Split into separate
    `viewport_width`/`viewport_height` (Python `_resolve_view_height` always uses
    height regardless of property), threaded the viewport through edge/grid/split
    resolution.
- **fix(style): percent/fraction scalars truncate, not round**
  - `resolve_scalar` used `.round()`; Python keeps exact fractions and floors at
    placement (`min-height: 75%` of 30 = 22, not 23). Switched Percent/Width/
    Height/ViewWidth/ViewHeight/Fraction to `.floor()`.
- **fix(layout): transparent `Node` wrapper no longer double-applies child size**
  - A `Node`-wrapped sized child (`#id{min-height}` on the wrapper, `height:50%`
    on the inner widget) collapsed the child height to `1fr` (dropping the value
    and the min-clamp), then the inner widget re-applied its `%` against the
    already-sized wrapper. Added axis-aware `wrapper_child_fill_axes`: the sole
    flow child fills the wrapper on axes the wrapper sized (adopting it, clearing
    that axis's min/max) — gated to axes where the wrapper has no explicit extent,
    so vertical centering (`center07`) still works. +5 tests, PTY 185, 0
    regressions. (Unblocks future `min_height`/`*_comparison` promotion once the
    out-of-lane plain-container overflow + Placeholder auto-size land.)

### 2026-06-17 (feat(widgets/containers): .id()/.class() builders on wrapper containers)

- **feat(widgets/containers): `.id()`/`.class()` on wrapper containers**
  - Wrapper containers delegate the `Widget` trait via `delegate_widget_to!` but
    did not expose the `.id()`/`.class()` seed builders that `Container` has, so
    `Horizontal::new().class("buttons")` didn't compile (ports used a
    `Container` + `layout: horizontal` workaround). Added `delegate_ident_methods!`
    to `Horizontal`, `Vertical`, `Center`, `Middle`, `CenterMiddle`, `Right`,
    `HorizontalGroup`, `VerticalGroup`, `ItemGrid` (delegating to the inner
    container) and `seed_ident_methods!` to `Row`; `Grid` already had them.
    Matches Python's `id=`/`classes=` kwargs. +11 tests.

### 2026-06-17 (fix(widgets/Label): default to markup=true — Python parity)

- **fix(widgets/Label): interpret console markup by default**
  - Python Textual's `Label`/`Static` parse console markup by default
    (`markup=True`); Rust `Label` defaulted to `false`, so `[link=…]`,
    `[@click=…]` and `[b]…[/]` rendered as literal tags. Flipped the default to
    `true` (use `.with_markup(false)` for literal text). rich-rs's markup parser
    already handled link/`@click`/style tags correctly — the gap was purely the
    Rust `Label` default. No regressions across the 180 PTY cases (no existing
    Label relies on literal-bracket text). Promotes the `link_*` examples
    (`link_color`, `link_color_hover`, `link_background`, `link_background_hover`,
    `link_style`). (`links` has a separate `Static` trailing-newline residual.)

### 2026-06-17 (test(parity): promote 10 port-wave docs examples — PTY 180)

- **test(parity): promote 10 newly-ported docs examples to PTY cases**
  - `docs_hello01`, `docs_hello02`, `docs_checker03`, `docs_fizzbuzz02`,
    `docs_tooltip01`, `docs_tooltip02`, `docs_content01`, `docs_key03`,
    `docs_binding01`, `docs_dom2` — the port-wave ports whose initial screen is
    stable and byte-matches Python. PTY 170 → 180, 0 regressions. (The other
    ported stubs are behavioral/time-varying — kept ported-not-promoted.)

### 2026-06-17 (port(examples): final 70 stub docs examples — porting complete)

- **port(examples): ported the last 70 auto-generated stub docs examples**
  - Faithful Rust ports of every remaining `TODO: Port` stub across `guide/`
    (widgets, reactivity, actions, core, input, compound, screens, content,
    workers, animator, command_palette, testing) and `tutorial/stopwatch`.
    All 70 build clean; every API was verified against real `textual-rs` source
    (no invented APIs) and independently reviewed. Where a faithful port needs a
    framework feature textual-rs lacks (named-action dispatch, per-widget
    `set_interval`, reactive/`watch_*`, custom leaf-widget authoring,
    `scroll_visible`, widget-level `query_one`/class mutation in handlers,
    `[@click=…]` action markup, locale datetime), the port documents the gap
    inline and uses the closest compiling equivalent — no faked parity. The docs
    examples are now fully ported; remaining work is parity + the noted gaps.

### 2026-06-17 (feat(widgets/Static): without_markup + with_expand convenience builders)

- **feat(widgets/Static): `without_markup()` and `with_expand(bool)` builders**
  - Mirror Python `Static(text, markup=False)` and `Static(expand=True)` by
    delegating to the inner `Label`. (Static defaults to `markup=True`.)

### 2026-06-17 (fix(style): CSS named colors use W3C values, not the ANSI palette)

- **fix(style): `color: white` is #ffffff (CSS), not the dim ANSI standard white**
  - `parse_color_like`/`Color::parse` deferred named-color resolution to rich-rs,
    whose color table uses the xterm/ANSI palette — so CSS keywords that collide
    with ANSI names resolved to terminal-palette values (`white`→(170/192,…),
    `cyan`→(0,170,170)) instead of their W3C values. Added the full CSS/W3C named
    color table (Python Textual `COLOR_NAME_TO_RGB`, 148 web keywords) consulted
    *before* the rich-rs fallback, so `white`=#ffffff, `cyan`=#00ffff, `green`=
    #008000 (≠`lime`), etc., while xterm-only names still fall through to rich-rs.
    `ansi_*` keywords keep their terminal-palette values. Unblocks correct
    scrollbar/border/title color parity (rendered colors now match Python).

### 2026-06-17 (feat(widgets): border_title/border_subtitle text setters on Label/Static)

- **feat(widgets/Label, widgets/Static): border-title / border-subtitle text API**
  - The render path already overlaid border title/subtitle text with align + color
    (`overlay_border_text`), but `Label`/`Static` had no way to set the text, so the
    `border_title*`/`border_subtitle*` examples rendered borders with no titles.
    Added `with_border_title`/`with_border_subtitle` builders + `set_border_title`/
    `set_border_subtitle` runtime setters (Python `widget.border_title = …`), and
    overrode the `Widget::border_title()`/`border_subtitle()` getters. `Static`
    delegates to its inner `Label`. Promotes `docs_border_title_align`,
    `docs_border_subtitle_align`, `docs_border_title_colors`, `docs_border_title`
    (PTY 166 → 170). (`border_sub_title_align_all` deferred — needs markup-in-titles
    + `[link=…]`, tracked with the link-markup work.)

### 2026-06-17 (fix(render): outline paints over the widget's own edge cells)

- **fix(render): `outline` overdraws the widget edge instead of reserving space**
  - The old `paint_outline` drew one cell OUTSIDE the layout rect. Python's
    `outline` reserves no space and is drawn over the widget's own perimeter
    cells (and over child content composited there). Replaced with
    `outline_edge_cells()` (perimeter glyphs from the existing border-char/style
    logic) painted AFTER children via `paint_outline_cells()`, so it overdraws
    final content on both leaves and containers. Promotes `docs_outline`,
    `docs_outline01` (PTY 164 → 166).
- **fix(render): scrollbar styling resolves from the host, not the bar itself**
  - `scrollbar-color`/`-background`/`-corner-color` are not inherited, so a
    `ScrollBar`/`ScrollBarCorner` reading its own style never saw the host's
    `scrollbar-color`. Added `current_host_style()` (the stack entry below the
    bar = the host during render) and resolve color/background/corner + base bg
    from it (Python `self.parent.styles.scrollbar_*`). Fixes the cyan thumb /
    host-colored corner; full scrollbar parity still pending out-of-lane
    named-color and geometry fixes.

### 2026-06-17 (feat(layout): width/height units + align middle + 1fr margin reserve)

- **feat(style): `w`/`h` scalar units (% of the parent's *other* axis)**
  - Added `Scalar::Width`/`Scalar::Height` so `40w` resolves to 40% of the parent
    WIDTH and `50h` to 50% of the parent HEIGHT regardless of which property they
    set (Python `_resolve_width`/`_resolve_height`). `parse_scalar` now parses them
    (after `vw`/`vh`, which share the trailing letter), `resolve_scalar` takes the
    parent width and height separately, and both dims are threaded through edge/
    min/max resolution (margin-adjusted on each axis). `interpolate_scalar` gained
    matching arms so the new units animate.
- **fix(layout): `align: … middle` reaches children behind a transparent wrapper**
  - When a node carries no `align` of its own and is the sole flow child of a
    transparent wrapper (`Node`) that does, it inherits that align — so an inner
    `Horizontal` applies the wrapper's center/middle to its buttons (Python has no
    wrapper; `#questions` *is* the `Horizontal`).
- **fix(layout): reserve collapsed margin before distributing `1fr` space**
  - Flow layouts divided the full available size among edges and subtracted each
    child's margin per-child afterward; fixed edges fold margin in but `fr` edges
    did not, so two `1fr` children split the full width and lost a cell each.
    Now the collapsed total margin is reserved from the resolver total *before*
    distribution and sizes resolve on margin-excluded boxes (Python
    `_resolve.resolve_box_models` + `layouts/horizontal.py`), fixing the inner
    `1fr` off-by-one. Subsumes the old partial `collapse_overlap` mitigation.
  - Promotes `docs_max_width`, `docs_max_height`, `docs_min_width`,
    `docs_nesting01`, `docs_nesting02` (PTY 159 → 164).

### 2026-06-16 (fix(Placeholder): default label derives from id)

- **fix(widgets/Placeholder): unlabelled Placeholder renders `#<id>`**
  - Python `Placeholder` defaults its label to `#{id}` when no label is given and
    an id is set (else `"Placeholder"`). Rust always rendered `"Placeholder"`.
    Added a custom `Placeholder::id()` that derives the default label from the id
    at build time (the id seed is consumed at mount, so it can't be recovered at
    render). Combined with the now-available direct `.id()` builder, the grid-span
    example placeholders attach their id directly (no `Node` wrapper) and render
    `#p1`…`#p7` like Python. Promotes `docs_column_span`, `docs_row_span`.


### 2026-06-16 (fix(layout/grid): faithful Python track resolution + spans + auto)

- **fix(layout/grid): port Python's exact-rational grid track resolution**
  - `layout_grid` converted each track to an `Edge` and ran the 1D resolver,
    which treated `auto` as `1fr` (no content sizing), `.ceil()`-weighted `fr`,
    and rounded `%`/fixed tracks independently (e.g. 25%→8 + 75%→23 = 31 > 30 →
    off-by-one rows). Replaced with a faithful port of Python `_resolve.resolve`:
    exact rational arithmetic, interleave frac+gutter and accumulate with
    floor-toward-neg-inf so rounding cascades and tracks tile the container
    exactly; `fr` weighted by value; `auto` columns/rows content-sized
    (intrinsic + chrome, respecting min/max) before resolving; `column-span`/
    `row-span` cells span the union of their tracks + gutters. Promotes
    `docs_grid_rows`, `docs_grid_layout4_row_col_adjust`, `docs_grid_layout_auto`.

### 2026-06-16 (fix(layout): clamp explicit/cross-axis sizes to min)

- **fix(layout): min-width/min-height clamp concrete + cross-axis edges**
  - `extract_child_spec` baked `min` into `Edge.min_size`, which the 1D resolver
    honors only for flexible main-axis edges — so an explicit size (`size=Some`)
    or the cross-axis was never min-clamped (`width:50%` + `min-width:60` resolved
    to 50%, ignoring the min). Now the resolved concrete width/height is clamped
    up to its `min_size` (outer, chrome-inclusive), matching Python
    `Widget._get_box_model`'s `max(content, min)`. (Several min/max docs examples
    still need the `w`/`h` Scalar units + non-`Node`-wrapped ids to fully match —
    tracked follow-ups.)


### 2026-06-16 (feat(widgets): complete the uniform `.id()`/`.class()` sweep)

- **feat(widgets): `.id()`/`.class()` on (nearly) every widget (Python parity)**
  - Followed up the macro from the prior entry by applying `seed_ident_methods!`
    to the remaining ~39 seed-bearing widgets (Checkbox, Switch, Rule, Log,
    RichLog, Select, Tree, DataTable, OptionList, SelectionList, RadioButton,
    RadioSet, Collapsible, ProgressBar, LoadingIndicator, ListView, ListItem,
    MarkdownViewer, Link, Pretty, Toast, Tooltip, DirectoryTree, ContentSwitcher,
    Header/Footer + their parts, Welcome, Spacer, Panel, Frame, Constrained,
    Overlay, Styled, ScrollBar, HelpPanel, KeyPanel, AppRoot, …). Every
    seed-bearing widget now accepts `.id(...)` / `.class(...)` directly, matching
    Python's universal `id=` / `classes=`.


### 2026-06-16 (feat(widgets): uniform `.id()` / `.class()` builders)

- **feat(widgets): seed-based `.id()` / `.class()` builders on more widgets**
  - Python gives every widget `id=` / `classes=`, but in Rust only a handful
    (Button, Input, the layout containers, Static, …) exposed `.id()`/`.class()`.
    Added two reusable macros — `seed_ident_methods!` (sets the widget's own
    `NodeSeed`, returning `Self`) and `delegate_ident_methods!` (forwards to an
    inner widget) — and applied them to `Label`, `Placeholder`, `Container`,
    `ScrollView`, `ScrollableContainer`, `VerticalScroll`, `HorizontalScroll`.
    Seed-based (single node) means a type selector and an `#id`/`.class` selector
    resolve to the **same** widget, matching Python — unlike wrapping in a `Node`,
    which splits them across two nodes. (Remaining seed-bearing widgets are a
    trivial one-line follow-up sweep.)


### 2026-06-16 (fix(render): clip descendants to content box for bordered/padded nodes)

- **fix(runtime/render): a node with gutter clips descendants to its content box**
  - Descendant content was only clipped to the content box for widgets that
    opt in (`clips_descendants_to_content`, e.g. scroll hosts). An over-wide
    bordered `Horizontal` therefore painted its children over its own right
    border column. Python's compositor clips every container's children to
    `region.shrink(gutter)`, so a node with any border/padding now clips its
    descendants to the content box (gutterless nodes are unchanged — content box
    == layout rect). Promotes `docs_containers06`.


### 2026-06-16 (fix(layout): Node-wrapped unset-height leaf fills the container)

- **fix(layout): a transparent `Node` wrapping an unset-height leaf fills (not 1fr-shares)**
  - `wrapper_unset_height` mapped any non-`auto` wrapped child to a `1fr` share,
    so N `Node`-wrapped unset-height leaves (e.g. `Placeholder`) split one track
    instead of each filling the container and overflowing. A transparent wrapper
    must mirror the wrapped child's intent: when the child's height is itself
    unset, return `None` so the wrapper inherits the bare-leaf fill-the-container
    rule (Python `Widget._get_box_model`); an explicit `1fr`/auto child is
    unchanged (keeps `docs_containers04`). Fixes a column of 19 placeholders not
    overflowing → no scrollbar → wrong width. Promotes `docs_layout05`.


### 2026-06-16 (fix(layout): unset height fills container; Static honors own padding)

- **fix(layout): an unset (`None`) height fills the full container, not a `1fr` share**
  - Rust conflated an unset height with `1fr`, so multiple bare children split the
    container 50/50. Python's `Widget._get_box_model` gives each unset-height child
    the **full** container height (so later siblings overflow below the fold). The
    `None`/`Auto` height arm in `extract_child_spec` is now split: `Auto` keeps
    content/flex behavior; truly-`None` with no intrinsic emits a fixed
    full-container edge. A transparent `Node` wrapper mirrors its child's intent
    (`auto`→shrink, else flex-fill) via a new `wrapper_unset_height` helper.
  - **fix(widgets/Static): `layout_height` includes the widget's own padding/border**
    — `Static::layout_height()` delegated to its inner `Label`, whose chrome
    resolves against the *Label* selector, so an app rule `Static { padding: 2 4 }`
    was invisible (box came out short → clipped + mis-centered). It now adds
    `Static`'s own resolved vertical chrome.
  - **fix(css): `HeaderClock` gets `dock: right; width: 10; padding: 0 1`** (Python
    grants these via `HeaderClock(HeaderClockSpace)` inheritance; Rust has no type
    inheritance, so the clock was stacking as a flow sibling).
  - Promotes docs parity cases render_compose and layout01.


### 2026-06-16 (fix(runtime): scrollable virtual size includes dock spacing + child margins)

- **fix(runtime/render): `host_content_extent` mirrors Python `DockArrangeResult`**
  - The scrollable virtual size of a scroll host unioned only the border-boxes of
    its non-docked children. Python's `DockArrangeResult.total_region` unions each
    placement grown by its **margin**, then grows the result by the **docked
    scroll-spacing** per edge. Two contributions were missing: (1) a docked
    Header/Footer enlarges the scrollable height by its thickness (max per edge),
    and (2) a flow child's margin enlarges the extent. Adding both fixes the
    modal dialog scrollbar thumb (was offset/absent) and horizontal-scroll virtual
    width. Promotes docs parity cases modal01/02/03 and layout06.


### 2026-06-16 (fix(renderables/LinearGradient): half-block glyphs)

- **fix(renderables/LinearGradient): emit `▀` half-block glyphs (Python parity)**
  - `LinearGradient::render` emitted plain space cells with a background color
    only; Python's `LinearGradient` emits upper-half-block `▀` glyphs carrying a
    foreground (top sample) + background (bottom sample) for 2× vertical color
    resolution. Rewrote it to match Python's algorithm (per-cell fg/bg geometry
    + a 50-step quantized color ramp). Fixes `how-to/render_compose` rendering as
    blank under plain-text capture (the bg-only spaces were invisible). Confirmed
    "render + compose" itself was already supported (added a regression test).


### 2026-06-16 (fix(App): default title to the app type name)

- **fix(App): `title` defaults to the app type's name (Python parity)**
  - The `TextualApp::title()` default was a hardcoded `"textual-rs"`. Now it
    defaults to `""` (sentinel for "unset") and the runtime falls back to the
    app type's name (final path segment of `std::any::type_name`), mirroring
    Python `self.title = self.TITLE if self.TITLE is not None else
    type(self).__name__`. An explicit `title()` override (or `set_title`) still
    wins. Also dropped the now-unnecessary hardcoded `Header::new().title(...)`
    workaround from the modal screen examples and aligned their struct names.


### 2026-06-16 (fix(Placeholder): stop double-centering its label)

- **fix(widgets/Placeholder): render bare content; let `content-align` center it**
  - `Placeholder` pre-centered its label inside `render()` AND the framework's
    `content-align: center middle` default re-centered it during composition
    (the composition measure used `trim_end`, which doesn't strip the widget's
    leading pad), netting a uniform +1-column shift. `render()` now emits bare
    content (matching Python `_placeholder.py`, where all centering is done once
    by `content-align`). Promotes docs parity cases containers01–05, 07–09 and
    layout02–04.


### 2026-06-16 (feat(ListView): arena-composed ListItem widgets)

- **feat(widgets/ListView+ListItem): first-class arena composition**
  - `ListView` was a flat inline-rendered widget over a `Vec<String>` with a
    hardcoded `› ` marker. It now composes real `ListItem` children through the
    arena (the RadioSet/OptionList pattern): each `ListItem` wraps arbitrary
    child widget(s) (typically a `Label`), the marker is gone, and the highlight
    is **background-only** (Python parity). Keyboard (up/down/enter/home/end/
    page) and mouse selection drive the highlight; `ListView` emits
    `Highlighted`/`Selected` (carrying index+item) and stages the initial
    `Highlighted` at mount. Highlight/hover are applied to child nodes via a new
    `Widget::child_classes_for_tree` hook (synced in the same pass as
    `child_display_for_tree`). The headless state API (`selected`/`offset`/
    `set_items`/…) is preserved so `command_palette` is unaffected. New public
    API: `ListView::from_list_items`, `ListItem::new/from_text/with_id/…`;
    `ListView::new(Vec<String>)` retained. Promotes `docs_list_view`.
- **fix(widgets/Label): `layout_height` reports outer height**
  - `Label::layout_height()` returned only its content height, so a styled
    `Label { padding: 1 2 }` overflowed its box (two stacked padded labels
    overlapped). It now adds the resolved vertical chrome, matching the
    `extract_child_spec` height-arm convention. Regression:
    `label_layout_height_includes_vertical_padding`.

### 2026-06-16 (fix(render): clear re-emits full screen, not a stale diff)

- **fix(runtime/render): diff against a blank frame when clearing the terminal**
  - When a frame was drawn with `clear_on_next_render` set, a `Clear` control
    blanked the terminal but the diff was still taken against the *previous*
    frame — so unchanged cells weren't re-emitted and got wiped off-screen
    (a stale-frame diff). Added `diff_body_for_draw`, used at all three render
    paths: when clearing it diffs `next` against a blank frame of the same size,
    so every visible cell is re-emitted after the wipe (non-clear paths are
    unchanged). Regression: `clear_before_draw_reemits_unchanged_content`. This
    also lets dynamic mount/remove use a clear when appropriate without losing
    siblings.

### 2026-06-16 (fix(delegate): forward arena composition hooks)

- **fix(widgets/delegate): `delegate_widget_to!` forwards the arena hooks**
  - The full-delegation macro forwarded `take_composed_children` but not
    `take_child_decl_meta`, `take_child_handle_sinks`, or
    `take_pending_mount_messages`. Delegated wrapper containers (`Vertical`,
    `Horizontal`, `Center`, `Middle`, `VerticalScroll`, …) therefore silently
    dropped declared child ids/classes, handle-sinks, and mount-time messages —
    on the initial build path too, not just dynamic mount. Added the three
    forwarding arms (method count 60→63). Regression: `tests/delegate_forwarding.rs`.

### 2026-06-16 (feat(runtime): dynamic mount/remove under a live parent)

- **feat(runtime): `mount_under` / `mount_before` / `mount_after` / `remove`**
  - Added a runtime API to insert or remove widgets under an already-mounted
    parent (resolved by selector or `NodeId`), mirroring Python
    `Widget.mount(..., before=/after=)` / `remove`. All mounts go through the
    canonical `mount_extracted_recursive` path, so composed children, child
    decl-metadata (id/classes), child handle-sinks, and mount-time messages all
    fire exactly as on initial build; `remove` clears focus across the subtree
    and emits `Unmount`. A structural mutation requests a full relayout+repaint
    (without a terminal clear, to avoid a stale-frame diff). `WidgetTree` gains
    `mount_at`/`child_index`/`reorder_child`. Unblocks `tutorial/stopwatch06`
    (`action_add_stopwatch`/`action_remove_stopwatch`; not promoted — the running
    clock is timer-driven, verified structurally instead).

### 2026-06-16 (fix(scrollbar): thumb glyph parity with Python)

- **fix(widgets/scrollbar): partial-block thumb glyphs match Python both axes**
  - `ScrollBarRender::render_bar` is rewritten to mirror Python
    `ScrollBarRender.render_bar` exactly: a single divmod-based start/end
    computation shared by both axes, the eighth-block glyph tables
    (`VERTICAL_BARS` lower blocks, `HORIZONTAL_BARS` left blocks), head/tail
    partial-block glyphs (`bars[7 - bar]`, space-skipped) with Python's
    fg/bg/reverse mapping per axis, and a `color=bar, reverse=true` thumb body.
    Fixes a horizontal head-glyph bug (used `HORIZONTAL_BARS[start_bar-1]`
    instead of `bars[7-start_bar]`, dropping the head entirely at `start_bar==0`)
    and the duplicated per-axis math. New unit tests assert the glyph sequence +
    per-cell fg/bg/reverse against live Python output, including fractional edges.

### 2026-06-16 (feat(runtime): widgets can post messages at mount; Select Changed)

- **feat(runtime): mount-time message hook for arena widgets**
  - Python widgets can post messages from `on_mount`, but the arena
    `on_mount(&mut self)` has no `EventCtx`, so an arena widget had no way to
    emit a message reflecting its initial state. Added
    `Widget::take_pending_mount_messages()` (default empty); the runtime drains
    it once right after a node mounts and routes each message through the normal
    bus with the mounted node as sender/control — a drain-at-mount adapter over
    the core message flow (same pattern as `take_child_decl_meta`), not a
    separate dispatch path. Wired into both the initial-mount loop and the
    dynamic-mount lifecycle path.
  - **fix(widgets/Select): `Changed` posts at mount for the auto-selected value**
    — `Select(allow_blank=false)` auto-selects the first option; it now stages a
    `SelectChanged` at mount (Python `_watch_value` parity), so apps observe the
    initial selection at startup. Promotes `docs_select_widget_no_blank`.

### 2026-06-16 (fix(Rule): orientation variant class + margins)

- **fix(widgets/Rule): orientation margins/sizing now apply (variant class)**
  - `Rule` pushed `rule--horizontal`/`rule--vertical` into its seed classes, but
    the default CSS (and Python) target the DOM variant classes
    `Rule.-horizontal` / `Rule.-vertical`. The mismatch meant the orientation
    rules (`margin: 1 0` + `width: 1fr` for horizontal, `margin: 0 2` +
    `height: 1fr` for vertical) never matched, so margins and flex sizing were
    silently dropped. Use the `-horizontal`/`-vertical` variant classes, add a
    `style_classes()` override so off-tree style resolution (`content_width`,
    `render`) sees the variant, and resolve `render` via the DOM style path.
  - Promotes docs parity cases `docs_horizontal_rules` / `docs_vertical_rules`.

### 2026-06-16 (fix(css): hatch property maps named patterns + blends opacity)

- **fix(css/hatch): named patterns map to glyphs; opacity blends over background**
  - The `hatch:` parser took the first *letter* of the pattern keyword as the
    fill char (so `hatch: right $foreground` painted `r`, never `╱`) and dropped
    the optional opacity. Now the named patterns map to Python's `HATCHES` glyphs
    (`left ╲`, `right ╱`, `cross ╳`, `horizontal ─`, `vertical │`) or accept a
    quoted single-cell char, and a trailing `N%` scales the color alpha
    (`color.multiply_alpha`). The painter blends the (alpha-scaled) hatch color
    over each blank cell's actual background (`flatten_over`) instead of painting
    a flat opaque color, matching Python's `apply_hatch`. Adds `hatch: none`.

### 2026-06-15 (fix(layout): border-box explicit size never collapses below chrome)

- **fix(layout): a border-box explicit size keeps its full border/padding chrome**
  - Following Python `Widget.get_box_model` (`content = max(0, size - gutter)`,
    box = `content + gutter`), an explicit border-box size smaller than the
    widget's own chrome now clamps the box up to `border + padding (+ margin)`
    instead of collapsing. Fixes an `Input { height: 1; border: tall }` (chrome 2)
    rendering only its top border row — it now renders both border rows with zero
    content height, matching Python. Applied in both `extract_child_spec`
    (flow/grid) and `carve_edge` (dock/split). Regression test:
    `tests/border_box_layout.rs`.

### 2026-06-15 (fix(Switch): render slider via ScrollBarRender; content width 4)

- **fix(widgets/Switch): slider is a horizontal scrollbar thumb (Python parity)**
  - Replaced the ad-hoc knob/fractional-edge drawing with Python's actual
    `Switch.render`: a `ScrollBarRender(virtual_size=100, window_size=50,
    position=slider_pos*50, vertical=False)` whose thumb occupies half the track
    and slides left (off) → right (on). Thumb/track colors come from the
    `switch--slider` component style flattened over the resolved surface.
  - **fix(widgets/Switch): `content_width` returns the bare content width (4)**
    matching Python `Switch.get_content_width`; padding/border chrome is added by
    the layout engine, not baked into the widget's reported width.

### 2026-06-15 (fix(layout): collapse adjacent margins in horizontal layout)

- **fix(layout/horizontal): adjacent child margins collapse (max, not sum)**
  - Horizontal layout summed neighbouring children's margins, so a row of
    buttons with `margin: 2 4` got an `8`-cell inter-button gap (and a shifted
    centered start) instead of Python's collapsed `max(right, left) = 4`.
    Positioning now advances by `box_right_edge + max(this.margin.right,
    next.margin.left)`, and width distribution subtracts the total collapse
    overlap so freed space goes to any `fr` sibling (guarded so a flexible first
    child, whose edge carries no folded margin, isn't reduced). `apply_parent_align`
    needed no change — it derives the align box from the placed margin-grown rects.
  - Promotes docs parity cases `docs_on_decorator01` / `docs_on_decorator02`.

### 2026-06-15 (feat(Markdown): blockquote bars + nesting)

- **feat(widgets/Markdown): render blockquotes with bars and nesting**
  - `build_markdown_children` dropped blockquotes entirely (catch-all arm) and
    flattened their content into plain paragraphs — no `▌` bars, no nesting. The
    `MarkdownBlockQuote` default CSS (`border-left: outer; padding: 0 1`) existed
    but no widget used it. Added a `MarkdownBlockQuoteBlock` widget (style type
    `MarkdownBlockQuote`) holding a recursive quote-child tree; the outer bar/pad
    come from the existing CSS, and each additional nesting level prefixes `▌ `
    with blank-bar margins around nested quotes (Python parity). Rewrote
    `build_markdown_children` to walk the top-level event stream so blockquotes
    become quote blocks while every other block delegates to the unchanged
    `parse_markdown_blocks`, preserving document order and all existing behavior.

### 2026-06-15 (fix(compose): with_compose preserves child id/classes/handle)

- **fix(compose): `with_compose` no longer drops child id/class/handle metadata**
  - `AppRoot::with_compose` and `Container::with_compose` flattened each
    `ChildDecl` to a bare `Box<dyn Widget>`, discarding the decl's `id`,
    `classes`, and `handle_sink`. Children mount via the
    `take_composed_children()` extraction path (which only reads the widget's own
    seed), so `.with_id(...)` / `.with_classes([...])` on a composed child never
    reached the node and CSS id/class selectors silently failed to match (e.g.
    `muted_backgrounds` `.text-*` padding/bg/fg never applied). Added a
    `Widget::take_child_decl_meta()` hook and a single `apply_child_decl_meta`
    helper (mirroring `App::mount_declarations`: `set_css_id` + `add_class`),
    applied at every runtime mount site that already fires child handle-sinks
    (`mount_declarations`, `mount_extracted_recursive`, `recompose_node_subtree`,
    ScreenHost). Classes are *added* (the widget's own component class coexists
    with user classes, matching Python). `handle_sink`s bound via
    `HandleSlot::bind` on composed children now also fire.

### 2026-06-15 (fix(DirectoryTree): suppress twisty, render emoji prefix)

- **fix(widgets/DirectoryTree): folder/file emoji replaces the twisty prefix**
  - Python's `DirectoryTree.render_label` overrides the base `Tree` prefix so the
    `📂`/`📁`/`📄` emoji *is* the node prefix and the expand/collapse twisty
    (`▼`/`▶`) is suppressed. Rust rendered both (`▼ 📂 ./`), and used the full
    path text for the root instead of `path.name` (basename). Added a
    `Tree::set_hide_twisty` flag (default off, no plain-`Tree` change) threaded
    through prefix/width/hit-test/render; `DirectoryTree` enables it and uses the
    basename for the root label. The toggle hit-zone extends over the leading
    emoji so clicking a folder icon still expands/collapses it (Python parity).

### 2026-06-15 (fix(Digits): honor parent-forwarded text-align; OptionList scrollbar)

- **fix(renderables/Digits): honor the parent-forwarded `justify` (text-align)**
  - `Digits::render` always re-resolved its own `Digits` type meta (default
    `text-align: left`), ignoring the `options.justify` that the render path
    forwards from a node's resolved `text-align` (engine #19). So a typed wrapper
    like `class TimeDisplay(Digits)` with `text-align: center` couldn't center its
    glyphs. Now `Digits::render` uses the forwarded justify when set, falling back
    to its own text-align. Fixes tutorial stopwatch03/04 (TimeDisplay centering).
- **feat(widgets/OptionList): real vertical scrollbar**
  - OptionList had no scrollbar mechanism (flat self-scroll). Added a dedicated
    `ScrollBar` child + `scroll_virtual_content_size`/`scroll_offset_f32`/
    `on_message(ScrollbarScrollTo)` (mirroring RichLog), registered
    `OPTION_LIST_VSCROLLBAR_ID` in render.rs. The host-scrollbar layout now keeps
    the outer box for chrome-bearing self-rendering hosts (OptionList) while
    chrome-less hosts (Log/RichLog) keep the viewport box (no regression). Fixes
    option_list_tables (the vbar thumb now renders via the #40 clip fix).

### 2026-06-15 (fix(runtime): dedicated scrollbar thumb no longer clipped away)

- **fix(runtime): paint a dedicated scrollbar into its gutter (don't clip it out)**
  - For self-scrolling hosts with no content children (Log/RichLog/KeyPanel),
    `apply_host_scrollbar_layout` shrinks the host `layout_rect` to the viewport
    (excluding the scrollbar gutter). The parent then clips the host's descendants
    to that shrunk rect, so the dedicated scrollbar child (which sits in the
    gutter) was entirely outside the clip and its thumb/track were dropped. In
    `render_tree_node`, expand the clip for `is_dedicated_scrollbar` children to
    cover their own layout rect (bounded by the frame) — applied only to that
    child's context, so siblings are unaffected. Fixes the missing vertical
    scrollbar thumb in `log` (and `rich_log`).

### 2026-06-15 (fix(widgets/Toast): severity border, word-wrap, side margin)

- **fix(widgets/Toast): paint the severity `▌` border in off-tree composition**
  - The runtime composes toasts off the arena tree via `render_styled()`/
    `selector_meta_generic()`, which read `style_classes()`. `Toast` didn't
    override it, so the severity class (`-information`/`-warning`/`-error`) was
    invisible and `Toast.-warning { border-left: outer $warning }` never matched.
    Store the class in a field + override `style_classes()`.
- **fix(widgets/Toast): word-wrap long messages** (Python `Static`/`Content`):
    `render` wrapped via `Text::wrap()` at the content-box width; `layout_height`
    counts wrapped lines.
- **fix(runtime): toast side margin 1, not 2** — Python's `ToastRack` has
    `overflow-y: scroll` (1-col gutter), so the toast box sits 1 col from the
    right edge. (`TOAST_SIDE_MARGIN`) Rust toast output is now byte-identical to
    Python. (Not in the strict PTY harness: notifications auto-dismiss → flaky
    golden.)

### 2026-06-15 (feat(widgets): OptionList multi-row options + TextArea gutter width)

- **feat(widgets/OptionList): render multi-row option content (line-based model)**
  - OptionList capped every option at one display row, collapsing multi-row Rich
    renderables (e.g. tables) to their first line. Reworked to a line-based model
    mirroring Python `OptionList._lines`: `render_rich_lines` returns all display
    lines (splitting on line-control segments AND embedded `\n`), with `item_height`/
    `total_lines`/`line_map`; render, scroll offset, `ensure_visible`, mouse/hover
    hit-testing, and `layout_height` all work in line space. (`src/widgets/option_list.rs`)
- **fix(widgets/TextArea): line-number gutter uses a 2-cell margin**
  - The gutter was `digits + 1` (one trailing space); Python uses `digits + 2`
    (`f"{n:>digits}  "`), so every code line was shifted 1 cell left. Now matches.
    (`src/widgets/text_area.rs`) Fixes text_area_example + text_area_selection.

### 2026-06-15 (fix(layout): don't double-count a bordered auto-height leaf's chrome)

- **fix(layout): auto-height measurement adds only margin to a leaf's outer height**
  - `measure_child_outer_height` added the full vertical chrome (margin+border+
    padding) on top of a child's measured height — but for a LEAF widget that
    height came from `layout_height()`, which already includes the widget's own
    border/padding (OUTER height). So a bordered `height: auto` leaf was inflated
    by its border (e.g. a `Checkbox` measured 5 rows instead of 3), making an auto
    container over-measure and spuriously overflow. Now: if the child reports its
    own `layout_height()` (leaf, already OUTER) add only margin; only the
    children-sum CONTENT path (drained/auto containers) adds the full chrome. The
    width side is unchanged (its `content_width()` is pure content by contract).
    Fixes `checkbox` (VerticalScroll no longer over-measures → no phantom
    app-root scrollbar).

### 2026-06-15 (fix(scrollbar): re-expand content when a reserved gutter is unneeded)

- **fix(runtime): scrollbar gutter converges (re-expands when not needed)**
  - The host-scrollbar pass laid children out at a reduced viewport when the first
    measurement reserved a lane, but if a later (corrected) measurement showed no
    overflow it never re-laid-out at the full width — leaving the gutter reserved
    forever. Common trigger: a `Markdown` child measured tall before its
    `on_layout` corrected its width, reserving a vertical gutter that stuck. The
    pass now iterates (capped) — re-laying out at the resolved viewport and
    recomputing until the reserved lanes stabilize — so an unneeded gutter is
    released and the children re-expand. Fixes `collapsible`.

### 2026-06-15 (fix(scrollbar): no spurious cross-axis scrollbar on partial content)

- **fix(scrollbar): reserve a scrollbar lane only on genuine overflow**
  - `ScrollbarPolicy::resolve` (and the host-scrollbar call sites) clamped the
    virtual content extent up to the widget size. For a self-rendering scrollable
    whose content is narrower/shorter than its box (e.g. a `DataTable` filling a
    120-wide box with only ~13 cols of columns), the clamp made the content appear
    to exactly fill the box — so as soon as one axis reserved a lane (e.g. a
    vertical scrollbar), the clamped extent "overflowed" the reduced viewport and
    a **spurious** scrollbar was reserved on the other axis (stealing a row/col).
    Now the actual virtual extent is used per axis. Fixes data_table_fixed (the
    phantom horizontal scrollbar that stole the last row) and removes a phantom
    hbar in the keys preview.

### 2026-06-15 (feat(widgets): Collapsible arena composition + DataTable cell justify)

- **fix(widgets/Collapsible): compose `CollapsibleTitle` + contents as arena nodes**
  - `Collapsible` painted its title and children from its own `render()`, but in
    arena mode a widget that returns `take_composed_children()` is a container
    whose own render is chrome-only — so the title glyph/label and all content
    were dropped, leaving only the top border. Now `take_composed_children()`
    yields `[CollapsibleTitle, CollapsibleContents(children)]` as real arena nodes
    (mirroring Python's `compose()`); `CollapsibleTitle` renders `symbol + label`
    and the `&.-collapsed > Contents { display: none }` rule works through the
    normal CSS/layout path. Fixes collapsible_nested + collapsible_custom_symbol.
- **feat(widgets/DataTable): per-cell justification (`CellJustify`)**
  - Added `CellJustify { Left, Right, Center }` and `set_cell_justify` /
    `set_row_justify` / `set_all_data_cells_justify` (Python `Text(justify=…)`
    cells). Data cells can now right/center-align within their column (headers
    stay left). Also: column widths size to content (`max(1)`, was `max(3)`), and
    `DataTable` flex-fills its container width (`content_width` → None +
    `auto_content_width` for explicit `width:auto`). Fixes data_table_renderables.

### 2026-06-15 (fix(widgets/Checkbox): Rich markup labels + content width)

- **fix(widgets/Checkbox): parse the label as Rich markup**
  - `[b]…[/b]` / `[magenta]…[/]` were rendered literally and inflated the width.
    The label is now parsed via `Text::from_markup(.., emoji=false)` (emoji
    shortcodes left literal, matching Python), so markup is stripped to plain
    text for rendering and measurement.
- **fix(widgets/Checkbox): content_width returns pure content (no double chrome)**
  - `content_width()` added its own border/padding chrome, which the layout's
    auto-width measurement then added again — making the box too wide. It now
    returns pure content (`3` for `▐X▌` + `2` for the label's 1-cell pad + label
    width), matching Python `ToggleButton.get_content_width`. Box widths now match
    Python exactly. (The checkbox demo still has a separate ~1-col centering /
    focus-scroll artifact tracked in the engine ledger; not promoted yet.)

### 2026-06-15 (feat(widgets/DataTable): per-row labels)

- **feat(widgets/DataTable): per-row labels render as a non-data label column**
  - Added `DataTable::add_row_labeled(row, label)` (Python `add_row(..., label=…)`).
    When any row is labelled and `show_row_labels` is set, a label column is
    rendered as a prefix to the left of the data cells (header label cell blank),
    sized to the widest label, and included in the table's content width. The
    label column is not a data/cursor column. Fixes the `data_table_labels` demo.

### 2026-06-15 (fix(layout): auto-height container fills an `fr` child / Center+Middle)

- **fix(layout): an `auto`-height container whose children are all dynamic-height
  fills an `fr` child instead of collapsing**
  - Mirrors Python `Layout.get_content_height`: a non-docked `height: auto`
    container whose displayed children all have a dynamic height (`auto`/`fr`/`%`)
    is measured against the full container height, so an `fr` child fills it.
    Previously such a container collapsed to the child's minimum — e.g.
    `Center(height:auto) > Middle(height:1fr)` was 1 row tall, so vertical
    centering (`Middle`'s `align-vertical: middle`) had no slack. Scoped to the
    all-dynamic + has-`fr` case to preserve size-to-content for every other auto
    container. Fixes `Center`/`Middle` vertical centering (progress_bar_gradient).
- **fix(widgets/ProgressBar): keep half-cell precision in the filled bar**
  - The render passed a pre-rounded integer fill length to the `Bar` renderable,
    dropping the half-cell (`╸`/`╺`) that Python produces by passing the
    fractional `width * percentage`. Now passes the fractional extent.

### 2026-06-15 (fix(widgets/ListView): nav bindings hidden from the Footer)

- **fix(widgets/ListView): cursor/select bindings no longer leak into the Footer**
  - `ListView::bindings()` declared `up`/`down`/`enter` with `show=true`, so a
    focused `ListView` flooded the `Footer` with its navigation hints. Marked them
    `.hidden()` to match Python (all `ListView.BINDINGS` are `show=False`).

### 2026-06-15 (fix(widgets): ProgressBar bar glyphs + Sparkline empty buckets)

- **fix(widgets/ProgressBar): render the Python bar glyphs `━`/`╺`/`╸`**
  - The determinate/indeterminate/gradient render paths overrode the `Bar`
    renderable with `█` (filled) and a **space** background, so the track was
    invisible (a 0% bar showed as blank). Dropped the overrides to use the
    `Bar` defaults (`━` bar, `╺`/`╸` halves), matching Python's
    `renderables/bar.py`. Fixes `progress_bar`.
- **fix(renderables/Sparkline): drop empty buckets when width > data length**
  - `Sparkline::buckets` kept empty partitions and rendered them as min-value
    (`▁`) columns. Python's `_buckets` yields a partition only `if partition`
    and re-samples the survivors across the width (`step = len(buckets)/width`).
    Now matches — fixes the spurious `▁` columns in `sparkline_colors`.

### 2026-06-15 (fix(layout/grid): size grid children by their own box model)

- **fix(layout/grid): grid children are sized by their own box model within the cell,
  not stretched to fill it**
  - `layout_grid` set every child to the full cell size (minus margin), so a
    `height: auto` widget (e.g. a `Button`) was stretched to fill a tall grid row
    instead of sitting at its natural height. Now each child resolves its own
    `width`/`height` against the cell — unset/`1fr` fills, `auto` sizes to content,
    explicit resolves against the cell — mirroring Python's
    `widget._get_box_model(cell_size)` (`layouts/grid.py`). Children with `100%`
    (e.g. five_by_five `GameCell`) still fill, so no regression there.
  - Fixes the docs app grid examples: question02/03, question_title01/02.

### 2026-06-15 (fix(layout/text): text-align + transparent-wrapper auto-sizing)

- **fix(widgets): `text-align: center/right/justify` now honored for Label/Static**
  - The render path only fed a hardcoded justify for `Button`; all other widgets ignored the
    resolved `text_align`. `render_widget_with_meta` now maps `resolved.text_align` →
    `content_options.justify` generically, so text alignment works for every widget.
- **fix(layout): transparent styling wrappers (`Node` from `.id()`/`.class()`) adopt the wrapped
  widget's auto-sizing instead of flex-filling**
  - `Static::id(..)`/`.class(..)` wrap the widget in a transparent `Node`, and the CSS (`#id`/
    `.class`) lands on the `Node`. With no sizing defaults the wrapper filled the screen. Added
    `Widget::is_transparent_wrapper()` + `wrapper_child_auto_axes()`: when the wrapped child is
    `auto` on an axis and the wrapper's axis is unset, the wrapper shrinks-to-content; an unset
    axis otherwise keeps `1fr` fill. `content-align` on such a wrapper maps to the child's `align`.
- **fix(widgets): Label/Static `width: auto` sizes to rendered text width**
  - Added `Widget::auto_content_width()` (consumed only by the `width: auto` measurement) so an
    explicit `width: auto` sizes to content; an UNSET width still fills (`content_width()` stays
    `None`, so `1fr` fill is unaffected).
- **fix(layout): drop double-counted horizontal chrome on measured-auto width**
  - `extract_child_spec`'s auto-WIDTH arm already adds full horizontal chrome; the vertical/
    horizontal layout no longer pre-adds it (height arm still adds vertical chrome). Width-
    dependent auto height now seeds the wrapped subtree at the real content width before measuring.

### 2026-06-15 (fix(DataTable): nav bindings hidden from the Footer)

- **fix(widgets/DataTable): cursor/select bindings no longer leak into the Footer**
  - `DataTable::bindings()` declared its `up`/`down`/`left`/`right`/`enter,space` bindings with
    `show=true`, so a focused DataTable flooded the `Footer` with its navigation hints (overflowing
    the app's own bindings). Marked them `.hidden()` to match Python (all DataTable bindings are
    `show=False`). Fixes the `data_table_sort` footer.


### 2026-06-15 (fix: RadioSet/Checkbox toggle glyphs + drained-auto-container chrome)

- **fix(layout): bottom-up-measured auto containers include their own border/padding**
  - `measure_intrinsic_content_*` returns only children's summed extents (it recurses, so it must
    not double-count). The call sites in `layout_vertical`/`layout_horizontal` now add the
    container's own border+padding (`own_box_chrome`) to the measured intrinsic, so a `height:auto`
    container with a border (e.g. `RadioSet { border: tall }`) is no longer clipped by it.
- **fix(widgets/RadioSet): keep buttons internal; report full height**
  - `RadioSet` renders its buttons inline but also drained them into the arena, leaving the inline
    render blank (height 1). It no longer drains (monolithic, as its render/navigation assume) and
    `layout_height` adds its border/padding chrome. The inner radio glyph is now always `●` (Python
    `ToggleButton` shows it always; selected state is color-only), not toggled `●`/`○`.
- **fix(widgets/Checkbox): render `▐X▌` like Python's ToggleButton**
  - Was `☐`/`☑`. Now renders `▐X▌` (the `X` always present; checked state via the `.toggle--button`
    color), matching Python and `SelectionList`.

### 2026-06-15 (fix(OptionList): remove hardcoded double indent on plain options)

- **fix(widgets/OptionList): plain options no longer double-indent**
  - The plain-text render path hardcoded a 2-space prefix on every option, on top of the
    `OptionList` default `padding: 0 1` — so options rendered 3 columns in from the border instead
    of Python's 1. Dropped the hardcoded prefix; the CSS padding alone supplies the single-space
    inset (Python parity). `SelectionList` (own button prefix) and the closed `Select` are
    unaffected.

### 2026-06-15 (fix(widgets): Checkbox/Switch/Digits auto-height include border chrome)

- **fix(widgets): `Checkbox`/`Switch`/`Digits` report their border/padding in `layout_height()`**
  - These returned content-only heights (`1`/`1`/`3`), so under `height: auto` with a border
    (`border: tall`/`double`) the layout allocated only the content rows and clipped the border —
    a bordered checkbox/switch showed just its top border, a bordered Digits lost 2 of its 3 glyph
    rows. They now add their resolved vertical chrome (new shared helper
    `helpers::resolved_vertical_chrome`), conforming to the documented contract that
    `layout_height()` reports the OUTER auto height (content + own border + padding) — the same
    contract `Input`, `SelectionList`, `Pretty`, `SelectCurrent`, and five_by_five's `GameCell`
    already follow. Fixed per-widget (not centrally) to avoid double-counting for widgets that
    already include chrome.

### 2026-06-14 (feat(css): support `/* */` block comments in stylesheets)

- **feat(css): the stylesheet parser now strips `/* */` comments**
  - The parser scans raw text for `{`/`}`, so a comment before a rule was folded into the
    following selector and silently dropped that rule. `parse_with_issues`
    (`src/css/selectors/parser.rs`) now strips `/* */` spans up front (replaced with whitespace,
    newlines preserved, so token positions stay stable; an unterminated `/*` consumes to EOF, per
    standard CSS). Comments may now appear before/between rules and inside blocks.

### 2026-06-14 (feat(layout): block-wise align + bottom-up auto-size container measurement)

- **feat(layout): `align` translates the whole arrangement by a single offset (block centering)**
  - `apply_parent_align` (`src/layout/mod.rs`) centered each child independently on the cross
    axis. It now computes one bounding box over all (margin-grown) children and applies a single
    `dx`/`dy` to every child, matching Python Textual's `_arrange.py`
    (`WidgetPlacement.get_bounds` + `Styles._align_size` → one `placement_offset`). Children keep
    their relative positions, so e.g. a narrow buttons row and a wide content box both shift to the
    same left edge instead of each being centered separately. (Margin-grown bounds retained.)
- **feat(layout): bottom-up intrinsic measurement for explicitly auto-sized drained containers**
  - A container whose renderable children are drained into the arena tree reports
    `content_width()`/`layout_height()` == `None`, so `width: auto`/`height: auto` was treated as a
    flex edge (filled its slot) instead of sizing to content. `src/layout/common.rs` adds
    `measure_intrinsic_content_width`/`_height` (sum children's outer extents along the layout axis,
    max across; `fr` children contribute their min, matching Python `get_content_*`), wired into
    `layout_horizontal`/`layout_vertical` ONLY when the dimension is explicitly `Scalar::Auto`
    (an UNSET dimension keeps flex-fill, so `Screen` and default `1fr` containers are unaffected —
    narrow blast radius).
- **test(parity): promote `docs_content_switcher` to `Status::Pass`**
  - With the above (plus the earlier Node/ContentSwitcher fixes and the ported example CSS), the
    `content_switcher` docs example matches the Python golden: buttons + switcher block-aligned, the
    active DataTable filling the rounded `1fr` ContentSwitcher box.

### 2026-06-14 (fix(layout): Node/ContentSwitcher arena-child sizing + visibility)

- **fix(containers/Node): report the real arena child's size after extraction**
  - `Node::take_composed_children` moves the real child into the arena tree and leaves a
    placeholder `Spacer(1)` behind. `Node::layout_height()`/`content_width()` then returned the
    placeholder's dimensions (height 1), clipping every `Node`-wrapped arena child to a single
    row. Now, once extracted, `Node` reports no intrinsic size (mirroring `Container`), so the
    arena layout sizes it from its real tree child.
- **fix(widgets/ContentSwitcher): populate child ids before draining children**
  - `with_child` pushed `None` id placeholders; the ids (often on a wrapping `Node`) were never
    synced, so `current_child_index()` matched nothing and `child_display_for_tree` hid EVERY
    pane (empty ContentSwitcher). `take_composed_children` now fills any unset `child_ids` from
    each child's `style_id()` before draining, so the active pane is shown.

### 2026-06-14 (feat(Select): SelectCurrent child owns the tall border (Python composition))

- **feat(widgets/Select): rearchitect the closed bar into a `SelectCurrent` widget**
  - New `SelectCurrent` widget (`src/widgets/select_current.rs`) owns the closed-state bar and
    its `border: tall` + `padding: 0 2` chrome via CSS (`style_type() == "SelectCurrent"`),
    mirroring Python Textual's composition where the border lives on `SelectCurrent`, not
    `Select`. `Select` builds a configured `SelectCurrent` (`make_current()`) and renders it
    through the styled pipeline, so the framework's border compositor draws a proper 3-row
    tall-bordered box — replacing the previous flat single-line `render_closed`. The bar is
    rendered tagged with the `Select` node id so click-to-open hit-testing still works.
  - `Select::layout_height` now reports the bar's outer height (3 closed; bar + dropdown when
    open); the dropdown overlay is positioned below the full bar height.
  - Focus parity: `SelectCurrent` carries a `-focus` class when the `Select` is focused, with a
    new `SelectCurrent.-focus { border: tall $border; }` default mirroring Python's
    `Select:focus > SelectCurrent`.
- **test(parity): promote `docs_select_widget` to `Status::Pass`**
  - The `select` docs example now matches the Python golden (closed Select renders the tall
    bordered box with the prompt + arrow).

### 2026-06-14 (chore(deps): rich-rs 1.1.1; move Pretty quote fix into the engine)

- **chore(deps): bump `rich-rs` to 1.1.1**
  - Updates the dependency across the crate and all `docs/examples` workspaces. rich-rs 1.1.1
    renders pretty-printed strings in Python `repr` style (single quotes) at the printer level,
    plus a `Progress` `max_refresh` parity fix and `Columns`/`Measurement` improvements.
- **refactor(Pretty): drop the local quote normalizer**
  - `Pretty::debug_str()` (`src/widgets/pretty.rs`) no longer rewrites Rust `Debug` double quotes
    to single quotes — that now happens in the `rich-rs` pretty printer (single source of truth).
    `debug_str()` returns the raw debug output again; the single-quoted rendering is verified by
    the `docs_selection_list_selected` PTY parity case and rich-rs's own tests.

### 2026-06-14 (fix(SelectionList/Pretty): toggle glyph, auto-height chrome, Python-repr quotes)

- **fix(SelectionList): toggle button always renders the `X` glyph**
  - `SelectionList` (`src/widgets/selection_list.rs`) drew `▐ ▌` for unselected and `▐X▌` for
    selected items. Python's `ToggleButton` always renders `BUTTON_INNER = "X"`; selected vs.
    deselected is conveyed only by the button foreground color. Rust now matches (always `▐X▌`,
    color-driven state).
- **fix(layout): `SelectionList`/`Pretty` `layout_height` include border/padding chrome**
  - `extract_child_spec` adds only margin on top of a widget's reported auto height, so
    `layout_height()` must report the OUTER height (content + own border + padding). `SelectionList`
    and `Pretty` returned content-only heights, so an example that added `border`/`padding` (e.g.
    `selection_list_selected.tcss`) clipped its rows / collapsed the panel. Both now resolve the
    cascaded style and add vertical chrome, matching `Input`'s existing behavior and the documented
    contract.
- **fix(Pretty): render strings Python-`repr` style (single quotes)**
  - `Pretty` (`src/widgets/pretty.rs`) fed Rust `Debug` output (double-quoted strings) straight to
    the pretty printer, so it showed `"value"` where Python Textual (via Rich) shows `'value'`.
    `debug_str()` now normalizes double-quoted string literals to single quotes (using CPython's
    quote-selection rule: double quotes only when the string has a `'` and no `"`). The transform
    is a quote-aware scan over the debug output and is idempotent.
- **example(selection_list_selected): app title + on-mount Pretty population**
  - Adds `title() = "SelectionListApp"` and populates the `Pretty` from the real initial selection
    on mount (was hardcoded `"[]"`), via a shared `refresh_pretty` helper.
- **test(parity): promote `docs_selection_list_selected` to `Status::Pass`**
  - The example now matches the Python golden pixel-for-pixel (header title, all `▐X▌` glyphs,
    full-height list + Pretty panels with single-quoted values).

### 2026-06-14 (fix(border): titles/subtitles fill the edge with the border character)

- **fix(border): border title/subtitle now fill the edge with the border glyph**
  - `overlay_border_text()` (`src/widgets/helpers.rs`) previously overwrote the whole inner
    edge with a space-padded title, erasing the border line (`┌Title      ┐`). It now mirrors
    Python's `_border.render_row`: the title is padded with one blank per present corner and
    the remaining edge is filled with the border character (`┌─ Title ─────┐`). Left/right
    alignment reserves one fill glyph on the anchor side; center splits evenly. Fill segments
    keep the border style; only the padded title carries the title style (and BORDER_TITLE_FLIP
    reverse for panel/tab borders).
  - Affects every titled bordered widget (panels, frames, `SelectionList`, etc.).
- **test: convert byte-offset title lookups to cell columns**
  - `render_panel_title_flip` (`tests/border_types_render.rs`) and
    `p2g29_border_title_subtitle_render_on_edges` (`tests/p2_render_css.rs`) used `str::find`
    (a byte offset) as a cell column. That coincidentally worked only while the whole edge was
    one styled segment; with the tight title segment it must be converted via
    `cell_len(&line[..byte])`. Behavior under test is unchanged.

### 2026-06-14 (fix(layout): `align` includes child margins; `Tabs` nav bindings hidden)

- **fix(layout): container `align` now grows alignment bounds by child margins**
  - `apply_parent_align()` (`src/layout/mod.rs`) computed the aligned block extent from each
    child's `layout_rect` (border box, margin-excluded). A child with `margin` + `height: 100%`
    (or `width: 100%`) was therefore shifted by half its own margins — the gap it already
    occupied was double-counted, pushing it off-center by a row/column.
  - Now both the block-axis bounds and the per-child cross-axis centering use the
    margin-grown box, mirroring Python Textual's `WidgetPlacement.get_bounds()`
    (`region.grow(margin)`). A margin-only child that fills its container produces zero
    alignment offset, matching Python.
- **fix(layout): explicit percentage size resolves against container minus margins**
  - `extract_child_spec()` (`src/layout/common.rs`) now resolves an explicit `height`
    (`100%`, `vh`, etc.) against `parent_height - (margin.top + margin.bottom)`, matching
    Python's `_get_box_model` (`styles_width.resolve(container - margin.totals, …)`).
    Margin-free widgets (e.g. five_by_five `GameCell`) are unaffected.
- **fix(widgets/Tabs): nav bindings hidden from the footer**
  - `Tabs::bindings()` (`src/widgets/tabs.rs`) now marks the `left/h previous` and
    `right/l next` bindings `.hidden()`, matching Python's `Binding(..., show=False)`.
    They remain functional; they just no longer leak into the `Footer` hint row.
- **test(parity): promote `docs_tabs` to `Status::Pass`**
  - The `tabs` docs example now matches the Python golden pixel-for-pixel (footer hints +
    centered bordered label). Locked in via the real-PTY parity harness.

### 2026-06-13 (fix(scrollbar): `overflow: scroll` now force-shows the corresponding scrollbar)

- **fix(scrollbar): split `force_visible` into `force_visible_v`/`force_visible_h`**
  - `ScrollbarPolicy::resolve()` (`src/widgets/scrollbar.rs`) and both render paths in
    `ScrollView::render()` (`src/widgets/containers/scroll_view.rs`) previously used a single
    `force_visible` flag that fired only when `scrollbar-visibility: visible`. This caused
    `overflow-y: scroll` / `overflow-x: scroll` to NOT force the corresponding scrollbar visible
    when content was shorter than the viewport.
  - Split into `force_visible_v = ScrollbarVisibility::Visible || Overflow::Scroll on Y` and
    `force_visible_h = ScrollbarVisibility::Visible || Overflow::Scroll on X`. Both the
    iterative `ScrollbarPolicy::resolve()` loop and the tree-mode / non-tree-mode inline
    loops in `ScrollView` now use the independent flags.
  - Effect: widgets with `overflow-y: scroll` (e.g. `RichLog`) now unconditionally show the
    vertical scrollbar, matching Python Textual behavior.
- **test(snapshot): update `keys_preview_layout_snapshot`**
  - `RichLog` has `overflow-y: scroll` in its default CSS; after the scrollbar fix, it now
    correctly shows a vertical scrollbar thumb on the initial layout. Snapshot updated via
    `INSTA_UPDATE=always` to reflect the correct behavior (`▁▁` on row 5).

### 2026-06-13 (SPEC-RA5 Step 2: GameCell containment rewrite)

- **refactor(example/five_by_five): rewrite `GameCell` via Button containment (SPEC-RA5 Step 2)**
  - `GameCell` now owns an `inner: Button` child field (compact, no CSS id) providing focus +
    press behavior. The outer wrapper (`GameCell`) is the CSS-identity node.
  - `take_composed_children` drains the Button into the arena tree on first call (idempotent gate
    via `child_extracted: bool`). Second call returns empty.
  - `style_type_aliases() -> &["Button"]` so both `GameCell { }` and `Button { }` CSS rules match,
    mirroring Python MRO-based selector matching for `GameCell(Button)`.
  - `on_message` intercepts `ButtonPressed` via `msg.downcast_ref::<ButtonPressed>()` (post-RA-1
    form) and calls `ctx.set_handled()` to stop bubble propagation past the wrapper.
  - `focusable() = false` / `can_focus_children() = false`: outer wrapper and Button child are
    excluded from the focus chain — all keyboard logic is at the app level (`on_key_with_app`).
    Mouse-click events still reach the Button via arena hit-testing independently of focus.
    `compact(true)` on the inner Button suppresses tall-border chrome (▔/▁) from default CSS.
  - `is_hovered`/`is_active`/`mouse_interactive` forwarded from inner Button for off-tree CSS
    pseudo-class resolution against the GameCell SelectorMeta node.
  - New unit tests: `game_cell_has_button_child`, `game_cell_style_aliases`,
    `game_cell_not_focusable`. New integration test file `tests/containment_pattern.rs` with
    four tests: `containment_take_composed_children_idempotent`,
    `containment_style_type_aliases_match`, `containment_style_type_aliases_returns_button`,
    `containment_outer_not_focusable`.
  - All PTY parity cases (`five_by_five_initial`, `five_by_five_after_move`,
    `five_by_five_help`) remain at their previous status (Pass/XFail).

### 2026-06-13 (SPEC-RA5 Step 1: deprecate delegation macros)

- **deprecate(delegate): mark `delegate_widget_method!`/`delegate_widget_to!` as migration-period only**
  - Added deprecation notice and removal criteria to the `src/widgets/delegate.rs` module doc.
  - Added `DEFERRED(RA-2)` comments on `pub use` re-exports in `delegate.rs` and the prelude
    in `src/lib.rs`. No behavioral change; no usage sites touched. The canonical delegate method
    count test (`canonical_delegate_method_count_matches_expected`) still passes.
  - New widget-wrapper code should use the containment pattern (SPEC-RA5) instead.

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
