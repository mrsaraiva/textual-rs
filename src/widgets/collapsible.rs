use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::css;
use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{
        adjust_line_length_no_bg, clamp_with_constraints, constraints_from_style, empty_classes,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};

pub struct Collapsible {
    id: WidgetId,
    title: String,
    collapsed: bool,
    collapsed_symbol: String,
    expanded_symbol: String,
    focused: bool,
    hovered: bool,
    pressed: bool,
    children: Vec<Box<dyn Widget>>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Collapsible {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            title: title.into(),
            collapsed: true,
            collapsed_symbol: "\u{25b6}".to_string(),
            expanded_symbol: "\u{25bc}".to_string(),
            focused: false,
            hovered: false,
            pressed: false,
            children: Vec::new(),
            classes: vec!["collapsible".to_string()],
            focused_classes: vec!["collapsible".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn collapsed_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.collapsed_symbol = symbol.into();
        self
    }

    pub fn expanded_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.expanded_symbol = symbol.into();
        self
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn add_child(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    fn toggle_with_ctx(&mut self, ctx: &mut EventCtx) {
        self.collapsed = !self.collapsed;
        ctx.post_message(
            self.id,
            Message::CollapsibleToggled {
                collapsed: self.collapsed,
            },
        );
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn current_symbol(&self) -> &str {
        if self.collapsed {
            &self.collapsed_symbol
        } else {
            &self.expanded_symbol
        }
    }
}

impl Widget for Collapsible {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn style_type(&self) -> &'static str {
        "Collapsible"
    }

    fn content_width(&self) -> Option<usize> {
        let symbol_width = rich_rs::cell_len(self.current_symbol());
        let title_width = rich_rs::cell_len(&self.title);
        // symbol + space + title
        Some(
            symbol_width
                .saturating_add(1)
                .saturating_add(title_width)
                .max(1),
        )
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
        if !self.collapsed {
            for child in &mut self.children {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.collapsed {
            for child in &mut self.children {
                child.on_event_capture(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        // Delegate to children first when expanded
        if !self.collapsed {
            for child in &mut self.children {
                child.on_event(event, ctx);
                if ctx.handled() {
                    return;
                }
            }
        }

        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                // Click on the title bar area (y == 0) toggles
                if mouse.y == 0 {
                    self.pressed = true;
                    ctx.request_repaint();
                    ctx.set_handled();
                }
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target == Some(self.id) && mouse.y == 0 {
                        self.toggle_with_ctx(ctx);
                        return;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_with_ctx(ctx);
                    return;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if !self.collapsed {
            for child in &mut self.children {
                child.on_message(message, ctx);
            }
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        if !self.collapsed {
            for child in &mut self.children {
                f(child.as_mut());
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        // Render the title line
        let title_style = crate::css::resolve_component_style(self, &["collapsible--title"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let title_text = format!("{} {}", self.current_symbol(), self.title);
        let title_text = rich_rs::set_cell_size(&title_text, width);
        let title_line = vec![Segment::styled(title_text, title_style)];
        let title_line = adjust_line_length_no_bg(&title_line, width);

        let mut lines = vec![title_line];

        // When expanded, render children below the title
        if !self.collapsed && height > 1 {
            let child_height_limit = height.saturating_sub(1);
            let mut cursor_y = 0usize;

            for child in &self.children {
                if cursor_y >= child_height_limit {
                    break;
                }
                let meta = css::selector_meta_generic(child.as_ref());
                let resolved = css::resolve_style(child.as_ref(), &meta);
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
                let remaining = child_height_limit.saturating_sub(cursor_y);
                let render_height = clamp_with_constraints(
                    remaining.saturating_sub(margin.top + margin.bottom).max(1),
                    constraints.min_height,
                    constraints.max_height,
                    remaining.saturating_sub(margin.top + margin.bottom).max(1),
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
                child_lines = Segment::set_shape(
                    &child_lines,
                    render_width,
                    Some(target_height),
                    None,
                    false,
                );
                child_lines = pad_lines_to_width(child_lines, render_width);
                child_lines = super::helpers::apply_margin(child_lines, width, margin);

                let child_h = child_lines.len();
                for line in child_lines {
                    if lines.len() >= height {
                        break;
                    }
                    lines.push(line);
                }
                cursor_y += child_h;
            }
        }

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
        if self.collapsed {
            return Some(1);
        }
        // Title line + children heights
        let mut total = 1usize;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            match child.layout_height() {
                Some(height) => {
                    total = total
                        .saturating_add(height)
                        .saturating_add(margin.top + margin.bottom);
                }
                None => return None,
            }
        }
        Some(total.max(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
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

impl Renderable for Collapsible {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
