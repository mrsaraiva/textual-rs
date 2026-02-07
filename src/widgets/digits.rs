use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{Widget, WidgetId, WidgetStyles};

/// Characters recognized by the 3×3 digit font.
const DIGITS: &str = " 0123456789+-^x:ABCDEF$£€()";

/// Normal-weight 3×3 glyph table.
///
/// Each recognized character maps to 3 consecutive entries (top / middle / bottom row).
const DIGITS3X3: &[&str] = &[
    // ' ' (space)
    "   ",
    "   ",
    "   ",
    // '0'
    "╭─╮",
    "│ │",
    "╰─╯",
    // '1'
    "╶╮ ",
    " │ ",
    "╶┴╴",
    // '2'
    "╶─╮",
    "┌─┘",
    "╰─╴",
    // '3'
    "╶─╮",
    " ─┤",
    "╶─╯",
    // '4'
    "╷ ╷",
    "╰─┤",
    "  ╵",
    // '5'
    "╭─╴",
    "╰─╮",
    "╶─╯",
    // '6'
    "╭─╴",
    "├─╮",
    "╰─╯",
    // '7'
    "╶─┐",
    "  │",
    "  ╵",
    // '8'
    "╭─╮",
    "├─┤",
    "╰─╯",
    // '9'
    "╭─╮",
    "╰─┤",
    "╶─╯",
    // '+'
    "   ",
    "╶┼╴",
    "   ",
    // '-'
    "   ",
    "╶─╴",
    "   ",
    // '^'
    " ^ ",
    "   ",
    "   ",
    // 'x'
    "   ",
    " × ",
    "   ",
    // ':'
    "   ",
    " : ",
    "   ",
    // 'A'
    "╭─╮",
    "├─┤",
    "╵ ╵",
    // 'B'
    "┌─╮",
    "├─┤",
    "└─╯",
    // 'C'
    "╭─╮",
    "│  ",
    "╰─╯",
    // 'D'
    "┌─╮",
    "│ │",
    "└─╯",
    // 'E'
    "╭─╴",
    "├─ ",
    "╰─╴",
    // 'F'
    "╭─╴",
    "├─ ",
    "╵  ",
    // '$'
    "╭╫╮",
    "╰╫╮",
    "╰╫╯",
    // '£'
    "╭─╮",
    "╪═ ",
    "┷━╸",
    // '€'
    "╭─╮",
    "╪═ ",
    "╰─╯",
    // '('
    "╭╴ ",
    "│  ",
    "╰╴ ",
    // ')'
    " ╶╮",
    "  │",
    " ╶╯",
];

/// Bold-weight 3×3 glyph table (same layout as [`DIGITS3X3`]).
const DIGITS3X3_BOLD: &[&str] = &[
    // ' ' (space)
    "   ",
    "   ",
    "   ",
    // '0'
    "┏━┓",
    "┃ ┃",
    "┗━┛",
    // '1'
    "╺┓ ",
    " ┃ ",
    "╺┻╸",
    // '2'
    "╺━┓",
    "┏━┛",
    "┗━╸",
    // '3'
    "╺━┓",
    " ━┫",
    "╺━┛",
    // '4'
    "╻ ╻",
    "┗━┫",
    "  ╹",
    // '5'
    "┏━╸",
    "┗━┓",
    "╺━┛",
    // '6'
    "┏━╸",
    "┣━┓",
    "┗━┛",
    // '7'
    "╺━┓",
    "  ┃",
    "  ╹",
    // '8'
    "┏━┓",
    "┣━┫",
    "┗━┛",
    // '9'
    "┏━┓",
    "┗━┫",
    "╺━┛",
    // '+'
    "   ",
    "╺╋╸",
    "   ",
    // '-'
    "   ",
    "╺━╸",
    "   ",
    // '^'
    " ^ ",
    "   ",
    "   ",
    // 'x'
    "   ",
    " × ",
    "   ",
    // ':'
    "   ",
    " : ",
    "   ",
    // 'A'
    "╭─╮",
    "├─┤",
    "╵ ╵",
    // 'B'
    "┌─╮",
    "├─┤",
    "└─╯",
    // 'C'
    "╭─╮",
    "│  ",
    "╰─╯",
    // 'D'
    "┌─╮",
    "│ │",
    "└─╯",
    // 'E'
    "╭─╴",
    "├─ ",
    "╰─╴",
    // 'F'
    "╭─╴",
    "├─ ",
    "╵  ",
    // '$'
    "╭╫╮",
    "╰╫╮",
    "╰╫╯",
    // '£'
    "╭─╮",
    "╪═ ",
    "┷━╸",
    // '€'
    "╭─╮",
    "╪═ ",
    "╰─╯",
    // '('
    "╭╴ ",
    "│  ",
    "╰╴ ",
    // ')'
    " ╶╮",
    "  │",
    " ╶╯",
];

/// A widget that displays text using a 3×3 Unicode block "font".
///
/// Each recognized character (digits 0-9, hex A-F, currency symbols, operators, etc.)
/// is rendered as a 3-cell-wide, 3-row-tall glyph. Unknown characters are rendered as-is
/// in the bottom row with spaces above. Periods are replaced with bullets (`•`).
///
/// The widget is always 3 lines tall. When the CSS `text-style: bold` is applied,
/// the bold glyph table is used automatically.
pub struct Digits {
    id: WidgetId,
    value: String,
    styles: WidgetStyles,
}

impl Digits {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            value: value.into(),
            styles: WidgetStyles::default(),
        }
    }

    /// Get the current display value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Set a new display value.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
    }

    /// Set a new display value (alias matching Python's `update` method).
    pub fn update(&mut self, value: impl Into<String>) {
        self.set_value(value);
    }

    /// Calculate the display width for a given text.
    ///
    /// Known characters occupy 3 cells each; unknown characters occupy 1 cell.
    pub fn get_width(text: &str) -> usize {
        text.chars()
            .map(|ch| if DIGITS.contains(ch) { 3 } else { 1 })
            .sum()
    }

    /// Render the 3×3 digit glyphs into three string rows.
    fn render_rows(&self, bold: bool) -> [String; 3] {
        let table = if bold { DIGITS3X3_BOLD } else { DIGITS3X3 };
        let mut rows = [String::new(), String::new(), String::new()];

        // Replace '.' with '•' as Python does
        let text: String = self.value.replace('.', "•");

        for ch in text.chars() {
            if let Some(pos) = DIGITS.chars().position(|c| c == ch) {
                let base = pos * 3;
                // ljust(3): the table entries are already 3 display-columns but we
                // pad in case a glyph row is shorter than 3 chars.
                for (i, row) in rows.iter_mut().enumerate() {
                    let glyph_row = table[base + i];
                    row.push_str(glyph_row);
                    // Pad to 3 display columns if needed
                    let cell_len = rich_rs::cell_len(glyph_row);
                    for _ in cell_len..3 {
                        row.push(' ');
                    }
                }
            } else {
                // Unknown character: spaces on top two rows, character on bottom
                rows[0].push(' ');
                rows[1].push(' ');
                rows[2].push(ch);
            }
        }

        rows
    }
}

impl Widget for Digits {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        // Detect bold from resolved CSS style
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let bold = resolved.bold == Some(true);
        let rich_style = resolved.to_rich().unwrap_or_else(rich_rs::Style::new);

        let rows = self.render_rows(bold);

        let mut out = Segments::new();
        for (i, row) in rows.iter().enumerate() {
            out.push(Segment::styled(row.clone(), rich_style));
            if i < 2 {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        Some(3)
    }

    fn content_width(&self) -> Option<usize> {
        let width = Self::get_width(&self.value);
        Some(width.max(1))
    }

    fn style_type(&self) -> &'static str {
        "Digits"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Digits {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
