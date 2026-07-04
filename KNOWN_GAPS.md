# Known Gaps — textual-rs (pre-1.0)

> **Status note (2026-07-02):** 1.0 has been **redefined** from "every demo passes" to
> **"hardened core + honest gaps + proven extension story"** (see
> `docs/devel/ROAD_TO_1.0_PIVOT.md`). Demo parity is the **verification floor**, not the release
> gate; the demo tail ships across 1.x. This file lists the *measured* gaps against the **real-app**
> harness, not the retired headless estimate.

Parity is measured against real Python by the real-app PTY harness (real Rust binary vs real Python
app, both in PTY+vt100, full cell-grid + truecolor compare) plus the styled/plain harnesses:
- **Styled per-cell-RGB harness** (`tests/visual_parity.rs`): **85 / 87** exact.
- **Plain-text PTY harness** (`tests/pty_parity.rs`): **186 / 186**.
- **Real-app interactive parity** (`tests/pty_interactive.rs`, real Rust vs real Python): after the
  **1.0 parity sweep** (2026-07-03) the ignore set is **42** (down from 44); the remaining ignores are
  honest, grouped into the divergence classes below. *(The earlier "139/141 LIVE" figure came from an
  in-process headless harness that diverged from the live loop and ignored color — it was inflated and is
  retired. Manual PTY spot-checks confirmed the real-app harness is truth.)*

> **Why real-app parity:** static render — and even an in-process "liveness" probe — can pass while the
> live app is broken (the tutorial stopwatch rendered perfectly and its headless probe was green, but its
> clock was dead in a real terminal). Only running the real binary against real Python catches this.

## Deferred to 1.1 (feature gaps)

- **Inline terminal render mode** (`run(inline=True)`) — no inline render region / alt-screen suppression / height clamp yet. Blocks `how-to/inline01`, `inline02`. (Low-leverage: 2 niche demos; full-screen mode is complete.)
- **`App.suspend()`** inline-subprocess context manager — suspend the TUI, run an external program inline (e.g. `$EDITOR`), resume. Needs the inline-mode alt-screen teardown/restore. Blocks `guide/app/suspend`.

## Styled parity (2 / 87 PENDING)

- **`display`** — a descendant-selected leaf with `display:none` + chrome (`#q .button { border; padding; margin }`) collapses because `layout_height()` is resolved context-free (can't match the descendant rule). The real fix unwinds the "`layout_height()` includes chrome" convention across the flow/grid layout modules + ~6 chrome-baking widgets — a large refactor scheduled for 1.1.
- **`offset`** — placement itself is now exact (the `u16→i32` signed-Rect refactor landed in 1.0; negative offsets clip correctly). The residual is **content-align block-vs-per-line for wrapped text**: under `align: center middle`, Python centers a wrapped multi-line block as a unit and left-aligns each line within it; Rust currently centers each wrapped line independently. Orthogonal to placement; lives in the `Content`/text render pipeline.

## Cosmetic / minor (broader demo tail)

- **Markdown code-fence syntax highlighting** (`markdown`, `markdown_viewer`) — fenced code blocks aren't tree-sitter-highlighted (TextArea is; Markdown isn't yet) and `code_indent_guides=False` isn't applied. Plain-text structure + tables are faithful.
- **`byte03` message `prevent()`** across a reactive feedback loop — `prevent(MessageType)` works for single-dispatch; spanning the `Reactive` update→re-dispatch cycle needs threading the prevent scope through `Handle::update`/`reactive.rs` (reproduced with a behavior-equivalent guard for now).
- Benign substitutions where output is visually identical: `digits`/`clock` use a type selector where Python uses an `#id` (single-widget apps), occasional emoji-literal vs shortcode.

## Interactive parity divergence classes (the 42 `pty_interactive` `#[ignore]`s)

The 1.0 sweep (2026-07-03) re-ran every ignored real-app case and grouped the survivors into classes.
Each class is tagged **[1.x]** (a real gap we intend to close in 1.x) or **[divergence]** (an
intentional/permanent difference we will not "fix"). Counts are the measured glyph/colour cell diffs on
the current tree; the point is the *class*, not the cell.

- **[1.x] `Color.a` u8-vs-float alpha blend rounding** *(keystone refactor)* — blended colours round
  ~1–4/channel off because `Color.a` is a `u8`, not Python's float alpha. Tests: `stopwatch04` (182c
  revealed `#stop` button surface), `stopwatch03` (201c button-border grey shade #8d8d8d vs #919191).
- **[1.x] Markdown block / vertical spacing (the framework `Welcome` widget)** — Rust inserts extra
  blank lines between Markdown blocks, shifting the Welcome body down. Tests: `widgets01` (669g),
  `widgets03` (673g), `widgets04` (669g). Shared root with the `widgets02` catch case.
- **[1.x] `SelectionList` surface/row colours + selected-values `Pretty`** — the focused list body and
  the `Pretty` panel diverge. Tests: `selection_list_selections` (2103c), `selection_list_selected`
  (153g).
- **[1.x] `RichLog`/`Log` surface + Rich repr-highlight palette** — three sub-roots: (a) the rich-rs repr
  highlighter palette differs from Python Rich (`#f4005f/#fd971f/#98e024` vs `#b73763/#f5a623/#98d168`);
  (b) `RichLog` scrollbar-track bg (#000000 vs widget surface); (c) `Log` bottom-row vertical-fill extent
  (row 29 py #000000 vs rust screen-default #121212 — the Log *body* already matches). Tests: `rich_log`
  (1147c), `log` (120c/1 row), `input_key01` (148c), `input_key02` (104c), `validate01` (57c).
- **[1.x] ANSI standard-palette → terminal-theme (MONOKAI) render mapping** *(needs a new mechanism)* —
  Rich `Style.parse("on white"/"on black")` yields ANSI standard colours (7/0) that Textual renders
  through the app's ANSI theme (MONOKAI: white→#c4c5b5, black→#1a1a1a). textual-rs has no
  standard-colour→theme mapping at paint, so it renders CSS white/black. Test: `checker01` (1920c).
  (Must NOT be "fixed" by changing named-colour resolution — that would break CSS-white parity.)
- **[1.x] `ScrollView` right-edge scrollbar colour/extent** — Tests: `checker04` (175c right-edge band),
  `checker02` (2 stray bottom-right corner cells past the 64-col board).
- **[1.x] `Pretty` inline-vs-expanded + `MaskedInput` `-invalid` state** — `Pretty` renders a failure
  list multi-line-expanded where Python renders it inline (`input_validation`, 43g); `MaskedInput`
  doesn't apply `&.-invalid:focus { border: tall $error }` on a partial card number (`masked_input`,
  245c border-fg).
- **[1.x] BBCode/Rich markup in widget labels not colourised** — `Checkbox`/`RadioButton` labels render
  inline markup literally / uncoloured. Tests: `checkbox` (8c: `[magenta]Ginaz` label fg + `.-on`
  checked-mark), `radio_set_navigate` (3c: `[bold italic red]The`).
- **[1.x] Layout gaps** — (a) `HorizontalScroll::scroll_visible` over-scrolls + `content-align: middle`
  off-by-one (`actions06`/`actions07`, 5g each, target page goes blank); (b) top/vertical margin on
  stacked mounted widgets not honoured (`binding01`, 24g) + horizontal margin-collapse
  (`set_reactive02`, 5g); (c) per-layer flow isolation — layer children should lay out from the full
  region, so the `ball`-layer widget lands off-screen (`mouse01`, Ball missing); (d) `height: 100%`
  vertical-extend fills from the live vs cached `visual_style` + a Rust-only Footer in the demo
  (`actions05`, 80g).
- **[1.x] Widget-local state / relayout follow-ups** — a grab-bag of single-root residuals:
  `Input` border not repainted on blur when focus moves to a Button (`prevent`, 408c); `Label.update()`
  on an empty auto-width Label inside a Horizontal doesn't relayout (`radio_set_changed`, 43g); Footer
  shows *all* keys of a multi-key binding (`up,k`) where Python shows only the first ↑ (`counter02`,
  24g); Header sub-title not dimmed — needs Rich `dim` on the subtitle+separator, mirroring
  `App.format_title` (`question_title01`, 25c); `ContentSwitcher` button/markdown surface colours
  (`content_switcher`, 37c); `KeyLogger` event formatting is Rust `Debug` vs Python repr (`input_key03`,
  34g); `Switch` slider-knob position + `#custom-design` custom colours after toggle (`switch`, 6c);
  compound `Input` shows a placeholder/value where Python is blank (`byte01` 4g, `byte02` 3g);
  `ColorButton` click doesn't set/animate the Screen bg in the live loop (`custom01`, 3600c); the Select
  `s` swap via `with_query_one_mut_as` drops its recompose request (ctx-less mutation path,
  `select_no_blank_swap`, 27g); focused-Input past-end reverse-cursor cell is blink-phase-dependent
  (`input_typing`, ≤1c, flaky — kept ignored rather than flip a 9/10).
- **[1.x] `Input` select-on-focus not implemented** *(feature)* — Python `Input("0", …)` +
  `select_on_focus=True` replaces the selected "0" on type; Rust has no `select_on_focus`, so pre-filling
  "0" and typing "123" yields "0123". Test: `computed01` (29g). `Input::with_value` exists; the missing
  piece is select-all-on-focus.
- **[1.x] `LoadingIndicator` not mounted on the `loading` state** *(feature)* — a widget's
  `loading=True` reactive draws nothing in Rust (Python mounts an animated `●` `LoadingIndicator` over
  the widget). Test: `loading01`.
- **[divergence] Python-only startup crash** — `set_reactive01`'s Python reference intentionally raises
  (a pre-mount watcher fires `query_one` before compose → `NoMatches`, the doc's "wrong way"). Rust's
  deferred reactive init doesn't reproduce the crash; reproducing a Rich traceback glyph-for-glyph is
  neither feasible nor meaningful. Test: `set_reactive01`.
- **[divergence] Inline terminal render mode** — `run(inline=True)` (see *Deferred to 1.1* above).
  Tests: `howto_inline01`, `howto_inline02`.

## Tracked correctness follow-ups (no demo impact)

- **Per-screen toast racks** — notifications render on the base app's docked
  `ToastRack`; a toast posted while a modal/pushed screen is active is shown on
  the base rack (behind the screen), not over the pushed screen. Python mounts a
  `ToastRack` on every screen. Deferred because per-screen racks touch every
  screen push for a rarely-hit case; base-only degrades gracefully (no panic, no
  wrong-z bleed — covered by `notify_while_screen_pushed_degrades_gracefully`).
  The `Screen` default CSS declares only the `_toastrack` named layer so far;
  the fuller Python `Screen.layers` set (`_tooltips`, `_loading`, …) is a related
  follow-up.
- **Theme LAB shade tokens** — base/semantic tokens are byte-exact vs Python; the LAB-derived shade family (`$*-lighten-2/3`, `$*-darken-3`) can diverge up to ~42/channel on some themes (pre-existing `rgb_to_lab`/`lab_to_rgb` precision). No demo uses the divergent shades.
- `scroll_view.rs` retains a parallel widget-local scrollbar path with the pre-fix lane behavior (the canonical arena render path is correct).
- The Pilot headless key-cascade re-implements (rather than shares) the live event-loop key arm — converge onto a shared primitive to prevent drift.
- `intrinsic_wrapped_height` handles a single trailing blank line exactly; multiple trailing newlines could under-count vs Python `split(allow_blank=True)`.
- **[1.x] `Constrained` min-only / max-only + chrome-bearing child under-reports chrome** — a `Constrained` with ONLY a `min_height` or ONLY a `max_height` clamps its child's PURE-content `layout_height()` without adding the child's own vertical chrome (the flow layout's `measure_intrinsic_content_height` recursion owns chrome and only runs when a child reports `None`). So a bordered/flat child in such a `Constrained` can clip its chrome. The unconstrained arm was fixed (returns `None`, defers to the recursion like `Container`), which un-clipped the real regression (flat `Button` in a bare `Constrained` inside a `Row`; guarded by `keys_preview_snapshot`). No in-tree usage hits the min/max-only edge. Fully closing it means routing `Constrained`'s min/max into the node CSS layout so the flow applies them atop the recursion result — a constraint-routing change deferred to 1.x. See the arm comment in `src/widgets/containers/constrained.rs`.

These are scoped, isolated, and tracked — none block the core framework or real-application porting.
