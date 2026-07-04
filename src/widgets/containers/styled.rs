use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::Event;
use crate::message::MessageEvent;
use crate::style::Style;

use crate::widgets::{LayoutConstraints, NodeSeed, Spacer, Widget};

pub struct Styled {
    child: Box<dyn Widget>,
    seed: NodeSeed,
    child_extracted: bool,
}

impl Styled {
    crate::seed_ident_methods!();

    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        let mut seed = NodeSeed::default();
        seed.styles.style = style;
        Self {
            child: Box::new(child),
            seed,
            child_extracted: false,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.seed.styles.style = style;
        self
    }

    fn seed_constraints(&self) -> LayoutConstraints {
        self.seed.styles.layout
    }
}

impl Widget for Styled {
    fn compose(&mut self) -> crate::compose::ComposeResult {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![crate::compose::ChildDecl::new(child)]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let _ = (console, options);
        Segments::new()
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let _ = (console, options, debug);
        Segments::new()
    }

    fn on_mount(&mut self, _ctx: &mut crate::event::WidgetCtx) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_layout(&mut self, _width: u16, _height: u16) {}

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut crate::event::WidgetCtx) {}

    fn on_event(&mut self, _event: &Event, _ctx: &mut crate::event::WidgetCtx) {}

    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {}

    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut crate::event::WidgetCtx) {}

    fn focusable(&self) -> bool {
        false
    }

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.seed_constraints();
        if let (Some(min), Some(max)) = (constraints.min_height, constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        self.child.layout_height()
    }

    fn style(&self) -> Option<Style> {
        let s = self.seed.styles.style.clone();
        if s == Default::default() {
            None
        } else {
            Some(s)
        }
    }

    fn style_type(&self) -> &'static str {
        "Styled"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        // Merge incoming layout hints (dock, height, box_sizing) with the
        // existing widget style (borders, bg, etc.) so neither loses its
        // properties. The incoming style's fields win only where they are set.
        self.seed.styles.style = self.seed.styles.style.combine(&style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    crate::seed_style_identity_methods!();
}

impl Renderable for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_extraction_returns_child() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let children = s.compose();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn styled_extraction_idempotent() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.compose();
        assert!(s.compose().is_empty());
    }

    #[test]
    fn styled_render_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.compose();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&s, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn styled_style_type_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.compose();
        assert_eq!(s.style_type(), "Styled");
    }
}
