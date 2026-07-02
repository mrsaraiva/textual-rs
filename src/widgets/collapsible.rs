use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::content::Content;
use crate::css;
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{NodeSeed, Widget};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// Internal message posted by a focused/clicked [`CollapsibleTitle`] asking its
/// parent [`Collapsible`] to toggle.
///
/// Mirrors Python `_collapsible.CollapsibleTitle.Toggle`: the title is the
/// focusable node and, on `enter`/click, posts a `Toggle` message that bubbles
/// to the enclosing `Collapsible`, which flips its `collapsed` state. Keeping
/// the toggle on the title (rather than the container) is what makes `:focus`
/// land on the title so its `&:focus { background: $block-cursor-background }`
/// rule applies (Python parity for the focused header surface).
#[derive(Debug, Clone)]
struct CollapsibleTitleToggle;
crate::impl_message!(CollapsibleTitleToggle);

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying CSS text attributes that have already been baked in by
/// `Content::render_strips`.
fn tag_segment_no_text_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
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
    crate::seed_ident_methods!();

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
    fn compose(&mut self) -> ComposeResult {
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

    /// The title is the focusable node (Python `CollapsibleTitle`), so it owns
    /// the toggle interaction: `enter` while focused, or a click, posts a
    /// [`CollapsibleTitleToggle`] that bubbles to the parent `Collapsible`.
    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) if self.pressed => {
                self.pressed = false;
                ctx.request_repaint();
                if mouse.target.is_some_and(|t| t == self.node_id()) {
                    ctx.post_message(CollapsibleTitleToggle);
                }
                ctx.set_handled();
            }
            Event::AppFocus(false) if self.pressed => {
                self.pressed = false;
                ctx.request_repaint();
            }
            Event::Key(key) if self.focused => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    ctx.post_message(CollapsibleTitleToggle);
                    ctx.set_handled();
                }
            }
            _ => {}
        }
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

    /// Render the symbol + label via Content::render_strips.
    /// The arena renderer applies the node's resolved style (color / text-style
    /// / padding / background) on top.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Get the resolved visual style (pushed by render_widget_with_meta).
        let visual_style = crate::css::current_self_style().unwrap_or_default();

        // Flatten the widget's own bg over the composited ancestor background.
        let parent_bg = crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
            crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        // Build Content from the symbol + label text (plain — no rich markup).
        let content = Content::from_text(self.label_text());

        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

        // CollapsibleTitle is always single-line, left-aligned, no word-wrap.
        let strips = content.render_strips(
            width,
            Some(1),
            &render_style,
            crate::style::TextAlign::Left,
            "fold",
            true,
            0,
            resolve_fn,
        );

        let mut out = Segments::new();
        for strip in strips {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                out.push(seg);
            }
        }
        out
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
/// path. Children are real arena nodes drained via `compose`.
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
    fn compose(&mut self) -> ComposeResult {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
            .into_iter()
            .map(crate::compose::ChildDecl::new)
            .collect()
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
            let child_height = child.layout_height()?;
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
    hovered: bool,
    pressed: bool,
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    seed: NodeSeed,
    /// Class add/remove ops staged by a post-mount collapsed-state change made
    /// outside an event handler (e.g. via `App::with_widget_mut_as`). The runtime
    /// drains these in `with_widget_mut` and applies them to the arena node so the
    /// `&.-collapsed > Contents { display: none }` rule matches and the body
    /// shows/hides. Mirrors Python `_update_collapsed` → `set_class(..,-collapsed)`.
    pending_class_ops: Vec<(String, bool)>,
}

impl Collapsible {
    crate::seed_ident_methods!();

    pub fn new(title: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("-collapsed".to_string());
        Self {
            title: title.into(),
            collapsed_symbol: "\u{25b6}".to_string(),
            expanded_symbol: "\u{25bc}".to_string(),
            collapsed: true,
            hovered: false,
            pressed: false,
            children: Vec::new(),
            children_extracted: false,
            seed,
            pending_class_ops: Vec::new(),
        }
    }

    /// Keep the detached seed class in sync (off-tree resolution / re-mount) AND
    /// stage a class op so a post-mount toggle reaches the arena node through the
    /// `drain_pending_class_ops` seam.
    fn sync_collapsed_class(&mut self) {
        if self.collapsed {
            if !self.seed.classes.iter().any(|c| c == "-collapsed") {
                self.seed.classes.push("-collapsed".to_string());
            }
        } else {
            self.seed.classes.retain(|c| c != "-collapsed");
        }
        self.pending_class_ops
            .push(("-collapsed".to_string(), self.collapsed));
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
            // Mirror Python `_update_collapsed`: toggle the `-collapsed` class so
            // the `&.-collapsed > Contents { display: none }` rule matches.
            ctx.set_class(value, "-collapsed");
            if self.collapsed {
                if !self.seed.classes.iter().any(|c| c == "-collapsed") {
                    self.seed.classes.push("-collapsed".to_string());
                }
            } else {
                self.seed.classes.retain(|c| c != "-collapsed");
            }
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_collapsed(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout().
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
        self.sync_collapsed_class();
    }

    fn toggle_with_ctx(&mut self, ctx: &mut EventCtx) {
        self.collapsed = !self.collapsed;
        if self.collapsed {
            ctx.add_class("-collapsed");
            if !self.seed.classes.iter().any(|c| c == "-collapsed") {
                self.seed.classes.push("-collapsed".to_string());
            }
        } else {
            ctx.remove_class("-collapsed");
            self.seed.classes.retain(|c| c != "-collapsed");
        }
        ctx.post_message(CollapsibleToggled {
            collapsed: self.collapsed,
        });
        // The body show/hide is CSS-`display`-driven, which is recomputed during
        // the layout pass — request relayout (not just repaint) so the Contents
        // child's `display:none` toggles and the box re-sizes.
        ctx.request_layout_invalidation();
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for Collapsible {
    /// Mirror Python's `compose()`: yield the `CollapsibleTitle`, then a
    /// `Contents` container holding the user children. Both become real arena
    /// nodes so the title glyph + label render and (when expanded) the children
    /// render beneath via the standard tree path.
    fn compose(&mut self) -> ComposeResult {
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
            crate::compose::ChildDecl::new(Box::new(title) as Box<dyn Widget>),
            crate::compose::ChildDecl::new(Box::new(contents) as Box<dyn Widget>),
        ]
    }

    // NOTE: `Collapsible` is intentionally NOT focusable (Python parity —
    // `class Collapsible(Widget)` has `can_focus=False`). The focusable node is
    // the child `CollapsibleTitle`, so `:focus` lands on the title and its
    // `&:focus { background: $block-cursor-background }` rule applies to the
    // header surface. `can_focus_children()` (default `true`) still lets focus
    // traversal descend into the title.

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
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

    /// Handle the toggle request bubbled up from our `CollapsibleTitle`.
    ///
    /// Mirrors Python `Collapsible._on_collapsible_title_toggle`: the title
    /// (the focusable node) posts a toggle message on `enter`/click, and the
    /// enclosing `Collapsible` flips its `collapsed` state and stops
    /// propagation (so a nested outer `Collapsible` is not also toggled).
    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if message.is::<CollapsibleTitleToggle>() {
            self.toggle_with_ctx(ctx);
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

    fn drain_pending_class_ops(&mut self) -> Vec<(String, bool)> {
        std::mem::take(&mut self.pending_class_ops)
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
        let mut title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(title.compose().is_empty());
    }

    // ── Collapsible compose tests ──────────────

    #[test]
    fn collapsible_compose_yields_title_and_contents() {
        use crate::widgets::aliases::Static;
        let mut c = Collapsible::new("Section")
            .collapsed(false)
            .with_child(Static::new("child1"))
            .with_child(Static::new("child2"));
        let taken = c.compose();
        // Python compose() yields exactly [CollapsibleTitle, Contents].
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].widget().style_type(), "CollapsibleTitle");
        assert_eq!(taken[1].widget().style_type(), "Contents");
        // Extraction is idempotent.
        assert!(c.compose().is_empty());
    }

    #[test]
    fn collapsible_contents_holds_user_children() {
        use crate::widgets::aliases::Static;
        let mut contents =
            CollapsibleContents::new(vec![Box::new(Static::new("a")), Box::new(Static::new("b"))]);
        let kids = contents.compose();
        assert_eq!(kids.len(), 2);
        assert!(contents.compose().is_empty());
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
        let taken = c.compose();
        let console = Console::new();
        let options = make_console_options(20, 1);
        let text: String = Widget::render(taken[0].widget(), &console, &options)
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
    fn collapsible_is_not_focusable_title_is() {
        // Python parity: `Collapsible` (a plain `Widget`) is not focusable; the
        // focusable node is the child `CollapsibleTitle`, so `:focus` lands on
        // the title and lightens the header surface.
        let c = Collapsible::new("Section");
        assert!(!c.focusable(), "Collapsible must not be focusable");
        assert!(c.can_focus_children(), "focus must descend into the title");
        let title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", true);
        assert!(title.focusable(), "CollapsibleTitle must be focusable");
    }

    #[test]
    fn collapsible_title_enter_posts_toggle_when_focused() {
        use crate::event::{Event, EventCtx};
        use crate::keys::KeyEventData;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", true);
        title.on_node_state_changed(
            crate::widgets::NodeState::default(),
            crate::widgets::NodeState {
                focused: true,
                ..Default::default()
            },
        );
        let mut ctx = EventCtx::default();
        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
        title.on_event(&Event::Key(key), &mut ctx);
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1, "focused title must post one toggle message");
        assert!(messages[0].is::<CollapsibleTitleToggle>());
    }

    #[test]
    fn collapsible_title_enter_ignored_when_blurred() {
        use crate::event::{Event, EventCtx};
        use crate::keys::KeyEventData;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", true);
        let mut ctx = EventCtx::default();
        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
        title.on_event(&Event::Key(key), &mut ctx);
        assert!(
            ctx.take_messages().is_empty(),
            "an unfocused title must not post a toggle message"
        );
    }

    #[test]
    fn collapsible_toggles_on_title_message() {
        use crate::event::EventCtx;
        use crate::message::MessageEvent;

        let mut c = Collapsible::new("Section");
        assert!(c.is_collapsed());
        let mut ctx = EventCtx::default();
        let sender = crate::node_id::node_id_from_ffi(1);
        let msg = MessageEvent::new(sender, CollapsibleTitleToggle);
        c.on_message(&msg, &mut ctx);
        assert!(!c.is_collapsed(), "toggle message must flip collapsed state");
        assert!(ctx.handled(), "handling the toggle must stop propagation");
    }
}
