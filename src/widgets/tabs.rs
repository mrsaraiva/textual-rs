use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

pub struct Tabs {
    id: WidgetId,
    tabs: Vec<Tab>,
    active: usize,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

pub struct Tab {
    title: String,
    child: Box<dyn Widget>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tabs: Vec::new(),
            active: 0,
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            classes: vec!["tabs".to_string()],
            focused_classes: vec!["tabs".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_tab(mut self, title: impl Into<String>, child: impl Widget + 'static) -> Self {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
        self
    }

    pub fn add_tab(&mut self, title: impl Into<String>, child: impl Widget + 'static) {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        self.activate(index, None);
    }

    fn activate(&mut self, index: usize, mut ctx: Option<&mut EventCtx>) {
        if self.tabs.is_empty() {
            self.active = 0;
            return;
        }
        let next = index.min(self.tabs.len() - 1);
        if next != self.active {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(false);
            }
            self.active = next;
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(true);
            }
            if let Some(ctx) = ctx.as_mut() {
                ctx.post_message(
                    self.id,
                    Message::TabActivated {
                        index: self.active,
                        title: self.tabs[self.active].title.clone(),
                    },
                );
                ctx.request_repaint();
            }
        }
    }

    pub fn activate_prev(&mut self) {
        self.activate_prev_with_ctx(None);
    }

    fn activate_prev_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        if self.tabs.is_empty() {
            return;
        }
        let prev = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.activate(prev, ctx);
    }

    pub fn activate_next(&mut self) {
        self.activate_next_with_ctx(None);
    }

    fn activate_next_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        if self.tabs.is_empty() {
            return;
        }
        let next = (self.active + 1) % self.tabs.len();
        self.activate(next, ctx);
    }

    fn tab_spans(&self, width: usize) -> Vec<(usize, usize, usize)> {
        let mut spans = Vec::new();
        let mut cursor = 0usize;
        for (index, tab) in self.tabs.iter().enumerate() {
            if cursor >= width {
                break;
            }
            let label = format!(" {} ", tab.title);
            let label_width = rich_rs::cell_len(&label);
            if label_width == 0 {
                continue;
            }
            let start = cursor;
            let end = start.saturating_add(label_width.saturating_sub(1));
            spans.push((start, end, index));
            cursor = cursor.saturating_add(label_width);
        }
        spans
    }

    fn hit_tab(&self, x: usize, y: usize) -> Option<usize> {
        if y > 0 {
            return None;
        }
        self.tab_spans(self.layout_width)
            .into_iter()
            .find(|(start, end, _)| x >= *start && x <= *end)
            .map(|(_, _, index)| index)
    }
}

impl Widget for Tabs {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.set_focus(focused);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hovered_tab = None;
        }
    }

    fn on_mount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.layout_width = usize::from(width).max(1);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_layout(width, height.saturating_sub(1));
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right => {
                        self.activate_next_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('h') => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('l') => {
                        self.activate_next_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.id {
                if let Some(index) = self.hit_tab(mouse.x as usize, mouse.y as usize) {
                    self.activate(index, Some(ctx));
                    ctx.set_handled();
                    return;
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_message(message, ctx);
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self.hit_tab(x as usize, y as usize);
        if hovered != self.hovered_tab {
            self.hovered_tab = hovered;
            return true;
        }
        false
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        for tab in &mut self.tabs {
            f(tab.child.as_mut());
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let mut header_line = Vec::new();
        if self.tabs.is_empty() {
            let bar_style = crate::css::resolve_component_style(self, &["tabs--bar"])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
            header_line.push(Segment::styled(" no tabs ".to_string(), bar_style));
        } else {
            for (idx, tab) in self.tabs.iter().enumerate() {
                let mut classes = vec!["tabs--tab"];
                if idx == self.active {
                    classes.push("-active");
                    if self.focused {
                        classes.push("-focus");
                    }
                }
                if self.hovered_tab == Some(idx) {
                    classes.push("-hover");
                }
                let style = crate::css::resolve_component_style(self, &classes)
                    .to_rich()
                    .unwrap_or_else(rich_rs::Style::new);
                header_line.push(Segment::styled(format!(" {} ", tab.title), style));
            }
        }
        let header_line = adjust_line_length_no_bg(&header_line, width);
        let mut lines = vec![header_line];

        if height > 1 {
            if let Some(tab) = self.tabs.get(self.active) {
                let mut child_options = options.clone();
                child_options.size = (width, height - 1);
                child_options.max_width = width;
                child_options.max_height = height - 1;
                let child_segments = tab.child.render_styled(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines =
                    Segment::set_shape(&child_lines, width, Some(height - 1), None, false);
                lines.extend(child_lines);
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
        let child_height = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.child.layout_height());
        child_height.map(|height| height + 1)
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

impl Renderable for Tabs {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
