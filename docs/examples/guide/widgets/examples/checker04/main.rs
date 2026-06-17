/// Port of Python Textual `docs/examples/guide/widgets/checker04.py`.
///
/// A scrollable checkerboard with a cursor highlight that follows the mouse.
/// The board has `board_size * 8` columns and `board_size * 4` rows of virtual
/// content; a `ScrollView` handles panning.
///
/// Python structure:
///   - `CheckerBoard(ScrollView)` — custom widget that tracks cursor via
///     `cursor_square = var(Offset(0, 0))`, renders via `render_line(y)`,
///     and sets `virtual_size = Size(board_size*8, board_size*4)`.
///   - `BoardApp(App)` — single `CheckerBoard(100)` filling the screen.
///
/// Rust mapping:
///   - `CheckerBoard` — custom `Widget` with `board_size`, `cursor_col`,
///     `cursor_row`. Reports virtual size via `content_width()` /
///     `layout_height()` so `ScrollView` can drive scrollbars.
///   - Wrapped in `ScrollView::new(CheckerBoard::new(100))` in `compose()`.
///   - `on_mouse_move(x, y)` receives content-local coordinates from the
///     runtime (which accounts for scroll offset), so `x / 8` and `y / 4`
///     directly give the cursor square — mirroring Python's
///     `event.offset + self.scroll_offset` arithmetic.
///
/// Framework gaps:
///   - Python `CheckerBoard` subclasses `ScrollView` directly; Rust has no
///     inheritance, so composition is used instead.
///   - Python `cursor_square = var(...)` triggers partial refresh
///     (`self.refresh(region)`) of old/new cursor squares.  Rust does not yet
///     expose a region-level partial refresh API (`EventCtx` repaint covers
///     the full widget); a full repaint is requested via returning `true` from
///     `on_mouse_move`.
///   - Python component classes (`COMPONENT_CLASSES` + `get_component_rich_style`)
///     let CSS override square colors.  `resolve_component_style` is
///     `pub(crate)` in `textual` and therefore not accessible from examples.
///     Colors are hardcoded to the Python DEFAULT_CSS values.
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
CheckerBoard {
    width: 1fr;
    height: 1fr;
}
"#;

// ---------------------------------------------------------------------------
// Helper: build a rich_rs::Style with an RGB background color.
// ---------------------------------------------------------------------------

fn bg_style(r: u8, g: u8, b: u8) -> rich_rs::Style {
    rich_rs::Style::new().with_bgcolor(rich_rs::SimpleColor::Rgb { r, g, b })
}

// ---------------------------------------------------------------------------
// CheckerBoard widget
// ---------------------------------------------------------------------------

struct CheckerBoard {
    board_size: usize,
    /// Column of the highlighted square in board coordinates.
    cursor_col: usize,
    /// Row of the highlighted square in board coordinates.
    cursor_row: usize,
    seed: NodeSeed,
}

impl CheckerBoard {
    fn new(board_size: usize) -> Self {
        Self {
            board_size,
            cursor_col: 0,
            cursor_row: 0,
            seed: NodeSeed::default(),
        }
    }

    /// Return the background style for the square at (col, row).
    ///
    /// Mirrors Python `get_square_style`:
    ///   - cursor square → "checkerboard--cursor-square" (darkred)
    ///   - otherwise     → alternate black/white based on `(col + is_odd) % 2`
    fn square_style(&self, col: usize, row: usize) -> rich_rs::Style {
        if self.cursor_col == col && self.cursor_row == row {
            // "darkred" ≈ #8B0000
            bg_style(0x8B, 0x00, 0x00)
        } else {
            let is_odd = row % 2;
            if (col + is_odd) % 2 == 0 {
                // "checkerboard--white-square": #A5BAC9
                bg_style(0xA5, 0xBA, 0xC9)
            } else {
                // "checkerboard--black-square": #004578
                bg_style(0x00, 0x45, 0x78)
            }
        }
    }
}

impl Widget for CheckerBoard {
    fn style_type(&self) -> &'static str {
        "CheckerBoard"
    }

    /// Render the full virtual board (all `board_size * 4` rows).
    ///
    /// `ScrollView` renders us at our virtual size and then crops/offsets as
    /// needed, so no manual scroll-offset arithmetic is required here.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let board_size = self.board_size;
        let total_rows = board_size * 4;
        let render_w = options.size.0.max(1);

        let mut out = Segments::new();
        for row in 0..total_rows {
            let row_index = row / 4; // 4 terminal lines per logical row

            // Build one line: board_size squares of 8 columns each.
            let mut line: Vec<Segment> = (0..board_size)
                .map(|col| {
                    let style = self.square_style(col, row_index);
                    Segment::styled(" ".repeat(8), style)
                })
                .collect();

            // Trim to render width if narrower than the virtual width.
            let virtual_w = board_size * 8;
            if render_w < virtual_w {
                let mut total = 0usize;
                line.retain_mut(|seg| {
                    if total >= render_w {
                        return false;
                    }
                    let len = seg.text.len();
                    if total + len > render_w {
                        seg.text = seg.text[..render_w - total].to_string().into();
                    }
                    total += seg.text.len();
                    true
                });
            }

            out.extend(line);
            if row + 1 < total_rows {
                out.push(Segment::line());
            }
        }
        out
    }

    /// Report virtual content width so `ScrollView` can size the horizontal
    /// scrollbar and render us at full virtual width.
    fn content_width(&self) -> Option<usize> {
        Some(self.board_size * 8)
    }

    /// Report virtual content height so `ScrollView` can size the vertical
    /// scrollbar.
    fn layout_height(&self) -> Option<usize> {
        Some(self.board_size * 4)
    }

    /// Called by the runtime with content-local coordinates.
    ///
    /// The arena runtime translates screen coordinates to content-local coords
    /// (accounting for the ancestor `ScrollView`'s scroll offset) before
    /// invoking `on_mouse_move`, so `x / 8` and `y / 4` directly yield the
    /// hovered board square — mirroring Python's
    /// `mouse_position = event.offset + self.scroll_offset` logic.
    ///
    /// Returns `true` when the cursor square changes (triggers repaint).
    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let new_col = ((x as usize) / 8).min(self.board_size.saturating_sub(1));
        let new_row = ((y as usize) / 4).min(self.board_size.saturating_sub(1));
        if new_col != self.cursor_col || new_row != self.cursor_row {
            self.cursor_col = new_col;
            self.cursor_row = new_row;
            true
        } else {
            false
        }
    }

    fn set_inline_style(&mut self, style: Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct BoardApp;

impl TextualApp for BoardApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(ScrollView::new(CheckerBoard::new(100)))
    }
}

fn main() -> Result<()> {
    run_sync(BoardApp)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_app_composes_without_panic() {
        let mut app = BoardApp;
        let _root = app.compose();
    }

    #[test]
    fn content_size_matches_board() {
        let board = CheckerBoard::new(100);
        assert_eq!(board.content_width(), Some(800));
        assert_eq!(board.layout_height(), Some(400));
    }

    #[test]
    fn cursor_moves_on_mouse_move() {
        let mut board = CheckerBoard::new(8);
        // Square (col=1, row=0): x in [8, 16), y in [0, 4)
        let changed = board.on_mouse_move(10, 2);
        assert!(changed);
        assert_eq!(board.cursor_col, 1);
        assert_eq!(board.cursor_row, 0);
    }

    #[test]
    fn cursor_no_change_same_square() {
        let mut board = CheckerBoard::new(8);
        board.on_mouse_move(10, 2); // col=1, row=0
        let changed = board.on_mouse_move(15, 3); // still col=1, row=0
        assert!(!changed);
    }

    #[test]
    fn cursor_at_origin_uses_cursor_style() {
        let mut board = CheckerBoard::new(4);
        board.cursor_col = 0;
        board.cursor_row = 0;
        let style = board.square_style(0, 0);
        assert!(style.bgcolor.is_some());
        // Cursor is darkred: r=0x8B, g=0x00, b=0x00
        if let Some(rich_rs::SimpleColor::Rgb { r, g, b }) = style.bgcolor {
            assert_eq!(r, 0x8B);
            assert_eq!(g, 0x00);
            assert_eq!(b, 0x00);
        }
    }

    #[test]
    fn checkerboard_alternates_colors() {
        let mut board = CheckerBoard::new(4);
        // Move cursor away from (0,0) so it doesn't interfere.
        board.cursor_col = 3;
        board.cursor_row = 3;
        // Row 0, is_odd=0: col 0 → (0+0)%2==0 → white; col 1 → (1+0)%2==1 → black
        let style_white = board.square_style(0, 0);
        let style_black = board.square_style(1, 0);
        if let (Some(rich_rs::SimpleColor::Rgb { r: rw, .. }), Some(rich_rs::SimpleColor::Rgb { r: rb, .. })) =
            (style_white.bgcolor, style_black.bgcolor)
        {
            assert_eq!(rw, 0xA5); // white square
            assert_eq!(rb, 0x00); // black square
        }
    }
}
