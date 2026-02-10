use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::render::{Cell, FrameBuffer};

use crate::widgets::{
    Widget, WidgetId, WidgetRenderable, WidgetStyles, helpers::fixed_height_from_constraints,
};

pub struct Overlay {
    id: WidgetId,
    base: Box<dyn Widget>,
    modal: Box<dyn Widget>,
    visible: bool,
    trap_base_events: bool,
    dismiss_on_escape: bool,
    styles: WidgetStyles,
}

impl Overlay {
    pub fn new(base: impl Widget + 'static, modal: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            base: Box::new(base),
            modal: Box::new(modal),
            visible: true,
            trap_base_events: true,
            dismiss_on_escape: true,
            styles: WidgetStyles::default(),
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
                        *base.get_mut(tx, ty) = cell.clone();
                    }
                    continue;
                }

                copied_lead = false;
                if !Self::cell_overwrites_base(cell) {
                    continue;
                }

                *base.get_mut(tx, ty) = cell.clone();
                copied_lead = cell.width() > 1;

                if cell.width().max(1) == 1
                    && tx + 1 < base.width
                    && base.get(tx + 1, ty).continuation
                {
                    *base.get_mut(tx + 1, ty) = Cell::blank(base.get(tx + 1, ty).style);
                }
            }
        }
    }

    fn set_visible(&mut self, visible: bool, ctx: &mut EventCtx) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        ctx.post_message(
            self.id,
            Message::OverlayVisibilityChanged {
                overlay: self.id,
                visible,
            },
        );
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

    fn modal_contains(&mut self, target: WidgetId) -> bool {
        fn contains(widget: &mut dyn Widget, target: WidgetId) -> bool {
            if widget.id() == target {
                return true;
            }
            let mut found = false;
            widget.visit_children_mut(&mut |child| {
                if !found && contains(child, target) {
                    found = true;
                }
            });
            found
        }
        contains(self.modal.as_mut(), target)
    }
}

impl Widget for Overlay {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
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
        self.base.on_mount();
        self.modal.on_mount();
    }

    fn on_unmount(&mut self) {
        self.base.on_unmount();
        self.modal.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.base.on_tick(tick);
        self.modal.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.base.on_resize(width, height);
        self.modal.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.base.on_layout(width, height);
        self.modal.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
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
        match &message.message {
            Message::OverlaySetVisible { overlay, visible } if *overlay == self.id => {
                self.set_visible(*visible, ctx);
                ctx.set_handled();
            }
            Message::OverlayToggle { overlay } if *overlay == self.id => {
                self.set_visible(!self.visible, ctx);
                ctx.set_handled();
            }
            Message::OverlayDismissRequested { overlay } => {
                let target_matches = overlay.map(|target| target == self.id).unwrap_or(true);
                let sender_in_modal = self.modal_contains(message.sender);
                if target_matches && (sender_in_modal || overlay.is_some()) {
                    self.set_visible(false, ctx);
                    ctx.set_handled();
                }
            }
            _ => {}
        }

        self.modal.on_message(message, ctx);
        if !ctx.handled() {
            self.base.on_message(message, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.modal.as_mut());
        f(self.base.as_mut());
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
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
    use rich_rs::Segment;

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
}
