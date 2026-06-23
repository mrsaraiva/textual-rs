use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, StyleMeta, Text};
use std::sync::Arc;

use crate::widgets::{Node, NodeSeed, Widget};

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying widget CSS text attributes (bold, italic, etc.) that have
/// already been baked into the segment by `Content::render_strips`.
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
    meta.meta = Some(Arc::new(map));
    seg.meta = Some(meta);
}

/// Holds the display content of a [`Static`] widget.
enum StaticContent {
    /// Plain or markup string rendered directly via `Content::render_strips`.
    Plain,
    /// Pre-rendered rich text (e.g. syntax-highlighted code).
    Rich(Text),
}

/// A static text widget with optional rich-text content.
///
/// Mirrors Python Textual's `Static` widget.  Compose with plain text or use
/// [`Static::update()`] / [`Static::update_rich()`] to change content at
/// runtime, matching Python's `Static.update(content)` API.
pub struct Static {
    text: String,
    markup: bool,
    wrap: bool,
    expand: bool,
    shrink: bool,
    layout_width: usize,
    content: StaticContent,
    border_title: Option<String>,
    border_subtitle: Option<String>,
    seed: NodeSeed,
}

impl Static {
    pub fn new(text: impl Into<String>) -> Self {
        // Python Textual's Static defaults to markup=True.
        Self {
            text: text.into(),
            markup: true,
            wrap: true,
            expand: false,
            shrink: false,
            layout_width: 0,
            content: StaticContent::Plain,
            border_title: None,
            border_subtitle: None,
            seed: NodeSeed::default(),
        }
    }

    /// Disable Rich markup parsing for this widget's text content.
    ///
    /// Mirrors Python `Static(text, markup=False)`: tags are rendered as-is
    /// (not interpreted).  The widget CSS type remains `Static`, so type-based
    /// CSS rules such as `Static { height: 1fr }` still apply.
    pub fn without_markup(mut self) -> Self {
        self.markup = false;
        self
    }

    /// When true, the widget expands to fill the available width.
    ///
    /// Mirrors Python `Static(expand=True)`.
    pub fn with_expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
    }

    pub fn id(self, value: impl Into<String>) -> Node {
        Node::new(self).id(value)
    }

    /// Replace content with a plain text string.
    ///
    /// Mirrors Python `Static.update(text)`.  Clears any previously set rich
    /// content.  Call `ctx.request_repaint()` after this if you have access to
    /// `EventCtx`; otherwise the repaint will happen on the next input cycle.
    pub fn update(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.content = StaticContent::Plain;
    }

    /// Replace content with a pre-rendered [`rich_rs::Text`] value.
    ///
    /// Use this to display syntax-highlighted code or other styled content:
    /// ```ignore
    /// use rich_rs::Syntax;
    ///
    /// let text = Syntax::from_path(path)?.highlight();
    /// app.with_query_one_mut_as::<Static, _>("#code", |s| s.update_rich(text))?;
    /// ```
    ///
    /// Mirrors Python `Static.update(syntax_renderable)`.
    pub fn update_rich(&mut self, text: Text) {
        self.content = StaticContent::Rich(text);
    }

    /// Clear all content (show empty widget).
    ///
    /// Mirrors Python `Static.update("")`.
    pub fn clear(&mut self) {
        self.text = String::new();
        self.content = StaticContent::Plain;
    }

    /// Set the text rendered on the top border (Python `widget.border_title`).
    pub fn with_border_title(mut self, title: impl Into<String>) -> Self {
        self.border_title = Some(title.into());
        self
    }

    /// Set the text rendered on the bottom border (Python `widget.border_subtitle`).
    pub fn with_border_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.border_subtitle = Some(subtitle.into());
        self
    }

    /// Set or clear the border title at runtime.
    pub fn set_border_title(&mut self, title: Option<impl Into<String>>) {
        self.border_title = title.map(Into::into);
    }

    /// Set or clear the border subtitle at runtime.
    pub fn set_border_subtitle(&mut self, subtitle: Option<impl Into<String>>) {
        self.border_subtitle = subtitle.map(Into::into);
    }

    fn intrinsic_height(&self) -> usize {
        let width = self.layout_width;
        let mut lines = 0usize;
        for line in self.text.lines() {
            if self.wrap && width > 0 {
                let len = rich_rs::cell_len(line);
                lines += len.div_ceil(width).max(1);
            } else {
                lines += 1;
            }
        }
        lines.max(1)
    }

    fn intrinsic_content_width(&self) -> usize {
        self.text
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1)
    }
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_update_changes_content() {
        let mut widget = Static::new("initial");
        widget.update("updated");
        // content is now "updated" — confirm Plain variant is active
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_update_rich_switches_to_rich_variant() {
        let mut widget = Static::new("initial");
        let text = Text::plain("rich content");
        widget.update_rich(text);
        assert!(matches!(widget.content, StaticContent::Rich(_)));
    }

    #[test]
    fn static_update_after_rich_reverts_to_plain() {
        let mut widget = Static::new("initial");
        widget.update_rich(Text::plain("rich"));
        widget.update("plain again");
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_clear_sets_plain_empty() {
        let mut widget = Static::new("hello");
        widget.update_rich(Text::plain("rich"));
        widget.clear();
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_layout_height_rich_returns_line_count() {
        let mut widget = Static::new("");
        let text = Text::plain("line one\nline two\nline three");
        widget.update_rich(text);
        assert_eq!(widget.layout_height(), Some(3));
    }
}

impl Widget for Static {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        match &self.content {
            StaticContent::Plain => {
                let width = options.size.0.max(1);

                // Get the widget's resolved visual style (pushed by render_widget_with_meta
                // onto the style stack before calling render()).
                let visual_style = crate::css::current_self_style().unwrap_or_default();

                // Determine text alignment from resolved CSS.
                let text_align = visual_style
                    .text_align
                    .unwrap_or(crate::style::TextAlign::Left);

                // Build the effective background: flatten the widget's own bg over the
                // composited ancestor background so transparent-bg statics still get the
                // correct surface color baked into every segment.
                //
                // Use current_ancestor_composited_background() which excludes the current
                // widget's own style from the composite. This matches what
                // apply_style_to_segments sees when it runs AFTER render() returns (at
                // which point the current widget's style has been popped from the stack).
                let parent_bg =
                    crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
                        crate::style::parse_color_like("$background")
                            .unwrap_or(crate::style::Color::rgb(0, 0, 0))
                    });
                let effective_bg = visual_style
                    .bg
                    .map(|c| c.flatten_over(parent_bg))
                    .unwrap_or(parent_bg);

                // Construct the render-time visual style: always has an explicit bg so
                // make_segment never falls back to black. fg/attrs come from the resolved
                // style (same source as apply_style_to_segments would use).
                let mut render_style = visual_style.clone();
                render_style.bg = Some(effective_bg);

                // Build Content from the static text.
                let content = if self.markup {
                    crate::content::Content::from_markup(&self.text)
                } else {
                    crate::content::Content::from_text(&self.text)
                };

                // Resolve theme tokens in span styles using parse_tag_style, which calls
                // parse_color_like internally and handles $primary/$surface etc.
                let resolve_fn = |raw: &str| {
                    crate::content::markup::parse_tag_style(raw)
                        .map(|t| t.style)
                        .unwrap_or_default()
                };

                // Render via Content::render_strips.
                // - No height cap: let wrap_and_format determine row count.
                // - overflow="fold": word-wrap (render.rs handles ellipsis/clip for no_wrap).
                // - no_wrap=false: always word-wrap here; render.rs applies overflow later.
                // - line_pad=0: handled by render_widget_with_meta.
                let strips = content.render_strips(
                    width,
                    None,
                    &render_style,
                    text_align,
                    "fold",
                    false,
                    0,
                    resolve_fn,
                );

                // Flatten strips into Segments joined by newlines.
                // Tag each data segment with textual:no_text_style so apply_style_to_segments
                // skips re-applying widget CSS text attrs (bold/italic/etc.), which have
                // already been baked in by render_strips (visual_style + span_style combined).
                let mut segments = Segments::new();
                let n_strips = strips.len();
                for (i, strip) in strips.into_iter().enumerate() {
                    for mut seg in strip {
                        tag_segment_no_text_style(&mut seg);
                        segments.push(seg);
                    }
                    if i + 1 < n_strips {
                        segments.push(Segment::line());
                    }
                }
                segments
            }
            StaticContent::Rich(text) => text.render(console, options),
        }
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        // Hidden/disconnected nodes can transiently receive width=0/1 during
        // tree display toggles. Keep the last stable width (>1) so wrapped-height
        // calculations remain stable across tab switches.
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        // `layout_height()` returns the widget's OUTER height (content + own
        // padding/border chrome). This matches the convention used by flow layout.
        let chrome = crate::widgets::helpers::resolved_vertical_chrome(self);
        match &self.content {
            StaticContent::Plain => {
                Some(self.intrinsic_height().saturating_add(chrome))
            }
            StaticContent::Rich(text) => {
                let line_count = text.plain_text().lines().count().max(1);
                Some(line_count.saturating_add(chrome))
            }
        }
    }

    fn content_width(&self) -> Option<usize> {
        if self.expand {
            // No intrinsic width constraint — fill available space.
            None
        } else if self.shrink {
            Some(self.intrinsic_content_width())
        } else {
            None
        }
    }

    fn auto_content_width(&self) -> Option<usize> {
        if self.expand {
            None
        } else {
            // For `width: auto` sizing, report the rendered text's cell width so
            // the box shrinks to its content (Python parity).
            Some(self.intrinsic_content_width())
        }
    }

    fn border_title(&self) -> Option<&str> {
        self.border_title.as_deref()
    }

    fn border_subtitle(&self) -> Option<&str> {
        self.border_subtitle.as_deref()
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}
