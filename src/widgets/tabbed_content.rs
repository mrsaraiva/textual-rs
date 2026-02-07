use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::time::Duration;

use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, Event, EventCtx,
};
use crate::message::Message;
use crate::style::TransitionTiming;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

pub struct TabPane {
    title: String,
    pane_id: Option<String>,
    child: Box<dyn Widget>,
}

impl TabPane {
    pub fn new(title: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            pane_id: None,
            child: Box::new(child),
        }
    }

    pub fn id(mut self, pane_id: impl Into<String>) -> Self {
        self.pane_id = Some(pane_id.into());
        self
    }

    fn component_selector_id(&self) -> Option<String> {
        self.pane_id
            .as_ref()
            .map(|pane_id| format!("--content-tab-{pane_id}"))
    }
}

pub struct TabbedContent {
    id: WidgetId,
    panes: Vec<TabPane>,
    active: usize,
    initial: Option<String>,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    tab_row_height: usize,
    underline_start: f32,
    underline_end: f32,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl TabbedContent {
    const UNDERLINE_START_ATTR: &'static str = "tabbed_content.underline_start";
    const UNDERLINE_END_ATTR: &'static str = "tabbed_content.underline_end";
    const UNDERLINE_ANIMATION_DURATION: Duration = Duration::from_millis(300);
    const UNDERLINE_ANIMATION_DELAY: Duration = Duration::ZERO;

    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            panes: Vec::new(),
            active: 0,
            initial: None,
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            tab_row_height: 2,
            underline_start: 0.0,
            underline_end: 0.0,
            classes: vec!["tabbed-content".to_string()],
            focused_classes: vec!["tabbed-content".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn initial(mut self, pane_id: impl Into<String>) -> Self {
        self.initial = Some(pane_id.into());
        self
    }

    pub fn with_pane(mut self, pane: TabPane) -> Self {
        self.panes.push(pane);
        self
    }

    pub fn add_pane(&mut self, pane: TabPane) {
        self.panes.push(pane);
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn active_id(&self) -> Option<&str> {
        self.panes
            .get(self.active)
            .and_then(|pane| pane.pane_id.as_deref())
    }

    pub fn set_active(&mut self, index: usize) {
        self.activate(index, None);
    }

    pub fn set_active_id(&mut self, pane_id: &str) -> bool {
        let target = self
            .panes
            .iter()
            .position(|pane| pane.pane_id.as_deref() == Some(pane_id));
        if let Some(index) = target {
            self.activate(index, None);
            return true;
        }
        false
    }

    fn activate(&mut self, index: usize, mut ctx: Option<&mut EventCtx>) {
        if self.panes.is_empty() {
            self.active = 0;
            self.underline_start = 0.0;
            self.underline_end = 0.0;
            return;
        }
        let previous_active = self.active;
        let next = index.min(self.panes.len() - 1);
        if next != self.active {
            if let Some(pane) = self.panes.get_mut(self.active) {
                pane.child.set_focus(false);
            }
            self.active = next;
            if let Some(pane) = self.panes.get_mut(self.active) {
                pane.child.set_focus(true);
            }
            let target_span = self.span_for_index(self.active);
            if let Some(ctx) = ctx.as_mut() {
                if let Some((target_start, target_end)) = target_span {
                    let (duration, delay, ease) = self.underline_animation_params();
                    let fallback_source = self
                        .span_for_index(previous_active)
                        .unwrap_or((target_start, target_end));
                    let from_start = if self.underline_end > self.underline_start {
                        self.underline_start
                    } else {
                        fallback_source.0
                    };
                    let from_end = if self.underline_end > self.underline_start {
                        self.underline_end
                    } else {
                        fallback_source.1
                    };
                    ctx.request_animation(
                        AnimationRequest::new(
                            self.id,
                            Self::UNDERLINE_START_ATTR,
                            from_start,
                            target_start,
                            duration,
                        )
                        .with_delay(delay)
                        .with_ease(ease)
                        .with_level(AnimationLevel::Basic),
                    );
                    ctx.request_animation(
                        AnimationRequest::new(
                            self.id,
                            Self::UNDERLINE_END_ATTR,
                            from_end,
                            target_end,
                            duration,
                        )
                        .with_delay(delay)
                        .with_ease(ease)
                        .with_level(AnimationLevel::Basic),
                    );
                } else {
                    self.underline_start = 0.0;
                    self.underline_end = 0.0;
                }
                let title = self.panes[self.active].title.clone();
                ctx.post_message(
                    self.id,
                    Message::TabActivated {
                        index: self.active,
                        title,
                    },
                );
                ctx.request_repaint();
            } else if let Some((target_start, target_end)) = target_span {
                self.underline_start = target_start;
                self.underline_end = target_end;
            } else {
                self.underline_start = 0.0;
                self.underline_end = 0.0;
            }
        }
    }

    fn activate_prev_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        if self.panes.is_empty() {
            return;
        }
        let prev = if self.active == 0 {
            self.panes.len() - 1
        } else {
            self.active - 1
        };
        self.activate(prev, ctx);
    }

    fn activate_next_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        if self.panes.is_empty() {
            return;
        }
        let next = (self.active + 1) % self.panes.len();
        self.activate(next, ctx);
    }

    fn tab_spans(&self, width: usize) -> Vec<(usize, usize, usize)> {
        let mut spans = Vec::new();
        let mut cursor = 0usize;
        for (index, pane) in self.panes.iter().enumerate() {
            if cursor >= width {
                break;
            }
            let label = format!(" {} ", pane.title);
            let label_width = rich_rs::cell_len(&label);
            if label_width == 0 {
                continue;
            }
            let start = cursor;
            let end = start.saturating_add(label_width);
            spans.push((start, end, index));
            cursor = cursor.saturating_add(label_width);
        }
        spans
    }

    fn span_for_index(&self, index: usize) -> Option<(f32, f32)> {
        self.tab_spans(self.layout_width)
            .into_iter()
            .find(|(_, _, tab_index)| *tab_index == index)
            .map(|(start, end, _)| (start as f32, end as f32))
    }

    fn sync_underline_to_active(&mut self) {
        if let Some((start, end)) = self.span_for_index(self.active) {
            self.underline_start = start;
            self.underline_end = end;
        } else {
            self.underline_start = 0.0;
            self.underline_end = 0.0;
        }
    }

    fn underline_animation_params(&self) -> (Duration, Duration, AnimationEase) {
        let style =
            crate::css::resolve_component_style(self, &["tabbed-content--underline", "-active"]);
        let duration = style
            .transition_duration
            .unwrap_or(Self::UNDERLINE_ANIMATION_DURATION);
        let delay = style
            .transition_delay
            .unwrap_or(Self::UNDERLINE_ANIMATION_DELAY);
        let ease = style
            .transition_timing
            .map(Self::transition_timing_to_animation_ease)
            .unwrap_or(AnimationEase::InOutCubic);
        (duration, delay, ease)
    }

    fn transition_timing_to_animation_ease(timing: TransitionTiming) -> AnimationEase {
        match timing {
            TransitionTiming::Linear => AnimationEase::Linear,
            TransitionTiming::InOutCubic => AnimationEase::InOutCubic,
            TransitionTiming::OutCubic => AnimationEase::OutCubic,
            TransitionTiming::Round => AnimationEase::Round,
            TransitionTiming::None => AnimationEase::None,
        }
    }

    fn hit_tab(&self, x: usize, y: usize) -> Option<usize> {
        if y > 0 {
            return None;
        }
        self.tab_spans(self.layout_width)
            .into_iter()
            .find(|(start, end, _)| x >= *start && x < *end)
            .map(|(_, _, index)| index)
    }

    fn render_underline_line(
        width: usize,
        start: f32,
        end: f32,
        base_style: rich_rs::Style,
        active_style: rich_rs::Style,
    ) -> Vec<Segment> {
        if width == 0 {
            return Vec::new();
        }

        let mut cells = vec![('─', false); width];
        let start = start.clamp(0.0, width as f32);
        let end = end.clamp(0.0, width as f32);
        if end > start {
            let start = (start * 2.0).round() / 2.0;
            let end = (end * 2.0).round() / 2.0;

            let full_start = start.ceil() as usize;
            let full_end = end.floor() as usize;
            for idx in full_start..full_end.min(width) {
                cells[idx] = ('━', true);
            }

            if start.fract().abs() > f32::EPSILON {
                let idx = start.floor() as usize;
                if idx < width {
                    cells[idx] = ('╺', true);
                }
            }

            if end.fract().abs() > f32::EPSILON {
                let idx = end.floor() as usize;
                if idx < width {
                    cells[idx] = if cells[idx].1 {
                        ('━', true)
                    } else {
                        ('╸', true)
                    };
                }
            }
        }

        let mut out = Vec::new();
        let mut current_active = cells[0].1;
        let mut buffer = String::new();
        for (ch, active) in cells {
            if active == current_active {
                buffer.push(ch);
            } else {
                let style = if current_active {
                    active_style.clone()
                } else {
                    base_style.clone()
                };
                out.push(Segment::styled(std::mem::take(&mut buffer), style));
                buffer.push(ch);
                current_active = active;
            }
        }
        if !buffer.is_empty() {
            let style = if current_active {
                active_style
            } else {
                base_style
            };
            out.push(Segment::styled(buffer, style));
        }
        out
    }
}

impl Widget for TabbedContent {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.set_focus(focused);
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
        if let Some(initial) = self.initial.clone() {
            let _ = self.set_active_id(&initial);
        }
        for pane in &mut self.panes {
            pane.child.on_mount();
        }
        self.sync_underline_to_active();
    }

    fn on_unmount(&mut self) {
        for pane in &mut self.panes {
            pane.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let next_layout_width = usize::from(width).max(1);
        if next_layout_width != self.layout_width {
            self.layout_width = next_layout_width;
            self.sync_underline_to_active();
        } else {
            self.layout_width = next_layout_width;
        }
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child
                .on_layout(width, height.saturating_sub(self.tab_row_height as u16));
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            ..
        }) = event
        {
            if *target == self.id {
                if attribute == Self::UNDERLINE_START_ATTR {
                    self.underline_start = *value;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::UNDERLINE_END_ATTR {
                    self.underline_end = *value;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
            }
        }
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
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
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(pane) = self.panes.get_mut(self.active) {
            pane.child.on_message(message, ctx);
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
        for pane in &mut self.panes {
            f(pane.child.as_mut());
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let bar_style = crate::css::resolve_component_style(self, &["tabbed-content--bar"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let base_underline_style =
            crate::css::resolve_component_style(self, &["tabbed-content--underline"])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
        let active_underline_style =
            crate::css::resolve_component_style(self, &["tabbed-content--underline", "-active"])
                .to_rich()
                .unwrap_or(base_underline_style);

        let mut header_line = Vec::new();
        if self.panes.is_empty() {
            header_line.push(Segment::styled(" no panes ".to_string(), bar_style));
        } else {
            for (idx, pane) in self.panes.iter().enumerate() {
                let mut classes = vec!["tabbed-content--tab"];
                if idx == self.active {
                    classes.push("-active");
                    if self.focused {
                        classes.push("-focus");
                    }
                }
                if self.hovered_tab == Some(idx) {
                    classes.push("-hover");
                }
                let selector_id = pane.component_selector_id();
                let style = crate::css::resolve_component_style_with_id(
                    self,
                    selector_id.as_deref(),
                    &classes,
                )
                .to_rich()
                .unwrap_or(bar_style);
                let tab_text = format!(" {} ", pane.title);
                header_line.push(Segment::styled(tab_text, style));
            }
        }
        let mut header_line = Segment::adjust_line_length(&header_line, width, None, false);
        let mut underline_line = Segment::adjust_line_length(
            &Self::render_underline_line(
                width,
                self.underline_start,
                self.underline_end,
                base_underline_style,
                active_underline_style,
            ),
            width,
            None,
            false,
        );
        let header_len = Segment::get_line_length(&header_line);
        if header_len < width {
            header_line.push(Segment::styled(" ".repeat(width - header_len), bar_style));
        }
        let underline_len = Segment::get_line_length(&underline_line);
        if underline_len < width {
            underline_line.push(Segment::styled(
                "─".repeat(width - underline_len),
                base_underline_style,
            ));
        }
        let mut lines = vec![header_line, underline_line];

        if height > self.tab_row_height {
            if let Some(pane) = self.panes.get(self.active) {
                let mut child_options = options.clone();
                child_options.size = (width, height - self.tab_row_height);
                child_options.max_width = width;
                child_options.max_height = height - self.tab_row_height;
                let child_segments = pane.child.render_styled(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines = Segment::set_shape(
                    &child_lines,
                    width,
                    Some(height - self.tab_row_height),
                    None,
                    false,
                );
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
            .panes
            .get(self.active)
            .and_then(|pane| pane.child.layout_height());
        child_height.map(|height| height + self.tab_row_height)
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

impl Renderable for TabbedContent {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
