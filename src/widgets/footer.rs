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

    fn component_style(&self, classes: &[&str], fallback: rich_rs::Style) -> rich_rs::Style {
        let style = crate::css::resolve_component_style(self, classes);
        if style.is_empty() {
            fallback
        } else {
            style.to_rich().unwrap_or(fallback)
        }
    }

    fn base_style(&self) -> rich_rs::Style {
        self.component_style(&["footer"], rich_rs::Style::new())
    }

    fn key_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--key"], self.base_style().with_bold(true))
    }

    fn description_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--description"], self.base_style())
    }

    fn command_palette_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--command-palette"], self.description_style())
    }

    fn render_binding(
        &self,
        binding: &FooterBinding,
        key_style: rich_rs::Style,
        description_style: rich_rs::Style,
    ) -> Vec<Segment> {
        let mut out = Vec::new();
        let key_text = if self.compact {
            binding.key.clone()
        } else {
            format!(" {}", binding.key)
        };
        out.push(Segment::styled(key_text, key_style));
        if binding.description.is_empty() {
            if !self.compact {
                out.push(Segment::styled(" ".to_string(), description_style));
            }
        } else {
            out.push(Segment::styled(
                format!(" {}", binding.description),
                description_style,
            ));
        }
        out
    }

    fn render_group(
        &self,
        group_label: &str,
        group_bindings: &[FooterBinding],
        key_style: rich_rs::Style,
        description_style: rich_rs::Style,
        base_style: rich_rs::Style,
    ) -> Vec<Segment> {
        let mut out = Vec::new();
        let key_separator = if self.compact { " " } else { "  " };
        for (index, binding) in group_bindings.iter().enumerate() {
            if index > 0 {
                out.push(Segment::styled(key_separator.to_string(), base_style));
            }
            let mut key_only = binding.clone();
            key_only.description.clear();
            out.extend(self.render_binding(&key_only, key_style, description_style));
        }
        out.push(Segment::styled(
            format!(" {}", group_label),
            description_style,
        ));
        out
    }

    fn split_bindings(&self) -> (Vec<LeftBindingItem>, Option<FooterBinding>) {
        let mut left_bindings = Vec::new();
        let mut palette = None::<FooterBinding>;
        for binding in &self.bindings {
            if binding.group.as_deref() == Some("command_palette") {
                palette = Some(binding.clone());
            } else {
                left_bindings.push(binding.clone());
            }
        }

        let mut left_items = Vec::new();
        let mut index = 0;
        while index < left_bindings.len() {
            let binding = &left_bindings[index];
            let Some(group_name) = binding.group.clone() else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
                index += 1;
                continue;
            };

            let mut run_end = index + 1;
            while run_end < left_bindings.len()
                && left_bindings[run_end].group.as_deref() == Some(group_name.as_str())
            {
                run_end += 1;
            }
            if run_end - index > 1 {
                left_items.push(LeftBindingItem::Grouped {
                    label: group_name,
                    bindings: left_bindings[index..run_end].to_vec(),
                });
            } else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
            }
            index = run_end;
        }

        (left_items, palette)
    }
}

#[derive(Debug, Clone)]
enum LeftBindingItem {
    Single(FooterBinding),
    Grouped {
        label: String,
        bindings: Vec<FooterBinding>,
    },
}

impl Widget for Footer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let base_style = self.base_style();
        let key_style = self.key_style();
        let description_style = self.description_style();
        let command_palette_style = self.command_palette_style();

        let (left_bindings, palette) = self.split_bindings();

        let separator = if self.compact { " " } else { "   " };
        let mut left_segments = Vec::new();
        for (index, binding) in left_bindings.iter().enumerate() {
            if index > 0 {
                left_segments.push(Segment::styled(separator.to_string(), base_style));
            }
            match binding {
                LeftBindingItem::Single(binding) => {
                    left_segments.extend(self.render_binding(
                        binding,
                        key_style,
                        description_style,
                    ));
                }
                LeftBindingItem::Grouped { label, bindings } => {
                    left_segments.extend(self.render_group(
                        label,
                        bindings,
                        key_style,
                        description_style,
                        base_style,
                    ));
                }
            }
        }

        let mut line_segments = left_segments;
        if let Some(palette_binding) = palette {
            let mut right_segments =
                self.render_binding(&palette_binding, key_style, command_palette_style);
            // Keep command palette hint docked at the right with a subtle separator.
            let right_separator = if self.compact { " " } else { "  " };
            right_segments.insert(
                0,
                Segment::styled(right_separator.to_string(), command_palette_style),
            );

            let left_width = Segment::get_line_length(&line_segments);
            let right_width = Segment::get_line_length(&right_segments);
            if left_width + right_width < width {
                line_segments.push(Segment::styled(
                    " ".repeat(width - left_width - right_width),
                    base_style,
                ));
                line_segments.extend(right_segments);
            } else {
                line_segments.extend(right_segments);
            }
        }

        let rendered = if line_segments.is_empty() {
            Text::plain(String::new()).render(console, options)
        } else {
            let mut out = Segments::new();
            out.extend(line_segments);
            out
        };
        let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let mut out = Segments::new();
        if let Some(line) = split.first() {
            out.extend(adjust_line_length_no_bg(line, width));
        } else {
            out.push(Segment::styled(" ".repeat(width), base_style));
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
