use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use crate::widgets::{Spacer, Widget, WidgetStyles, helpers::fixed_height_from_constraints};

pub struct Panel {
    child: Box<dyn Widget>,
    title: Option<String>,
    padding: usize,
    border: bool,
    styles: WidgetStyles,
    child_extracted: bool,
}

impl Panel {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            title: None,
            padding: 0,
            border: true,
            styles: WidgetStyles::default(),
            child_extracted: false,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
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

impl Widget for Panel {
    fn border_title(&self) -> Option<&str> {
        self.title.as_deref()
    }

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
            // Tree-mode: render border + title chrome only, with blank content.
            let border_width: usize = if self.border { 1 } else { 0 };
            let total_padding = self.padding * 2;
            let width = options.size.0.max(1);
            let height = options.size.1.max(1);
            let inner_width = width
                .saturating_sub(border_width * 2 + total_padding)
                .max(1);
            let content_height = height.saturating_sub(border_width * 2).max(1);

            if !self.border {
                let mut out = Segments::new();
                for idx in 0..height {
                    out.push(Segment::new(" ".repeat(width)));
                    if idx + 1 < height {
                        out.push(Segment::line());
                    }
                }
                return out;
            }

            let box_chars = rich_rs::r#box::SQUARE;
            let mut out_lines: Vec<Vec<Segment>> = Vec::new();

            // Top border with optional title
            let mut top = String::new();
            top.push(box_chars.top_left);
            let mut title = self.title.clone().unwrap_or_default();
            if !title.is_empty() && inner_width >= 2 {
                title = format!(" {title} ");
            }
            let title_width = rich_rs::cell_len(&title);
            if title_width >= inner_width {
                top.push_str(&rich_rs::set_cell_size(&title, inner_width));
            } else {
                let remaining = inner_width.saturating_sub(title_width);
                let left = remaining / 2;
                let right = remaining - left;
                top.push_str(&box_chars.top.to_string().repeat(left));
                top.push_str(&title);
                top.push_str(&box_chars.top.to_string().repeat(right));
            }
            top.push(box_chars.top_right);
            out_lines.push(vec![Segment::new(top)]);

            // Blank content rows
            for _ in 0..content_height {
                let mut middle = Vec::new();
                middle.push(Segment::new(box_chars.mid_left.to_string()));
                middle.push(Segment::new(" ".repeat(inner_width)));
                middle.push(Segment::new(box_chars.mid_right.to_string()));
                out_lines.push(middle);
            }

            // Bottom border
            let mut bottom = String::new();
            bottom.push(box_chars.bottom_left);
            bottom.push_str(&box_chars.bottom.to_string().repeat(inner_width));
            bottom.push(box_chars.bottom_right);
            out_lines.push(vec![Segment::new(bottom)]);

            let out_lines = Segment::set_shape(&out_lines, width, Some(height), None, false);
            let line_count = out_lines.len();
            let mut out = Segments::new();
            for (idx, line) in out_lines.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
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
        let inner_height = height
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);

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

        let content_height = content_lines.len().max(1);
        let content_height = content_height.min(height.saturating_sub(border_width * 2).max(1));
        let mut content_lines = Segment::set_shape(
            &content_lines,
            inner_width,
            Some(content_height),
            None,
            false,
        );

        if !self.border {
            let line_count = content_lines.len();
            let mut out = Segments::new();
            for (idx, line) in content_lines.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let box_chars = rich_rs::r#box::SQUARE;
        let mut out_lines: Vec<Vec<Segment>> = Vec::new();

        let mut top = String::new();
        top.push(box_chars.top_left);
        let mut title = self.title.clone().unwrap_or_default();
        if !title.is_empty() && inner_width >= 2 {
            title = format!(" {title} ");
        }
        let title_width = rich_rs::cell_len(&title);
        if title_width >= inner_width {
            top.push_str(&rich_rs::set_cell_size(&title, inner_width));
        } else {
            let remaining = inner_width.saturating_sub(title_width);
            let left = remaining / 2;
            let right = remaining - left;
            top.push_str(&box_chars.top.to_string().repeat(left));
            top.push_str(&title);
            top.push_str(&box_chars.top.to_string().repeat(right));
        }
        top.push(box_chars.top_right);
        out_lines.push(vec![Segment::new(top)]);

        for line in content_lines.drain(..) {
            let mut middle = Vec::new();
            middle.push(Segment::new(box_chars.mid_left.to_string()));
            middle.extend(line);
            middle.push(Segment::new(box_chars.mid_right.to_string()));
            out_lines.push(middle);
        }

        let mut bottom = String::new();
        bottom.push(box_chars.bottom_left);
        bottom.push_str(&box_chars.bottom.to_string().repeat(inner_width));
        bottom.push(box_chars.bottom_right);
        out_lines.push(vec![Segment::new(bottom)]);

        let out_lines = Segment::set_shape(&out_lines, width, Some(height), None, false);
        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height().map(|child| {
            let border = if self.border { 2 } else { 0 };
            let meta = crate::css::selector_meta_generic(self);
            let resolved = crate::css::resolve_style(self, &meta);
            let padding = resolved.effective_padding();
            let (border_top, border_bottom, _border_left, _border_right) =
                crate::widgets::helpers::border_spacing_from_style(&resolved);
            let chrome_tb = usize::from(padding.top.saturating_add(padding.bottom))
                + border_top
                + border_bottom;
            child + self.padding * 2 + border + chrome_tb
        })
    }

    fn content_width(&self) -> Option<usize> {
        self.child.content_width().map(|child| {
            let border = if self.border { 2 } else { 0 };
            let meta = crate::css::selector_meta_generic(self);
            let resolved = crate::css::resolve_style(self, &meta);
            let padding = resolved.effective_padding();
            let (_border_top, _border_bottom, border_left, border_right) =
                crate::widgets::helpers::border_spacing_from_style(&resolved);
            let chrome_lr = usize::from(padding.left.saturating_add(padding.right))
                + border_left
                + border_right;
            child + self.padding * 2 + border + chrome_lr
        })
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Panel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_extraction_returns_child() {
        let mut panel = Panel::new(Spacer::new(1));
        let children = panel.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn panel_extraction_idempotent() {
        let mut panel = Panel::new(Spacer::new(1));
        let _ = panel.take_composed_children();
        assert!(panel.take_composed_children().is_empty());
    }

    #[test]
    fn panel_render_after_extraction_with_border() {
        let mut panel = Panel::new(Spacer::new(1)).title("Test");
        let _ = panel.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&panel, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn panel_render_after_extraction_no_border() {
        let mut panel = Panel::new(Spacer::new(1)).border(false);
        let _ = panel.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&panel, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn panel_uses_tree_path_after_extraction() {
        let mut panel = Panel::new(Spacer::new(1));
        assert!(!panel.child_extracted);
        let _ = panel.take_composed_children();
        assert!(panel.child_extracted);
    }
}
