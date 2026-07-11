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
  full cell-grid + truecolor): **27** honest `#[ignore]`s remain (84 / 84 non-ignored green), grouped
  into the divergence classes below.

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

Per-layer flow isolation (`mouse01`'s Ball-off-screen root) is partly addressed — see the Layout class
below.

## Deferred to 1.1 (feature gaps)

- **Inline terminal render mode** (`run(inline=True)`) — no inline render region / alt-screen
  suppression / height clamp. Blocks `how-to/inline01`, `inline02`, `clock`. (2–3 niche demos;
  full-screen mode is complete.)
- **`App.suspend()`** inline-subprocess context manager — needs the inline-mode alt-screen
  teardown/restore. Blocks `guide/app/suspend`.

## Interactive divergence classes (the 27 `pty_interactive` `#[ignore]`s)

Each class is tagged **[1.0-candidate]** (a real gap Fable's breadth makes realistically closeable for
1.0 — leverage-ordered), **[1.x]** (deferred), or **[divergence]** (an intentional/permanent difference
we will not "fix"). Counts are measured glyph/colour cell diffs; the point is the *class*.

- **[1.0-candidate] `#stop` button surface not tinted after `#start` hides (`stopwatch04`, 182c)** —
  Python moves focus to `#stop` when `#start` goes `display:none`, so `#stop` gets
  `Button:focus background-tint: $foreground 5%` (surface `#ba4461`); Rust renders it untinted
  (`#b93c5b`) — focus doesn't transfer on hide (or the `:focus` tint isn't applied post-reveal). NOT a
  colour-math gap. *(The old "Color.a u8-vs-float / LAB rounding" framing here was wrong: `Color.a` is
  already `f32`, and rgb_to_lab/lab_to_rgb are bit-exact to Python (196k-case sweep). `stopwatch03` is
  FIXED — it was `Digits` pre-flattening its fg over `$background` instead of the composited surface;
  the 19 hardcoded-shade workarounds were removed as the LAB math was always correct.)*
- **[1.0-candidate] Markdown code-fence syntax highlighting** — fenced code isn't tree-sitter
  highlighted (TextArea is; wire it into Markdown). Tests: `markdown`, `markdown_viewer` (COLOUR only
  now — tables + scrollbar thumb are fixed).
- **[1.0-candidate] Layout follow-up — per-layer flow** — the `ball`-layer widget still lands
  off-screen (`mouse01`). Partly addressed by the `layers` per-layer arrangement fix; needs a re-check
  that layer children lay out from the full region. (The `actions06`/`actions07`, `binding01`,
  `set_reactive02` members of this class are now closed — see *Recently closed*.)
- **[1.x] `SelectionList` surface/row colours + selected-values `Pretty`** — `selection_list_selections`,
  `selection_list_selected`.
- **[1.x] `RichLog`/`Log` surface + Rich repr-highlight palette** — rich-rs repr palette differs;
  scrollbar-track bg; `Log` bottom-row fill. `rich_log`, `log`, `input_key01/02`, `validate01`.
- **[1.x] ANSI standard-palette → terminal-theme (MONOKAI) render mapping** — `checker01`. A global
  `apply_ansi_truecolor_to_segments` paint filter now maps rich ANSI-indexed colours (the Markdown /
  BBCode path) to the theme, which is what closed `widgets01/03/04`. It does **not** reach `checker01`
  (re-verified 2026-07-10, still 1920 colour cells): the board's `on white`/`on black` arrive as
  named-colour surfaces via a different emitter, not ANSI-indexed segments. Closing it needs that
  emitter routed through the same filter — and must NOT be "fixed" by changing named-colour resolution.
- **[1.x] `ScrollView` right-edge scrollbar colour/extent** — `checker04`, `checker02`.
- **[1.x] Widget-local state / relayout grab-bag** — `Input` border not repainted on blur to a Button
  (`prevent`); empty auto-width `Label.update()` in a Horizontal doesn't relayout (`radio_set_changed`);
  Footer shows all keys of a multi-key binding (`counter02`); Header sub-title not dimmed
  (`question_title01`); `ContentSwitcher` surface colours (`content_switcher`); `KeyLogger` Debug-vs-repr
  (`input_key03`); `Switch` knob position + `#custom-design` colours after toggle (`switch`); compound
  `Input` placeholder/value where Python is blank (`byte01`/`byte02`); `ColorButton` click doesn't
  animate Screen bg (`custom01`); focused-Input past-end reverse-cursor blink-phase flake
  (`input_typing`, ≤1c, kept ignored).
- **[divergence] Python-only startup crash** (`set_reactive01`) — the Python ref intentionally raises
  (pre-mount `query_one` → `NoMatches`, the doc's "wrong way"); reproducing a Rich traceback
  glyph-for-glyph isn't meaningful.
- **[divergence] Inline render mode** (`howto_inline01/02`) — see *Deferred to 1.1*.

## Cosmetic / minor (broader demo tail)

- **`byte03` message `prevent()` across a reactive feedback loop** — `prevent(MessageType)` works for
  single-dispatch; spanning the `Reactive` update→re-dispatch cycle needs threading the prevent scope
  through `Handle::update`/`reactive.rs` (behavior-equivalent guard for now).
- Benign substitutions where output is visually identical: `digits`/`clock` type-vs-`#id` selector
  (single-widget apps), occasional emoji-literal vs shortcode.

## Tracked correctness follow-ups (no demo impact)

- **Widget-initiated layout invalidation** — Rust has no `Static.update(layout=True)` parity: a plain
  setter (`Static::set_text`, `Pretty` string update) can't signal "my intrinsic size changed", and
  `App::with_widget_mut` creates a local `PendingInvalidation` and drops it. Two demos this batch
  (`set_reactive02`, `input_validation`) had to call `ctx.request_layout()` in their watchers to force
  the relayout Python does implicitly. A `.update()`-parity mechanism would remove that demo-side
  responsibility. *(Surfaced by the layout + widgets Fable passes, 2026-07-11.)*
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
