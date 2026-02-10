use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::render::FrameBuffer;

use super::{
    Overlay, Widget, WidgetId, WidgetRenderable, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// Tooltip overlay wrapper for a child widget.
///
/// This baseline implementation keeps tooltip composition fully inside the widget render path,
/// using the shared overlay framebuffer compositor introduced in PR4.
pub struct Tooltip {
    id: WidgetId,
    child: Box<dyn Widget>,
    text: String,
    visible: bool,
    max_width: usize,
    y_offset: usize,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Tooltip {
    pub fn new(child: impl Widget + 'static, text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            text: text.into(),
            visible: false,
            max_width: 40,
            y_offset: 1,
            classes: vec!["tooltip".to_string(), "-textual-system".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn with_max_width(mut self, width: usize) -> Self {
        self.max_width = width.max(1);
        self
    }

    pub fn with_y_offset(mut self, y_offset: usize) -> Self {
        self.y_offset = y_offset;
        self
    }

    pub fn is_visible(&self) -> bool {
        self.visible
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

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        let width = width.max(1);
        let mut out = Vec::new();

        for source_line in text.lines() {
            let mut current = String::new();
            for word in source_line.split_whitespace() {
                let word_width = rich_rs::cell_len(word);
                if current.is_empty() {
                    if word_width <= width {
                        current.push_str(word);
                    } else {
                        let mut chunk = String::new();
                        for ch in word.chars() {
                            chunk.push(ch);
                            if rich_rs::cell_len(&chunk) >= width {
                                out.push(chunk.clone());
                                chunk.clear();
                            }
                        }
                        if !chunk.is_empty() {
                            current.push_str(&chunk);
                        }
                    }
                    continue;
                }

                let with_space = format!("{current} {word}");
                if rich_rs::cell_len(&with_space) <= width {
                    current = with_space;
                } else {
                    out.push(current);
                    current = String::new();
                    if word_width <= width {
                        current.push_str(word);
                    } else {
                        let mut chunk = String::new();
                        for ch in word.chars() {
                            chunk.push(ch);
                            if rich_rs::cell_len(&chunk) >= width {
                                out.push(chunk.clone());
                                chunk.clear();
                            }
                        }
                        if !chunk.is_empty() {
                            current.push_str(&chunk);
                        }
                    }
                }
            }

            if current.is_empty() {
                out.push(String::new());
            } else {
                out.push(current);
            }
        }

        if out.is_empty() {
            out.push(String::new());
        }

        out
    }

    fn tooltip_frame(&self, width_limit: usize, height_limit: usize) -> Option<FrameBuffer> {
        if self.text.trim().is_empty() || width_limit == 0 || height_limit == 0 {
            return None;
        }

        let inner_limit = self.max_width.min(width_limit.saturating_sub(2)).max(1);
        let wrapped = Self::wrap_text(&self.text, inner_limit);
        let inner_width = wrapped
            .iter()
            .map(|line| rich_rs::cell_len(line))
            .max()
            .unwrap_or(1)
            .max(1)
            .min(inner_limit);
        let frame_width = inner_width.saturating_add(2).min(width_limit).max(1);

        let mut body_lines = wrapped;
        let max_body_lines = height_limit.saturating_sub(2).max(1);
        if body_lines.len() > max_body_lines {
            body_lines.truncate(max_body_lines);
        }
        let frame_height = body_lines.len().saturating_add(2).min(height_limit).max(1);

        let style = rich_rs::Style::new();
        let box_chars = rich_rs::r#box::SQUARE;
        let mut lines: Vec<Vec<Segment>> = Vec::with_capacity(frame_height);

        if frame_height == 1 {
            lines.push(vec![Segment::styled(" ".repeat(frame_width), style)]);
            return Some(FrameBuffer::from_lines(
                &lines,
                frame_width,
                frame_height,
                Some(style),
            ));
        }

        let mut top = String::new();
        top.push(box_chars.top_left);
        top.push_str(
            &box_chars
                .top
                .to_string()
                .repeat(frame_width.saturating_sub(2)),
        );
        top.push(box_chars.top_right);
        lines.push(vec![Segment::styled(top, style)]);

        let middle_count = frame_height.saturating_sub(2);
        for index in 0..middle_count {
            let mut row = String::new();
            row.push(box_chars.mid_left);
            let content = body_lines.get(index).cloned().unwrap_or_default();
            row.push_str(&rich_rs::set_cell_size(
                &content,
                frame_width.saturating_sub(2),
            ));
            row.push(box_chars.mid_right);
            lines.push(vec![Segment::styled(row, style)]);
        }

        let mut bottom = String::new();
        bottom.push(box_chars.bottom_left);
        bottom.push_str(
            &box_chars
                .bottom
                .to_string()
                .repeat(frame_width.saturating_sub(2)),
        );
        bottom.push(box_chars.bottom_right);
        lines.push(vec![Segment::styled(bottom, style)]);

        Some(FrameBuffer::from_lines(
            &lines,
            frame_width,
            frame_height,
            Some(style),
        ))
    }
}

impl Widget for Tooltip {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let base_renderable = WidgetRenderable::new(self.child.as_ref());
        let mut merged = FrameBuffer::from_renderable(console, options, &base_renderable, None);

        if self.visible {
            if let Some(tooltip) = self.tooltip_frame(options.size.0.max(1), options.size.1.max(1))
            {
                let x0 = merged.width.saturating_sub(tooltip.width) / 2;
                let y0 = self
                    .y_offset
                    .min(merged.height.saturating_sub(tooltip.height.max(1)));
                Overlay::compose_overlay_at(&mut merged, &tooltip, x0, y0);
            }
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(self.child.layout_height())
    }

    fn content_width(&self) -> Option<usize> {
        self.child.content_width()
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
        if ctx.handled() {
            return;
        }

        if self.visible
            && matches!(
                event,
                Event::Key(key) if key.code == KeyCode::Esc
            )
        {
            self.set_visible(false, ctx);
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
                if self.visible && target_matches {
                    self.set_visible(false, ctx);
                    ctx.set_handled();
                }
            }
            _ => {
                self.child.on_message(message, ctx);
            }
        }
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.child.has_focus()
    }

    fn is_disabled(&self) -> bool {
        self.child.is_disabled()
    }

    fn is_hovered(&self) -> bool {
        self.child.is_hovered()
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.child.set_hovered(hovered);
    }

    fn mouse_interactive(&self) -> bool {
        self.child.mouse_interactive()
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }

    fn style_type(&self) -> &'static str {
        "Tooltip"
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Tooltip {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_overlay_messages_toggle_visibility() {
        let mut tooltip = Tooltip::new(super::super::Label::new("base"), "tip");
        let mut ctx = EventCtx::default();
        tooltip.on_message(
            &MessageEvent {
                sender: WidgetId::new(),
                message: Message::OverlaySetVisible {
                    overlay: tooltip.id(),
                    visible: true,
                },
            },
            &mut ctx,
        );
        assert!(tooltip.is_visible());

        let messages = ctx.take_messages();
        assert!(messages.iter().any(|event| {
            matches!(
                event.message,
                Message::OverlayVisibilityChanged {
                    overlay,
                    visible: true
                } if overlay == tooltip.id()
            )
        }));
    }
}
