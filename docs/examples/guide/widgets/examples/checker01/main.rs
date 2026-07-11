/// Port of Python Textual `docs/examples/guide/widgets/checker01.py`.
///
/// Demonstrates a custom widget that renders an 8×8 checkerboard
/// by generating colored segments directly (analogous to Python's
/// `render_line`/`Strip` approach).
use rich_rs::{Console, ConsoleOptions, Segment, Segments, Style as RichStyle};
use textual::prelude::*;

/// An 8×8 checkerboard widget.
///
/// Each logical square is 4 terminal rows tall and 8 columns wide,
/// giving a 32-row × 64-column board — matching the Python example's
/// `row_index = y // 4` / `Segment(" " * 8, ...)` layout.
struct CheckerBoard;

impl Widget for CheckerBoard {
    fn style_type(&self) -> &'static str {
        "CheckerBoard"
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1) as usize;

        // Python: `Style.parse("on white")` / `Style.parse("on black")` —
        // rich ANSI STANDARD colours (7 / 0), NOT CSS truecolor. Textual's
        // always-on ANSI→truecolor paint filter then maps them through the
        // terminal theme (MONOKAI when dark: white→#c4c5b5, black→#1a1a1a).
        // The Rust runtime applies the same filter at end-of-render, so parse
        // the same rich style strings.
        let white_bg = RichStyle::parse("on white").expect("valid style");
        let black_bg = RichStyle::parse("on black").expect("valid style");

        let mut all_segments: Vec<Segment> = Vec::new();

        // 32 terminal rows (8 logical squares × 4 rows each).
        for y in 0..32usize {
            let row_index = y / 4; // which logical row (0..8)

            if row_index >= 8 {
                // Blank padding line (should not be reached given the loop bound,
                // but kept for fidelity with the Python guard).
                all_segments.push(Segment::new(" ".repeat(width)));
                all_segments.push(Segment::new("\n".to_string()));
                continue;
            }

            let is_odd = row_index % 2; // alternates the starting colour

            // 8 columns of 8 spaces each with alternating colours.
            for column in 0..8usize {
                let style = if (column + is_odd) % 2 == 0 {
                    white_bg.clone()
                } else {
                    black_bg.clone()
                };
                all_segments.push(Segment::styled(" ".repeat(8), style));
            }

            // Newline after each row so the renderer moves to the next line.
            all_segments.push(Segment::new("\n".to_string()));
        }

        Segments::from(all_segments)
    }
}

struct BoardApp;

impl TextualApp for BoardApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(CheckerBoard)
    }
}

fn main() -> Result<()> {
    run_sync(BoardApp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions};

    #[test]
    fn board_app_composes_without_panic() {
        let mut app = BoardApp;
        let _root = app.compose();
    }

    #[test]
    fn checker_board_renders_32_newlines() {
        let console = Console::default();
        let mut options = ConsoleOptions::default();
        options.size = (64, 32);
        options.max_width = 64;
        let board = CheckerBoard;
        let segs = board.render(&console, &options);
        let newlines: usize = segs
            .iter()
            .filter(|s| s.text.contains('\n'))
            .count();
        assert_eq!(newlines, 32);
    }
}
