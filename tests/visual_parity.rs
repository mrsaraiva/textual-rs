//! T-visual parity harness (tiered styled layer over the plain-text harness).
//!
//! Locked design: exact per-cell match, forced truecolor, no tmux — BOTH the
//! Rust example and the Python source run through the SAME portable-pty + vt100
//! path, and goldens store per-cell RGB. This catches color/background bugs the
//! plain-text `pty_parity` harness is blind to.
//!
//! Goldens are generated FROM PYTHON (the parity source of truth):
//!   REGEN_STYLED=1 cargo test --test visual_parity
//! and compared against the Rust example:
//!   cargo test --test visual_parity

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const ROWS: u16 = 30;
const COLS: u16 = 120;
const PYTHON: &str = "/tmp/textual-venv/bin/python";

struct StyledCase {
    name: &'static str,
    bin: &'static str,    // rust example binary under docs/examples/target/debug/examples/
    py_rel: &'static str, // relative to ../textual/docs/examples/
    /// `Some(reason)` => known styled divergence (documented bug); the harness
    /// asserts it STILL diverges (a match means the bug was fixed — update this).
    xfail: Option<&'static str>,
}

macro_rules! sc {
    ($n:literal, $p:literal) => { StyledCase { name: $n, bin: $n, py_rel: $p, xfail: None } };
    ($n:literal, $p:literal, $x:literal) => { StyledCase { name: $n, bin: $n, py_rel: $p, xfail: Some($x) } };
}

// Styles batch (color-focused). Measured 2026-06-17 at cell-RGB exactness:
// 5 PASS, 16 XFAIL. The xfails are the COLOR-PARITY WORKSTREAM — the plain-text
// pty harness can't see these; each is a real per-cell-color divergence vs Python.
// Clusters: default-fg emission (Rust leaves un-set text fg as terminal-default;
// Python emits resolved $foreground/$text — needs base `color: $foreground` +
// fg applied to content cells), color blend (tint/opacity), border/outline/
// scrollbar/hatch color application, auto-contrast `color: auto N%`.
const CASES: &[StyledCase] = &[
    sc!("background", "styles/background.py"),
    sc!("color", "styles/color.py"),
    sc!("color_auto", "styles/color_auto.py"),
    sc!("text_style_all", "styles/text_style_all.py", "styled color-parity: text-style + default-fg"),
    sc!("tint", "styles/tint.py", "styled color-parity: tint blend"),
    sc!("background_transparency", "styles/background_transparency.py", "styled color-parity: bg alpha blend"),
    sc!("opacity", "styles/opacity.py", "styled color-parity: opacity blend"),
    sc!("text_opacity", "styles/text_opacity.py", "styled color-parity: text-opacity blend"),
    sc!("hatch", "styles/hatch.py", "styled color-parity: hatch color"),
    sc!("border", "styles/border.py"),
    sc!("outline", "styles/outline.py", "styled color-parity: outline color + default-fg"),
    sc!("scrollbar_size", "styles/scrollbar_size.py", "styled color-parity: scrollbar color"),
    sc!("text_overflow", "styles/text_overflow.py", "styled color-parity: default-fg"),
    sc!("text_wrap", "styles/text_wrap.py", "styled color-parity: default-fg"),
    sc!("align_all", "styles/align_all.py"),
    sc!("content_align_all", "styles/content_align_all.py", "styled color-parity: default-fg"),
    sc!("margin", "styles/margin.py", "styled color-parity: default-fg/surface"),
    sc!("padding", "styles/padding.py", "styled color-parity: default-fg/surface"),
    sc!("link_color", "styles/link_color.py", "styled color-parity: link color application"),
    sc!("background_tint", "styles/background_tint.py",
        "styled color-parity: color: auto N% auto-contrast + background-tint blend"),
    sc!("colors", "guide/styles/colors.py",
        "styled color-parity: default-foreground emission (un-set text fg vs $text)"),
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn col(c: vt100::Color) -> String {
    match c {
        vt100::Color::Default => "def".into(),
        vt100::Color::Idx(i) => format!("idx{i}"),
        vt100::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

/// Capture a command's stable screen as a styled grid, serialized RLE-per-row
/// (runs of cells sharing fg+bg). Trailing whitespace at end of each row is
/// trimmed only when its bg is the row's final run bg AND chars are blank — we
/// keep bg-significant trailing cells.
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
            if n == 0 { break; }
            feed.lock().unwrap().process(&buf[..n]);
        }
    });

    let mut prev = String::new();
    let mut out = String::new();
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(250));
        let p = parser.lock().unwrap();
        let screen = p.screen();
        let mut text = String::new();
        let mut serial = String::new();
        for r in 0..ROWS {
            let mut run_start = 0u16;
            let mut run_fg = String::new();
            let mut run_bg = String::new();
            let mut run_txt = String::new();
            let mut flush = |start: u16, end: u16, txt: &str, fg: &str, bg: &str, s: &mut String| {
                if !fg.is_empty() {
                    s.push_str(&format!("[{start}-{end}] {:?} fg={fg} bg={bg}\n", txt));
                }
            };
            for c in 0..COLS {
                let cell = screen.cell(r, c);
                let (ch, fg, bg) = match cell {
                    Some(cl) => (cl.contents(), col(cl.fgcolor()), col(cl.bgcolor())),
                    None => (String::new(), "def".into(), "def".into()),
                };
                let chs = if ch.is_empty() { " ".to_string() } else { ch };
                text.push_str(&chs);
                if c == 0 {
                    run_start = 0; run_fg = fg; run_bg = bg; run_txt = chs;
                } else if fg == run_fg && bg == run_bg {
                    run_txt.push_str(&chs);
                } else {
                    flush(run_start, c - 1, &run_txt, &run_fg, &run_bg, &mut serial);
                    run_start = c; run_fg = fg; run_bg = bg; run_txt = chs;
                }
            }
            flush(run_start, COLS - 1, &run_txt, &run_fg, &run_bg, &mut serial);
            serial.push_str(&format!("--row {r}--\n"));
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
    let report_only = std::env::var("REPORT_ONLY").is_ok(); // tally, never panic
    let py_base = repo().join("../textual/docs/examples");
    let mut failures: Vec<String> = Vec::new();
    let (mut n_pass, mut n_xfail, mut n_fail) = (0u32, 0u32, 0u32);

    for case in CASES {
        if regen {
            let script = py_base.join(case.py_rel);
            assert!(script.exists(), "missing py {}", script.display());
            let cwd = script.parent().unwrap().to_path_buf();
            let mut cmd = CommandBuilder::new(PYTHON);
            cmd.arg(script.to_str().unwrap());
            let g = capture(cmd, cwd);
            std::fs::create_dir_all(golden_path(case.name).parent().unwrap()).ok();
            std::fs::write(golden_path(case.name), &g).expect("write golden");
            eprintln!("regen {} ({} rows captured)", case.name, g.matches("--row").count());
        } else {
            let bin = repo().join("docs/examples/target/debug/examples").join(case.bin);
            if !bin.exists() {
                eprintln!("SKIP  {} (rust bin not built)", case.name);
                continue;
            }
            let golden = match std::fs::read_to_string(golden_path(case.name)) {
                Ok(g) => g,
                Err(_) => {
                    eprintln!("SKIP  {} (no styled golden; run REGEN_STYLED=1)", case.name);
                    continue;
                }
            };
            let actual = capture(CommandBuilder::new(bin.to_str().unwrap()), repo());
            let matches = actual.trim() == golden.trim();
            match (matches, case.xfail) {
                (true, None) => { n_pass += 1; eprintln!("PASS  {}", case.name); }
                (false, Some(reason)) => { n_xfail += 1; eprintln!("XFAIL {} (known: {reason})", case.name); }
                (true, Some(_)) => {
                    eprintln!("UNEXPECTED PASS {} — its xfail bug appears fixed; clear the xfail", case.name);
                    if !report_only { failures.push(format!("{} (unexpected-pass)", case.name)); }
                }
                (false, None) => {
                    n_fail += 1;
                    let gl: Vec<&str> = golden.lines().collect();
                    let al: Vec<&str> = actual.lines().collect();
                    let mut first = String::from("(len diff)");
                    for i in 0..gl.len().max(al.len()) {
                        let g = gl.get(i).copied().unwrap_or("<none>");
                        let a = al.get(i).copied().unwrap_or("<none>");
                        if g != a {
                            first = format!("line {i}:\n    py  : {g}\n    rust: {a}");
                            break;
                        }
                    }
                    eprintln!("FAIL  {}\n  {first}", case.name);
                    if !report_only { failures.push(case.name.to_string()); }
                }
            }
        }
    }

    if !regen {
        eprintln!("\nstyled tally: {n_pass} PASS, {n_xfail} XFAIL, {n_fail} FAIL (of {})", CASES.len());
    }
    if !regen && !report_only && !failures.is_empty() {
        panic!("styled parity FAILED for: {}", failures.join(", "));
    }
}
