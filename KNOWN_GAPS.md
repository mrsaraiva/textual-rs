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
  full cell-grid + truecolor): **41** honest `#[ignore]`s remain (75 / 75 non-ignored green), grouped
  into the divergence classes below.

> **Why real-app parity:** static render — and even an in-process "liveness" probe — can pass while the
> live app is broken. Two examples this cycle: the tutorial stopwatch rendered perfectly and its
> headless probe was green while its clock was dead in a real terminal; and the `Select` dropdown was
> 2 rows too tall (a keystone chrome double-count) — invisible to the static harnesses because the
> overlay only exists once opened. Only running the real binary against real Python catches these.

## Recently closed (2026-07-09 → 07-10)

Fixed this cycle (were listed as gaps here): **`display`** + **`offset`** styled (styled is now
87/87); **`placeholder`** (content-align block-centering); **markdown tables + scrollbar thumb**;
**`layers`** (per-CSS-layer arrangement + a z-order paint-walk bug); **`switch`** title (Static markup
width); **`horizontal_layout_overflow`** + the underlying **scroll-container CSS identity** architecture
(`ScrollableContainer`/`VerticalScroll`/`HorizontalScroll` now report the right `style_type` + inherit
`width:1fr`); **`select_no_blank_swap`** (recompose now routes through the ctx path); and a **`Select`
dropdown height regression** (keystone chrome double-count). Per-layer flow isolation (`mouse01`'s
Ball-off-screen root) is partly addressed — see the Layout class below.

## Deferred to 1.1 (feature gaps)

- **Inline terminal render mode** (`run(inline=True)`) — no inline render region / alt-screen
  suppression / height clamp. Blocks `how-to/inline01`, `inline02`, `clock`. (2–3 niche demos;
  full-screen mode is complete.)
- **`App.suspend()`** inline-subprocess context manager — needs the inline-mode alt-screen
  teardown/restore. Blocks `guide/app/suspend`.

## Interactive divergence classes (the 41 `pty_interactive` `#[ignore]`s)

Each class is tagged **[1.0-candidate]** (a real gap Fable's breadth makes realistically closeable for
1.0 — leverage-ordered), **[1.x]** (deferred), or **[divergence]** (an intentional/permanent difference
we will not "fix"). Counts are measured glyph/colour cell diffs; the point is the *class*.

- **[1.0-candidate] `Color.a` u8-vs-float alpha blend rounding** *(keystone; HIGHEST leverage)* —
  blended colours round ~1–4/channel off because `Color.a` is a `u8`, not Python's float alpha.
  Collapses a whole class. Tests: `stopwatch04` (182c), `stopwatch03` (201c).
- **[1.0-candidate] Markdown block / vertical spacing (framework `Welcome` widget)** — Rust inserts
  extra blank lines between Markdown blocks, shifting the Welcome body down. Tests: `widgets01`,
  `widgets03`, `widgets04`.
- **[1.0-candidate] BBCode/Rich markup in widget labels not colourised** — `Checkbox`/`RadioButton`
  labels render inline markup literally. Tests: `checkbox` (8c), `radio_set_navigate` (3c).
- **[1.0-candidate] Markdown code-fence syntax highlighting** — fenced code isn't tree-sitter
  highlighted (TextArea is; wire it into Markdown). Tests: `markdown`, `markdown_viewer` (COLOUR only
  now — tables + scrollbar thumb are fixed).
- **[1.0-candidate] Layout follow-ups** — (a) `HorizontalScroll::scroll_visible` over-scrolls +
  `content-align: middle` off-by-one (`actions06`/`actions07`, target page blanks); (b) top/vertical
  margin on stacked mounted widgets (`binding01`) + horizontal margin-collapse (`set_reactive02`);
  (c) per-layer flow — the `ball`-layer widget still lands off-screen (`mouse01`) — partly addressed by
  the `layers` per-layer arrangement fix; needs re-check that layer children lay out from the full
  region.
- **[1.0-candidate] `Input select_on_focus`** *(feature)* + **`LoadingIndicator` on the `loading`
  state** *(feature)* — two small features. Tests: `computed01`, `loading01`.
- **[1.0-candidate] `Pretty` inline-vs-expanded + `MaskedInput -invalid`** — `Pretty` renders a failure
  list expanded where Python is inline (`input_validation`); `MaskedInput` doesn't apply
  `&.-invalid:focus` (`masked_input`).
- **[1.x] `SelectionList` surface/row colours + selected-values `Pretty`** — `selection_list_selections`,
  `selection_list_selected`.
- **[1.x] `RichLog`/`Log` surface + Rich repr-highlight palette** — rich-rs repr palette differs;
  scrollbar-track bg; `Log` bottom-row fill. `rich_log`, `log`, `input_key01/02`, `validate01`.
- **[1.x] ANSI standard-palette → terminal-theme (MONOKAI) render mapping** *(needs a new paint-time
  mechanism)* — `checker01`. (Must NOT be "fixed" by changing named-colour resolution.)
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
