use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::compose::ComposeResult;
use crate::event::{Event, EventCtx};
use crate::widgets::{Container, Widget, WidgetStyles};

use super::thin::{align_line_horizontal, effective_rendered_height};

pub struct CenterMiddle {
    child: Container,
    styles: WidgetStyles,
}

impl CenterMiddle {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.child = self.child.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }
}

impl Widget for CenterMiddle {
    fn compose(&self) -> ComposeResult {
        self.child.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.child.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let child_width = self
            .child
            .content_width()
            .unwrap_or(width)
            .max(1)
            .min(width);
        let intrinsic_height = self
            .child
            .layout_height()
            .unwrap_or(height)
            .max(1)
            .min(height);

        let mut child_options = options.clone();
        child_options.size = (child_width, intrinsic_height);
        child_options.max_width = child_width;
        child_options.max_height = intrinsic_height;

        let segments = self.child.render_styled(console, &child_options);
        let lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
        let child_height = self
            .child
            .layout_height()
            .unwrap_or_else(|| effective_rendered_height(&lines))
            .max(1)
            .min(height);
        let top = height.saturating_sub(child_height) / 2;
        let left = width.saturating_sub(child_width) / 2;

        let mut out_lines: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for _ in 0..top {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
        }
        out_lines.extend(
            lines
                .into_iter()
                .take(child_height)
                .map(|line| align_line_horizontal(&line, width, child_width, left)),
        );
        while out_lines.len() < height {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
        }

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
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.child.on_mouse_move(x, y)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}
