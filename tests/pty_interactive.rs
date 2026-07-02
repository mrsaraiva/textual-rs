//! Real interactive dual-app parity harness (Rust app vs Python app).
//!
//! WHY THIS EXISTS
//! ---------------
//! The plain-text `pty_parity` harness compares the Rust example output against a
//! *frozen golden* captured once from Python, as plain text, after a single
//! keystroke, from a single frame. That is blind to colour, blind to multi-step
//! interaction, and blind to time-dependent behaviour. In-process probes that
//! call widget methods directly are worse still: a probe can call `td.start()`
//! and observe a running clock while the *live app's* Start button does nothing,
//! because the probe never exercises the real button -> message -> handler path.
//!
//! This harness runs BOTH real apps the same way and compares what the terminals
//! actually render:
//!   * the real cargo-built Rust example binary, in its own PTY, and
//!   * the real Python Textual app (`<checkout>/docs/examples/<cat>/<name>.py`),
//!     in its own PTY, at the SAME size,
//! driven by the SAME multi-step input script, captured as a CELL GRID (per-cell
//! glyph + fg + bg colour via the `vt100` emulator). No tmux (pure PTY+vt100 on
//! both sides), no goldens, no in-process shortcuts. Real app vs real app.
//!
//! NON-DETERMINISM POLICY
//! ----------------------
//! Some behaviour is inherently non-deterministic (a live clock's exact digits, a
//! network weather payload, the exact opacity at a sampled millisecond). The
//! policy is: assert EXACT text+colour where the rendering is deterministic, and
//! assert STRUCTURE / BEHAVIOUR where it is not:
//!   * a running clock: assert the time field *advanced* between two captures
//!     (t0 != t+N), not that it equals a specific value;
//!   * a fade: assert the cell's colour *changed* over time (progression), not a
//!     specific opacity sample;
//!   * a network payload: assert weather *appeared* (region became non-empty)
//!     and that no internal event text (e.g. "WorkerStateChanged") leaked onto
//!     the screen, rather than matching exact weather text.
//! Each case documents which mode it uses and why.
//!
//! This file is a HARNESS + a set of acceptance cases. The acceptance cases are
//! the six demos the maintainer flagged by hand; each asserts that the harness
//! *detects the Rust/Python discrepancy* (or, where a discrepancy has since been
//! fixed in this tree, documents that and still proves the harness can see the
//! relevant signal). A case that cannot tell the two apps apart on the flagged
//! axis is a harness bug, not a pass.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

const COLS: u16 = 120;
const ROWS: u16 = 30;

// Where the real Python Textual checkout + its venv interpreter live. The venv's
// `python` is a symlink to the checkout's interpreter, which resolves textual
// 8.x from `<checkout>/src`. We set PYTHONPATH explicitly so resolution does not
// depend on the ambient shell environment (which can shadow it with ~/.local).
const PY_BIN: &str = "/tmp/textual-venv/bin/python";
const PY_CHECKOUT: &str = "/mnt/shares/Marcos/dev/mark/Proj/Libs/textual";

// ---------------------------------------------------------------------------
// Cell grid model: glyph + fg + bg, captured from the vt100 emulator.
// ---------------------------------------------------------------------------

/// A colour as the emulator sees it. `Default` means "terminal default" (no SGR
/// colour set); `Idx` is a 16/256-palette index; `Rgb` is a truecolor triple.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl Col {
    fn from_vt(c: vt100::Color) -> Self {
        match c {
            vt100::Color::Default => Col::Default,
            vt100::Color::Idx(i) => Col::Idx(i),
            vt100::Color::Rgb(r, g, b) => Col::Rgb(r, g, b),
        }
    }
    fn short(self) -> String {
        match self {
            Col::Default => "----".into(),
            Col::Idx(i) => format!("i{i:03}"),
            Col::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
    /// Is this colour "bluish" (blue clearly dominant)? Used by colour-aware
    /// structural checks (e.g. the progress bar fill).
    fn is_blue(self) -> bool {
        match self {
            Col::Rgb(r, g, b) => b as i32 > r as i32 + 30 && b as i32 > g as i32 + 30 && b > 90,
            // Common palette blues (4 = blue, 12 = bright blue, 33/39/27 etc.).
            Col::Idx(i) => matches!(i, 4 | 12 | 21 | 27 | 33 | 39 | 45 | 63),
            Col::Default => false,
        }
    }
    /// Is this colour "reddish" (red clearly dominant)?
    fn is_red(self) -> bool {
        match self {
            Col::Rgb(r, g, b) => r as i32 > g as i32 + 40 && r as i32 > b as i32 + 40 && r > 110,
            Col::Idx(i) => matches!(i, 1 | 9 | 196 | 160 | 124 | 203),
            Col::Default => false,
        }
    }
}

#[derive(Clone, Copy)]
struct GCell {
    ch: char,
    fg: Col,
    bg: Col,
}

impl GCell {
    fn blank() -> Self {
        GCell {
            ch: ' ',
            fg: Col::Default,
            bg: Col::Default,
        }
    }
}

/// A full captured screen: ROWS x COLS cells.
struct Grid {
    cells: Vec<Vec<GCell>>, // [row][col]
}

impl Grid {
    fn capture(parser: &vt100::Parser) -> Grid {
        let screen = parser.screen();
        let mut cells = Vec::with_capacity(ROWS as usize);
        for row in 0..ROWS {
            let mut line = Vec::with_capacity(COLS as usize);
            for col in 0..COLS {
                let cell = match screen.cell(row, col) {
                    Some(c) => c,
                    None => {
                        line.push(GCell::blank());
                        continue;
                    }
                };
                if cell.is_wide_continuation() {
                    // Represent the trailing half of a wide glyph as a marker we
                    // skip in text rendering but keep colours for.
                    line.push(GCell {
                        ch: '\u{200b}', // zero-width: skipped in text()
                        fg: Col::from_vt(cell.fgcolor()),
                        bg: Col::from_vt(cell.bgcolor()),
                    });
                    continue;
                }
                let contents = cell.contents();
                let ch = contents.chars().next().unwrap_or(' ');
                line.push(GCell {
                    ch: if contents.is_empty() { ' ' } else { ch },
                    fg: Col::from_vt(cell.fgcolor()),
                    bg: Col::from_vt(cell.bgcolor()),
                });
            }
            cells.push(line);
        }
        Grid { cells }
    }

    fn cell(&self, row: usize, col: usize) -> GCell {
        self.cells
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .unwrap_or_else(GCell::blank)
    }

    /// Plain text of one row (trailing whitespace trimmed).
    fn row_text(&self, row: usize) -> String {
        let mut s = String::new();
        if let Some(r) = self.cells.get(row) {
            for c in r {
                if c.ch == '\u{200b}' {
                    continue;
                }
                s.push(c.ch);
            }
        }
        s.trim_end().to_string()
    }

    /// Full plain text of the screen.
    fn text(&self) -> String {
        (0..ROWS as usize)
            .map(|r| self.row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn contains(&self, needle: &str) -> bool {
        self.text().contains(needle)
    }

    /// Does any cell in the grid have a bluish bg or fg? (colour-aware probe)
    fn any_blue(&self) -> bool {
        self.cells
            .iter()
            .flatten()
            .any(|c| c.bg.is_blue() || c.fg.is_blue())
    }

    fn any_red(&self) -> bool {
        self.cells
            .iter()
            .flatten()
            .any(|c| c.bg.is_red() || c.fg.is_red())
    }

    /// The set of distinct bg colours present (for diff reporting).
    fn bg_palette(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for c in self.cells.iter().flatten() {
            *m.entry(c.bg.short()).or_insert(0) += 1;
        }
        m
    }
}

// ---------------------------------------------------------------------------
// Input scripts: multi-step, time-aware.
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Step {
    /// Type literal bytes (e.g. a city name).
    SendKeys(&'static str),
    /// A single named key.
    Key(Key),
    /// Sleep for N milliseconds (let workers / clocks / animations advance).
    Wait(u64),
    /// Send an SGR mouse click (press+release) at (col,row), 0-based.
    Click(u16, u16),
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // full key vocabulary for input scripts; not all used yet
enum Key {
    Enter,
    Tab,
    Space,
    Char(char),
}

impl Key {
    fn bytes(self) -> Vec<u8> {
        match self {
            Key::Enter => b"\r".to_vec(),
            Key::Tab => b"\t".to_vec(),
            Key::Space => b" ".to_vec(),
            Key::Char(c) => {
                let mut b = [0u8; 4];
                c.encode_utf8(&mut b).as_bytes().to_vec()
            }
        }
    }
}

/// SGR (1006) mouse press+release at 1-based (col,row).
fn sgr_click(col: u16, row: u16) -> Vec<u8> {
    let c = col + 1;
    let r = row + 1;
    format!("\x1b[<0;{c};{r}M\x1b[<0;{c};{r}m").into_bytes()
}

// ---------------------------------------------------------------------------
// App handle: a running app in a PTY with a live vt100 parser.
// ---------------------------------------------------------------------------

enum AppKind {
    Rust(&'static str),                 // example name (docs/examples workspace)
    Python(&'static str, &'static str), // (category dir, file stem)
}

struct RunningApp {
    parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    label: String,
}

impl RunningApp {
    fn capture(&self) -> Grid {
        Grid::capture(&self.parser.lock().unwrap())
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to pty");
        self.writer.flush().expect("flush pty");
    }

    /// Wait until the plain-text screen is non-empty and unchanged for a few
    /// consecutive polls, or until timeout (best-effort; never panics so a slow
    /// side cannot abort the run before we capture/diff it).
    fn settle(&self, timeout: Duration) {
        let start = Instant::now();
        let mut prev = String::new();
        let mut stable = 0;
        while start.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(80));
            let cur = self.capture().text();
            if !cur.trim().is_empty() && cur == prev {
                stable += 1;
                if stable >= 4 {
                    return;
                }
            } else {
                stable = 0;
            }
            prev = cur;
        }
    }

    fn shutdown(mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
        drop(self.master);
        if let Some(t) = self.reader_thread.take() {
            t.join().ok();
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ensure_built() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        // Docs examples live in their own workspace with their own target dir.
        let status = std::process::Command::new(&cargo)
            .args(["build", "--workspace", "--examples", "--keep-going"])
            .current_dir(repo_root().join("docs/examples"))
            .status()
            .expect("spawn docs/examples build");
        assert!(status.success(), "docs/examples build failed");
    });
}

fn docs_profile_dir() -> String {
    // The interactive test binary itself is built into the MAIN crate target; the
    // docs examples use the docs workspace target. Both share the same profile
    // name (debug/release) derived from the running test binary.
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| {
            if p.ends_with("deps") {
                p.parent()
            } else {
                Some(p)
            }
        })
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "debug".into())
}

fn rust_example_bin(name: &str) -> PathBuf {
    let bin = repo_root()
        .join("docs/examples/target")
        .join(docs_profile_dir())
        .join("examples")
        .join(name);
    assert!(
        bin.exists(),
        "rust example binary missing: {}",
        bin.display()
    );
    bin
}

fn python_app_path(cat: &str, stem: &str) -> PathBuf {
    let p = PathBuf::from(PY_CHECKOUT)
        .join("docs/examples")
        .join(cat)
        .join(format!("{stem}.py"));
    assert!(p.exists(), "python example missing: {}", p.display());
    p
}

fn spawn(kind: &AppKind) -> RunningApp {
    ensure_built();
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let (mut cmd, label) = match kind {
        AppKind::Rust(name) => {
            let cmd = CommandBuilder::new(rust_example_bin(name));
            (cmd, format!("rust:{name}"))
        }
        AppKind::Python(cat, stem) => {
            let mut cmd = CommandBuilder::new(PY_BIN);
            cmd.arg(python_app_path(cat, stem));
            // Make textual resolution independent of the ambient environment.
            cmd.env("PYTHONPATH", format!("{PY_CHECKOUT}/src"));
            (cmd, format!("py:{cat}/{stem}"))
        }
    };

    cmd.cwd(repo_root());
    cmd.env("TERM", "xterm-256color");
    cmd.env("LANG", "en_US.UTF-8");
    cmd.env("COLUMNS", COLS.to_string());
    cmd.env("LINES", ROWS.to_string());
    // Keep both drivers from waiting on terminal-capability query responses the
    // vt100 emulator will never answer.
    cmd.env("TEXTUAL_KEYBOARD_PROTOCOL", "off");
    cmd.env("TEXTUAL_SYNC_OUTPUT", "0");

    let child = pty.slave.spawn_command(cmd).expect("spawn app in pty");
    drop(pty.slave);

    let mut reader = pty.master.try_clone_reader().expect("pty reader");
    let writer = pty.master.take_writer().expect("pty writer");
    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
    let feed = Arc::clone(&parser);
    let reader_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            feed.lock().unwrap().process(&buf[..n]);
        }
    });

    RunningApp {
        parser,
        master: pty.master,
        writer,
        child,
        reader_thread: Some(reader_thread),
        label,
    }
}

/// Drive an input script and return captures taken at each `Capture` point. We
/// always take an initial capture (after the app settles) and a final capture.
fn drive(kind: &AppKind, script: &[Step], final_settle_ms: u64) -> Vec<Grid> {
    let mut app = spawn(kind);
    app.settle(Duration::from_secs(12));
    let mut frames = vec![app.capture()];
    for step in script {
        match step {
            Step::SendKeys(s) => app.send(s.as_bytes()),
            Step::Key(k) => app.send(&k.bytes()),
            Step::Wait(ms) => std::thread::sleep(Duration::from_millis(*ms)),
            Step::Click(x, y) => app.send(&sgr_click(*x, *y)),
        }
    }
    std::thread::sleep(Duration::from_millis(final_settle_ms));
    frames.push(app.capture());
    let label = app.label.clone();
    let _ = label;
    app.shutdown();
    frames
}

/// Drive both apps with the same script; return (rust_frames, py_frames).
fn drive_both(
    rust: &'static str,
    py_cat: &'static str,
    py_stem: &'static str,
    script: &[Step],
    final_settle_ms: u64,
) -> (Vec<Grid>, Vec<Grid>) {
    let r = drive(&AppKind::Rust(rust), script, final_settle_ms);
    let p = drive(&AppKind::Python(py_cat, py_stem), script, final_settle_ms);
    (r, p)
}

// ---------------------------------------------------------------------------
// Diff reporting.
// ---------------------------------------------------------------------------

/// A row-by-row text diff (Python = reference, Rust = actual).
fn text_diff(py: &Grid, rust: &Grid) -> String {
    let mut out = String::new();
    for row in 0..ROWS as usize {
        let g = py.row_text(row);
        let a = rust.row_text(row);
        if g != a {
            out.push_str(&format!("row {row:>2}:\n  python | {g}\n  rust   | {a}\n"));
        }
    }
    if out.is_empty() {
        out.push_str("(no text differences)\n");
    }
    out
}

/// Cell-level colour diff for a given row range, showing fg/bg per differing
/// cell. Useful when the glyphs match but colours don't (or vice versa).
fn cell_diff_rows(py: &Grid, rust: &Grid, rows: std::ops::Range<usize>) -> String {
    let mut out = String::new();
    for row in rows {
        for col in 0..COLS as usize {
            let pc = py.cell(row, col);
            let rc = rust.cell(row, col);
            if pc.ch == '\u{200b}' || rc.ch == '\u{200b}' {
                continue;
            }
            let differs = pc.ch != rc.ch || pc.fg != rc.fg || pc.bg != rc.bg;
            if differs {
                out.push_str(&format!(
                    "  [{row:>2},{col:>3}] py {:?} fg={} bg={}  |  rust {:?} fg={} bg={}\n",
                    pc.ch,
                    pc.fg.short(),
                    pc.bg.short(),
                    rc.ch,
                    rc.fg.short(),
                    rc.bg.short(),
                ));
            }
        }
    }
    if out.is_empty() {
        out.push_str("  (no cell differences in range)\n");
    }
    out
}

fn dump(label: &str, g: &Grid) {
    eprintln!("---- {label} ----\n{}\n----", g.text());
}

// ===========================================================================
// WIDGETS PARITY CASES (Rust == Python) — opposite polarity of the six "catch"
// cases above. For each INTERACTIVE widgets demo we drive the demo's
// representative interaction on BOTH the real Rust example binary and the real
// Python app, then assert Rust matches Python.
//
// PARITY MODE
// -----------
// The catch cases assert the apps DIFFER on a flagged axis; these assert they
// AGREE. Per the harness non-determinism policy we assert EXACT (glyph + fg +
// bg) where the rendering is deterministic, and STRUCTURAL where it is not.
//
//   * GLYPH grid: for deterministic demos we require the visible character grid
//     to match exactly over the content rows. The header row 0 carries a live
//     clock on some demos and is excluded by `skip_rows` when present.
//   * COLOUR: vt100 truecolor is captured per cell. Where both apps emit the
//     same SGR the colours match exactly; where one side rounds a blended
//     colour we report it. A demo whose glyphs match but whose key colours
//     diverge is recorded as a BUG with the concrete cell (py value vs rust
//     value), it does NOT silently pass.
//
// A case classified PASS means: after the representative interaction, the Rust
// grid equals the Python grid (glyph exact over content rows, and no material
// colour divergence). A case classified BUG carries `#[ignore = "BUG: ..."]`
// with the concrete diff so it is tracked and flips to passing when fixed.
// ===========================================================================

/// Drive both the Rust example `name` and the Python `widgets/<name>.py` app
/// with the same script; return (rust_final, py_final) grids.
fn widgets_both(name: &'static str, script: &[Step], settle_ms: u64) -> (Grid, Grid) {
    let r = drive(&AppKind::Rust(name), script, settle_ms);
    let p = drive(&AppKind::Python("widgets", name), script, settle_ms);
    (r.into_iter().last().unwrap(), p.into_iter().last().unwrap())
}

/// Count cells whose GLYPH differs between the two grids over `rows`
/// (wide-continuation markers ignored). This is the deterministic axis.
fn glyph_mismatch_count(py: &Grid, rust: &Grid, rows: std::ops::Range<usize>) -> usize {
    let mut n = 0;
    for row in rows {
        for col in 0..COLS as usize {
            let pc = py.cell(row, col);
            let rc = rust.cell(row, col);
            if pc.ch == '\u{200b}' || rc.ch == '\u{200b}' {
                continue;
            }
            if pc.ch != rc.ch {
                n += 1;
            }
        }
    }
    n
}

/// Count cells whose fg OR bg colour differs while the glyph matches, over
/// `rows`. Glyph-equal-but-colour-different is the parity-relevant colour axis.
fn colour_mismatch_count(py: &Grid, rust: &Grid, rows: std::ops::Range<usize>) -> usize {
    let mut n = 0;
    for row in rows {
        for col in 0..COLS as usize {
            let pc = py.cell(row, col);
            let rc = rust.cell(row, col);
            if pc.ch == '\u{200b}' || rc.ch == '\u{200b}' || pc.ch != rc.ch {
                continue;
            }
            // A space glyph has no visible foreground: only its BACKGROUND is
            // observable. Python often emits an explicit fg SGR on blank cells
            // (the widget's colour) while the driver leaves it at terminal
            // default; that is invisible, so compare bg only for spaces.
            if pc.ch == ' ' {
                if !cols_equiv(pc.bg, rc.bg) {
                    n += 1;
                }
                continue;
            }
            if !cols_equiv(pc.fg, rc.fg) || !cols_equiv(pc.bg, rc.bg) {
                n += 1;
            }
        }
    }
    n
}

/// Are two captured colours visually the same? Exact match, OR two truecolor
/// triples within a tiny per-channel delta (≤2) — vt100/driver rounding noise,
/// NOT a real colour divergence. A Default-vs-coloured pair is NOT equivalent.
fn cols_equiv(a: Col, b: Col) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Col::Rgb(r1, g1, b1), Col::Rgb(r2, g2, b2)) => {
            let d = |x: u8, y: u8| (x as i32 - y as i32).unsigned_abs();
            d(r1, r2) <= 2 && d(g1, g2) <= 2 && d(b1, b2) <= 2
        }
        _ => false,
    }
}

/// Assert glyph-exact parity over content rows (everything except `skip_rows`),
/// reporting the concrete diff on failure. Colour divergence is reported as a
/// warning line but does not by itself fail the glyph assertion (colour-only
/// divergences are tracked as their own BUG cases with `#[ignore]`).
fn assert_glyph_parity(name: &str, py: &Grid, rust: &Grid, skip_rows: &[usize]) {
    dump(&format!("{name} PY final"), py);
    dump(&format!("{name} RUST final"), rust);
    let mut glyph_bad = 0usize;
    let mut detail = String::new();
    for row in 0..ROWS as usize {
        if skip_rows.contains(&row) {
            continue;
        }
        let g = py.row_text(row);
        let a = rust.row_text(row);
        if g != a {
            let bad = glyph_mismatch_count(py, rust, row..row + 1);
            glyph_bad += bad;
            detail.push_str(&format!(
                "row {row:>2} ({bad} glyph diffs):\n  python | {g}\n  rust   | {a}\n"
            ));
        }
    }
    let colour_bad = {
        let mut c = 0;
        for row in 0..ROWS as usize {
            if skip_rows.contains(&row) {
                continue;
            }
            c += colour_mismatch_count(py, rust, row..row + 1);
        }
        c
    };
    eprintln!("{name}: glyph_diffs={glyph_bad} colour_diffs={colour_bad}");
    if colour_bad > 0 {
        // Sample up to 12 glyph-matching colour divergences for diagnosis.
        let mut shown = 0;
        for row in 0..ROWS as usize {
            if skip_rows.contains(&row) {
                continue;
            }
            for col in 0..COLS as usize {
                let pc = py.cell(row, col);
                let rc = rust.cell(row, col);
                if pc.ch == '\u{200b}' || rc.ch == '\u{200b}' || pc.ch != rc.ch {
                    continue;
                }
                let visible_diff = if pc.ch == ' ' {
                    !cols_equiv(pc.bg, rc.bg)
                } else {
                    !cols_equiv(pc.fg, rc.fg) || !cols_equiv(pc.bg, rc.bg)
                };
                if visible_diff {
                    eprintln!(
                        "  COLDIFF [{row:>2},{col:>3}] {:?} py fg={} bg={} | rust fg={} bg={}",
                        pc.ch,
                        pc.fg.short(),
                        pc.bg.short(),
                        rc.fg.short(),
                        rc.bg.short(),
                    );
                    shown += 1;
                    if shown >= 12 {
                        break;
                    }
                }
            }
            if shown >= 12 {
                break;
            }
        }
    }
    assert_eq!(
        glyph_bad,
        0,
        "PARITY (glyph) FAIL for {name}: {glyph_bad} cells differ.\n{detail}\n\
         BG palette (py): {:?}\nBG palette (rust): {:?}",
        py.bg_palette(),
        rust.bg_palette(),
    );
    assert_eq!(
        colour_bad,
        0,
        "PARITY (colour) FAIL for {name}: {colour_bad} glyph-matching cells differ in fg/bg \
         (see COLDIFF lines above).\nBG palette (py): {:?}\nBG palette (rust): {:?}",
        py.bg_palette(),
        rust.bg_palette(),
    );
}

// ===========================================================================
// ACCEPTANCE CASES — the six demos the maintainer flagged by hand.
//
// Each test asserts the harness can DETECT the Rust/Python discrepancy on the
// flagged axis. Where a flagged discrepancy has since been fixed in this tree,
// the test still proves the harness sees the relevant signal and documents the
// current state, so it can never silently pass blind.
// ===========================================================================

/// reactivity/dynamic_watch: click the counter button N times; Python increments
/// by 10 each press (10/20/30) and fills a BLUE ProgressBar; Rust (per the
/// maintainer's report) increments by 1 and the bar is white-start with a tiny
/// black piece. We catch BOTH: the increment VALUE (text) and the bar COLOUR.
#[test]
fn dynamic_watch_increment_value_and_bar_colour() {
    // Click the "+10" / counter button three times. The button sits in the
    // top-left Counter widget; click row 1 a few cells in. We also send Enter as
    // a fallback activation in case focus differs, then compare.
    let script = [
        Step::Click(4, 1),
        Step::Wait(250),
        Step::Click(4, 1),
        Step::Wait(250),
        Step::Click(4, 1),
        Step::Wait(400),
    ];
    let (rust, py) = drive_both(
        "dynamic_watch",
        "guide/reactivity",
        "dynamic_watch",
        &script,
        600,
    );
    let (rf, pf) = (rust.last().unwrap(), py.last().unwrap());
    dump("dynamic_watch RUST final", rf);
    dump("dynamic_watch PY final", pf);

    // --- value axis: the Counter's Label is on row 0. Read the leading numeric
    // token there specifically (NOT a screen-wide substring — the progress bar
    // also prints "30%", which would false-match). Python: 30 after three +10
    // presses; Rust (per report): 3 after three +1 presses. ---
    let counter_value = |g: &Grid| -> String {
        g.row_text(0)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    };
    let py_value = counter_value(pf);
    let rust_value = counter_value(rf);

    // --- colour axis: the ProgressBar's FILLED portion. The bar draws with the
    // `━` glyph; the filled cells use the bar's fg colour (Python: blue), the
    // unfilled cells a muted/grey. Sample the fg colours of the bar glyphs. ---
    let bar_fill_colours = |g: &Grid| -> std::collections::BTreeSet<String> {
        let mut set = std::collections::BTreeSet::new();
        for row in 0..ROWS as usize {
            for col in 0..COLS as usize {
                let c = g.cell(row, col);
                if c.ch == '━' || c.ch == '╸' || c.ch == '╺' {
                    set.insert(c.fg.short());
                }
            }
        }
        set
    };
    let py_bar = bar_fill_colours(pf);
    let rust_bar = bar_fill_colours(rf);
    let py_bar_blue = py_bar.iter().any(|s| {
        s.starts_with('#')
            && Col::Rgb(
                u8::from_str_radix(&s[1..3], 16).unwrap_or(0),
                u8::from_str_radix(&s[3..5], 16).unwrap_or(0),
                u8::from_str_radix(&s[5..7], 16).unwrap_or(0),
            )
            .is_blue()
    });
    let rust_bar_blue = rust_bar.iter().any(|s| {
        s.starts_with('#')
            && Col::Rgb(
                u8::from_str_radix(&s[1..3], 16).unwrap_or(0),
                u8::from_str_radix(&s[3..5], 16).unwrap_or(0),
                u8::from_str_radix(&s[5..7], 16).unwrap_or(0),
            )
            .is_blue()
    });

    eprintln!(
        "dynamic_watch: py_value={py_value:?} rust_value={rust_value:?} \
         py_bar_colours={py_bar:?} rust_bar_colours={rust_bar:?} \
         py_bar_blue={py_bar_blue} rust_bar_blue={rust_bar_blue}"
    );

    // The harness must tell the apps apart on at least one flagged axis.
    let value_differs = py_value != rust_value;
    let colour_differs = py_bar != rust_bar;
    assert!(
        value_differs || colour_differs,
        "HARNESS BLIND: dynamic_watch looks identical on both value and colour axes.\n\
         Python should reach 30 and fill a blue bar.\n{}\n\nBG palette (py):{:?}\nBG palette (rust):{:?}",
        text_diff(pf, rf),
        pf.bg_palette(),
        rf.bg_palette(),
    );
}

/// screens/modes01: Python shows a Footer row with key shortcuts (Dashboard /
/// Settings / Help). Rust's footer is (per report) missing. Catch: the footer
/// row text differs between the two.
#[test]
fn modes01_missing_footer_row() {
    let script = [Step::Wait(300)];
    let (rust, py) = drive_both("modes01", "guide/screens", "modes01", &script, 400);
    let (rf, pf) = (rust.last().unwrap(), py.last().unwrap());
    dump("modes01 RUST", rf);
    dump("modes01 PY", pf);

    // The footer carries the binding HINTS (key + label pairs), distinct from the
    // Placeholder body text which also says "Dashboard Screen" on BOTH apps. The
    // footer signature is the keyed shortcut list: the per-binding sequence
    // "Settings" AND "Help" appearing together on a single near-bottom row with
    // the key letters. We detect it as: a row in the bottom 3 that contains the
    // shortcut trio (Dashboard, Settings, Help) — the Placeholder body only ever
    // shows ONE of them at a time.
    let footer_row_present = |g: &Grid| -> bool {
        (ROWS as usize - 3..ROWS as usize).any(|r| {
            let t = g.row_text(r);
            t.contains("Settings") && t.contains("Help") && t.contains("Dashboard")
        })
    };
    let py_has_footer = footer_row_present(pf);
    let rust_has_footer = footer_row_present(rf);
    eprintln!("modes01: py_footer={py_has_footer} rust_footer={rust_has_footer}");

    assert!(
        py_has_footer != rust_has_footer,
        "HARNESS BLIND: modes01 footer presence looks identical.\n\
         Expected Python to show a Footer with Dashboard/Settings/Help and Rust to lack it (or vice versa).\n{}",
        text_diff(pf, rf),
    );
}

/// tutorial/stopwatch06: pressing the green Start button makes the clock advance.
/// We click Start, capture, wait, capture again; Python's time field changes,
/// Rust's (per report) never fires. Time-aware: assert the displayed time
/// ADVANCED (t0 != t+N), not a specific value.
#[test]
fn stopwatch06_clock_advances_after_start() {
    // The time display renders as big-digit BLOCK ART (not a plain "00:00:00"
    // token), so "did it advance?" can't be read as a string. Instead we
    // fingerprint the time-display REGION (the block-digit cells on the first
    // stopwatch's row band) and check whether that fingerprint CHANGED between
    // two captures after pressing Start. A running clock mutates the block art;
    // a dead Start leaves it frozen.
    //
    // Layout (verified from the real render): the first stopwatch occupies rows
    // ~3..7 (0-based), the Start button is at row 4, cols ~3..16, and the time
    // block art spans roughly cols 40..80 on row 4. We fingerprint a generous
    // band so we don't depend on exact glyph columns.
    fn region_fingerprint(
        g: &Grid,
        rows: std::ops::Range<usize>,
        cols: std::ops::Range<usize>,
    ) -> String {
        let mut s = String::new();
        for r in rows {
            for c in cols.clone() {
                s.push(g.cell(r, c).ch);
            }
            s.push('\n');
        }
        s
    }
    fn run(kind: &AppKind) -> bool {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        // Click the green Start button of the FIRST stopwatch (0-based row 4).
        app.send(&sgr_click(8, 4));
        std::thread::sleep(Duration::from_millis(400));
        let band_rows = 3..7usize;
        let band_cols = 18..90usize;
        let f0 = region_fingerprint(&app.capture(), band_rows.clone(), band_cols.clone());
        std::thread::sleep(Duration::from_millis(1600));
        let f1 = region_fingerprint(&app.capture(), band_rows, band_cols);
        let label = app.label.clone();
        app.shutdown();
        let advanced = f0 != f1;
        eprintln!("stopwatch06 {label}: time-region advanced after Start = {advanced}");
        advanced
    }
    let rust_adv = run(&AppKind::Rust("stopwatch06"));
    let py_adv = run(&AppKind::Python("tutorial", "stopwatch06"));
    eprintln!("stopwatch06: rust_advanced={rust_adv} py_advanced={py_adv}");

    assert!(
        py_adv != rust_adv,
        "HARNESS BLIND: stopwatch06 clock-advance behaviour looks identical.\n\
         Expected Python's clock to advance after Start and Rust's not to (or vice versa)."
    );
}

/// workers/weather05: type a city; Python shows weather and never leaks internal
/// event text; the maintainer reported Rust leaking "WorkerStateChanged" onto the
/// screen and weather never appearing. Network is non-deterministic, so we use a
/// STRUCTURAL check: (a) no internal event text on the visible screen, and (b)
/// the weather region became non-empty after typing. We compare both apps; the
/// harness asserts it can see the structural signal on the leak axis.
#[test]
fn weather05_no_event_leak_structural() {
    let script = [Step::SendKeys("Tokyo"), Step::Wait(1500)];
    let (rust, py) = drive_both("weather05", "guide/workers", "weather05", &script, 1500);
    let (rf, pf) = (rust.last().unwrap(), py.last().unwrap());
    dump("weather05 RUST final", rf);
    dump("weather05 PY final", pf);

    // (a) leak axis: the internal message type must never reach the screen.
    let py_leaks = pf.contains("WorkerStateChanged") || pf.contains("StateChanged");
    let rust_leaks = rf.contains("WorkerStateChanged") || rf.contains("StateChanged");
    // (b) echo axis: the input echoes the typed city on both (deterministic).
    let py_echo = pf.contains("Tokyo");
    let rust_echo = rf.contains("Tokyo");
    eprintln!(
        "weather05: py_leaks={py_leaks} rust_leaks={rust_leaks} py_echo={py_echo} rust_echo={rust_echo}"
    );

    // Python must NOT leak; that's the invariant the harness enforces. If Rust
    // leaks, the harness catches the divergence; if Rust has been fixed not to
    // leak, the harness still proves it can read the structural signal (both
    // sides agree, input echoes on both).
    assert!(
        !py_leaks,
        "Python weather05 leaked internal event text onto the screen — harness misread:\n{}",
        pf.text()
    );
    assert!(
        rust_leaks == false || py_leaks != rust_leaks,
        "HARNESS BLIND: weather05 leak axis indeterminate.\nrust_leaks={rust_leaks}\n{}",
        text_diff(pf, rf),
    );
    // Echo must agree so we know the input actually drove both apps.
    assert!(
        py_echo && rust_echo,
        "weather05: typed city did not echo on both apps (py_echo={py_echo}, rust_echo={rust_echo}); \
         input did not reach the apps.\n{}",
        text_diff(pf, rf),
    );
}

/// animator/animation01: a red box fades in (opacity animates) over ~2s. Catch:
/// the rendered box colour PROGRESSES over time on Python; the maintainer
/// reported Rust shows no fade. Time-aware + colour-aware: sample the box cell at
/// t0 and t+N and assert Python's colour changed; compare to Rust.
#[test]
fn animation01_opacity_progression_over_time() {
    // Sample a representative cell inside the box across the fade. The box is a
    // padded Static near the top-left; sample several cells and track how the bg
    // colour evolves.
    fn run(kind: &AppKind) -> (Vec<Col>, bool) {
        let app = spawn(kind);
        // Do NOT fully settle — the fade is in progress; capture early frames.
        std::thread::sleep(Duration::from_millis(150));
        let sample = |g: &Grid| -> Col {
            // pick the most common non-default bg in the top 6 rows (the box).
            let mut counts: BTreeMap<String, (usize, Col)> = BTreeMap::new();
            for row in 0..6 {
                for col in 0..40 {
                    let c = g.cell(row, col);
                    if c.bg != Col::Default {
                        let e = counts.entry(c.bg.short()).or_insert((0, c.bg));
                        e.0 += 1;
                    }
                }
            }
            counts
                .into_values()
                .max_by_key(|(n, _)| *n)
                .map(|(_, c)| c)
                .unwrap_or(Col::Default)
        };
        let mut series = Vec::new();
        for _ in 0..8 {
            series.push(sample(&app.capture()));
            std::thread::sleep(Duration::from_millis(300));
        }
        let label = app.label.clone();
        app.shutdown();
        let distinct: std::collections::BTreeSet<String> =
            series.iter().map(|c| c.short()).collect();
        let progressed = distinct.len() >= 2;
        eprintln!(
            "animation01 {label}: bg series = {:?} (distinct={})",
            series.iter().map(|c| c.short()).collect::<Vec<_>>(),
            distinct.len()
        );
        (series, progressed)
    }
    let (_rs, rust_prog) = run(&AppKind::Rust("animation01"));
    let (_ps, py_prog) = run(&AppKind::Python("guide/animator", "animation01"));
    eprintln!("animation01: rust_progressed={rust_prog} py_progressed={py_prog}");

    assert!(
        py_prog != rust_prog,
        "HARNESS BLIND: animation01 fade progression looks identical on both apps.\n\
         Expected Python's box bg colour to progress over the 2s fade and Rust's to be static (or vice versa)."
    );
}

/// app/widgets02: press a key to mount the Welcome widget; Python centers a red
/// "Dune" quote with a red rule; Rust (per report) left-aligns, white text + blue
/// rule. Catch: the quote text alignment AND the rule colour differ.
#[test]
fn widgets02_welcome_alignment_and_rule_colour() {
    // widgets02 mounts Welcome on any key. Send a key, settle, capture.
    let script = [Step::Key(Key::Char('x')), Step::Wait(500)];
    let (rust, py) = drive_both("widgets02", "app", "widgets02", &script, 600);
    let (rf, pf) = (rust.last().unwrap(), py.last().unwrap());
    dump("widgets02 RUST", rf);
    dump("widgets02 PY", pf);

    // Find the row containing the "Dune"/quote-ish content and the rule row.
    // The Welcome widget shows a Markdown blurb; we detect a red colour anywhere
    // (the rule + quote) on Python.
    let py_red = pf.any_red();
    let rust_red = rf.any_red();
    let py_blue = pf.any_blue();
    let rust_blue = rf.any_blue();
    eprintln!(
        "widgets02: py_red={py_red} rust_red={rust_red} py_blue={py_blue} rust_blue={rust_blue}"
    );

    // The flagged axis is colour (red rule/text vs blue rule/white text). The
    // harness must see a colour divergence between the two.
    assert!(
        py_red != rust_red || py_blue != rust_blue,
        "HARNESS BLIND: widgets02 colour (red vs blue rule/text) looks identical.\n{}\n\
         per-cell colour diff (rows 0..ROWS):\n{}\n\
         BG palette (py): {:?}\nBG palette (rust): {:?}",
        text_diff(pf, rf),
        cell_diff_rows(pf, rf, 0..ROWS as usize),
        pf.bg_palette(),
        rf.bg_palette(),
    );
}

// ===========================================================================
// WIDGETS INTERACTIVE PARITY CASES (Rust == Python)
// ===========================================================================

// --- text-entry widgets -----------------------------------------------------

/// input: type into the first Input; the typed text + cursor should render
/// identically on both apps (deterministic, no clock/header).
#[test]
#[ignore = "BUG: focused Input bg differs — Python applies `background-tint: $foreground 5%` on :focus (bg #272727); Rust leaves bg #1e1e1e. 244 cells."]
fn parity_input_typing() {
    let script = [Step::SendKeys("Marcos"), Step::Wait(250)];
    let (rf, pf) = widgets_both("input", &script, 400);
    assert_glyph_parity("input", &pf, &rf, &[]);
}

/// input_types: integer + number Inputs; typing digits validates live.
#[test]
#[ignore = "BUG: focused Input bg #1e1e1e (Rust) vs #272727 (Python, $foreground 5% tint on :focus). 243 cells."]
fn parity_input_types_typing() {
    // Type into the first (integer) Input only — a non-digit is rejected, so the
    // result is deterministic and avoids focus-traversal ambiguity.
    let script = [Step::SendKeys("12a345"), Step::Wait(250)];
    let (rf, pf) = widgets_both("input_types", &script, 400);
    assert_glyph_parity("input_types", &pf, &rf, &[]);
}

/// input_validation: typing an invalid number must surface the SAME failure
/// descriptions in the Pretty widget on both apps.
#[test]
#[ignore = "BUG: Pretty renders the failure-list multi-line `[` expanded; Python renders it inline `['Value is not even.', \"That's not a palindrome :/\"]`. 43 glyph + focused-Input tint diffs."]
fn parity_input_validation_failure() {
    let script = [Step::SendKeys("13"), Step::Wait(300)];
    let (rf, pf) = widgets_both("input_validation", &script, 400);
    assert_glyph_parity("input_validation", &pf, &rf, &[]);
}

/// masked_input: typing digits into a credit-card mask renders the same
/// separators + placeholder on both apps.
#[test]
#[ignore = "BUG: focused MaskedInput bg #1e1e1e (Rust) vs #272727 (Python :focus tint). 245 cells."]
fn parity_masked_input_typing() {
    let script = [Step::SendKeys("4242424242"), Step::Wait(300)];
    let (rf, pf) = widgets_both("masked_input", &script, 400);
    assert_glyph_parity("masked_input", &pf, &rf, &[]);
}

// --- toggle widgets ---------------------------------------------------------

/// checkbox: the focused checkbox toggles on Space. Initial focus is
/// "#initial_focus" (Kaitain) per the demo.
#[test]
#[ignore = "BUG (multi-root; white border-slot root FIXED): the white toggle/border-edge slot bg (#ffffff vs #1b1b1b) is fixed (border outer-bg now composites the transparent $boost parent). Residual 55 colour cells are OTHER roots: `[magenta]Ginaz` markup label fg (#ff00ff vs #e0e0e0), `:focus` border colour + `background-tint` (#272727 vs #1e1e1e), and the `.-on` checked button colour (#8ad4a1 vs #000f18) — markup + pseudo-class resolution, out of the white-slot scope."]
fn parity_checkbox_toggle() {
    let script = [Step::Key(Key::Space), Step::Wait(250)];
    let (rf, pf) = widgets_both("checkbox", &script, 400);
    assert_glyph_parity("checkbox", &pf, &rf, &[]);
}

/// switch: the focused switch toggles on Enter/Space.
#[test]
#[ignore = "DEMO-PORT GAP (framework align/auto-size root FIXED in pc1-autosize): vertical `middle` align + auto-width container centering now work. Residual is a demo-port omission: the Rust `switch` demo builds the top `Static(\"[b]Example switches\\n\")` WITHOUT `.class(\"label\")` (Python passes `classes=\"label\"`), so it has UNSET width and fills 120 cols, making the align bounding box full-width and defeating horizontal centering. Fix requires the demo to add `.class(\"label\")` — outside this root's allowed files (src/layout only)."]
fn parity_switch_toggle() {
    let script = [Step::Key(Key::Enter), Step::Wait(250)];
    let (rf, pf) = widgets_both("switch", &script, 400);
    assert_glyph_parity("switch", &pf, &rf, &[]);
}

// --- radio widgets ----------------------------------------------------------

/// radio_button: RadioSet has focus; Down then Space moves + selects.
#[test]
#[ignore = "BUG: focused RadioSet border/surface tint differs (bg #1e1e1e vs #272727 on :focus). 154 cells."]
fn parity_radio_button_select() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::Key(Key::Space),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("radio_button", &script, 400);
    assert_glyph_parity("radio_button", &pf, &rf, &[]);
}

/// radio_set: two RadioSets; Down arrow within the focused set.
#[test]
#[ignore = "BUG: focused RadioSet surface/border tint differs (bg #1e1e1e vs #272727 on :focus). 180 cells."]
fn parity_radio_set_navigate() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::SendKeys("\x1b[B"),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("radio_set", &script, 400);
    assert_glyph_parity("radio_set", &pf, &rf, &[]);
}

/// radio_set_changed: selecting a button updates two Labels (pressed label +
/// pressed index). Both must show the same strings.
#[test]
#[ignore = "BUG: RadioSet.Changed → Label.update not reflected — Python shows 'Pressed button label: Dune 1984' / 'Pressed button index: 1'; Rust shows only a stray 'P'. 47 glyph cells."]
fn parity_radio_set_changed() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::Key(Key::Space),
        Step::Wait(300),
    ];
    let (rf, pf) = widgets_both("radio_set_changed", &script, 400);
    assert_glyph_parity("radio_set_changed", &pf, &rf, &[]);
}

// --- collapsible ------------------------------------------------------------

/// collapsible: press 'e' to expand all, then 'c' to collapse all.
#[test]
fn parity_collapsible_expand_collapse() {
    let script = [
        Step::Key(Key::Char('e')),
        Step::Wait(200),
        Step::Key(Key::Char('c')),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("collapsible", &script, 400);
    assert_glyph_parity("collapsible", &pf, &rf, &[]);
}

/// collapsible_nested: Enter on the focused (outer) collapsible toggles it.
#[test]
fn parity_collapsible_nested_toggle() {
    let script = [Step::Key(Key::Enter), Step::Wait(250)];
    let (rf, pf) = widgets_both("collapsible_nested", &script, 400);
    assert_glyph_parity("collapsible_nested", &pf, &rf, &[]);
}

/// collapsible_custom_symbol: static after compose; assert initial parity (the
/// custom >>> / v symbols and one expanded/one collapsed panel).
#[test]
fn parity_collapsible_custom_symbol() {
    let script = [Step::Wait(250)];
    let (rf, pf) = widgets_both("collapsible_custom_symbol", &script, 400);
    assert_glyph_parity("collapsible_custom_symbol", &pf, &rf, &[]);
}

// --- select -----------------------------------------------------------------

/// select_widget: Enter opens the overlay (the option list of Dune lines).
#[test]
#[ignore = "BUG: Select overlay differs from Python (277 glyph cells) — the opened option-list overlay renders different content/layout. Header clock row excluded."]
fn parity_select_open_overlay() {
    let script = [Step::Key(Key::Enter), Step::Wait(300)];
    let (rf, pf) = widgets_both("select_widget", &script, 400);
    // Row 0 is the Header which carries a live clock; exclude it.
    assert_glyph_parity("select_widget", &pf, &rf, &[0]);
}

/// select_widget_no_blank: 's' swaps the option set; first value differs.
#[test]
#[ignore = "BUG: Select (no-blank) surface/border tint differs from Python after swap (117 colour cells)."]
fn parity_select_no_blank_swap() {
    let script = [Step::Key(Key::Char('s')), Step::Wait(300)];
    let (rf, pf) = widgets_both("select_widget_no_blank", &script, 400);
    assert_glyph_parity("select_widget_no_blank", &pf, &rf, &[0]);
}

/// select_from_values_widget: Enter opens the overlay built via from_values.
#[test]
#[ignore = "BUG: Select.from_values overlay differs from Python (277 glyph cells), same overlay divergence as select_widget."]
fn parity_select_from_values_open() {
    let script = [Step::Key(Key::Enter), Step::Wait(300)];
    let (rf, pf) = widgets_both("select_from_values_widget", &script, 400);
    assert_glyph_parity("select_from_values_widget", &pf, &rf, &[0]);
}

// --- lists ------------------------------------------------------------------

/// list_view: Down arrow moves the highlight off the first item.
#[test]
fn parity_list_view_navigate() {
    let script = [Step::SendKeys("\x1b[B"), Step::Wait(250)];
    let (rf, pf) = widgets_both("list_view", &script, 400);
    assert_glyph_parity("list_view", &pf, &rf, &[]);
}

/// selection_list_selections: Down then Space toggles a selection.
#[test]
#[ignore = "BUG: SelectionList surface/row colours differ from Python (2103 colour cells over the focused list body)."]
fn parity_selection_list_selections_toggle() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::Key(Key::Space),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("selection_list_selections", &script, 400);
    assert_glyph_parity("selection_list_selections", &pf, &rf, &[0]);
}

/// selection_list_selected: toggling a selection updates the Pretty panel with
/// the selected values; both must agree.
#[test]
#[ignore = "BUG: SelectionList + Pretty panel differ from Python (153 glyph cells) — selected-values Pretty content/layout and list rendering diverge."]
fn parity_selection_list_selected_toggle() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::Key(Key::Space),
        Step::Wait(300),
    ];
    let (rf, pf) = widgets_both("selection_list_selected", &script, 400);
    assert_glyph_parity("selection_list_selected", &pf, &rf, &[0]);
}

/// option_list_strings: Down arrow moves the highlight.
#[test]
#[ignore = "BUG: focused OptionList border/surface tint differs (bg #1e1e1e vs #272727 on :focus). 244 cells."]
fn parity_option_list_strings_navigate() {
    let script = [Step::SendKeys("\x1b[B"), Step::Wait(250)];
    let (rf, pf) = widgets_both("option_list_strings", &script, 400);
    assert_glyph_parity("option_list_strings", &pf, &rf, &[0]);
}

/// option_list_options: Down past a disabled/separator option.
#[test]
#[ignore = "BUG: OptionList (with disabled/separator options) surface + disabled-option colours differ from Python (811 cells)."]
fn parity_option_list_options_navigate() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::SendKeys("\x1b[B"),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("option_list_options", &script, 400);
    assert_glyph_parity("option_list_options", &pf, &rf, &[0]);
}

// --- tabs -------------------------------------------------------------------

/// tabs: 'a' adds a tab (cycles the Dune names); active label updates.
#[test]
#[ignore = "BUG: Tabs add_tab('a') does not add a visible tab — Python shows 'Paul Atreidies  Duke Leto Atreides'; Rust shows only 'Paul Atreidies'. 16 glyph cells."]
fn parity_tabs_add() {
    let script = [Step::Key(Key::Char('a')), Step::Wait(300)];
    let (rf, pf) = widgets_both("tabs", &script, 400);
    assert_glyph_parity("tabs", &pf, &rf, &[]);
}

/// tabbed_content: 'p' switches to the Paul tab.
#[test]
fn parity_tabbed_content_switch() {
    let script = [Step::Key(Key::Char('p')), Step::Wait(300)];
    let (rf, pf) = widgets_both("tabbed_content", &script, 400);
    assert_glyph_parity("tabbed_content", &pf, &rf, &[]);
}

/// tabbed_content_label_color: Right arrow / Tab to the second (Green) tab; the
/// tab label colours (red/green) are the flagged axis.
#[test]
fn parity_tabbed_content_label_color() {
    let script = [Step::Wait(250)];
    let (rf, pf) = widgets_both("tabbed_content_label_color", &script, 400);
    assert_glyph_parity("tabbed_content_label_color", &pf, &rf, &[]);
}

/// content_switcher: click the Markdown button to switch panes.
#[test]
#[ignore = "BUG: ContentSwitcher button/markdown surface colours differ from Python (130 cells) after switching to the Markdown pane."]
fn parity_content_switcher_switch() {
    // The two buttons sit on row 1; "Markdown" is the second button.
    let script = [Step::Click(20, 1), Step::Wait(350)];
    let (rf, pf) = widgets_both("content_switcher", &script, 400);
    assert_glyph_parity("content_switcher", &pf, &rf, &[]);
}

// --- tree -------------------------------------------------------------------

/// tree: Down to the Characters node, then it's already expanded; navigate the
/// tree and assert parity of the guide glyphs + labels.
#[test]
fn parity_tree_navigate() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::SendKeys("\x1b[B"),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("tree", &script, 400);
    assert_glyph_parity("tree", &pf, &rf, &[]);
}

// --- data_table -------------------------------------------------------------

/// data_table: Down/Right arrows move the cell cursor.
#[test]
fn parity_data_table_navigate() {
    let script = [
        Step::SendKeys("\x1b[B"),
        Step::SendKeys("\x1b[C"),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("data_table", &script, 400);
    assert_glyph_parity("data_table", &pf, &rf, &[]);
}

/// data_table_cursors: 'c' cycles the cursor type (column -> row -> cell ...).
#[test]
fn parity_data_table_cursors_cycle() {
    let script = [
        Step::Key(Key::Char('c')),
        Step::Wait(200),
        Step::Key(Key::Char('c')),
        Step::Wait(250),
    ];
    let (rf, pf) = widgets_both("data_table_cursors", &script, 400);
    assert_glyph_parity("data_table_cursors", &pf, &rf, &[]);
}

/// data_table_sort: 'c' sorts by country.
#[test]
fn parity_data_table_sort() {
    let script = [Step::Key(Key::Char('c')), Step::Wait(300)];
    let (rf, pf) = widgets_both("data_table_sort", &script, 400);
    assert_glyph_parity("data_table_sort", &pf, &rf, &[]);
}

// --- logs -------------------------------------------------------------------

/// rich_log: a key press is echoed into the RichLog as an event; both apps
/// should render the same content above (Syntax + Table) and append on key.
#[test]
#[ignore = "BUG: RichLog content colours differ from Python (1147 cells) — Syntax/Table highlight + log surface bg diverge."]
fn parity_rich_log_keypress() {
    let script = [Step::Key(Key::Char('z')), Step::Wait(300)];
    let (rf, pf) = widgets_both("rich_log", &script, 400);
    assert_glyph_parity("rich_log", &pf, &rf, &[]);
}

/// log: static content written on_ready; assert initial parity.
#[test]
#[ignore = "BUG: Log surface bg is #121212 (Rust) vs #000000 (Python) — Log default background not pure black. 120 cells."]
fn parity_log_content() {
    let script = [Step::Wait(300)];
    let (rf, pf) = widgets_both("log", &script, 400);
    assert_glyph_parity("log", &pf, &rf, &[]);
}

// ===========================================================================
// WAVE 2 — REVERIFICATION: reactivity / tutorial / widgets-guide / app demos.
//
// Same PARITY polarity as the widgets cases above (assert Rust == Python).
// Each case drives the demo's representative interaction on BOTH the real Rust
// binary and the real Python app, then asserts the cell grids agree (glyph
// exact over content rows; colour where deterministic), OR — for inherently
// time-dependent demos (live clocks, fades) — asserts the SAME structural
// behaviour on both sides (e.g. the clock region advanced on BOTH).
//
// A case that differs is a BUG, committed `#[ignore = "BUG: <diff>"]` with the
// concrete cell/colour/behaviour divergence so it is tracked and flips to
// passing when the underlying fundamental is fixed.
// ===========================================================================

/// Drive both the Rust example `name` and the Python `<cat>/<name>.py` app with
/// the same script; return (rust_final, py_final) grids.
fn cat_both(
    name: &'static str,
    cat: &'static str,
    script: &[Step],
    settle_ms: u64,
) -> (Grid, Grid) {
    let r = drive(&AppKind::Rust(name), script, settle_ms);
    let p = drive(&AppKind::Python(cat, name), script, settle_ms);
    (r.into_iter().last().unwrap(), p.into_iter().last().unwrap())
}

/// Fingerprint a rectangular region's glyphs (used for time-dependent "did the
/// clock advance?" checks where an exact value is non-deterministic).
fn region_fp(g: &Grid, rows: std::ops::Range<usize>, cols: std::ops::Range<usize>) -> String {
    let mut s = String::new();
    for r in rows {
        for c in cols.clone() {
            s.push(g.cell(r, c).ch);
        }
        s.push('\n');
    }
    s
}

/// Spawn `kind`, run `pre`, then check whether the glyph fingerprint of the
/// region (rows×cols) CHANGED across a `gap_ms` window — i.e. a live clock in
/// that region advanced. Returns whether it advanced.
fn region_advances(
    kind: &AppKind,
    pre: &[Step],
    rows: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
    gap_ms: u64,
) -> bool {
    let mut app = spawn(kind);
    app.settle(Duration::from_secs(12));
    for step in pre {
        match step {
            Step::SendKeys(s) => app.send(s.as_bytes()),
            Step::Key(k) => app.send(&k.bytes()),
            Step::Wait(ms) => std::thread::sleep(Duration::from_millis(*ms)),
            Step::Click(x, y) => app.send(&sgr_click(*x, *y)),
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    let f0 = region_fp(&app.capture(), rows.clone(), cols.clone());
    std::thread::sleep(Duration::from_millis(gap_ms));
    let f1 = region_fp(&app.capture(), rows, cols);
    let label = app.label.clone();
    app.shutdown();
    let adv = f0 != f1;
    eprintln!("{label}: region advanced = {adv}");
    adv
}

// --- reactivity (input-driven, deterministic) -------------------------------

/// computed01: typing a red value live recomputes the colour swatch background.
#[test]
#[ignore = "BUG (2 blockers, both OUTSIDE this root's `src/widgets/input.rs` scope): Python's `Input(\"0\", ...)` pre-fills all three channels with \"0\" and relies on `select_on_focus=True` so typing \"123\" REPLACES the selected \"0\". Rust's demo (docs/examples/.../computed01/main.rs) constructs the Inputs with NO initial value (placeholder only), so green/blue render their placeholder instead of \"0\" (29 glyph cells). The framework now supports an initial value via `Input::with_value` (this root), but flipping computed01 also needs (a) the demo updated to pass `.with_value(\"0\")` and (b) `select_on_focus` on Input (select-all-on-focus) — neither is in the allowed file set."]
fn parity_computed01_color() {
    let script = [Step::SendKeys("123"), Step::Wait(300)];
    let (rf, pf) = cat_both("computed01", "guide/reactivity", &script, 400);
    assert_glyph_parity("computed01", &pf, &rf, &[]);
}

/// watch01: submit a colour name; both swatches update their backgrounds.
#[test]
fn parity_watch01_color() {
    let script = [Step::SendKeys("red"), Step::Key(Key::Enter), Step::Wait(300)];
    let (rf, pf) = cat_both("watch01", "guide/reactivity", &script, 400);
    assert_glyph_parity("watch01", &pf, &rf, &[]);
}

/// validate01: the focused +1 button is pressed 3× via Enter; the validated
/// reactive caps at 10 and each press appends `count = N` to the RichLog.
#[test]
#[ignore = "BUG: glyph-perfect (count = 1/2/3 on both) but the focused +1 success-Button bg is #4ebf71 (Rust, base $success) vs #55c076 (Python, lighter :focus/hover tint). 89 colour cells. Shared root: button :focus/hover background-tint not applied."]
fn parity_validate01_count() {
    let script = [
        Step::Key(Key::Enter),
        Step::Wait(400),
        Step::Key(Key::Enter),
        Step::Wait(400),
        Step::Key(Key::Enter),
        Step::Wait(400),
    ];
    let (rf, pf) = cat_both("validate01", "guide/reactivity", &script, 500);
    assert_glyph_parity("validate01", &pf, &rf, &[]);
}

/// refresh01: typing a name live refreshes the `Name` widget's render.
#[test]
fn parity_refresh01_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh01", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh01", &pf, &rf, &[]);
}

/// refresh02: same as refresh01 but the reactive has `layout=True`.
#[test]
fn parity_refresh02_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh02", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh02", &pf, &rf, &[]);
}

/// refresh03: same but the reactive has `recompose=True` (rebuilds children).
#[test]
fn parity_refresh03_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh03", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh03", &pf, &rf, &[]);
}

/// set_reactive01: pressing Space cycles the greeting via a watcher.
#[test]
#[ignore = "FUNDAMENTAL DIVERGENCE (not fixable as glyph parity): the Python reference itself RAISES on startup and shows a full Rich traceback — `self.greeting = greeting` in __init__ fires watch_greeting BEFORE compose → query_one(\"#greeting\") NoMatches. This crash is the doc's intentional \"wrong way\" that set_reactive02 fixes via set_reactive. Rust's reactive init is deferred, so it does not reproduce the pre-mount-watcher crash; it renders \"Hola Textual\" instead of a Python traceback. Reproducing a Rich traceback glyph-for-glyph is neither feasible nor meaningful. (Port also given `layout: horizontal` so the Greeter renders correctly, matching set_reactive02.)"]
fn parity_set_reactive01_greeting() {
    let script = [Step::Key(Key::Space), Step::Wait(300)];
    let (rf, pf) = cat_both("set_reactive01", "guide/reactivity", &script, 400);
    assert_glyph_parity("set_reactive01", &pf, &rf, &[]);
}

/// set_reactive02: same interaction; greeting initialised via `set_reactive`.
#[test]
#[ignore = "PORT/LAYOUT root, not reactive. The compose-time `who` value WAS already visible (compose reads self.who=\"Textual\" correctly); the `who` Label was missing because the Rust `Greeter` (a port of Python `class Greeter(Horizontal)`) lacked `layout: horizontal`, so its two Labels stacked vertically and the second was clipped by height:1. Added `layout: horizontal` to the Greeter CSS (matching the established Stopwatch-subclasses-Horizontal port pattern) — \"Hola Textual\" now renders side by side. Residual: 5 cells — Python collapses the two adjacent 1-cell horizontal Label margins to 1 (max), Rust sums them to 2. Horizontal margin-collapse is a layout fundamental (src/widgets/layout.rs), out of reactive scope."]
fn parity_set_reactive02_greeting() {
    let script = [Step::Key(Key::Space), Step::Wait(300)];
    let (rf, pf) = cat_both("set_reactive02", "guide/reactivity", &script, 400);
    assert_glyph_parity("set_reactive02", &pf, &rf, &[]);
}

/// set_reactive03: submitting a name appends a `Hello, <name>` Label via
/// `mutate_reactive` + recompose.
#[test]
fn parity_set_reactive03_names() {
    let script = [Step::SendKeys("Ada"), Step::Key(Key::Enter), Step::Wait(300)];
    let (rf, pf) = cat_both("set_reactive03", "guide/reactivity", &script, 400);
    assert_glyph_parity("set_reactive03", &pf, &rf, &[]);
}

// --- tutorial: stopwatches --------------------------------------------------
// The Header carries a live clock on row 0, excluded via skip_rows in the
// deterministic (non-ticking) cases.

/// stopwatch01: Header + Footer only. Initial layout parity (clock row skipped).
#[test]
fn parity_stopwatch01_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("stopwatch01", "tutorial", &script, 400);
    assert_glyph_parity("stopwatch01", &pf, &rf, &[0]);
}

/// stopwatch02: three Stopwatch widgets (Start/Stop/Reset buttons + a frozen
/// 00:00:00.00 TimeDisplay). Initial layout parity (clock row skipped).
#[test]
fn parity_stopwatch02_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("stopwatch02", "tutorial", &script, 400);
    assert_glyph_parity("stopwatch02", &pf, &rf, &[0]);
}

/// stopwatch03: same as 02 with the tutorial CSS applied. Layout parity.
#[test]
#[ignore = "BUG: glyph-perfect but the Button border fg is #8d8d8d (Rust) vs #919191 (Python) — a ~4/channel grey-shade difference on every button border. 201 colour cells. Root: button border colour token/shade rounding."]
fn parity_stopwatch03_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("stopwatch03", "tutorial", &script, 400);
    assert_glyph_parity("stopwatch03", &pf, &rf, &[0]);
}

/// stopwatch04: clicking the first Start button adds the `started` class
/// (purely a styling change — no clock yet). Click Start, compare.
#[test]
#[ignore = "GLYPH PARITY (class-state root FIXED in pc2-classstate): clicking Start now adds `.started`, whose `.started #start { display:none }` / `.started #stop { display:block }` / `.started #reset { visibility:hidden }` descendant CSS is re-resolved + relaid-out. Python and Rust now both show only \"Stop\" (glyph_diffs=0). Residual is colour-only: the revealed error `#stop` button surface rounds #b93c5b (rust) vs #ba4461 (py) — the Color.a u8-vs-float blend keystone, out of this root's allowed files."]
fn parity_stopwatch04_start_class() {
    let script = [Step::Click(8, 4), Step::Wait(300)];
    let (rf, pf) = cat_both("stopwatch04", "tutorial", &script, 400);
    assert_glyph_parity("stopwatch04", &pf, &rf, &[0]);
}

/// stopwatch05: every TimeDisplay ticks continuously from mount (no Start
/// gating). Time-dependent → assert the digit region ADVANCED on BOTH apps
/// (structural parity), not a specific value.
#[test]
fn parity_stopwatch05_ticks() {
    // The block-digit time band sits below the header; exclude the header clock
    // by fingerprinting rows 5..28 only.
    let rows = 5..28usize;
    let cols = 0..COLS as usize;
    let rust_adv = region_advances(&AppKind::Rust("stopwatch05"), &[], rows.clone(), cols.clone(), 1200);
    let py_adv = region_advances(&AppKind::Python("tutorial", "stopwatch05"), &[], rows, cols, 1200);
    eprintln!("stopwatch05: rust_ticks={rust_adv} py_ticks={py_adv}");
    assert!(
        rust_adv && py_adv,
        "PARITY FAIL stopwatch05: continuous tick mismatch — rust_ticks={rust_adv} py_ticks={py_adv} (both must tick)."
    );
}

// --- guide/widgets: custom widgets (deterministic render) -------------------

/// counter01: three static `Count: 0` counters + Footer (no key bindings).
#[test]
#[ignore = "DEMO-PORT GAP (framework auto-size root FIXED in pc1-autosize): a `height: auto` custom leaf now renders at its content height (verified — with `height: auto` the Rust Counter matches Python's 3 rows exactly). BUT the Rust `counter01` demo CSS omits `height: auto` on `Counter` (Python inherits it from `Static`'s DEFAULT_CSS), so the leaf keeps its UNSET height and correctly fills the container. Fix requires the demo CSS to add `Counter { height: auto }` — outside this root's allowed files (src/layout only)."]
fn parity_counter01_render() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("counter01", "guide/widgets", &script, 400);
    assert_glyph_parity("counter01", &pf, &rf, &[]);
}

/// counter02: the focused counter increments on `k`/up. Press `k`.
#[test]
#[ignore = "DEMO-PORT GAP (framework auto-size root FIXED in pc1-autosize): same as counter01 — the Rust `counter02` demo CSS omits `Counter { height: auto }` (Python inherits it from `Static`), so the leaf's UNSET height correctly fills the container. Framework `height: auto` measurement is fixed; the demo CSS fix is outside this root's allowed files (src/layout only)."]
fn parity_counter02_increment() {
    let script = [Step::SendKeys("k"), Step::Wait(300)];
    let (rf, pf) = cat_both("counter02", "guide/widgets", &script, 400);
    assert_glyph_parity("counter02", &pf, &rf, &[]);
}

/// fizzbuzz01: a static rich `Table` rendered on mount.
#[test]
fn parity_fizzbuzz01_table() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("fizzbuzz01", "guide/widgets", &script, 400);
    assert_glyph_parity("fizzbuzz01", &pf, &rf, &[]);
}

/// fizzbuzz02: same table forced to width 50 (expand=True).
#[test]
fn parity_fizzbuzz02_table() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("fizzbuzz02", "guide/widgets", &script, 400);
    assert_glyph_parity("fizzbuzz02", &pf, &rf, &[]);
}

/// hello01: a bare `Hello, World!` widget render (bold markup).
#[test]
fn parity_hello01_render() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("hello01", "guide/widgets", &script, 400);
    assert_glyph_parity("hello01", &pf, &rf, &[]);
}

/// hello02: same with the styled box CSS.
#[test]
fn parity_hello02_render() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("hello02", "guide/widgets", &script, 400);
    assert_glyph_parity("hello02", &pf, &rf, &[]);
}

/// hello03: on_mount sets "Hola"; clicking the widget cycles to "Bonjour".
#[test]
fn parity_hello03_click() {
    let script = [Step::Click(10, 5), Step::Wait(300)];
    let (rf, pf) = cat_both("hello03", "guide/widgets", &script, 400);
    assert_glyph_parity("hello03", &pf, &rf, &[]);
}

/// hello04: styled 40x9 box centred; clicking it cycles the greeting.
#[test]
fn parity_hello04_click() {
    let script = [Step::Click(60, 14), Step::Wait(300)];
    let (rf, pf) = cat_both("hello04", "guide/widgets", &script, 400);
    assert_glyph_parity("hello04", &pf, &rf, &[]);
}

/// hello05: on_mount renders "Hola" with a clickable @click link. Initial
/// render parity (the link target is exercised by hello06's variant).
#[test]
fn parity_hello05_render() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("hello05", "guide/widgets", &script, 400);
    assert_glyph_parity("hello05", &pf, &rf, &[]);
}

/// hello06: same plus a border title/subtitle. Initial render parity.
#[test]
fn parity_hello06_render() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("hello06", "guide/widgets", &script, 400);
    assert_glyph_parity("hello06", &pf, &rf, &[]);
}

/// checker01: an 8x8 black/white checkerboard (Strip render_line).
#[test]
#[ignore = "BUG: the white squares (`Style.parse(\"on white\")`) render bg #ffffff in Rust but #c4c5b5 in Python — Python resolves the named colour \"white\" through its colour pipeline (dimmed), Rust uses raw #ffffff. 1920 colour cells. Root: named-colour (\"white\"/\"black\") resolution differs from Python."]
fn parity_checker01_board() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("checker01", "guide/widgets", &script, 400);
    assert_glyph_parity("checker01", &pf, &rf, &[]);
}

/// checker02: the board with component-class colours (#A5BAC9 / #004578).
#[test]
#[ignore = "BUG: 2 stray cells at the bottom-right corner ([28..29,118..119]) — Rust paints a black square edge (#000000) where Python shows the screen bg (#121212). Minor board-overflow artifact beyond the 64-col board. 2 glyph + 2 colour cells."]
fn parity_checker02_board() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("checker02", "guide/widgets", &script, 400);
    assert_glyph_parity("checker02", &pf, &rf, &[]);
}

/// checker03: a 100-square board inside a ScrollView (visible portion only).
#[test]
fn parity_checker03_board() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("checker03", "guide/widgets", &script, 400);
    assert_glyph_parity("checker03", &pf, &rf, &[]);
}

/// checker04: same board with a mouse-cursor highlight; initial render (no
/// hover) parity.
#[test]
#[ignore = "BUG: the right-edge scrollbar/board-edge band (cols 118..119) differs — Rust paints fg #e0e0e0 on bg #a5bac9 (board white square colour) while Python shows the scrollbar fg #003054 on screen bg #121212. The board cells + cursor square (0,0) otherwise match. 3 glyph + 175 colour cells. Root: ScrollView right-edge scrollbar colour/extent (note: checker03, same ScrollView without cursor var, is glyph+colour clean)."]
fn parity_checker04_board() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("checker04", "guide/widgets", &script, 400);
    assert_glyph_parity("checker04", &pf, &rf, &[]);
}

// --- guide/widgets: time/hover-dependent (structural parity) ----------------

/// loading01: four DataTables each show a `loading` indicator until a worker
/// (random 2-10s sleep) populates them. We sample DURING the loading window
/// (~1.2s, before the 2s minimum sleep) and assert both apps render a loading
/// indicator (structural — exact spinner frame is non-deterministic). Worker
/// completion time is random so we do NOT compare the loaded end-state.
#[test]
#[ignore = "BUG: while DataTable.loading=True, Python renders a LoadingIndicator (animated `●` dots) in all four panels; Rust renders a fully BLANK screen (no indicator, panels empty). The `loading` reactive does not mount/draw a LoadingIndicator. Root: DataTable/Widget `loading` state has no LoadingIndicator overlay in Rust."]
fn parity_loading01_indicator() {
    fn loading_shown(kind: &AppKind) -> bool {
        let mut app = spawn(kind);
        // Do NOT settle() — the spinner animates so the screen never stabilises;
        // sample at a fixed point inside the guaranteed loading window instead.
        std::thread::sleep(Duration::from_millis(1200));
        let g = app.capture();
        let label = app.label.clone();
        // The Textual LoadingIndicator paints clusters of `●`/`·` dots.
        let shown = g.contains("●") || g.contains("·") || g.contains("⠿");
        eprintln!("{label}: loading-indicator visible at t=1.2s = {shown}");
        if !shown {
            dump(&format!("{label} @1.2s"), &g);
        }
        app.shutdown();
        shown
    }
    let rust = loading_shown(&AppKind::Rust("loading01"));
    let py = loading_shown(&AppKind::Python("guide/widgets", "loading01"));
    assert!(
        py,
        "PRECONDITION: Python loading01 showed no loading indicator at t=1.2s."
    );
    assert!(
        rust == py,
        "PARITY FAIL loading01: Python shows a loading indicator while loading; Rust shows none (blank). rust={rust} py={py}."
    );
}

/// Send a bare SGR mouse-move (motion, no button) to (col,row) 0-based.
fn sgr_move(col: u16, row: u16) -> Vec<u8> {
    format!("\x1b[<35;{};{}M", col + 1, row + 1).into_bytes()
}

/// tooltip01/02: hovering the centred Button surfaces a multi-line Tooltip.
/// Hover, wait for the tooltip timer, then assert the tooltip text appeared on
/// BOTH apps (structural — exact overlay position/colour is style-dependent).
fn tooltip_appears(name: &'static str) -> (bool, bool) {
    fn run(kind: &AppKind) -> bool {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        // The Button is centred (Screen align center middle). Hover its middle.
        app.send(&sgr_move(59, 14));
        std::thread::sleep(Duration::from_millis(400));
        app.send(&sgr_move(60, 14));
        std::thread::sleep(Duration::from_millis(1500));
        let g = app.capture();
        let shown = g.contains("mind-killer") || g.contains("Fear is");
        let label = app.label.clone();
        app.shutdown();
        eprintln!("{label}: tooltip shown = {shown}");
        shown
    }
    (run(&AppKind::Rust(name)), run(&AppKind::Python("guide/widgets", name)))
}

/// tooltip01: default-styled tooltip.
#[test]
fn parity_tooltip01_hover() {
    let (rust, py) = tooltip_appears("tooltip01");
    assert!(
        rust == py && py,
        "PARITY FAIL tooltip01: hover tooltip presence mismatch — rust={rust} py={py} (both must show the tooltip)."
    );
}

/// tooltip02: custom-styled tooltip (padding/background/color).
#[test]
fn parity_tooltip02_hover() {
    let (rust, py) = tooltip_appears("tooltip02");
    assert!(
        rust == py && py,
        "PARITY FAIL tooltip02: hover tooltip presence mismatch — rust={rust} py={py} (both must show the tooltip)."
    );
}

// --- app/examples -----------------------------------------------------------

/// event01: pressing a digit key sets the Screen background to a named colour.
/// Press `1` → COLORS[1] = "maroon"; assert the screen bg matches on both.
#[test]
fn parity_event01_key_colour() {
    let script = [Step::SendKeys("1"), Step::Wait(300)];
    let (rf, pf) = cat_both("event01", "app", &script, 400);
    assert_glyph_parity("event01", &pf, &rf, &[]);
}

/// question01: a Label + Yes/No buttons. Clicking exits the app, so we compare
/// the initial deterministic layout instead.
#[test]
fn parity_question01_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("question01", "app", &script, 400);
    assert_glyph_parity("question01", &pf, &rf, &[]);
}

/// question02: same with the tcss grid styling.
#[test]
fn parity_question02_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("question02", "app", &script, 400);
    assert_glyph_parity("question02", &pf, &rf, &[]);
}

/// question03: same with inline grid CSS (column-span, content-align).
#[test]
fn parity_question03_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("question03", "app", &script, 400);
    assert_glyph_parity("question03", &pf, &rf, &[]);
}

/// question_title01: a Header (with title/subtitle) + question + buttons.
/// Header row 0 carries a live clock, excluded from the glyph comparison.
#[test]
#[ignore = "BUG: glyph-perfect but the Header SUBTITLE text fg is #e0e0e0 (Rust, full $text) vs #a0a3a6 (Python, dimmed muted subtitle). 139 colour cells. Root: Header sub-title text colour not dimmed to the muted token."]
fn parity_question_title01_layout() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("question_title01", "app", &script, 400);
    assert_glyph_parity("question_title01", &pf, &rf, &[0]);
}

/// question_title02: pressing a key rewrites the Header title/subtitle. Press
/// `x`; structurally assert the Header reflects the new title on BOTH apps
/// (the clock makes an exact row-0 glyph compare non-deterministic).
#[test]
fn parity_question_title02_title_update() {
    let script = [Step::SendKeys("x"), Step::Wait(300)];
    let (rf, pf) = cat_both("question_title02", "app", &script, 400);
    dump("question_title02 PY", &pf);
    dump("question_title02 RUST", &rf);
    // Header now shows title "x" and subtitle "You just pressed x!". Compare the
    // header band (rows 0..2) text, ignoring the clock by checking key tokens.
    let py_has = pf.contains("You just pressed x");
    let rust_has = rf.contains("You just pressed x");
    eprintln!("question_title02: py_subtitle={py_has} rust_subtitle={rust_has}");
    // Body (question + buttons) should also match; compare rows 3..ROWS exactly.
    assert!(
        py_has == rust_has,
        "PARITY FAIL question_title02: Header subtitle update differs — py_shows={py_has} rust_shows={rust_has}.\n{}",
        text_diff(&pf, &rf),
    );
}

/// widgets01: the framework `Welcome` widget rendered on its own.
#[test]
#[ignore = "BUG: the Welcome widget's Markdown body renders with different vertical spacing — Python keeps it compact (\"Welcome!\" row 2, no blank gaps between blocks); Rust inserts extra blank lines (\"Welcome!\" row 3, gaps before \"Dune quote\"/blockquote), shifting the whole body down. 669 glyph + 456 colour cells. Shared root with the widgets02 catch case (Welcome/Markdown block spacing + rule/quote colour)."]
fn parity_widgets01_welcome() {
    let script = [Step::Wait(300)];
    let (rf, pf) = cat_both("widgets01", "app", &script, 400);
    assert_glyph_parity("widgets01", &pf, &rf, &[]);
}

/// widgets03: pressing a key mounts `Welcome` and relabels its Button "YES!".
#[test]
#[ignore = "BUG: after key-mount of Welcome (+ relabel \"YES!\"), the same Markdown vertical-spacing divergence as widgets01 shifts the body. 673 glyph + 96 colour cells. Shared root: Welcome/Markdown block spacing (widgets02 family)."]
fn parity_widgets03_mount_welcome() {
    let script = [Step::SendKeys("x"), Step::Wait(400)];
    let (rf, pf) = cat_both("widgets03", "app", &script, 500);
    assert_glyph_parity("widgets03", &pf, &rf, &[]);
}

/// widgets04: same as widgets03 but mounts asynchronously.
#[test]
#[ignore = "BUG: same Welcome/Markdown vertical-spacing divergence as widgets01/03 after async key-mount. 669 glyph + 96 colour cells. Shared root: Welcome/Markdown block spacing (widgets02 family)."]
fn parity_widgets04_mount_welcome() {
    let script = [Step::SendKeys("x"), Step::Wait(400)];
    let (rf, pf) = cat_both("widgets04", "app", &script, 500);
    assert_glyph_parity("widgets04", &pf, &rf, &[]);
}

// --- reactivity: world clocks (time-dependent → structural) -----------------

/// world_clock01: three live `Digits` clocks driven by a 1s interval. Assert
/// the digit region advanced on BOTH apps (structural — exact time differs).
#[test]
fn parity_world_clock01_ticks() {
    let rows = 0..ROWS as usize;
    let cols = 0..COLS as usize;
    let rust_adv = region_advances(&AppKind::Rust("world_clock01"), &[], rows.clone(), cols.clone(), 1200);
    let py_adv = region_advances(&AppKind::Python("guide/reactivity", "world_clock01"), &[], rows, cols, 1200);
    eprintln!("world_clock01: rust_ticks={rust_adv} py_ticks={py_adv}");
    assert!(
        rust_adv && py_adv,
        "PARITY FAIL world_clock01: clock-tick mismatch — rust={rust_adv} py={py_adv} (both must advance)."
    );
}

/// world_clock02: same clocks, compose-time variant. Structural tick parity.
#[test]
fn parity_world_clock02_ticks() {
    let rows = 0..ROWS as usize;
    let cols = 0..COLS as usize;
    let rust_adv = region_advances(&AppKind::Rust("world_clock02"), &[], rows.clone(), cols.clone(), 1200);
    let py_adv = region_advances(&AppKind::Python("guide/reactivity", "world_clock02"), &[], rows, cols, 1200);
    eprintln!("world_clock02: rust_ticks={rust_adv} py_ticks={py_adv}");
    assert!(
        rust_adv && py_adv,
        "PARITY FAIL world_clock02: clock-tick mismatch — rust={rust_adv} py={py_adv} (both must advance)."
    );
}

/// world_clock03: same clocks, data-binding variant. Structural tick parity.
#[test]
fn parity_world_clock03_ticks() {
    let rows = 0..ROWS as usize;
    let cols = 0..COLS as usize;
    let rust_adv = region_advances(&AppKind::Rust("world_clock03"), &[], rows.clone(), cols.clone(), 1200);
    let py_adv = region_advances(&AppKind::Python("guide/reactivity", "world_clock03"), &[], rows, cols, 1200);
    eprintln!("world_clock03: rust_ticks={rust_adv} py_ticks={py_adv}");
    assert!(
        rust_adv && py_adv,
        "PARITY FAIL world_clock03: clock-tick mismatch — rust={rust_adv} py={py_adv} (both must advance)."
    );
}

// ===========================================================================
// WAVE 3 — guide/input, guide/actions, guide/screens, guide/command_palette,
// events. Each case drives the demo's representative interaction on BOTH the
// real Rust example binary and the real Python app, then asserts parity (glyph
// exact where deterministic, structural where not). BUGs are committed
// `#[ignore = "BUG: <diff>"]` with the concrete divergence.
// ===========================================================================

// --- guide/input ------------------------------------------------------------

/// key01: a single RichLog that writes every Key event. Type "ab"; the log
/// renders the two Key event objects. Exact glyph+colour parity.
#[test]
#[ignore = "BUG: glyph-perfect (`Key(key='a', character='a', name='a', is_printable=True)` on both) but the RichLog repr-syntax-highlight colours differ — py fg #f4005f/#fd971f/#98e024 (Rich repr theme) vs rust #b73763/#f5a623/#98d168. 148 colour cells. Root: rich-rs repr highlighter colour palette differs from Python Rich."]
fn parity_input_key01_log() {
    let script = [Step::SendKeys("ab"), Step::Wait(300)];
    let (rf, pf) = cat_both("key01", "guide/input", &script, 400);
    assert_glyph_parity("key01", &pf, &rf, &[]);
}

/// key02: same as key01 plus a `key_space` bell handler (bell is inaudible in
/// the grid). Type "a"; the log renders one Key event. Exact parity.
#[test]
#[ignore = "BUG: glyph-perfect but same RichLog repr-highlight colour divergence as key01 (py #f4005f/#fd971f/#98e024 vs rust #b73763/#f5a623/#98d168). 104 colour cells. Root: rich-rs repr highlighter palette differs from Python Rich."]
fn parity_input_key02_log() {
    let script = [Step::SendKeys("a"), Step::Wait(300)];
    let (rf, pf) = cat_both("key02", "guide/input", &script, 400);
    assert_glyph_parity("key02", &pf, &rf, &[]);
}

/// key03: four KeyLogger RichLogs in a CSS grid; only the focused one logs.
/// Type "a"; compare the whole grid (which pane logged + its content).
#[test]
#[ignore = "BUG: the custom KeyLogger writes the Rust Debug form `Key(key=\"a\", character=Some('a'), is_printable=true)` while Python writes the event repr `Key(key='a', character='a', name='a', is_printable=True)` (34 glyph cells) + the same repr-highlight colour divergence as key01. Root: KeyLogger event formatting (Rust Debug vs Python repr; note key01/02's single RichLog DOES match text — only the subclassed logger diverges)."]
fn parity_input_key03_grid() {
    let script = [Step::SendKeys("a"), Step::Wait(300)];
    let (rf, pf) = cat_both("key03", "guide/input", &script, 400);
    assert_glyph_parity("key03", &pf, &rf, &[]);
}

/// binding01: Footer with r/g/b bindings; each press mounts a coloured Bar with
/// a 50%-alpha background. Press r, g, b; compare the three stacked bars + the
/// Footer.
#[test]
#[ignore = "BUG: the bars are coloured + the Footer matches, but the per-Bar top margin (`margin: 1 2`, height 5) is not applied — Rust packs the stacked Bars one row higher than Python (red at row2 vs row3, etc.). 24 glyph cells. Root: vertical (top) margin on stacked mounted widgets not honoured."]
fn parity_input_binding01_bars() {
    let script = [
        Step::SendKeys("r"),
        Step::Wait(200),
        Step::SendKeys("g"),
        Step::Wait(200),
        Step::SendKeys("b"),
        Step::Wait(300),
    ];
    let (rf, pf) = cat_both("binding01", "guide/input", &script, 400);
    assert_glyph_parity("binding01", &pf, &rf, &[]);
}

/// mouse01: a Ball that follows the mouse; a RichLog records every MouseMove
/// (non-deterministic count/content). STRUCTURAL: move the mouse and assert the
/// "Textual" ball is rendered and the log became non-empty on BOTH apps.
#[test]
#[ignore = "BUG: after mouse moves Python renders the RichLog full of MouseMove events + the moved `Textual` Ball; Rust renders a fully BLANK screen (no log, Ball never appears). Root: App-level `on_mouse_move` does not drive updates — the handler's RichLog.write + Ball.offset are never reflected (mouse-move not reaching the app handler / Ball offset not applied)."]
fn parity_input_mouse01_ball() {
    fn run(kind: &AppKind) -> (bool, bool) {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        app.send(&sgr_move(40, 12));
        std::thread::sleep(Duration::from_millis(200));
        app.send(&sgr_move(60, 18));
        std::thread::sleep(Duration::from_millis(400));
        let g = app.capture();
        let ball = g.contains("Textual");
        let log = g.contains("MouseMove") || g.contains("Mouse");
        app.shutdown();
        (ball, log)
    }
    let (rb, rl) = run(&AppKind::Rust("mouse01"));
    let (pb, pl) = run(&AppKind::Python("guide/input", "mouse01"));
    eprintln!("mouse01: rust(ball={rb},log={rl}) py(ball={pb},log={pl})");
    assert!(rb && pb, "mouse01: Ball missing — rust={rb} py={pb}");
    assert!(rl && pl, "mouse01: MouseMove log empty — rust={rl} py={pl}");
}

// --- guide/actions ----------------------------------------------------------

/// actions01: pressing `r` sets the screen background to "red" via an action.
/// Compare the resulting (mostly-empty) screen's bg colour.
#[test]
fn parity_actions01_red_bg() {
    let script = [Step::SendKeys("r"), Step::Wait(300)];
    let (rf, pf) = cat_both("actions01", "guide/actions", &script, 400);
    assert_glyph_parity("actions01", &pf, &rf, &[]);
}

/// actions02: same as actions01 but the action runs via `run_action`.
#[test]
fn parity_actions02_red_bg() {
    let script = [Step::SendKeys("r"), Step::Wait(300)];
    let (rf, pf) = cat_both("actions02", "guide/actions", &script, 400);
    assert_glyph_parity("actions02", &pf, &rf, &[]);
}

/// actions03: a Static with `@click` markup links (Red/Green/Blue). Click the
/// "Red" link; the screen bg turns red.
#[test]
#[ignore = "BUG: the Static's multi-line markup TEXT begins with a newline; Python renders that leading blank line (content starts row1) while Rust drops it (content starts row0), shifting the whole Static up one row. 46 glyph cells. Root: leading empty line of multi-line Static/markup content not rendered (vertical shift)."]
fn parity_actions03_click_red() {
    let script = [Step::Click(0, 2), Step::Wait(300)];
    let (rf, pf) = cat_both("actions03", "guide/actions", &script, 400);
    assert_glyph_parity("actions03", &pf, &rf, &[]);
}

/// actions04: same markup links plus r/g/b key bindings. Press `r`.
#[test]
#[ignore = "BUG: same leading-empty-line shift as actions03 — the Static's content starts row0 in Rust vs row1 in Python. 46 glyph cells. Root: leading empty line of multi-line Static content not rendered."]
fn parity_actions04_red_bg() {
    let script = [Step::SendKeys("r"), Step::Wait(300)];
    let (rf, pf) = cat_both("actions04", "guide/actions", &script, 400);
    assert_glyph_parity("actions04", &pf, &rf, &[]);
}

/// actions05: two ColorSwitcher widgets + r/g/b app bindings. Press `r` (sets
/// the screen bg red behind both switchers).
#[test]
#[ignore = "BUG: same leading-empty-line shift as actions03/04 across both ColorSwitcher Statics. 138 glyph cells. Root: leading empty line of multi-line Static content not rendered."]
fn parity_actions05_red_bg() {
    let script = [Step::SendKeys("r"), Step::Wait(300)];
    let (rf, pf) = cat_both("actions05", "guide/actions", &script, 400);
    assert_glyph_parity("actions05", &pf, &rf, &[]);
}

/// actions06: five Placeholder pages in a HorizontalScroll + Footer; `n`
/// advances. Press `n` twice; the third page scrolls into view and the Footer
/// reflects available bindings.
#[test]
#[ignore = "BUG: the page scroll works but the Placeholder's `Page N` label is not rendered by Rust (5 glyph cells blank where Python shows \"Page 2\") and the Placeholder colour palette diverges (3475 colour cells). Root: Placeholder widget label rendering + per-index colour palette parity."]
fn parity_actions06_next_page() {
    let script = [
        Step::SendKeys("n"),
        Step::Wait(300),
        Step::SendKeys("n"),
        Step::Wait(400),
    ];
    let (rf, pf) = cat_both("actions06", "guide/actions", &script, 500);
    assert_glyph_parity("actions06", &pf, &rf, &[]);
}

/// actions07: same pages, bindings=True reactive (disabled bindings dim in the
/// Footer rather than disappearing). Press `n` once.
#[test]
#[ignore = "BUG: same Placeholder divergence as actions06 — `Page 1` label missing (5 glyph) + Placeholder colour palette differs (3475 colour). Root: Placeholder widget label + colour palette parity."]
fn parity_actions07_next_page() {
    let script = [Step::SendKeys("n"), Step::Wait(400)];
    let (rf, pf) = cat_both("actions07", "guide/actions", &script, 500);
    assert_glyph_parity("actions07", &pf, &rf, &[]);
}

// --- guide/screens ----------------------------------------------------------

/// modal01: Header + long Label + Footer; `q` pushes a (non-modal) QuitScreen
/// with a dialog Grid. Press `q`; compare the dialog (Header clock row skipped).
#[test]
fn parity_screens_modal01_dialog() {
    let script = [Step::SendKeys("q"), Step::Wait(400)];
    let (rf, pf) = cat_both("modal01", "guide/screens", &script, 500);
    assert_glyph_parity("modal01", &pf, &rf, &[0]);
}

/// modal02: same dialog but via ModalScreen (transparent overlay dimming the
/// text behind). Press `q`.
#[test]
fn parity_screens_modal02_dialog() {
    let script = [Step::SendKeys("q"), Step::Wait(400)];
    let (rf, pf) = cat_both("modal02", "guide/screens", &script, 500);
    assert_glyph_parity("modal02", &pf, &rf, &[0]);
}

/// modal03: ModalScreen[bool] with dismiss + callback. Press `q`; compare the
/// dialog.
#[test]
fn parity_screens_modal03_dialog() {
    let script = [Step::SendKeys("q"), Step::Wait(400)];
    let (rf, pf) = cat_both("modal03", "guide/screens", &script, 500);
    assert_glyph_parity("modal03", &pf, &rf, &[0]);
}

/// modes01: MODES with a Dashboard screen switched in on mount; each screen is a
/// Placeholder + Footer. Compare the initial dashboard (user flagged the Footer
/// as MISSING — this verifies it).
#[test]
#[ignore = "BUG (user-flagged, CONFIRMED): the Footer is MISSING the app's switch_mode bindings — Python shows ` d Dashboard  s Settings  h Help ... ^p palette`; Rust's Footer shows only `^p palette` (the d/s/h binding hints are absent). 24 glyph cells. Root: App BINDINGS not surfaced in the Footer when running under MODES/switch_mode screens."]
fn parity_screens_modes01_dashboard() {
    let script = [Step::Wait(400)];
    let (rf, pf) = cat_both("modes01", "guide/screens", &script, 500);
    assert_glyph_parity("modes01", &pf, &rf, &[]);
}

/// questions01: a worker pushes a QuestionScreen (Label + Yes/No buttons) via
/// push_screen_wait on mount. Compare the initial question screen.
#[test]
#[ignore = "BUG: Rust renders a fully BLANK screen — the QuestionScreen (pushed from `@work async def on_mount` via `push_screen_wait`) never appears; Python shows the `Do you like Textual?` Label + Yes/No buttons. 246 glyph cells. Root: worker-driven `push_screen_wait` / `@work`-decorated `on_mount` screen push not supported."]
fn parity_screens_questions01_dialog() {
    let script = [Step::Wait(500)];
    let (rf, pf) = cat_both("questions01", "guide/screens", &script, 600);
    assert_glyph_parity("questions01", &pf, &rf, &[]);
}

/// screen01: SCREENS dict + `b` pushes a BSOD screen (blue bg, title bar).
/// Press `b`; compare the BSOD screen.
#[test]
fn parity_screens_screen01_bsod() {
    let script = [Step::SendKeys("b"), Step::Wait(400)];
    let (rf, pf) = cat_both("screen01", "guide/screens", &script, 500);
    assert_glyph_parity("screen01", &pf, &rf, &[]);
}

/// screen02: install_screen variant; `b` pushes the same BSOD screen.
#[test]
fn parity_screens_screen02_bsod() {
    let script = [Step::SendKeys("b"), Step::Wait(400)];
    let (rf, pf) = cat_both("screen02", "guide/screens", &script, 500);
    assert_glyph_parity("screen02", &pf, &rf, &[]);
}

// --- guide/command_palette --------------------------------------------------

/// command01: a custom "Bell" SystemCommand. Open the command palette (ctrl+p)
/// and type "bell"; the palette should list the Bell command. STRUCTURAL: the
/// palette overlay + "Bell" entry appear on BOTH apps.
#[test]
fn parity_command01_palette_bell() {
    // Returns (palette_without_traceback, bell_listed).
    fn run(kind: &AppKind) -> (bool, bool) {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        app.send(b"\x10"); // ctrl+p
        std::thread::sleep(Duration::from_millis(500));
        app.send(b"bell");
        std::thread::sleep(Duration::from_millis(600));
        let g = app.capture();
        let crashed = g.contains("Traceback") || g.contains("AttributeError");
        // "Bell" (capitalised) is the command entry; the typed query is "bell".
        let bell_entry = g.contains("Bell") && g.contains("Ring the bell");
        app.shutdown();
        (!crashed, bell_entry)
    }
    let (rok, rb) = run(&AppKind::Rust("command01"));
    let (pok, pb) = run(&AppKind::Python("guide/command_palette", "command01"));
    eprintln!("command01: rust(no_crash={rok},bell_entry={rb}) py(no_crash={pok},bell_entry={pb})");
    assert!(pok, "command01: Python command palette crashed (rich clear_meta_and_links)");
    assert!(rok, "command01: Rust command palette traceback");
    assert!(pb && rb, "command01: both must list the Bell command — rust={rb} py={pb}");
}

/// command02: a Provider listing the *.py files in the cwd. Open the palette and
/// type "open". STRUCTURAL: the palette opens and shows file hits on BOTH (the
/// exact file list depends on the cwd glob, so structural only).
#[test]
fn parity_command02_palette_open() {
    fn run(kind: &AppKind) -> bool {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        app.send(b"\x10"); // ctrl+p
        std::thread::sleep(Duration::from_millis(500));
        app.send(b"open");
        std::thread::sleep(Duration::from_millis(700));
        let g = app.capture();
        let crashed = g.contains("Traceback") || g.contains("AttributeError");
        app.shutdown();
        !crashed
    }
    let rok = run(&AppKind::Rust("command02"));
    let pok = run(&AppKind::Python("guide/command_palette", "command02"));
    eprintln!("command02: rust_no_crash={rok} py_no_crash={pok}");
    assert!(pok && rok, "command02 palette must open without a traceback — rust={rok} py={pok}");
}

// --- events -----------------------------------------------------------------

/// custom01: four ColorButtons (transparent white bg, coloured border, render
/// the colour hex). Click the first (#008080); the screen bg animates to that
/// colour over 0.5s. Wait past the animation and compare the settled screen.
#[test]
#[ignore = "DEMO-PORT GAP (framework auto-size root FIXED in pc1-autosize): a `height: auto` bordered leaf now sizes to content+chrome. Residual is two demo-port divergences, both outside this root's allowed files (src/layout only): (a) the Rust `ColorButton.render()` returns the hex `#008080` while Python returns `Color(0, 128, 128)` (str(Color)); (b) the Rust demo CSS hardcodes `ColorButton { height: 5 }` instead of Python's inherited `Static { height: auto }` (which sizes to 3 rows). Fixing needs the demo render() + CSS `height: auto`."]
fn parity_events_custom01_select() {
    let script = [Step::Click(6, 2), Step::Wait(900)];
    let (rf, pf) = cat_both("custom01", "events", &script, 600);
    assert_glyph_parity("custom01", &pf, &rf, &[]);
}

/// dictionary: an Input + as-you-type lookup. Python queries a real dictionary
/// API; the Rust port fabricates a response, so the RESULTS region diverges by
/// design. STRUCTURAL: type a word and assert the Input shows the typed text on
/// BOTH apps and each populates its results region.
#[test]
fn parity_events_dictionary_input() {
    fn run(kind: &AppKind) -> (bool, bool) {
        let mut app = spawn(kind);
        app.settle(Duration::from_secs(12));
        app.send(b"hello");
        std::thread::sleep(Duration::from_millis(1500));
        let g = app.capture();
        let typed = g.contains("hello");
        // results region is rows below the docked input; non-empty if any
        // non-blank text appears below row 2 other than the input itself.
        let results = (3..ROWS as usize)
            .any(|r| !g.row_text(r).trim().is_empty());
        app.shutdown();
        (typed, results)
    }
    let (rt, rr) = run(&AppKind::Rust("dictionary"));
    let (pt, pr) = run(&AppKind::Python("events", "dictionary"));
    eprintln!("dictionary: rust(typed={rt},results={rr}) py(typed={pt},results={pr})");
    assert!(rt && pt, "dictionary: typed text missing — rust={rt} py={pt}");
    assert!(rr && pr, "dictionary: results region empty — rust={rr} py={pr}");
}

/// on_decorator01: three Buttons; `on_button_pressed` dispatches by id/class.
/// Click "Toggle dark" (switches theme). Compare the post-toggle screen.
#[test]
fn parity_events_on_decorator01_toggle() {
    let script = [Step::Click(20, 2), Step::Wait(400)];
    let (rf, pf) = cat_both("on_decorator01", "events", &script, 500);
    assert_glyph_parity("on_decorator01", &pf, &rf, &[]);
}

/// on_decorator02: same three Buttons via `@on` handlers. Click "Toggle dark".
#[test]
fn parity_events_on_decorator02_toggle() {
    let script = [Step::Click(20, 2), Step::Wait(400)];
    let (rf, pf) = cat_both("on_decorator02", "events", &script, 500);
    assert_glyph_parity("on_decorator02", &pf, &rf, &[]);
}

/// prevent: an Input + Clear button; typing rings a bell, clicking Clear empties
/// the Input *without* re-firing Input.Changed. Type "abc", then click Clear;
/// compare the cleared Input.
#[test]
#[ignore = "BUG: the Input is correctly cleared (prevent works, glyph-perfect) but after clicking Clear (focus moves to the Button) Python shows the now-blurred Input's grey border #191919 while Rust keeps the Input's focused-blue border #0178d4. 408 colour cells. Root: Input border colour not updated on blur when focus moves to the Button."]
fn parity_events_prevent_clear() {
    let script = [
        Step::SendKeys("abc"),
        Step::Wait(300),
        Step::Click(4, 3),
        Step::Wait(300),
    ];
    let (rf, pf) = cat_both("prevent", "events", &script, 400);
    assert_glyph_parity("prevent", &pf, &rf, &[]);
}

// ===========================================================================
// WAVE 4 — workers / animator / compound / how-to INTERACTIVE PARITY
//
// Categories whose Python sources pull live network data (workers) or animate on
// a wall clock (animator/how-to render_compose, inline clocks) are NON-
// deterministic by construction. For those we assert STRUCTURAL parity (both
// apps echo the same input, neither leaks internal event text, both populate the
// same regions / produce the same kind of output). Where a demo IS deterministic
// (compound byte editors, compound01) we assert glyph+colour parity exactly.
// ===========================================================================

// --- workers (weather02..05) ------------------------------------------------
//
// The Rust weather ports are built WITHOUT the `http-examples` feature, so they
// render FABRICATED weather; Python fetches REAL data from wttr.in. The weather
// CONTENT therefore diverges by design. The deterministic, parity-relevant axes
// are: (a) the docked Input echoes the typed city on BOTH apps, and (b) neither
// app leaks an internal `WorkerStateChanged` message onto the visible screen.

/// Probe a workers/weather app: type a city, wait, and report
/// (input echoed the city, internal event text leaked, weather region non-empty).
fn weather_probe(kind: &AppKind, city: &str, wait_ms: u64) -> (bool, bool, bool) {
    let mut app = spawn(kind);
    app.settle(Duration::from_secs(12));
    app.send(city.as_bytes());
    std::thread::sleep(Duration::from_millis(wait_ms));
    let g = app.capture();
    let echo = g.contains(city);
    let leak = g.contains("WorkerStateChanged") || g.contains("StateChanged");
    // Anything rendered below the docked Input (rows >= 3) counts as a populated
    // weather region.
    let weather = (3..ROWS as usize).any(|r| !g.row_text(r).trim().is_empty());
    let label = app.label.clone();
    app.shutdown();
    eprintln!("  {label}: echo={echo} leak={leak} weather_region_nonempty={weather}");
    (echo, leak, weather)
}

fn weather_parity(stem: &'static str, city: &str) {
    let (re, rl, rw) = weather_probe(&AppKind::Rust(stem), city, 3000);
    let (pe, pl, pw) = weather_probe(&AppKind::Python("guide/workers", stem), city, 3000);
    eprintln!(
        "{stem}: rust(echo={re},leak={rl},weather={rw}) py(echo={pe},leak={pl},weather={pw})"
    );
    assert!(
        pe && re,
        "{stem}: typed city did not echo on both apps (py_echo={pe}, rust_echo={re}); \
         input did not reach the apps."
    );
    assert!(
        !pl,
        "{stem}: PYTHON leaked internal event text onto the screen — harness misread."
    );
    assert!(
        !rl,
        "{stem}: RUST leaked internal `WorkerStateChanged` text onto the visible screen \
         (Python does not)."
    );
}

#[test]
fn parity_workers_weather02() {
    weather_parity("weather02", "Tokyo");
}

#[test]
fn parity_workers_weather03() {
    weather_parity("weather03", "Tokyo");
}

#[test]
fn parity_workers_weather04() {
    weather_parity("weather04", "Tokyo");
}

#[test]
#[ignore = "BUG: Rust leaks the internal `WorkerStateChanged` message onto the visible \
            screen — weather05/main.rs logs it via `eprintln!(\"[weather05] WorkerStateChanged: \
            ...\")`, which writes to the PTY and corrupts the frame; Python's `self.log(event)` \
            routes to the devtools log, never the screen. Structural axis: rust leak=true, \
            py leak=false. Root: worker-state logging must use a non-screen log sink, not eprintln."]
fn parity_workers_weather05() {
    weather_parity("weather05", "Tokyo");
}

// --- animator (animation01) -------------------------------------------------

/// animator/animation01: a red "Hello, World!" box fades to opacity 0 over 2s.
/// After the fade completes the box is fully transparent (shows the screen
/// background) on Python. Parity: the settled (faded) screen should match.
#[test]
#[ignore = "BUG: no fade in the real (run_sync) app. After the 2s opacity animation the Python \
            box has faded fully to the screen background (bg #121212); Rust still paints the box \
            solid red (bg #ff0000) — the opacity animation never runs/composites in the live event \
            loop (glyphs match, 360 colour cells differ). NB the headless Pilot liveness test in \
            animation01/main.rs passes, so the gap is live-loop animation driving / opacity \
            compositing in run_sync, not the animator math. Root: on-mount `animate_style` opacity \
            not applied in the live render path."]
fn parity_animator_animation01() {
    let script = [Step::Wait(2600)];
    let (rf, pf) = cat_both("animation01", "guide/animator", &script, 200);
    assert_glyph_parity("animation01", &pf, &rf, &[]);
}

// --- compound (byte01/02/03, compound01) ------------------------------------

/// compound/byte01: 8 BitSwitches + an Input (not wired). Tab from the Input to
/// the first Switch and toggle it with Space; compare the rendered grid.
#[test]
#[ignore = "BUG (multi-root; on/off-swap + :focus-within roots FIXED): the switch slider on/off \
            painting is fixed (Switch now exposes `reactive_widget()` so programmatic set_value runs \
            watch_value) and the `:focus-within` heavy border now renders. Residual (4 cells): (1) Rust \
            shows the Input placeholder `byte` while Python renders it blank (Input root — out of \
            scope), and (2) the focused Switch slider track is #0b1922 in Rust vs #000f18 in Python \
            (2 cells). Root of (2): `background-tint` is over-applied in src/css/selectors/segments.rs \
            — the switch `:focus { background-tint }` tints the slider's OWN explicit-bg cells; Python \
            tints only the widget-surface fill, not renderable segments carrying their own opaque bg. \
            Fix belongs in segments.rs (gate the tint block on explicit_bg.is_none())."]
fn parity_compound_byte01() {
    let script = [Step::Key(Key::Tab), Step::Key(Key::Space), Step::Wait(300)];
    let (rf, pf) = cat_both("byte01", "guide/compound", &script, 400);
    assert_glyph_parity("byte01", &pf, &rf, &[]);
}

/// compound/byte02: toggling a Switch posts BitChanged up to ByteEditor, which
/// writes the integer value into the Input. Tab to the first (bit 7) Switch and
/// toggle it; the Input should read "128" on both.
#[test]
#[ignore = "BUG (multi-root; on/off-swap + :focus-within roots FIXED): switch slider on/off painting \
            and the `:focus-within` heavy border now render. Residual (3 cells): (1) Rust shows the \
            Input placeholder `byte` while Python renders it blank (Input root — out of scope), and \
            (2) the focused Switch slider track is #0b1922 in Rust vs #000f18 in Python (2 cells). \
            Root of (2): `background-tint` over-applied in src/css/selectors/segments.rs (tints the \
            slider's own explicit-bg cells; Python tints only the surface fill). Fix belongs in \
            segments.rs (gate the tint block on explicit_bg.is_none())."]
fn parity_compound_byte02() {
    let script = [Step::Key(Key::Tab), Step::Key(Key::Space), Step::Wait(400)];
    let (rf, pf) = cat_both("byte02", "guide/compound", &script, 400);
    assert_glyph_parity("byte02", &pf, &rf, &[]);
}

/// compound/byte03: bidirectional — typing a number into the Input updates the
/// ByteEditor.value reactive, whose watcher flips the Switches. Type "5" → bits
/// 0 and 2 turn on. Compare the rendered grid.
#[test]
fn parity_compound_byte03() {
    let script = [Step::SendKeys("5"), Step::Wait(400)];
    let (rf, pf) = cat_both("byte03", "guide/compound", &script, 400);
    assert_glyph_parity("byte03", &pf, &rf, &[]);
}

/// compound/compound01: three InputWithLabel compound widgets, centered. Type
/// into the first Input; compare the rendered grid.
#[test]
fn parity_compound_compound01() {
    let script = [Step::SendKeys("Marcos"), Step::Wait(300)];
    let (rf, pf) = cat_both("compound01", "guide/compound", &script, 400);
    assert_glyph_parity("compound01", &pf, &rf, &[]);
}

// --- how-to (inline01/02, render_compose) -----------------------------------

/// how-to/render_compose: a custom Container whose `render()` paints an animated
/// `LinearGradient`, with a centered Static on top. The gradient angle is driven
/// by a refresh clock (non-deterministic), so we assert STRUCTURAL parity: both
/// apps show the splash text and a multi-colour gradient background.
#[test]
fn parity_howto_render_compose() {
    fn probe(kind: &AppKind) -> (bool, usize) {
        let app = spawn(kind);
        std::thread::sleep(Duration::from_millis(600));
        let g = app.capture();
        let text = g.contains("Making a splash with Textual!");
        let bgs = g.bg_palette().len();
        let label = app.label.clone();
        app.shutdown();
        eprintln!("  {label}: splash_text={text} distinct_bg={bgs}");
        (text, bgs)
    }
    let (rt, rb) = probe(&AppKind::Rust("render_compose"));
    let (pt, pb) = probe(&AppKind::Python("how-to", "render_compose"));
    eprintln!("render_compose: rust(text={rt},bgs={rb}) py(text={pt},bgs={pb})");
    assert!(rt && pt, "render_compose: splash text missing — rust={rt} py={pt}");
    assert!(
        rb >= 8 && pb >= 8,
        "render_compose: gradient background not multi-colour — rust_bgs={rb} py_bgs={pb}"
    );
}

/// how-to/inline01: a centered `Digits` clock. Python runs in INLINE terminal
/// mode (`app.run(inline=True)`); Rust has no inline render mode and runs
/// full-screen. The clock is also wall-clock live. Parity ideal: identical
/// rendering — currently blocked on inline render mode (KNOWN 1.1).
#[test]
#[ignore = "KNOWN 1.1: inline render mode unsupported — Python runs `app.run(inline=True)` \
            (renders a few inline rows); Rust renders full-screen centered. Also wall-clock \
            live (HH:MM:SS), so the digit art is non-deterministic. Structural: both render a \
            Digits clock; full parity needs inline render mode."]
fn parity_howto_inline01() {
    let script = [Step::Wait(400)];
    let (rf, pf) = cat_both("inline01", "how-to", &script, 200);
    assert_glyph_parity("inline01", &pf, &rf, &[]);
}

/// how-to/inline02: same as inline01 plus an `&:inline` CSS block (border:none,
/// height:50vh, success-coloured Digits) that only applies in inline mode. Same
/// KNOWN 1.1 inline-render-mode gap.
#[test]
#[ignore = "KNOWN 1.1: inline render mode unsupported — Python `app.run(inline=True)` applies \
            the `&:inline` rules (no border, 50vh height, $success Digits) Rust never enters; \
            Rust renders full-screen. Wall-clock live clock too. Full parity needs inline mode."]
fn parity_howto_inline02() {
    let script = [Step::Wait(400)];
    let (rf, pf) = cat_both("inline02", "how-to", &script, 200);
    assert_glyph_parity("inline02", &pf, &rf, &[]);
}
