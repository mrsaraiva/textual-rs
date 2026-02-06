use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, fixed_height_from_constraints},
};

#[derive(Clone)]
pub struct Pretty {
    id: WidgetId,
    values: Arc<Mutex<Vec<String>>>,
    layout_width: usize,
    styles: WidgetStyles,
}

impl Pretty {
    pub fn new(values: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id: WidgetId::new(),
            values,
            layout_width: 1,
            styles: WidgetStyles::default(),
        }
    }

    fn quote(value: &str) -> String {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for ch in value.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                other => out.push(other),
            }
        }
        out.push('"');
        out
    }

    fn punctuation_style(&self) -> rich_rs::Style {
        crate::css::resolve_component_style(self, &["pretty--punct"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new)
    }

    fn string_style(&self) -> rich_rs::Style {
        crate::css::resolve_component_style(self, &["pretty--string"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new)
    }

    fn empty_style(&self) -> rich_rs::Style {
        crate::css::resolve_component_style(self, &["pretty--empty"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new)
    }

    fn inline_width(values: &[String]) -> usize {
        if values.is_empty() {
            return 2;
        }
        let quoted: Vec<String> = values.iter().map(|v| Self::quote(v)).collect();
        rich_rs::cell_len(&format!("[ {} ]", quoted.join(", "))).max(1)
    }
}

impl Widget for Pretty {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn style_type(&self) -> &'static str {
        "Pretty"
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.layout_width = usize::from(width).max(1);
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let punct_style = self.punctuation_style();
        let string_style = self.string_style();

        if values.is_empty() {
            let line = adjust_line_length_no_bg(
                &[Segment::styled("[]".to_string(), self.empty_style())],
                width,
            );
            let mut out = Segments::new();
            out.extend(line);
            return out;
        }

        let inline_width = Self::inline_width(&values);
        let mut out = Segments::new();
        if inline_width <= width {
            out.push(Segment::styled("[ ".to_string(), punct_style));
            for (idx, value) in values.iter().enumerate() {
                if idx > 0 {
                    out.push(Segment::styled(", ".to_string(), punct_style));
                }
                out.push(Segment::styled(Self::quote(value), string_style));
            }
            out.push(Segment::styled(" ]".to_string(), punct_style));
            let lines = Segment::split_and_crop_lines(out, width, None, true, false);
            let mut flattened = Segments::new();
            if let Some(line) = lines.into_iter().next() {
                flattened.extend(adjust_line_length_no_bg(&line, width));
            }
            return flattened;
        }

        let mut rows: Vec<Vec<Segment>> = Vec::new();
        rows.push(adjust_line_length_no_bg(
            &[Segment::styled("[".to_string(), punct_style)],
            width,
        ));
        for (idx, value) in values.iter().enumerate() {
            let comma = if idx + 1 < values.len() { "," } else { "" };
            rows.push(adjust_line_length_no_bg(
                &[
                    Segment::styled("  ".to_string(), punct_style),
                    Segment::styled(Self::quote(value), string_style),
                    Segment::styled(comma.to_string(), punct_style),
                ],
                width,
            ));
        }
        rows.push(adjust_line_length_no_bg(
            &[Segment::styled("]".to_string(), punct_style)],
            width,
        ));

        let line_count = rows.len();
        for (idx, row) in rows.into_iter().enumerate() {
            out.extend(row);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn content_width(&self) -> Option<usize> {
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let max_item_width = values
            .iter()
            .map(|value| rich_rs::cell_len(&Self::quote(value)).saturating_add(4))
            .max()
            .unwrap_or(2);
        Some(Self::inline_width(&values).max(max_item_width).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        if values.is_empty() {
            return Some(1);
        }
        let inline_width = Self::inline_width(&values);
        if inline_width <= self.layout_width.max(1) {
            Some(1)
        } else {
            Some(values.len().saturating_add(2))
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Pretty {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
