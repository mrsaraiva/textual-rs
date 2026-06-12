use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{
    NodeSeed, Widget, WidgetStyles,
    helpers::adjust_line_length_no_bg,
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
    message: String,
    title: Option<String>,
    severity: ToastSeverity,
    timeout_remaining: u64,
    dismissed: bool,
    seed: NodeSeed,
}

impl Toast {
    pub fn new(message: impl Into<String>, severity: ToastSeverity) -> Self {
        let message = message.into();
        let mut seed = NodeSeed::default();
        seed.classes.push(severity.class_name().to_string());
        Self {
            message,
            title: None,
            severity,
            timeout_remaining: DEFAULT_TIMEOUT,
            dismissed: false,
            seed,
        }
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

    fn dismiss(&mut self, ctx: &mut EventCtx) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        ctx.post_message(ToastDismissed);
        ctx.request_repaint();
        ctx.set_handled();
    }

    /// Parse a line with Rich markup and return width-adjusted segments.
    ///
    /// Uses `rich_rs::markup::render()` for full markup support (bold, italic,
    /// underline, colors, nesting). Falls back to plain text on parse error.
    fn render_markup_line(line: &str, width: usize, console: &Console) -> Vec<Segment> {
        let text = match rich_rs::markup::render(line, false) {
            Ok(t) => t,
            Err(_) => Text::plain(line),
        };
        let options = ConsoleOptions {
            size: (width.max(1), 1),
            max_width: width.max(1),
            no_wrap: true,
            ..console.options().clone()
        };
        let segments: Vec<Segment> = text.render(console, &options).into_iter().collect();
        adjust_line_length_no_bg(&segments, width.max(1))
    }

    /// Compute the visual width of a markup line (excluding tags).
    fn markup_cell_len(line: &str) -> usize {
        match rich_rs::markup::render(line, false) {
            Ok(text) => text.cell_len(),
            Err(_) => rich_rs::cell_len(line),
        }
    }
}

/// Build a `SelectorMeta` for off-tree toast rendering (e.g. notification overlay).
///
/// This constructs the meta explicitly from severity so that CSS rules for
/// Toast severity classes apply correctly when the toast is rendered without
/// being mounted in the arena tree. See §T-9 in SPEC-RA2-node-record.md.
pub(crate) fn toast_selector_meta(severity: ToastSeverity) -> crate::css::SelectorMeta {
    crate::css::SelectorMeta::new(
        "Toast".to_string(),
        None,
        vec![severity.class_name().to_string()],
    )
}

impl Widget for Toast {
    fn focusable(&self) -> bool {
        false
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn content_width(&self) -> Option<usize> {
        let msg_width = self
            .message
            .lines()
            .map(Self::markup_cell_len)
            .max()
            .unwrap_or(0);
        let title_width = self
            .title
            .as_ref()
            .map(|t| rich_rs::cell_len(t))
            .unwrap_or(0);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(msg_width.max(title_width).saturating_add(chrome_lr).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
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

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut out = Segments::new();
        let title_style = crate::css::resolve_component_style(self, &["toast--title"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        // Render title line if present.
        if let Some(title) = &self.title {
            out.push(Segment::styled(
                rich_rs::set_cell_size(title, width),
                title_style,
            ));
            if !self.message.is_empty() {
                out.push(Segment::line());
            }
        }

        // Render message lines with full Rich markup support.
        if self.message.is_empty() {
            if self.title.is_none() {
                out.push(Segment::new(" ".repeat(width)));
            }
        } else {
            let lines: Vec<&str> = self.message.lines().collect();
            let line_count = lines.len();
            for (index, line) in lines.into_iter().enumerate() {
                out.extend(Self::render_markup_line(line, width, console));
                if index + 1 < line_count {
                    out.push(Segment::line());
                }
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        let title_lines = if self.title.is_some() { 1 } else { 0 };
        let message_lines = if self.message.is_empty() {
            0
        } else {
            self.message.lines().count().max(1)
        };
        let content_lines = (title_lines + message_lines).max(1);

        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (border_top, border_bottom, _border_left, _border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_height = usize::from(padding.top.saturating_add(padding.bottom))
            .saturating_add(border_top)
            .saturating_add(border_bottom);
        Some(content_lines.saturating_add(chrome_height))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn style_type(&self) -> &'static str {
        "Toast"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        // Preserve the inline style so post-mount style() queries remain accurate.
        self.seed.styles = seed.styles.clone();
        seed
    }
}

impl Renderable for Toast {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::FrameBuffer;

    #[test]
    fn toast_italic_markup_renders_without_literal_brackets() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let toast = Toast::new(
            "This is [i]italic[/i] and [b]bold[/b] text",
            ToastSeverity::Warning,
        );

        let console = Console::new();
        let mut options = console.options().clone();
        let width = 50usize;
        let height = toast.layout_height().expect("toast layout height");
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;

        let rendered = toast.render_styled(&console, &options);
        let lines = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let lines = Segment::set_shape(&lines, width, Some(height), None, false);
        let frame = FrameBuffer::from_lines(&lines, width, height, None);
        let text = frame.as_plain_lines().join("\n");

        // Markup tags should be parsed, not rendered as literal text.
        assert!(
            !text.contains("[i]"),
            "literal [i] tag should not appear in rendered toast: {text:?}"
        );
        assert!(
            !text.contains("[/i]"),
            "literal [/i] tag should not appear in rendered toast: {text:?}"
        );
        assert!(
            text.contains("italic"),
            "italic word should appear in rendered toast: {text:?}"
        );
        assert!(
            text.contains("bold"),
            "bold word should appear in rendered toast: {text:?}"
        );
    }

    #[test]
    fn toast_markup_cell_len_excludes_tags() {
        let line = "Press [b]ctrl+q[/b] to quit";
        let visual_len = Toast::markup_cell_len(line);
        // "Press ctrl+q to quit" = 20 chars, not 30 with tags
        assert_eq!(visual_len, 20);
    }

    #[test]
    fn toast_title_and_message_survive_fixed_height_composition() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let toast = Toast::new(
            "Press [b]ctrl+q[/b] to quit the app",
            ToastSeverity::Information,
        )
        .with_title("Do you want to quit?");

        let console = Console::new();
        let mut options = console.options().clone();
        let width = 50usize;
        let height = toast.layout_height().expect("toast layout height");
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;

        let rendered = toast.render_styled(&console, &options);
        let lines = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let lines = Segment::set_shape(&lines, width, Some(height), None, false);
        let frame = FrameBuffer::from_lines(&lines, width, height, None);
        let text = frame.as_plain_lines().join("\n");

        assert!(
            text.contains("Do you want to quit?"),
            "title should be visible in toast frame: {text:?}"
        );
        assert!(
            text.contains("Press ctrl+q to quit the app"),
            "message should be visible in toast frame: {text:?}"
        );
    }
}
