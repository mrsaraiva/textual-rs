use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, BindingHint, Event, EventCtx};
use crate::keys::format_key_display;
use crate::message::*;
use crate::style::parse_color_like;

use super::footer::FooterBinding;
use super::helpers::{adjust_line_length_no_bg, pad_lines_to_width};

use super::{NodeSeed, ScrollBar, ScrollView, Widget};

pub(crate) const KEY_PANEL_VSCROLLBAR_ID: &str = "__key_panel_vscrollbar";

#[derive(Debug, Clone)]
pub struct BindingsTable {
    bindings: Vec<FooterBinding>,
    seed: NodeSeed,
}

impl BindingsTable {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            seed: NodeSeed::default(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
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
        let tooltip_style = description_style.with_dim(true);
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
        let description_width = width
            .saturating_sub(key_column_width.saturating_add(2))
            .max(1);

        let mut out = Vec::new();
        let mut previous_group: Option<String> = None;
        for binding in &self.bindings {
            if let Some(group) = &binding.group {
                if let Some(previous) = &previous_group
                    && previous != group
                {
                    out.push(adjust_line_length_no_bg(
                        &[Segment::new(String::new())],
                        width,
                    ));
                }
                previous_group = Some(group.clone());
            }

            let key_len = rich_rs::cell_len(&binding.key);
            let key = format!(
                "{}{}",
                " ".repeat(key_column_width.saturating_sub(key_len)),
                binding.key
            );

            let mut description_lines =
                wrap_text_for_width(&binding.description, description_width);
            if description_lines.is_empty() {
                description_lines.push(String::new());
            }

            out.push(adjust_line_length_no_bg(
                &[
                    Segment::styled(key, key_style),
                    Segment::new("  ".to_string()),
                    Segment::styled(description_lines[0].clone(), description_style),
                ],
                width,
            ));

            let key_blank = " ".repeat(key_column_width);
            for extra in description_lines.into_iter().skip(1) {
                out.push(adjust_line_length_no_bg(
                    &[
                        Segment::styled(key_blank.clone(), key_style),
                        Segment::new("  ".to_string()),
                        Segment::styled(extra, description_style),
                    ],
                    width,
                ));
            }

            if let Some(tooltip) = &binding.tooltip {
                for line in wrap_text_for_width(tooltip, description_width) {
                    out.push(adjust_line_length_no_bg(
                        &[
                            Segment::styled(key_blank.clone(), key_style),
                            Segment::new("  ".to_string()),
                            Segment::styled(line, tooltip_style),
                        ],
                        width,
                    ));
                }
            }
        }
        out
    }
}

fn wrap_text_for_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }
            let projected = rich_rs::cell_len(&current) + 1 + rich_rs::cell_len(word);
            if projected <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        } else if paragraph.is_empty() {
            lines.push(String::new());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn format_binding_key_display(binding_key: &str) -> String {
    binding_key
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            if matches!(part, "tab" | "shift+tab") {
                part.to_string()
            } else {
                format_key_display(part)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
        Some(self.line_count())
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for BindingsTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug)]
pub struct KeyPanel {
    title: String,
    table: BindingsTable,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    scrollbar_extracted: bool,
    seed: NodeSeed,
}

impl KeyPanel {
    pub fn new() -> Self {
        Self {
            title: "Key Bindings".to_string(),
            table: BindingsTable::new().with_id("bindings-table"),
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            widget_width: AtomicUsize::new(1),
            widget_height: AtomicUsize::new(1),
            scrollbar_extracted: false,
            seed: NodeSeed::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
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
            let key = hint
                .key_display
                .clone()
                .unwrap_or_else(|| format_binding_key_display(&hint.key));
            let namespace = hint.namespace.clone();
            let signature = (namespace.clone(), key.clone(), hint.description.clone());
            if !seen.insert(signature) {
                continue;
            }
            // Footer grouping is a footer concern. KeyPanel groups by namespace
            // in Python, which we model elsewhere.
            let mut binding = FooterBinding::new(key, hint.description.clone());
            binding.tooltip = hint.tooltip.clone();
            binding.group = namespace;
            mapped.push(binding);
        }
        self.set_bindings(mapped);
    }

    fn emit_scroll_changed_message(&self, ctx: &mut EventCtx) {
        ctx.post_message(KeyPanelScrolled {
            offset: self.offset_y,
            max_offset: self.max_offset(),
        });
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
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.scrollbar_extracted {
            return Vec::new();
        }
        self.scrollbar_extracted = true;
        let mut vbar = ScrollBar::new(true, 1);
        vbar.seed.css_id = Some(KEY_PANEL_VSCROLLBAR_ID.to_string());
        vec![Box::new(vbar)]
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);
        let body_viewport = height.max(1);
        let viewport_width = width;
        let table_lines = self.table.lines(viewport_width);
        let content_height = table_lines.len().max(1);
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
                ctx.post_message(KeyPanelBindingsUpdated {
                    count: self.table.bindings.len(),
                });
                ctx.request_repaint();
            }
            return;
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

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }

    fn layout_height(&self) -> Option<usize> {
        None
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn on_message(&mut self, event: &MessageEvent, ctx: &mut EventCtx) {
        let Some(payload) = event.downcast_ref::<ScrollbarScrollTo>() else {
            return;
        };
        if payload.axis != ScrollbarAxis::Vertical {
            return;
        }
        let body_viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        let content_height = self.content_height.load(Ordering::Relaxed).max(1);
        let next = ScrollView::line_clamp_offset(
            payload.offset.max(0.0).round() as usize,
            content_height,
            body_viewport,
        );
        if next != self.offset_y {
            self.offset_y = next;
            ctx.request_repaint();
            self.emit_scroll_changed_message(ctx);
        }
        ctx.set_handled();
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset_y)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        (0.0, self.offset_y as f32)
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        let width = self.widget_width.load(Ordering::Relaxed).max(1);
        let height = self.table.line_count().max(1);
        Some((width, height))
    }
}

impl Renderable for KeyPanel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::{KEY_PANEL_VSCROLLBAR_ID, KeyPanel};
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
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<KeyPanelBindingsUpdated>()
                .map_or(false, |k| k.count == 1)
        }));
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
        assert!(messages.iter().any(|m| m.is::<KeyPanelScrolled>()));
    }

    #[test]
    fn mouse_up_after_thumb_drag_requests_repaint() {
        let console = Console::new();
        let options = options_for(&console, 32, 6);
        let bindings = (1..=16)
            .map(|index| FooterBinding::new(format!("k{index:02}"), format!("item {index:02}")))
            .collect::<Vec<_>>();
        let mut panel = KeyPanel::new().with_bindings(bindings);
        let _ = panel.take_composed_children();
        let _ = panel.render(&console, &options);
        assert_eq!(panel.offset_y, 0);

        let mut ctx = EventCtx::default();
        panel.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 2.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert!(panel.offset_y > 0);
    }

    #[test]
    fn tree_mode_extracts_dedicated_scrollbar_child() {
        let mut panel = KeyPanel::new();
        let mut children = panel.take_composed_children();
        assert_eq!(children.len(), 1);
        let seed = children[0].take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some(KEY_PANEL_VSCROLLBAR_ID));
    }

    #[test]
    fn scrollbar_message_updates_offset_in_tree_mode() {
        let console = Console::new();
        let options = options_for(&console, 32, 4);
        let mut panel = KeyPanel::new().with_bindings(
            (1..=8)
                .map(|index| FooterBinding::new(format!("k{index}"), format!("item {index}")))
                .collect(),
        );
        let _ = panel.take_composed_children();
        let _ = panel.render(&console, &options);

        let mut ctx = EventCtx::default();
        panel.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 2.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert_eq!(panel.offset_y, 2);
    }
}
