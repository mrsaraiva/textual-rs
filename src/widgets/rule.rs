use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{
    Widget, WidgetId, WidgetStyles,
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
    id: WidgetId,
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
            id: WidgetId::new(),
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
}

impl Widget for Rule {
    fn id(&self) -> WidgetId {
        self.id
    }

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
