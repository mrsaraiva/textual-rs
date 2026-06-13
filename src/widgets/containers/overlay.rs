use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;
use crate::render::{Cell, FrameBuffer};

use crate::node_id::NodeId;
use crate::widgets::{NodeSeed, Spacer, Widget, WidgetRenderable};

pub struct Overlay {
    base: Box<dyn Widget>,
    modal: Box<dyn Widget>,
    visible: bool,
    trap_base_events: bool,
    dismiss_on_escape: bool,
    seed: NodeSeed,
    children_extracted: bool,
}

impl Overlay {
    pub fn new(base: impl Widget + 'static, modal: impl Widget + 'static) -> Self {
        Self {
            base: Box::new(base),
            modal: Box::new(modal),
            visible: true,
            trap_base_events: true,
            dismiss_on_escape: true,
            seed: NodeSeed::default(),
            children_extracted: false,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn trap_base_events(mut self, trap: bool) -> Self {
        self.trap_base_events = trap;
        self
    }

    pub fn dismiss_on_escape(mut self, enabled: bool) -> Self {
        self.dismiss_on_escape = enabled;
        self
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn cell_overwrites_base(cell: &Cell) -> bool {
        if cell.continuation {
            return false;
        }
        let has_text = !cell.text.is_empty() && cell.text != " ";
        has_text || cell.style.is_some() || cell.meta.is_some()
    }

    /// Compose a full-screen top layer over a base frame.
    pub(crate) fn compose_overlay(base: &FrameBuffer, top: &FrameBuffer) -> FrameBuffer {
        let mut merged = base.clone();
        Self::compose_overlay_at(&mut merged, top, 0, 0);
        merged
    }

    /// Compose a top layer at an origin over a base frame.
    pub(crate) fn compose_overlay_at(
        base: &mut FrameBuffer,
        top: &FrameBuffer,
        x0: usize,
        y0: usize,
    ) {
        for y in 0..top.height {
            let ty = y0.saturating_add(y);
            if ty >= base.height {
                break;
            }

            let mut copied_lead = false;
            for x in 0..top.width {
                let tx = x0.saturating_add(x);
                if tx >= base.width {
                    break;
                }

                let cell = top.get(x, y);
                if cell.continuation {
                    if copied_lead {
                        base.set_cell(tx, ty, cell.clone());
                    }
                    continue;
                }

                copied_lead = false;
                if !Self::cell_overwrites_base(cell) {
                    continue;
                }

                base.set_cell(tx, ty, cell.clone());
                copied_lead = cell.width() > 1;

                if cell.width().max(1) == 1
                    && tx + 1 < base.width
                    && base.get(tx + 1, ty).continuation
                {
                    base.set_cell(tx + 1, ty, Cell::blank(base.get(tx + 1, ty).style));
                }
            }
        }
    }

    fn set_visible(&mut self, visible: bool, ctx: &mut EventCtx) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        // Runtime consumes OverlayVisibilityChanged and toggles modal subtree
        // `display` in tree mode.
        ctx.post_message(OverlayVisibilityChanged {
            overlay: self.node_id(),
            visible,
        });
        ctx.request_repaint();
    }

    fn interactive_event(event: &Event) -> bool {
        matches!(
            event,
            Event::Key(..)
                | Event::Action(..)
                | Event::MouseDown(..)
                | Event::MouseUp(..)
                | Event::MouseScroll(..)
        )
    }

    /// Returns whether the given target node is part of the modal subtree.
    ///
    /// Currently returns `true` conservatively because Overlay does not have
    /// access to the widget tree arena during `on_event`/`on_message`.  This
    /// means dismiss requests from *any* sender are accepted when no explicit
    /// overlay target is specified.  A precise check would require arena
    /// traversal (ancestor-of query), which is not available in widget-level
    /// event handlers.
    fn modal_contains(&mut self, _target: NodeId) -> bool {
        true
    }
}

impl Widget for Overlay {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        let base = std::mem::replace(&mut self.base, Box::new(Spacer::new(1)));
        let modal = std::mem::replace(&mut self.modal, Box::new(Spacer::new(1)));
        vec![base, modal]
    }

    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        if !self.children_extracted {
            return None;
        }
        match child_index {
            // Base subtree is always present; modal subtree follows overlay visibility.
            0 => Some(true),
            1 => Some(self.visible),
            _ => None,
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.children_extracted {
            // Tree-mode: Overlay has no chrome of its own; return empty.
            return Segments::new();
        }
        if !self.visible {
            return self.base.render_styled(console, options);
        }
        let base_renderable = WidgetRenderable::new(self.base.as_ref());
        let modal_renderable = WidgetRenderable::new(self.modal.as_ref());
        let base = FrameBuffer::from_renderable(console, options, &base_renderable, None);
        let top = FrameBuffer::from_renderable(console, options, &modal_renderable, None);
        Self::compose_overlay(&base, &top).to_segments()
    }

    fn on_mount(&mut self) {
        if !self.children_extracted {
            self.base.on_mount();
            self.modal.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.children_extracted {
            self.base.on_unmount();
            self.modal.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.children_extracted {
            self.base.on_tick(tick);
            self.modal.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.children_extracted {
            self.base.on_resize(width, height);
            self.modal.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if !self.children_extracted {
            self.base.on_layout(width, height);
            self.modal.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.children_extracted {
            // Tree mode: tree handles capture routing to children.
            return;
        }
        if !self.visible {
            self.base.on_event_capture(event, ctx);
            return;
        }
        self.modal.on_event_capture(event, ctx);
        if !ctx.handled() && self.trap_base_events && Self::interactive_event(event) {
            ctx.set_handled();
        } else if !ctx.handled() {
            self.base.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.children_extracted {
            // Tree mode: handle overlay-specific behavior only.
            if self.dismiss_on_escape
                && matches!(
                    event,
                    Event::Key(key) if key.code == KeyCode::Esc
                )
            {
                self.set_visible(false, ctx);
                ctx.set_handled();
            }
            return;
        }
        if !self.visible {
            self.base.on_event(event, ctx);
            return;
        }
        self.modal.on_event(event, ctx);
        if ctx.handled() {
            return;
        }
        if self.dismiss_on_escape
            && matches!(
                event,
                Event::Key(key) if key.code == KeyCode::Esc
            )
        {
            self.set_visible(false, ctx);
            ctx.set_handled();
            return;
        }
        if !self.trap_base_events {
            self.base.on_event(event, ctx);
            return;
        }
        if Self::interactive_event(event) {
            ctx.set_handled();
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Overlay-specific message handling (works in both modes).
        if let Some(m) = message.downcast_ref::<OverlaySetVisible>() {
            if m.overlay == self.node_id() {
                self.set_visible(m.visible, ctx);
                ctx.set_handled();
                return;
            }
        }
        if let Some(m) = message.downcast_ref::<OverlayToggle>() {
            if m.overlay == self.node_id() {
                self.set_visible(!self.visible, ctx);
                ctx.set_handled();
                return;
            }
        }
        if let Some(m) = message.downcast_ref::<OverlayDismissRequested>() {
            let target_matches = m.overlay.map(|id| id == self.node_id()).unwrap_or(true);
            let sender_in_modal = self.modal_contains(message.sender);
            if target_matches && (sender_in_modal || m.overlay.is_some()) {
                self.set_visible(false, ctx);
                ctx.set_handled();
            }
            return;
        }

        // In tree mode, skip child forwarding — the tree handles message routing.
        if self.children_extracted {
            return;
        }

        self.modal.on_message(message, ctx);
        if !ctx.handled() {
            self.base.on_message(message, ctx);
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn layout_height(&self) -> Option<usize> {
        if self.visible {
            match (self.base.layout_height(), self.modal.layout_height()) {
                (Some(base), Some(modal)) => Some(base.max(modal)),
                (Some(base), None) => Some(base),
                (None, Some(modal)) => Some(modal),
                (None, None) => None,
            }
        } else {
            self.base.layout_height()
        }
    }
}

impl Renderable for Overlay {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use rich_rs::Segment;

    fn make_node_id() -> NodeId {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn compose_overlay_keeps_base_for_blank_unstyled_top_cells() {
        let base = FrameBuffer::from_lines(&[vec![Segment::new("abc")]], 3, 1, None);
        let top = FrameBuffer::from_lines(&[vec![Segment::new("   ")]], 3, 1, None);

        let merged = Overlay::compose_overlay(&base, &top);
        assert_eq!(merged.as_plain_lines()[0], "abc");
    }

    #[test]
    fn compose_overlay_applies_styled_space_cells() {
        let base = FrameBuffer::from_lines(&[vec![Segment::new("abc")]], 3, 1, None);
        let mut styled_space = Segment::new(" ");
        styled_space.style = Some(
            rich_rs::Style::new().with_bgcolor(rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 255 }),
        );
        let top = FrameBuffer::from_lines(&[vec![styled_space]], 3, 1, None);

        let merged = Overlay::compose_overlay(&base, &top);
        assert_eq!(merged.get(0, 0).text, " ");
        assert!(merged.get(0, 0).style.is_some());
        assert_eq!(merged.get(1, 0).text, "b");
    }

    #[test]
    fn overlay_set_visible_matches_real_node_id() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let mut ctx = EventCtx::default();

        // OverlaySetVisible targeting our real NodeId should hide the overlay.
        let msg_event = MessageEvent::new(
            NodeId::default(),
            OverlaySetVisible {
                overlay: id,
                visible: false,
            },
        );
        overlay.on_message(&msg_event, &mut ctx);
        assert!(
            ctx.handled(),
            "OverlaySetVisible with matching NodeId must be handled"
        );
        assert!(!overlay.is_visible());
    }

    #[test]
    fn overlay_set_visible_ignores_foreign_node_id() {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        let id = sm.insert(());
        let other = sm.insert(());
        assert_ne!(id, other);
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let mut ctx = EventCtx::default();

        // OverlaySetVisible targeting a different NodeId should be ignored.
        let msg_event = MessageEvent::new(
            NodeId::default(),
            OverlaySetVisible {
                overlay: other,
                visible: false,
            },
        );
        overlay.on_message(&msg_event, &mut ctx);
        assert!(
            !ctx.handled(),
            "OverlaySetVisible with foreign NodeId must not be handled"
        );
        assert!(overlay.is_visible());
    }

    #[test]
    fn overlay_toggle_matches_real_node_id() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let mut ctx = EventCtx::default();

        let msg_event = MessageEvent::new(NodeId::default(), OverlayToggle { overlay: id });
        overlay.on_message(&msg_event, &mut ctx);
        assert!(ctx.handled());
        assert!(
            !overlay.is_visible(),
            "Toggle should have hidden the overlay"
        );
    }

    #[test]
    fn overlay_dismiss_with_explicit_target_matches_self() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let mut ctx = EventCtx::default();

        let msg_event = MessageEvent::new(
            NodeId::default(),
            OverlayDismissRequested { overlay: Some(id) },
        );
        overlay.on_message(&msg_event, &mut ctx);
        assert!(ctx.handled());
        assert!(!overlay.is_visible());
    }

    #[test]
    fn overlay_dismiss_with_foreign_target_ignored() {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        let id = sm.insert(());
        let other = sm.insert(());
        assert_ne!(id, other);
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let mut ctx = EventCtx::default();

        let msg_event = MessageEvent::new(
            NodeId::default(),
            OverlayDismissRequested {
                overlay: Some(other),
            },
        );
        overlay.on_message(&msg_event, &mut ctx);
        assert!(!ctx.handled());
        assert!(overlay.is_visible());
    }

    // --- Tree extraction regression tests ---

    #[test]
    fn overlay_extraction_returns_both_children() {
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal);
        let children = overlay.take_composed_children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn overlay_extraction_idempotent() {
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal);
        let _ = overlay.take_composed_children();
        assert!(overlay.take_composed_children().is_empty());
    }

    #[test]
    fn overlay_render_after_extraction() {
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal);
        let _ = overlay.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&overlay, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn overlay_visibility_after_extraction() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let _ = overlay.take_composed_children();
        let mut ctx = EventCtx::default();

        // set_visible via message should not panic after extraction
        let msg_event = MessageEvent::new(
            NodeId::default(),
            OverlaySetVisible {
                overlay: id,
                visible: false,
            },
        );
        overlay.on_message(&msg_event, &mut ctx);
        assert!(ctx.handled());
        assert!(!overlay.is_visible());
    }

    #[test]
    fn overlay_toggle_after_extraction() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal).visible(true);
        let _ = overlay.take_composed_children();
        let mut ctx = EventCtx::default();

        let msg_event = MessageEvent::new(NodeId::default(), OverlayToggle { overlay: id });
        overlay.on_message(&msg_event, &mut ctx);
        assert!(ctx.handled());
        assert!(!overlay.is_visible());
    }

    #[test]
    fn overlay_uses_tree_path_after_extraction() {
        let base = crate::prelude::Label::new("base");
        let modal = crate::prelude::Label::new("modal");
        let mut overlay = Overlay::new(base, modal);
        assert!(!overlay.children_extracted);
        let _ = overlay.take_composed_children();
        assert!(overlay.children_extracted);
    }
}
