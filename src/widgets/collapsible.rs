use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{NodeSeed, Widget};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

// ── CollapsibleTitle ────────────────────────────────────────────────────

/// Child widget that renders the title bar of a `Collapsible`.
///
/// Displays the collapsed/expanded symbol followed by the title text. This is a
/// first-class arena node (mirroring Python's `CollapsibleTitle`, a focusable
/// `Static`). The runtime renders it as a child of `Collapsible`; CSS selectors
/// like `CollapsibleTitle { ... }` resolve against this node directly and the
/// arena renderer applies the resolved style (color / text-style / padding).
pub struct CollapsibleTitle {
    title: String,
    collapsed_symbol: String,
    expanded_symbol: String,
    collapsed: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    seed: NodeSeed,
}

impl CollapsibleTitle {
    pub fn new(
        title: impl Into<String>,
        collapsed_symbol: impl Into<String>,
        expanded_symbol: impl Into<String>,
        collapsed: bool,
    ) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("collapsible--title".to_string());
        Self {
            title: title.into(),
            collapsed_symbol: collapsed_symbol.into(),
            expanded_symbol: expanded_symbol.into(),
            collapsed,
            focused: false,
            hovered: false,
            pressed: false,
            seed,
        }
    }

    fn current_symbol(&self) -> &str {
        if self.collapsed {
            &self.collapsed_symbol
        } else {
            &self.expanded_symbol
        }
    }

    /// The label line as Python assembles it: `<symbol> <label>`.
    fn label_text(&self) -> String {
        format!("{} {}", self.current_symbol(), self.title)
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }
}

impl Widget for CollapsibleTitle {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.focused = new.focused;
        self.hovered = new.hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn style_type(&self) -> &'static str {
        "CollapsibleTitle"
    }

    /// `width: auto` — report the intrinsic label width so the box shrinks to
    /// its content (Python parity). The arena renderer adds CSS padding.
    fn auto_content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.label_text()).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    /// Render the symbol + label as plain content. The arena renderer applies
    /// the node's resolved style (color / text-style / padding / background).
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let text = rich_rs::Text::plain(self.label_text());
        text.render(console, options)
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for CollapsibleTitle {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ── CollapsibleContents ─────────────────────────────────────────────────

/// Inner container for a `Collapsible`'s expandable content.
///
/// Mirrors Python's `Collapsible.Contents` (a `Container` subclass) so that the
/// CSS rule `Collapsible.-collapsed > Contents { display: none }` and the
/// `Contents { padding: 1 0 0 3 }` indentation both apply via the standard CSS
/// path. Children are real arena nodes drained via `take_composed_children`.
pub struct CollapsibleContents {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    seed: NodeSeed,
}

impl CollapsibleContents {
    pub fn new(children: Vec<Box<dyn Widget>>) -> Self {
        Self {
            children,
            children_extracted: false,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for CollapsibleContents {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn style_type(&self) -> &'static str {
        "Contents"
    }

    /// Chrome-only render; children render through the arena tree.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let resolved = css::resolve_style(self, &css::selector_meta_generic(self));
        let paints_surface = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set();
        if !paints_surface {
            return Segments::new();
        }
        let height = options.size.1.max(1);
        let mut out = Segments::new();
        for idx in 0..height {
            out.push(Segment::new(" ".repeat(width)));
            if idx + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }

    fn content_width(&self) -> Option<usize> {
        if self.children_extracted {
            return None;
        }
        let mut max_width = 0usize;
        let mut saw = false;
        for child in &self.children {
            if let Some(width) = child.content_width() {
                max_width = max_width.max(width.max(1));
                saw = true;
            }
        }
        if saw { Some(max_width.max(1)) } else { None }
    }

    fn layout_height(&self) -> Option<usize> {
        if self.children_extracted {
            return None;
        }
        let mut total = 0usize;
        for child in &self.children {
            let Some(child_height) = child.layout_height() else {
                return None;
            };
            total = total.saturating_add(child_height.max(1));
        }
        if total == 0 { None } else { Some(total) }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for CollapsibleContents {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ── Collapsible ─────────────────────────────────────────────────────────

pub struct Collapsible {
    title: String,
    collapsed_symbol: String,
    expanded_symbol: String,
    collapsed: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    seed: NodeSeed,
}

impl Collapsible {
    pub fn new(title: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("-collapsed".to_string());
        Self {
            title: title.into(),
            collapsed_symbol: "\u{25b6}".to_string(),
            expanded_symbol: "\u{25bc}".to_string(),
            collapsed: true,
            focused: false,
            hovered: false,
            pressed: false,
            children: Vec::new(),
            children_extracted: false,
            seed,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        if collapsed {
            if !self.seed.classes.contains(&"-collapsed".to_string()) {
                self.seed.classes.push("-collapsed".to_string());
            }
        } else {
            self.seed.classes.retain(|c| c != "-collapsed");
        }
        self
    }

    pub fn collapsed_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.collapsed_symbol = symbol.into();
        self
    }

    pub fn expanded_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.expanded_symbol = symbol.into();
        self
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn add_child(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    /// Read-only access to the collapsible's (not-yet-extracted) children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the collapsible's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `collapsed`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_collapsed(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.collapsed != value {
            let old = self.collapsed;
            self.collapsed = value;
            ctx.record_change(
                "collapsed",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_collapsed(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout().
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    fn toggle_with_ctx(&mut self, ctx: &mut EventCtx) {
        self.collapsed = !self.collapsed;
        if self.collapsed {
            ctx.add_class("-collapsed");
        } else {
            ctx.remove_class("-collapsed");
        }
        ctx.post_message(CollapsibleToggled {
            collapsed: self.collapsed,
        });
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for Collapsible {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    /// Mirror Python's `compose()`: yield the `CollapsibleTitle`, then a
    /// `Contents` container holding the user children. Both become real arena
    /// nodes so the title glyph + label render and (when expanded) the children
    /// render beneath via the standard tree path.
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        let title = CollapsibleTitle::new(
            self.title.clone(),
            self.collapsed_symbol.clone(),
            self.expanded_symbol.clone(),
            self.collapsed,
        );
        let contents = CollapsibleContents::new(std::mem::take(&mut self.children));
        vec![
            Box::new(title) as Box<dyn Widget>,
            Box::new(contents) as Box<dyn Widget>,
        ]
    }

    fn focusable(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.focused = new.focused;
        self.hovered = new.hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn style_type(&self) -> &'static str {
        "Collapsible"
    }

    /// Intrinsic content width for `width: auto` measurement only.
    ///
    /// Python's `Collapsible` is `width: 1fr` (a fill), so this does NOT affect
    /// the default rendering. But when a `Collapsible` is given `width: auto`, it
    /// should size to its title's content (symbol + label) plus the title
    /// component's own padding (`CollapsibleTitle { padding: 0 1 }`). The layout
    /// adds the `Collapsible`'s own box chrome on top of this value, so it must
    /// be padding-independent here. Returning `None` from the default
    /// `content_width()` keeps the unset-width fill behaviour intact.
    fn auto_content_width(&self) -> Option<usize> {
        let symbol = if self.collapsed {
            &self.collapsed_symbol
        } else {
            &self.expanded_symbol
        };
        let label_width = rich_rs::cell_len(symbol)
            .saturating_add(1)
            .saturating_add(rich_rs::cell_len(&self.title));
        // CollapsibleTitle component padding (0 1 => left 1 + right 1).
        Some(label_width.saturating_add(2).max(1))
    }

    /// Chrome-only render. The title and contents render through the arena tree.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let resolved = css::resolve_style(self, &css::selector_meta_generic(self));
        let paints_surface = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set();
        if !paints_surface {
            return Segments::new();
        }
        let height = options.size.1.max(1);
        let mut out = Segments::new();
        for idx in 0..height {
            out.push(Segment::new(" ".repeat(width)));
            if idx + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                if mouse.y == 0 {
                    self.pressed = true;
                    ctx.request_repaint();
                    ctx.set_handled();
                }
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == self.node_id()) && mouse.y == 0 {
                        self.toggle_with_ctx(ctx);
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.focused => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    self.toggle_with_ctx(ctx);
                }
            }
            _ => {}
        }
    }

    fn layout_height(&self) -> Option<usize> {
        // height: auto — computed from the (now extracted) child nodes.
        if self.children_extracted {
            return None;
        }
        // Pre-extraction estimate: title line + (children when expanded).
        let mut total = 1usize;
        if !self.collapsed {
            for child in &self.children {
                match child.layout_height() {
                    Some(height) => total = total.saturating_add(height.max(1)),
                    None => return None,
                }
            }
        }
        Some(total.max(1))
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Collapsible {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Collapsible {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "collapsed" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<bool>(),
                    change.new_value.downcast_ref::<bool>(),
                ) {
                    self.watch_collapsed(old, new, ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions};

    fn make_console_options(width: usize, height: usize) -> ConsoleOptions {
        let mut opts = ConsoleOptions::default();
        opts.size = (width, height);
        opts.max_width = width;
        opts.max_height = height;
        opts
    }

    // ── CollapsibleTitle tests ──────────────────────────────────────────

    #[test]
    fn collapsible_title_renders_collapsed_symbol() {
        let title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", true);
        let console = Console::new();
        let options = make_console_options(20, 1);
        let segments = Widget::render(&title, &console, &options);
        let text: String = segments.iter().map(|s| &*s.text).collect();
        assert!(text.contains("\u{25b6}"));
        assert!(text.contains("Section"));
    }

    #[test]
    fn collapsible_title_renders_expanded_symbol() {
        let title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", false);
        let console = Console::new();
        let options = make_console_options(20, 1);
        let segments = Widget::render(&title, &console, &options);
        let text: String = segments.iter().map(|s| &*s.text).collect();
        assert!(text.contains("\u{25bc}"));
        assert!(text.contains("Section"));
    }

    #[test]
    fn collapsible_title_style_type() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert_eq!(title.style_type(), "CollapsibleTitle");
    }

    #[test]
    fn collapsible_title_focusable() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(title.focusable());
    }

    #[test]
    fn collapsible_title_style_classes() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(
            title
                .seed
                .classes
                .contains(&"collapsible--title".to_string())
        );
    }

    #[test]
    fn collapsible_title_auto_content_width() {
        let title = CollapsibleTitle::new("Hi", ">", "v", true);
        // ">" (1) + " " (1) + "Hi" (2) = 4 (CSS padding is added by the renderer).
        assert_eq!(title.auto_content_width(), Some(4));
    }

    #[test]
    fn collapsible_title_layout_height() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert_eq!(title.layout_height(), Some(1));
    }

    #[test]
    fn collapsible_title_compose_returns_empty() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(title.compose().is_empty());
    }

    // ── Collapsible compose / take_composed_children tests ──────────────

    #[test]
    fn collapsible_compose_returns_empty() {
        let c = Collapsible::new("Section");
        assert!(c.compose().is_empty());
    }

    #[test]
    fn collapsible_take_composed_children_yields_title_and_contents() {
        use crate::widgets::aliases::Static;
        let mut c = Collapsible::new("Section")
            .collapsed(false)
            .with_child(Static::new("child1"))
            .with_child(Static::new("child2"));
        let taken = c.take_composed_children();
        // Python compose() yields exactly [CollapsibleTitle, Contents].
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].style_type(), "CollapsibleTitle");
        assert_eq!(taken[1].style_type(), "Contents");
        // Extraction is idempotent.
        assert!(c.take_composed_children().is_empty());
    }

    #[test]
    fn collapsible_contents_holds_user_children() {
        use crate::widgets::aliases::Static;
        let mut contents =
            CollapsibleContents::new(vec![Box::new(Static::new("a")), Box::new(Static::new("b"))]);
        let kids = contents.take_composed_children();
        assert_eq!(kids.len(), 2);
        assert!(contents.take_composed_children().is_empty());
    }

    #[test]
    fn collapsible_contents_style_type() {
        let contents = CollapsibleContents::new(Vec::new());
        assert_eq!(contents.style_type(), "Contents");
    }

    // ── Collapsible state tests ─────────────────────────────────────────

    #[test]
    fn collapsible_toggle_flips_state() {
        let mut c = Collapsible::new("Section");
        assert!(c.is_collapsed());
        c.toggle();
        assert!(!c.is_collapsed());
        c.toggle();
        assert!(c.is_collapsed());
    }

    #[test]
    fn collapsible_builder_collapsed_sets_class() {
        let c = Collapsible::new("Section").collapsed(false);
        assert!(!c.is_collapsed());
        assert!(!c.seed.classes.contains(&"-collapsed".to_string()));
        let c2 = Collapsible::new("Section").collapsed(true);
        assert!(c2.is_collapsed());
        assert!(c2.seed.classes.contains(&"-collapsed".to_string()));
    }

    #[test]
    fn collapsible_builder_symbols_propagate_to_title() {
        let mut c = Collapsible::new("Section")
            .collapsed(true)
            .collapsed_symbol("+")
            .expanded_symbol("-");
        let taken = c.take_composed_children();
        let console = Console::new();
        let options = make_console_options(20, 1);
        let text: String = Widget::render(taken[0].as_ref(), &console, &options)
            .iter()
            .map(|s| &*s.text)
            .collect();
        assert!(text.contains('+'));
    }

    #[test]
    fn collapsible_style_type() {
        let c = Collapsible::new("Section");
        assert_eq!(c.style_type(), "Collapsible");
    }

    #[test]
    fn collapsible_focus_syncs_state() {
        use crate::widgets::NodeState;
        let mut c = Collapsible::new("Section");
        assert!(!c.focused);
        c.on_node_state_changed(
            NodeState::default(),
            NodeState {
                focused: true,
                ..Default::default()
            },
        );
        assert!(c.focused);
    }
}
