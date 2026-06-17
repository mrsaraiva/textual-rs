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

// First batch: color-focused styles examples.
const CASES: &[StyledCase] = &[
    StyledCase { name: "background", bin: "background", py_rel: "styles/background.py", xfail: None },
    StyledCase { name: "color", bin: "color", py_rel: "styles/color.py", xfail: None },
    StyledCase { name: "color_auto", bin: "color_auto", py_rel: "styles/color_auto.py", xfail: None },
    StyledCase {
        name: "background_tint",
        bin: "background_tint",
        py_rel: "styles/background_tint.py",
        xfail: Some("`color: auto 90%` auto-contrast color + background-tint blend rounding differ \
                     (fg #e0e0e0 vs py #e9eaeb; bg off-by-1)"),
    },
    StyledCase {
        name: "colors",
        bin: "colors",
        py_rel: "guide/styles/colors.py",
        xfail: Some("Rust leaves un-set text fg as terminal-default; Python emits the resolved \
                     theme $text (#e0e0e0). Default-foreground emission model."),
    },
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
    let py_base = repo().join("../textual/docs/examples");
    let mut failures: Vec<String> = Vec::new();

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
                (true, None) => eprintln!("PASS  {}", case.name),
                (false, Some(reason)) => eprintln!("XFAIL {} (known: {reason})", case.name),
                (true, Some(_)) => {
                    eprintln!("UNEXPECTED PASS {} — its xfail bug appears fixed; clear the xfail", case.name);
                    failures.push(format!("{} (unexpected-pass)", case.name));
                }
                (false, None) => {
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
                    failures.push(case.name.to_string());
                }
            }
        }
    }

    if !regen && !failures.is_empty() {
        panic!("styled parity FAILED for: {}", failures.join(", "));
    }
}
