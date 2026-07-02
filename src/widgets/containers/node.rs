use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::style::Style;

use crate::widgets::{LayoutConstraints, NodeSeed, Spacer, Widget};

pub struct Node {
    child: Box<dyn Widget>,
    seed: NodeSeed,
    child_extracted: bool,
    /// Optional text drawn on the top border (Python `widget.border_title`).
    /// The `Node` wrapper carries the border (via its CSS class/id), so the
    /// title must live here rather than on the inner content child.
    border_title: Option<String>,
    /// Optional text drawn on the bottom border (Python `widget.border_subtitle`).
    border_subtitle: Option<String>,
}

impl Node {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            seed: NodeSeed::default(),
            child_extracted: false,
            border_title: None,
            border_subtitle: None,
        }
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

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.seed.css_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.seed.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.seed.classes.push(value.into());
        }
        self
    }

    fn seed_constraints(&self) -> LayoutConstraints {
        self.seed.styles.layout
    }
}

impl Widget for Node {
    fn compose(&mut self) -> crate::compose::ComposeResult {
        // Non-collapse path: a `Node` that stays a real arena node (a classed,
        // border-titled, or otherwise styled box — see `elide_transparent_wrapper`)
        // mounts its single inner child beneath it. When the wrapper instead
        // collapses out, the mount pipeline consumes `elide_transparent_wrapper`
        // FIRST and this is never reached (so the child is drained exactly once).
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![crate::compose::ChildDecl::new(child)]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let _ = (console, options);
        Segments::new()
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let _ = (console, options, debug);
        Segments::new()
    }

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn focusable(&self) -> bool {
        false
    }

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.seed_constraints();
        if let (Some(min), Some(max)) = (constraints.min_height, constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        // After `compose`, the real child is moved into the arena
        // tree and `self.child` is a placeholder `Spacer(1)`. Reporting the
        // placeholder's height (1) would clip the arena child to a single row.
        // Mirror `Container`: defer to the arena layout (which sizes this node
        // from its real tree child) by reporting no intrinsic height.
        if self.child_extracted {
            return None;
        }
        self.child.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        // Same rationale as `layout_height`: once the real child is in the arena
        // tree, don't report the placeholder Spacer's width.
        if self.child_extracted {
            return None;
        }
        self.child.content_width()
    }

    fn style(&self) -> Option<Style> {
        let s = self.seed.styles.style.clone();
        if s == Default::default() {
            None
        } else {
            Some(s)
        }
    }

    fn style_type(&self) -> &'static str {
        "Node"
    }

    fn is_transparent_wrapper(&self) -> bool {
        true
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn elide_transparent_wrapper(&mut self) -> Option<(Box<dyn Widget>, NodeSeed)> {
        // Collapse a *purely structural* transparent `Node` OUT of the arena: its
        // single inner widget is mounted in the wrapper's place and owns the
        // forwarded id/inline-style. This matches Python, where attaching an id is
        // a property of one widget (`Placeholder(id="page-0")`,
        // `HorizontalScroll(id="page-container")`) — never an extra wrapper node.
        // Keeping the wrapper interposes a `width:auto` (shrink-to-content) layout
        // layer ABOVE the inner widget, which:
        //   - drops the inner widget's explicit size (a `width:100vw` page shrinks
        //     to its text), and
        //   - prevents a wrapped scroll container from establishing its own scroll
        //     viewport (so id-targeted mounts / `scroll_visible` land in a node
        //     with no laid-out region).
        //
        // Collapse is gated to wrappers that carry NO CSS CLASS of their own: an
        // id-only wrapper (or a scroll-host wrapper) is pure targeting and folds
        // cleanly onto the inner widget. A `.class(..)` wrapper (e.g.
        // `Static.class("words")` whose class carries `border`/`background`/
        // `width:auto`, or `Bar.class("red")` carrying a background) is a real
        // STYLED box: the wrapper is the rendered surface and must stay, so its
        // class styling is not silently moved onto a differently-rendering inner
        // widget. (A classed *scroll host* still collapses — it must own its
        // viewport — and its class travels with the forwarded seed.)
        //
        // A `Node` carrying a border title/subtitle is always a styled box and
        // never collapses.
        if self.child_extracted {
            return None;
        }
        if self.border_title.is_some() || self.border_subtitle.is_some() {
            return None;
        }
        if self.seed.css_id.is_none() && self.seed.classes.is_empty() {
            // Nothing to forward; keep the wrapper (may be a style-only stand-in).
            return None;
        }
        let is_scroll_host = self.child.clips_descendants_to_content();
        if !self.seed.classes.is_empty() && !is_scroll_host {
            // Classed, non-scroll wrapper: a real styled box — keep it (the class
            // styling renders on the wrapper, not the inner widget).
            return None;
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        let seed = std::mem::take(&mut self.seed);
        Some((child, seed))
    }

    // NOTE: a *classed* non-scroll `Node` wrapper (e.g.
    // `Node::new(Placeholder).class("box")`) is deliberately NOT collapsed by
    // `elide_transparent_wrapper` and keeps its own id/classes — it is a real
    // STYLED box whose class carries layout styling (`width:1fr`/`border`/`bg`)
    // that must render on the wrapper. Moving that class onto the inner widget
    // would double-apply the styling and corrupt sizing. The two cases that
    // genuinely need the INNER widget to own the identity — an id-only wrapper
    // and a scroll-host wrapper — collapse the wrapper out entirely (so there is
    // no wrapper to double-apply against). Non-`Node` id-bearing widgets
    // propagate an externally-declared id via `set_seed_css_id` on their own seed.

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
    }

    fn style_id(&self) -> Option<&str> {
        self.seed.css_id.as_deref()
    }

    fn border_title(&self) -> Option<&str> {
        self.border_title.as_deref()
    }

    fn border_subtitle(&self) -> Option<&str> {
        self.border_subtitle.as_deref()
    }
}

impl Renderable for Node {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_compose_returns_child() {
        let mut n = Node::new(Spacer::new(1));
        let children = n.compose();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn node_compose_idempotent() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.compose();
        assert!(n.compose().is_empty());
    }

    #[test]
    fn node_render_after_extraction() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.compose();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&n, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn node_style_type_after_extraction() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.compose();
        assert_eq!(n.style_type(), "Node");
    }
}
