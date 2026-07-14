# Known Gaps — textual-rs (pre-1.0)

> **Status note (2026-07-02):** 1.0 has been **redefined** from "every demo passes" to
> **"hardened core + honest gaps + proven extension story"** (see
> `docs/devel/ROAD_TO_1.0_PIVOT.md`). Demo parity is the **verification floor**, not the release
> gate; the demo tail ships across 1.x. This file lists the *measured* gaps against the **real-app**
> harness, not the retired headless estimate.

Parity is measured against real Python by three harnesses:
- **Styled per-cell-RGB harness** (`tests/visual_parity.rs`): **87 / 87** exact.
- **Plain-text PTY harness** (`tests/pty_parity.rs`): **186 / 186**.
- **Real-app interactive parity** (`tests/pty_interactive.rs`, real Rust vs real Python, PTY+vt100
  full cell-grid + truecolor): **4** honest `#[ignore]`s remain (107 / 107 non-ignored green), grouped
  into the divergence classes below — and **all 4 are intentional divergences or the 1.1 inline
  feature. Zero open bugs.** (The last real gap, `rich_log`, closed with rich-rs 1.2.2.)

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

## Deferred to 1.1 (feature gaps)

- **Inline terminal render mode** (`run(inline=True)`) — no inline render region / alt-screen
  suppression / height clamp. Blocks `how-to/inline01`, `inline02`, `clock`. (2–3 niche demos;
  full-screen mode is complete.)
- **`App.suspend()`** inline-subprocess context manager — needs the inline-mode alt-screen
  teardown/restore. Blocks `guide/app/suspend`.

## Interactive divergence classes (the 4 `pty_interactive` `#[ignore]`s)

**All four are intentional divergences or the deferred 1.1 inline feature — no open bugs remain.**

- **[divergence] Python-only startup crash** (`set_reactive01`) — the Python ref intentionally raises
  (pre-mount `query_one` → `NoMatches`, the doc's "wrong way"); reproducing a Rich traceback
  glyph-for-glyph isn't meaningful.
- **[divergence] Rust demo composes an extra Footer** (`actions05`) — the Rust `actions` port mounts a
  Footer the Python doc omits, shifting content one row. A demo-authoring difference (reconcilable by
  dropping the Footer), not a framework gap.
- **[divergence] Inline render mode** (`howto_inline01/02`) — see *Deferred to 1.1*.

## Cosmetic / minor (broader demo tail)

- **`byte03` message `prevent()` across a reactive feedback loop** — `prevent(MessageType)` works for
  single-dispatch; spanning the `Reactive` update→re-dispatch cycle needs threading the prevent scope
  through `Handle::update`/`reactive.rs` (behavior-equivalent guard for now).
- Benign substitutions where output is visually identical: `digits`/`clock` type-vs-`#id` selector
  (single-widget apps), occasional emoji-literal vs shortcode.

## Tracked correctness follow-ups (no demo impact)

- **Widget-initiated layout invalidation** — CLOSED across waves 2–3: `set_styles` diff-detects
  layout-affecting mutations and relayouts, and `with_widget_mut` compares intrinsic size around the
  closure — now including `auto_content_width/height` — and promotes an absorbed invalidation to a
  forced relayout (Python `refresh(layout=True)`). Both the style-mutation and the content-update
  (`Static.update`) paths are covered.
- **`OptionList`/`SelectionList` keyboard nav uses raw `on_event`, not declarative BINDINGS** — same bug
  class `RadioSet` had (fixed in wave 3): they handle Up/Down/PageUp/PageDown in `on_event`
  (`src/widgets/option_list.rs`) with no `BINDINGS`, so an ancestor scroll container's binding can steal
  the arrows before the raw handler runs. Give them Python's declarative `BINDINGS`. *(No demo currently
  reproduces it — the RadioSet case did because it was nested in a `VerticalScroll`.)*
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
- **Per-screen toast racks** — a toast posted while a modal/pushed screen is active shows on the base
  rack (behind the screen). Python mounts a `ToastRack` on every screen. Degrades gracefully (guarded).
  The fuller `Screen.layers` set (`_tooltips`, `_loading`, …) is a related follow-up.
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
