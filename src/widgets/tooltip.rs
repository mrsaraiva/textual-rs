use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;
use crate::render::FrameBuffer;
use crate::style::{Constrain, parse_color_like};

use crate::node_id::NodeId;

use super::{
    Overlay, Widget, WidgetRenderable, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// Tooltip overlay wrapper for a child widget.
///
/// This baseline implementation keeps tooltip composition fully inside the widget render path,
/// using the shared overlay framebuffer compositor introduced in PR4.
pub struct Tooltip {
    child: Box<dyn Widget>,
    text: String,
    visible: bool,
    max_width: usize,
    y_offset: usize,
    anchor: Option<(usize, usize)>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Tooltip {
    pub fn new(child: impl Widget + 'static, text: impl Into<String>) -> Self {
        Self {
            child: Box::new(child),
            text: text.into(),
            visible: false,
            max_width: 40,
            y_offset: 1,
            anchor: None,
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

    pub fn with_anchor(mut self, x: usize, y: usize) -> Self {
        self.anchor = Some((x, y));
        self
    }

    pub fn set_anchor(&mut self, x: usize, y: usize) {
        self.anchor = Some((x, y));
    }

    pub fn clear_anchor(&mut self) {
        self.anchor = None;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn anchor_target_id(&self) -> NodeId {
        self.node_id()
    }

    fn set_visible(&mut self, visible: bool, ctx: &mut EventCtx) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        ctx.post_message(Message::OverlayVisibilityChanged(
            OverlayVisibilityChanged {
                overlay: self.node_id(),
                visible,
            },
        ));
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

    fn tooltip_styles(&self) -> (rich_rs::Style, rich_rs::Style) {
        let bubble = crate::css::resolve_component_style(self, &["tooltip--bubble"])
            .to_rich()
            .unwrap_or_else(|| {
                let mut fallback = rich_rs::Style::new();
                if let Some(bg) = parse_color_like("$panel") {
                    fallback = fallback.with_bgcolor(bg.to_simple_opaque());
                }
                if let Some(fg) = parse_color_like("$foreground") {
                    fallback = fallback.with_color(fg.to_simple_opaque());
                }
                fallback
            });
        let text = crate::css::resolve_component_style(self, &["tooltip--text"])
            .to_rich()
            .unwrap_or_else(|| {
                if let Some(fg) = parse_color_like("$foreground") {
                    rich_rs::Style::new().with_color(fg.to_simple_opaque())
                } else {
                    rich_rs::Style::new()
                }
            });
        (bubble, text)
    }

    fn tooltip_frame(&self, width_limit: usize, height_limit: usize) -> Option<FrameBuffer> {
        if self.text.trim().is_empty() || width_limit == 0 || height_limit == 0 {
            return None;
        }

        let pad_x = 2usize;
        let pad_y = 1usize;
        let max_frame_width = self.max_width.min(width_limit).max(1);
        let inner_limit = max_frame_width
            .saturating_sub(pad_x.saturating_mul(2))
            .max(1);
        let wrapped = Self::wrap_text(&self.text, inner_limit);
        let inner_width = wrapped
            .iter()
            .map(|line| rich_rs::cell_len(line))
            .max()
            .unwrap_or(1)
            .max(1)
            .min(inner_limit);
        let frame_width = inner_width
            .saturating_add(pad_x.saturating_mul(2))
            .min(width_limit)
            .max(1);

        let mut body_lines = wrapped;
        let max_body_lines = height_limit.saturating_sub(pad_y.saturating_mul(2)).max(1);
        if body_lines.len() > max_body_lines {
            body_lines.truncate(max_body_lines);
        }
        let frame_height = body_lines
            .len()
            .saturating_add(pad_y.saturating_mul(2))
            .min(height_limit)
            .max(1);

        let (bubble_style, text_style) = self.tooltip_styles();
        let text_on_bubble = bubble_style.combine(&text_style);
        let mut lines: Vec<Vec<Segment>> = Vec::with_capacity(frame_height);

        let top_rows = pad_y.min(frame_height);
        for _ in 0..top_rows {
            lines.push(vec![Segment::styled(" ".repeat(frame_width), bubble_style)]);
        }

        let content_rows = frame_height.saturating_sub(top_rows + pad_y);
        for index in 0..content_rows {
            let content = body_lines.get(index).cloned().unwrap_or_default();
            let inner_available = frame_width.saturating_sub(pad_x.saturating_mul(2));
            if inner_available == 0 {
                lines.push(vec![Segment::styled(" ".repeat(frame_width), bubble_style)]);
                continue;
            }
            let left = " ".repeat(pad_x.min(frame_width));
            let right_width = frame_width.saturating_sub(left.len() + inner_available);
            let right = " ".repeat(right_width);
            lines.push(vec![
                Segment::styled(left, bubble_style),
                Segment::styled(
                    rich_rs::set_cell_size(&content, inner_available),
                    text_on_bubble,
                ),
                Segment::styled(right, bubble_style),
            ]);
        }

        while lines.len() < frame_height {
            lines.push(vec![Segment::styled(" ".repeat(frame_width), bubble_style)]);
        }

        Some(FrameBuffer::from_lines(
            &lines,
            frame_width,
            frame_height,
            Some(bubble_style),
        ))
    }

    fn overlay_origin(
        &self,
        base_width: usize,
        base_height: usize,
        overlay_width: usize,
        overlay_height: usize,
        constrain_x: Constrain,
        constrain_y: Constrain,
    ) -> (usize, usize) {
        let (anchor_x, anchor_y) = self.anchor.unwrap_or((base_width.saturating_sub(1) / 2, 0));
        let anchor_x = anchor_x.min(base_width.saturating_sub(1));
        let anchor_y = anchor_y.min(base_height.saturating_sub(1));

        // X-axis: center on anchor, then apply constrain mode.
        let x0 = match constrain_x {
            Constrain::Inside | Constrain::Inflect => {
                let max_x = base_width.saturating_sub(overlay_width);
                anchor_x.saturating_sub(overlay_width / 2).min(max_x)
            }
            Constrain::None => anchor_x.saturating_sub(overlay_width / 2),
        };

        // Y-axis: prefer below anchor, flip/clamp per constrain mode.
        let y0 = match constrain_y {
            Constrain::Inside => {
                let max_y = base_height.saturating_sub(overlay_height);
                let preferred_below = anchor_y.saturating_add(self.y_offset);
                if preferred_below.saturating_add(overlay_height) <= base_height {
                    preferred_below.min(max_y)
                } else {
                    let needed_above = overlay_height.saturating_add(self.y_offset);
                    if anchor_y >= needed_above {
                        anchor_y.saturating_sub(needed_above).min(max_y)
                    } else {
                        max_y
                    }
                }
            }
            Constrain::Inflect => {
                let preferred_below = anchor_y.saturating_add(self.y_offset);
                if preferred_below.saturating_add(overlay_height) <= base_height {
                    preferred_below
                } else {
                    let needed_above = overlay_height.saturating_add(self.y_offset);
                    if anchor_y >= needed_above {
                        anchor_y.saturating_sub(needed_above)
                    } else {
                        0
                    }
                }
            }
            Constrain::None => anchor_y.saturating_add(self.y_offset),
        };

        (x0, y0)
    }
}

impl Widget for Tooltip {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let base_renderable = WidgetRenderable::new(self.child.as_ref());
        let mut merged = FrameBuffer::from_renderable(console, options, &base_renderable, None);

        if self.visible {
            if let Some(tooltip) = self.tooltip_frame(options.size.0.max(1), options.size.1.max(1))
            {
                let component_style = crate::css::resolve_component_style(self, &[]);
                // Tooltip defaults to Inside when no constrain CSS is set.
                let (cx, cy) = {
                    let base = component_style.constrain.unwrap_or(Constrain::Inside);
                    let cx = component_style.constrain_x.unwrap_or(base);
                    let cy = component_style.constrain_y.unwrap_or(base);
                    (cx, cy)
                };
                let (x0, y0) = self.overlay_origin(
                    merged.width,
                    merged.height,
                    tooltip.width,
                    tooltip.height,
                    cx,
                    cy,
                );
                Overlay::compose_overlay_at(&mut merged, &tooltip, x0, y0);
            }
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(self.child.layout_height())
    }

    fn content_width(&self) -> Option<usize> {
        let child_width = self.child.content_width().unwrap_or(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(child_width.saturating_add(chrome_lr).max(1))
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.visible = false;
        self.clear_anchor();
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
        match event {
            Event::AppFocus(active) => {
                if !*active && self.visible {
                    self.set_visible(false, ctx);
                    ctx.set_handled();
                }
            }
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.set_anchor(mouse.x as usize, mouse.y as usize);
            }
            Event::MouseUp(mouse) if mouse.target.is_some_and(|t| t == self.node_id()) => {
                self.set_anchor(mouse.x as usize, mouse.y as usize);
            }
            Event::MouseScroll(mouse) if mouse.target.is_some_and(|t| t == self.node_id()) => {
                self.set_anchor(mouse.x as usize, mouse.y as usize);
            }
            _ => {}
        }

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
            Message::OverlaySetVisible(OverlaySetVisible { overlay, visible })
                if *overlay == self.node_id() =>
            {
                self.set_visible(*visible, ctx);
                ctx.set_handled();
            }
            Message::OverlaySetAnchor(OverlaySetAnchor { overlay, x, y })
                if *overlay == self.node_id() =>
            {
                self.set_anchor(*x, *y);
                ctx.request_repaint();
                ctx.set_handled();
            }
            Message::OverlayClearAnchor(OverlayClearAnchor { overlay })
                if *overlay == self.node_id() =>
            {
                self.clear_anchor();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Message::OverlayToggle(OverlayToggle { overlay }) if *overlay == self.node_id() => {
                self.set_visible(!self.visible, ctx);
                ctx.set_handled();
            }
            Message::OverlayDismissRequested(OverlayDismissRequested { overlay }) => {
                let target_matches = overlay.map(|t| t == self.node_id()).unwrap_or(true);
                if self.visible && target_matches {
                    self.set_visible(false, ctx);
                    ctx.set_handled();
                } else {
                    self.child.on_message(message, ctx);
                }
            }
            _ => {
                self.child.on_message(message, ctx);
            }
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.set_anchor(x as usize, y as usize);
        self.child.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.child.on_mouse_scroll(delta_x, delta_y, ctx);
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
    use crate::node_id::NodeId;

    #[test]
    fn tooltip_overlay_messages_toggle_visibility() {
        let mut tooltip = Tooltip::new(super::super::Label::new("base"), "tip");
        let mut ctx = EventCtx::default();
        tooltip.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::OverlaySetVisible(OverlaySetVisible {
                    overlay: NodeId::default(),
                    visible: true,
                }),
                control: None,
            },
            &mut ctx,
        );
        assert!(tooltip.is_visible());

        let messages = ctx.take_messages();
        assert!(messages.iter().any(|event| {
            matches!(
                event.message,
                Message::OverlayVisibilityChanged(OverlayVisibilityChanged {
                    overlay,
                    visible: true
                }) if overlay == NodeId::default()
            )
        }));
    }

    #[test]
    fn tooltip_inflects_above_anchor_when_no_room_below() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (28, 6);
        options.max_width = 28;
        options.max_height = 6;

        let tooltip = Tooltip::new(super::super::Label::new("base"), "anchor-tip")
            .visible(true)
            .with_anchor(14, 5);
        let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
        let lines = buf.as_plain_lines();
        let line_idx = lines
            .iter()
            .position(|line| line.contains("anchor-tip"))
            .expect("tooltip text line");
        assert_eq!(line_idx, 2);
    }

    #[test]
    fn tooltip_clamps_horizontally_inside_viewport() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (20, 6);
        options.max_width = 20;
        options.max_height = 6;

        let tooltip = Tooltip::new(super::super::Label::new("base"), "left-edge")
            .visible(true)
            .with_anchor(0, 0);
        let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
        let lines = buf.as_plain_lines();
        let line = lines
            .iter()
            .find(|line| line.contains("left-edge"))
            .expect("tooltip text line");
        let x = line.find("left-edge").expect("tooltip x position");
        assert_eq!(x, 2);
    }
}
