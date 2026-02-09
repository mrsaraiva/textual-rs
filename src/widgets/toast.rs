use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

/// Severity level for toast notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Information,
    Warning,
    Error,
}

impl ToastSeverity {
    fn class_name(self) -> &'static str {
        match self {
            ToastSeverity::Information => "-information",
            ToastSeverity::Warning => "-warning",
            ToastSeverity::Error => "-error",
        }
    }
}

/// Default timeout in ticks before a toast auto-dismisses.
const DEFAULT_TIMEOUT: u64 = 60;

/// A notification widget that displays a message with optional title and severity.
///
/// Auto-dismisses after a configurable timeout. Click to dismiss immediately.
/// Not focusable — it's an overlay notification.
#[derive(Debug, Clone)]
pub struct Toast {
    id: WidgetId,
    message: String,
    title: Option<String>,
    severity: ToastSeverity,
    timeout_remaining: u64,
    dismissed: bool,
    hovered: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Toast {
    pub fn new(message: impl Into<String>, severity: ToastSeverity) -> Self {
        let message = message.into();
        Self {
            id: WidgetId::new(),
            message,
            title: None,
            severity,
            timeout_remaining: DEFAULT_TIMEOUT,
            dismissed: false,
            hovered: false,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
        .rebuild_classes()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_timeout(mut self, ticks: u64) -> Self {
        self.timeout_remaining = ticks;
        self
    }

    pub fn severity(&self) -> ToastSeverity {
        self.severity
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn rebuild_classes(mut self) -> Self {
        self.classes = vec!["toast".to_string(), self.severity.class_name().to_string()];
        self
    }

    fn dismiss(&mut self, ctx: &mut EventCtx) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        ctx.post_message(self.id, Message::ToastDismissed);
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn strip_bold_markup(input: &str) -> String {
        input.replace("[b]", "").replace("[/b]", "")
    }

    fn render_line_with_bold_markup(line: &str, width: usize) -> Vec<Segment> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut remaining = line;
        let mut bold = false;

        loop {
            let next_open = remaining.find("[b]");
            let next_close = remaining.find("[/b]");
            let next = match (next_open, next_close) {
                (Some(open), Some(close)) => {
                    if open <= close {
                        Some((open, true))
                    } else {
                        Some((close, false))
                    }
                }
                (Some(open), None) => Some((open, true)),
                (None, Some(close)) => Some((close, false)),
                (None, None) => None,
            };

            let Some((idx, is_open)) = next else {
                if !remaining.is_empty() {
                    if bold {
                        segments.push(Segment::styled(
                            remaining.to_string(),
                            rich_rs::Style::new().with_bold(true),
                        ));
                    } else {
                        segments.push(Segment::new(remaining.to_string()));
                    }
                }
                break;
            };

            if idx > 0 {
                let text = &remaining[..idx];
                if bold {
                    segments.push(Segment::styled(
                        text.to_string(),
                        rich_rs::Style::new().with_bold(true),
                    ));
                } else {
                    segments.push(Segment::new(text.to_string()));
                }
            }

            remaining = if is_open {
                bold = true;
                &remaining[idx + 3..]
            } else {
                bold = false;
                &remaining[idx + 4..]
            };
        }

        if segments.is_empty() {
            segments.push(Segment::new(String::new()));
        }
        adjust_line_length_no_bg(&segments, width.max(1))
    }
}

impl Widget for Toast {
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

    fn content_width(&self) -> Option<usize> {
        let msg_width = self
            .message
            .lines()
            .map(Self::strip_bold_markup)
            .map(|line| rich_rs::cell_len(&line))
            .max()
            .unwrap_or(0);
        let title_width = self
            .title
            .as_ref()
            .map(|t| rich_rs::cell_len(t))
            .unwrap_or(0);
        Some(msg_width.max(title_width).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.dismiss(ctx);
            }
            Event::Tick(_) => {
                if self.timeout_remaining == 0 {
                    // timeout(0) means dismiss immediately on first tick.
                    self.dismiss(ctx);
                } else {
                    self.timeout_remaining -= 1;
                    if self.timeout_remaining == 0 {
                        self.dismiss(ctx);
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut out = Segments::new();
        let title_style = crate::css::resolve_component_style(self, &["toast--title"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        // Python Textual toasts use padding: 1 1; our core style model currently only
        // has horizontal line padding (`line-pad`), so keep vertical padding explicit.
        out.push(Segment::new(" ".repeat(width)));
        out.push(Segment::line());

        // Render title line if present.
        if let Some(title) = &self.title {
            out.push(Segment::styled(
                rich_rs::set_cell_size(title, width),
                title_style,
            ));
            out.push(Segment::line());
        }

        // Render message lines (always at least one line).
        if self.message.is_empty() {
            out.push(Segment::new(" ".repeat(width)));
        } else {
            let lines: Vec<&str> = self.message.lines().collect();
            let line_count = lines.len();
            for (index, line) in lines.into_iter().enumerate() {
                out.extend(Self::render_line_with_bold_markup(line, width));
                if index + 1 < line_count {
                    out.push(Segment::line());
                }
            }
        }
        out.push(Segment::line());
        out.push(Segment::new(" ".repeat(width)));

        out
    }

    fn layout_height(&self) -> Option<usize> {
        let title_lines = if self.title.is_some() { 1 } else { 0 };
        let message_lines = self.message.lines().count().max(1);
        let vertical_padding = 2;
        let intrinsic = title_lines + message_lines + vertical_padding;
        fixed_height_from_constraints(self.layout_constraints()).or(Some(intrinsic))
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Toast"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Toast {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
