use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, BindingHint, Event, EventCtx};
use crate::message::*;
use crate::style::parse_color_like;

use super::footer::FooterBinding;
use super::helpers::{
    adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints, pad_lines_to_width,
};

use super::{ScrollView, Widget, WidgetStyles};

#[derive(Debug, Clone)]
pub struct BindingsTable {
    bindings: Vec<FooterBinding>,
    id: Option<String>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl BindingsTable {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            id: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_bindings(mut self, bindings: Vec<FooterBinding>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    fn line_count(&self) -> usize {
        self.bindings.len().max(1)
    }

    fn component_style(&self, classes: &[&str], fallback: rich_rs::Style) -> rich_rs::Style {
        let meta = crate::css::selector_meta_component("KeyPanel", classes);
        let resolved = crate::css::resolve_style_for_meta(&meta);
        if resolved.is_empty() {
            fallback
        } else {
            resolved.to_rich().unwrap_or(fallback)
        }
    }

    fn component_styles(
        &self,
    ) -> (
        rich_rs::Style,
        rich_rs::Style,
        rich_rs::Style,
        rich_rs::Style,
    ) {
        let key_fallback = rich_rs::Style::new()
            .with_color(
                parse_color_like("$text-accent")
                    .or_else(|| parse_color_like("$primary"))
                    .unwrap_or_else(|| crate::style::Color::rgb(183, 55, 99))
                    .to_simple_opaque(),
            )
            .with_bold(true);
        let description_fallback = rich_rs::Style::new().with_color(
            parse_color_like("$foreground")
                .unwrap_or_else(|| crate::style::Color::rgb(215, 219, 224))
                .to_simple_opaque(),
        );
        let divider_fallback = rich_rs::Style::new()
            .with_color(
                parse_color_like("$border-blurred")
                    .or_else(|| parse_color_like("$foreground"))
                    .unwrap_or_else(|| crate::style::Color::rgb(127, 134, 141))
                    .to_simple_opaque(),
            )
            .with_dim(true);
        let header_fallback = rich_rs::Style::new()
            .with_color(
                parse_color_like("$text")
                    .or_else(|| parse_color_like("$foreground"))
                    .unwrap_or_else(|| crate::style::Color::rgb(242, 244, 246))
                    .to_simple_opaque(),
            )
            .with_bold(true)
            .with_underline(true);
        (
            self.component_style(&["bindings-table--key"], key_fallback),
            self.component_style(&["bindings-table--description"], description_fallback),
            self.component_style(&["bindings-table--divider"], divider_fallback),
            self.component_style(&["bindings-table--header"], header_fallback),
        )
    }

    fn lines(&self, width: usize) -> Vec<Vec<Segment>> {
        let (key_style, description_style, _divider_style, _header_style) = self.component_styles();
        if self.bindings.is_empty() {
            return vec![adjust_line_length_no_bg(
                &[Segment::styled(
                    "(no bindings)".to_string(),
                    description_style,
                )],
                width,
            )];
        }

        let key_column_width = self
            .bindings
            .iter()
            .map(|binding| rich_rs::cell_len(&binding.key))
            .max()
            .unwrap_or(0)
            .min(24)
            .max(3);

        let mut out = Vec::new();
        for binding in &self.bindings {
            let key_len = rich_rs::cell_len(&binding.key);
            let key = format!(
                "{}{}",
                " ".repeat(key_column_width.saturating_sub(key_len)),
                binding.key
            );
            out.push(adjust_line_length_no_bg(
                &[
                    Segment::styled(key, key_style),
                    Segment::new("  ".to_string()),
                    Segment::styled(binding.description.clone(), description_style),
                ],
                width,
            ));
        }
        out
    }
}

impl Widget for BindingsTable {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut lines = self.lines(width);
        lines = pad_lines_to_width(lines, width);
        let line_count = lines.len();
        let mut out = Segments::new();
        for (index, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if index + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.line_count()))
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for BindingsTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug)]
pub struct KeyPanel {
    id: Option<String>,
    title: String,
    table: BindingsTable,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    drag_v: Option<usize>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl KeyPanel {
    pub fn new() -> Self {
        Self {
            id: None,
            title: "Key Bindings".to_string(),
            table: BindingsTable::new().with_id("bindings-table"),
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            widget_width: AtomicUsize::new(1),
            widget_height: AtomicUsize::new(1),
            drag_v: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_bindings(mut self, bindings: Vec<FooterBinding>) -> Self {
        self.table.set_bindings(bindings);
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.table.set_bindings(bindings);
        self.clamp_offset();
    }

    pub fn set_binding_hints(&mut self, bindings: &[BindingHint]) {
        let mut seen = std::collections::BTreeSet::new();
        let mut mapped = Vec::new();
        for hint in bindings {
            // Python parity: key panel hides only system bindings.
            if hint.system {
                continue;
            }
            let key = hint.key_display.clone().unwrap_or_else(|| hint.key.clone());
            let signature = (key.clone(), hint.description.clone());
            if !seen.insert(signature) {
                continue;
            }
            // Footer grouping is a footer concern. KeyPanel groups by namespace
            // in Python, which we model elsewhere.
            mapped.push(FooterBinding::new(key, hint.description.clone()));
        }
        self.set_bindings(mapped);
    }

    fn emit_scroll_changed_message(&self, ctx: &mut EventCtx) {
        ctx.post_message(Message::KeyPanelScrolled(KeyPanelScrolled {
            offset: self.offset_y,
            max_offset: self.max_offset(),
        }));
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    fn max_offset(&self) -> usize {
        ScrollView::line_max_offset(
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        )
    }

    fn clamp_offset(&mut self) {
        self.offset_y = ScrollView::line_clamp_offset(
            self.offset_y,
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn scroll_by(&mut self, delta: i32) {
        self.offset_y = ScrollView::line_scroll_by(
            self.offset_y,
            delta,
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn can_scroll(&self) -> bool {
        self.content_height.load(Ordering::Relaxed) > self.viewport_height.load(Ordering::Relaxed)
    }
}

impl Widget for KeyPanel {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);
        const V_SCROLLBAR_SIZE: usize = 1;
        let body_viewport = height.max(1);
        let mut viewport_width = width;
        let mut table_lines = self.table.lines(viewport_width);
        let mut content_height = table_lines.len().max(1);
        let mut show_scrollbar = content_height > body_viewport && width > 2;
        if show_scrollbar {
            viewport_width = width.saturating_sub(V_SCROLLBAR_SIZE).max(1);
            table_lines = self.table.lines(viewport_width);
            content_height = table_lines.len().max(1);
            show_scrollbar = content_height > body_viewport;
        }
        self.viewport_height.store(body_viewport, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(body_viewport);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(content_height);
        let end = (start + body_viewport).min(content_height);
        let mut body = table_lines[start..end].to_vec();
        body = pad_lines_to_width(body, viewport_width);
        while body.len() < body_viewport {
            body.push(vec![Segment::new(" ".repeat(viewport_width))]);
        }

        if show_scrollbar {
            let (track_style, thumb_style, thumb_active_style) =
                ScrollView::line_scrollbar_styles();
            let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                body_viewport,
                content_height,
                body_viewport,
                offset,
            );
            let mut thumb_drawn = false;
            for (row, line) in body.iter_mut().enumerate() {
                let active = row >= thumb_start && row < thumb_start + thumb_len;
                line.push(Segment::styled(
                    " ".to_string(),
                    if active {
                        if self.drag_v.is_some() {
                            thumb_active_style
                        } else {
                            thumb_style
                        }
                    } else {
                        track_style
                    },
                ));
                thumb_drawn |= active;
            }
            if !thumb_drawn && !body.is_empty() {
                let row = body_viewport.saturating_sub(1).min(body.len() - 1);
                if !body[row].is_empty() {
                    body[row].pop();
                }
                let active_style = if self.drag_v.is_some() {
                    thumb_active_style
                } else {
                    thumb_style
                };
                body[row].push(Segment::styled(" ".to_string(), active_style));
            }
        }

        let mut out = Segments::new();
        for (index, line) in body.into_iter().enumerate() {
            out.extend(line);
            if index + 1 < body_viewport {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::BindingsChanged(bindings) = event {
            let previous = self.table.bindings.clone();
            self.set_binding_hints(bindings);
            if self.table.bindings != previous {
                ctx.post_message(Message::KeyPanelBindingsUpdated(KeyPanelBindingsUpdated {
                    count: self.table.bindings.len(),
                }));
                ctx.request_repaint();
            }
            return;
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                let width = self.widget_width.load(Ordering::Relaxed).max(1);
                let body_viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
                let content_height = self.content_height.load(Ordering::Relaxed).max(1);
                if content_height > body_viewport
                    && width > 1
                    && mouse.x as usize >= width.saturating_sub(1)
                    && mouse.y > 0
                {
                    let local_y = (mouse.y as usize).saturating_sub(1);
                    if local_y < body_viewport {
                        let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                            body_viewport,
                            content_height,
                            body_viewport,
                            self.offset_y,
                        );
                        if local_y >= thumb_start && local_y < thumb_start.saturating_add(thumb_len)
                        {
                            self.drag_v = Some(local_y.saturating_sub(thumb_start));
                            ctx.set_handled();
                            return;
                        }
                        let before = self.offset_y;
                        if local_y < thumb_start {
                            self.scroll_by(-(body_viewport as i32));
                        } else {
                            self.scroll_by(body_viewport as i32);
                        }
                        if self.offset_y != before {
                            ctx.request_repaint();
                            self.emit_scroll_changed_message(ctx);
                        }
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }
        if matches!(event, Event::MouseUp(_) | Event::AppFocus(false)) {
            if self.drag_v.take().is_some() {
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
        if let Event::Action(action) = event {
            if !self.can_scroll() {
                return;
            }
            let before = self.offset_y;
            match action {
                Action::ScrollUp => self.scroll_by(-(self.scroll_step as i32)),
                Action::ScrollDown => self.scroll_by(self.scroll_step as i32),
                Action::ScrollPageUp => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(-(page as i32));
                }
                Action::ScrollPageDown => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(page as i32);
                }
                _ => return,
            }
            if self.offset_y != before {
                ctx.request_repaint();
                self.emit_scroll_changed_message(ctx);
                ctx.set_handled();
            }
        }
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if delta_y == 0 || !self.can_scroll() {
            return;
        }
        let before = self.offset_y;
        self.scroll_by(delta_y.saturating_mul(self.scroll_step as i32));
        if self.offset_y != before {
            ctx.request_repaint();
            self.emit_scroll_changed_message(ctx);
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        let Some(grab_offset) = self.drag_v else {
            return false;
        };
        let body_viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        let content_height = self.content_height.load(Ordering::Relaxed).max(1);
        if content_height <= body_viewport {
            return false;
        }

        let local_y = (y as usize).saturating_sub(1);
        let new_offset = ScrollView::line_drag_offset(
            local_y,
            grab_offset,
            body_viewport,
            content_height,
            body_viewport,
            self.offset_y,
        );
        if new_offset != self.offset_y {
            self.offset_y = new_offset;
            return true;
        }
        false
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for KeyPanel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::KeyPanel;
    use crate::event::{Action, BindingHint, Event, EventCtx};
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::widgets::{FooterBinding, Widget};
    use rich_rs::Console;

    fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
        let mut options = console.options().clone();
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;
        options
    }

    #[test]
    fn bindings_changed_posts_bindings_updated_message() {
        let mut panel = KeyPanel::new();
        let mut ctx = EventCtx::default();
        panel.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| matches!(
            m.message,
            Message::KeyPanelBindingsUpdated(KeyPanelBindingsUpdated { count: 1 })
        )));
    }

    #[test]
    fn binding_hints_filter_system_entries_only() {
        let mut panel = KeyPanel::new();
        let hints = vec![
            BindingHint::new("ctrl+p", "Palette")
                .hidden(true)
                .with_system(true),
            BindingHint::new("j", "Jessica"),
            BindingHint::new("j", "Jessica"),
            BindingHint::new("p", "Paul").with_system(true),
            BindingHint::new("l", "Leto"),
            BindingHint::new("tab", "Focus Next").hidden(true),
        ];
        panel.set_binding_hints(&hints);
        assert_eq!(
            panel.table.bindings,
            vec![
                FooterBinding::new("j", "Jessica"),
                FooterBinding::new("l", "Leto"),
                FooterBinding::new("tab", "Focus Next")
            ]
        );
    }

    #[test]
    fn scroll_action_posts_scrolled_message() {
        let console = Console::new();
        let options = options_for(&console, 32, 4);
        let mut panel = KeyPanel::new().with_bindings(vec![
            FooterBinding::new("a", "one"),
            FooterBinding::new("b", "two"),
            FooterBinding::new("c", "three"),
            FooterBinding::new("d", "four"),
            FooterBinding::new("e", "five"),
            FooterBinding::new("f", "six"),
            FooterBinding::new("g", "seven"),
        ]);
        let _ = panel.render(&console, &options);

        let mut ctx = EventCtx::default();
        panel.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::KeyPanelScrolled(..)))
        );
    }

    #[test]
    fn mouse_up_after_thumb_drag_requests_repaint() {
        let console = Console::new();
        let options = options_for(&console, 32, 6);
        let bindings = (1..=16)
            .map(|index| FooterBinding::new(format!("k{index:02}"), format!("item {index:02}")))
            .collect::<Vec<_>>();
        let mut panel = KeyPanel::new().with_bindings(bindings);
        let _ = panel.render(&console, &options);

        let id = NodeId::default();
        let mut down_ctx = EventCtx::default();
        panel.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 31,
                screen_y: 1,
                x: 31,
                y: 1,
            }),
            &mut down_ctx,
        );
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        panel.on_event(
            &Event::MouseUp(crate::event::MouseUpEvent {
                target: Some(id),
                screen_x: 31,
                screen_y: 1,
                x: 31,
                y: 1,
            }),
            &mut up_ctx,
        );
        assert!(up_ctx.handled());
        assert!(up_ctx.repaint_requested());
    }
}
