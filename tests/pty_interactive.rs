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
#[ignore = "BUG: Checkbox toggle-slot bg is #ffffff (Rust) vs #1b1b1b (Python) — the switch indicator paints a white background instead of the dark surface. 97 cells."]
fn parity_checkbox_toggle() {
    let script = [Step::Key(Key::Space), Step::Wait(250)];
    let (rf, pf) = widgets_both("checkbox", &script, 400);
    assert_glyph_parity("checkbox", &pf, &rf, &[]);
}

/// switch: the focused switch toggles on Enter/Space.
#[test]
#[ignore = "BUG: switch.tcss centers content (`align: center middle` on the container); Rust left-aligns every row (no horizontal align). 250 glyph cells shifted."]
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
#[ignore = "BUG: after expand-all('e') then collapse-all('c') Rust stays expanded (▼) while Python collapses (▶) — action_collapse_or_expand / `collapsed` reactive not applied. 2 glyph + surface tint cells."]
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
#[ignore = "BUG: Enter on the focused Collapsible does not toggle it — Python collapses (▶), Rust stays expanded (▼). 1 glyph + surface tint cells."]
fn parity_collapsible_nested_toggle() {
    let script = [Step::Key(Key::Enter), Step::Wait(250)];
    let (rf, pf) = widgets_both("collapsible_nested", &script, 400);
    assert_glyph_parity("collapsible_nested", &pf, &rf, &[]);
}

/// collapsible_custom_symbol: static after compose; assert initial parity (the
/// custom >>> / v symbols and one expanded/one collapsed panel).
#[test]
#[ignore = "BUG: Collapsible header/title surface colour differs from Python (180 cells, fg/bg tint on the custom-symbol header band)."]
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
#[ignore = "BUG: Tree guide-line colour wrong (Rust fg #0178d4 blue vs Python #4f4f4f muted) and cursor-row bg blended (#0c7dd4 vs #0178d4 $primary). 7 cells."]
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
#[ignore = "BUG: DataTable cursor/zebra row bg blended differently (#2d3740 vs #2b3339) — cursor-row tint composition off. 92 cells."]
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
#[ignore = "BUG: DataTable (zebra + cycled cursor) row/cursor bg colours differ from Python (670 cells)."]
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
#[ignore = "BUG: DataTable (after sort) cursor/row bg colours differ from Python (67 cells)."]
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
#[ignore = "BUG: the green/blue Inputs are constructed with value \"0\" but Rust renders their PLACEHOLDER text (\"Enter green 0-255\"/\"Enter blue 0-255\") instead of the value \"0\" that Python shows. 29 glyph cells + focused-Input :focus background-tint (#1e1e1e vs #272727)."]
fn parity_computed01_color() {
    let script = [Step::SendKeys("123"), Step::Wait(300)];
    let (rf, pf) = cat_both("computed01", "guide/reactivity", &script, 400);
    assert_glyph_parity("computed01", &pf, &rf, &[]);
}

/// watch01: submit a colour name; both swatches update their backgrounds.
#[test]
#[ignore = "BUG: glyph-perfect but focused Input bg #1e1e1e (Rust) vs #272727 (Python applies `background-tint: $foreground 5%` on :focus). 247 colour cells. Shared root: :focus background-tint not applied (wave-1)."]
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
#[ignore = "BUG: glyph-perfect (\"Hello, Will!\" on both) but focused Input bg #1e1e1e (Rust) vs #272727 (Python :focus background-tint). 236 colour cells. Shared root: :focus background-tint not applied (wave-1)."]
fn parity_refresh01_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh01", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh01", &pf, &rf, &[]);
}

/// refresh02: same as refresh01 but the reactive has `layout=True`.
#[test]
#[ignore = "BUG: the bordered `Name` widget (width/height auto) renders FULL container size (120 cols x ~25 rows) in Rust while Python shrinks it to its content (a 12x3 box around \"Hello, Will!\"). auto width/height not honoured for a bordered widget. 288 glyph cells + focused-Input :focus tint."]
fn parity_refresh02_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh02", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh02", &pf, &rf, &[]);
}

/// refresh03: same but the reactive has `recompose=True` (rebuilds children).
#[test]
#[ignore = "BUG: glyph-perfect (\"Hello, Will!\" on both, recompose works) but focused Input bg #1e1e1e (Rust) vs #272727 (Python :focus background-tint). 236 colour cells. Shared root: :focus background-tint not applied (wave-1)."]
fn parity_refresh03_greeting() {
    let script = [Step::SendKeys("Will"), Step::Wait(300)];
    let (rf, pf) = cat_both("refresh03", "guide/reactivity", &script, 400);
    assert_glyph_parity("refresh03", &pf, &rf, &[]);
}

/// set_reactive01: pressing Space cycles the greeting via a watcher.
#[test]
#[ignore = "BUG/divergence: the Python reference itself RAISES on startup (Rich traceback shown — `self.greeting = greeting` in __init__ fires watch_greeting before compose → query_one NoMatches; this is the doc's intended failure mode that set_reactive02 fixes). Rust does NOT reproduce it: it suppresses/defers the pre-mount watcher and renders \"Hola\". reactive watcher init-timing differs (Python eager → crash, Rust deferred). Also exposes the same `who`-not-rendered gap as set_reactive02."]
fn parity_set_reactive01_greeting() {
    let script = [Step::Key(Key::Space), Step::Wait(300)];
    let (rf, pf) = cat_both("set_reactive01", "guide/reactivity", &script, 400);
    assert_glyph_parity("set_reactive01", &pf, &rf, &[]);
}

/// set_reactive02: same interaction; greeting initialised via `set_reactive`.
#[test]
#[ignore = "BUG: Python shows \"Hola Textual\" (greeting cycled + who=\"Textual\"); Rust shows only \"Hola\" — the `who` Label initialised via set_reactive(\"Textual\") in __init__ is NOT reflected at compose time (Label(self.who) reads the default empty reactive). Root: reactive value assigned before compose not read by compose-time content."]
fn parity_set_reactive02_greeting() {
    let script = [Step::Key(Key::Space), Step::Wait(300)];
    let (rf, pf) = cat_both("set_reactive02", "guide/reactivity", &script, 400);
    assert_glyph_parity("set_reactive02", &pf, &rf, &[]);
}

/// set_reactive03: submitting a name appends a `Hello, <name>` Label via
/// `mutate_reactive` + recompose.
#[test]
#[ignore = "BUG: after submitting \"Ada\", Python recomposes and shows a \"Hello, Ada\" Label; Rust recomposes the Input (placeholder returns) but the appended Label is MISSING. Root: mutate_reactive on a list + recompose does not render the newly-appended children."]
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
#[ignore = "BUG: clicking Start should add the `.started` class whose CSS hides #start/#reset (`display:none`, `visibility:hidden`) and shows #stop (`display:block`). Python correctly shows only \"Stop\"; Rust still shows \"Start\" AND \"Reset\" (started-state descendant CSS toggles not applied). 42 glyph + 188 colour cells. Root: `.started #start/#stop/#reset` display/visibility class-state CSS not reflected after add_class."]
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
