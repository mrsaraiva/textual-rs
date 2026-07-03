use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, Text};
use std::sync::Arc;

#[allow(deprecated)] // `.class()` builds a `Node` wrapper; deprecated but supported for one release.
use crate::widgets::{Node, NodeSeed, Widget};

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying widget CSS text attributes (bold, italic, etc.) that have
/// already been baked into the segment by `Content::render_strips`.
fn tag_segment_no_text_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert("textual:no_text_style".to_string(), MetaValue::Bool(true));
    meta.meta = Some(Arc::new(map));
    seg.meta = Some(meta);
}

/// Holds the display content of a [`Static`] widget.
enum StaticContent {
    /// Plain or markup string rendered directly via `Content::render_strips`.
    Plain,
    /// Pre-built [`Content`] (e.g. markup with template variables already
    /// substituted via `Content::from_markup_with_vars`). Mirrors Python
    /// `Static.update(content)` where `content` is a `Content`/`Visual`.
    Content(crate::content::Content),
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
    /// CSS id cache preserved across `take_node_seed()`.
    ///
    /// `take_node_seed()` moves the seed out of the widget (clearing `seed.css_id`).
    /// Off-tree CSS resolution (`layout_height()` → `resolved_vertical_chrome()`)
    /// runs AFTER mounting, so `seed.css_id` would be `None` at that point.
    /// We preserve the id here before the seed is taken so `style_id()` keeps
    /// returning the correct value for off-tree resolution.
    css_id_cache: Option<String>,
    /// CSS classes cache preserved across `take_node_seed()` (same rationale).
    classes_cache: Vec<String>,
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
            css_id_cache: None,
            classes_cache: Vec::new(),
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

    /// Set this widget's CSS id (sets `seed.css_id` directly so the id is on
    /// the Static node itself, not a transparent Node wrapper).
    ///
    /// This allows CSS rules like `#custom { link-color: ... }` to target the
    /// Static widget directly with id-selector specificity.
    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.seed.css_id = Some(value.into());
        self
    }

    /// Add a CSS class via a transparent `Node` wrapper.
    ///
    /// Returns a `Node` (same as before) so that class-based CSS descendant
    /// rules (`#questions .button { ... }`) continue to apply to the wrapper
    /// and the Rust layout tree structure stays compatible with nesting01/02.
    ///
    /// DEFERRED(display-clear): making this SEED-BASED (the class on the Static's
    /// own node, like Python `Static(classes=...)`) is the faithful fix for
    /// `docs/examples/styles/display` (`Static.remove { display:none }`), and it
    /// DOES clear `display` cleanly (verified: only type/class selectors, which
    /// the leaf resolves fine). But it regresses `nesting01`/`nesting02`, and the
    /// REAL root is NOT `apply_parent_align` (that already centers the margin box
    /// correctly): it is auto-HEIGHT chrome resolution for a DESCENDANT-selected
    /// leaf. `#questions .button { border; padding; margin }` is a descendant
    /// rule, so the leaf Static's context-free `layout_height()` (via
    /// `resolved_vertical_chrome` → `selector_meta_generic`, which has NO ancestor
    /// chain) cannot match `#questions .button` and reports content height with
    /// ZERO chrome. The box then collapses to 1 row (debug: `lr h=1`, expected 5),
    /// and the (correct) margin-box centering places that 1-row box ~2 rows low.
    /// The Node wrapper masked this because the chrome lived on the wrapper, whose
    /// height the layout engine resolves WITH full ancestor context.
    /// The faithful fix is to make the auto/unset HEIGHT edge add the
    /// CSS-resolved `v_chrome` to the widget's PURE content height (symmetric with
    /// the WIDTH arm in `extract_child_spec`, which already does
    /// `content + full_h_chrome`). That requires unwinding the mixed
    /// `layout_height()`-includes-chrome convention across the height-measurement
    /// callsites (`horizontal.rs`/`vertical.rs`/grid) and the ~6 widgets that bake
    /// chrome into `layout_height()` (Static/Label/Checkbox/Switch/RadioSet/Panel),
    /// with five_by_five `GameCell` as the regression guard — out of this cluster's
    /// allowed-file scope. Re-land seed-based `class()` together with that
    /// height-chrome convention fix.
    #[allow(deprecated)] // Returns a `Node` wrapper (deprecated RA2.6, supported one release).
    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
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

    /// The current plain text content (the value last set via [`new`](Self::new)
    /// or [`update`](Self::update)). Mirrors reading Python `Static.renderable`
    /// for the plain-text case; used by tests/Pilot to assert content state.
    pub fn text(&self) -> &str {
        &self.text
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

    /// Replace content with a pre-built [`Content`] value.
    ///
    /// Use this to display markup whose template variables were already
    /// substituted (e.g. `Content::from_markup_with_vars(...)`), so the Static
    /// renders the exact spans/text of that `Content` instead of re-parsing a
    /// string. Theme-token resolution, alignment and link styling go through the
    /// same render path as plain markup.
    ///
    /// Mirrors Python `Static.update(content)` where `content` is a `Content`.
    pub fn update_content(&mut self, content: crate::content::Content) {
        self.content = StaticContent::Content(content);
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
        // Route through the shared word-wrap line counter (same path as Label):
        // a naive `cell_len.div_ceil(width)` char-count under-counts because real
        // wrapping breaks at WORD boundaries, so wide paragraphs produce MORE
        // lines and the wrapped tail would get clipped (padding02).
        crate::widgets::text::intrinsic_wrapped_height(&self.text, self.layout_width, self.wrap)
    }

    fn intrinsic_content_width(&self) -> usize {
        self.text
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1)
    }

    /// Render a `Content` value to `Segments`, shared by the `Plain` (string)
    /// and `Content` (pre-built) paths so both go through identical
    /// alignment/background/theme-token/link resolution.
    ///
    /// `apply_link_style` mirrors the `markup` flag: only markup-derived content
    /// gets `[@click=...]` link styling overlaid.
    fn render_content(
        &self,
        content: &crate::content::Content,
        options: &ConsoleOptions,
        apply_link_style: bool,
    ) -> Segments {
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
        let parent_bg = crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
            crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);

        // Construct the render-time visual style: always has an explicit bg so
        // make_segment never falls back to black.
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        let mut content = content.clone();

        // Apply link-* CSS styling to `[@click=...]` markup spans.
        if apply_link_style {
            if let Some(link_span_style) =
                crate::widgets::text::compute_link_span_style(&render_style, effective_bg)
            {
                let link_ranges: Vec<(usize, usize)> = content
                    .spans()
                    .iter()
                    .filter(|span| {
                        matches!(&span.span_style, crate::content::SpanStyle::Raw(raw)
                            if raw.starts_with("@click=") || raw == "@click")
                    })
                    .map(|span| (span.start, span.end))
                    .collect();
                for (start, end) in link_ranges {
                    content = content.stylize(link_span_style.clone(), start, end);
                }
            }
        }

        // Resolve theme tokens in span styles using parse_tag_style, which calls
        // parse_color_like internally and handles $primary/$surface etc.
        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

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

        let mut segments = Segments::new();
        let n_strips = strips.len();
        let n_trailing_empty = strips.iter().rev().take_while(|s| s.is_empty()).count();
        for (i, strip) in strips.into_iter().enumerate() {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                segments.push(seg);
            }
            if i + 1 < n_strips {
                segments.push(Segment::line());
            }
        }
        for _ in 0..n_trailing_empty {
            segments.push(Segment::line());
        }
        segments
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

    /// `intrinsic_height` must count REAL word-wrapped lines, not a
    /// `cell_len.div_ceil(width)` char-count. A paragraph longer than the width
    /// breaks at word boundaries and produces MORE lines than the char-count
    /// estimate, so the old estimate under-counted and clipped the wrapped tail
    /// (`docs/examples/guide/styles/padding02`).
    #[test]
    fn static_intrinsic_height_uses_real_word_wrap() {
        let mut widget = Static::new("Fear is the little-death that brings total obliteration.");
        // Content width 22 (padding02: width 30 - padding 4*2).
        Widget::on_layout(&mut widget, 22, 0);
        let h = widget.intrinsic_height();
        // Word-wrapping "Fear is the little-death that brings total
        // obliteration." at 22 cells yields 4 lines (Rich word-wrap). The naive
        // char-count estimate `56.div_ceil(22)` = 3 would clip a line.
        assert!(
            h >= 4,
            "word-wrapped height should be >= 4 lines, got {h} (char-count \
             estimate would under-count to 3)"
        );
    }
}

impl Widget for Static {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        match &self.content {
            StaticContent::Plain => {
                // Build Content from the static text (markup or plain), then render
                // through the shared Content path.
                let content = if self.markup {
                    crate::content::Content::from_markup(&self.text)
                } else {
                    crate::content::Content::from_text(&self.text)
                };
                self.render_content(&content, options, self.markup)
            }
            StaticContent::Content(content) => {
                // Pre-built Content (e.g. with template variables substituted).
                // Treat it like markup output for link styling/resolution purposes.
                self.render_content(content, options, true)
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
            StaticContent::Plain => Some(self.intrinsic_height().saturating_add(chrome)),
            StaticContent::Content(content) => {
                // Word-wrap-aware height from the content's plain text (same
                // shared counter as the Plain path), so wrapped pre-built Content
                // sizes to its real line count rather than the char-count estimate.
                let plain = content.plain();
                let lines = crate::widgets::text::intrinsic_wrapped_height(
                    plain,
                    self.layout_width,
                    self.wrap,
                );
                Some(lines.saturating_add(chrome))
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
        // Update the seed, consumed at mount for the pre-mount path. A POST-mount
        // inline-style write (the seed is already drained) must instead go through
        // `WidgetCtx::update_styles` so it reaches the arena node record — mirroring
        // Python `widget.styles.background = color`.
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        // Preserve id/classes in the cache so `style_id()` / `style_classes()`
        // keep working after the seed has been taken (off-tree CSS resolution
        // in `layout_height()` runs post-mount when `seed.css_id` would be gone).
        self.css_id_cache = seed.css_id.clone();
        self.classes_cache = seed.classes.clone();
        seed
    }

    fn style_id(&self) -> Option<&str> {
        // Pre-mount: seed has the id. Post-mount: seed is empty, use the cache.
        if self.seed.css_id.is_some() {
            self.seed.css_id.as_deref()
        } else {
            self.css_id_cache.as_deref()
        }
    }

    fn style_classes(&self) -> &[String] {
        // Pre-mount: seed has the classes. Post-mount: use the cache.
        if !self.seed.classes.is_empty() {
            &self.seed.classes
        } else {
            &self.classes_cache
        }
    }
}
