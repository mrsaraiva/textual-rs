use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::style::{Color, parse_color_like};

use super::{Widget, WidgetId, WidgetStyles};

#[derive(Clone)]
pub struct Pretty {
    id: WidgetId,
    values: Arc<Mutex<Vec<String>>>,
    styles: WidgetStyles,
}

impl Pretty {
    pub fn new(values: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id: WidgetId::new(),
            values,
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
}

impl Widget for Pretty {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn style_type(&self) -> &'static str {
        "Pretty"
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());

        if values.is_empty() {
            let mut out = Segments::new();
            out.push(Segment::new("[]".to_string()));
            return out;
        }

        // Approximate Textual's `Pretty` output (single line list with quoted strings).
        // We also apply a simple "string" color (green) to mirror the Python demo.
        let string_color = parse_color_like("$success").unwrap_or(Color::rgb(0, 200, 0));
        let string_style = rich_rs::Style::new().with_color(string_color.to_simple_opaque());

        let mut out = Segments::new();
        out.push(Segment::new("[ ".to_string()));
        for (idx, value) in values.iter().enumerate() {
            if idx > 0 {
                out.push(Segment::new(", ".to_string()));
            }
            out.push(Segment::styled(Self::quote(value), string_style));
        }
        out.push(Segment::new(" ]".to_string()));

        // Clamp to available width; this is a demo helper, not a full pretty-printer.
        let lines = Segment::split_and_crop_lines(out, width, None, true, false);
        let mut flattened = Segments::new();
        if let Some(line) = lines.into_iter().next() {
            flattened.extend(line);
        }
        flattened
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}
