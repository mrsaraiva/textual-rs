//! A list item widget for use with [`ListView`](crate::widgets::ListView).
//!
//! Mirrors Python Textual's `ListItem` (`textual/widgets/_list_item.py`): an
//! arena container that wraps arbitrary child widget(s) — typically a `Label` —
//! with its own CSS. It is **not** focusable; the parent `ListView`
//! (`can_focus_children=False`) drives the highlight by setting the `-highlight`
//! class on the item's node, and the highlight is conveyed **by background only**
//! (no text marker). Hover is tracked via the `-hovered` class.

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{NodeSeed, Widget};

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
pub struct ListItem {
    children: Vec<Box<dyn Widget>>,
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
    pub fn new(child: impl Widget + 'static) -> Self {
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
    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        if self.text.is_empty() {
            self.text = widget_text(&child);
        }
        self.children.push(Box::new(child));
        self
    }

    /// Append another child widget to this item.
    pub fn push(&mut self, child: impl Widget + 'static) {
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
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

/// Best-effort recovery of a child's text for the headless `ListView` API.
fn widget_text(child: &(impl Widget + 'static)) -> String {
    let any = child as &dyn std::any::Any;
    if let Some(label) = any.downcast_ref::<crate::widgets::Label>() {
        return label.text().to_string();
    }
    String::new()
}

impl Widget for ListItem {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn focusable(&self) -> bool {
        // Python: `ListItem(Widget, can_focus=False)`.
        false
    }

    fn style_type(&self) -> &'static str {
        "ListItem"
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() && !self.disabled {
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

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for ListItem {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
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
    fn take_composed_children_drains_children() {
        let mut item = ListItem::new(Label::new("a")).with_child(Label::new("b"));
        let kids = item.take_composed_children();
        assert_eq!(kids.len(), 2);
        assert!(item.take_composed_children().is_empty());
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
        item.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
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
        item.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(ctx.take_messages().is_empty());
    }
}
