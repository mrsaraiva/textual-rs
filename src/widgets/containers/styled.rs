use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;
use crate::style::Style;

use crate::widgets::{
    LayoutConstraints, Spacer, Widget, WidgetStyles,
    helpers::{fixed_height_from_constraints, merge_constraints},
};

pub struct Styled {
    child: Box<dyn Widget>,
    styles: WidgetStyles,
    child_extracted: bool,
}

impl Styled {
    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        let mut styles = WidgetStyles::default();
        styles.style = style;
        Self {
            child: Box::new(child),
            styles,
            child_extracted: false,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.styles.style = style;
        self
    }
}

impl Widget for Styled {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
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

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_layout(&mut self, _width: u16, _height: u16) {}

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}

    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut EventCtx) {}

    fn focusable(&self) -> bool {
        false
    }

    fn set_focus(&mut self, _focused: bool) {}

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.child.layout_constraints())
    }

    fn style(&self) -> Option<Style> {
        Some(self.styles.style.clone())
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_type(&self) -> &'static str {
        "Styled"
    }
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
        let children = s.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn styled_extraction_idempotent() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.take_composed_children();
        assert!(s.take_composed_children().is_empty());
    }

    #[test]
    fn styled_render_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.take_composed_children();
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
        let _ = s.take_composed_children();
        assert_eq!(s.style_type(), "Styled");
    }
}
