/// Port of Python Textual `docs/examples/guide/widgets/checker02.py`.
///
/// Renders an 8×8 checkerboard using a custom `CheckerBoard` widget that
/// overrides `render_line` to produce one visual row at a time.
///
/// Python uses `COMPONENT_CLASSES` + `get_component_rich_style()` to allow
/// CSS-driven colour overrides for the two square colours.  The Rust framework
/// exposes `resolve_component_style` only as `pub(crate)`, so the colours are
/// hardcoded here using the same values from the Python DEFAULT_CSS:
///   - white square: #A5BAC9
///   - black square: #004578
///
/// Framework gap: `resolve_component_style` / `get_component_rich_style`
/// equivalent is not yet public API.  Once it is, the hardcoded colours should
/// be replaced by CSS-driven component style resolution.
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;

// ── CSS ──────────────────────────────────────────────────────────────────────
//
// The component-class rules below mirror Python's DEFAULT_CSS.  They are
// loaded into the stylesheet but currently cannot be read back from the widget
// render method (resolve_component_style is crate-internal).  They are
// preserved here for documentation and for when the public API is added.

const CSS: &str = r#"
CheckerBoard .checkerboard--white-square {
    background: #A5BAC9;
}
CheckerBoard .checkerboard--black-square {
    background: #004578;
}
CheckerBoard {
    height: 32;
}
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_hex_color(hex: &str) -> rich_rs::SimpleColor {
    let s = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
    rich_rs::SimpleColor::Rgb { r, g, b }
}

// ── CheckerBoard widget ───────────────────────────────────────────────────────

/// Renders an 8×8 checkerboard.  Each logical square is 8 columns wide and
/// 4 terminal rows tall, mirroring the Python `render_line` logic.
struct CheckerBoard {
    seed: NodeSeed,
}

impl CheckerBoard {
    fn new() -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("checkerboard".to_string());
        Self { seed }
    }
}

impl Widget for CheckerBoard {
    fn style_type(&self) -> &'static str {
        "CheckerBoard"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn set_inline_style(&mut self, style: textual::style::Style) {
        self.seed.styles.style = style;
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Render the full widget by assembling all lines.
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = options.size.1.max(1) as usize;
        let mut out = Segments::new();
        for y in 0..height {
            let line = self.render_line(y, console, options);
            for seg in line {
                out.push(seg);
            }
            out.push(Segment::line());
        }
        out
    }

    /// Render a single visual line at row `y` (widget-local coordinates).
    ///
    /// Mirrors Python:
    ///   row_index = y // 4        # four terminal lines per logical square row
    ///   is_odd    = row_index % 2
    ///   segments  = [Segment(" " * 8, black if (col + is_odd) % 2 else white)
    ///                for col in range(8)]
    fn render_line(&self, y: usize, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1) as usize;

        let row_index = y / 4; // four terminal lines per logical row
        if row_index >= 8 {
            // Beyond the 8×8 board — return a blank line.
            let blank_style = rich_rs::Style::new();
            let mut out = Segments::new();
            out.push(Segment::styled(" ".repeat(width), blank_style));
            return out;
        }

        let is_odd = row_index % 2;

        // Colours from Python DEFAULT_CSS.
        let white_bg = parse_hex_color("#A5BAC9");
        let black_bg = parse_hex_color("#004578");

        let white_style = rich_rs::Style::new().with_bgcolor(white_bg);
        let black_style = rich_rs::Style::new().with_bgcolor(black_bg);

        let mut out = Segments::new();
        for col in 0..8usize {
            let style = if (col + is_odd) % 2 == 0 {
                white_style
            } else {
                black_style
            };
            out.push(Segment::styled(" ".repeat(8), style));
        }
        out
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

struct BoardApp;

impl TextualApp for BoardApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(CheckerBoard::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(BoardApp)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_app_composes_without_panic() {
        let mut app = BoardApp;
        let _root = app.compose();
    }

    #[test]
    fn checkerboard_style_type() {
        let board = CheckerBoard::new();
        assert_eq!(board.style_type(), "CheckerBoard");
    }

    #[test]
    fn checkerboard_not_focusable() {
        let board = CheckerBoard::new();
        assert!(!board.focusable());
    }
}
