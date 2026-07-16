//! Width-0 render byte-identity gate for the Phase 4 wrapped-render
//! rewrite: with `soft_wrap(false)` (wrap width 0) the render output must
//! be byte-identical to the pre-wrap render path. Golden files were
//! generated from the Phase 3 (pre-rewrite) code.
//!
//! Regenerate with: `TEXT_AREA_RENDER_GATE_REGEN=1 cargo test --test
//! text_area_render_gate` (only do this deliberately; the point of the
//! gate is that the goldens do NOT change).
//!
//! NOTE: the gate covers painted output only; Up/Down boundary NAVIGATION
//! intentionally changes to Python parity in Phase 4.

use rich_rs::Console;
use textual::prelude::*;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

fn dump_render(ta: &TextArea, width: usize, height: usize) -> String {
    let console = Console::new();
    let options = options_for(&console, width, height);
    let segments = textual::widgets::Render::render(ta, &console, &options);
    format!("{segments:#?}\n")
}

fn check_golden(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/text_area_render_gate")
        .join(format!("{name}.txt"));
    if std::env::var("TEXT_AREA_RENDER_GATE_REGEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing golden {path:?}: {e}"));
    assert_eq!(
        actual, expected,
        "width-0 render output changed for case {name}"
    );
}

const DOC: &str = "The quick brown fox\njumps over\nthe lazy dog near the riverbank\n\nend";

#[test]
fn width0_plain_document() {
    let ta = TextArea::new(DOC).with_soft_wrap(false);
    check_golden("plain", &dump_render(&ta, 24, 6));
}

#[test]
fn width0_gutter_and_unicode() {
    let ta = TextArea::new("好好学习\na\u{0301}👩‍🚀 tab\there\nplain")
        .with_soft_wrap(false)
        .with_show_line_numbers(true);
    check_golden("gutter_unicode", &dump_render(&ta, 20, 5));
}

#[test]
fn width0_horizontal_scroll() {
    let mut ta = TextArea::new("a very long single line that overflows the view width")
        .with_soft_wrap(false);
    ta.on_layout(20, 4);
    let end = ta.document().end();
    ta.set_selection(textual::document::Selection::cursor(end.into()));
    check_golden("hscroll", &dump_render(&ta, 20, 4));
}

#[test]
fn width0_selection_spanning_lines() {
    let mut ta = TextArea::new(DOC).with_soft_wrap(false);
    ta.set_selection(textual::document::Selection {
        start: textual::document::Cursor { row: 0, col: 4 },
        end: textual::document::Cursor { row: 2, col: 8 },
    });
    check_golden("selection", &dump_render(&ta, 24, 6));
}
