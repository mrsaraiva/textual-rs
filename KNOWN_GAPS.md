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
- **Real-app interactive parity** (`tests/pty_interactive.rs`, real Rust vs real Python): **~68 / 117**
  exact and climbing. *(The earlier "139/141 LIVE" figure came from an in-process headless harness that
  diverged from the live loop and ignored color — it was inflated and is retired. Manual PTY spot-checks
  confirmed the real-app harness is truth.)*

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

These are scoped, isolated, and tracked — none block the core framework or real-application porting.
