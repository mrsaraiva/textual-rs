//! T-visual parity harness (tiered styled layer over the plain-text harness).
//!
//! Locked design: exact per-cell match, forced truecolor, no tmux — BOTH the
//! Rust example and the Python source run through the SAME portable-pty + vt100
//! path, and goldens store per-cell RGB. Catches color/background bugs the
//! plain-text `pty_parity` harness is blind to.
//!
//! AUTO-DISCOVERS every `styles/` + `guide/styles/` example that has a built
//! Rust binary + a Python source. Only the `PASSING` allowlist is ASSERTED
//! (must match Python exactly); the rest are reported as the color-parity
//! workstream (PENDING) or flagged READY (matches → promote into PASSING). The
//! test fails only on a PASSING regression.
//!
//!   REGEN_STYLED=1 cargo test --test visual_parity   # (re)gen goldens from Python
//!   REPORT_ONLY=1  cargo test --test visual_parity   # full tally, never panics
//!   cargo test --test visual_parity                  # assert PASSING set

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const ROWS: u16 = 30;
const COLS: u16 = 120;
const PYTHON: &str = "/tmp/textual-venv/bin/python";

/// Styled-verified examples (asserted exact). Grows as color-parity clusters land.
const PASSING: &[&str] = &[
    "background",
    "color",
    "color_auto",
    "border",
    "align_all",
    // Promoted from the 87-example full sizing (already cell-exact vs Python):
    "border_subtitle_align",
    "border_title_align",
    "grid_columns",
    "grid_gutter",
    "grid_rows",
    "grid_size_both",
    "grid_size_columns",
    "screen",
    // Promoted after the foreground/fill-surface fix (default-colored glyphs
    // inherit $foreground; content-align fill carries fg; other fill bg-only):
    "border_title_colors",
    "outline_vs_border",
    "colors",
    "colors01",
    "margin01",
    "widget",
    // Promoted after the vertical-extend fill fix (rows beyond content height
    // carry $foreground via visual_style; trailing horizontal pad stays bg-only):
    "content_align_all",
    "text_overflow",
    "text_wrap",
    "visibility",
    "dimensions04",
    // Promoted after removing Label's stray `fg: $foreground` default so an
    // explicit ancestor `color` (e.g. Screen { color: black }) inherits down:
    "margin",
    "outline",
    "padding",
    // Promoted after float-faithful auto/contrast compositing (blend_over_float):
    // avoids u8 alpha-quantization drift on $text/auto-color text:
    "max_height",
    "max_width",
    "min_width",
    // Promoted after skipping the fg-bearing align fill for the default
    // content-align (left, top) — matches Python's `!= ("left","top")` guard,
    // so the trailing pad of the content row stays background-only:
    "content_align",
    // Promoted after the Color.a u8->f32 float-alpha keystone: parsed-alpha bg/fg
    // blends (`red 10%`) now composite with the exact float factor (Python),
    // not the u8-quantized one — removes the ±1 RGB drift:
    "align",
    "background_transparency",
    "colors02",
    // Promoted after apply_parent_align now runs for Grid layout (was skipped by
    // an early-return guard) — border_all/outline_all grid cells are now centred
    // vertically/horizontally just like every other layout strategy:
    "border_all",
    "outline_all",
    // Promoted after Fix B: apply_host_scrollbar_layout now uses geometry.show_vertical
    // (content overflows AND allowed) as the scrollbar-widget SHOW flag instead of
    // geometry.vertical_lane_width>0.  This separates gutter RESERVATION (stable gutter)
    // from widget VISIBILITY, so scrollbar-gutter:stable reserves the lane without
    // displaying the scrollbar widget when content does not overflow.
    "scrollbar_gutter",
    // Promoted after paint_keylines was extended to draw the full outer boundary
    // (top/bottom horizontal lines + corner/T junctions) for Horizontal/Vertical
    // layouts — previously only drew interior vertical dividers.  Background of
    // keyline characters now preserves the surface beneath (bg-overlay fix):
    "keyline_horizontal",
    // Promoted after bumping rich-rs to 1.2.1: [link=url] markup no longer applies
    // a hardcoded cyan/underline (OSC8 meta only, matching Python Rich+Textual), so
    // link styling now comes from the CSS link-* tokens as Python does:
    "link_background_hover",
    "link_color_hover",
    "link_style",
    "link_style_hover",
    // Promoted after applying link-* CSS tokens to [@click=...] markup spans in
    // Label/Static render (mirrors Python's `widget.link_style` applied to @click
    // segments), and fixing CSS parser to handle `link-color: <color> <N>%` alpha
    // shorthand (same as `background`/`color` already supported):
    "link_color",
    // Promoted after migrating Label/Static render() to Content::render_strips:
    // text-style flags (bold/italic/underline/reverse), border, border_title,
    // box-sizing, padding, and dimension examples now use the Content pipeline
    // which correctly bakes visual_style + span_style into segments:
    "text_style_all",
    "border01",
    "border_title",
    "box_sizing01",
    "dimensions01",
    "dimensions02",
    "dimensions03",
    "outline01",
    "padding01",
    "text_opacity",
    // Color-residual (re-scoped to real roots): placeholder bg uses exact float 0.5
    // (was 128/255=0.50196) → column_span/row_span; border-fg + bg opacity now match
    // Python's double-application (background_colors blend + _apply_opacity) → opacity.
    "column_span",
    "row_span",
    "opacity",
    // Promoted after links-parity workstream: (1) css_id preserved post-mount via
    // css_id_cache so id-selector link-* rules apply to Static correctly; (2)
    // @click spans get link-color/link-background CSS overlay in Static::render();
    // (3) intrinsic_height counts trailing '\n' as a blank row; (4) trailing empty
    // strips emit an extra Segment::line() so split_and_crop_lines produces the right
    // row count (no spurious fill_fg_style bleed on blank trailing rows):
    "links",
    // tint: bake explicit host `color` (fg) into scrollbar track_style so
    // apply_style_to_segments sees s.color.is_some() and does not drop it —
    // matching Python `_Styled` applying fg to ALL scrollbar render segments.
    // background_tint: use Vertical::new().id() directly (not Node wrapper) so
    // `background: $panel` and `background-tint: ...` resolve on the same node,
    // matching Python `Vertical(Label(...), id="tint1")`.
    "tint",
    "background_tint",
    // Match Python exactly (promoted as free wins this pass): min_height was a
    // pre-existing missed promotion; box_sizing now matches after the Static
    // off-tree id() resolution stopped inserting a spurious Node wrapper level.
    "box_sizing",
    "min_height",
    // Promoted after the BOX vertical-extend fill discriminates chrome-only
    // containers from content widgets: a bordered Container renders no text
    // content (Python `Widget.render` -> `Blank(background_colors[1])`, bg-only),
    // so its interior extend rows are BG-ONLY even though `color` is inherited
    // from `Screen { color: $foreground }`. Content widgets (Static/Label) keep
    // the fg-bearing `visual_style` extend (Python `render_line` IndexError).
    // This fixes the docked/bordered container blank-row fg bleed.
    "dock_all",
    "margin_all",
    // Promoted after replacing Placeholder (cycling bg, centered label) with
    // Label::new("Widget") in the width/height example mains — matches Python's
    // bare Widget(): literal "Widget" text top-left (no content-align), green bg,
    // white fg. CSS adds the unset dimension (height:1fr / width:1fr) to mirror
    // Python's fill-the-screen default for a bare Widget. (Example-only fix;
    // Placeholder bg cycling must stay for max_width/min_width.)
    "width",
    "height",
    // Promoted after the text-render residual fixes:
    //  - text_align: (1) content widgets with `color: auto` now carry the
    //    auto-contrast fg into their vertical-extend fill rows (the vfill
    //    discriminator checked only `fg`, missing `fg_auto`); (2) implemented
    //    `text-align: justify` inter-word space distribution (Python
    //    `_FormattedLine.to_strip`), with the final paragraph line left-aligned
    //    and fg-bearing pad spaces.
    //  - link_background: `link-color: auto` (the default `$link-color`/`$text`)
    //    now recomputes its contrast against the LINK background (Python
    //    `auto_link_color` → `link_background.get_contrast_text(a)`), so a bright
    //    `link-background: $accent` yields dark link text instead of the screen
    //    contrast. Added `link_color_auto` marker to Style.
    "text_align",
    "link_background",
    // Promoted after fixing CSS `hatch` compositing: the fill is now DEFERRED
    // until after children render (so the inner content child of a `.class()`
    // Node wrapper can no longer un-hatch the first inner row) and SCOPED to the
    // node content box (inside border/padding) so it never bleeds into the
    // border row — matching Python `line_post`/`apply_hatch`. The `Node` wrapper
    // also gained `border_title`/`border_subtitle` support so the per-panel
    // titles render on the border (Python `static.border_title = hatch`).
    "hatch",
    // Free win (matched Python after the text-align/auto-fg pass moved it to READY):
    "grid",
    // Promoted after the fr-distribution / cumulative-floor layout pass:
    //  - `layout_resolve_1d_exact` sizes fixed AND fr children to exact f64 cells
    //    and floors the RUNNING position (Python `_resolve.resolve` +
    //    `layouts/{vertical,horizontal}.py`), so a stack of non-integer relative
    //    units (`12.5%`/`5w`/`12.5h`/`6.25vw`/`12.5vh` + `fr`) fence-posts like
    //    Python instead of each box truncating independently AND the fr children
    //    over-reserving against the un-carried integer fixed sizes.
    //    → width_comparison, height_comparison.
    //  - per-layer dock isolation: a `dock`ed widget on a SEPARATE layer
    //    (`layer: ruler`) overlays the flow region instead of carving it, in BOTH
    //    `resolve_layout` (flow region) and `host_content_extent` (scrollable
    //    virtual size) — without the latter the overlay Ruler inflated virtual_h
    //    and triggered a phantom scrollbar lane that shifted every relative-unit
    //    child. → width_comparison, height_comparison.
    //  - horizontal.rs width-aware height remeasure: a content-height child
    //    (unset OR `auto` height) in an `fr`-width row now re-measures its wrapped
    //    height at the RESOLVED width (was using the stale `layout_height()`), so a
    //    wrapping Label sized its box to the right line count. → text_style.
    "width_comparison",
    "height_comparison",
    "text_style",
    // Promoted after the visibility-as-inheritance + layout-of-hidden-subtrees fix:
    //  (1) `apply_display_visibility_to_tree` now inherits effective visibility
    //      down the tree (Python `DOMNode.visible`): a `visibility:hidden`
    //      container hides descendants, but a descendant with an explicit
    //      `visibility:visible` (`#bot > Placeholder`) re-shows.
    //  (2) layout no longer skips the descendants of a `visibility:hidden` node
    //      (only `display:none` removes from layout) — so a visible descendant of
    //      a hidden container gets a real rect and paints. Visibility is a
    //      paint-time concern (`render_tree_node::should_render`), not layout.
    //  (3) example rewired: id on the Horizontal itself (`Horizontal::new().id`)
    //      instead of a `Node` wrapper, so `#bot > Placeholder` resolves.
    "visibility_containers",
    // Promoted after the widget-render cluster (cycle 9 "widgetrender"):
    //  - padding02: `Static::intrinsic_height` routes through the shared
    //    word-wrap line counter (`text::intrinsic_wrapped_height`) instead of a
    //    naive `cell_len.div_ceil(width)` char-count, so wrapped paragraphs size
    //    to their real (larger) line count and no longer clip the tail.
    //  - padding_all: example rewired to put `id` on each `Placeholder` directly
    //    (Python `Placeholder(label, id=...)`) so the `#pN` padding rules AND the
    //    `Placeholder { width:auto; height:auto }` rule resolve on the SAME node;
    //    plus a new `Widget::auto_content_height()` hook (height counterpart of
    //    `auto_content_width`) lets a `height:auto` Placeholder shrink to its
    //    label's line count while an UNSET height still flex-fills the container.
    //  - display: DEFERRED — the faithful fix (seed-based `Static::class()`)
    //    cleared it but regressed nesting01/02's `align: center middle` by 2 rows
    //    (a margin-vs-`apply_parent_align` layout bug the Node wrapper masked).
    //    See `Static::class` doc-comment; re-land with the layout fix.
    "padding02",
    "padding_all",
    // Promoted after the TEXT-HEIGHT trailing-blank fix: `Label::intrinsic_height`
    // now routes through `text::intrinsic_wrapped_height` (Python
    // `Content.split(allow_blank=True)`), so a `Label(TEXT * N)` whose TEXT ends
    // with '\n' counts the trailing empty row — its auto/content height matches
    // Python (e.g. 71 vs 70 rows), driving the correct scroll geometry. (The
    // remaining scrollbars/overflow/scrollbar_size* cases still need the separate
    // scrollbar-thumb/lane geometry pass; only this single-Label case is exact.)
    "scrollbars2",
    // Promoted after the KEYLINE canvas-background fix: a container with a
    // `keyline` now paints its whole content box as a solid `fg=<bg> bg=<bg>`
    // canvas base BEFORE children render (Python `layout.py::render_keyline` ->
    // `Canvas.render(primitives, container.rich_style)`, whose spanned rows set the
    // blank fg to the background color). Visible children overpaint; the gutter and
    // `visibility:hidden` cell (the hidden Placeholder) show the canvas color
    // `fg=#121212 bg=#121212` instead of the screen's `fg=default` base blank.
    "keyline",
    // Promoted after the PERCENTAGE-WIDTH / horizontal-overflow-clip fix
    // (c13-scrollh): an explicit `width: 150%` child of a horizontally-scrollable
    // parent now resolves to 1.5x the container content width and KEEPS that
    // oversized width (the layout no longer clamps explicit widths to the
    // viewport in `layout_vertical` when `allow_h_overflow` is set). The
    // compositor clips the overflow to the viewport and the horizontal scrollbar
    // scrolls it — matching Python `_resolve.resolve_box_models` (no
    // `constrain_width`) + the clipping content region. `scrollbars` (Label
    // `width: 150%; height: 150%`) and `scrollbar_size` (Label `width: 200`)
    // are now cell-and-RGB exact vs Python.
    "scrollbars",
    "scrollbar_size",
    // Promoted after the border (sub)title markup + truncation + grid-vcenter pass
    // ("bordertitle"):
    //  - border (sub)titles now render through the Content markup pipeline
    //    (`Content::render_label_segments`) so embedded tags (`[b red]`,
    //    `[reverse]`, `[u][r]…[/]`, `white on black`) style the label, matching
    //    Python `_border.render_border_label`.
    //  - ellipsis truncation uses Python's exact edge arithmetic
    //    (`render_border_label` width-2 / `2*corners` reserve + `render_row`
    //    space distribution) so over-long titles cut with `…`.
    //  - layout reconciles a leaf's under-counted vertical chrome: `layout_height()`
    //    resolves border/padding OFF-TREE (id/class CSS invisible post-mount), so the
    //    vertical layout now adds the on-tree chrome the widget could not see — fixing
    //    the `align: center middle` off-by-one for id-bordered Labels.
    //  - markup `X on Y` color parse fixed (committed pending fg before `on`).
    "border_sub_title_align_all",
];

struct StyledCase {
    name: String,
    py_rel: String,
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Discover styled candidates: a Python source under styles/ or guide/styles/
/// that has a matching built Rust example binary.
fn discover() -> Vec<StyledCase> {
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    for sub in ["styles", "guide/styles"] {
        let dir = repo().join("../textual/docs/examples").join(sub);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "py").unwrap_or(false))
            .collect();
        paths.sort();
        for p in paths {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            if seen.contains(&stem) {
                continue;
            }
            let bin = repo()
                .join("docs/examples/target/debug/examples")
                .join(&stem);
            if !bin.exists() {
                continue;
            }
            seen.insert(stem.clone());
            cases.push(StyledCase {
                name: stem.clone(),
                py_rel: format!("{sub}/{stem}.py"),
            });
        }
    }
    cases
}

fn col(c: vt100::Color) -> String {
    match c {
        vt100::Color::Default => "def".into(),
        vt100::Color::Idx(i) => format!("idx{i}"),
        vt100::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

/// Capture a command's stable screen as RLE-per-row styled runs (cells sharing fg+bg).
fn capture(mut cmd: CommandBuilder, cwd: PathBuf) -> String {
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("LANG", "en_US.UTF-8");
    cmd.env("TEXTUAL_KEYBOARD_PROTOCOL", "off");
    cmd.env("TEXTUAL_SYNC_OUTPUT", "0");
    cmd.env("TEXTUAL_COLOR_SYSTEM", "truecolor");

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut child = pty.slave.spawn_command(cmd).expect("spawn");
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().expect("reader");
    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
    let feed = Arc::clone(&parser);
    let t = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            feed.lock().unwrap().process(&buf[..n]);
        }
    });

    let mut prev = String::new();
    let mut out = String::new();
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(250));
        let p = parser.lock().unwrap();
        let screen = p.screen();
        let mut text = String::new();
        let mut serial = String::new();
        for r in 0..ROWS {
            let mut start = 0u16;
            let (mut fg, mut bg, mut run) = (String::new(), String::new(), String::new());
            for c in 0..COLS {
                let (ch, cfg, cbg) = match screen.cell(r, c) {
                    Some(cl) => (cl.contents(), col(cl.fgcolor()), col(cl.bgcolor())),
                    None => (String::new(), "def".into(), "def".into()),
                };
                let chs = if ch.is_empty() { " ".to_string() } else { ch };
                text.push_str(&chs);
                if c == 0 {
                    start = 0;
                    fg = cfg;
                    bg = cbg;
                    run = chs;
                } else if cfg == fg && cbg == bg {
                    run.push_str(&chs);
                } else {
                    serial.push_str(&format!("[{start}-{}] {run:?} fg={fg} bg={bg}\n", c - 1));
                    start = c;
                    fg = cfg;
                    bg = cbg;
                    run = chs;
                }
            }
            serial.push_str(&format!(
                "[{start}-{}] {run:?} fg={fg} bg={bg}\n--row {r}--\n",
                COLS - 1
            ));
        }
        if !text.trim().is_empty() && text == prev {
            out = serial;
            break;
        }
        prev = text;
        out = serial;
    }

    child.kill().ok();
    child.wait().ok();
    drop(pty.master);
    t.join().ok();
    out
}

fn golden_path(name: &str) -> PathBuf {
    repo()
        .join("tests/pty_parity/golden_styled")
        .join(format!("{name}.styled"))
}

#[test]
fn visual_parity_batch() {
    let regen = std::env::var("REGEN_STYLED").is_ok();
    let report_only = std::env::var("REPORT_ONLY").is_ok();
    let cases = discover();
    let mut regressions: Vec<String> = Vec::new();
    let (mut n_pass, mut n_ready, mut n_pending, mut n_skip) = (0u32, 0u32, 0u32, 0u32);
    let mut ready: Vec<String> = Vec::new();

    for case in &cases {
        if regen {
            let script = repo().join("../textual/docs/examples").join(&case.py_rel);
            let cwd = script.parent().unwrap().to_path_buf();
            let mut cmd = CommandBuilder::new(PYTHON);
            cmd.arg(script.to_str().unwrap());
            let g = capture(cmd, cwd);
            if g.trim().is_empty() {
                eprintln!("regen SKIP {} (empty capture)", case.name);
                continue;
            }
            std::fs::create_dir_all(golden_path(&case.name).parent().unwrap()).ok();
            std::fs::write(golden_path(&case.name), &g).expect("write golden");
            continue;
        }
        let golden = match std::fs::read_to_string(golden_path(&case.name)) {
            Ok(g) => g,
            Err(_) => {
                n_skip += 1;
                continue;
            }
        };
        let bin = repo()
            .join("docs/examples/target/debug/examples")
            .join(&case.name);
        let actual = capture(CommandBuilder::new(bin.to_str().unwrap()), repo());
        let matches = actual.trim() == golden.trim();
        if !matches && std::env::var("DEBUG_CASE").map_or(false, |d| d == case.name || d == "ALL") {
            let (gl, al): (Vec<&str>, Vec<&str>) =
                (golden.lines().collect(), actual.lines().collect());
            eprintln!("--- DEBUG {} (py vs rust), first 12 diffs ---", case.name);
            let mut shown = 0;
            for i in 0..gl.len().max(al.len()) {
                let (g, a) = (
                    gl.get(i).copied().unwrap_or("<none>"),
                    al.get(i).copied().unwrap_or("<none>"),
                );
                if g != a {
                    eprintln!("  py  : {g}\n  rust: {a}");
                    shown += 1;
                    if shown >= 12 {
                        break;
                    }
                }
            }
        }
        if std::env::var("DUMP_CASE").map_or(false, |d| d == case.name) {
            eprintln!("--- DUMP {} actual (rust) ---", case.name);
            for line in actual.lines().take(50) {
                eprintln!("  {line}");
            }
        }
        // DUMP_FILE=<name>: write the full actual + golden captures side-by-side to
        // /tmp for offline structural diffing (no take() truncation).
        if std::env::var("DUMP_FILE").map_or(false, |d| d == case.name) {
            std::fs::write(format!("/tmp/vp_{}_actual.txt", case.name), &actual).ok();
            std::fs::write(format!("/tmp/vp_{}_golden.txt", case.name), &golden).ok();
            eprintln!(
                "--- DUMP_FILE wrote /tmp/vp_{}_{{actual,golden}}.txt ---",
                case.name
            );
        }
        let passing = PASSING.contains(&case.name.as_str());
        match (matches, passing) {
            (true, true) => n_pass += 1,
            (true, false) => {
                n_ready += 1;
                ready.push(case.name.clone());
            }
            (false, false) => n_pending += 1,
            (false, true) => {
                eprintln!("REGRESSION {} (in PASSING but now diverges)", case.name);
                regressions.push(case.name.clone());
            }
        }
    }

    if !regen {
        eprintln!(
            "\nstyled tally (of {} discovered): {n_pass} PASS, {n_ready} READY-to-promote, \
             {n_pending} PENDING (workstream), {n_skip} no-golden",
            cases.len()
        );
        if !ready.is_empty() {
            eprintln!(
                "READY (match Python — add to PASSING): {}",
                ready.join(", ")
            );
        }
    }
    if !regen && !report_only && !regressions.is_empty() {
        panic!("styled PASSING regressed: {}", regressions.join(", "));
    }
}
