use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::style::TextAlign;

use super::{Widget, WidgetStyles};

/// Deprecated: use [`TextAlign`] from `crate::style` instead.
///
/// Previously the Digits widget used its own alignment enum. Now it reads
/// `text-align` from the resolved CSS style, so this is a backward-compatible
/// alias kept for code that still references the old name.
#[deprecated(note = "Use crate::style::TextAlign instead")]
pub type DigitsAlign = TextAlign;

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
///
/// Text alignment is controlled via the CSS `text-align` property on the widget's
/// resolved style. Defaults to left-aligned when no CSS rule is set.
pub struct Digits {
    value: String,
    styles: WidgetStyles,
}

impl Digits {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
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
    /// Known characters occupy 3 cells each; unknown characters are measured
    /// via `rich_rs::cell_len` (handles wide CJK and other multi-cell chars).
    pub fn get_width(text: &str) -> usize {
        text.chars()
            .map(|ch| {
                if DIGITS.contains(ch) {
                    3
                } else {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    rich_rs::cell_len(s).max(1)
                }
            })
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

    /// Apply text alignment to rendered rows within the given width.
    fn align_rows(
        rows: [String; 3],
        content_width: usize,
        available_width: usize,
        align: TextAlign,
    ) -> [String; 3] {
        if available_width <= content_width {
            return rows;
        }
        let pad = available_width - content_width;
        let (left_pad, right_pad) = match align {
            TextAlign::Left | TextAlign::Justify => (0, pad),
            TextAlign::Right => (pad, 0),
            TextAlign::Center => {
                let left = pad / 2;
                (left, pad - left)
            }
        };
        let left_spaces: String = " ".repeat(left_pad);
        let right_spaces: String = " ".repeat(right_pad);
        rows.map(|row| format!("{left_spaces}{row}{right_spaces}"))
    }
}

impl Widget for Digits {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        // Detect bold and text-align from resolved CSS style
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let bold = resolved.bold == Some(true);
        let rich_style = resolved.to_rich().unwrap_or_default();
        let align = resolved.text_align.unwrap_or(TextAlign::Left);

        let rows = self.render_rows(bold);
        let content_width = Self::get_width(&self.value);
        let available_width = options.size.0.max(1);
        let rows = Self::align_rows(rows, content_width, available_width, align);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_width_digits() {
        assert_eq!(Digits::get_width("0"), 3);
        assert_eq!(Digits::get_width("12"), 6);
        assert_eq!(Digits::get_width("0123456789"), 30);
    }

    #[test]
    fn get_width_hex() {
        assert_eq!(Digits::get_width("A"), 3);
        assert_eq!(Digits::get_width("ABCDEF"), 18);
    }

    #[test]
    fn get_width_operators() {
        assert_eq!(Digits::get_width("+"), 3);
        assert_eq!(Digits::get_width("-"), 3);
        assert_eq!(Digits::get_width("^"), 3);
        assert_eq!(Digits::get_width("x"), 3);
        assert_eq!(Digits::get_width(":"), 3);
    }

    #[test]
    fn get_width_currency() {
        assert_eq!(Digits::get_width("$"), 3);
        assert_eq!(Digits::get_width("£"), 3);
        assert_eq!(Digits::get_width("€"), 3);
    }

    #[test]
    fn get_width_parens() {
        assert_eq!(Digits::get_width("("), 3);
        assert_eq!(Digits::get_width(")"), 3);
        assert_eq!(Digits::get_width("(0)"), 9);
    }

    #[test]
    fn get_width_unknown_char() {
        assert_eq!(Digits::get_width("z"), 1);
        assert_eq!(Digits::get_width("0z"), 4);
    }

    #[test]
    fn get_width_empty() {
        assert_eq!(Digits::get_width(""), 0);
    }

    #[test]
    fn get_width_space() {
        assert_eq!(Digits::get_width(" "), 3);
    }

    #[test]
    fn render_rows_digit_zero() {
        let d = Digits::new("0");
        let rows = d.render_rows(false);
        assert_eq!(rows[0], "╭─╮");
        assert_eq!(rows[1], "│ │");
        assert_eq!(rows[2], "╰─╯");
    }

    #[test]
    fn render_rows_bold_zero() {
        let d = Digits::new("0");
        let rows = d.render_rows(true);
        assert_eq!(rows[0], "┏━┓");
        assert_eq!(rows[1], "┃ ┃");
        assert_eq!(rows[2], "┗━┛");
    }

    #[test]
    fn render_rows_unknown_char_fallback() {
        let d = Digits::new("z");
        let rows = d.render_rows(false);
        assert_eq!(rows[0], " ");
        assert_eq!(rows[1], " ");
        assert_eq!(rows[2], "z");
    }

    #[test]
    fn render_rows_dot_replaced_with_bullet() {
        let d = Digits::new("1.0");
        let rows = d.render_rows(false);
        assert!(rows[2].contains('•'));
    }

    #[test]
    fn render_rows_multiple_digits() {
        let d = Digits::new("12");
        let rows = d.render_rows(false);
        assert_eq!(rich_rs::cell_len(&rows[0]), 6);
        assert_eq!(rich_rs::cell_len(&rows[1]), 6);
        assert_eq!(rich_rs::cell_len(&rows[2]), 6);
    }

    #[test]
    fn content_width_matches_get_width() {
        let d = Digits::new("42");
        assert_eq!(d.content_width(), Some(Digits::get_width("42")));
    }

    #[test]
    fn content_width_empty_is_at_least_1() {
        let d = Digits::new("");
        assert_eq!(d.content_width(), Some(1));
    }

    #[test]
    fn layout_height_is_3() {
        let d = Digits::new("0");
        assert_eq!(d.layout_height(), Some(3));
    }

    #[test]
    fn style_type_is_digits() {
        let d = Digits::new("0");
        assert_eq!(d.style_type(), "Digits");
    }

    #[test]
    fn set_value_changes_content() {
        let mut d = Digits::new("0");
        d.set_value("99");
        assert_eq!(d.value(), "99");
    }

    #[test]
    fn update_alias_changes_content() {
        let mut d = Digits::new("0");
        d.update("42");
        assert_eq!(d.value(), "42");
    }

    #[test]
    fn align_rows_left_no_change_when_exact_fit() {
        let rows = ["abc".to_string(), "def".to_string(), "ghi".to_string()];
        let aligned = Digits::align_rows(rows.clone(), 3, 3, TextAlign::Left);
        assert_eq!(aligned, rows);
    }

    #[test]
    fn align_rows_left_pads_right() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let aligned = Digits::align_rows(rows, 2, 6, TextAlign::Left);
        assert_eq!(aligned[0], "ab    ");
        assert_eq!(aligned[1], "cd    ");
        assert_eq!(aligned[2], "ef    ");
    }

    #[test]
    fn align_rows_right_pads_left() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let aligned = Digits::align_rows(rows, 2, 6, TextAlign::Right);
        assert_eq!(aligned[0], "    ab");
        assert_eq!(aligned[1], "    cd");
        assert_eq!(aligned[2], "    ef");
    }

    #[test]
    fn align_rows_center_pads_both() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let aligned = Digits::align_rows(rows, 2, 6, TextAlign::Center);
        assert_eq!(aligned[0], "  ab  ");
        assert_eq!(aligned[1], "  cd  ");
        assert_eq!(aligned[2], "  ef  ");
    }

    #[test]
    fn align_rows_center_odd_pad() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let aligned = Digits::align_rows(rows, 2, 5, TextAlign::Center);
        assert_eq!(aligned[0], " ab  ");
        assert_eq!(aligned[1], " cd  ");
        assert_eq!(aligned[2], " ef  ");
    }

    #[test]
    fn align_rows_justify_treated_as_left() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let aligned = Digits::align_rows(rows, 2, 6, TextAlign::Justify);
        assert_eq!(aligned[0], "ab    ");
        assert_eq!(aligned[1], "cd    ");
        assert_eq!(aligned[2], "ef    ");
    }

    #[test]
    fn alignment_defaults_to_left_without_css() {
        let rows = ["ab".to_string(), "cd".to_string(), "ef".to_string()];
        let default_align = TextAlign::Left;
        let aligned = Digits::align_rows(rows, 2, 6, default_align);
        assert_eq!(aligned[0], "ab    ");
    }
}
