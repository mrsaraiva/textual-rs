use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::Event;
use crate::message::MessageEvent;

use crate::widgets::{NodeSeed, Spacer, Widget};

pub struct Panel {
    child: Box<dyn Widget>,
    title: Option<String>,
    padding: usize,
    border: bool,
    seed: NodeSeed,
    child_extracted: bool,
}

impl Panel {
    crate::seed_ident_methods!();

    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            title: None,
            padding: 0,
            border: true,
            seed: NodeSeed::default(),
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

    fn compose(&mut self) -> crate::compose::ComposeResult {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![crate::compose::ChildDecl::new(child)]
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
                let middle = vec![
                    Segment::new(box_chars.mid_left.to_string()),
                    Segment::new(" ".repeat(inner_width)),
                    Segment::new(box_chars.mid_right.to_string()),
                ];
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
        content_lines.extend(child_lines);
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
        // Report the child's height plus Panel's OWN STRUCTURAL frame (the border
        // it always draws + its structural `self.padding`). CSS-resolved chrome
        // (an author's `Panel { border/padding }`) is NOT added here — the flow
        // layout adds that via `full_v_chrome`, symmetric with the width axis.
        // (Previously this also baked the CSS-resolved `chrome_tb`, which now
        // double-counts against the layout side; removed as part of the
        // pure-content height-chrome keystone.)
        self.child
            .layout_height()
            .map(|child| child + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn content_width(&self) -> Option<usize> {
        // Structural frame only (see `layout_height`); the flow layout's width arm
        // adds the CSS-resolved `full_h_chrome`, so the CSS `chrome_lr` is not
        // baked here either.
        self.child
            .content_width()
            .map(|child| child + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn on_mount(&mut self, ctx: &mut crate::event::WidgetCtx) {
        if !self.child_extracted {
            self.child.on_mount(ctx);
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

    fn on_event_capture(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if !self.child_extracted {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if !self.child_extracted {
            self.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if !self.child_extracted {
            self.child.on_message(message, ctx);
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
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

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    crate::seed_style_identity_methods!();
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
        let children = panel.compose();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn panel_layout_height_reports_structural_frame_only_not_css_chrome() {
        // Height-chrome keystone guard: Panel's `layout_height` must report the
        // child height + its OWN STRUCTURAL frame (border + `self.padding`) and
        // NOT the CSS-resolved border/padding — the flow layout adds that via
        // `full_v_chrome`. Baking it here would double-count. Even under a
        // stylesheet that adds a CSS `Panel { border; padding }`, the reported
        // height must stay child(1) + structural border(2) + structural padding(0).
        use crate::css::StyleSheet;
        let sheet = StyleSheet::parse("Panel { border: solid; padding: 2; }");
        let _guard = crate::css::set_style_context(sheet);

        let panel = Panel::new(Spacer::new(1)); // border=true, padding=0
        assert_eq!(
            panel.layout_height(),
            Some(3),
            "layout_height must be child(1)+structural border(2), excluding CSS chrome"
        );
    }

    #[test]
    fn panel_extraction_idempotent() {
        let mut panel = Panel::new(Spacer::new(1));
        let _ = panel.compose();
        assert!(panel.compose().is_empty());
    }

    #[test]
    fn panel_render_after_extraction_with_border() {
        let mut panel = Panel::new(Spacer::new(1)).title("Test");
        let _ = panel.compose();
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
        let _ = panel.compose();
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
        let _ = panel.compose();
        assert!(panel.child_extracted);
    }
}
