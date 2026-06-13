use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use crate::widgets::{
    LayoutConstraints, NodeSeed, Spacer, Widget,
    helpers::{clamp_with_constraints, merge_constraints},
};

pub struct Constrained {
    child: Box<dyn Widget>,
    constraints: LayoutConstraints,
    seed: NodeSeed,
    child_extracted: bool,
    extracted_child_layout_height: Option<usize>,
    extracted_child_content_width: Option<usize>,
}

impl Constrained {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            constraints: LayoutConstraints::default(),
            seed: NodeSeed::default(),
            child_extracted: false,
            extracted_child_layout_height: None,
            extracted_child_content_width: None,
        }
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_width(value);
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_width(value);
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_height(value);
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_height(value);
        self
    }

    fn seed_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.seed.styles.layout, self.constraints)
    }
}

impl Widget for Constrained {
    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        self.extracted_child_layout_height = self.child.layout_height();
        self.extracted_child_content_width = self.child.content_width();
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.child_extracted {
            return Segments::new();
        }
        let constraints = self.seed_constraints();
        let width = clamp_with_constraints(
            options.size.0.max(1),
            constraints.min_width,
            constraints.max_width,
            options.size.0.max(1),
        );
        let height = clamp_with_constraints(
            options.size.1.max(1),
            constraints.min_height,
            constraints.max_height,
            options.size.1.max(1),
        );

        let mut child_options = options.clone();
        child_options.size = (width, height);
        child_options.max_width = width;
        child_options.max_height = height;
        self.child.render_styled(console, &child_options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        Widget::render(self, console, options)
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
        let constraints = self.seed_constraints();
        let width = clamp_with_constraints(
            usize::from(width.max(1)),
            constraints.min_width,
            constraints.max_width,
            usize::from(width.max(1)),
        ) as u16;
        let height = clamp_with_constraints(
            usize::from(height.max(1)),
            constraints.min_height,
            constraints.max_height,
            usize::from(height.max(1)),
        ) as u16;
        self.child.on_layout(width, height);
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

    fn focusable(&self) -> bool {
        !self.child_extracted && self.child.focusable()
    }

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.seed_constraints();
        let child_height = if self.child_extracted {
            self.extracted_child_layout_height
        } else {
            self.child.layout_height()
        };
        match (constraints.min_height, constraints.max_height, child_height) {
            (Some(min), Some(max), Some(child)) => Some(child.max(min).min(max)),
            (Some(min), Some(max), None) if min == max => Some(min),
            (Some(min), _, Some(child)) => Some(child.max(min)),
            (_, Some(max), Some(child)) => Some(child.min(max)),
            (_, _, other) => other,
        }
    }

    fn content_width(&self) -> Option<usize> {
        if self.child_extracted {
            return self.extracted_child_content_width;
        }
        self.child.content_width()
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Constrained {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constrained_extraction_returns_child() {
        let mut c = Constrained::new(Spacer::new(1));
        let children = c.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn constrained_extraction_idempotent() {
        let mut c = Constrained::new(Spacer::new(1));
        let _ = c.take_composed_children();
        assert!(c.take_composed_children().is_empty());
    }

    #[test]
    fn constrained_render_after_extraction() {
        let mut c = Constrained::new(Spacer::new(1));
        let _ = c.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&c, &console, &options);
        assert!(segments.is_empty());
    }
}
