# Known Gaps - textual-rs (as of 1.1.0)

> **Status note (2026-07-16):** 1.0 through 1.1.0 are shipped. 1.0 was **redefined** from
> "every demo passes" to **"hardened core + honest gaps + proven extension story"** (see
> `docs/devel/ROAD_TO_1.0_PIVOT.md`); 1.1.0 delivered the extension story and deep structural
> parity (cross-screen access, component classes, Tree/OptionList key identity, the TextArea
> document subsystem, the keymap subsystem, typed action/validation/reactive foundations,
> fine-grained messages). Demo parity is the **verification floor**, not the release gate; the
> demo tail ships across 1.x. This file lists the *measured* gaps against the **real-app**
> harness, not the retired headless estimate.

Parity is measured against real Python by three harnesses:
- **Styled per-cell-RGB harness** (`tests/visual_parity.rs`): **87 / 87** exact.
- **Plain-text PTY harness** (`tests/pty_parity.rs`): **186 / 186**.
- **Real-app interactive parity** (`tests/pty_interactive.rs`, real Rust vs real Python, PTY+vt100
  full cell-grid + truecolor): **3** honest `#[ignore]`s remain (108 / 108 non-ignored green), grouped
  into the divergence classes below — and **all 3 are intentional divergences or the 1.1 inline
  feature. Zero open bugs.** (`rich_log` closed with rich-rs 1.2.2; `actions05` reconciled to Python —
  the Rust port had spuriously composed a Footer Python's demo omits.)

> **Why real-app parity:** static render — and even an in-process "liveness" probe — can pass while the
> live app is broken. Two examples this cycle: the tutorial stopwatch rendered perfectly and its
> headless probe was green while its clock was dead in a real terminal; and the `Select` dropdown was
> 2 rows too tall (a keystone chrome double-count) — invisible to the static harnesses because the
> overlay only exists once opened. Only running the real binary against real Python catches these.

## Recently closed (2026-07-09 → 07-11)

Earlier this cycle: **`display`** + **`offset`** styled (styled is now 87/87); **`placeholder`**
(content-align block-centering); **markdown tables + scrollbar thumb**; **`layers`** (per-CSS-layer
arrangement + a z-order paint-walk bug); **`switch`** title (Static markup width);
**`horizontal_layout_overflow`** + the underlying **scroll-container CSS identity** architecture
(`ScrollableContainer`/`VerticalScroll`/`HorizontalScroll` now report the right `style_type` + inherit
`width:1fr`); **`select_no_blank_swap`** (recompose ctx path); a **`Select` dropdown height regression**
(keystone chrome double-count); **`stopwatch03`** (`Digits` translucent-fg composition; 19 hardcoded
shade RGBs removed); **`checkbox`** + **`radio_set_navigate`** (BBCode markup in toggle labels via a
shared `toggle_label_content` path); and **`widgets01`/`widgets03`/`widgets04`** (framework `Welcome`
now composes `Static(rich.markdown.Markdown)` + a new global ANSI→truecolor paint filter).

Closed in the 1.0-candidate batch (2026-07-11, three parallel Fable investigations + integration):
- **Layout** (`actions06`/`actions07`/`binding01`/`set_reactive02`): scrolled-child clip used the
  unscrolled origin (viewport blanked at any scroll offset); `scroll_visible` double-counted the
  current offset; `scrollbar-size: 0` was clamped up to 1 (Python honours 0 to hide the bar); a
  ProgressBar-internal `Bar` default-CSS rule leaked onto user widgets typed `Bar` (now scoped);
  intrinsic auto width/height summed adjacent child margins instead of collapsing them.
- **Features** (`computed01`/`loading01`): `Input select_on_focus` (focus-gain selects the whole
  value); `loading` reactive now paints a `LoadingIndicator` **cover** (Python `_cover_widget`), plus
  the core fix that the **live loop now ticks arena-extracted widgets** (tick-driven animations were
  frozen live while animating headless).
- **Widgets** (`masked_input`/`input_validation`): `Input` & `MaskedInput` `-valid`/`-invalid` state
  now syncs onto the live arena node via `ctx.set_class` (was only mutating pre-mount `seed.classes`,
  so `&.-invalid:focus` never applied); shared `input_chrome.rs` colour helpers; `Pretty.update()`
  relayouts (was clipping the inline repr to `[` at its stale rect).
- **Test-hygiene**: `render_compose` (poll-until-painted, was a cold-start timing flake) and
  `input_types_typing` + `input_validation` (switched to `assert_glyph_only_parity` — the caret
  reverse-cursor cell blinks non-deterministically in a live PTY; glyph/layout is exact).

Closed in the 1.0-candidate **second wave** (2026-07-11, three more parallel Fable investigations +
integration — 27 → 7 ignores):
- **Focus fundamentals** (`stopwatch04`, `events_prevent_clear`, + broad click blast radius):
  focus-on-mouse-down was entirely missing (Python `Screen._forward_event` focuses the nearest
  focusable under the pointer before forwarding), and focus now transfers off a widget the frame hides
  (`Widget._on_hide` → `Screen._reset_focus`).
- **`set_styles`/`with_widget_mut` relayout** (`mouse01`): runtime style mutations that change intrinsic
  size now relayout (Python `refresh(layout=True)`); closes the mouse-following Ball.
- **Markdown code-fence highlighting**: syntect → app-theme-token mapping (new `src/highlight.rs`,
  Python `highlight.py` pygments parity) instead of rich-rs' foreign monokai scheme.
- **Theme-token alpha hex-quantization** + **user-CSS-over-DEFAULT_CSS cascade layer** (two wide colour/CSS
  roots): closed ContentSwitcher, SelectionList ×2, and (as demo misports) checker01/02/04.
- **Global dim pre-blend** (`FrameBuffer::preblend_dim`, Python `ANSIToTruecolor`): `question_title01`
  header sub-title, and every other dim emitter.
- **Footer multi-key display**, **Switch** knob-animation + `-on` sync + `#custom-design` component
  styles, **RichLog/Log** scrollbar lanes + width + repr highlighter (`log`, `validate01`,
  `input_key01/02/03`), and `compound_byte01/02` (test de-race).
- **Test-hygiene**: `input_typing` → `assert_glyph_only_parity` (blink-caret class).

Closed in the 1.0-candidate **third wave** (2026-07-12, the deep/upstream trio — 7 → 5 ignores):
- **`custom01`** (`FROZEN_ANCESTOR_BG`): a node's OWN translucent-bg content now freezes at bake time
  against the ancestor surface captured on the widget's style cache key (Python `visual_style` cache),
  while borders/padding (`background_colors`) stay live — mirroring Python's border-vs-content split.
  Divergence-gated (byte-identical in steady state).
- **`radio_set_changed`**: RadioSet keyboard nav now rides Python's declarative `BINDINGS` (an ancestor
  `VerticalScroll`'s `down→scroll_down` binding was stealing the arrows before the raw handler ran); and
  `with_widget_mut` now probes `auto_content_width/height` so a `Static.update()` that grows an
  auto-width label relayouts (the content-update path — see the follow-up below, now closed).
- **`rich_log`** (26 cells, the last real bug) closed by **rich-rs 1.2.2**: the `Syntax` fenced-code
  token palette (annotation colon / docstring quotes / for-in `in` / comment delimiters) is aligned to
  Pygments and the indent-guide colour is theme-derived with Python's `DIM_FACTOR` dim pre-blend
  (`monokai.tmTheme` + `syntax.rs`), published to crates.io and bumped here.

## Deferred beyond 1.1 (feature gaps)

1.1.0 shipped without these (they were not in its scope); they remain deferred to a future
release:

- **Inline terminal render mode** (`run(inline=True)`) - no inline render region / alt-screen
  suppression / height clamp. Blocks `how-to/inline01`, `inline02`, `clock`. (2-3 niche demos;
  full-screen mode is complete.)
- **`App.suspend()`** inline-subprocess context manager - needs the inline-mode alt-screen
  teardown/restore. Blocks `guide/app/suspend`.

## 1.1.x follow-ups (small, tracked)

Non-blocking items noted during the 1.1.0 release:

- **DataTable `--header-cursor` / `--fixed-cursor` component classes** are declared but not yet
  consumed by `render` (the pre-migration composition reused the base cursor/fixed styles);
  consuming them shifts pixels (e.g. header cursor `$primary` -> `$accent-darken-1`), a
  Python-parity behavior change deferred so it lands deliberately.
- **`textual-macros` publishing:** the release workflow's OIDC token is not valid for the
  `textual-macros` crate (trusted publishing is configured for `textual` only), so a macros
  version bump must be published manually until trusted publishing is set up for `textual-macros`
  on crates.io.
- **CI `visual_parity`:** the styled per-cell harness needs the Python reference repo (`../textual`),
  absent on CI, so it runs only locally; a `TEXTUAL_PY_REF` harness override would let CI check out
  the reference and run it. `pty_parity` (committed goldens) is already a blocking CI job.

## Interactive divergence classes (the 3 `pty_interactive` `#[ignore]`s)

**All three are intentional divergences or the deferred 1.1 inline feature — no open bugs remain.**

- **[divergence] Python-only startup crash** (`set_reactive01`) — the Python ref intentionally raises
  (pre-mount `query_one` → `NoMatches`, the doc's "wrong way"); reproducing a Rich traceback
  glyph-for-glyph isn't meaningful.
- **[divergence] Inline render mode** (`howto_inline01/02`) — see *Deferred to 1.1*.

## Cosmetic / minor (broader demo tail)

- **`byte03` message `prevent()` across a reactive feedback loop** — CLOSED: the prevent stack is now an
  ambient thread-local (was per-`EventCtx`, dying each dispatch), each `MessageEvent` carries a prevent
  snapshot stamped at post time, and dispatch re-activates it (transitive), matching Python's ContextVar
  semantics. `ReactiveCtx` gained a prevent-checked `post_message`; byte03's guard bool is deleted for the
  real `ctx.prevent` scope. (Also fixed a masking gap: `Switch.watch_value` now emits `SwitchChanged` on
  programmatic sets.)
- Benign substitutions where output is visually identical: `digits`/`clock` type-vs-`#id` selector
  (single-widget apps), occasional emoji-literal vs shortcode.

## Tracked correctness follow-ups (no demo impact)

- **`layers` nested-declaration semantics are nearest-wins (intentional divergence, decided
  2026-07-16).** When nested containers BOTH declare `layers`, Rust orders each container's children
  by the NEAREST declaration (the container's own resolved `layers`). Python `Widget.layers`
  (widget.py:2613-2626) lets the ROOT-most ancestor declaration win instead, which looks like an
  accident of loop direction rather than a contract; nearest-wins matches the documented mental model
  ("declare layers on a container, assign its children") and what the per-container arrangement
  already implements. Pinned by `runtime::render::tests::nested_layers_declarations_are_nearest_wins`.
  If a future parity capture diverges on a demo where nested containers both declare `layers`, this is
  the first suspect; revisit only if a real Python demo depends on outermost-wins. Related hardening
  (same pass): the system screen layers (`_loading`, `_toastrack`, `_tooltips`) are now appended
  programmatically after the CSS-derived list (`src/runtime/layers.rs`, mirroring Python
  `Screen.layers`), so a user `Screen { layers: ... }` can no longer clobber toast z-order; and the
  default/unknown layer bucket now maps to the FIRST declared layer's index with DOM-order tiebreak
  (Python `_compositor.py` `layers_to_index.get(layer, 0)`) instead of a strictly-lower bucket.

- **Runtime unit tests need a real TTY (1.x test-harness cleanup).** ~130 `runtime::event_loop::tests`
  call `app.initialize()`, which brings up the real terminal driver and fails on headless CI with
  `Terminal(Os { WouldBlock })` (`src/runtime/event_loop.rs:8738`). CI works around this by running the
  suite under a PTY (`script -qefc 'cargo test -- --test-threads=1' /dev/null`) so the driver
  initializes; single-threaded also avoids TTY contention. As of 1.1.0 the CI `test` job is BLOCKING
  and runs `--lib` plus the ~102 headless integration bins under this PTY wrapper (excluding the
  parity harnesses and `click_actions_pty`, which spawn a real PTY / need the Python reference). The
  real fix is still a headless/mock driver for unit tests so they run without a TTY, after which the
  PTY wrapper could be dropped. (Diagnosed at 1.0 release; CI made blocking at 1.1.0 release.) Additionally, 3 `runtime::render::tests::{modal_screen_layer_preserves_underlay_text,
  modal_screen_layer_tints_underlay_colors, screen_stylesheet_does_not_leak_to_underlay_layer}` are
  `#[ignore]`d: their translucent-modal-over-underlay assertion needs the truecolor profile the real
  driver negotiates, which a headless PTY can't answer (it degrades to opaque). Un-ignore once the test
  helper forces a fixed color profile.

- **Widget-initiated layout invalidation** — CLOSED across waves 2–3: `set_styles` diff-detects
  layout-affecting mutations and relayouts, and `with_widget_mut` compares intrinsic size around the
  closure — now including `auto_content_width/height` — and promotes an absorbed invalidation to a
  forced relayout (Python `refresh(layout=True)`). Both the style-mutation and the content-update
  (`Static.update`) paths are covered.
- **`OptionList`/`SelectionList` keyboard nav** — CLOSED: both now declare Python's `BINDINGS` and
  execute via widget actions (raw `on_event` key arms removed), so a list nested in a scroll container
  wins the arrows via the focused→root chain. `SelectOverlay`/`PaletteCommandList` inherit the bindings
  through the widget-macro base delegation.
- **`loading`/`disabled` → focus & hit-test** — Python's `is_disabled = disabled or loading` removes a
  covered/loading widget from the focus chain and interaction; Rust's focus chain and hit-test don't
  consult `state.loading` yet (the `loading` cover now paints, but the widget under it is still
  focusable/clickable). Wiring it touches focus semantics broadly.
- **`get_loading_widget()` customization hook** — Python lets a Screen/App override the cover widget;
  Rust always uses the default `LoadingIndicator`.
- **Live arena tick breadth** — the live loop now ticks every `is_active()` arena widget/cover (the fix
  that unfroze live animations). This is intended Python parity and matches headless, but it's the
  widest live-behavioral change of the batch — note it as a first suspect if a future live/pty golden
  drifts on an animated widget.
- **Per-screen toast racks** — CLOSED: each pushed/modal screen tree mounts its own system `ToastRack`
  (id `textual-toastrack`), `App::notify` routes to the ACTIVE screen's rack, and notifications re-sync on
  every screen-stack transition. (The old single AppRoot rack didn't merely occlude — a toast over a
  modal rendered *nowhere*, a cross-tree NodeId miss.) Remaining sub-follow-ups: (a) a toast crossing
  screens gets a fresh countdown (needs a creation-instant on `AppNotification`); (b) the underlying
  cross-tree widget-timer hazard (a suspended tree's node timers fire against the active tree's slotmap);
  (c) a screen-layer `Tooltip` escape (Tooltip still rides `overlay: screen`). The system-layer
  ORDERING half of (c) is CLOSED (2026-07-16): `_loading`/`_toastrack`/`_tooltips` are appended
  programmatically to every walk root's layer list (`src/runtime/layers.rs`), no base-CSS declaration
  needed.
- **Theme LAB shade tokens** — base/semantic tokens are byte-exact; the LAB-derived shade family
  (`$*-lighten-2/3`, `$*-darken-3`) can diverge up to ~42/channel (pre-existing `rgb_to_lab`
  precision). No demo uses the divergent shades.
- **`styles/layout` 2-row vertical drift** — a pre-existing auto-height/margin gap (flagged during the
  scroll-container sweep; byte-identical pre/post that change, so not a regression).
- **`Constrained` min-only / max-only + chrome-bearing child under-reports chrome** — routing
  `Constrained`'s min/max into the node CSS layout so the flow applies them atop the keystone recursion.
  No in-tree usage hits it. See the arm comment in `src/widgets/containers/constrained.rs`.
- `scroll_view.rs` retains a parallel widget-local scrollbar path (the canonical arena render path is
  correct); the Pilot headless key-cascade re-implements the live key arm (converge to prevent drift);
  `intrinsic_wrapped_height` under-counts multiple trailing newlines vs Python `split(allow_blank=True)`;
  `MarkdownViewer` doesn't match `VerticalScroll` selectors (Python would).

These are scoped, isolated, and tracked — none block the core framework or real-application porting.

## Intentional divergences (Rust-by-design, will not match Python)

These are deliberate consequences of Rust's type system and idioms. A parity capture or a
Python porter hitting one of these should recognize it as by-design, not a bug to fix. They
are NOT on the 1.1 (or any) roadmap.

- **Handler dispatch by type, not name-convention.** Python dispatches via string handler
  names (`on_<namespace>_<name>`, `key_<name>`) resolved at runtime. Rust uses a single typed
  `Widget::on_message` plus `#[on(Type)]` flat-TypeId dispatch: no `on_button_pressed`-style
  method-name convention and no `key_x` method dispatch (movement keys ride the `ActionMap` /
  bindings). This is the core transport-model choice.
- **No message-class MRO inheritance.** Python message subclasses are received by base-type
  handlers. Rust messages are flat TypeId types; `#[on(Base)]` does not receive `Derived`.
  Compose or match explicitly.
- **No BINDINGS class-hierarchy inheritance / `inherit_bindings`.** Python merges `BINDINGS`
  up the class hierarchy. Rust's `Widget::bindings()` is a plain trait method fully resolved
  by the time it returns; a composition wrapper that wants a parent's bindings delegates
  explicitly. (The 1.1 keymap subsystem is USER-side key remapping, a separate concern layered
  over this, not binding inheritance.)
- **Compile-time reactive derive; no private `_watch`/`_validate`/`_compute` or reactive
  subclass inheritance.** Python resolves reactive behavior via runtime descriptors plus
  subclassing. Rust uses `#[derive(Reactive)]` with typed watch/validate/compute wired at
  compile time; there is no reactive inheritance across widget types and no private
  method-name convention.
- **Deferred-phase watcher dispatch.** Python fires reactive watchers synchronously inside the
  setter. Rust defers watcher dispatch to a reactive phase after the mutating call returns, a
  documented deliberate divergence that keeps the dispatch live-borrow invariant intact.
- **Theme tokens follow the modern Textual palette, not 0.58.** Base and semantic tokens are
  byte-exact against current Python; textual-rs does not reproduce the older 0.58 palette some
  third-party apps were written against. Faithful ports of such apps hardcode their era's hex
  or ship a custom theme.
