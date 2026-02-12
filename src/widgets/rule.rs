use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// Orientation of a rule separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOrientation {
    Horizontal,
    Vertical,
}

/// Line drawing style for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Ascii,
    Blank,
    Dashed,
    Double,
    Heavy,
    Hidden,
    None,
    Solid,
    Thick,
}

impl LineStyle {
    fn horizontal_char(self) -> &'static str {
        match self {
            LineStyle::Ascii => "-",
            LineStyle::Blank | LineStyle::Hidden | LineStyle::None => " ",
            LineStyle::Dashed => "╍",
            LineStyle::Double => "═",
            LineStyle::Heavy => "━",
            LineStyle::Solid => "─",
            LineStyle::Thick => "█",
        }
    }

    fn vertical_char(self) -> &'static str {
        match self {
            LineStyle::Ascii => "|",
            LineStyle::Blank | LineStyle::Hidden | LineStyle::None => " ",
            LineStyle::Dashed => "╏",
            LineStyle::Double => "║",
            LineStyle::Heavy => "┃",
            LineStyle::Solid => "│",
            LineStyle::Thick => "█",
        }
    }
}

/// A rule widget to separate content, similar to an `<hr>` HTML tag.
///
/// Renders a horizontal or vertical line using box-drawing characters.
/// Not focusable or interactive.
#[derive(Debug, Clone)]
pub struct Rule {
    orientation: RuleOrientation,
    line_style: LineStyle,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Rule {
    pub fn new(orientation: RuleOrientation) -> Self {
        let class = match orientation {
            RuleOrientation::Horizontal => "rule--horizontal",
            RuleOrientation::Vertical => "rule--vertical",
        };
        Self {
            orientation,
            line_style: LineStyle::Solid,
            classes: vec!["rule".to_string(), class.to_string()],
            styles: WidgetStyles::default(),
        }
    }

    /// Create a horizontal rule (default).
    pub fn horizontal() -> Self {
        Self::new(RuleOrientation::Horizontal)
    }

    /// Create a vertical rule.
    pub fn vertical() -> Self {
        Self::new(RuleOrientation::Vertical)
    }

    /// Set the line drawing style.
    pub fn line_style(mut self, style: LineStyle) -> Self {
        self.line_style = style;
        self
    }

    pub fn orientation(&self) -> RuleOrientation {
        self.orientation
    }

    /// Get the current line style.
    pub fn get_line_style(&self) -> LineStyle {
        self.line_style
    }

    /// Dynamically change the orientation (reactive setter).
    ///
    /// Updates the CSS class to match, mirroring Python Textual's reactive `orientation` attribute.
    pub fn set_orientation(&mut self, orientation: RuleOrientation) {
        if self.orientation == orientation {
            return;
        }
        // Remove old orientation class, add new one
        let old_class = match self.orientation {
            RuleOrientation::Horizontal => "rule--horizontal",
            RuleOrientation::Vertical => "rule--vertical",
        };
        let new_class = match orientation {
            RuleOrientation::Horizontal => "rule--horizontal",
            RuleOrientation::Vertical => "rule--vertical",
        };
        self.classes.retain(|c| c != old_class);
        self.classes.push(new_class.to_string());
        self.orientation = orientation;
    }

    /// Dynamically change the line style (reactive setter).
    ///
    /// Mirrors Python Textual's reactive `line_style` attribute.
    pub fn set_line_style(&mut self, style: LineStyle) {
        self.line_style = style;
    }
}

impl Widget for Rule {
    fn focusable(&self) -> bool {
        false
    }

    fn content_width(&self) -> Option<usize> {
        match self.orientation {
            RuleOrientation::Horizontal => None, // expand to fill
            RuleOrientation::Vertical => Some(1),
        }
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(match self.orientation {
            RuleOrientation::Horizontal => Some(1),
            RuleOrientation::Vertical => None, // expand to fill
        })
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let class = match self.orientation {
            RuleOrientation::Horizontal => "rule--horizontal",
            RuleOrientation::Vertical => "rule--vertical",
        };
        let style = crate::css::resolve_component_style(self, &[class])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        match self.orientation {
            RuleOrientation::Horizontal => {
                let ch = self.line_style.horizontal_char();
                let line: String = ch.repeat(width);
                out.push(Segment::styled(line, style));
            }
            RuleOrientation::Vertical => {
                let ch = self.line_style.vertical_char();
                for row in 0..height {
                    let mut text = ch.to_string();
                    if width > 1 {
                        text.push_str(&" ".repeat(width - 1));
                    }
                    out.push(Segment::styled(text, style));
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
        }

        out
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Rule"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Rule {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_default_orientation() {
        let r = Rule::horizontal();
        assert_eq!(r.orientation(), RuleOrientation::Horizontal);
    }

    #[test]
    fn vertical_constructor() {
        let r = Rule::vertical();
        assert_eq!(r.orientation(), RuleOrientation::Vertical);
    }

    #[test]
    fn default_line_style_is_solid() {
        let r = Rule::horizontal();
        assert_eq!(r.get_line_style(), LineStyle::Solid);
    }

    #[test]
    fn builder_line_style() {
        let r = Rule::horizontal().line_style(LineStyle::Dashed);
        assert_eq!(r.get_line_style(), LineStyle::Dashed);
    }

    #[test]
    fn set_line_style_changes() {
        let mut r = Rule::horizontal();
        r.set_line_style(LineStyle::Heavy);
        assert_eq!(r.get_line_style(), LineStyle::Heavy);
    }

    #[test]
    fn set_orientation_updates_classes() {
        let mut r = Rule::horizontal();
        assert!(r.style_classes().iter().any(|c| c == "rule--horizontal"));
        assert!(!r.style_classes().iter().any(|c| c == "rule--vertical"));

        r.set_orientation(RuleOrientation::Vertical);
        assert_eq!(r.orientation(), RuleOrientation::Vertical);
        assert!(r.style_classes().iter().any(|c| c == "rule--vertical"));
        assert!(!r.style_classes().iter().any(|c| c == "rule--horizontal"));
    }

    #[test]
    fn set_orientation_noop_same() {
        let mut r = Rule::horizontal();
        let classes_before: Vec<String> = r.style_classes().to_vec();
        r.set_orientation(RuleOrientation::Horizontal);
        assert_eq!(r.style_classes(), &classes_before[..]);
    }

    #[test]
    fn not_focusable() {
        let r = Rule::horizontal();
        assert!(!r.focusable());
    }

    #[test]
    fn horizontal_content_width_is_none() {
        let r = Rule::horizontal();
        assert_eq!(r.content_width(), None);
    }

    #[test]
    fn vertical_content_width_is_one() {
        let r = Rule::vertical();
        assert_eq!(r.content_width(), Some(1));
    }

    #[test]
    fn horizontal_layout_height_is_one() {
        let r = Rule::horizontal();
        assert_eq!(r.layout_height(), Some(1));
    }

    #[test]
    fn vertical_layout_height_is_none() {
        let r = Rule::vertical();
        assert_eq!(r.layout_height(), None);
    }

    #[test]
    fn style_type_is_rule() {
        let r = Rule::horizontal();
        assert_eq!(r.style_type(), "Rule");
    }

    #[test]
    fn horizontal_line_chars_all_styles() {
        // Verify each line style maps to a non-empty character
        let styles = [
            LineStyle::Ascii,
            LineStyle::Blank,
            LineStyle::Dashed,
            LineStyle::Double,
            LineStyle::Heavy,
            LineStyle::Hidden,
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Thick,
        ];
        for s in styles {
            let ch = s.horizontal_char();
            assert!(
                !ch.is_empty(),
                "horizontal_char for {:?} should not be empty",
                s
            );
        }
    }

    #[test]
    fn vertical_line_chars_all_styles() {
        let styles = [
            LineStyle::Ascii,
            LineStyle::Blank,
            LineStyle::Dashed,
            LineStyle::Double,
            LineStyle::Heavy,
            LineStyle::Hidden,
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Thick,
        ];
        for s in styles {
            let ch = s.vertical_char();
            assert!(
                !ch.is_empty(),
                "vertical_char for {:?} should not be empty",
                s
            );
        }
    }

    #[test]
    fn horizontal_char_specific_values() {
        assert_eq!(LineStyle::Ascii.horizontal_char(), "-");
        assert_eq!(LineStyle::Solid.horizontal_char(), "─");
        assert_eq!(LineStyle::Heavy.horizontal_char(), "━");
        assert_eq!(LineStyle::Dashed.horizontal_char(), "╍");
        assert_eq!(LineStyle::Double.horizontal_char(), "═");
        assert_eq!(LineStyle::Thick.horizontal_char(), "█");
    }

    #[test]
    fn vertical_char_specific_values() {
        assert_eq!(LineStyle::Ascii.vertical_char(), "|");
        assert_eq!(LineStyle::Solid.vertical_char(), "│");
        assert_eq!(LineStyle::Heavy.vertical_char(), "┃");
        assert_eq!(LineStyle::Dashed.vertical_char(), "╏");
        assert_eq!(LineStyle::Double.vertical_char(), "║");
        assert_eq!(LineStyle::Thick.vertical_char(), "█");
    }

    #[test]
    fn round_trip_orientation_switch() {
        let mut r = Rule::horizontal();
        r.set_orientation(RuleOrientation::Vertical);
        r.set_orientation(RuleOrientation::Horizontal);
        assert_eq!(r.orientation(), RuleOrientation::Horizontal);
        assert!(r.style_classes().iter().any(|c| c == "rule--horizontal"));
        assert!(!r.style_classes().iter().any(|c| c == "rule--vertical"));
    }
}
