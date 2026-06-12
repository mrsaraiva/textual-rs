//! Real-PTY parity harness.
//!
//! Runs example binaries in a genuine pseudo-terminal, drives them with key
//! input, captures the emulated screen (via `vt100`), and compares the plain
//! text against golden screens generated from **Python Textual** by
//! `tools/parity/gen-python-goldens.sh`.
//!
//! Rules:
//! - Goldens define parity. They are only ever regenerated from Python output;
//!   there is deliberately no "bless from Rust" mechanism.
//! - Known parity gaps are declared as `Status::XFail` with a reason. XFail is
//!   strict: if an xfail case starts matching, the test fails with XPASS until
//!   the manifest entry is promoted to `Status::Pass`. Regressions in `Pass`
//!   cases fail immediately.
//! - Comparison is plain text (trailing whitespace trimmed). Color/attribute
//!   parity is out of scope for this harness version; structural and content
//!   regressions are what it guards.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const COLS: u16 = 120;
const ROWS: u16 = 30;
const STABILIZE_POLL: Duration = Duration::from_millis(100);
const STABLE_POLLS: usize = 5;
const STABILIZE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
enum Status {
    /// Screen must match the Python golden exactly (after replacements).
    Pass,
    /// Known parity gap: screen must NOT match. Matching is an error (XPASS)
    /// so fixes are promoted explicitly instead of silently.
    XFail(&'static str),
}

struct Case {
    name: &'static str,
    example: &'static str,
    args: &'static [&'static str],
    /// Working directory relative to the repo root (None = repo root).
    cwd: Option<&'static str>,
    /// Keys to send after the initial screen stabilizes.
    keys: &'static str,
    /// Literal replacements applied to the golden before comparison, for
    /// intentional Rust/Python differences (e.g. demo.md says "markdown.rs").
    golden_replacements: &'static [(&'static str, &'static str)],
    status: Status,
}

const FIXTURE_SAMPLE_DIR: &str = "tests/pty_parity/fixtures/sample_dir";

const CASES: &[Case] = &[
    Case {
        name: "markdown_initial",
        example: "markdown",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[("markdown.py", "markdown.rs")],
        status: Status::Pass,
    },
    Case {
        name: "markdown_toc_toggle",
        example: "markdown",
        args: &[],
        cwd: None,
        keys: "t",
        golden_replacements: &[("markdown.py", "markdown.rs")],
        status: Status::Pass,
    },
    Case {
        name: "five_by_five_initial",
        example: "five_by_five",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "json_tree_initial",
        example: "json_tree",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "json_tree_add_node",
        example: "json_tree",
        args: &[],
        cwd: None,
        keys: "a",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "dictionary_initial",
        example: "dictionary",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::XFail(
            "#results container does not render; Input bottom border edge missing",
        ),
    },
    Case {
        name: "code_browser_initial",
        example: "code_browser",
        args: &["./"],
        cwd: Some(FIXTURE_SAMPLE_DIR),
        keys: "",
        golden_replacements: &[],
        status: Status::XFail(
            "DirectoryTree never renders; hatch code pane missing; `f` toggle no-op",
        ),
    },
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ensure_examples_built() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = std::process::Command::new(cargo)
            .args(["build", "--examples"])
            .current_dir(repo_root())
            .status()
            .expect("failed to spawn cargo build --examples");
        assert!(status.success(), "cargo build --examples failed");
    });
}

fn example_binary(example: &str) -> PathBuf {
    // Tests and examples share the same profile directory; the test binary
    // lives in target/<profile>/deps/, examples in target/<profile>/examples/.
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // strip test binary name
    if dir.ends_with("deps") {
        dir.pop();
    }
    let bin = dir.join("examples").join(example);
    assert!(
        bin.exists(),
        "example binary missing after build: {}",
        bin.display()
    );
    bin
}

/// Extract the visible screen as plain text: ROWS lines, wide-char
/// continuations skipped, trailing whitespace trimmed.
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

/// Poll until the plain-text screen is non-empty and unchanged for
/// `STABLE_POLLS` consecutive polls, or panic on timeout.
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

fn run_case(case: &Case) -> String {
    ensure_examples_built();
    let bin = example_binary(case.example);

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let mut cmd = CommandBuilder::new(bin);
    for arg in case.args {
        cmd.arg(arg);
    }
    let workdir = match case.cwd {
        Some(rel) => repo_root().join(rel),
        None => repo_root(),
    };
    cmd.cwd(workdir);
    cmd.env("TERM", "xterm-256color");
    cmd.env("LANG", "en_US.UTF-8");
    // Keep the driver from waiting on terminal-capability query responses the
    // vt100 emulator will never send.
    cmd.env("TEXTUAL_KEYBOARD_PROTOCOL", "off");
    cmd.env("TEXTUAL_SYNC_OUTPUT", "0");

    let mut child = pty.slave.spawn_command(cmd).expect("spawn example in pty");
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

    let mut screen = wait_for_stable(&parser, case.name);
    if !case.keys.is_empty() {
        writer.write_all(case.keys.as_bytes()).expect("send keys");
        writer.flush().expect("flush keys");
        // Let the input land before demanding a stable (possibly unchanged) screen.
        std::thread::sleep(Duration::from_millis(300));
        screen = wait_for_stable(&parser, case.name);
    }

    child.kill().ok();
    child.wait().ok();
    drop(pty.master);
    reader_thread.join().ok();

    screen
}

fn load_golden(case: &Case) -> String {
    let path = repo_root()
        .join("tests/pty_parity/golden")
        .join(format!("{}.txt", case.name));
    let mut golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden {}: {e}", path.display()));
    for (from, to) in case.golden_replacements {
        golden = golden.replace(from, to);
    }
    golden
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_actual(case: &Case, actual: &str) -> PathBuf {
    let dir = repo_root().join("target/pty-parity-actual");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join(format!("{}.txt", case.name));
    std::fs::write(&path, actual).ok();
    path
}

fn diff_summary(golden: &str, actual: &str) -> String {
    let mut out = String::new();
    let golden_lines: Vec<&str> = golden.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let rows = golden_lines.len().max(actual_lines.len());
    for i in 0..rows {
        let g = golden_lines.get(i).copied().unwrap_or("<missing>");
        let a = actual_lines.get(i).copied().unwrap_or("<missing>");
        if g != a {
            out.push_str(&format!(
                "line {:>2}:\n  python | {g}\n  rust   | {a}\n",
                i + 1
            ));
        }
    }
    out
}

fn check_case(case: &Case) {
    let actual = run_case(case);
    let golden = load_golden(case);
    let matches = actual == golden;
    let actual_path = write_actual(case, &actual);

    match (case.status, matches) {
        (Status::Pass, true) => {}
        (Status::Pass, false) => {
            panic!(
                "PARITY REGRESSION: `{}` no longer matches the Python golden.\n\
                 Golden: tests/pty_parity/golden/{}.txt\n\
                 Actual: {}\n\n{}",
                case.name,
                case.name,
                actual_path.display(),
                diff_summary(&golden, &actual)
            );
        }
        (Status::XFail(reason), false) => {
            eprintln!("xfail (expected, still broken): `{}` — {reason}", case.name);
        }
        (Status::XFail(reason), true) => {
            panic!(
                "XPASS: `{}` now matches the Python golden but is still marked \
                 XFail (\"{reason}\").\nPromote it to Status::Pass in the \
                 tests/pty_parity.rs manifest so the fix is locked in.",
                case.name
            );
        }
    }
}

macro_rules! pty_case {
    ($fn_name:ident, $case_name:literal) => {
        #[test]
        fn $fn_name() {
            let case = CASES
                .iter()
                .find(|c| c.name == $case_name)
                .expect("case in manifest");
            check_case(case);
        }
    };
}

pty_case!(markdown_initial, "markdown_initial");
pty_case!(markdown_toc_toggle, "markdown_toc_toggle");
pty_case!(five_by_five_initial, "five_by_five_initial");
pty_case!(json_tree_initial, "json_tree_initial");
pty_case!(json_tree_add_node, "json_tree_add_node");
pty_case!(dictionary_initial, "dictionary_initial");
pty_case!(code_browser_initial, "code_browser_initial");

/// Every golden file must have a manifest entry and vice versa, so cases can't
/// silently rot.
#[test]
fn manifest_matches_golden_files() {
    let golden_dir = repo_root().join("tests/pty_parity/golden");
    let mut on_disk: Vec<String> = std::fs::read_dir(&golden_dir)
        .expect("golden dir")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".txt").map(str::to_string)
        })
        .collect();
    on_disk.sort();
    let mut in_manifest: Vec<String> = CASES.iter().map(|c| c.name.to_string()).collect();
    in_manifest.sort();
    assert_eq!(
        on_disk, in_manifest,
        "golden files and pty_parity manifest entries out of sync"
    );
}

// Keep Path imported for future fixture assertions without warnings.
#[allow(dead_code)]
fn _fixture_dir_exists() {
    assert!(Path::new(FIXTURE_SAMPLE_DIR).is_relative());
}
