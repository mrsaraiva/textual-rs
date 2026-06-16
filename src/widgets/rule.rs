use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{NodeSeed, Widget};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// Orientation of a rule separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOrientation {
    Horizontal,
    Vertical,
}

/// Line drawing style for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Ascii,
    Blank,
    Dashed,
    Double,
    Heavy,
    Hidden,
    None,
    Solid,
    Thick,
}

impl LineStyle {
    fn horizontal_char(self) -> &'static str {
        match self {
            LineStyle::Ascii => "-",
            LineStyle::Blank | LineStyle::Hidden | LineStyle::None => " ",
            LineStyle::Dashed => "╍",
            LineStyle::Double => "═",
            LineStyle::Heavy => "━",
            LineStyle::Solid => "─",
            LineStyle::Thick => "█",
        }
    }

    fn vertical_char(self) -> &'static str {
        match self {
            LineStyle::Ascii => "|",
            LineStyle::Blank | LineStyle::Hidden | LineStyle::None => " ",
            LineStyle::Dashed => "╏",
            LineStyle::Double => "║",
            LineStyle::Heavy => "┃",
            LineStyle::Solid => "│",
            LineStyle::Thick => "█",
        }
    }
}

/// A rule widget to separate content, similar to an `<hr>` HTML tag.
///
/// Renders a horizontal or vertical line using box-drawing characters.
/// Not focusable or interactive.
#[derive(Debug, Clone)]
pub struct Rule {
    orientation: RuleOrientation,
    line_style: LineStyle,
    /// Mirror of the DOM classes (`rule`, `-horizontal`/`-vertical`) so off-tree
    /// style resolution (`content_width`, `render`) sees the orientation variant
    /// and matches the default-CSS selectors `Rule.-horizontal` / `Rule.-vertical`.
    classes: Vec<String>,
    seed: NodeSeed,
}

impl Rule {
    pub fn new(orientation: RuleOrientation) -> Self {
        let class = match orientation {
            RuleOrientation::Horizontal => "-horizontal",
            RuleOrientation::Vertical => "-vertical",
        };
        let classes = vec!["rule".to_string(), class.to_string()];
        let seed = NodeSeed {
            classes: classes.clone(),
            ..Default::default()
        };
        Self {
            orientation,
            line_style: LineStyle::Solid,
            classes,
            seed,
        }
    }

    /// Create a horizontal rule (default).
    pub fn horizontal() -> Self {
        Self::new(RuleOrientation::Horizontal)
    }

    /// Create a vertical rule.
    pub fn vertical() -> Self {
        Self::new(RuleOrientation::Vertical)
    }

    /// Set the line drawing style.
    pub fn line_style(mut self, style: LineStyle) -> Self {
        self.line_style = style;
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn orientation(&self) -> RuleOrientation {
        self.orientation
    }

    /// Get the current line style.
    pub fn get_line_style(&self) -> LineStyle {
        self.line_style
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `orientation`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers watcher dispatch for class updates.
    pub fn set_orientation(&mut self, orientation: RuleOrientation, ctx: &mut ReactiveCtx) {
        if self.orientation != orientation {
            let old = self.orientation;
            self.orientation = orientation;
            ctx.record_change(
                "orientation",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(orientation),
            );
        }
    }

    /// Reactive setter for `line_style`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_line_style(&mut self, style: LineStyle, ctx: &mut ReactiveCtx) {
        if self.line_style != style {
            let old = self.line_style;
            self.line_style = style;
            ctx.record_change(
                "line_style",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(style),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_orientation(
        &mut self,
        old: &RuleOrientation,
        new: &RuleOrientation,
        ctx: &mut ReactiveCtx,
    ) {
        let old_class = match old {
            RuleOrientation::Horizontal => "-horizontal",
            RuleOrientation::Vertical => "-vertical",
        };
        let new_class = match new {
            RuleOrientation::Horizontal => "-horizontal",
            RuleOrientation::Vertical => "-vertical",
        };
        ctx.remove_class(old_class);
        ctx.add_class(new_class);
        self.classes.retain(|c| c != old_class);
        if !self.classes.iter().any(|c| c == new_class) {
            self.classes.push(new_class.to_string());
        }
    }
}

impl Widget for Rule {
    fn focusable(&self) -> bool {
        false
    }

    fn content_width(&self) -> Option<usize> {
        match self.orientation {
            RuleOrientation::Horizontal => None, // expand to fill
            RuleOrientation::Vertical => {
                let meta = crate::css::selector_meta_generic(self);
                let resolved = crate::css::resolve_style(self, &meta);
                let padding = resolved.effective_padding();
                let (_, _, border_left, border_right) =
                    super::helpers::border_spacing_from_style(&resolved);
                let chrome_lr = usize::from(padding.left.saturating_add(padding.right))
                    + border_left
                    + border_right;
                Some(1usize.saturating_add(chrome_lr).max(1))
            }
        }
    }

    fn layout_height(&self) -> Option<usize> {
        match self.orientation {
            RuleOrientation::Horizontal => Some(1),
            RuleOrientation::Vertical => None, // expand to fill
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let meta = crate::css::selector_meta_generic(self);
        let style = crate::css::resolve_style(self, &meta)
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        match self.orientation {
            RuleOrientation::Horizontal => {
                let ch = self.line_style.horizontal_char();
                let line: String = ch.repeat(width);
                out.push(Segment::styled(line, style));
            }
            RuleOrientation::Vertical => {
                let ch = self.line_style.vertical_char();
                for row in 0..height {
                    let mut text = ch.to_string();
                    if width > 1 {
                        text.push_str(&" ".repeat(width - 1));
                    }
                    out.push(Segment::styled(text, style));
                    if row + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
        }

        out
    }

    fn style_type(&self) -> &'static str {
        "Rule"
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

impl Renderable for Rule {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Rule {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "orientation" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<RuleOrientation>(),
                        change.new_value.downcast_ref::<RuleOrientation>(),
                    ) {
                        self.watch_orientation(old, new, ctx);
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
    use crate::reactive::ReactiveCtx;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn horizontal_default_orientation() {
        let r = Rule::horizontal();
        assert_eq!(r.orientation(), RuleOrientation::Horizontal);
    }

    #[test]
    fn vertical_constructor() {
        let r = Rule::vertical();
        assert_eq!(r.orientation(), RuleOrientation::Vertical);
    }

    #[test]
    fn default_line_style_is_solid() {
        let r = Rule::horizontal();
        assert_eq!(r.get_line_style(), LineStyle::Solid);
    }

    #[test]
    fn builder_line_style() {
        let r = Rule::horizontal().line_style(LineStyle::Dashed);
        assert_eq!(r.get_line_style(), LineStyle::Dashed);
    }

    #[test]
    fn set_line_style_changes() {
        let mut r = Rule::horizontal();
        let mut ctx = ReactiveCtx::new(make_node_id());
        r.set_line_style(LineStyle::Heavy, &mut ctx);
        assert_eq!(r.get_line_style(), LineStyle::Heavy);
    }

    #[test]
    fn set_orientation_updates_class_ops() {
        use crate::event::ClassOp;
        let mut r = Rule::horizontal();
        let node_id = make_node_id();
        let mut ctx = ReactiveCtx::new(node_id);

        r.set_orientation(RuleOrientation::Vertical, &mut ctx);
        assert_eq!(r.orientation(), RuleOrientation::Vertical);

        // Drain changes and run reactive_dispatch to trigger watch_orientation.
        let changes = ctx.take_changes();
        r.reactive_dispatch(&changes, &mut ctx);

        let ops = ctx.take_class_ops();
        let removes: Vec<_> = ops
            .iter()
            .filter(|(_, op)| matches!(op, ClassOp::Remove(c) if c == "-horizontal"))
            .collect();
        let adds: Vec<_> = ops
            .iter()
            .filter(|(_, op)| matches!(op, ClassOp::Add(c) if c == "-vertical"))
            .collect();
        assert!(!removes.is_empty(), "should remove -horizontal");
        assert!(!adds.is_empty(), "should add -vertical");
    }

    #[test]
    fn set_orientation_noop_same() {
        let mut r = Rule::horizontal();
        let mut ctx = ReactiveCtx::new(make_node_id());
        // Setting same orientation should record no changes.
        r.set_orientation(RuleOrientation::Horizontal, &mut ctx);
        let changes = ctx.take_changes();
        r.reactive_dispatch(&changes, &mut ctx);
        let ops = ctx.take_class_ops();
        assert!(ops.is_empty(), "no class ops for noop orientation change");
    }

    #[test]
    fn not_focusable() {
        let r = Rule::horizontal();
        assert!(!r.focusable());
    }

    #[test]
    fn horizontal_content_width_is_none() {
        let r = Rule::horizontal();
        assert_eq!(r.content_width(), None);
    }

    #[test]
    fn vertical_content_width_is_one() {
        let r = Rule::vertical();
        assert_eq!(r.content_width(), Some(1));
    }

    #[test]
    fn horizontal_layout_height_is_one() {
        let r = Rule::horizontal();
        assert_eq!(r.layout_height(), Some(1));
    }

    #[test]
    fn vertical_layout_height_is_none() {
        let r = Rule::vertical();
        assert_eq!(r.layout_height(), None);
    }

    #[test]
    fn style_type_is_rule() {
        let r = Rule::horizontal();
        assert_eq!(r.style_type(), "Rule");
    }

    #[test]
    fn horizontal_line_chars_all_styles() {
        // Verify each line style maps to a non-empty character
        let styles = [
            LineStyle::Ascii,
            LineStyle::Blank,
            LineStyle::Dashed,
            LineStyle::Double,
            LineStyle::Heavy,
            LineStyle::Hidden,
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Thick,
        ];
        for s in styles {
            let ch = s.horizontal_char();
            assert!(
                !ch.is_empty(),
                "horizontal_char for {:?} should not be empty",
                s
            );
        }
    }

    #[test]
    fn vertical_line_chars_all_styles() {
        let styles = [
            LineStyle::Ascii,
            LineStyle::Blank,
            LineStyle::Dashed,
            LineStyle::Double,
            LineStyle::Heavy,
            LineStyle::Hidden,
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Thick,
        ];
        for s in styles {
            let ch = s.vertical_char();
            assert!(
                !ch.is_empty(),
                "vertical_char for {:?} should not be empty",
                s
            );
        }
    }

    #[test]
    fn horizontal_char_specific_values() {
        assert_eq!(LineStyle::Ascii.horizontal_char(), "-");
        assert_eq!(LineStyle::Solid.horizontal_char(), "─");
        assert_eq!(LineStyle::Heavy.horizontal_char(), "━");
        assert_eq!(LineStyle::Dashed.horizontal_char(), "╍");
        assert_eq!(LineStyle::Double.horizontal_char(), "═");
        assert_eq!(LineStyle::Thick.horizontal_char(), "█");
    }

    #[test]
    fn vertical_char_specific_values() {
        assert_eq!(LineStyle::Ascii.vertical_char(), "|");
        assert_eq!(LineStyle::Solid.vertical_char(), "│");
        assert_eq!(LineStyle::Heavy.vertical_char(), "┃");
        assert_eq!(LineStyle::Dashed.vertical_char(), "╏");
        assert_eq!(LineStyle::Double.vertical_char(), "║");
        assert_eq!(LineStyle::Thick.vertical_char(), "█");
    }

    #[test]
    fn horizontal_rule_carries_variant_class() {
        // DOM variant class must match the default CSS selector `Rule.-horizontal`
        // (Python adds `-horizontal`), not the old `rule--horizontal`.
        let mut r = Rule::horizontal();
        let seed = r.take_node_seed();
        assert!(seed.classes.iter().any(|c| c == "rule"));
        assert!(seed.classes.iter().any(|c| c == "-horizontal"));
    }

    #[test]
    fn vertical_rule_carries_variant_class() {
        let mut r = Rule::vertical();
        let seed = r.take_node_seed();
        assert!(seed.classes.iter().any(|c| c == "rule"));
        assert!(seed.classes.iter().any(|c| c == "-vertical"));
    }

    #[test]
    fn default_css_applies_horizontal_vertical_margins() {
        // Regression: the Rule's `-horizontal` / `-vertical` variant classes must
        // match the default-CSS selectors (`Rule.-horizontal` / `Rule.-vertical`)
        // so the layout engine sees the orientation margins. Previously the widget
        // added `rule--horizontal` / `rule--vertical`, which never matched, so the
        // vertical margins (`margin: 1 0`) and horizontal margins (`margin: 0 2`)
        // were silently dropped.
        let _guard =
            crate::css::set_style_context(crate::css::default_widget_stylesheet());

        let h = Rule::horizontal();
        let h_meta = crate::css::selector_meta_generic(&h);
        let h_margin = crate::css::resolve_style(&h, &h_meta).effective_margin();
        assert_eq!(h_margin.top, 1, "horizontal rule top margin (margin: 1 0)");
        assert_eq!(h_margin.bottom, 1, "horizontal rule bottom margin (margin: 1 0)");
        assert_eq!(h_margin.left, 0, "horizontal rule left margin (margin: 1 0)");
        assert_eq!(h_margin.right, 0, "horizontal rule right margin (margin: 1 0)");

        let v = Rule::vertical();
        let v_meta = crate::css::selector_meta_generic(&v);
        let v_margin = crate::css::resolve_style(&v, &v_meta).effective_margin();
        assert_eq!(v_margin.left, 2, "vertical rule left margin (margin: 0 2)");
        assert_eq!(v_margin.right, 2, "vertical rule right margin (margin: 0 2)");
        assert_eq!(v_margin.top, 0, "vertical rule top margin (margin: 0 2)");
        assert_eq!(v_margin.bottom, 0, "vertical rule bottom margin (margin: 0 2)");
    }

    #[test]
    fn round_trip_orientation_switch() {
        let mut r = Rule::horizontal();
        let mut ctx = ReactiveCtx::new(make_node_id());
        r.set_orientation(RuleOrientation::Vertical, &mut ctx);
        r.set_orientation(RuleOrientation::Horizontal, &mut ctx);
        assert_eq!(r.orientation(), RuleOrientation::Horizontal);
    }
}
