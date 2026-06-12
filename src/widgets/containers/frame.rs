use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use crate::widgets::{
    NodeSeed, Spacer, Widget, WidgetStyles,
    helpers::{apply_debug_box, fixed_height_from_constraints},
};

pub struct Frame {
    child: Box<dyn Widget>,
    padding: usize,
    border: bool,
    seed: NodeSeed,
    child_extracted: bool,
}

impl Frame {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            padding: 1,
            border: true,
            seed: NodeSeed::default(),
            child_extracted: false,
        }
    }

    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = padding;
        self
    }

    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }
}

impl Widget for Frame {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.child_extracted {
            // Tree-mode: render border chrome only, with blank content inside.
            // The tree pipeline renders children separately.
            let border_width: usize = if self.border { 1 } else { 0 };
            let total_padding = self.padding * 2;
            let width = options.size.0.max(1);
            let height = options.size.1.max(1);
            let inner_width = width
                .saturating_sub(border_width * 2 + total_padding)
                .max(1);
            let inner_total = inner_width + total_padding;
            let content_height = height.saturating_sub(border_width * 2).max(1);

            let mut out = Segments::new();
            if self.border {
                let b = rich_rs::r#box::SQUARE;
                let top = format!(
                    "{}{}{}",
                    b.top_left,
                    std::iter::repeat(b.top)
                        .take(inner_total)
                        .collect::<String>(),
                    b.top_right
                );
                out.push(Segment::new(top));
                out.push(Segment::line());
                for idx in 0..content_height {
                    out.push(Segment::new(b.mid_left.to_string()));
                    out.push(Segment::new(" ".repeat(inner_total)));
                    out.push(Segment::new(b.mid_right.to_string()));
                    if idx + 1 < content_height {
                        out.push(Segment::line());
                    }
                }
                out.push(Segment::line());
                let bottom = format!(
                    "{}{}{}",
                    b.bottom_left,
                    std::iter::repeat(b.bottom)
                        .take(inner_total)
                        .collect::<String>(),
                    b.bottom_right
                );
                out.push(Segment::new(bottom));
            } else {
                for idx in 0..height {
                    out.push(Segment::new(" ".repeat(width)));
                    if idx + 1 < height {
                        out.push(Segment::line());
                    }
                }
            }
            return out;
        }

        let border_width: usize = if self.border { 1 } else { 0 };
        let total_padding = self.padding * 2;

        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let inner_width = width
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);
        let mut inner_height = height
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);
        if let Some(child_height) = self.child.layout_height() {
            inner_height = inner_height.min(child_height.max(1));
        }

        let mut child_options = options.clone();
        child_options.size = (inner_width, inner_height);
        child_options.max_width = inner_width;
        child_options.max_height = inner_height;

        let child_segments = self.child.render_styled(console, &child_options);
        let mut child_lines =
            Segment::split_and_crop_lines(child_segments, inner_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            let capped = height.min(inner_height);
            child_lines = Segment::set_shape(&child_lines, inner_width, Some(capped), None, false);
        }

        let padding_line = vec![Segment::new(" ".repeat(inner_width))];
        let mut content_lines: Vec<Vec<Segment>> = Vec::new();
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines.extend(child_lines.into_iter());
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines = Segment::set_shape(
            &content_lines,
            inner_width,
            Some(inner_height + total_padding),
            None,
            false,
        );

        let inner_total = inner_width + total_padding;
        let mut out = Segments::new();
        let line_count = content_lines.len();

        if self.border {
            let b = rich_rs::r#box::SQUARE;
            let top = format!(
                "{}{}{}",
                b.top_left,
                std::iter::repeat(b.top)
                    .take(inner_total)
                    .collect::<String>(),
                b.top_right
            );
            out.push(Segment::new(top));
            out.push(Segment::line());

            for (idx, line) in content_lines.into_iter().enumerate() {
                out.push(Segment::new(b.mid_left.to_string()));
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                let adjusted = Segment::adjust_line_length(&line, inner_width, None, true);
                out.extend(adjusted);
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                out.push(Segment::new(b.mid_right.to_string()));
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }

            let bottom = format!(
                "{}{}{}",
                b.bottom_left,
                std::iter::repeat(b.bottom)
                    .take(inner_total)
                    .collect::<String>(),
                b.bottom_right
            );
            out.push(Segment::line());
            out.push(Segment::new(bottom));
        } else {
            for (idx, line) in content_lines.into_iter().enumerate() {
                let adjusted = Segment::adjust_line_length(&line, inner_total, None, true);
                out.extend(adjusted);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
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
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(lines, width, height, label.as_deref(), debug.style_for(0));
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

    fn on_mount(&mut self) {
        if !self.child_extracted {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.child_extracted {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.child_extracted {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.child_extracted {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if self.child_extracted {
            return;
        }
        let border_width: usize = if self.border { 1 } else { 0 };
        let total_padding = self.padding.saturating_mul(2);
        let inner_width = usize::from(width)
            .saturating_sub(border_width.saturating_mul(2) + total_padding)
            .max(1);
        let inner_height = usize::from(height)
            .saturating_sub(border_width.saturating_mul(2) + total_padding)
            .max(1);
        self.child
            .on_layout(inner_width as u16, inner_height as u16);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.child_extracted {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.child_extracted {
            self.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if !self.child_extracted {
            self.child.on_message(message, ctx);
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if !self.child_extracted {
            self.child.on_mouse_scroll(delta_x, delta_y, ctx);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child
            .layout_height()
            .map(|h| h + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        self.seed.styles = seed.styles.clone();
        seed
    }

    fn focusable(&self) -> bool {
        if self.child_extracted {
            return false;
        }
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        if !self.child_extracted {
            self.child.set_focus(focused);
        }
    }
}

impl Renderable for Frame {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_extraction_returns_child() {
        let mut frame = Frame::new(Spacer::new(1));
        let children = frame.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn frame_extraction_idempotent() {
        let mut frame = Frame::new(Spacer::new(1));
        let _ = frame.take_composed_children();
        assert!(frame.take_composed_children().is_empty());
    }

    #[test]
    fn frame_render_after_extraction_with_border() {
        let mut frame = Frame::new(Spacer::new(1));
        let _ = frame.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&frame, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn frame_render_after_extraction_no_border() {
        let mut frame = Frame::new(Spacer::new(1)).border(false);
        let _ = frame.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&frame, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn frame_uses_tree_path_after_extraction() {
        let mut frame = Frame::new(Spacer::new(1));
        assert!(!frame.child_extracted);
        let _ = frame.take_composed_children();
        assert!(frame.child_extracted);
    }
}
