//! End-to-end verification for the `@click` action-link subsystem.
//!
//! This drives the real `actions03` docs example in a PTY (the full runtime:
//! input decode → hit-test → `@click` cell-meta lookup → action dispatch →
//! `on_app_action_str` → screen-style mutation → re-render), then asserts that
//! *clicking* a `[@click=app.set_background('red')]Red[/]` span actually fired
//! the action and changed the screen background to red.
//!
//! Proves the keystone the audit flagged as dead: markup parsed `@click` meta
//! but nothing hit-tested a clicked span and dispatched its action.  A plain
//! key-only PTY case could never exercise this; the mouse click is essential.
//!
//! Run idle — like `pty_parity`, `wait_for_stable` is load-sensitive.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const COLS: u16 = 120;
const ROWS: u16 = 30;
const STABILIZE_POLL: Duration = Duration::from_millis(100);
const STABLE_POLLS: usize = 5;
const STABILIZE_TIMEOUT: Duration = Duration::from_secs(15);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ensure_docs_examples_built() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = std::process::Command::new(&cargo)
            .args(["build", "--workspace", "--examples", "--keep-going"])
            .current_dir(repo_root().join("docs/examples"))
            .status()
            .expect("failed to spawn docs/examples build");
        assert!(status.success(), "docs/examples build failed");
    });
}

fn profile_dir_name() -> String {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| if p.ends_with("deps") { p.parent() } else { Some(p) })
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "debug".to_string())
}

fn docs_example_binary(name: &str) -> PathBuf {
    let bin = repo_root()
        .join("docs/examples/target")
        .join(profile_dir_name())
        .join("examples")
        .join(name);
    assert!(
        bin.exists(),
        "example binary missing after build: {}",
        bin.display()
    );
    bin
}

fn screen_text(parser: &vt100::Parser) -> String {
    let screen = parser.screen();
    let mut lines = Vec::with_capacity(ROWS as usize);
    for row in 0..ROWS {
        let mut line = String::new();
        for col in 0..COLS {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let contents = cell.contents();
            if contents.is_empty() {
                line.push(' ');
            } else {
                line.push_str(&contents);
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

fn wait_for_stable(parser: &Arc<Mutex<vt100::Parser>>, label: &str) -> String {
    let start = Instant::now();
    let mut prev = String::new();
    let mut stable = 0usize;
    loop {
        std::thread::sleep(STABILIZE_POLL);
        let cur = screen_text(&parser.lock().unwrap());
        if !cur.trim().is_empty() && cur == prev {
            stable += 1;
            if stable >= STABLE_POLLS {
                return cur;
            }
        } else {
            stable = 0;
        }
        prev = cur;
        assert!(
            start.elapsed() < STABILIZE_TIMEOUT,
            "{label}: screen did not stabilize within {STABILIZE_TIMEOUT:?}; last screen:\n{prev}"
        );
    }
}

/// Locate the (row, col) of the first character of `needle` in the screen.
fn find_text(screen_text: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in screen_text.lines().enumerate() {
        if let Some(byte_col) = line.find(needle) {
            // byte_col == char col here because the demo text is ASCII.
            return Some((row as u16, byte_col as u16));
        }
    }
    None
}

/// Count how many cells across the whole screen have an RGB background whose
/// red channel dominates (red >> green, red >> blue) — i.e. the "red" surface.
fn red_bg_cell_count(parser: &vt100::Parser) -> usize {
    let screen = parser.screen();
    let mut count = 0usize;
    for row in 0..ROWS {
        for col in 0..COLS {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if let vt100::Color::Rgb(r, g, b) = cell.bgcolor() {
                if r > 120 && g < 90 && b < 90 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// SGR mouse press + release at a 1-based (col, row) cell.
fn sgr_click(col: u16, row: u16) -> String {
    // Button 0 (left). SGR is 1-based.
    let c = col + 1;
    let r = row + 1;
    format!("\x1b[<0;{c};{r}M\x1b[<0;{c};{r}m")
}

#[test]
fn click_at_click_span_fires_action_and_changes_background() {
    ensure_docs_examples_built();
    let bin = docs_example_binary("actions03");

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let mut cmd = CommandBuilder::new(bin);
    cmd.cwd(repo_root());
    cmd.env("TERM", "xterm-256color");
    cmd.env("LANG", "en_US.UTF-8");
    cmd.env("TEXTUAL_KEYBOARD_PROTOCOL", "off");
    cmd.env("TEXTUAL_SYNC_OUTPUT", "0");

    let mut child = pty.slave.spawn_command(cmd).expect("spawn actions03 in pty");
    drop(pty.slave);

    let mut reader = pty.master.try_clone_reader().expect("pty reader");
    let mut writer = pty.master.take_writer().expect("pty writer");

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

    // Initial frame: "Red" link is present and the background is NOT red yet.
    let initial = wait_for_stable(&parser, "actions03-initial");
    assert!(
        initial.contains("Red"),
        "expected the 'Red' action-link to be visible; screen:\n{initial}"
    );
    let red_before = red_bg_cell_count(&parser.lock().unwrap());
    assert_eq!(
        red_before, 0,
        "background should not be red before the click; screen:\n{initial}"
    );

    let (row, col) = find_text(&initial, "Red").expect("locate 'Red' link cell");

    // Click the 'R' of the Red link (left button press + release, SGR).
    writer
        .write_all(sgr_click(col, row).as_bytes())
        .expect("send mouse click");
    writer.flush().expect("flush mouse click");
    std::thread::sleep(Duration::from_millis(300));
    let _after = wait_for_stable(&parser, "actions03-after-click");

    let red_after = red_bg_cell_count(&parser.lock().unwrap());

    child.kill().ok();
    child.wait().ok();
    drop(pty.master);
    reader_thread.join().ok();

    assert!(
        red_after > 100,
        "clicking the [@click=app.set_background('red')] span must turn the \
         screen background red (red bg cells before={red_before}, after={red_after})"
    );
}

// NOTE on hello05/06 + actions05 (widget-scoped `@click`):
//
// The `@click` *routing* is the same code path proven above (a click reads the
// cell `@click` meta and dispatches via the runtime).  Their additional concern
// — an unnamespaced action resolving to the *widget's own* `action_<name>` —
// is covered deterministically by the `action::resolve_action` unit tests and
// each example's own `execute_action` test.  A full-PTY assertion for hello05
// is currently blocked by a *pre-existing, unrelated* render bug: the
// `Hello(Static)` wrapper widget (hello04/05/06) renders an empty content box
// (its inner `Static` text does not paint).  That wrapper-delegation render gap
// predates and is independent of the action subsystem; see the task report
// `deferred` note.  The actions03 PTY case above is the load-bearing
// end-to-end proof of the `@click` → hit-test → dispatch → mutate chain.
