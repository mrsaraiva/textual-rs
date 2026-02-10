use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetId, WidgetStyles};

#[derive(Debug, Clone)]
pub struct Header {
    id: WidgetId,
    title: String,
    subtitle: Option<String>,
    tall: bool,
    icon: String,
    icon_hover: bool,
    icon_width: usize,
    clock_width: usize,
    show_clock: bool,
    time_format: String,
    last_clock_second: Arc<AtomicU64>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Header {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            title: "textual-rs".to_string(),
            subtitle: None,
            tall: false,
            icon: "⭘".to_string(),
            icon_hover: false,
            icon_width: 8,
            clock_width: 10,
            show_clock: false,
            time_format: "%X".to_string(),
            last_clock_second: Arc::new(AtomicU64::new(0)),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn clear_subtitle(mut self) -> Self {
        self.subtitle = None;
        self
    }

    pub fn tall(mut self, tall: bool) -> Self {
        self.tall = tall;
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn show_clock(mut self, show_clock: bool) -> Self {
        self.show_clock = show_clock;
        self
    }

    pub fn time_format(mut self, time_format: impl Into<String>) -> Self {
        self.time_format = time_format.into();
        self
    }

    fn line_text(&self) -> String {
        match &self.subtitle {
            Some(subtitle) if !subtitle.is_empty() => format!(" {} — {}", self.title, subtitle),
            _ => format!(" {}", self.title),
        }
    }

    fn current_clock_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn format_clock(&self, epoch_seconds: u64) -> String {
        let day_seconds = epoch_seconds % 86_400;
        let hours = day_seconds / 3_600;
        let minutes = (day_seconds % 3_600) / 60;
        let seconds = day_seconds % 60;

        let h = format!("{hours:02}");
        let m = format!("{minutes:02}");
        let s = format!("{seconds:02}");
        let hms = format!("{h}:{m}:{s}");

        let mut formatted = self.time_format.clone();
        formatted = formatted.replace("%X", &hms);
        formatted = formatted.replace("%T", &hms);
        formatted = formatted.replace("%H", &h);
        formatted = formatted.replace("%M", &m);
        formatted = formatted.replace("%S", &s);
        if formatted == self.time_format {
            hms
        } else {
            formatted
        }
    }

    fn component_style(&self, classes: &[&str]) -> rich_rs::Style {
        let style = crate::css::resolve_component_style(self, classes);
        if style.is_empty() {
            rich_rs::Style::new()
        } else {
            style.to_rich().unwrap_or_else(rich_rs::Style::new)
        }
    }
}

impl Widget for Header {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn style_type(&self) -> &'static str {
        "Header"
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, x: u16, _y: u16) -> bool {
        let new_hover = (x as usize) < self.icon_width;
        if new_hover != self.icon_hover {
            self.icon_hover = new_hover;
            return true;
        }
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseUp(mouse) => {
                if mouse.target != Some(self.id) {
                    return;
                }
                if (mouse.x as usize) < self.icon_width {
                    // Parity with Python Header: icon click is handled separately and
                    // shouldn't toggle header height.
                    ctx.set_handled();
                    return;
                }
                self.tall = !self.tall;
                ctx.post_message(self.id, Message::HeaderToggled { tall: self.tall });
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::AppFocus(false) => {
                if self.icon_hover {
                    self.icon_hover = false;
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line_text = self.line_text();
        let icon_text = format!(" {} ", self.icon);
        let icon_core_width = rich_rs::cell_len(&icon_text);
        let icon_text = if icon_core_width >= self.icon_width {
            rich_rs::set_cell_size(&icon_text, self.icon_width)
        } else {
            format!(
                "{}{}",
                icon_text,
                " ".repeat(self.icon_width.saturating_sub(icon_core_width))
            )
        };
        let clock_seconds = Self::current_clock_seconds();
        self.last_clock_second
            .store(clock_seconds, Ordering::Relaxed);
        let right_label = if self.show_clock {
            self.format_clock(clock_seconds)
        } else {
            String::new()
        };
        let right_text = {
            let right_width = self.clock_width.min(width.saturating_sub(1));
            let clipped = rich_rs::set_cell_size(&right_label, right_width);
            if rich_rs::cell_len(&clipped) >= right_width {
                clipped
            } else {
                format!(
                    "{}{}",
                    clipped,
                    " ".repeat(right_width - rich_rs::cell_len(&clipped))
                )
            }
        };
        let center_width = width
            .saturating_sub(self.icon_width.min(width))
            .saturating_sub(rich_rs::cell_len(&right_text))
            .max(1);
        let title_width = rich_rs::cell_len(&line_text).min(center_width);
        let left_pad = center_width.saturating_sub(title_width) / 2;
        let right_pad = center_width.saturating_sub(title_width + left_pad);
        let center_text = format!(
            "{}{}{}",
            " ".repeat(left_pad),
            rich_rs::set_cell_size(&line_text, title_width),
            " ".repeat(right_pad)
        );

        let icon_style = if self.icon_hover {
            self.component_style(&["header--icon", "-hover"])
        } else {
            self.component_style(&["header--icon"])
        };
        let title_style = self.component_style(&["header--title"]);
        let clock_style = self.component_style(&["header--clock"]);

        let mut first_line = Vec::new();
        first_line.push(Segment::styled(icon_text, icon_style));
        first_line.push(Segment::styled(center_text, title_style));
        first_line.push(Segment::styled(right_text, clock_style));
        let first_line = adjust_line_length_no_bg(&first_line, width);

        let mut out = Segments::new();
        out.extend(first_line);
        if self.tall {
            for _ in 0..2 {
                out.push(Segment::line());
                out.push(Segment::new(" ".repeat(width)));
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(if self.tall {
            3
        } else {
            1
        }))
    }

    fn style_classes(&self) -> &[String] {
        if self.tall {
            static TALL: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
            TALL.get_or_init(|| vec!["-tall".to_string()])
        } else if self.classes.is_empty() {
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

    fn is_active(&self) -> bool {
        if !self.show_clock {
            return false;
        }
        let current = Self::current_clock_seconds();
        current != self.last_clock_second.load(Ordering::Relaxed)
    }
}

impl Renderable for Header {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseUpEvent;

    #[test]
    fn header_body_click_toggles_tall_and_emits_message() {
        let mut header = Header::new();
        let mut ctx = EventCtx::default();
        let id = header.id();
        header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 9,
                y: 0,
                screen_x: 9,
                screen_y: 0,
                target: Some(id),
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, id);
        assert!(matches!(
            messages[0].message,
            Message::HeaderToggled { tall: true }
        ));
    }

    #[test]
    fn header_icon_click_does_not_emit_toggle_message() {
        let mut header = Header::new();
        let mut ctx = EventCtx::default();
        let id = header.id();
        header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: Some(id),
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.take_messages().is_empty());
    }
}
