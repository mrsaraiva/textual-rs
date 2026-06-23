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
    "background", "color", "color_auto", "border", "align_all",
    // Promoted from the 87-example full sizing (already cell-exact vs Python):
    "border_subtitle_align", "border_title_align", "grid_columns", "grid_gutter",
    "grid_rows", "grid_size_both", "grid_size_columns", "screen",
    // Promoted after the foreground/fill-surface fix (default-colored glyphs
    // inherit $foreground; content-align fill carries fg; other fill bg-only):
    "border_title_colors", "outline_vs_border", "colors", "colors01", "margin01",
    "widget",
    // Promoted after the vertical-extend fill fix (rows beyond content height
    // carry $foreground via visual_style; trailing horizontal pad stays bg-only):
    "content_align_all", "text_overflow", "text_wrap", "visibility", "dimensions04",
    // Promoted after removing Label's stray `fg: $foreground` default so an
    // explicit ancestor `color` (e.g. Screen { color: black }) inherits down:
    "margin", "outline", "padding",
    // Promoted after float-faithful auto/contrast compositing (blend_over_float):
    // avoids u8 alpha-quantization drift on $text/auto-color text:
    "max_height", "max_width", "min_width",
    // Promoted after skipping the fg-bearing align fill for the default
    // content-align (left, top) — matches Python's `!= ("left","top")` guard,
    // so the trailing pad of the content row stays background-only:
    "content_align",
    // Promoted after the Color.a u8->f32 float-alpha keystone: parsed-alpha bg/fg
    // blends (`red 10%`) now composite with the exact float factor (Python),
    // not the u8-quantized one — removes the ±1 RGB drift:
    "align", "background_transparency", "colors02",
    // Promoted after apply_parent_align now runs for Grid layout (was skipped by
    // an early-return guard) — border_all/outline_all grid cells are now centred
    // vertically/horizontally just like every other layout strategy:
    "border_all", "outline_all",
    // Promoted after paint_keylines was extended to draw the full outer boundary
    // (top/bottom horizontal lines + corner/T junctions) for Horizontal/Vertical
    // layouts — previously only drew interior vertical dividers.  Background of
    // keyline characters now preserves the surface beneath (bg-overlay fix):
    "keyline_horizontal",
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
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
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
            let bin = repo().join("docs/examples/target/debug/examples").join(&stem);
            if !bin.exists() {
                continue;
            }
            seen.insert(stem.clone());
            cases.push(StyledCase { name: stem.clone(), py_rel: format!("{sub}/{stem}.py") });
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
        .openpty(PtySize { rows: ROWS, cols: COLS, pixel_width: 0, pixel_height: 0 })
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
            serial.push_str(&format!("[{start}-{}] {run:?} fg={fg} bg={bg}\n--row {r}--\n", COLS - 1));
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
    repo().join("tests/pty_parity/golden_styled").join(format!("{name}.styled"))
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
        let bin = repo().join("docs/examples/target/debug/examples").join(&case.name);
        let actual = capture(CommandBuilder::new(bin.to_str().unwrap()), repo());
        let matches = actual.trim() == golden.trim();
        if !matches && std::env::var("DEBUG_CASE").map_or(false, |d| d == case.name || d == "ALL") {
            let (gl, al): (Vec<&str>, Vec<&str>) = (golden.lines().collect(), actual.lines().collect());
            eprintln!("--- DEBUG {} (py vs rust), first 12 diffs ---", case.name);
            let mut shown = 0;
            for i in 0..gl.len().max(al.len()) {
                let (g, a) = (gl.get(i).copied().unwrap_or("<none>"), al.get(i).copied().unwrap_or("<none>"));
                if g != a {
                    eprintln!("  py  : {g}\n  rust: {a}");
                    shown += 1;
                    if shown >= 12 { break; }
                }
            }
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
            eprintln!("READY (match Python — add to PASSING): {}", ready.join(", "));
        }
    }
    if !regen && !report_only && !regressions.is_empty() {
        panic!("styled PASSING regressed: {}", regressions.join(", "));
    }
}
