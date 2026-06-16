use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};

use crate::widgets::{NodeSeed, Widget, helpers::apply_debug_box};

pub struct Container {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    seed: NodeSeed,
    /// (index into `children`, css_id, classes) recorded by `with_compose` so
    /// `.with_id()`/`.with_classes()` metadata on declared children reaches the
    /// mounted node.
    child_decl_meta: Vec<crate::widgets::ChildDeclMeta>,
    /// (index into `children`, sink) recorded by `with_compose` for decls bound
    /// via `HandleSlot::bind`.
    child_handle_sinks: Vec<(usize, crate::handle::HandleSink)>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            seed: NodeSeed::default(),
            child_decl_meta: Vec::new(),
            child_handle_sinks: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children from a `compose![]` result.
    ///
    /// Preserves each `ChildDecl`'s `id`/`classes` (so CSS id/class selectors
    /// match the mounted nodes) and any `handle_sink` bound via
    /// `HandleSlot::bind`, mirroring `App::mount_declarations`.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        for decl in children {
            let crate::compose::ChildDecl {
                builder,
                id,
                classes,
                handle_sink,
                ..
            } = decl;
            let crate::compose::WidgetBuilder::Ready(widget) = builder;
            let index = self.children.len();
            self.children.push(widget);
            if id.is_some() || !classes.is_empty() {
                self.child_decl_meta.push((index, id, classes));
            }
            if let Some(sink) = handle_sink {
                self.child_handle_sinks.push((index, sink));
            }
        }
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    /// Read-only access to the container's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the container's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    /// Mutable access to the pre-mount `NodeSeed` (css_id, classes, inline styles).
    ///
    /// Valid until the widget is mounted into the arena tree; after mount the
    /// node record is the single source of truth and seed changes have no effect.
    pub fn seed_mut(&mut self) -> &mut NodeSeed {
        &mut self.seed
    }
}

impl Widget for Container {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn take_child_handle_sinks(&mut self) -> Vec<(usize, crate::handle::HandleSink)> {
        std::mem::take(&mut self.child_handle_sinks)
    }

    fn take_child_decl_meta(&mut self) -> Vec<crate::widgets::ChildDeclMeta> {
        std::mem::take(&mut self.child_decl_meta)
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        // Chrome-only render. Children are rendered through the arena tree.
        let meta = css::selector_meta_generic(self);
        let resolved = css::resolve_style(self, &meta);
        let paints_surface = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set()
            || resolved.outline_top.is_set()
            || resolved.outline_right.is_set()
            || resolved.outline_bottom.is_set()
            || resolved.outline_left.is_set();
        if !paints_surface {
            return Segments::new();
        }

        let lines = vec![vec![Segment::new(" ".repeat(width))]; height_limit];
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let rendered = Widget::render(self, console, options);
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let mut lines = Segment::split_and_crop_lines(rendered, width, None, true, false);
        lines = Segment::set_shape(&lines, width, Some(height_limit), None, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height_limit}"))
        } else {
            None
        };
        let boxed = apply_debug_box(
            lines,
            width,
            height_limit,
            label.as_deref(),
            debug.style_for(0),
        );
        let line_count = boxed.len();
        let mut out = Segments::new();
        for (idx, line) in boxed.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_layout(&mut self, _width: u16, _height: u16) {}

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
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

    fn style(&self) -> Option<crate::style::Style> {
        if self.seed.styles.style != Default::default() {
            Some(self.seed.styles.style.clone())
        } else {
            None
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Container {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Label;

    #[test]
    fn compose_returns_empty() {
        let c = Container::new().with_child(Label::new("a"));
        assert!(c.compose().is_empty());
    }

    #[test]
    fn take_composed_children_extracts_all() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = c.take_composed_children();
        assert_eq!(children.len(), 2);
        // After extraction, internal Vec is empty.
        assert!(c.children().is_empty());
    }

    #[test]
    fn extraction_is_idempotent() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let _ = c.take_composed_children();
        assert!(c.take_composed_children().is_empty());
    }

    #[test]
    fn tree_mode_render_transparent_container_is_empty() {
        let mut c = Container::new()
            .with_child(Label::new("hello"))
            .with_child(Label::new("world"));
        let _ = c.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 4);
        options.max_width = 10;
        options.max_height = 4;
        let segments = Widget::render(&c, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn tree_mode_render_styled_container_produces_surface_fill() {
        let mut c = Container::new()
            .with_child(Label::new("hello"))
            .with_child(Label::new("world"));
        c.seed.styles.style.bg = Some(crate::style::Color::rgb(10, 20, 30));
        let _ = c.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 4);
        options.max_width = 10;
        options.max_height = 4;
        let segments = Widget::render(&c, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn tree_mode_on_event_does_not_panic() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();

        let mut ctx = EventCtx::default();
        // Key event should not panic even though children are gone.
        c.on_event(&Event::Action(crate::event::Action::FocusNext), &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn tree_mode_on_mouse_move_returns_false() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();
        assert!(!c.on_mouse_move(0, 0));
    }

    #[test]
    fn tree_mode_layout_height_returns_none() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();
        // Without fixed constraints, tree mode returns None.
        assert!(c.layout_height().is_none());
    }
}
