/// Port of Python Textual `docs/examples/guide/widgets/checker03.py`.
///
/// Demonstrates a scrollable checkerboard rendered via component-class CSS colors.
///
/// Python structure:
///   - `CheckerBoard(ScrollView)` — custom widget with COMPONENT_CLASSES for
///     "checkerboard--white-square" and "checkerboard--black-square", renders via
///     `render_line(y)` honouring `scroll_offset`, virtual_size = (board*8, board*4)
///   - `BoardApp(App)` — single `CheckerBoard(100)` filling the screen
///
/// Rust mapping:
///   - `CheckerBoardContent` — custom `Widget` that renders the full board
///     content and exposes content_width / layout_height for ScrollView sizing.
///     Colors are hardcoded from Python DEFAULT_CSS because `resolve_component_style`
///     is pub(crate) and not accessible from example crates (framework gap).
///   - Wrapped in `ScrollView::new(CheckerBoardContent::new(100))` in the app's
///     `compose()` so scrolling works.
///
/// Framework gaps:
///   - Python `CheckerBoard` subclasses `ScrollView` directly; Rust has no
///     inheritance, so composition via `ScrollView::new(CheckerBoardContent)`
///     is used instead.
///   - `render_line(y, console, options)` in Rust already receives the
///     widget-local `y` within the full content area (ScrollView handles the
///     offset), so no explicit `scroll_offset` adjustment is needed.
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS — mirrors Python DEFAULT_CSS + component-class color declarations
// ---------------------------------------------------------------------------

const CSS: &str = r#"
CheckerBoardContent .checkerboard--white-square {
    background: #A5BAC9;
}
CheckerBoardContent .checkerboard--black-square {
    background: #004578;
}
"#;

// ---------------------------------------------------------------------------
// CheckerBoardContent — renders the full virtual checker surface
// ---------------------------------------------------------------------------

/// Inner content widget for the checkerboard.
///
/// Each square is 8 columns wide and 4 rows tall. `board_size` squares per
/// side gives a virtual area of `(board_size * 8) × (board_size * 4)` cells.
struct CheckerBoardContent {
    board_size: usize,
}

impl CheckerBoardContent {
    fn new(board_size: usize) -> Self {
        Self { board_size }
    }
}

impl Widget for CheckerBoardContent {
    fn style_type(&self) -> &'static str {
        "CheckerBoardContent"
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.board_size * 8)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(self.board_size * 4)
    }

    /// Render one visual row of the board.
    ///
    /// `y` is the absolute row index within the full content (0 … board_size*4-1).
    /// The `ScrollView` owner passes only the rows it wants rendered, already
    /// accounting for the scroll offset — no manual `scroll_y` arithmetic needed.
    fn render_line(&self, y: usize, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let row_index = y / 4; // four lines per row

        // Blank if beyond board extents.
        if row_index >= self.board_size {
            return vec![Segment::new(" ".repeat(width))].into();
        }

        // Hardcoded colors from Python DEFAULT_CSS.
        //
        // FRAMEWORK GAP: Python uses `get_component_rich_style()` to resolve
        // CSS-declared component class colors at runtime.  In Rust the equivalent
        // `textual::css::resolve_component_style` is pub(crate) only and therefore
        // not accessible from example crates.  The values here reproduce the exact
        // defaults from the Python source; once the framework exposes a public
        // `resolve_component_style` API the CSS block above will take effect.
        let white_style = rich_rs::Style::new()
            .with_bgcolor(rich_rs::SimpleColor::rgb(0xA5, 0xBA, 0xC9));
        let black_style = rich_rs::Style::new()
            .with_bgcolor(rich_rs::SimpleColor::rgb(0x00, 0x45, 0x78));

        let is_odd = row_index % 2;
        // Each square is 8 columns wide; alternate color per column index.
        let segments: Vec<Segment> = (0..self.board_size)
            .map(|column| {
                let style = if (column + is_odd) % 2 == 0 {
                    white_style
                } else {
                    black_style
                };
                Segment::styled(" ".repeat(8), style)
            })
            .collect();

        segments.into()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = self.board_size * 4;
        let mut out = Segments::new();
        for y in 0..height {
            let line = self.render_line(y, console, options);
            out.extend(line);
            if y + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// BoardApp
// ---------------------------------------------------------------------------

struct BoardApp;

impl TextualApp for BoardApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(ScrollView::new(CheckerBoardContent::new(100)))
    }
}

fn main() -> textual::Result<()> {
    run_sync(BoardApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkerboard_content_size() {
        let cb = CheckerBoardContent::new(10);
        assert_eq!(cb.content_width(), Some(80));
        assert_eq!(cb.layout_height(), Some(40));
    }

    #[test]
    fn board_app_composes_without_panic() {
        let mut app = BoardApp;
        let _root = app.compose();
    }
}
