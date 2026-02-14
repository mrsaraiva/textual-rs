use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;
use crate::style::Color;

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};
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
    hovered: bool,
    disabled: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Placeholder {
    pub fn new(label: impl Into<String>) -> Self {
        let color_index = COLOR_COUNTER.fetch_add(1, Ordering::Relaxed) % PLACEHOLDER_COLORS.len();
        let label = label.into();
        Self {
            label,
            variant: PlaceholderVariant::Default,
            color_index,
            last_width: 0,
            last_height: 0,
            hovered: false,
            disabled: false,
            classes: vec!["placeholder".to_string(), "-default".to_string()],
            styles: WidgetStyles::default(),
        }
        .apply_bg_color()
    }

    /// Set the initial disabled state (builder pattern).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_variant(mut self, variant: PlaceholderVariant) -> Self {
        self.variant = variant;
        self.rebuild_classes();
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

    /// Reactive setter for `disabled`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_disabled(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.disabled != value {
            let old = self.disabled;
            self.disabled = value;
            ctx.record_change(
                "disabled",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_variant(
        &mut self,
        _old: &PlaceholderVariant,
        _new: &PlaceholderVariant,
        _ctx: &mut ReactiveCtx,
    ) {
        self.rebuild_classes();
    }

    pub fn cycle_variant(&mut self) {
        self.variant = self.variant.next();
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        self.classes = vec![
            "placeholder".to_string(),
            self.variant.class_name().to_string(),
        ];
    }

    fn apply_bg_color(mut self) -> Self {
        let hex = PLACEHOLDER_COLORS[self.color_index];
        if let Some(color) = crate::style::parse_color_like(hex) {
            // Apply at 50% opacity to match Python Textual's `background: {color} 50%`.
            let bg = Color::rgba(color.r, color.g, color.b, 128);
            self.styles.set_bg(bg);
        }
        self
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

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width as usize;
        self.last_height = height as usize;
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn set_disabled_state(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.cycle_variant();
                ctx.post_message(Message::PlaceholderVariantChanged(
                    PlaceholderVariantChanged {
                        variant: self.variant.message_name().to_string(),
                    },
                ));
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

        match self.variant {
            PlaceholderVariant::Text => {
                // Word-wrap the text to fill the area.
                let lines = word_wrap(&text, width);
                for row in 0..height {
                    let content = lines.get(row).map(|s| s.as_str()).unwrap_or("");
                    let line = rich_rs::set_cell_size(content, width);
                    out.push(Segment::styled(line, style));
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
            _ => {
                // Center the text both horizontally and vertically.
                let text_width = rich_rs::cell_len(&text).min(width);
                let vert_pad = height.saturating_sub(1) / 2;

                for row in 0..height {
                    if row == vert_pad {
                        let left = width.saturating_sub(text_width) / 2;
                        let right = width.saturating_sub(text_width + left);
                        let line = format!(
                            "{}{}{}",
                            " ".repeat(left),
                            rich_rs::set_cell_size(&text, text_width),
                            " ".repeat(right)
                        );
                        out.push(Segment::styled(line, style));
                    } else {
                        out.push(Segment::styled(" ".repeat(width), style));
                    }
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Placeholder"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
            match change.field_name {
                "variant" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<PlaceholderVariant>(),
                        change.new_value.downcast_ref::<PlaceholderVariant>(),
                    ) {
                        self.watch_variant(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::NodeId;

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
    fn disabled_blocks_events() {
        let mut ph = Placeholder::new("test").disabled(true);
        assert!(ph.is_disabled());
        // Create a fake mouse event targeting this placeholder.
        let event = Event::MouseDown(crate::event::MouseDownEvent {
            x: 0,
            y: 0,
            screen_x: 0,
            screen_y: 0,
            target: NodeId::default(),
        });
        let mut ctx = EventCtx::default();
        ph.on_event(&event, &mut ctx);
        // Should remain Default because disabled blocks event handling.
        assert_eq!(ph.variant(), PlaceholderVariant::Default);
        assert!(!ctx.handled());
        assert!(ctx.take_messages().is_empty());
    }

    #[test]
    fn enabled_handles_click() {
        let mut ph = Placeholder::new("test");
        assert!(!ph.is_disabled());
        let event = Event::MouseDown(crate::event::MouseDownEvent {
            x: 0,
            y: 0,
            screen_x: 0,
            screen_y: 0,
            target: NodeId::default(),
        });
        let mut ctx = EventCtx::default();
        ph.on_event(&event, &mut ctx);
        assert_eq!(ph.variant(), PlaceholderVariant::Size);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::PlaceholderVariantChanged(PlaceholderVariantChanged { ref variant }) if variant == "size"
        ));
    }

    #[test]
    fn with_variant_sets_classes() {
        let ph = Placeholder::new("test").with_variant(PlaceholderVariant::Text);
        let classes = ph.style_classes();
        assert!(classes.iter().any(|c| c == "-text"));
    }

    #[test]
    fn style_type_is_placeholder() {
        let ph = Placeholder::new("test");
        assert_eq!(ph.style_type(), "Placeholder");
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
