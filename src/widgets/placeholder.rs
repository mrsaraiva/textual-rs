use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::style::Color;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// The variant determines what text a placeholder displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderVariant {
    /// Shows the label or widget identifier.
    Default,
    /// Shows the WxH dimensions.
    Size,
    /// Shows Lorem Ipsum text.
    Text,
}

impl PlaceholderVariant {
    fn next(self) -> Self {
        match self {
            PlaceholderVariant::Default => PlaceholderVariant::Size,
            PlaceholderVariant::Size => PlaceholderVariant::Text,
            PlaceholderVariant::Text => PlaceholderVariant::Default,
        }
    }

    fn class_name(self) -> &'static str {
        match self {
            PlaceholderVariant::Default => "-default",
            PlaceholderVariant::Size => "-size",
            PlaceholderVariant::Text => "-text",
        }
    }
}

const PLACEHOLDER_COLORS: &[&str] = &[
    "#881177", "#aa3355", "#cc6666", "#ee9944", "#eedd00", "#99dd55", "#44dd88", "#22ccbb",
    "#00bbcc", "#0099cc", "#3366bb", "#663399",
];

const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam feugiat ac elit sit amet accumsan. Suspendisse bibendum nec libero quis gravida. Phasellus id eleifend ligula. Nullam imperdiet sem tellus, sed vehicula nisl faucibus sit amet. Praesent iaculis tempor ultricies. Sed lacinia, tellus id rutrum lacinia, sapien sapien congue mauris, sit amet pellentesque quam quam vel nisl. Curabitur vulputate erat pellentesque mauris posuere, non dictum risus mattis.";

/// Global counter for assigning cycling background colors to consecutive placeholders.
static COLOR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A placeholder widget for prototyping layouts.
///
/// Shows a colored area with identifying text. Cycles through variants on click.
/// Each new instance gets the next color from a rotating palette.
#[derive(Debug, Clone)]
pub struct Placeholder {
    id: WidgetId,
    label: String,
    variant: PlaceholderVariant,
    color_index: usize,
    /// Cached content-box dimensions from the last layout pass.
    last_width: usize,
    last_height: usize,
    hovered: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Placeholder {
    pub fn new(label: impl Into<String>) -> Self {
        let color_index = COLOR_COUNTER.fetch_add(1, Ordering::Relaxed) % PLACEHOLDER_COLORS.len();
        let label = label.into();
        Self {
            id: WidgetId::new(),
            label,
            variant: PlaceholderVariant::Default,
            color_index,
            last_width: 0,
            last_height: 0,
            hovered: false,
            classes: vec!["placeholder".to_string(), "-default".to_string()],
            styles: WidgetStyles::default(),
        }
        .apply_bg_color()
    }

    pub fn with_variant(mut self, variant: PlaceholderVariant) -> Self {
        self.variant = variant;
        self.rebuild_classes();
        self
    }

    pub fn variant(&self) -> PlaceholderVariant {
        self.variant
    }

    pub fn cycle_variant(&mut self) {
        self.variant = self.variant.next();
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        self.classes = vec![
            "placeholder".to_string(),
            self.variant.class_name().to_string(),
        ];
    }

    fn apply_bg_color(mut self) -> Self {
        let hex = PLACEHOLDER_COLORS[self.color_index];
        if let Some(color) = crate::style::parse_color_like(hex) {
            // Apply at 50% opacity to match Python Textual's `background: {color} 50%`.
            let bg = Color::rgba(color.r, color.g, color.b, 128);
            self.styles.set_bg(bg);
        }
        self
    }

    fn render_text(&self, width: usize, height: usize) -> String {
        match self.variant {
            PlaceholderVariant::Default => {
                if self.label.is_empty() {
                    "Placeholder".to_string()
                } else {
                    self.label.clone()
                }
            }
            PlaceholderVariant::Size => {
                format!("{} x {}", width, height)
            }
            PlaceholderVariant::Text => {
                // Repeat the lorem ipsum a few times to fill larger placeholders.
                let mut text = String::new();
                for i in 0..5 {
                    if i > 0 {
                        text.push_str("  ");
                    }
                    text.push_str(LOREM_IPSUM);
                }
                text
            }
        }
    }
}

impl Widget for Placeholder {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        false
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width as usize;
        self.last_height = height as usize;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.cycle_variant();
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let text = self.render_text(width, height);
        let mut out = Segments::new();

        let style = crate::css::resolve_component_style(self, &["placeholder"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        match self.variant {
            PlaceholderVariant::Text => {
                // Word-wrap the text to fill the area.
                let lines = word_wrap(&text, width);
                for row in 0..height {
                    let content = lines.get(row).map(|s| s.as_str()).unwrap_or("");
                    let line = rich_rs::set_cell_size(content, width);
                    out.push(Segment::styled(line, style));
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
            _ => {
                // Center the text both horizontally and vertically.
                let text_width = rich_rs::cell_len(&text).min(width);
                let vert_pad = height.saturating_sub(1) / 2;

                for row in 0..height {
                    if row == vert_pad {
                        let left = width.saturating_sub(text_width) / 2;
                        let right = width.saturating_sub(text_width + left);
                        let line = format!(
                            "{}{}{}",
                            " ".repeat(left),
                            rich_rs::set_cell_size(&text, text_width),
                            " ".repeat(right)
                        );
                        out.push(Segment::styled(line, style));
                    } else {
                        out.push(Segment::styled(" ".repeat(width), style));
                    }
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Placeholder"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Placeholder {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Simple word-wrap that breaks text on spaces to fit within `width` cells.
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for word in text.split_whitespace() {
        let word_len = rich_rs::cell_len(word);
        if current.is_empty() {
            current = word.to_string();
            current_len = word_len;
        } else if current_len + 1 + word_len <= width {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
        } else {
            lines.push(current);
            current = word.to_string();
            current_len = word_len;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
