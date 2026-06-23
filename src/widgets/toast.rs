use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, StyleMeta, Text};

use crate::content::{Content, ContentPart};
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{NodeSeed, Widget};

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying widget CSS text attributes that have already been baked in
/// by `Content::render_strips`.
fn tag_segment_no_text_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_else(StyleMeta::new);
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert(
        "textual:no_text_style".to_string(),
        MetaValue::Bool(true),
    );
    meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(meta);
}

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
    /// CSS classes carried by this toast (currently the severity class such as
    /// `-information`). Stored as a field so off-tree style resolution
    /// (`selector_meta_generic`/`render_styled`) can see them via
    /// `style_classes()`; the runtime composes toasts off-tree, so without this
    /// the severity-scoped rules (e.g. `Toast.-warning { border-left: ... }`)
    /// would never match.
    classes: Vec<String>,
    timeout_remaining: u64,
    dismissed: bool,
    seed: NodeSeed,
}

impl Toast {
    crate::seed_ident_methods!();

    pub fn new(message: impl Into<String>, severity: ToastSeverity) -> Self {
        let message = message.into();
        let classes = vec![severity.class_name().to_string()];
        let mut seed = NodeSeed::default();
        seed.classes = classes.clone();
        Self {
            message,
            title: None,
            severity,
            classes,
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

    /// Number of visual lines a markup source produces once word-wrapped to
    /// `width` (newline-split first, then word-wrapped each segment).
    fn wrapped_line_count(source: &str, width: usize) -> usize {
        let width = width.max(1);
        source
            .lines()
            .map(|line| {
                let text = match rich_rs::markup::render(line, false) {
                    Ok(t) => t,
                    Err(_) => Text::plain(line),
                };
                text.wrap(width, None, None, 8, false).len().max(1)
            })
            .sum()
    }

    /// Resolve the toast content-box width (CSS `width` minus border + padding).
    /// Falls back to the value the runtime composes toasts at (60).
    fn content_box_width(&self) -> usize {
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let outer = match resolved.width {
            Some(crate::style::Scalar::Cells(c)) => c as usize,
            _ => 60,
        };
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        outer
            .saturating_sub(padding.left as usize + padding.right as usize)
            .saturating_sub(border_left + border_right)
            .max(1)
    }

    /// Compute the visual width of a markup line (excluding tags).
    fn markup_cell_len(line: &str) -> usize {
        match rich_rs::markup::render(line, false) {
            Ok(text) => text.cell_len(),
            Err(_) => rich_rs::cell_len(line),
        }
    }
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

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Resolve the widget's visual style (pushed by render_widget_with_meta
        // before calling render()).
        let visual_style = crate::css::current_self_style().unwrap_or_default();

        // Flatten widget's own bg over the composited ancestor background so
        // transparent-bg toasts still get the correct surface color baked in.
        let parent_bg =
            crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
                crate::style::parse_color_like("$background")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        // Build the Content object — mirroring Python's Toast.render() which
        // uses Content.assemble((title, header_style), "\n", message_content).
        let message_content = Content::from_markup(&self.message);

        let content = if let Some(title) = &self.title {
            // Resolve the `toast--title` component style for the title text.
            let title_style = crate::css::resolve_component_style(self, &["toast--title"]);
            Content::assemble([
                ContentPart::from((title.as_str(), title_style)),
                ContentPart::from("\n"),
                ContentPart::from(message_content),
            ])
        } else {
            message_content
        };

        // Resolve theme tokens in span styles.
        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

        // Render via Content::render_strips.
        // - width: content width as received (border + padding subtracted by caller).
        // - height=None: let wrap_and_format determine row count (height is
        //   set correctly by layout_height()).
        // - no_wrap=false: word-wrap the message body (Python Static wraps).
        // - line_pad=0: render_widget_with_meta handles outer padding.
        // - align=Left: toast message is left-aligned (Python default).
        let strips = content.render_strips(
            width,
            None,
            &render_style,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            resolve_fn,
        );

        // Flatten strips into Segments joined by newlines, tagging each segment
        // with no_text_style so apply_style_to_segments does not re-apply CSS
        // text attrs (bold, italic, etc.) that render_strips already baked in.
        let mut out = Segments::new();
        let n_strips = strips.len();
        for (i, strip) in strips.into_iter().enumerate() {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                out.push(seg);
            }
            if i + 1 < n_strips {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let title_lines = if self.title.is_some() { 1 } else { 0 };
        let content_width = self.content_box_width();
        let message_lines = if self.message.is_empty() {
            0
        } else {
            Self::wrapped_line_count(&self.message, content_width).max(1)
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

    fn style_type(&self) -> &'static str {
        "Toast"
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
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

    /// The severity class is exposed off-tree via `style_classes()`, so the
    /// `Toast.-<severity> { border-left: outer ... }` rule resolves and the
    /// `▌` marker is painted even when the runtime composes the toast off the
    /// arena tree.
    #[test]
    fn toast_renders_border_left_marker() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let toast = Toast::new("It's a trap!", ToastSeverity::Error);
        assert_eq!(toast.style_classes(), &["-error".to_string()]);

        let console = Console::new();
        let mut options = console.options().clone();
        let width = 60usize;
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
            text.contains('▌'),
            "border-left outer marker should be painted: {text:?}"
        );
    }

    /// A long message word-wraps at the content width (Python `Static`/`Content`
    /// behavior) and `layout_height()` accounts for the wrapped lines.
    #[test]
    fn toast_long_message_wraps_to_multiple_lines() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let toast = Toast::new(
            "Now witness the firepower of this fully ARMED and OPERATIONAL battle station!",
            ToastSeverity::Warning,
        )
        .with_title("Possible trap detected");

        // title (1) + wrapped message (2) + padding top/bottom (2) = 5
        let height = toast.layout_height().expect("toast layout height");
        assert_eq!(height, 5, "wrapped toast should be 5 rows tall");

        let console = Console::new();
        let mut options = console.options().clone();
        let width = 60usize;
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;

        let rendered = toast.render_styled(&console, &options);
        let lines = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let lines = Segment::set_shape(&lines, width, Some(height), None, false);
        let frame = FrameBuffer::from_lines(&lines, width, height, None);
        let rows: Vec<String> = frame.as_plain_lines();

        // The message must break before OPERATIONAL, not truncate it.
        assert!(
            rows.iter().any(|r| r.contains("ARMED and") && !r.contains("OPERATIONAL")),
            "first message line should end at 'ARMED and': {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("OPERATIONAL battle station!")),
            "wrapped continuation line should be present: {rows:?}"
        );
    }
}
