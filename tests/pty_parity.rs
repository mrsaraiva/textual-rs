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
        // Exercises the post-keypress update path (move counter + filled count +
        // cell toggling) — the behavior RA-4's Handle<Label> migration touched,
        // which the initial-screen cases do not cover.
        name: "five_by_five_after_move",
        example: "five_by_five",
        args: &[],
        cwd: None,
        keys: " ",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        // Diagnostic for the five_by_five input bug: `?` is an app-level binding
        // (help screen push), not a cursor/cell key. If this works but
        // after_move does not, keys are being captured by the focused GameCell;
        // if this also fails, input is dead app-wide for this example.
        name: "five_by_five_help",
        example: "five_by_five",
        args: &[],
        cwd: None,
        keys: "?",
        golden_replacements: &[],
        status: Status::XFail(
            "Input is FIXED — `?` now pushes the help screen (overlay box \
             appears). Remaining gap: the help screen's Markdown content renders \
             EMPTY (Python shows the 5x5/Introduction/Objective text). This is a \
             Markdown-in-screen rendering bug, same class as the dictionary/ \
             code_browser content-rendering xfails (P3/P4), not an input issue.",
        ),
    },
    Case {
        // Second interactive case for json_tree (beyond add_node) to confirm it
        // is genuinely interactive, not coincidentally passing one key.
        name: "json_tree_toggle_root",
        example: "json_tree",
        args: &[],
        cwd: None,
        keys: "t",
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
        status: Status::Pass,
    },
    // ---- Phase 1A: interactive docs/examples (separate workspace, docs_ prefix) ----
    // Built from docs/examples/ and located in its target dir. Goldens generated
    // from the Python docs examples (../textual/docs/examples/widgets/<name>.py).
    // Start as XFail "unverified"; promote each to Pass once confirmed matching.
    Case {
        name: "docs_text_area_extended",
        example: "text_area_extended",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Verified at parity via tools/parity/scoreboard-docs.sh (widgets tier),
    // goldens generated from ../textual/docs/examples/widgets/<name>.py.
    Case {
        name: "docs_input",
        example: "input",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_input_types",
        example: "input_types",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_input_validation",
        example: "input_validation",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_tabbed_content",
        example: "tabbed_content",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_tabbed_content_label_color",
        example: "tabbed_content_label_color",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_area_custom_theme",
        example: "text_area_custom_theme",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_tree",
        example: "tree",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Wave-1 ports verified at parity (scoreboard + strict harness).
    Case {
        name: "docs_label",
        example: "label",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_static",
        example: "static",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_link",
        example: "link",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_digits",
        example: "digits",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_option_list_strings",
        example: "option_list_strings",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_radio_set",
        example: "radio_set",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_radio_button",
        example: "radio_button",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_data_table_cursors",
        example: "data_table_cursors",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_data_table_sort",
        example: "data_table_sort",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center01",
        example: "center01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // how-to center variants — verified at parity via scoreboard-docs.sh after
    // the text-align + transparent-wrapper auto-sizing fundamentals landed.
    Case {
        name: "docs_center02",
        example: "center02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center03",
        example: "center03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center04",
        example: "center04",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center06",
        example: "center06",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center07",
        example: "center07",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // app examples (docs/examples/app) with non-empty initial screens.
    Case {
        name: "docs_question01",
        example: "question01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_suspend",
        example: "suspend",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_suspend_process",
        example: "suspend_process",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // grid examples — verified at parity after grid children are sized by their
    // own box model within the cell (auto height no longer stretches to the row).
    Case {
        name: "docs_question02",
        example: "question02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_question03",
        example: "question03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_question_title01",
        example: "question_title01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_question_title02",
        example: "question_title02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_footer",
        example: "footer",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_header",
        example: "header",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_header_app_title",
        example: "header_app_title",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_masked_input",
        example: "masked_input",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_pretty",
        example: "pretty",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_radio_set_changed",
        example: "radio_set_changed",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_select_from_values_widget",
        example: "select_from_values_widget",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_selection_list_selections",
        example: "selection_list_selections",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_selection_list_tuples",
        example: "selection_list_tuples",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_sparkline",
        example: "sparkline",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_sparkline_basic",
        example: "sparkline_basic",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_sparkline_colors",
        example: "sparkline_colors",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_horizontal_rules",
        example: "horizontal_rules",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_vertical_rules",
        example: "vertical_rules",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_progress_bar",
        example: "progress_bar",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_progress_bar_gradient",
        example: "progress_bar_gradient",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_button",
        example: "button",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_option_list_options",
        example: "option_list_options",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_area_example",
        example: "text_area_example",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_area_selection",
        example: "text_area_selection",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_log",
        example: "log",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_rich_log",
        example: "rich_log",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_prevent",
        example: "prevent",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // how-to center variants (vertical/horizontal centering).
    Case {
        name: "docs_center08",
        example: "center08",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center09",
        example: "center09",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_center10",
        example: "center10",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // themes: muted background tokens (compose class metadata fix).
    Case {
        name: "docs_muted_backgrounds",
        example: "muted_backgrounds",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers01",
        example: "containers01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers02",
        example: "containers02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers03",
        example: "containers03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers04",
        example: "containers04",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers05",
        example: "containers05",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers07",
        example: "containers07",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers08",
        example: "containers08",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers09",
        example: "containers09",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout02",
        example: "layout02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout03",
        example: "layout03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout04",
        example: "layout04",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Horizontal button row (margin: 2 4). Verifies adjacent-margin collapse
    // (gap = max(right, left) = 4, not summed) — fixed in layout/horizontal.rs.
    Case {
        name: "docs_on_decorator01",
        example: "on_decorator01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_on_decorator02",
        example: "on_decorator02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_colored_text",
        example: "colored_text",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_stopwatch01",
        example: "stopwatch01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_stopwatch02",
        example: "stopwatch02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_stopwatch03",
        example: "stopwatch03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_stopwatch04",
        example: "stopwatch04",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_option_list_tables",
        example: "option_list_tables",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Engine-gap wave 2 (layoutsize): w/h units + align middle + 1fr margin-reserve.
    Case {
        name: "docs_max_width",
        example: "max_width",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_max_height",
        example: "max_height",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_min_width",
        example: "min_width",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_nesting01",
        example: "nesting01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_nesting02",
        example: "nesting02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Engine-gap wave 2 (render): outline painted over the widget's own edge cells.
    Case {
        name: "docs_outline",
        example: "outline",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_outline01",
        example: "outline01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    // Border-title widget API: Label/Static gained with_border_title/-subtitle.
    Case {
        name: "docs_border_title_align",
        example: "border_title_align",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_border_subtitle_align",
        example: "border_subtitle_align",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_border_title_colors",
        example: "border_title_colors",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_border_title",
        example: "border_title",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_checkbox",
        example: "checkbox",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_data_table_labels",
        example: "data_table_labels",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_list_view",
        example: "list_view",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_data_table_renderables",
        example: "data_table_renderables",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_data_table_fixed",
        example: "data_table_fixed",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_collapsible",
        example: "collapsible",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_collapsible_nested",
        example: "collapsible_nested",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_collapsible_custom_symbol",
        example: "collapsible_custom_symbol",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_content_switcher",
        example: "content_switcher",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        // Fixed: DataTable cell-padding (Python cell_padding=1 leading space) +
        // content_width + scroll_virtual_content_size (no spurious scrollbar).
        // Matches Python; 26 data_table unit tests green; snapshot legitimately
        // updated to reflect the corrected padding.
        name: "docs_data_table",
        example: "data_table",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_selection_list_selected",
        example: "selection_list_selected",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_select_widget",
        example: "select_widget",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_select_widget_no_blank",
        example: "select_widget_no_blank",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_tabs",
        example: "tabs",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "code_browser_initial",
        example: "code_browser",
        args: &["./"],
        cwd: Some(FIXTURE_SAMPLE_DIR),
        keys: "",
        golden_replacements: &[],
        status: Status::XFail(
            "Closest diff isolated to ONE row: Python golden row 29 is a hatch \
             (`╱`) fill row; Rust leaves it blank and the Footer lands one row too \
             high (content area is 27 rows vs Python's 28). Root cause: spurious \
             HORIZONTAL scrollbar on `#code-view` (a VerticalScroll with CSS \
             `overflow: auto scroll`). overflow-y=scroll force-shows the vertical \
             scrollbar, shrinking the viewport from 102 to 100 cols. \
             `apply_host_scrollbar_layout` (src/runtime/render.rs) then re-measures \
             children via `host_content_extent`, which reports the child `#code` \
             (Static, `width: auto`) at its LAYOUT width (fills the container) \
             rather than its NATURAL content width. Python uses natural content \
             width (~2 cols for empty padding): 2 <= 100 => no horizontal \
             scrollbar. Rust sees 100 and, combined with the \
             `content_width.max(widget_width)` floor in `ScrollbarPolicy::resolve` \
             (src/widgets/scrollbar.rs) plus the `virtual_w.max(content_w)` floor \
             at the second resolve() call, computes 102 > 100 => a horizontal \
             scrollbar that eats the missing content row. \
             TRIED AND REVERTED (do not re-attempt without the real fix): (a) \
             dropping the `max(widget_width/height)` floors so content that merely \
             FILLS the viewport is not counted as overflow — makes code_browser \
             Pass but regresses `tree_mode_render_produces_chrome_not_blank` and \
             `layout_info_sets_vertical_scroll_virtual_content_in_tree_mode` (they \
             rely on the floor to size the viewport when virtual content is \
             unknown; without it the viewport collapses to 0 lines); (b) moving \
             ScrollView overflow out of `seed.styles.style` into dedicated policy \
             fields — regresses `layout_info_sets_vertical_scroll_virtual_content_\
             in_tree_mode` because the off-tree VerticalScroll virtual-sizing path \
             reads overflow back from `seed.styles.style`; approach (b) now \
             attempted after fixing those tests, but did not resolve the core \
             issue: the hatch area remains absent because `host_content_extent` \
             still uses layout extent. \
             CORRECT FIX (future): `host_content_extent` must use NATURAL content \
             size for `width:auto`/`height:auto` scroll children instead of layout \
             extent, so the scrollbar floors can stay AND empty `width:auto` \
             children are not treated as filling the viewport.",
        ),
    },
    Case {
        name: "docs_render_compose",
        example: "render_compose",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout01",
        example: "layout01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout06",
        example: "layout06",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_modal01",
        example: "modal01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_modal02",
        example: "modal02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_modal03",
        example: "modal03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_layout05",
        example: "layout05",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_containers06",
        example: "containers06",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_align_all",
        example: "align_all",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_background",
        example: "background",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_background_tint",
        example: "background_tint",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_background_transparency",
        example: "background_transparency",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_border",
        example: "border",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_border01",
        example: "border01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_box_sizing01",
        example: "box_sizing01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_color",
        example: "color",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_color_auto",
        example: "color_auto",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_colors",
        example: "colors",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_colors01",
        example: "colors01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_colors02",
        example: "colors02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_content_align_all",
        example: "content_align_all",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_dimensions01",
        example: "dimensions01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_dimensions02",
        example: "dimensions02",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_dimensions03",
        example: "dimensions03",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_dimensions04",
        example: "dimensions04",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid",
        example: "grid",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_columns",
        example: "grid_columns",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_gutter",
        example: "grid_gutter",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout1",
        example: "grid_layout1",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout2",
        example: "grid_layout2",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout3_row_col_adjust",
        example: "grid_layout3_row_col_adjust",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout5_col_span",
        example: "grid_layout5_col_span",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout6_row_span",
        example: "grid_layout6_row_span",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout7_gutter",
        example: "grid_layout7_gutter",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_size_both",
        example: "grid_size_both",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_size_columns",
        example: "grid_size_columns",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_horizontal_layout",
        example: "horizontal_layout",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_link_style_hover",
        example: "link_style_hover",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_margin",
        example: "margin",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_margin01",
        example: "margin01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_margin_all",
        example: "margin_all",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_padding",
        example: "padding",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_padding01",
        example: "padding01",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_screen",
        example: "screen",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_scrollbar_gutter",
        example: "scrollbar_gutter",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_overflow",
        example: "text_overflow",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_style_all",
        example: "text_style_all",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_text_wrap",
        example: "text_wrap",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_tint",
        example: "tint",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_vertical_layout",
        example: "vertical_layout",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_vertical_layout_scrolled",
        example: "vertical_layout_scrolled",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_visibility",
        example: "visibility",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_widget",
        example: "widget",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_rows",
        example: "grid_rows",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout4_row_col_adjust",
        example: "grid_layout4_row_col_adjust",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_grid_layout_auto",
        example: "grid_layout_auto",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_column_span",
        example: "column_span",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
    Case {
        name: "docs_row_span",
        example: "row_span",
        args: &[],
        cwd: None,
        keys: "",
        golden_replacements: &[],
        status: Status::Pass,
    },
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ensure_examples_built() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = std::process::Command::new(&cargo)
            .args(["build", "--examples"])
            .current_dir(repo_root())
            .status()
            .expect("failed to spawn cargo build --examples");
        assert!(status.success(), "cargo build --examples failed");

        // The docs examples live in a separate workspace (docs/examples/) with
        // its own target dir; build them too so `docs_*` parity cases can run.
        let docs_status = std::process::Command::new(&cargo)
            .args(["build", "--workspace", "--examples", "--keep-going"])
            .current_dir(repo_root().join("docs/examples"))
            .status()
            .expect("failed to spawn docs/examples build");
        assert!(docs_status.success(), "docs/examples build failed");
    });
}

/// Resolve the profile dir name (debug/release) from the running test binary.
fn profile_dir_name() -> String {
    let exe = std::env::current_exe().expect("current_exe");
    // .../target/<profile>/deps/<test-bin>
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
        .unwrap_or_else(|| "debug".to_string())
}

fn example_binary(case: &Case) -> PathBuf {
    let bin = if case.name.starts_with("docs_") {
        // Docs examples build into the separate docs/examples workspace target.
        repo_root()
            .join("docs/examples/target")
            .join(profile_dir_name())
            .join("examples")
            .join(case.example)
    } else {
        // Main-crate examples share the test binary's profile dir.
        let mut dir = std::env::current_exe().expect("current_exe");
        dir.pop(); // strip test binary name
        if dir.ends_with("deps") {
            dir.pop();
        }
        dir.join("examples").join(case.example)
    };
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
    let bin = example_binary(case);

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
pty_case!(five_by_five_after_move, "five_by_five_after_move");
pty_case!(five_by_five_help, "five_by_five_help");
pty_case!(json_tree_toggle_root, "json_tree_toggle_root");
pty_case!(docs_content_switcher, "docs_content_switcher");
pty_case!(docs_data_table, "docs_data_table");
pty_case!(docs_selection_list_selected, "docs_selection_list_selected");
pty_case!(docs_select_widget, "docs_select_widget");
pty_case!(docs_select_widget_no_blank, "docs_select_widget_no_blank");
pty_case!(docs_tabs, "docs_tabs");
pty_case!(docs_text_area_extended, "docs_text_area_extended");
pty_case!(docs_input, "docs_input");
pty_case!(docs_input_types, "docs_input_types");
pty_case!(docs_input_validation, "docs_input_validation");
pty_case!(docs_tabbed_content, "docs_tabbed_content");
pty_case!(docs_tabbed_content_label_color, "docs_tabbed_content_label_color");
pty_case!(docs_text_area_custom_theme, "docs_text_area_custom_theme");
pty_case!(docs_tree, "docs_tree");
pty_case!(docs_label, "docs_label");
pty_case!(docs_static, "docs_static");
pty_case!(docs_link, "docs_link");
pty_case!(docs_digits, "docs_digits");
pty_case!(docs_option_list_strings, "docs_option_list_strings");
pty_case!(docs_radio_set, "docs_radio_set");
pty_case!(docs_radio_button, "docs_radio_button");
pty_case!(docs_data_table_cursors, "docs_data_table_cursors");
pty_case!(docs_data_table_sort, "docs_data_table_sort");
pty_case!(docs_center01, "docs_center01");
pty_case!(docs_center02, "docs_center02");
pty_case!(docs_center03, "docs_center03");
pty_case!(docs_center04, "docs_center04");
pty_case!(docs_center06, "docs_center06");
pty_case!(docs_center07, "docs_center07");
pty_case!(docs_question01, "docs_question01");
pty_case!(docs_suspend, "docs_suspend");
pty_case!(docs_suspend_process, "docs_suspend_process");
pty_case!(docs_question02, "docs_question02");
pty_case!(docs_question03, "docs_question03");
pty_case!(docs_question_title01, "docs_question_title01");
pty_case!(docs_question_title02, "docs_question_title02");
pty_case!(docs_footer, "docs_footer");
pty_case!(docs_header, "docs_header");
pty_case!(docs_header_app_title, "docs_header_app_title");
pty_case!(docs_masked_input, "docs_masked_input");
pty_case!(docs_pretty, "docs_pretty");
pty_case!(docs_radio_set_changed, "docs_radio_set_changed");
pty_case!(docs_select_from_values_widget, "docs_select_from_values_widget");
pty_case!(docs_selection_list_selections, "docs_selection_list_selections");
pty_case!(docs_selection_list_tuples, "docs_selection_list_tuples");
pty_case!(docs_sparkline, "docs_sparkline");
pty_case!(docs_sparkline_basic, "docs_sparkline_basic");
pty_case!(docs_sparkline_colors, "docs_sparkline_colors");
pty_case!(docs_horizontal_rules, "docs_horizontal_rules");
pty_case!(docs_vertical_rules, "docs_vertical_rules");
pty_case!(docs_progress_bar, "docs_progress_bar");
pty_case!(docs_progress_bar_gradient, "docs_progress_bar_gradient");
pty_case!(docs_button, "docs_button");
pty_case!(docs_checkbox, "docs_checkbox");
pty_case!(docs_stopwatch03, "docs_stopwatch03");
pty_case!(docs_stopwatch04, "docs_stopwatch04");
pty_case!(docs_option_list_tables, "docs_option_list_tables");
pty_case!(docs_max_width, "docs_max_width");
pty_case!(docs_max_height, "docs_max_height");
pty_case!(docs_min_width, "docs_min_width");
pty_case!(docs_nesting01, "docs_nesting01");
pty_case!(docs_nesting02, "docs_nesting02");
pty_case!(docs_outline, "docs_outline");
pty_case!(docs_outline01, "docs_outline01");
pty_case!(docs_border_title_align, "docs_border_title_align");
pty_case!(docs_border_subtitle_align, "docs_border_subtitle_align");
pty_case!(docs_border_title_colors, "docs_border_title_colors");
pty_case!(docs_border_title, "docs_border_title");
pty_case!(docs_prevent, "docs_prevent");
pty_case!(docs_center08, "docs_center08");
pty_case!(docs_center09, "docs_center09");
pty_case!(docs_center10, "docs_center10");
pty_case!(docs_muted_backgrounds, "docs_muted_backgrounds");
pty_case!(docs_containers01, "docs_containers01");
pty_case!(docs_containers02, "docs_containers02");
pty_case!(docs_containers03, "docs_containers03");
pty_case!(docs_containers04, "docs_containers04");
pty_case!(docs_containers05, "docs_containers05");
pty_case!(docs_containers07, "docs_containers07");
pty_case!(docs_containers08, "docs_containers08");
pty_case!(docs_containers09, "docs_containers09");
pty_case!(docs_layout02, "docs_layout02");
pty_case!(docs_layout03, "docs_layout03");
pty_case!(docs_layout04, "docs_layout04");
pty_case!(docs_on_decorator01, "docs_on_decorator01");
pty_case!(docs_on_decorator02, "docs_on_decorator02");
pty_case!(docs_colored_text, "docs_colored_text");
pty_case!(docs_stopwatch01, "docs_stopwatch01");
pty_case!(docs_stopwatch02, "docs_stopwatch02");
pty_case!(docs_log, "docs_log");
pty_case!(docs_rich_log, "docs_rich_log");
pty_case!(docs_option_list_options, "docs_option_list_options");
pty_case!(docs_text_area_example, "docs_text_area_example");
pty_case!(docs_text_area_selection, "docs_text_area_selection");

pty_case!(docs_data_table_labels, "docs_data_table_labels");
pty_case!(docs_list_view, "docs_list_view");
pty_case!(docs_data_table_renderables, "docs_data_table_renderables");
pty_case!(docs_data_table_fixed, "docs_data_table_fixed");
pty_case!(docs_collapsible, "docs_collapsible");
pty_case!(docs_collapsible_nested, "docs_collapsible_nested");
pty_case!(docs_collapsible_custom_symbol, "docs_collapsible_custom_symbol");
pty_case!(json_tree_initial, "json_tree_initial");
pty_case!(json_tree_add_node, "json_tree_add_node");
pty_case!(dictionary_initial, "dictionary_initial");
pty_case!(code_browser_initial, "code_browser_initial");
pty_case!(docs_render_compose, "docs_render_compose");
pty_case!(docs_layout01, "docs_layout01");
pty_case!(docs_layout06, "docs_layout06");
pty_case!(docs_modal01, "docs_modal01");
pty_case!(docs_modal02, "docs_modal02");
pty_case!(docs_modal03, "docs_modal03");
pty_case!(docs_layout05, "docs_layout05");
pty_case!(docs_containers06, "docs_containers06");
pty_case!(docs_align_all, "docs_align_all");
pty_case!(docs_background, "docs_background");
pty_case!(docs_background_tint, "docs_background_tint");
pty_case!(docs_background_transparency, "docs_background_transparency");
pty_case!(docs_border, "docs_border");
pty_case!(docs_border01, "docs_border01");
pty_case!(docs_box_sizing01, "docs_box_sizing01");
pty_case!(docs_color, "docs_color");
pty_case!(docs_color_auto, "docs_color_auto");
pty_case!(docs_colors, "docs_colors");
pty_case!(docs_colors01, "docs_colors01");
pty_case!(docs_colors02, "docs_colors02");
pty_case!(docs_content_align_all, "docs_content_align_all");
pty_case!(docs_dimensions01, "docs_dimensions01");
pty_case!(docs_dimensions02, "docs_dimensions02");
pty_case!(docs_dimensions03, "docs_dimensions03");
pty_case!(docs_dimensions04, "docs_dimensions04");
pty_case!(docs_grid, "docs_grid");
pty_case!(docs_grid_columns, "docs_grid_columns");
pty_case!(docs_grid_gutter, "docs_grid_gutter");
pty_case!(docs_grid_layout1, "docs_grid_layout1");
pty_case!(docs_grid_layout2, "docs_grid_layout2");
pty_case!(docs_grid_layout3_row_col_adjust, "docs_grid_layout3_row_col_adjust");
pty_case!(docs_grid_layout5_col_span, "docs_grid_layout5_col_span");
pty_case!(docs_grid_layout6_row_span, "docs_grid_layout6_row_span");
pty_case!(docs_grid_layout7_gutter, "docs_grid_layout7_gutter");
pty_case!(docs_grid_size_both, "docs_grid_size_both");
pty_case!(docs_grid_size_columns, "docs_grid_size_columns");
pty_case!(docs_horizontal_layout, "docs_horizontal_layout");
pty_case!(docs_link_style_hover, "docs_link_style_hover");
pty_case!(docs_margin, "docs_margin");
pty_case!(docs_margin01, "docs_margin01");
pty_case!(docs_margin_all, "docs_margin_all");
pty_case!(docs_padding, "docs_padding");
pty_case!(docs_padding01, "docs_padding01");
pty_case!(docs_screen, "docs_screen");
pty_case!(docs_scrollbar_gutter, "docs_scrollbar_gutter");
pty_case!(docs_text_overflow, "docs_text_overflow");
pty_case!(docs_text_style_all, "docs_text_style_all");
pty_case!(docs_text_wrap, "docs_text_wrap");
pty_case!(docs_tint, "docs_tint");
pty_case!(docs_vertical_layout, "docs_vertical_layout");
pty_case!(docs_vertical_layout_scrolled, "docs_vertical_layout_scrolled");
pty_case!(docs_visibility, "docs_visibility");
pty_case!(docs_widget, "docs_widget");
pty_case!(docs_grid_rows, "docs_grid_rows");
pty_case!(docs_grid_layout4_row_col_adjust, "docs_grid_layout4_row_col_adjust");
pty_case!(docs_grid_layout_auto, "docs_grid_layout_auto");
pty_case!(docs_column_span, "docs_column_span");
pty_case!(docs_row_span, "docs_row_span");

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
