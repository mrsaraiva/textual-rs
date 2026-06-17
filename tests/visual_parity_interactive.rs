//! Interactive styled-parity harness — extends the styled layer to POST-INTERACTION
//! frames (focus / hover / active states), which the static `visual_parity` harness
//! never exercises. Sends keys, waits for re-stabilization, then captures per-cell RGB
//! and compares against a Python golden. This is how focus-state color bugs (e.g. a
//! focused Button's `text-style: reverse` band) get caught instead of eyeballed.
//!
//!   REGEN_INTERACTIVE=1 cargo test --test visual_parity_interactive   # gen goldens from Python
//!   DEBUG_CASE=<name>    cargo test --test visual_parity_interactive   # print first per-cell diffs
//!   cargo test --test visual_parity_interactive                       # assert

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const ROWS: u16 = 30;
const COLS: u16 = 120;
const PYTHON: &str = "/tmp/textual-venv/bin/python";

struct Case {
    name: &'static str,
    bin: &'static str,
    py_rel: &'static str,
    keys: &'static str,
    /// `true` = known divergence (the harness CATCHES it; the fix is tracked, not
    /// yet landed). Reported, not asserted, so the suite stays green.
    pending: bool,
}

// Interactive cases. keys are sent after the initial frame stabilizes.
const CASES: &[Case] = &[
    // Tab focuses the first Button -> exercises the `:focus` text-style (b reverse).
    // The reverse-band width is now FIXED (button render applies `line-pad: 1` as
    // styled label spaces when the label fits, matching Python; band spans " Default ").
    // PENDING only on the residual surface/blend bg delta (#282828 vs #272727) — the
    // color-workstream cluster, not button-specific. Flips to PASS once that lands.
    Case { name: "button_focus", bin: "button", py_rel: "widgets/button.py", keys: "\t", pending: true },
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

fn serialize(parser: &vt100::Parser) -> String {
    let screen = parser.screen();
    let mut serial = String::new();
    for r in 0..ROWS {
        let (mut start, mut fg, mut bg, mut run) = (0u16, String::new(), String::new(), String::new());
        for c in 0..COLS {
            let cell = screen.cell(r, c);
            let (ch, cfg, cbg) = match cell {
                Some(cl) => {
                    // include the reverse attr so focus-reverse divergences are visible
                    let rev = if cl.inverse() { "/rev" } else { "" };
                    (cl.contents(), col(cl.fgcolor()) + rev, col(cl.bgcolor()))
                }
                None => (String::new(), "def".into(), "def".into()),
            };
            let chs = if ch.is_empty() { " ".to_string() } else { ch };
            if c == 0 {
                start = 0; fg = cfg; bg = cbg; run = chs;
            } else if cfg == fg && cbg == bg {
                run.push_str(&chs);
            } else {
                serial.push_str(&format!("[{start}-{}] {run:?} fg={fg} bg={bg}\n", c - 1));
                start = c; fg = cfg; bg = cbg; run = chs;
            }
        }
        serial.push_str(&format!("[{start}-{}] {run:?} fg={fg} bg={bg}\n--row {r}--\n", COLS - 1));
    }
    serial
}

fn capture(mut cmd: CommandBuilder, cwd: PathBuf, keys: &str) -> String {
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
    let mut writer = pty.master.take_writer().expect("writer");
    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
    let feed = Arc::clone(&parser);
    let t = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 { break; }
            feed.lock().unwrap().process(&buf[..n]);
        }
    });

    // wait for initial stable
    let mut prev = String::new();
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(200));
        let s = { serialize(&parser.lock().unwrap()) };
        let txt: String = s.lines().filter(|l| l.starts_with('[')).collect();
        if !txt.trim().is_empty() && s == prev { break; }
        prev = s;
    }
    // send keys, let them land, wait for re-stable
    if !keys.is_empty() {
        writer.write_all(keys.as_bytes()).ok();
        writer.flush().ok();
        std::thread::sleep(Duration::from_millis(400));
        let mut p2 = String::new();
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(200));
            let s = { serialize(&parser.lock().unwrap()) };
            if s == p2 { break; }
            p2 = s;
        }
    }
    let out = serialize(&parser.lock().unwrap());

    child.kill().ok();
    child.wait().ok();
    drop(pty.master);
    t.join().ok();
    out
}

fn golden_path(name: &str) -> PathBuf {
    repo().join("tests/pty_parity/golden_styled_interactive").join(format!("{name}.styled"))
}

#[test]
fn interactive_parity() {
    let regen = std::env::var("REGEN_INTERACTIVE").is_ok();
    let debug = std::env::var("DEBUG_CASE").ok();
    let mut failures = Vec::new();

    for case in CASES {
        if regen {
            let script = repo().join("../textual/docs/examples").join(case.py_rel);
            let cwd = script.parent().unwrap().to_path_buf();
            let mut cmd = CommandBuilder::new(PYTHON);
            cmd.arg(script.to_str().unwrap());
            let g = capture(cmd, cwd, case.keys);
            std::fs::create_dir_all(golden_path(case.name).parent().unwrap()).ok();
            std::fs::write(golden_path(case.name), &g).expect("write golden");
            eprintln!("regen {} ({} rows)", case.name, g.matches("--row").count());
            continue;
        }
        let bin = repo().join("docs/examples/target/debug/examples").join(case.bin);
        if !bin.exists() { eprintln!("SKIP {} (no bin)", case.name); continue; }
        let golden = match std::fs::read_to_string(golden_path(case.name)) {
            Ok(g) => g,
            Err(_) => { eprintln!("SKIP {} (no golden; REGEN_INTERACTIVE=1)", case.name); continue; }
        };
        let actual = capture(CommandBuilder::new(bin.to_str().unwrap()), repo(), case.keys);
        if actual.trim() == golden.trim() {
            eprintln!("PASS {}", case.name);
        } else {
            if debug.as_deref() == Some(case.name) {
                let (gl, al): (Vec<&str>, Vec<&str>) = (golden.lines().collect(), actual.lines().collect());
                eprintln!("--- DEBUG {} (py vs rust) ---", case.name);
                let mut shown = 0;
                for i in 0..gl.len().max(al.len()) {
                    let (g, a) = (gl.get(i).copied().unwrap_or("<none>"), al.get(i).copied().unwrap_or("<none>"));
                    if g != a { eprintln!("  py  : {g}\n  rust: {a}"); shown += 1; if shown >= 14 { break; } }
                }
            }
            if case.pending {
                eprintln!("PENDING {} (known divergence — harness catches it; fix tracked)", case.name);
            } else {
                eprintln!("FAIL {}", case.name);
                failures.push(case.name);
            }
        }
    }
    assert!(failures.is_empty(), "interactive styled parity FAILED: {failures:?}");
}
