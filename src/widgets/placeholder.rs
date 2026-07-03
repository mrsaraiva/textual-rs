use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::Event;
use crate::message::*;
use crate::style::Color;

use super::{NodeSeed, Widget};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// The variant determines what text a placeholder displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderVariant {
    /// Shows the label or widget identifier.
    Default,
    /// Shows the WxH dimensions.
    Size,
    /// Shows Lorem Ipsum text.
    Text,
}

impl PlaceholderVariant {
    fn next(self) -> Self {
        match self {
            PlaceholderVariant::Default => PlaceholderVariant::Size,
            PlaceholderVariant::Size => PlaceholderVariant::Text,
            PlaceholderVariant::Text => PlaceholderVariant::Default,
        }
    }

    fn class_name(self) -> &'static str {
        match self {
            PlaceholderVariant::Default => "-default",
            PlaceholderVariant::Size => "-size",
            PlaceholderVariant::Text => "-text",
        }
    }

    fn message_name(self) -> &'static str {
        match self {
            PlaceholderVariant::Default => "default",
            PlaceholderVariant::Size => "size",
            PlaceholderVariant::Text => "text",
        }
    }
}

const PLACEHOLDER_COLORS: &[&str] = &[
    "#881177", "#aa3355", "#cc6666", "#ee9944", "#eedd00", "#99dd55", "#44dd88", "#22ccbb",
    "#00bbcc", "#0099cc", "#3366bb", "#663399",
];

const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam feugiat ac elit sit amet accumsan. Suspendisse bibendum nec libero quis gravida. Phasellus id eleifend ligula. Nullam imperdiet sem tellus, sed vehicula nisl faucibus sit amet. Praesent iaculis tempor ultricies. Sed lacinia, tellus id rutrum lacinia, sapien sapien congue mauris, sit amet pellentesque quam quam vel nisl. Curabitur vulputate erat pellentesque mauris posuere, non dictum risus mattis.";

/// Global counter for assigning cycling background colors to consecutive placeholders.
static COLOR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A placeholder widget for prototyping layouts.
///
/// Shows a colored area with identifying text. Cycles through variants on click.
/// Each new instance gets the next color from a rotating palette.
#[derive(Debug, Clone)]
pub struct Placeholder {
    label: String,
    variant: PlaceholderVariant,
    color_index: usize,
    /// Cached content-box dimensions from the last layout pass.
    last_width: usize,
    last_height: usize,
    seed: NodeSeed,
}

impl Placeholder {
    /// Set the CSS id. Unlike the generic builder, Placeholder also derives its
    /// default label from the id when no explicit label was given — matching
    /// Python `_placeholder.py`, where `label = label or (f"#{id}" if id else
    /// "Placeholder")`. The label is fixed at build time (the id seed is consumed
    /// at mount, so it can't be recovered at render).
    pub fn id(mut self, value: impl Into<String>) -> Self {
        let id = value.into();
        if self.label.is_empty() {
            self.label = format!("#{id}");
        }
        self.seed.css_id = Some(id);
        self
    }

    /// Add a CSS class (Python `classes=`). Idempotent.
    pub fn class(mut self, value: impl Into<String>) -> Self {
        let v = value.into();
        if !self.seed.classes.iter().any(|c| c == &v) {
            self.seed.classes.push(v);
        }
        self
    }

    pub fn new(label: impl Into<String>) -> Self {
        let color_index = COLOR_COUNTER.fetch_add(1, Ordering::Relaxed) % PLACEHOLDER_COLORS.len();
        let label = label.into();
        let mut seed = NodeSeed::default();
        seed.classes.push("placeholder".to_string());
        seed.classes.push("-default".to_string());
        let mut ph = Self {
            label,
            variant: PlaceholderVariant::Default,
            color_index,
            last_width: 0,
            last_height: 0,
            seed,
        };
        ph.apply_bg_color();
        ph
    }

    pub fn with_variant(mut self, variant: PlaceholderVariant) -> Self {
        // Remove old variant class and add new one in seed.
        let old_class = self.variant.class_name().to_string();
        self.seed.classes.retain(|c| c != &old_class);
        self.variant = variant;
        self.seed
            .classes
            .push(self.variant.class_name().to_string());
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn variant(&self) -> PlaceholderVariant {
        self.variant
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `variant`. Records the change and triggers
    /// watcher dispatch via [`ReactiveWidget::reactive_dispatch`].
    pub fn set_variant(&mut self, value: PlaceholderVariant, ctx: &mut ReactiveCtx) {
        if self.variant != value {
            let old = self.variant;
            self.variant = value;
            ctx.record_change(
                "variant",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_variant(
        &mut self,
        old: &PlaceholderVariant,
        new: &PlaceholderVariant,
        ctx: &mut ReactiveCtx,
    ) {
        ctx.remove_class(old.class_name());
        ctx.add_class(new.class_name());
    }

    pub fn cycle_variant(&mut self) {
        self.variant = self.variant.next();
    }

    fn apply_bg_color(&mut self) {
        let hex = PLACEHOLDER_COLORS[self.color_index];
        if let Some(color) = crate::style::parse_color_like(hex) {
            // Apply at exactly 50% opacity to match Python Textual's `background: {color} 50%`.
            // Use rgba_f (float alpha) so the blend factor is exactly 0.5, not the
            // u8-quantized 128/255 = 0.50196 which would drift the composited RGB by ±1.
            let bg = Color::rgba_f(color.r, color.g, color.b, 0.5);
            self.seed.styles.set_bg(bg);
        }
    }

    fn render_text(&self, width: usize, height: usize) -> String {
        match self.variant {
            PlaceholderVariant::Default => {
                if self.label.is_empty() {
                    "Placeholder".to_string()
                } else {
                    self.label.clone()
                }
            }
            PlaceholderVariant::Size => {
                format!("{} x {}", width, height)
            }
            PlaceholderVariant::Text => {
                // Repeat the lorem ipsum with paragraph breaks (matches Python Textual).
                let mut text = String::new();
                for i in 0..5 {
                    if i > 0 {
                        text.push_str("\n\n");
                    }
                    text.push_str(LOREM_IPSUM);
                }
                text
            }
        }
    }
}

impl Widget for Placeholder {
    fn focusable(&self) -> bool {
        false
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width as usize;
        self.last_height = height as usize;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.node_state().disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                let old_class = self.variant.class_name().to_string();
                self.cycle_variant();
                let new_class = self.variant.class_name().to_string();
                ctx.remove_class(&old_class);
                ctx.add_class(&new_class);
                ctx.post_message(PlaceholderVariantChanged {
                    variant: self.variant.message_name().to_string(),
                });
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let text = self.render_text(width, height);
        let mut out = Segments::new();

        let style = crate::css::resolve_component_style(self, &["placeholder"])
            .to_rich()
            .unwrap_or_default();

        // Python parity: `Placeholder.render()` returns the *bare* renderable
        // (the label, the "W x H" string, or the lorem-ipsum text). All
        // centering — horizontal AND vertical — is performed by the framework
        // from the `content-align: center middle` CSS default (see
        // `apply_content_alignment` in `widgets/core.rs`, mirroring Python's
        // `_segment_tools.align_lines`). The widget must NOT pre-center its
        // own content, or it would be centered twice (the leading padding from a
        // self-centered line is not trimmed by the alignment pass, shifting the
        // content one column right). We only emit the raw glyph lines here; the
        // surface fill/alignment composes them into the content box.
        match self.variant {
            PlaceholderVariant::Text => {
                // Word-wrap the text to fill the available width, but leave the
                // wrapped lines un-padded/un-aligned (content-align handles the
                // rest). The `.-text` variant uses `padding: 1`, so the content
                // width is already inset by the framework before render.
                let lines = word_wrap(&text, width);
                for (row, content) in lines.iter().enumerate() {
                    out.push(Segment::styled(content.clone(), style));
                    if row + 1 < lines.len() {
                        out.push(Segment::line());
                    }
                }
            }
            _ => {
                // Default / size variant: a single bare line, centered by
                // content-align.
                out.push(Segment::styled(text, style));
            }
        }

        out
    }

    /// Intrinsic content width for `width: auto` sizing (Python parity).
    ///
    /// Python `Placeholder.render()` returns the label string (e.g. `"#auto"`).
    /// A `width: auto` box model shrinks to that rendered text width, so
    /// `Placeholder(id="auto")` becomes 5 cells wide — not flex-filled.
    ///
    /// Kept separate from `content_width()` so an UNSET width (the common case)
    /// still flex-fills, matching Python's default `1fr` behavior for placeholders
    /// whose width is not explicitly set to `auto`.
    fn auto_content_width(&self) -> Option<usize> {
        let label = if self.label.is_empty() {
            "Placeholder"
        } else {
            &self.label
        };
        Some(rich_rs::cell_len(label).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        // UNSET height must flex-fill the container (Python: a bare Placeholder
        // with no `height` rule fills its column — see the layout05 Tweet stack).
        // So report no intrinsic height here; `height: auto` sizing goes through
        // `auto_content_height()` instead.
        None
    }

    /// Intrinsic content height for `height: auto` sizing (Python parity).
    ///
    /// Python `Placeholder.render()` returns a renderable whose line count the
    /// box model uses when `height: auto`. For the default/size variants that's a
    /// single line (e.g. `"#p1"` / `"30 x 5"`); the text variant wraps the lorem
    /// ipsum at the current width. We mirror that here so a `Placeholder` with
    /// `height: auto` shrinks to its label height (e.g. `padding_all`'s `#p1`
    /// becomes 1 row, not the full grid cell), while an UNSET height still fills
    /// (guarded by `layout_height() == None`).
    fn auto_content_height(&self) -> Option<usize> {
        let width = self.last_width.max(1);
        let text = self.render_text(width, self.last_height.max(1));
        let lines = match self.variant {
            // Default and size are always a single rendered line.
            PlaceholderVariant::Default | PlaceholderVariant::Size => 1,
            // Text wraps at the current width — count the wrapped lines.
            PlaceholderVariant::Text => word_wrap(&text, width).len().max(1),
        };
        Some(lines)
    }

    fn style_type(&self) -> &'static str {
        "Placeholder"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Placeholder {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Placeholder {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "variant" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<PlaceholderVariant>(),
                    change.new_value.downcast_ref::<PlaceholderVariant>(),
                ) {
                    self.watch_variant(old, new, ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;

    fn make_console_options(width: usize, height: usize) -> ConsoleOptions {
        let mut opts = ConsoleOptions::default();
        opts.size = (width, height);
        opts.max_width = width;
        opts.max_height = height;
        opts
    }

    /// Python parity: `Placeholder.render()` returns the BARE label — it must
    /// NOT pre-center horizontally or vertically. Centering is owned by the
    /// framework's `content-align: center middle` composition pass
    /// (`apply_content_alignment`), exactly like Python's `align_lines`.
    ///
    /// If the widget pre-centered (emitting `"  Placeholder   "`), the alignment
    /// pass would re-center the un-trimmed leading padding and shift the label
    /// one column right — the `docs/examples/how-to/containers01` off-by-one.
    #[test]
    fn default_render_emits_bare_label_no_pre_centering() {
        let ph = Placeholder::new("");
        let console = Console::new();
        let options = make_console_options(16, 8);
        let segments = Widget::render(&ph, &console, &options);

        // Exactly one styled segment, no `line()` separators, no padding rows.
        let line_segments = segments.iter().filter(|s| s.text.as_ref() == "\n").count();
        assert_eq!(line_segments, 0, "default variant must be a single line");

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(
            text, "Placeholder",
            "render must return the bare label (no leading/trailing/centering pad)"
        );
    }

    /// The size variant must likewise be a bare "W x H" string.
    #[test]
    fn size_render_emits_bare_string_no_pre_centering() {
        let ph = Placeholder::new("x").with_variant(PlaceholderVariant::Size);
        let console = Console::new();
        let options = make_console_options(20, 5);
        let segments = Widget::render(&ph, &console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "20 x 5");
    }

    #[test]
    fn variant_cycles_on_click() {
        let mut ph = Placeholder::new("test");
        assert_eq!(ph.variant(), PlaceholderVariant::Default);
        ph.cycle_variant();
        assert_eq!(ph.variant(), PlaceholderVariant::Size);
        ph.cycle_variant();
        assert_eq!(ph.variant(), PlaceholderVariant::Text);
        ph.cycle_variant();
        assert_eq!(ph.variant(), PlaceholderVariant::Default);
    }

    #[test]
    fn color_rotation_across_instances() {
        // Use a single-threaded sequence to verify consecutive instances get
        // distinct colors. Due to the global atomic counter and parallel tests,
        // we only verify all three are distinct (palette has 12 colors).
        let p1 = Placeholder::new("a");
        let p2 = Placeholder::new("b");
        let p3 = Placeholder::new("c");
        assert_ne!(p1.color_index, p2.color_index);
        assert_ne!(p2.color_index, p3.color_index);
        assert_ne!(p1.color_index, p3.color_index);
        // All indices are valid palette positions.
        assert!(p1.color_index < PLACEHOLDER_COLORS.len());
        assert!(p2.color_index < PLACEHOLDER_COLORS.len());
        assert!(p3.color_index < PLACEHOLDER_COLORS.len());
    }

    #[test]
    fn size_variant_shows_dimensions() {
        let ph = Placeholder::new("test").with_variant(PlaceholderVariant::Size);
        let text = ph.render_text(80, 24);
        assert_eq!(text, "80 x 24");
    }

    #[test]
    fn text_variant_has_paragraph_breaks() {
        let ph = Placeholder::new("test").with_variant(PlaceholderVariant::Text);
        let text = ph.render_text(80, 24);
        // Python uses "\n\n".join() — check for paragraph breaks.
        assert!(
            text.contains("\n\n"),
            "text variant should contain paragraph breaks"
        );
        // Should repeat LOREM_IPSUM 5 times.
        assert_eq!(text.matches(LOREM_IPSUM).count(), 5);
    }

    #[test]
    fn word_wrap_respects_line_breaks() {
        let text = "hello world\n\nfoo bar";
        let lines = word_wrap(text, 80);
        assert_eq!(lines, vec!["hello world", "", "foo bar"]);
    }

    #[test]
    fn word_wrap_wraps_long_lines() {
        let text = "aaa bbb ccc ddd";
        let lines = word_wrap(text, 7);
        assert_eq!(lines, vec!["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn word_wrap_zero_width() {
        assert!(word_wrap("hello", 0).is_empty());
    }

    #[test]
    fn default_label_fallback() {
        let ph = Placeholder::new("");
        let text = ph.render_text(80, 24);
        assert_eq!(text, "Placeholder");
    }

    #[test]
    fn custom_label() {
        let ph = Placeholder::new("My Panel");
        let text = ph.render_text(80, 24);
        assert_eq!(text, "My Panel");
    }

    #[test]
    fn enabled_handles_click() {
        let mut ph = Placeholder::new("test");
        let event = Event::MouseDown(crate::event::MouseDownEvent {
            x: 0,
            y: 0,
            screen_x: 0,
            screen_y: 0,
            target: NodeId::default(),
        });
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            ph.on_event(&event, &mut __w);
        }
        assert_eq!(ph.variant(), PlaceholderVariant::Size);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<PlaceholderVariantChanged>());
        assert_eq!(
            messages[0]
                .downcast_ref::<PlaceholderVariantChanged>()
                .unwrap()
                .variant,
            "size"
        );
    }

    #[test]
    fn with_variant_sets_seed_class() {
        let ph = Placeholder::new("test").with_variant(PlaceholderVariant::Text);
        assert!(ph.seed.classes.iter().any(|c| c == "-text"));
        assert!(!ph.seed.classes.iter().any(|c| c == "-default"));
    }

    #[test]
    fn style_type_is_placeholder() {
        let ph = Placeholder::new("test");
        assert_eq!(ph.style_type(), "Placeholder");
    }

    /// Python parity (`Placeholder.DEFAULT_CSS`): a `Placeholder` does NOT set a
    /// `height` rule, and the base `Widget` doesn't either. Per Python's box
    /// model (`Widget._get_box_model`), an unset height makes the widget *fill
    /// the full container height* — it is NOT `height: auto` (shrink-to-content)
    /// and NOT a fixed cell height. The Rust layout engine reproduces that for an
    /// unset-CSS-height leaf ONLY when the widget reports no intrinsic layout
    /// height (`layout_height() == None`); a non-`None` value would instead size
    /// the widget to that content height and break the fill semantics (e.g.
    /// `docs/how-to/layout05`, where 19 `Tweet` placeholders must each fill their
    /// column and overflow). Guard that `Placeholder` keeps reporting `None`.
    #[test]
    fn default_placeholder_reports_no_intrinsic_height_so_it_fills_container() {
        let ph = Placeholder::new("#Tweet1");
        assert_eq!(
            Widget::layout_height(&ph),
            None,
            "Placeholder must report no intrinsic height (Python: unset height \
             fills the container); a fixed value here would stop it overflowing"
        );
        // It must also carry no `height` rule in its node seed (parity with
        // Python's `DEFAULT_CSS`, which omits `height`).
        assert!(
            ph.seed.styles.style.height.is_none(),
            "Placeholder default CSS must not set an explicit height"
        );
    }

    #[test]
    fn click_queues_class_ops_on_event_ctx() {
        let mut ph = Placeholder::new("test");
        // Default variant is "-default"; click cycles to "-size".
        let event = Event::MouseDown(crate::event::MouseDownEvent {
            x: 0,
            y: 0,
            screen_x: 0,
            screen_y: 0,
            target: NodeId::default(),
        });
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            ph.on_event(&event, &mut __w);
        }
        assert_eq!(ph.variant(), PlaceholderVariant::Size);
        // Class ops should reflect the variant change. Post-RA2.3 the handler's
        // `ctx.set_class` enqueues AddClass/RemoveClass commands on the deferred
        // queue (not the dispatch EventCtx), so drain the command queue.
        let ops = crate::runtime::drain_class_commands_for_test();
        assert!(
            ops.iter()
                .any(|(_id, op)| matches!(op, crate::event::ClassOp::Remove(c) if c == "-default"))
        );
        assert!(
            ops.iter()
                .any(|(_id, op)| matches!(op, crate::event::ClassOp::Add(c) if c == "-size"))
        );
    }

    /// Python parity (`width: auto` shrink-to-content):
    /// A `Placeholder` with an explicit `width: auto` CSS rule must report its
    /// label text width via `auto_content_width()` so the layout engine shrinks
    /// the widget to that intrinsic width instead of flex-filling.
    ///
    /// Example: `Placeholder(id="auto")` has label `"#auto"` (5 cells) →
    /// `auto_content_width()` = 5.  Without this, the `#auto` bar in
    /// `docs/examples/styles/width_comparison` is 10 cells wide (flex-fill)
    /// instead of Python's 5.
    ///
    /// The UNSET width case (no `width:` in CSS) must still flex-fill, which is
    /// guarded by the separate `content_width() == None` assertion.
    #[test]
    fn auto_content_width_returns_label_cell_width() {
        // Named placeholder: label derived from id → "#auto" = 5 cells.
        let ph = Placeholder::new("").id("auto");
        assert_eq!(
            Widget::auto_content_width(&ph),
            Some(5),
            "`width: auto` Placeholder(id='auto') must shrink to 5 cells (#auto)"
        );

        // Explicit label.
        let ph = Placeholder::new("Hello");
        assert_eq!(Widget::auto_content_width(&ph), Some(5));

        // Fallback label when neither label nor id is given.
        let ph = Placeholder::new("");
        assert_eq!(
            Widget::auto_content_width(&ph),
            Some(rich_rs::cell_len("Placeholder")),
            "empty-label Placeholder must report 'Placeholder' cell width"
        );
    }

    /// `content_width()` must stay `None` so an UNSET width flex-fills the
    /// container (Python's `1fr` default for bare Placeholder instances).
    /// Only the `width: auto` measurement path (`auto_content_width`) opts in.
    #[test]
    fn content_width_stays_none_for_unset_width_flex_fill() {
        let ph = Placeholder::new("Hello");
        assert_eq!(
            Widget::content_width(&ph),
            None,
            "Placeholder with unset width must not leak a content-width hint \
             (would collapse flex-fill to content-width)"
        );
    }

    /// Python parity (`height: auto` shrink-to-content):
    /// A `Placeholder` with an explicit `height: auto` rule must report its
    /// label's line count via `auto_content_height()` so the box model shrinks to
    /// content. The default/size variants are a single line; the text variant
    /// wraps the lorem ipsum. Without this, `docs/examples/styles/padding_all`'s
    /// `#p1` fills the whole grid cell (14 rows) and `content-align: center
    /// middle` shifts the label to the middle row instead of row 0.
    #[test]
    fn auto_content_height_returns_label_line_count() {
        // Default variant: a single rendered line regardless of width.
        let mut ph = Placeholder::new("no padding");
        Widget::on_layout(&mut ph, 10, 14);
        assert_eq!(
            Widget::auto_content_height(&ph),
            Some(1),
            "`height: auto` default-variant Placeholder must shrink to 1 row"
        );

        // Size variant: the "W x H" string is also a single line.
        let mut ph = Placeholder::new("x").with_variant(PlaceholderVariant::Size);
        Widget::on_layout(&mut ph, 20, 5);
        assert_eq!(Widget::auto_content_height(&ph), Some(1));

        // Text variant: lorem ipsum wraps to many lines at a narrow width.
        let mut ph = Placeholder::new("y").with_variant(PlaceholderVariant::Text);
        Widget::on_layout(&mut ph, 20, 10);
        assert!(
            Widget::auto_content_height(&ph).unwrap() > 1,
            "text-variant Placeholder must wrap to more than one row"
        );
    }

    /// The UNSET-height case must still report `None` from BOTH `layout_height()`
    /// (so an unset height flex-fills the container — the layout05 Tweet stack)
    /// — `auto_content_height()` is the only opt-in path for `height: auto`.
    #[test]
    fn layout_height_stays_none_for_unset_height_flex_fill() {
        let ph = Placeholder::new("#Tweet1");
        assert_eq!(
            Widget::layout_height(&ph),
            None,
            "Placeholder unset height must stay None so it flex-fills"
        );
    }
}

/// Simple word-wrap that breaks text on spaces to fit within `width` cells.
/// Respects explicit `\n` line breaks (including blank lines from `\n\n`).
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            // Blank line (from \n\n paragraph break).
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_len = 0usize;

        for word in paragraph.split_whitespace() {
            let word_len = rich_rs::cell_len(word);
            if current.is_empty() {
                current = word.to_string();
                current_len = word_len;
            } else if current_len + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
                current_len += 1 + word_len;
            } else {
                lines.push(current);
                current = word.to_string();
                current_len = word_len;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}
