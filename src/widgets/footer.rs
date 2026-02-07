use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::event::{Event, EventCtx};

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetId, WidgetStyles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterBinding {
    pub key: String,
    pub description: String,
    pub group: Option<String>,
}

impl FooterBinding {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            group: None,
        }
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct Footer {
    id: WidgetId,
    bindings: Vec<FooterBinding>,
    compact: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Footer {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            bindings: Vec::new(),
            compact: false,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_binding(mut self, key: impl Into<String>, description: impl Into<String>) -> Self {
        self.bindings.push(FooterBinding::new(key, description));
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    fn line_text(&self) -> String {
        if self.bindings.is_empty() {
            return String::new();
        }

        let separator = if self.compact { " " } else { "   " };
        let mut line = String::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            if index > 0 {
                line.push_str(separator);
            }
            line.push(' ');
            line.push_str(&binding.key);
            line.push(' ');
            line.push_str(&binding.description);
        }
        line
    }
}

impl Widget for Footer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut left_bindings = Vec::new();
        let mut palette = None::<FooterBinding>;
        for binding in &self.bindings {
            if binding.group.as_deref() == Some("command_palette") {
                palette = Some(binding.clone());
            } else {
                left_bindings.push(binding.clone());
            }
        }
        let text = if let Some(palette) = palette {
            let mut left = Footer {
                id: self.id,
                bindings: left_bindings,
                compact: self.compact,
                classes: self.classes.clone(),
                styles: self.styles.clone(),
            }
            .line_text();
            let right = format!(" {} {}", palette.key, palette.description);
            let left_width = rich_rs::cell_len(&left);
            let right_width = rich_rs::cell_len(&right);
            if left_width + right_width < width {
                let pad = width.saturating_sub(left_width + right_width);
                left.push_str(&" ".repeat(pad));
                left.push_str(&right);
                Text::plain(left)
            } else {
                Text::plain(format!("{left}{right}"))
            }
        } else {
            Text::plain(self.line_text())
        };
        let rendered = text.render(console, options);
        let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let mut out = Segments::new();
        if let Some(line) = split.first() {
            out.extend(adjust_line_length_no_bg(line, width));
        } else {
            out.push(Segment::new(" ".repeat(width)));
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::BindingsChanged(bindings) = event {
            let next = bindings
                .iter()
                .filter(|hint| hint.show)
                .map(|hint| {
                    let mut binding = FooterBinding::new(
                        hint.key_display.clone().unwrap_or_else(|| hint.key.clone()),
                        hint.description.clone(),
                    );
                    binding.group = hint.group.clone();
                    binding
                })
                .collect::<Vec<_>>();
            if next != self.bindings {
                self.bindings = next;
                ctx.request_repaint();
            }
        }
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
