use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::css;
use crate::event::{Event, EventCtx};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{
        apply_margin, clamp_with_constraints, constraints_from_style,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};

pub struct ContentSwitcher {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    current: Option<String>,
    styles: WidgetStyles,
}

impl Default for ContentSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentSwitcher {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
            current: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn initial(mut self, id: impl Into<String>) -> Self {
        self.current = Some(id.into());
        self
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn add_child(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn set_current(&mut self, current: Option<String>) {
        self.current = current;
    }

    fn visible_child(&self) -> Option<&dyn Widget> {
        let current = self.current.as_deref()?;
        self.children
            .iter()
            .find(|child| child.style_id() == Some(current))
            .map(|child| child.as_ref())
    }

    fn visible_child_mut(&mut self) -> Option<&mut Box<dyn Widget>> {
        let current = self.current.as_deref()?;
        // Find the index first to avoid borrow-checker issues with self.current.
        let idx = self
            .children
            .iter()
            .position(|child| child.style_id() == Some(current))?;
        Some(&mut self.children[idx])
    }
}

impl Widget for ContentSwitcher {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        false
    }

    fn style_type(&self) -> &'static str {
        "ContentSwitcher"
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(child) = self.visible_child_mut() {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(child) = self.visible_child_mut() {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_message(message, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        if let Some(child) = self.visible_child_mut() {
            f(child.as_mut());
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let child = match self.visible_child() {
            Some(child) => child,
            None => {
                // No visible child: render empty space
                let mut out = Segments::new();
                out.push(Segment::styled(
                    " ".repeat(width),
                    rich_rs::Style::new(),
                ));
                return out;
            }
        };

        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        let style_constraints = constraints_from_style(&resolved);
        let constraints = merge_constraints(style_constraints, child.layout_constraints());
        let available_width = width.saturating_sub(margin.left + margin.right).max(1);
        let render_width = clamp_with_constraints(
            available_width,
            constraints.min_width,
            constraints.max_width,
            available_width,
        );
        let render_height = clamp_with_constraints(
            height.saturating_sub(margin.top + margin.bottom).max(1),
            constraints.min_height,
            constraints.max_height,
            height.saturating_sub(margin.top + margin.bottom).max(1),
        );
        let render_height = if let Some(fixed) = child.layout_height() {
            render_height.min(fixed.max(1))
        } else {
            render_height
        };

        let mut child_options = options.clone();
        child_options.size = (render_width, render_height);
        child_options.max_width = render_width;
        child_options.max_height = render_height;

        let segments = child.render_styled(console, &child_options);
        let mut child_lines =
            Segment::split_and_crop_lines(segments, render_width, None, true, false);
        let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
        target_height = clamp_with_constraints(
            target_height,
            constraints.min_height,
            constraints.max_height,
            target_height,
        );
        child_lines =
            Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
        child_lines = pad_lines_to_width(child_lines, render_width);
        let lines = apply_margin(child_lines, width, margin);

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

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let child = self.visible_child()?;
        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        child
            .layout_height()
            .map(|h| h.saturating_add(margin.top + margin.bottom).max(1))
    }

    fn content_width(&self) -> Option<usize> {
        let child = self.visible_child()?;
        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        child
            .content_width()
            .map(|w| w.saturating_add(margin.left + margin.right).max(1))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for ContentSwitcher {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
