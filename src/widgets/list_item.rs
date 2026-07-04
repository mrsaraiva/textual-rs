//! A list item widget for use with [`ListView`](crate::widgets::ListView).
//!
//! Mirrors Python Textual's `ListItem` (`textual/widgets/_list_item.py`): an
//! arena container that wraps arbitrary child widget(s) — typically a `Label` —
//! with its own CSS. It is **not** focusable; the parent `ListView`
//! (`can_focus_children=False`) drives the highlight by setting the `-highlight`
//! class on the item's node, and the highlight is conveyed **by background only**
//! (no text marker). Hover is tracked via the `-hovered` class.

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::compose::ComposeResult;
use crate::css;
use crate::event::Event;
use crate::message::*;

use super::{Focus, Interactive, Layout, NodeSeed, Render};

/// A widget that is a single item within a [`ListView`](crate::widgets::ListView).
///
/// A `ListItem` is an arena container: its children render through the normal
/// widget tree, and its own render is chrome-only (a styled surface fill that
/// paints the highlight/hover background). It corresponds 1:1 to Python's
/// `ListItem(*children)`.
///
/// # Example
///
/// ```rust
/// use textual::prelude::*;
///
/// let item = ListItem::new(Label::new("One"));
/// assert_eq!(item.text(), "One");
/// ```
#[widget(Focus, Interactive, Layout, style_type = "ListItem")]
pub struct ListItem {
    children: Vec<Box<dyn crate::widgets::Widget>>,
    children_extracted: bool,
    /// Cached text of the first label-like child, used by `ListView` for its
    /// headless state API (`items()`, message payloads). Empty when the child
    /// has no recoverable text.
    text: String,
    /// Ordinal index assigned by the owning `ListView` at compose time so the
    /// click message can report which item was activated without the item
    /// needing to know its arena `NodeId`.
    ordinal: usize,
    disabled: bool,
    seed: NodeSeed,
}

impl ListItem {
    crate::seed_ident_methods!();

    /// Create a new `ListItem` wrapping a single child widget.
    ///
    /// Python: `ListItem(Label("One"))`.
    pub fn new(child: impl crate::widgets::Widget + 'static) -> Self {
        let text = widget_text(&child);
        Self {
            children: vec![Box::new(child)],
            children_extracted: false,
            text,
            ordinal: 0,
            disabled: false,
            seed: NodeSeed::default(),
        }
    }

    /// Create an empty `ListItem` (no children). Useful for building up an item
    /// via [`with_child`](Self::with_child) / [`push`](Self::push).
    pub fn empty() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            text: String::new(),
            ordinal: 0,
            disabled: false,
            seed: NodeSeed::default(),
        }
    }

    /// Create a `ListItem` from a plain string, wrapping it in a `Label`.
    ///
    /// Convenience for the common `ListItem(Label(text))` shape and for the
    /// headless string-based constructors of [`ListView`](crate::widgets::ListView).
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let item = Self::new(crate::widgets::Label::new(text.clone()));
        Self { text, ..item }
    }

    /// Builder: append another child widget to this item.
    pub fn with_child(mut self, child: impl crate::widgets::Widget + 'static) -> Self {
        if self.text.is_empty() {
            self.text = widget_text(&child);
        }
        self.children.push(Box::new(child));
        self
    }

    /// Append another child widget to this item.
    pub fn push(&mut self, child: impl crate::widgets::Widget + 'static) {
        if self.text.is_empty() {
            self.text = widget_text(&child);
        }
        self.children.push(Box::new(child));
    }

    /// Builder: set the CSS id of this item.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
        self
    }

    /// Builder: add a CSS class to this item.
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.seed.classes.push(class.into());
        self
    }

    /// Builder: mark this item disabled (skipped by keyboard navigation).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Whether this item is disabled.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// The recovered text content of this item (first label-like child).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The ordinal index of this item within its `ListView`.
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// Set the ordinal index. Called by `ListView` at compose time.
    pub(crate) fn set_ordinal(&mut self, ordinal: usize) {
        self.ordinal = ordinal;
    }

    /// Read-only access to the item's (not-yet-extracted) children.
    pub fn children(&self) -> &[Box<dyn crate::widgets::Widget>] {
        &self.children
    }
}

/// Best-effort recovery of a child's text for the headless `ListView` API.
fn widget_text(child: &(impl crate::widgets::Widget + 'static)) -> String {
    let any = child as &dyn std::any::Any;
    if let Some(label) = any.downcast_ref::<crate::widgets::Label>() {
        return label.text().to_string();
    }
    String::new()
}

impl Render for ListItem {
    fn compose(&mut self) -> ComposeResult {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
            .into_iter()
            .map(crate::compose::ChildDecl::new)
            .collect()
    }

    /// Chrome-only render: paints the resolved surface (highlight/hover
    /// background) across the item's box. Children render through the arena.
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
}

impl Focus for ListItem {
    fn focusable(&self) -> bool {
        // Python: `ListItem(Widget, can_focus=False)`.
        false
    }
}

impl Interactive for ListItem {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if let Event::MouseDown(mouse) = event {
            if mouse.target == crate::widgets::Widget::node_id(self) && !self.disabled {
                // Inform the parent ListView so it can highlight + select this
                // item (Python: `ListItem._on_click` posts `_ChildClicked`).
                ctx.post_message(ListItemChildClicked {
                    ordinal: self.ordinal,
                    item: self.text.clone(),
                });
                ctx.set_handled();
            }
        }
    }
}

impl Layout for ListItem {
    fn layout_height(&self) -> Option<usize> {
        // height: auto — sum the children's heights. After extraction the arena
        // owns the children and computes layout, so return None.
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
}

impl std::fmt::Debug for ListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListItem")
            .field("text", &self.text)
            .field("ordinal", &self.ordinal)
            .field("disabled", &self.disabled)
            .field("children", &self.children.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::widgets::Label;

    #[test]
    fn new_recovers_label_text() {
        let item = ListItem::new(Label::new("Hello"));
        assert_eq!(item.text(), "Hello");
    }

    #[test]
    fn from_text_wraps_in_label() {
        let item = ListItem::from_text("World");
        assert_eq!(item.text(), "World");
        assert_eq!(item.children().len(), 1);
    }

    #[test]
    fn style_type_is_list_item() {
        let item = ListItem::from_text("x");
        assert_eq!(item.style_type(), "ListItem");
    }

    #[test]
    fn list_item_is_not_focusable() {
        let item = ListItem::from_text("x");
        assert!(!item.focusable());
    }

    #[test]
    fn compose_drains_children() {
        let mut item = ListItem::new(Label::new("a")).with_child(Label::new("b"));
        let kids = item.compose();
        assert_eq!(kids.len(), 2);
        assert!(item.compose().is_empty());
    }

    #[test]
    fn ordinal_round_trips() {
        let mut item = ListItem::from_text("x");
        item.set_ordinal(3);
        assert_eq!(item.ordinal(), 3);
    }

    #[test]
    fn click_posts_child_clicked_with_ordinal() {
        use crate::node_id::NodeId;
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use crate::widgets::NodeState;
        use slotmap::SlotMap;

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let id = sm.insert(());
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut item = ListItem::from_text("two");
        item.set_ordinal(1);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            item.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        let clicked = messages[0].downcast_ref::<ListItemChildClicked>().unwrap();
        assert_eq!(clicked.ordinal, 1);
        assert_eq!(clicked.item, "two");
    }

    #[test]
    fn disabled_click_is_ignored() {
        use crate::node_id::NodeId;
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use crate::widgets::NodeState;
        use slotmap::SlotMap;

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let id = sm.insert(());
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut item = ListItem::from_text("x").disabled(true);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            item.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(ctx.take_messages().is_empty());
    }
}
